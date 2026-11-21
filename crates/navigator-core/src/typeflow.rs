//! Demand-driven nominal receiver proofs. These facts describe syntax edges,
//! not a replacement global type inference engine. Every successful edge ends
//! at a concrete accessible source declaration in the current snapshot.
use crate::library_types::{self, Identity as LibraryIdentity, LibraryMember, TypeExpr};
use crate::syntax::{ParsedFile, RecordType, TypeAlias, TypeSource};
use crate::{Symbol, Workspace};
use anyhow::Result;

const MAX_PROOF_DEPTH: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TypeIdentity {
    path: String,
    token: usize,
    arguments: Vec<TypeArgument>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct TypeArgument {
    path: String,
    token: usize,
    name: String,
}
// Absence is a successful negative lookup in this lookup's certified domain;
// ambiguity, unsupported syntax and missing certificates are always Unknown.
// External alternative lookup may proceed only after Absent, never Unknown.
pub(crate) enum Lookup<T> {
    Absent,
    Resolved(T),
    Unknown,
}
impl<T> Lookup<T> {
    fn from_proof(value: Option<T>) -> Self {
        value.map(Self::Resolved).unwrap_or(Self::Unknown)
    }
    fn resolved(self) -> Option<T> {
        if let Self::Resolved(value) = self {
            Some(value)
        } else {
            None
        }
    }
}
type TypeLookup = Lookup<TypeIdentity>;
type DeclarationLookup = Lookup<Symbol>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttributeEffect {
    AutoOpen,
    RequireQualifiedAccess,
    Other,
    Unknown,
}

pub(crate) struct ProofState {
    active: std::collections::HashSet<(String, usize, bool)>,
    memo: std::collections::HashMap<(String, usize, bool), Option<TypeIdentity>>,
    candidates: std::collections::HashMap<String, Vec<String>>,
    modules: std::collections::HashMap<String, Vec<(String, crate::syntax::ModuleScope, bool)>>,
    nominal: std::collections::HashMap<String, Vec<String>>,
    attributes: std::collections::HashMap<(String, usize), AttributeEffect>,
    pairs: std::collections::HashMap<(String, usize, String), Option<Symbol>>,
    retained_bytes: usize,
    remaining: usize,
    pub exhausted: bool,
    pub metadata_used: bool,
    pub builtins_used: bool,
    pub trace_references: bool,
    pub reference_notes: std::collections::HashMap<(String, usize), String>,
}
impl Default for ProofState {
    fn default() -> Self {
        Self {
            active: Default::default(),
            memo: Default::default(),
            candidates: Default::default(),
            modules: Default::default(),
            nominal: Default::default(),
            attributes: Default::default(),
            pairs: Default::default(),
            retained_bytes: 0,
            remaining: 16_384,
            exhausted: false,
            metadata_used: false,
            builtins_used: false,
            trace_references: false,
            reference_notes: Default::default(),
        }
    }
}

impl ProofState {
    pub(crate) fn clear_context(&mut self) {
        self.active.clear();
        self.memo.clear();
        self.candidates.clear();
        self.modules.clear();
        self.nominal.clear();
        self.attributes.clear();
        self.pairs.clear();
        self.reference_notes.clear();
        self.retained_bytes = 0;
    }
}

fn nominal_name(text: &str) -> Option<String> {
    let mut parts = Vec::new();
    for raw in text.trim().split('.') {
        let part = raw.trim();
        if let Some(escaped) = part.strip_prefix("``").and_then(|p| p.strip_suffix("``")) {
            if escaped.is_empty() {
                return None;
            }
            parts.push(escaped.to_string());
        } else if !part.is_empty()
            && part.chars().enumerate().all(|(i, c)| {
                c == '_'
                    || (i == 0 && unicode_ident::is_xid_start(c))
                    || (i > 0 && (unicode_ident::is_xid_continue(c) || c == '\''))
            })
        {
            parts.push(part.to_string());
        } else {
            return None;
        }
    }
    Some(parts.join("."))
}
fn nominal_application(text: &str) -> Option<(String, Vec<String>)> {
    let text = text.trim();
    let Some(open) = text.find('<') else {
        return nominal_name(text).map(|n| (n, Vec::new()));
    };
    let name = nominal_name(&text[..open])?;
    let inner = text[open + 1..].strip_suffix('>')?;
    let mut arguments = Vec::new();
    let mut start = 0;
    let mut nesting = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '<' => nesting += 1,
            '>' => nesting = nesting.checked_sub(1)?,
            ',' if nesting == 0 => {
                arguments.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    if nesting != 0 {
        return None;
    }
    arguments.push(inner[start..].trim().to_string());
    if arguments.iter().any(|s| s.is_empty()) {
        return None;
    }
    Some((name, arguments))
}

fn supported(parsed: &ParsedFile, token: usize) -> bool {
    parsed.tokens.get(token).is_some_and(|t| {
        !parsed.uncertain.iter().any(|u| {
            u.start <= t.start && t.start < u.end && u.name.as_ref().is_none_or(|n| n == &t.text)
        })
    })
}

// This certificate is only about names escaping a complete ordinary RHS.
// It never makes an uncertain expression supported for type/receiver inference.
fn uncertainty_can_bind_name(
    span: &crate::syntax::UncertainSpan,
    owner_path: &str,
    path: &str,
    offset: usize,
    name: &str,
) -> bool {
    if span
        .binding_names
        .as_ref()
        .is_some_and(|names| !names.iter().any(|n| n == name))
    {
        return false;
    }
    if let Some([start, end]) = span.local_binding_scope {
        if start <= span.start
            && span.end <= end
            && start < end
            && (owner_path != path || offset < start || offset >= end)
        {
            return false;
        }
    }
    true
}

// A completed name-specific pattern ambiguity cannot change the record-label
// environment outside its lexical scope. Structural uncertainty remains a
// barrier even when recovery reports an earlier end byte.
fn label_environment_supported(parsed: &ParsedFile, offset: usize) -> bool {
    !parsed
        .uncertain
        .iter()
        .any(|span| span.start < offset && (span.name.is_none() || offset < span.end))
}

impl Workspace {
    pub(crate) fn attribute_effect(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        attribute: &crate::syntax::AppliedAttribute,
    ) -> Result<AttributeEffect> {
        let key = (path.to_string(), attribute.token);
        if let Some(effect) = self.type_proofs.attributes.get(&key) {
            return Ok(*effect);
        }
        // Insert an in-progress blocker. Recursive abbreviation/import graphs
        // cannot certify their own attribute identity.
        self.type_proofs
            .attributes
            .insert(key.clone(), AttributeEffect::Unknown);
        let suffixed = format!("{}Attribute", attribute.type_name);
        let mut identity = self.library_named_type(path, parsed, attribute.token, &suffixed, 1)?;
        if identity.is_none() {
            let simple = suffixed.rsplit('.').next().unwrap_or(&suffixed);
            let absent = !self.source_name_shadow(path, parsed, attribute.token, simple, false)?
                && self.ensure_builtins()?
                && self
                    .builtin_environment
                    .as_ref()
                    .unwrap()
                    .type_names(&self.index.db, simple)?
                    .is_some_and(|entries| entries.is_empty());
            if absent {
                identity = self.library_named_type(
                    path,
                    parsed,
                    attribute.token,
                    &attribute.type_name,
                    1,
                )?;
            }
        }
        let effect = match identity {
            Some(TypeExpr::Source { .. }) => AttributeEffect::Other,
            Some(TypeExpr::Named { identity, .. }) => match identity.full_name.as_str() {
                "Microsoft.FSharp.Core.AutoOpenAttribute"
                    if identity.assembly_name.split(',').next() == Some("FSharp.Core") =>
                {
                    AttributeEffect::AutoOpen
                }
                "Microsoft.FSharp.Core.RequireQualifiedAccessAttribute"
                    if identity.assembly_name.split(',').next() == Some("FSharp.Core") =>
                {
                    AttributeEffect::RequireQualifiedAccess
                }
                _ => AttributeEffect::Other,
            },
            _ => AttributeEffect::Unknown,
        };
        self.type_proofs.attributes.insert(key, effect);
        Ok(effect)
    }
    pub(crate) fn effective_imports(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Result<Vec<crate::syntax::Import>> {
        self.effective_imports_since(path, parsed, token, 0)
    }
    pub(crate) fn effective_imports_since(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        after: usize,
    ) -> Result<Vec<crate::syntax::Import>> {
        let offset = parsed.tokens[token].start;
        let current = parsed.module_at(token).unwrap_or("");
        let mut imports = parsed.opens.clone();
        for attribute in parsed.attributes.iter().filter(|a| {
            a.owner_kind == "module"
                && a.owner_end > after
                && a.owner_end <= offset
                && offset < a.scope_end
        }) {
            let parent = attribute.owner.rsplit_once('.').map(|p| p.0).unwrap_or("");
            if !parent.is_empty()
                && current != parent
                && !current.starts_with(&format!("{parent}."))
            {
                continue;
            }
            let name = match self.attribute_effect(path, parsed, attribute)? {
                AttributeEffect::AutoOpen => attribute.owner.clone(),
                AttributeEffect::Unknown => String::new(),
                _ => continue,
            };
            imports.push(crate::syntax::Import {
                name,
                offset: attribute.owner_end,
                scope_end: attribute.scope_end,
            });
        }
        imports.sort_by_key(|o| o.offset);
        Ok(imports)
    }
    fn record_qualification(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        record: &RecordType,
    ) -> Result<Option<bool>> {
        let mut required = record.require_qualified_access;
        for attribute in parsed
            .attributes
            .iter()
            .filter(|a| a.owner_kind == "type" && a.owner == record.name)
        {
            match self.attribute_effect(path, parsed, attribute)? {
                AttributeEffect::RequireQualifiedAccess => required = true,
                AttributeEffect::Unknown => return Ok(None),
                _ => {}
            }
        }
        Ok(Some(required))
    }
    fn proof_step(&mut self, depth: usize) -> bool {
        if self.cancellation.is_cancelled() {
            return false;
        }
        if depth >= MAX_PROOF_DEPTH || self.type_proofs.remaining == 0 {
            self.type_proofs.exhausted = true;
            return false;
        }
        self.type_proofs.remaining -= 1;
        self.index.counters.type_proof_steps += 1;
        true
    }
    fn nominal_candidate_paths(&mut self, name: &str) -> Result<Vec<String>> {
        if let Some(paths) = self.type_proofs.candidates.get(name) {
            return Ok(paths.clone());
        }
        self.ensure_name_coverage(name)?;
        let paths = self.index.candidate_paths(name)?;
        // Store file identities only. Transient syntax summaries are never
        // accumulated to accelerate a repository-wide lookup.
        let bytes = paths.iter().map(|p| p.len() + 32).sum::<usize>() + name.len();
        if self.type_proofs.retained_bytes + bytes <= 4 * 1024 * 1024 {
            self.type_proofs.retained_bytes += bytes;
            self.type_proofs
                .candidates
                .insert(name.into(), paths.clone());
        }
        Ok(paths)
    }
    fn type_candidate_paths(&mut self, name: &str) -> Result<Vec<String>> {
        if let Some(paths) = self.type_proofs.nominal.get(name) {
            return Ok(paths.clone());
        }
        let mut found = Vec::new();
        for path in self.nominal_candidate_paths(name)? {
            if !self.query_domain.contains(&path) {
                continue;
            }
            self.cancellation.check()?;
            if self.ambiguous_instance(&path) {
                found.push(path);
                continue;
            }
            let parsed = self.parsed(&path)?;
            if (parsed.tokens.is_empty() && !parsed.supported)
                || parsed
                    .owned_types
                    .iter()
                    .any(|r| r.name.rsplit('.').next() == Some(name))
            {
                found.push(path);
            }
        }
        let bytes = found.iter().map(|p| p.len() + 32).sum::<usize>() + name.len();
        if self.type_proofs.retained_bytes + bytes <= 4 * 1024 * 1024 {
            self.type_proofs.retained_bytes += bytes;
            self.type_proofs.nominal.insert(name.into(), found.clone());
        }
        Ok(found)
    }
    fn module_facts(
        &mut self,
        name: &str,
    ) -> Result<Vec<(String, crate::syntax::ModuleScope, bool)>> {
        if let Some(facts) = self.type_proofs.modules.get(name) {
            return Ok(facts.clone());
        }
        let mut facts = Vec::new();
        for path in self.nominal_candidate_paths(name)? {
            if !self.query_domain.contains(&path) {
                continue;
            }
            if self.ambiguous_instance(&path) {
                facts.push((
                    path,
                    crate::syntax::ModuleScope {
                        name: String::new(),
                        start: 0,
                        end: usize::MAX,
                    },
                    false,
                ));
                continue;
            }
            let owner = self.parsed(&path)?;
            if owner.tokens.is_empty() && !owner.supported {
                facts.push((
                    path,
                    crate::syntax::ModuleScope {
                        name: String::new(),
                        start: 0,
                        end: usize::MAX,
                    },
                    false,
                ));
                continue;
            }
            for module in owner
                .modules
                .iter()
                .filter(|m| m.name.rsplit('.').next() == Some(name))
            {
                let valid = !owner
                    .uncertain
                    .iter()
                    .any(|u| u.start <= module.start && module.start < u.end);
                facts.push((path.clone(), module.clone(), valid));
            }
        }
        let bytes = facts
            .iter()
            .map(|(p, m, _)| p.len() + m.name.len() + 64)
            .sum::<usize>()
            + name.len();
        if self.type_proofs.retained_bytes + bytes <= 4 * 1024 * 1024 {
            self.type_proofs.retained_bytes += bytes;
            self.type_proofs.modules.insert(name.into(), facts.clone());
        }
        Ok(facts)
    }
    pub(crate) fn has_member_syntax(parsed: &ParsedFile, token: usize) -> bool {
        parsed.method_calls.iter().any(|m| m.member == token)
            || parsed.method_declarations.iter().any(|m| m.token == token)
            || parsed.member_accesses.iter().any(|m| m.token == token)
            || parsed
                .record_types
                .iter()
                .any(|r| r.fields.iter().any(|f| f.token == token))
            || parsed.binding_types.iter().any(
                |f| matches!(&f.source, TypeSource::Record {fields,..} if fields.contains(&token)),
            )
            || parsed
                .record_expressions
                .iter()
                .any(|r| r.fields.contains(&token))
    }

    pub(crate) fn member_binding(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Result<Option<Symbol>> {
        self.member_binding_depth(path, parsed, token, 0)
    }

    fn member_binding_depth(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        depth: usize,
    ) -> Result<Option<Symbol>> {
        if !self.proof_step(depth) || !supported(parsed, token) {
            return Ok(None);
        }
        let name = &parsed.tokens[token].text;
        if parsed.method_calls.iter().any(|m| m.member == token)
            || (parsed.method_declarations.iter().any(|m| m.token == token)
                && !parsed
                    .record_types
                    .iter()
                    .flat_map(|r| &r.fields)
                    .any(|f| f.token == token))
        {
            return self.method_binding(path, parsed, token, depth + 1);
        }
        let mut static_access = false;
        for record in &parsed.record_types {
            if record.fields.iter().any(|f| f.token == token) {
                return Ok(Some(
                    self.source_symbol(path, parsed, token, name, false, true),
                ));
            }
        }
        let receiver =
            if let Some(record) = parsed
                .record_expressions
                .iter()
                .find(|r| r.fields.contains(&token))
            {
                self.record_receiver_type(path, parsed, record, depth + 1)?
            } else if let Some(access) = parsed.member_accesses.iter().find(|m| m.token == token) {
                let known = if let Some(source) = &access.source {
                    self.source_type(path, parsed, token, source, depth + 1)?
                } else {
                    self.value_type(path, parsed, access.receiver, depth + 1)?
                };
                if known.is_some() {
                    known
                } else if let Some(fresh) =
                    self.fresh_parameter_type(path, parsed, token, access.receiver)
                {
                    Some(fresh)
                } else if access.source.is_none()
                    && self.resolve(path, parsed, access.receiver)?.is_none()
                {
                    if let Some(qualifier) = parsed.qualifier(token) {
                        let identity =
                            self.named_type(path, parsed, access.receiver, &qualifier, depth + 1)?;
                        static_access = identity.is_some();
                        identity
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else if let Some(fact) = parsed.binding_types.iter().find(
                |f| matches!(&f.source,TypeSource::Record {fields,..} if fields.contains(&token)),
            ) {
                self.binding_type(path, parsed, fact.binding, depth + 1)?
            } else {
                None
            };
        let Some(identity) = receiver else {
            return Ok(None);
        };
        self.member_of_type(
            identity,
            path,
            parsed,
            token,
            name,
            static_access,
            depth + 1,
        )
    }

    fn primitive_annotation(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
    ) -> Result<Option<&'static str>> {
        let name = name.trim();
        if !matches!(name, "int" | "string") {
            return Ok(None);
        }
        if self.source_name_shadow(path, parsed, token, name, false)? || !self.ensure_builtins()? {
            return Ok(None);
        }
        let Some(ty) = self
            .builtin_environment
            .as_ref()
            .unwrap()
            .unique_type(name, 0)
        else {
            return Ok(None);
        };
        let identity = ty["canonicalType"]["identity"]["fullName"]
            .as_str()
            .unwrap_or("");
        Ok(match identity {
            "System.Int32" => Some("int"),
            "System.String" => Some("string"),
            _ => None,
        })
    }
    fn method_binding(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        depth: usize,
    ) -> Result<Option<Symbol>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        if let Some(method) = parsed.method_declarations.iter().find(|m| m.token == token) {
            if method.supported {
                return Ok(Some(self.source_symbol(
                    path,
                    parsed,
                    token,
                    &method.name,
                    false,
                    true,
                )));
            }
            return Ok(None);
        }
        let Some(call) = parsed.method_calls.iter().find(|c| c.member == token) else {
            return Ok(None);
        };
        let Some(qualifier) = parsed.qualifier(token) else {
            return Ok(None);
        };
        if self.resolve(path, parsed, call.receiver)?.is_some() {
            return Ok(None);
        }
        let Some(identity) = self.named_type(path, parsed, call.receiver, &qualifier, depth + 1)?
        else {
            return Ok(None);
        };
        let loaded = if identity.path == path {
            None
        } else {
            Some(self.parsed(&identity.path)?)
        };
        let owner = loaded.as_ref().unwrap_or(parsed);
        let Some(record) = owner
            .record_types
            .iter()
            .find(|r| r.token == identity.token)
        else {
            return Ok(None);
        };
        if record.base_type.is_some() || !record.type_parameters.is_empty() {
            return Ok(None);
        }
        let methods = owner
            .method_declarations
            .iter()
            .filter(|m| m.owner == record.name && m.name == parsed.tokens[token].text)
            .collect::<Vec<_>>();
        if methods.is_empty() {
            return Ok(None);
        }
        // With one ordinary static declaration there is no overload choice to
        // infer. Literal classification is needed only for a competing family.
        if methods.len() == 1 {
            let method = methods[0];
            if method.supported
                && method.is_static
                && method.parameters.len() == call.args.len()
                && self.nominal_accessible(
                    &identity.path,
                    owner,
                    method.token,
                    &method.accessibility,
                    path,
                    parsed,
                    token,
                )
            {
                return Ok(Some(self.source_symbol(
                    &identity.path,
                    owner,
                    method.token,
                    &method.name,
                    false,
                    true,
                )));
            }
            return Ok(None);
        }
        let mut selected = Vec::new();
        for method in methods {
            if !method.supported
                || !method.is_static
                || method.parameters.len() != 1
                || !self.nominal_accessible(
                    &identity.path,
                    owner,
                    method.token,
                    &method.accessibility,
                    path,
                    parsed,
                    token,
                )
            {
                return Ok(None);
            }
            let parameter = &method.parameters[0];
            let Some(kind) = self.primitive_annotation(
                &identity.path,
                owner,
                parameter.token,
                &parameter.type_name,
            )?
            else {
                return Ok(None);
            };
            let matches = matches!(call.args.as_slice(),[crate::syntax::ArgumentSource::IntrinsicLiteral(crate::syntax::IntrinsicLiteral::String)] if kind=="string")
                || matches!(call.args.as_slice(),[crate::syntax::ArgumentSource::IntrinsicLiteral(crate::syntax::IntrinsicLiteral::UnsuffixedDecimalInt32)] if kind=="int");
            if matches {
                selected.push(method);
            }
        }
        if selected.len() != 1 {
            return Ok(None);
        }
        Ok(Some(self.source_symbol(
            &identity.path,
            owner,
            selected[0].token,
            &selected[0].name,
            false,
            true,
        )))
    }

    // Resolve a declared source case without treating a qualified spelling as
    // an identity. Values, companion modules and unresolved earlier nominal
    // candidates still block this deliberately local envelope.
    fn qualified_source_case(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        callee: usize,
        depth: usize,
    ) -> Result<Option<crate::syntax::PatternDeclaration>> {
        if !self.proof_step(depth) || !supported(parsed, callee) {
            return Ok(None);
        }
        let Some(qualifier) = parsed.qualifier(callee) else {
            return Ok(None);
        };
        let Some(qualifier) = nominal_name(&qualifier) else {
            return Ok(None);
        };
        let offset = parsed.tokens[callee].start;
        let first = qualifier.split('.').next().unwrap_or("");
        if parsed
            .aliases
            .iter()
            .any(|a| a.name == first && a.offset < offset && offset < a.scope_end)
        {
            return Ok(None);
        }
        if !self.term_qualification_clear(path, parsed, callee, &qualifier, depth + 1)? {
            return Ok(None);
        }
        let choices = if let Some(global) = qualifier.strip_prefix("global.") {
            vec![global.to_string()]
        } else {
            Self::module_choices(parsed, callee, &qualifier, false)
        };
        let mut selected = None;
        for choice in choices {
            if let Some(owner) = parsed
                .owned_types
                .iter()
                .filter(|o| o.name == choice && o.start < offset && offset < o.end)
                .max_by_key(|o| o.start)
            {
                selected = Some(owner);
                break;
            }
            if !matches!(self.external_type_candidates(&choice, 0)?, Lookup::Absent) {
                return Ok(None);
            }
        }
        let Some(owner) = selected else {
            return Ok(None);
        };
        if parsed
            .type_aliases
            .iter()
            .any(|a| a.name == owner.name && a.start <= offset && offset < a.scope_end)
        {
            return Ok(None);
        }
        if self
            .effective_imports_since(path, parsed, callee, owner.start)?
            .iter()
            .any(|o| o.offset > owner.start && o.offset < offset && offset < o.scope_end)
        {
            return Ok(None);
        }
        // .fsi controls externally visible case/payload identity. Until this
        // branch is paired explicitly, it cannot use implementation-only facts.
        if path.ends_with(".fs") && self.query_domain.contains(&format!("{path}i")) {
            return Ok(None);
        }
        let cases = parsed
            .pattern_declarations
            .iter()
            .filter(|d| {
                matches!(d.kind.as_str(), "union-case" | "enum-case")
                    && d.name == parsed.tokens[callee].text
                    && d.type_owner.as_deref() == Some(owner.name.as_str())
                    && d.scope_start >= owner.start
                    && d.scope_start <= offset
                    && offset < d.scope_end
            })
            .collect::<Vec<_>>();
        if cases.len() != 1 {
            return Ok(None);
        }
        let case = cases[0];
        if !self.nominal_accessible(
            path,
            parsed,
            case.token,
            &case.accessibility,
            path,
            parsed,
            callee,
        ) {
            return Ok(None);
        }
        Ok(Some(case.clone()))
    }

    // Initial source-DU payload envelope: a nongeneric declaration whose
    // parameter annotation is resolved in the declaration's own environment.
    fn qualified_union_payload(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        callee: usize,
        position: usize,
        arity: Option<usize>,
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        let Some(case) = self.qualified_source_case(path, parsed, callee, depth + 1)? else {
            return Ok(None);
        };
        if case.kind != "union-case" {
            return Ok(None);
        }
        let Some(payload) = parsed
            .union_case_payloads
            .iter()
            .find(|p| p.case_token == case.token)
        else {
            return Ok(None);
        };
        if !payload.supported
            || !payload.type_parameters.is_empty()
            || arity.is_some_and(|n| n != payload.fields.len())
        {
            return Ok(None);
        }
        let Some(field) = payload.fields.get(position) else {
            return Ok(None);
        };
        self.named_type(path, parsed, field.token, &field.type_name, depth + 1)
    }
    fn fresh_method_result_proven(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        record: &crate::syntax::RecordExpression,
        depth: usize,
    ) -> Result<bool> {
        if !record.method_result_literal_group
            || !record.method_result_eligible
            || !self.proof_step(depth)
            || self.possible_signature_context(path)
        {
            return Ok(false);
        }
        if record.fresh_method_result {
            return Ok(true);
        }
        let Some(method) = parsed
            .method_declarations
            .iter()
            .find(|m| Some(m.token) == record.method_result)
        else {
            return Ok(false);
        };
        // Every syntactically suspicious occurrence must be a different case
        // identity. In particular, source order alone does not exempt a later
        // method call or erase an earlier result constraint.
        for token in &record.method_result_uses {
            let Some(case) = self.qualified_source_case(path, parsed, *token, depth + 1)? else {
                return Ok(false);
            };
            if case
                .type_owner
                .as_deref()
                .is_none_or(|owner| owner == method.owner)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn record_receiver_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        record: &crate::syntax::RecordExpression,
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        if let (Some(name), Some(at)) = (&record.type_name, record.type_token) {
            return self.named_type(path, parsed, at, name, depth + 1);
        }
        if let Some(copy) = record.copy {
            return self.value_type(path, parsed, copy, depth + 1);
        }
        if let Some(parent) = record.parent_field {
            if let Some(symbol) = self.member_binding_depth(path, parsed, parent, depth + 1)? {
                let loaded = if symbol.path == path {
                    None
                } else {
                    Some(self.parsed(&symbol.path)?)
                };
                let owner = loaded.as_ref().unwrap_or(parsed);
                if let Some(field) = owner
                    .record_types
                    .iter()
                    .flat_map(|r| &r.fields)
                    .find(|f| f.token == symbol.token)
                {
                    if let (Some(name), Some(at)) = (&field.type_name, field.type_token) {
                        return self.named_type(&symbol.path, owner, at, name, depth + 1);
                    }
                }
            }
            return Ok(None);
        }
        if let Some(argument) = &record.argument {
            return self.qualified_union_payload(
                path,
                parsed,
                argument.callee,
                argument.position as usize,
                Some(argument.arity as usize),
                depth + 1,
            );
        }
        if let Some(binding) = record.binding {
            if parsed.function_returns.iter().any(|f| f.binding == binding)
                || parsed.function_results.iter().any(|f| f.binding == binding)
            {
                return self.return_type(path, parsed, binding, depth + 1);
            }
            return self.binding_type(path, parsed, binding, depth + 1);
        }
        for field in &record.fields {
            if let Some(qualifier) = parsed.qualifier(*field) {
                return self.named_type(path, parsed, *field, &qualifier, depth + 1);
            }
        }
        if record.method_result_eligible {
            if let Some(binding) = record.method_result {
                return Ok(self.local_record_construction(path, parsed, binding, &record.fields));
            }
        }
        Ok(None)
    }

    fn external_type_candidates(
        &mut self,
        name: &str,
        arity: usize,
    ) -> Result<Lookup<Vec<serde_json::Value>>> {
        if !self.ensure_builtins()? {
            return Ok(Lookup::Unknown);
        }
        let simple = name.rsplit('.').next().unwrap_or(name);
        let Some(entries) = self
            .builtin_environment
            .as_ref()
            .unwrap()
            .type_names(&self.index.db, simple)?
        else {
            return Ok(Lookup::Unknown);
        };
        let matching = entries
            .into_iter()
            .filter(|entry| {
                let full = entry["sourceFullName"]
                    .as_str()
                    .or_else(|| entry["identity"]["fullName"].as_str())
                    .unwrap_or("");
                let full = full
                    .rsplit_once('`')
                    .filter(|(_, a)| a.chars().all(|c| c.is_ascii_digit()))
                    .map(|(n, _)| n)
                    .unwrap_or(full);
                full == name && entry["genericArity"].as_u64() == Some(arity as u64)
            })
            .collect::<Vec<_>>();
        Ok(if matching.is_empty() {
            Lookup::Absent
        } else {
            Lookup::Resolved(matching)
        })
    }
    fn logical_type_choice(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        arity: usize,
    ) -> Result<DeclarationLookup> {
        let simple = name.rsplit('.').next().unwrap_or(name);
        let visible = self.visible_files(path);
        let offset = parsed.tokens[token].start;
        let mut found = Vec::new();
        for candidate in self.type_candidate_paths(simple)? {
            if !visible.contains(&candidate) {
                continue;
            }
            if candidate != path
                && candidate.ends_with(".fs")
                && visible.contains(&format!("{candidate}i"))
            {
                continue;
            }
            let loaded = if candidate == path {
                None
            } else {
                Some(self.parsed(&candidate)?)
            };
            let owner = loaded.as_ref().unwrap_or(parsed);
            if owner.tokens.is_empty() && !owner.supported {
                return Ok(DeclarationLookup::Unknown);
            }
            let declarations = owner
                .type_aliases
                .iter()
                .filter(|a| {
                    a.name == name
                        && a.type_parameters.len() == arity
                        && (candidate != path || a.start < offset)
                })
                .map(|a| (a.token, a.accessibility.as_str()))
                .chain(
                    owner
                        .record_types
                        .iter()
                        .filter(|r| {
                            r.name == name
                                && r.type_parameters.len() == arity
                                && (candidate != path || r.start < offset)
                        })
                        .map(|r| (r.token, r.accessibility.as_str())),
                )
                .collect::<Vec<_>>();
            for (at, access) in declarations {
                if !supported(owner, at) {
                    return Ok(DeclarationLookup::Unknown);
                }
                if self.nominal_accessible(&candidate, owner, at, access, path, parsed, token) {
                    found.push(self.source_symbol(
                        &candidate,
                        owner,
                        at,
                        &owner.tokens[at].text,
                        false,
                        false,
                    ));
                }
            }
            if found.is_empty()
                && owner
                    .owned_types
                    .iter()
                    .any(|t| t.name == name && (candidate != path || t.start < offset))
            {
                return Ok(DeclarationLookup::Unknown);
            }
        }
        if found.len() == 1 {
            return Ok(DeclarationLookup::Resolved(found.remove(0)));
        }
        if found.len() > 1 {
            return Ok(DeclarationLookup::Unknown);
        }
        // A source namespace/module contribution does not close the same
        // namespace's external members. Only the current complete inventory
        // can prove that an earlier declaration is not shadowed here.
        Ok(match self.external_type_candidates(name, arity)? {
            Lookup::Absent => Lookup::Absent,
            _ => Lookup::Unknown,
        })
    }
    pub(crate) fn type_definition(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Result<Option<Symbol>> {
        if !supported(parsed, token) {
            return Ok(None);
        }
        // This logical declaration namespace is distinct from canonical runtime
        // types used for receiver proofs. In particular, alias constructor
        // expressions never enter this route merely because names coincide.
        if parsed.record_types.iter().any(|r| r.token == token)
            || parsed.type_aliases.iter().any(|a| a.token == token)
        {
            return Ok(Some(self.source_symbol(
                path,
                parsed,
                token,
                &parsed.tokens[token].text,
                false,
                false,
            )));
        }
        let text = parsed
            .type_aliases
            .iter()
            .find(|a| a.target_token == token)
            .map(|a| a.target.as_str())
            .or_else(|| {
                parsed.binding_types.iter().find_map(|f| match &f.source {
                    TypeSource::Named { name, token: t } if *t == token => Some(name.as_str()),
                    _ => None,
                })
            })
            .or_else(|| {
                parsed
                    .function_returns
                    .iter()
                    .find(|f| f.type_token == token)
                    .map(|f| f.type_name.as_str())
            })
            .or_else(|| {
                parsed
                    .record_types
                    .iter()
                    .flat_map(|r| &r.fields)
                    .find(|f| f.type_token == Some(token))
                    .and_then(|f| f.type_name.as_deref())
            });
        let (name, arity) = if let Some(usage) = parsed.type_uses.iter().find(|u| u.token == token)
        {
            let (Some(name), Some(arity)) = (nominal_name(&usage.name), usage.generic_arity) else {
                return Ok(None);
            };
            (name, arity as usize)
        } else {
            let Some(text) = text else {
                return Ok(None);
            };
            let Some((name, args)) = nominal_application(text) else {
                return Ok(None);
            };
            (name, args.len())
        };
        self.logical_source_type(path, parsed, token, name, arity)
    }

    fn logical_source_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        mut name: String,
        arity: usize,
    ) -> Result<Option<Symbol>> {
        let global = name.starts_with("global.");
        if global {
            name = name[7..].into();
        }
        let offset = parsed.tokens[token].start;
        let mut choices = if global {
            vec![name.clone()]
        } else {
            Self::module_choices(parsed, token, &name, true)
        };
        let local = choices.iter().find_map(|choice| {
            parsed
                .type_aliases
                .iter()
                .filter(|a| {
                    a.name == *choice
                        && a.type_parameters.len() == arity
                        && a.start < offset
                        && offset < a.scope_end
                })
                .map(|a| (a.token, a.start, a.accessibility.as_str()))
                .chain(
                    parsed
                        .record_types
                        .iter()
                        .filter(|r| {
                            r.name == *choice
                                && r.type_parameters.len() == arity
                                && r.start < offset
                                && offset < r.scope_end
                        })
                        .map(|r| (r.token, r.start, r.accessibility.as_str())),
                )
                .max_by_key(|d| d.1)
        });
        let mut imports = Vec::new();
        if !name.contains('.') && !global {
            imports = self
                .effective_imports_since(path, parsed, token, local.map(|d| d.1).unwrap_or(0))?
                .into_iter()
                .filter(|o| {
                    o.offset < offset
                        && offset < o.scope_end
                        && local.is_none_or(|d| o.offset > d.1)
                })
                .collect();
        }
        if imports.is_empty() {
            if let Some((target, _, access)) = local {
                if self.nominal_accessible(path, parsed, target, access, path, parsed, token) {
                    return Ok(Some(self.source_symbol(
                        path,
                        parsed,
                        target,
                        &parsed.tokens[target].text,
                        false,
                        false,
                    )));
                }
                return Ok(None);
            }
        }
        if !self.context_exact(path) || self.ambiguous_instance(path) {
            return Ok(None);
        }
        let mut ordered = Vec::new();
        for open in imports.iter().rev() {
            if open.name.is_empty() {
                return Ok(None);
            }
            let first = open.name.split('.').next().unwrap_or(&open.name);
            let modules = if parsed
                .aliases
                .iter()
                .any(|a| a.name == first && a.offset < offset && offset < a.scope_end)
            {
                self.opened_modules(path, parsed, token, &open.name)?
            } else {
                Self::module_choices(parsed, token, &open.name, false)
            };
            if modules.is_empty() {
                return Ok(None);
            }
            ordered.extend(modules.into_iter().map(|module| format!("{module}.{name}")));
        }
        if !global {
            if let Some((qualifier, leaf)) = name.rsplit_once('.') {
                let first = qualifier.split('.').next().unwrap_or(qualifier);
                if parsed
                    .aliases
                    .iter()
                    .any(|a| a.name == first && a.offset < offset && offset < a.scope_end)
                {
                    choices = self
                        .opened_modules(path, parsed, token, qualifier)?
                        .into_iter()
                        .map(|m| format!("{m}.{leaf}"))
                        .collect();
                }
            }
        }
        ordered.extend(choices);
        let mut seen = std::collections::HashSet::new();
        for choice in ordered {
            if !seen.insert(choice.clone()) {
                continue;
            }
            match self.logical_type_choice(path, parsed, token, &choice, arity)? {
                DeclarationLookup::Resolved(symbol) => return Ok(Some(symbol)),
                DeclarationLookup::Unknown => return Ok(None),
                DeclarationLookup::Absent => {}
            }
        }
        Ok(None)
    }

    fn intrinsic_nameof(&mut self, path: &str, parsed: &ParsedFile, token: usize) -> Result<bool> {
        let blocker = if parsed.tokens[token].text != "nameof" || parsed.qualifier(token).is_some()
        {
            Some("intrinsic-nameof-shape")
        } else if !self.ensure_builtins()? {
            Some("intrinsic-nameof-environment")
        } else if self.source_name_shadow(path, parsed, token, "nameof", false)? {
            Some("intrinsic-nameof-source-type")
        } else if self.source_name_shadow(path, parsed, token, "nameof", true)? {
            Some("intrinsic-nameof-source-term")
        } else {
            None
        };
        if let Some(reason) = blocker {
            self.reference_note(path, token, reason.into());
            return Ok(false);
        }
        let visible = self.visible_files(path);
        let offset = parsed.tokens[token].start;
        for file in self.nominal_candidate_paths("nameof")? {
            if !visible.contains(&file) {
                continue;
            }
            let loaded = if file == path {
                None
            } else {
                Some(self.parsed(&file)?)
            };
            let owner = loaded.as_ref().unwrap_or(parsed);
            // Exported declarations outlive their module body's local interval.
            // Until precise type/case import ordering is available here, any
            // earlier such spelling remains a conservative intrinsic blocker.
            if owner.owned_types.iter().any(|t| {
                t.name.rsplit('.').next() == Some("nameof") && (file != path || t.start < offset)
            }) || owner.type_aliases.iter().any(|t| {
                t.name.rsplit('.').next() == Some("nameof") && (file != path || t.start < offset)
            }) || owner
                .pattern_declarations
                .iter()
                .any(|d| d.name == "nameof" && (file != path || d.scope_start < offset))
                || owner.uncertain.iter().any(|u| {
                    u.name.is_none() && uncertainty_can_bind_name(u, &file, path, offset, "nameof")
                })
            {
                if let Some(span) = owner.uncertain.iter().find(|u| {
                    u.name.is_none() && uncertainty_can_bind_name(u, &file, path, offset, "nameof")
                }) {
                    self.reference_note(
                        path,
                        token,
                        format!(
                            "intrinsic-nameof-source-uncertainty: {file}: {}:{}:{}",
                            span.start, span.end, span.reason
                        ),
                    );
                } else {
                    self.reference_note(
                        path,
                        token,
                        format!("intrinsic-nameof-source-declaration: {file}"),
                    );
                }
                return Ok(false);
            }
        }
        let env = self.builtin_environment.as_ref().unwrap();
        let Some(core) = env.unique_type("string", 0).map(|t| t["identity"].clone()) else {
            return Ok(false);
        };
        let (Some(terms), Some(types)) = (
            env.term_names(&self.index.db, "nameof")?,
            env.type_names(&self.index.db, "nameof")?,
        ) else {
            return Ok(false);
        };
        if !types.is_empty() {
            return Ok(false);
        }
        let matching = terms
            .into_iter()
            .filter(|row| {
                row["kind"] == "value"
                    && row["identity"]["assemblyPath"] == core["assemblyPath"]
                    && row["identity"]["assemblyName"] == core["assemblyName"]
                    && row["identity"]["fullName"] == "Microsoft.FSharp.Core.Operators.NameOf"
                    && row["xmlDocId"] == "M:Microsoft.FSharp.Core.Operators.NameOf``1(``0)"
            })
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Ok(false);
        }
        self.term_qualification_clear_except(path, parsed, token, "nameof", 1, matching.first())
    }

    pub(crate) fn definition_binding(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Result<Option<Symbol>> {
        let symbol = self.reference_binding(path, parsed, token)?;
        if let Some(candidate) = parsed
            .expression_type_candidates
            .iter()
            .find(|c| c.token == token)
        {
            if matches!(
                candidate.role,
                crate::syntax::ExpressionTypeRole::ApplicationHead
            ) {
                if let Some(found) = &symbol {
                    let loaded = if found.path == path {
                        None
                    } else {
                        Some(self.parsed(&found.path)?)
                    };
                    let owner = loaded.as_ref().unwrap_or(parsed);
                    if owner.type_aliases.iter().any(|a| a.token == found.token) {
                        // Constructor overload/physical definition routing is
                        // separate from this logical alias occurrence identity.
                        return Ok(None);
                    }
                }
            }
        }
        Ok(symbol)
    }

    // Alias constructor spelling has a logical reference identity even when
    // go-to-definition routes to an external constructor. This route is used
    // only by references, never by constructor definition navigation.
    fn reference_note(&mut self, path: &str, token: usize, reason: String) {
        if self.type_proofs.trace_references && self.type_proofs.reference_notes.len() < 32 {
            self.type_proofs
                .reference_notes
                .insert((path.into(), token), reason);
        }
    }

    // Classify an unresolved occurrence for opt-in diagnostics only. These
    // labels describe its syntax and available context, not a negative binding
    // proof; the occurrence remains in the incomplete reference domain.
    fn unresolved_reference_note(&mut self, path: &str, parsed: &ParsedFile, token: usize) {
        if !self.type_proofs.trace_references
            || self
                .type_proofs
                .reference_notes
                .contains_key(&(path.into(), token))
        {
            return;
        }
        let reason = if !supported(parsed, token) {
            "unresolved-source-syntax"
        } else if let Some(record) = parsed
            .record_expressions
            .iter()
            .find(|r| r.fields.contains(&token))
        {
            if record.copy.is_some() {
                "unresolved-record-copy-receiver"
            } else if record.parent_field.is_some() {
                "unresolved-nested-record-context"
            } else if record.argument.is_some() {
                "unresolved-record-argument-context"
            } else if record.method_result.is_some() && !record.method_result_literal_group {
                "unclosed-factory-constraint-group"
            } else if record.method_result.is_some() && !record.fresh_method_result {
                "unresolved-record-member-result-context"
            } else if record.type_name.is_some() {
                "unresolved-record-annotation"
            } else {
                "unresolved-record-label-environment"
            }
        } else if parsed.member_accesses.iter().any(|m| m.token == token) {
            "unresolved-member-receiver-or-dispatch"
        } else if parsed.non_value_tokens.contains(&token) {
            "unresolved-nonvalue-syntax-role"
        } else {
            "unresolved-source-binding"
        };
        self.reference_note(path, token, reason.into());
    }

    pub(crate) fn reference_binding(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Result<Option<Symbol>> {
        if parsed.explicit_constructor_heads.contains(&token) {
            // Explicit new enters the type namespace even when a local value
            // shares the written qualifier. Physical constructor routing is a
            // separate proof; never fall through to a local member/value here.
            return Ok(None);
        }
        if let Some(symbol) = self.type_definition(path, parsed, token)? {
            return Ok(Some(symbol));
        }
        let resolved = self.resolve(path, parsed, token)?;
        if resolved.as_ref().is_some_and(|symbol| symbol.local) {
            return Ok(resolved);
        }
        let Some(candidate) = parsed
            .expression_type_candidates
            .iter()
            .find(|c| c.token == token)
        else {
            if resolved.is_none() {
                self.unresolved_reference_note(path, parsed, token);
            }
            return Ok(resolved);
        };
        let simple = candidate.name.rsplit('.').next().unwrap_or(&candidate.name);
        let visible = self.visible_files(path);
        let mut alias_exists = false;
        let mut competing_case = false;
        let mut unsupported = false;
        for file in self.nominal_candidate_paths(simple)? {
            if !visible.contains(&file) {
                continue;
            }
            let loaded = if file == path {
                None
            } else {
                Some(self.parsed(&file)?)
            };
            let owner = loaded.as_ref().unwrap_or(parsed);
            let aliases = owner
                .type_aliases
                .iter()
                .filter(|a| a.name.rsplit('.').next() == Some(simple))
                .collect::<Vec<_>>();
            alias_exists |= !aliases.is_empty();
            let unsupported_here = if owner.tokens.is_empty() && !owner.supported {
                Some("missing-summary".to_string())
            } else if let Some(span) = owner.uncertain.iter().find(|u| u.name.is_none()) {
                Some(format!("{}:{}:{}", span.start, span.end, span.reason))
            } else if aliases.iter().any(|a| !a.type_parameters.is_empty()) {
                Some("generic-alias".to_string())
            } else {
                None
            };
            if let Some(reason) = unsupported_here {
                if !unsupported {
                    self.reference_note(
                        path,
                        token,
                        format!("alias-environment-unsupported: {file}: {reason}"),
                    );
                }
                unsupported = true;
            }
            for case in owner
                .pattern_declarations
                .iter()
                .filter(|d| d.name == simple)
            {
                // Singleton-case syntax may instead be an alias RHS. Only an
                // actual declaration-site type proof removes that ambiguity.
                let alias_rhs = owner
                    .type_aliases
                    .iter()
                    .any(|alias| alias.target_token == case.token);
                if !alias_rhs || self.type_definition(&file, owner, case.token)?.is_none() {
                    competing_case = true;
                    self.reference_note(
                        path,
                        token,
                        format!("source-case-competition: {file}:{}", case.token),
                    );
                }
            }
        }
        if !alias_exists {
            if resolved.is_none() {
                self.unresolved_reference_note(path, parsed, token);
            }
            return Ok(resolved);
        }
        // Value/type precedence depends on declaration order. A successful value
        // lookup alone cannot settle an expression when a source alias competes.
        // The initial envelope proves only a closed, nongeneric alias with no
        // competing expression binding; ordered competition stays unavailable.
        let eligible_role = match candidate.role {
            crate::syntax::ExpressionTypeRole::ApplicationHead => true,
            crate::syntax::ExpressionTypeRole::NameofArgument { callee } => {
                let proven = self.intrinsic_nameof(path, parsed, callee)?;
                if !proven {
                    if let Some(reason) = self
                        .type_proofs
                        .reference_notes
                        .get(&(path.into(), callee))
                        .cloned()
                    {
                        self.reference_note(path, token, reason);
                    }
                }
                proven
            }
        };
        let reason = if resolved.is_some() {
            Some("source-value-competition")
        } else if competing_case {
            Some("source-case-competition")
        } else if unsupported {
            Some("alias-environment-unsupported")
        } else if !supported(parsed, token) {
            Some("use-site-syntax-uncertainty")
        } else if !eligible_role {
            Some("intrinsic-nameof-not-proven")
        } else if candidate.name.contains('.') {
            Some("qualified-alias-expression-not-proven")
        } else if candidate.generic_arity.is_some_and(|arity| arity != 0) {
            Some("generic-alias-expression-not-proven")
        } else if !self.term_qualification_clear(path, parsed, token, &candidate.name, 1)? {
            Some("expression-term-absence-not-proven")
        } else {
            None
        };
        if let Some(reason) = reason {
            if self.type_proofs.trace_references
                && !self
                    .type_proofs
                    .reference_notes
                    .contains_key(&(path.into(), token))
            {
                self.reference_note(path, token, reason.into());
            }
            return Ok(None);
        }
        let Some(symbol) =
            self.logical_source_type(path, parsed, token, candidate.name.clone(), 0)?
        else {
            self.reference_note(path, token, "ordered-logical-type-not-proven".into());
            return Ok(None);
        };
        let loaded = if symbol.path == path {
            None
        } else {
            Some(self.parsed(&symbol.path)?)
        };
        let owner = loaded.as_ref().unwrap_or(parsed);
        if owner
            .type_aliases
            .iter()
            .any(|a| a.token == symbol.token && a.type_parameters.is_empty())
        {
            Ok(Some(symbol))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn paired_symbol(&mut self, symbol: &Symbol) -> Result<Option<Symbol>> {
        if symbol.local {
            return Ok(None);
        }
        let key = (symbol.path.clone(), symbol.token, symbol.context.clone());
        if let Some(pair) = self.type_proofs.pairs.get(&key) {
            return Ok(pair.clone());
        }
        let peer = if let Some(path) = symbol.path.strip_suffix(".fsi") {
            format!("{path}.fs")
        } else if symbol.path.ends_with(".fs") {
            format!("{}i", symbol.path)
        } else {
            return Ok(None);
        };
        let Some(project) = self
            .model
            .source_instance_project(&symbol.path, self.active_project.as_deref())
        else {
            return Ok(None);
        };
        if !project.exact {
            return Ok(None);
        }
        let signature = if symbol.path.ends_with(".fsi") {
            &symbol.path
        } else {
            &peer
        };
        let implementation = if symbol.path.ends_with(".fsi") {
            &peer
        } else {
            &symbol.path
        };
        let Some(si) = project.files.iter().position(|f| f == signature) else {
            return Ok(None);
        };
        let Some(ii) = project.files.iter().position(|f| f == implementation) else {
            return Ok(None);
        };
        if si >= ii {
            return Ok(None);
        }
        let origin = self.parsed(&symbol.path)?;
        let counterpart = self.parsed(&peer)?;
        if counterpart.tokens.is_empty() && !counterpart.supported {
            return Ok(None);
        }
        let mut tokens = Vec::new();
        if let Some(declaration) = origin.declaration_at(symbol.token) {
            if declaration.top_level {
                tokens.extend(
                    counterpart
                        .declarations
                        .iter()
                        .filter(|d| {
                            d.top_level
                                && d.name == symbol.name
                                && counterpart.declaration_module(d).unwrap_or("") == symbol.owner
                                && d.accessibility != "private"
                        })
                        .map(|d| d.token),
                );
            }
        } else if symbol.member {
            if let Some(record) = origin
                .record_types
                .iter()
                .find(|r| r.fields.iter().any(|f| f.token == symbol.token))
            {
                let field = record
                    .fields
                    .iter()
                    .find(|f| f.token == symbol.token)
                    .unwrap();
                tokens.extend(
                    counterpart
                        .record_types
                        .iter()
                        .filter(|r| {
                            r.name == record.name
                                && r.kind == record.kind
                                && r.type_parameters.len() == record.type_parameters.len()
                        })
                        .flat_map(|r| &r.fields)
                        .filter(|f| {
                            f.name == field.name
                                && f.kind == field.kind
                                && f.accessibility != "private"
                        })
                        .map(|f| f.token),
                );
            }
        } else if let Some(record) = origin.record_types.iter().find(|r| r.token == symbol.token) {
            tokens.extend(
                counterpart
                    .record_types
                    .iter()
                    .filter(|r| {
                        r.name == record.name
                            && r.kind == record.kind
                            && r.type_parameters.len() == record.type_parameters.len()
                    })
                    .map(|r| r.token),
            );
        } else if let Some(alias) = origin.type_aliases.iter().find(|a| a.token == symbol.token) {
            tokens.extend(
                counterpart
                    .type_aliases
                    .iter()
                    .filter(|a| {
                        a.name == alias.name
                            && a.type_parameters.len() == alias.type_parameters.len()
                    })
                    .map(|a| a.token),
            );
        }
        let result = if tokens.len() == 1 && supported(&counterpart, tokens[0]) {
            Some(self.source_symbol(
                &peer,
                &counterpart,
                tokens[0],
                &symbol.name,
                false,
                symbol.member,
            ))
        } else {
            None
        };
        if self.type_proofs.pairs.len() < 512 {
            self.type_proofs.pairs.insert(key, result.clone());
        }
        Ok(result)
    }
    pub(crate) fn canonical_symbol(&mut self, symbol: &Symbol) -> Result<Symbol> {
        if symbol.path.ends_with(".fs") {
            if let Some(pair) = self.paired_symbol(symbol)? {
                return Ok(pair);
            }
        }
        Ok(symbol.clone())
    }
    pub(crate) fn definition_target(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        symbol: Symbol,
    ) -> Result<Option<Symbol>> {
        if let Some(pair) = self.paired_symbol(&symbol)? {
            if symbol.path.ends_with(".fs") || symbol.path == path && symbol.token == token {
                return Ok(Some(pair));
            }
        }
        if symbol.path != path || symbol.token != token || !symbol.member {
            return Ok(Some(symbol));
        }
        let Some(record) = parsed
            .record_types
            .iter()
            .find(|r| r.fields.iter().any(|f| f.token == token))
        else {
            return Ok(Some(symbol));
        };
        let field = record.fields.iter().find(|f| f.token == token).unwrap();
        if !matches!(
            field.kind.as_str(),
            "default-property" | "override-property"
        ) {
            return Ok(Some(symbol));
        }
        let mut identity = TypeIdentity {
            path: path.into(),
            token: record.token,
            arguments: Vec::new(),
        };
        for _ in 0..32 {
            let loaded = if identity.path == path {
                None
            } else {
                Some(self.parsed(&identity.path)?)
            };
            let owner = loaded.as_ref().unwrap_or(parsed);
            let Some(r) = owner
                .record_types
                .iter()
                .find(|r| r.token == identity.token)
            else {
                return Ok(None);
            };
            if let Some(contract) = r
                .fields
                .iter()
                .find(|f| f.name == field.name && f.kind == "abstract-property")
            {
                return Ok(Some(self.source_symbol(
                    &identity.path,
                    owner,
                    contract.token,
                    &field.name,
                    false,
                    true,
                )));
            }
            let (Some(base), Some(at)) = (&r.base_type, r.base_type_token) else {
                return Ok(None);
            };
            let Some(next) = self.named_type(&identity.path, owner, at, base, 1)? else {
                return Ok(None);
            };
            identity = next;
        }
        Ok(None)
    }

    fn member_of_type(
        &mut self,
        identity: TypeIdentity,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        static_access: bool,
        depth: usize,
    ) -> Result<Option<Symbol>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        let loaded = if identity.path == path {
            None
        } else {
            Some(self.parsed(&identity.path)?)
        };
        let owner = loaded.as_ref().unwrap_or(parsed);
        let Some(record) = owner
            .record_types
            .iter()
            .find(|r| r.token == identity.token)
        else {
            return Ok(None);
        };
        let mut fields = record
            .fields
            .iter()
            .filter(|f| f.name == name)
            .collect::<Vec<_>>();
        if fields.len() == 2
            && fields.iter().any(|f| f.kind == "default-property")
            && fields.iter().any(|f| f.kind == "abstract-property")
        {
            fields.retain(|f| f.kind == "default-property");
        }
        if fields.len() > 1 {
            return Ok(None);
        }
        if let Some(field) = fields.first() {
            if (field.kind == "static-property") != static_access {
                return Ok(None);
            }
            if !supported(owner, field.token)
                || !self.type_accessible(
                    &identity.path,
                    owner,
                    record,
                    path,
                    parsed,
                    token,
                    &field.accessibility,
                )
            {
                return Ok(None);
            }
            return Ok(Some(self.source_symbol(
                &identity.path,
                owner,
                field.token,
                name,
                false,
                true,
            )));
        }
        if record.blocked_members.iter().any(|n| n == name) {
            return Ok(None);
        }
        if record.kind == "class" {
            if let (Some(base), Some(at)) = (&record.base_type, record.base_type_token) {
                if let Some(base) = self.named_type(&identity.path, owner, at, base, depth + 1)? {
                    return self.member_of_type(
                        base,
                        path,
                        parsed,
                        token,
                        name,
                        static_access,
                        depth + 1,
                    );
                }
            }
        }
        Ok(None)
    }

    fn value_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        if !self.proof_step(depth) || !supported(parsed, token) {
            return Ok(None);
        }
        if let Some(access) = parsed.member_accesses.iter().find(|m| m.token == token) {
            let Some(field) = self.member_binding_depth(path, parsed, token, depth + 1)? else {
                if let Some(member) = self.library_member(path, parsed, token, depth + 1)? {
                    if let TypeExpr::Source {
                        path,
                        token,
                        arguments,
                    } = member.return_type
                    {
                        if arguments.is_empty() {
                            return Ok(Some(TypeIdentity {
                                path,
                                token,
                                arguments: Vec::new(),
                            }));
                        }
                    }
                }
                // A dot may qualify a module value rather than a receiver field.
                if let Some(value) = self.resolve(path, parsed, token)? {
                    if !value.member {
                        let loaded = if value.path == path {
                            None
                        } else {
                            Some(self.parsed(&value.path)?)
                        };
                        return self.binding_type(
                            &value.path,
                            loaded.as_ref().unwrap_or(parsed),
                            value.token,
                            depth + 1,
                        );
                    }
                }
                return Ok(None);
            };
            let receiver = if let Some(source) = &access.source {
                self.source_type(path, parsed, token, source, depth + 1)?
            } else {
                self.value_type(path, parsed, access.receiver, depth + 1)?
            };
            let Some(receiver) = receiver else {
                return Ok(None);
            };
            let loaded = if field.path == path {
                None
            } else {
                Some(self.parsed(&field.path)?)
            };
            let owner = loaded.as_ref().unwrap_or(parsed);
            if let Some(record) = owner
                .record_types
                .iter()
                .find(|r| r.token == receiver.token && field.path == receiver.path)
            {
                if let Some(definition) = record.fields.iter().find(|f| f.token == field.token) {
                    if let (Some(name), Some(at)) = (&definition.type_name, definition.type_token) {
                        if let Some(parameter) =
                            record.type_parameters.iter().position(|p| p == name.trim())
                        {
                            let Some(argument) = receiver.arguments.get(parameter) else {
                                return Ok(None);
                            };
                            let argument_loaded = if argument.path == path {
                                None
                            } else {
                                Some(self.parsed(&argument.path)?)
                            };
                            let argument_owner = argument_loaded.as_ref().unwrap_or(parsed);
                            return self.named_type(
                                &argument.path,
                                argument_owner,
                                argument.token,
                                &argument.name,
                                depth + 1,
                            );
                        }
                        let result = self.named_type(&field.path, &owner, at, name, depth + 1)?;
                        return Ok(result.map(|mut result| {
                            for argument in &mut result.arguments {
                                if let Some(parameter) = record
                                    .type_parameters
                                    .iter()
                                    .position(|p| p == argument.name.trim())
                                {
                                    if let Some(replacement) = receiver.arguments.get(parameter) {
                                        *argument = replacement.clone();
                                    }
                                }
                            }
                            result
                        }));
                    }
                }
            }
            return Ok(None);
        }
        let Some(symbol) = self.resolve(path, parsed, token)? else {
            return Ok(None);
        };
        if symbol.path == path {
            self.binding_type(path, parsed, symbol.token, depth + 1)
        } else {
            let declaration_file = self.parsed(&symbol.path)?;
            self.binding_type(&symbol.path, &declaration_file, symbol.token, depth + 1)
        }
    }

    fn binding_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        binding: usize,
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        self.memoized_binding_type(path, parsed, binding, depth, false)
    }
    fn return_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        binding: usize,
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        self.memoized_binding_type(path, parsed, binding, depth, true)
    }
    fn memoized_binding_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        binding: usize,
        depth: usize,
        returns: bool,
    ) -> Result<Option<TypeIdentity>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        let key = (path.to_string(), binding, returns);
        if let Some(answer) = self.type_proofs.memo.get(&key) {
            return Ok(answer.clone());
        }
        if !self.type_proofs.active.insert(key.clone()) {
            return Ok(None);
        }
        let result = self.binding_type_inner(path, parsed, binding, depth, returns);
        self.type_proofs.active.remove(&key);
        if let Ok(answer) = &result {
            if self.type_proofs.memo.len() < 512 {
                self.type_proofs.memo.insert(key, answer.clone());
            }
        }
        result
    }
    fn binding_type_inner(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        binding: usize,
        depth: usize,
        returns: bool,
    ) -> Result<Option<TypeIdentity>> {
        if returns {
            if let Some(result) = parsed
                .function_returns
                .iter()
                .find(|f| f.binding == binding)
            {
                return self.named_type(
                    path,
                    parsed,
                    result.type_token,
                    &result.type_name,
                    depth + 1,
                );
            }
            if let Some(result) = parsed
                .function_results
                .iter()
                .find(|f| f.binding == binding)
            {
                return self.source_type(path, parsed, binding, &result.source, depth + 1);
            }
            if let Some(result) = parsed
                .record_expressions
                .iter()
                .find(|r| r.method_result == Some(binding))
            {
                if result.method_result_eligible || result.type_name.is_some() {
                    return self.record_receiver_type(path, parsed, result, depth + 1);
                }
            }
            return Ok(None);
        }
        if parsed.function_returns.iter().any(|f| f.binding == binding)
            || parsed.function_results.iter().any(|f| f.binding == binding)
        {
            return Ok(None);
        }
        // An explicit annotation is the strongest constraint. Do not bypass a
        // type we cannot resolve by guessing from the initializer instead.
        if let Some(TypeSource::Named { name, token }) = parsed
            .binding_types
            .iter()
            .filter(|f| f.binding == binding)
            .map(|f| &f.source)
            .find(|s| matches!(s, TypeSource::Named { .. }))
        {
            return self.named_type(path, parsed, *token, name, depth + 1);
        }
        if let Some(pattern) = parsed
            .pattern_bindings
            .iter()
            .find(|p| p.binding == binding && parsed.qualifier(p.constructor).is_some())
        {
            return self.qualified_union_payload(
                path,
                parsed,
                pattern.constructor,
                pattern.argument_index as usize,
                None,
                depth + 1,
            );
        }
        if let Some(pattern) = parsed
            .record_pattern_bindings
            .iter()
            .find(|p| p.binding == binding)
        {
            if let Some(field) =
                self.member_binding_depth(path, parsed, pattern.field, depth + 1)?
            {
                let loaded = if field.path == path {
                    None
                } else {
                    Some(self.parsed(&field.path)?)
                };
                let owner = loaded.as_ref().unwrap_or(parsed);
                if let Some(f) = owner
                    .record_types
                    .iter()
                    .flat_map(|r| &r.fields)
                    .find(|f| f.token == field.token)
                {
                    if let (Some(name), Some(at)) = (&f.type_name, f.type_token) {
                        return self.named_type(&field.path, owner, at, name, depth + 1);
                    }
                }
            }
            return Ok(None);
        }
        if parsed.pattern_bindings.iter().any(|p| p.binding == binding) {
            return self.option_payload_type(path, parsed, binding, depth + 1);
        }
        for fact in parsed.binding_types.iter().filter(|f| f.binding == binding) {
            if let Some(found) = self.source_type(path, parsed, binding, &fact.source, depth + 1)? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }
    fn source_expression(
        &mut self,
        identity: TypeIdentity,
        path: &str,
        parsed: &ParsedFile,
        depth: usize,
    ) -> Result<Option<TypeExpr>> {
        let mut arguments = Vec::new();
        for argument in identity.arguments {
            let loaded = if argument.path == path {
                None
            } else {
                Some(self.parsed(&argument.path)?)
            };
            let Some(value) = self.library_named_type(
                &argument.path,
                loaded.as_ref().unwrap_or(parsed),
                argument.token,
                &argument.name,
                depth + 1,
            )?
            else {
                return Ok(None);
            };
            arguments.push(value);
        }
        Ok(Some(TypeExpr::Source {
            path: identity.path,
            token: identity.token,
            arguments,
        }))
    }
    fn instantiate_library(ty: &TypeExpr, args: &[TypeExpr], depth: usize) -> Option<TypeExpr> {
        if depth > 32 {
            return None;
        }
        library_types::instantiate(ty, args, &[])
    }
    fn library_type_choice(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        args: &[String],
        depth: usize,
    ) -> Result<Lookup<TypeExpr>> {
        if !self.proof_step(depth) {
            return Ok(Lookup::Unknown);
        }
        let simple = name.rsplit('.').next().unwrap_or(name);
        let visible = self.visible_files(path);
        for candidate in self.type_candidate_paths(simple)? {
            if !visible.contains(&candidate) {
                continue;
            }
            let loaded = if candidate == path {
                None
            } else {
                Some(self.parsed(&candidate)?)
            };
            let owner = loaded.as_ref().unwrap_or(parsed);
            if owner.tokens.is_empty() && !owner.supported {
                return Ok(Lookup::Unknown);
            }
            if candidate != path
                && candidate.ends_with(".fs")
                && visible.contains(&format!("{candidate}i"))
            {
                continue;
            }
            if let Some(record) = owner.record_types.iter().find(|r| {
                r.name == name
                    && (candidate != path
                        || r.start <= parsed.tokens[token].start
                            && parsed.tokens[token].start < r.scope_end)
            }) {
                if !self.type_accessible(
                    &candidate,
                    owner,
                    record,
                    path,
                    parsed,
                    token,
                    &record.accessibility,
                ) {
                    continue;
                }
                let identity = self.instantiate_type(
                    TypeIdentity {
                        path: candidate.clone(),
                        token: record.token,
                        arguments: Vec::new(),
                    },
                    path,
                    token,
                    args,
                    parsed,
                )?;
                return Ok(Lookup::from_proof(match identity {
                    Some(i) => self.source_expression(i, path, parsed, depth + 1)?,
                    None => None,
                }));
            }
            if let Some(alias) = owner.type_aliases.iter().find(|a| {
                a.name == name
                    && (candidate != path
                        || a.start <= parsed.tokens[token].start
                            && parsed.tokens[token].start < a.scope_end)
            }) {
                if !self.nominal_accessible(
                    &candidate,
                    owner,
                    alias.token,
                    &alias.accessibility,
                    path,
                    parsed,
                    token,
                ) {
                    continue;
                }
                if !alias.type_parameters.is_empty() || !args.is_empty() {
                    return Ok(Lookup::Unknown);
                }
                return Ok(Lookup::from_proof(self.library_named_type(
                    &candidate,
                    owner,
                    alias.target_token,
                    &alias.target,
                    depth + 1,
                )?));
            }
            if owner.owned_types.iter().any(|t| {
                t.name == name
                    && (candidate != path
                        || t.start <= parsed.tokens[token].start
                            && parsed.tokens[token].start < t.end)
            }) {
                return Ok(Lookup::Unknown);
            }
        }
        let entries = match self.external_type_candidates(name, args.len())? {
            Lookup::Resolved(entries) => entries,
            Lookup::Absent => return Ok(Lookup::Absent),
            Lookup::Unknown => return Ok(Lookup::Unknown),
        };
        if entries.len() != 1 {
            return Ok(Lookup::Unknown);
        }
        let identity: LibraryIdentity = match serde_json::from_value(entries[0]["identity"].clone())
        {
            Ok(i) => i,
            Err(_) => return Ok(Lookup::Unknown),
        };
        let mut arguments = Vec::new();
        for arg in args {
            let Some(arg) = self.library_named_type(path, parsed, token, arg, depth + 1)? else {
                return Ok(Lookup::Unknown);
            };
            arguments.push(arg);
        }
        // The complete import inventory can certify a physical nominal owner
        // without loading rendered signatures. F# abbreviations and erased
        // measures lack this guarantee and still require canonical type facts.
        if entries[0]["isNominal"] == true && entries[0]["isAbbreviation"] == false {
            return Ok(Lookup::Resolved(TypeExpr::Named {
                identity,
                arguments,
            }));
        }
        if !self.ensure_metadata()? {
            return Ok(Lookup::Unknown);
        }
        let choices =
            library_types::lookup_type_for_declaration(self.catalog_db(), name, &identity)?;
        if choices.len() != 1 {
            return Ok(Lookup::Unknown);
        }
        Ok(Lookup::from_proof(Self::instantiate_library(
            &choices[0],
            &arguments,
            0,
        )))
    }
    // A type-position proof is insufficient for an expression qualifier: a
    // value or module may bind the same prefix before the nominal type is used.
    // Certify absence only in the applicable complete import environments.
    fn external_scope_is_open(
        &mut self,
        scope: &str,
        active: &std::collections::HashSet<String>,
        depth: usize,
    ) -> Result<Option<bool>> {
        if active.contains(scope) {
            return Ok(Some(true));
        }
        if scope.is_empty() {
            return Ok(Some(false));
        }
        if !self.proof_step(depth) {
            return Ok(None);
        }
        let simple = scope.rsplit('.').next().unwrap_or(scope);
        let Some(rows) = self
            .builtin_environment
            .as_ref()
            .unwrap()
            .scope_names(&self.index.db, simple)?
        else {
            return Ok(None);
        };
        for row in rows
            .iter()
            .filter(|r| r["sourceFullName"].as_str() == Some(scope))
        {
            if row["kind"] == "module" && row["autoOpen"] == true {
                let Some(parent) = row["enclosingScope"].as_str() else {
                    return Ok(None);
                };
                if parent == scope {
                    return Ok(None);
                }
                match self.external_scope_is_open(parent, active, depth + 1)? {
                    Some(true) => return Ok(Some(true)),
                    None => return Ok(None),
                    _ => {}
                }
            }
        }
        Ok(Some(false))
    }
    fn term_qualification_clear(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        text: &str,
        depth: usize,
    ) -> Result<bool> {
        self.term_qualification_clear_except(path, parsed, token, text, depth, None)
    }

    fn term_qualification_clear_except(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        text: &str,
        depth: usize,
        allowed: Option<&serde_json::Value>,
    ) -> Result<bool> {
        if !self.proof_step(depth) || !self.ensure_builtins()? {
            return Ok(false);
        }
        let Some(mut name) = nominal_name(text) else {
            return Ok(false);
        };
        let global = name.starts_with("global.");
        if global {
            name = name[7..].to_string();
        }
        let parts = name.split('.').collect::<Vec<_>>();
        if parts.is_empty() || parts.len() > 64 {
            return Ok(false);
        }
        let offset = parsed.tokens[token].start;
        if !global
            && parsed
                .potential_bindings
                .iter()
                .any(|d| d.name == parts[0] && d.visible_start <= offset && offset < d.scope_end)
        {
            return Ok(false);
        }
        if !global
            && parsed.uncertain.iter().any(|u| {
                u.start <= offset
                    && offset < u.end
                    && u.name.as_deref().is_none_or(|n| n == parts[0])
                    && !parsed
                        .potential_bindings
                        .iter()
                        .any(|d| parsed.tokens[d.token].start == u.start && d.name == parts[0])
            })
        {
            return Ok(false);
        }
        let mut active = if global {
            std::collections::HashSet::from([String::new()])
        } else {
            let Some(scopes) = self.source_lookup_scopes(path, parsed, token)? else {
                return Ok(false);
            };
            scopes
        };
        if !global {
            // Module abbreviations capture an ordered module environment. Until
            // that environment is represented here, they are not nominal roots.
            if parsed
                .aliases
                .iter()
                .any(|a| a.name == parts[0] && a.offset < offset && offset < a.scope_end)
            {
                return Ok(false);
            }
            let Some(opens) = self
                .builtin_environment
                .as_ref()
                .unwrap()
                .assembly_auto_opens()
            else {
                return Ok(false);
            };
            for open in opens {
                let Some(scope) = open["modulePath"].as_str() else {
                    return Ok(false);
                };
                active.insert(scope.to_string());
            }
        }
        if active.len() > 2048 {
            return Ok(false);
        }
        let visible = self.visible_files(path);
        let mut allowed_visible = false;
        let mut prefix = String::new();
        for (position, part) in parts.iter().enumerate() {
            if !self.proof_step(depth + 1) {
                return Ok(false);
            }
            if !prefix.is_empty() {
                prefix.push('.');
            }
            prefix.push_str(part);
            let Some(terms) = self
                .builtin_environment
                .as_ref()
                .unwrap()
                .term_names(&self.index.db, part)?
            else {
                return Ok(false);
            };
            let Some(scopes) = self
                .builtin_environment
                .as_ref()
                .unwrap()
                .scope_names(&self.index.db, part)?
            else {
                return Ok(false);
            };
            let mut candidates = std::collections::HashSet::new();
            for scope in &active {
                candidates.insert(if scope.is_empty() {
                    prefix.clone()
                } else {
                    format!("{scope}.{prefix}")
                });
            }
            for row in terms {
                let Some(full) = row["sourceFullName"].as_str() else {
                    return Ok(false);
                };
                if allowed == Some(&row) {
                    let declaring = row["declaringScope"].as_str().unwrap_or("");
                    let visible = if candidates.contains(full) {
                        Some(true)
                    } else if position == 0 && !global {
                        self.external_scope_is_open(declaring, &active, depth + 1)?
                    } else {
                        Some(false)
                    };
                    match visible {
                        Some(true) => allowed_visible = true,
                        Some(false) => {}
                        None => return Ok(false),
                    }
                    continue;
                }
                if candidates.contains(full) {
                    return Ok(false);
                }
                if position == 0 && !global {
                    let Some(declaring) = row["declaringScope"].as_str() else {
                        return Ok(false);
                    };
                    let scope = if row["kind"] == "unionCase" {
                        row["enclosingScope"].as_str().unwrap_or(declaring)
                    } else {
                        declaring
                    };
                    // Enum/static literal fields are visible only through their
                    // actual declaring type/open type, not its namespace.
                    if self.external_scope_is_open(scope, &active, depth + 1)? != Some(false) {
                        return Ok(false);
                    }
                }
            }
            for row in scopes {
                if row["kind"] != "module" {
                    continue;
                }
                let Some(full) = row["sourceFullName"].as_str() else {
                    return Ok(false);
                };
                if candidates.contains(full) {
                    return Ok(false);
                }
                if position == 0 && !global {
                    let Some(parent) = row["enclosingScope"].as_str() else {
                        return Ok(false);
                    };
                    if self.external_scope_is_open(parent, &active, depth + 1)? != Some(false) {
                        return Ok(false);
                    }
                }
            }
            // Source declarations use the same applicable prefix paths. No
            // global spelling scan is treated as a binding proof.
            if position == 0
                && !global
                && parsed
                    .declarations
                    .iter()
                    .any(|d| d.name == *part && d.visible_start <= offset && offset < d.scope_end)
            {
                return Ok(false);
            }
            for candidate in &candidates {
                let (scope, name) = candidate.rsplit_once('.').unwrap_or(("", candidate));
                if !self
                    .visible_exports(scope, name, path, parsed, token, &visible)?
                    .is_empty()
                {
                    return Ok(false);
                }
            }
            // A closed module can export an unresolved uppercase binding. Its
            // lexical uncertainty span has ended, but opening/qualifying that
            // module does not turn the missing declaration into proven absence.
            for file in self.nominal_candidate_paths(part)? {
                if !visible.contains(&file) {
                    continue;
                }
                if file != path && file.ends_with(".fs") && visible.contains(&format!("{file}i")) {
                    continue;
                }
                let loaded = if file == path {
                    None
                } else {
                    Some(self.parsed(&file)?)
                };
                let owner = loaded.as_ref().unwrap_or(parsed);
                for declaration in owner
                    .declarations
                    .iter()
                    .chain(&owner.potential_bindings)
                    .filter(|d| {
                        d.top_level
                            && d.name == *part
                            && (file != path || d.visible_start <= offset)
                    })
                {
                    if !self.nominal_accessible(
                        &file,
                        owner,
                        declaration.token,
                        &declaration.accessibility,
                        path,
                        parsed,
                        token,
                    ) {
                        continue;
                    }
                    let scope = owner.declaration_module(declaration).unwrap_or("");
                    let full = if scope.is_empty() {
                        part.to_string()
                    } else {
                        format!("{scope}.{part}")
                    };
                    if candidates.contains(&full)
                        || position == 0
                            && !global
                            && self.source_scope_is_open(&file, owner, scope, &active, depth + 1)?
                                != Some(false)
                    {
                        return Ok(false);
                    }
                }
                for declaration in owner.pattern_declarations.iter().filter(|d| {
                    d.top_level && d.name == *part && (file != path || d.scope_start <= offset)
                }) {
                    if !self.nominal_accessible(
                        &file,
                        owner,
                        declaration.token,
                        &declaration.accessibility,
                        path,
                        parsed,
                        token,
                    ) {
                        continue;
                    }
                    // The grammar's singleton-case candidate may be an alias
                    // RHS; only declaration-site type proof discharges it.
                    if owner
                        .type_aliases
                        .iter()
                        .any(|a| a.target_token == declaration.token)
                        && self
                            .type_definition(&file, owner, declaration.token)?
                            .is_some()
                    {
                        continue;
                    }
                    let full = format!("{}.{}", declaration.module, part);
                    let qualified = declaration
                        .type_owner
                        .as_ref()
                        .map(|owner| format!("{owner}.{part}"));
                    if (!declaration.qualified_only && candidates.contains(&full))
                        || qualified
                            .as_ref()
                            .is_some_and(|name| candidates.contains(name))
                        || position == 0
                            && !global
                            && !declaration.qualified_only
                            && self.source_scope_is_open(
                                &file,
                                owner,
                                &declaration.module,
                                &active,
                                depth + 1,
                            )? != Some(false)
                    {
                        return Ok(false);
                    }
                }
            }
            for (file, module, valid) in self.module_facts(part)? {
                if !visible.contains(&file) {
                    continue;
                }
                if !valid {
                    return Ok(false);
                }
                if candidates.contains(&module.name) && (file != path || module.start < offset) {
                    return Ok(false);
                }
            }
        }
        Ok(allowed.is_none() || allowed_visible)
    }
    pub(crate) fn qualified_nominal_receiver(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        text: &str,
        depth: usize,
    ) -> Result<Lookup<TypeExpr>> {
        if !self.term_qualification_clear(path, parsed, token, text, depth + 1)? {
            return Ok(Lookup::Unknown);
        }
        self.library_named_type_proof(path, parsed, token, text, depth + 1)
    }
    pub(crate) fn library_named_type_proof(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        text: &str,
        depth: usize,
    ) -> Result<Lookup<TypeExpr>> {
        if !self.proof_step(depth) || !supported(parsed, token) {
            return Ok(Lookup::Unknown);
        }
        // Source names that dominate all later imports need no external work.
        if let Some(identity) = self.named_type(path, parsed, token, text, depth + 1)? {
            return Ok(Lookup::from_proof(self.source_expression(
                identity,
                path,
                parsed,
                depth + 1,
            )?));
        }
        let Some((mut name, args)) = nominal_application(text) else {
            return Ok(Lookup::Unknown);
        };
        let global = name.starts_with("global.");
        if global {
            name = name[7..].into();
        }
        let offset = parsed.tokens[token].start;
        let mut choices = Vec::new();
        let local_start = if !name.contains('.') && !global {
            Self::module_choices(parsed, token, &name, false)
                .into_iter()
                .find_map(|choice| {
                    parsed
                        .owned_types
                        .iter()
                        .filter(|t| t.name == choice && t.start < offset && offset < t.end)
                        .map(|t| t.start)
                        .max()
                })
        } else {
            None
        };
        if !name.contains('.') && !global {
            let imports =
                self.effective_imports_since(path, parsed, token, local_start.unwrap_or(0))?;
            for open in imports.iter().rev().filter(|o| {
                o.offset < offset
                    && offset < o.scope_end
                    && local_start.is_none_or(|s| o.offset > s)
            }) {
                if open.name.is_empty() {
                    return Ok(Lookup::Unknown);
                }
                // Source and external module/namespace environments are merged
                // in order. Missing detailed rows never certify name absence.
                for module in Self::module_choices(parsed, token, &open.name, false) {
                    choices.push(format!("{module}.{name}"));
                }
            }
        }
        choices.extend(if global {
            vec![name.clone()]
        } else {
            Self::module_choices(parsed, token, &name, true)
        });
        if !name.contains('.') && !global {
            choices.push(format!("Microsoft.FSharp.Core.{name}"));
        }
        let mut seen = std::collections::HashSet::new();
        for choice in choices {
            if !seen.insert(choice.clone()) {
                continue;
            }
            match self.library_type_choice(path, parsed, token, &choice, &args, depth + 1)? {
                Lookup::Absent => {}
                outcome => return Ok(outcome),
            }
        }
        Ok(Lookup::Absent)
    }
    pub(crate) fn library_named_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        text: &str,
        depth: usize,
    ) -> Result<Option<TypeExpr>> {
        Ok(self
            .library_named_type_proof(path, parsed, token, text, depth)?
            .resolved())
    }
    fn library_binding_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        binding: usize,
        depth: usize,
    ) -> Result<Option<TypeExpr>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        for fact in parsed.binding_types.iter().filter(|f| f.binding == binding) {
            if let TypeSource::Named { name, token } = &fact.source {
                return self.library_named_type(path, parsed, *token, name, depth + 1);
            }
        }
        for fact in parsed.binding_types.iter().filter(|f| f.binding == binding) {
            if let Some(ty) =
                self.library_source_type(path, parsed, binding, &fact.source, depth + 1)?
            {
                return Ok(Some(ty));
            }
        }
        Ok(None)
    }
    fn library_source_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        source: &TypeSource,
        depth: usize,
    ) -> Result<Option<TypeExpr>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        match source {
            TypeSource::Named { name, token } => {
                self.library_named_type(path, parsed, *token, name, depth + 1)
            }
            TypeSource::Alias { token } => self.library_value_type(path, parsed, *token, depth + 1),
            TypeSource::FunctionResult {
                function,
                arguments,
            } => {
                let Some(callee) = self.resolve(path, parsed, *function)? else {
                    return Ok(None);
                };
                let loaded = if callee.path == path {
                    None
                } else {
                    Some(self.parsed(&callee.path)?)
                };
                let owner = loaded.as_ref().unwrap_or(parsed);
                if let Some(result) = owner
                    .function_returns
                    .iter()
                    .find(|f| f.binding == callee.token && f.arity == *arguments)
                {
                    return self.library_named_type(
                        &callee.path,
                        owner,
                        result.type_token,
                        &result.type_name,
                        depth + 1,
                    );
                }
                Ok(None)
            }
            _ => {
                if let Some(identity) = self.source_type(path, parsed, token, source, depth + 1)? {
                    self.source_expression(identity, path, parsed, depth + 1)
                } else {
                    Ok(None)
                }
            }
        }
    }
    fn library_value_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        depth: usize,
    ) -> Result<Option<TypeExpr>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        if parsed.member_accesses.iter().any(|m| m.token == token) {
            if let Some(member) = self.library_member(path, parsed, token, depth + 1)? {
                return Ok(Some(member.return_type));
            }
        }
        let Some(symbol) = self.resolve(path, parsed, token)? else {
            return Ok(None);
        };
        if symbol.member {
            return Ok(None);
        }
        let loaded = if symbol.path == path {
            None
        } else {
            Some(self.parsed(&symbol.path)?)
        };
        self.library_binding_type(
            &symbol.path,
            loaded.as_ref().unwrap_or(parsed),
            symbol.token,
            depth + 1,
        )
    }
    pub(crate) fn display_proven_type(
        &mut self,
        ty: &TypeExpr,
        depth: usize,
    ) -> Result<Option<String>> {
        if depth > 32 {
            return Ok(None);
        }
        let (name, arguments) = match ty {
            TypeExpr::Source {
                path,
                token,
                arguments,
            } => {
                let parsed = self.parsed(path)?;
                let Some(record) = parsed.record_types.iter().find(|r| r.token == *token) else {
                    return Ok(None);
                };
                (
                    record
                        .name
                        .rsplit('.')
                        .next()
                        .unwrap_or(&record.name)
                        .to_string(),
                    arguments,
                )
            }
            TypeExpr::Named {
                identity,
                arguments,
            } => {
                let name = match identity.full_name.as_str() {
                    "System.Int32" => "int",
                    "System.String" => "string",
                    "System.Boolean" => "bool",
                    "System.Int64" => "int64",
                    "System.Double" => "float",
                    "System.Single" => "float32",
                    "System.Object" => "obj",
                    other => other.split('`').next().unwrap_or(other),
                };
                (name.to_string(), arguments)
            }
            _ => return Ok(None),
        };
        if arguments.is_empty() {
            return Ok(Some(name));
        }
        let mut shown = Vec::new();
        for argument in arguments {
            let Some(text) = self.display_proven_type(argument, depth + 1)? else {
                return Ok(None);
            };
            shown.push(text);
        }
        Ok(Some(format!("{}<{}>", name, shown.join(", "))))
    }
    pub(crate) fn library_member(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        depth: usize,
    ) -> Result<Option<LibraryMember>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        let Some(access) = parsed.member_accesses.iter().find(|m| m.token == token) else {
            return Ok(None);
        };
        let mut is_static = false;
        let mut receiver = if let Some(source) = &access.source {
            self.library_source_type(path, parsed, token, source, depth + 1)?
        } else {
            self.library_value_type(path, parsed, access.receiver, depth + 1)?
        };
        if receiver.is_none()
            && access.source.is_none()
            && self.resolve(path, parsed, access.receiver)?.is_none()
        {
            if let Some(qualifier) = parsed.qualifier(token) {
                receiver = self
                    .qualified_nominal_receiver(
                        path,
                        parsed,
                        access.receiver,
                        &qualifier,
                        depth + 1,
                    )?
                    .resolved();
                is_static = receiver.is_some();
            }
        }
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        if matches!(receiver, TypeExpr::Source { .. }) || !self.ensure_metadata()? {
            return Ok(None);
        }
        library_types::lookup_property(
            self.catalog_db(),
            &receiver,
            &parsed.tokens[token].text,
            is_static,
        )
    }

    fn explicit_annotation(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        depth: usize,
    ) -> Result<Option<(String, usize, String)>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        let Some(symbol) = self.resolve(path, parsed, token)? else {
            return Ok(None);
        };
        let loaded = if symbol.path == path {
            None
        } else {
            Some(self.parsed(&symbol.path)?)
        };
        let owner = loaded.as_ref().unwrap_or(parsed);
        for fact in owner
            .binding_types
            .iter()
            .filter(|f| f.binding == symbol.token)
        {
            match &fact.source {
                TypeSource::Named { name, token } => {
                    return Ok(Some((symbol.path.clone(), *token, name.clone())))
                }
                TypeSource::Alias { token } => {
                    return self.explicit_annotation(&symbol.path, owner, *token, depth + 1)
                }
                _ => {}
            }
        }
        Ok(None)
    }
    // A declaration's lexical interval is not its exported lifetime. Keep the
    // applicable source scopes separate so a closed, opened module can still
    // contribute values, patterns and types, while an unopened sibling cannot.
    fn source_lookup_scopes(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Result<Option<std::collections::HashSet<String>>> {
        let offset = parsed.tokens[token].start;
        let mut scopes = std::collections::HashSet::from([String::new()]);
        let mut current = parsed.module_at(token);
        while let Some(scope) = current {
            scopes.insert(scope.to_string());
            current = scope.rsplit_once('.').map(|p| p.0);
        }
        for import in self.effective_imports(path, parsed, token)? {
            if import.offset >= offset || offset >= import.scope_end {
                continue;
            }
            if import.name.is_empty() {
                return Ok(None);
            }
            scopes.extend(Self::module_choices(parsed, token, &import.name, false));
            scopes.extend(self.opened_modules(path, parsed, token, &import.name)?);
        }
        Ok((scopes.len() <= 2048).then_some(scopes))
    }

    fn source_scope_is_open(
        &mut self,
        owner_path: &str,
        owner: &ParsedFile,
        scope: &str,
        active: &std::collections::HashSet<String>,
        depth: usize,
    ) -> Result<Option<bool>> {
        if !self.proof_step(depth) {
            return Ok(None);
        }
        if active.contains(scope) {
            return Ok(Some(true));
        }
        let parent = scope.rsplit_once('.').map(|p| p.0).unwrap_or("");
        let attributes = owner
            .attributes
            .iter()
            .filter(|a| a.owner_kind == "module" && a.owner == scope)
            .collect::<Vec<_>>();
        if attributes.is_empty() {
            return Ok(Some(false));
        }
        let parent_open =
            self.source_scope_is_open(owner_path, owner, parent, active, depth + 1)?;
        if parent_open == Some(false) {
            return Ok(Some(false));
        }
        let mut unknown = parent_open.is_none();
        for attribute in attributes {
            match self.attribute_effect(owner_path, owner, attribute)? {
                AttributeEffect::AutoOpen if !unknown => return Ok(Some(true)),
                AttributeEffect::Unknown => unknown = true,
                _ => {}
            }
        }
        Ok(if unknown { None } else { Some(false) })
    }

    fn source_name_shadow(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        pattern: bool,
    ) -> Result<bool> {
        let Some(active) = self.source_lookup_scopes(path, parsed, token)? else {
            return Ok(true);
        };
        let offset = parsed.tokens[token].start;
        let visible = self.visible_files(path);
        for candidate in self.nominal_candidate_paths(name)? {
            if !visible.contains(&candidate) {
                continue;
            }
            if candidate != path
                && candidate.ends_with(".fs")
                && visible.contains(&format!("{candidate}i"))
            {
                continue;
            }
            let loaded = if candidate == path {
                None
            } else {
                Some(self.parsed(&candidate)?)
            };
            let owner = loaded.as_ref().unwrap_or(parsed);
            if owner.tokens.is_empty() && !owner.supported {
                return Ok(true);
            }
            if pattern {
                for declaration in owner
                    .declarations
                    .iter()
                    .chain(&owner.potential_bindings)
                    .filter(|d| d.name == name && (candidate != path || d.visible_start <= offset))
                {
                    if !self.nominal_accessible(
                        &candidate,
                        owner,
                        declaration.token,
                        &declaration.accessibility,
                        path,
                        parsed,
                        token,
                    ) {
                        continue;
                    }
                    if !declaration.top_level {
                        if candidate == path && offset < declaration.scope_end {
                            return Ok(true);
                        }
                    } else {
                        let scope = owner.declaration_module(declaration).unwrap_or("");
                        if self.source_scope_is_open(&candidate, owner, scope, &active, 0)?
                            != Some(false)
                        {
                            return Ok(true);
                        }
                    }
                }

                for declaration in owner.pattern_declarations.iter().filter(|d| d.name == name) {
                    if candidate == path && declaration.scope_start > offset {
                        continue;
                    }
                    if !self.nominal_accessible(
                        &candidate,
                        owner,
                        declaration.token,
                        &declaration.accessibility,
                        path,
                        parsed,
                        token,
                    ) {
                        continue;
                    }
                    if !declaration.top_level {
                        if candidate == path
                            && declaration.scope_start <= offset
                            && offset < declaration.scope_end
                        {
                            return Ok(true);
                        }
                        continue;
                    }
                    if !declaration.qualified_only
                        && self.source_scope_is_open(
                            &candidate,
                            owner,
                            &declaration.module,
                            &active,
                            0,
                        )? != Some(false)
                    {
                        return Ok(true);
                    }
                }
            } else {
                for declaration in owner.owned_types.iter().filter(|t| {
                    t.name.rsplit('.').next() == Some(name)
                        && (candidate != path || t.start <= offset)
                }) {
                    let scope = declaration.name.rsplit_once('.').map(|p| p.0).unwrap_or("");
                    if self.source_scope_is_open(&candidate, owner, scope, &active, 0)?
                        != Some(false)
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
    fn option_payload_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        binding: usize,
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        let Some(fact) = parsed
            .pattern_bindings
            .iter()
            .find(|f| f.binding == binding)
        else {
            return Ok(None);
        };
        if fact.argument_index != 0 {
            return Ok(None);
        }
        let case = parsed.tokens[fact.constructor].text.as_str();
        if !matches!(case, "Some" | "ValueSome") || parsed.qualifier(fact.constructor).is_some() {
            return Ok(None);
        }
        let Some((owner_path, at, text)) =
            self.explicit_annotation(path, parsed, fact.scrutinee, depth + 1)?
        else {
            return Ok(None);
        };
        let (container, payload) = if let Some(inner) = text.trim().strip_suffix(" option") {
            ("option".to_string(), inner.trim().to_string())
        } else if let Some(inner) = text.trim().strip_suffix(" voption") {
            ("voption".to_string(), inner.trim().to_string())
        } else if let Some((name, args)) = nominal_application(&text) {
            if args.len() != 1 {
                return Ok(None);
            }
            (name, args[0].clone())
        } else {
            return Ok(None);
        };
        if !matches!(
            (container.as_str(), case),
            ("option", "Some") | ("voption", "ValueSome")
        ) {
            return Ok(None);
        }
        let loaded = if owner_path == path {
            None
        } else {
            Some(self.parsed(&owner_path)?)
        };
        let annotation = loaded.as_ref().unwrap_or(parsed);
        if self.source_name_shadow(&owner_path, annotation, at, &container, false)?
            || self.source_name_shadow(path, parsed, fact.constructor, case, true)?
            || !self.ensure_builtins()?
        {
            return Ok(None);
        }
        let env = self.builtin_environment.as_ref().unwrap();
        let (Some(ty), Some(pattern)) = (env.unique_type(&container, 1), env.unique_pattern(case))
        else {
            return Ok(None);
        };
        if pattern["payloadCount"] != 1
            || pattern["payloadType"]["kind"] != "typeParameter"
            || pattern["payloadType"]["index"] != 0
            || pattern["declaringType"]["identity"] != ty["canonicalType"]["identity"]
        {
            return Ok(None);
        }
        self.named_type(&owner_path, annotation, at, &payload, depth + 1)
    }

    fn selector_has_value_receiver(parsed: &ParsedFile, token: usize) -> bool {
        let mut current = token;
        for _ in 0..64 {
            let Some(access) = parsed
                .member_accesses
                .iter()
                .find(|access| access.token == current)
            else {
                return false;
            };
            if access.source.is_some() {
                // Parenthesized/annotated expression facts are not used as a
                // negative type-namespace proof until their root is certified.
                return false;
            }
            if parsed
                .local_binding(access.receiver)
                .is_some_and(|binding| !binding.top_level)
                && supported(parsed, access.receiver)
            {
                return true;
            }
            if access.receiver >= current {
                return false;
            }
            current = access.receiver;
        }
        false
    }

    pub(crate) fn type_reference_role_disjoint(parsed: &ParsedFile, token: usize) -> bool {
        // The grammar's singleton union case can also be an alias RHS. Keep
        // these candidate type positions until nominal lookup settles them.
        if parsed.explicit_constructor_heads.contains(&token)
            || parsed.type_uses.iter().any(|usage| usage.token == token)
            || parsed
                .type_aliases
                .iter()
                .any(|alias| alias.token == token || alias.target_token == token)
            || parsed
                .record_types
                .iter()
                .any(|record| record.token == token)
        {
            return false;
        }
        parsed
            .record_expressions
            .iter()
            .any(|r| r.fields.contains(&token))
            || parsed
                .record_pattern_bindings
                .iter()
                .any(|p| p.field == token)
            || parsed
                .record_types
                .iter()
                .flat_map(|r| &r.fields)
                .any(|f| f.token == token)
            || parsed.pattern_declarations.iter().any(|d| d.token == token)
            || parsed.declaration_at(token).is_some()
            || parsed.potential_bindings.iter().any(|d| d.token == token)
            || parsed.union_payload_labels.contains(&token)
            || parsed.module_name_tokens.contains(&token)
            || Self::selector_has_value_receiver(parsed, token)
    }

    // These are parser-certified occurrences in disjoint namespaces, not
    // failed semantic lookups. Record labels and puns deliberately stay in the
    // candidate domain even when they also introduce a local binding.
    pub(crate) fn record_field_role_disjoint(parsed: &ParsedFile, token: usize) -> bool {
        if parsed
            .record_expressions
            .iter()
            .any(|r| r.fields.contains(&token))
            || parsed
                .record_pattern_bindings
                .iter()
                .any(|p| p.field == token)
            || parsed
                .record_types
                .iter()
                .flat_map(|r| &r.fields)
                .any(|f| f.token == token)
        {
            return false;
        }
        parsed.explicit_constructor_heads.contains(&token)
            || parsed.import_name_tokens.contains(&token)
            || parsed.type_name_tokens.contains(&token)
            || parsed.type_uses.iter().any(|u| u.token == token)
            || parsed
                .type_aliases
                .iter()
                .any(|a| a.token == token || a.target_token == token)
            || parsed.record_types.iter().any(|r| r.token == token)
            || parsed.pattern_declarations.iter().any(|d| d.token == token)
            || parsed.declaration_at(token).is_some()
            || parsed.potential_bindings.iter().any(|d| d.token == token)
            || parsed.module_name_tokens.contains(&token)
            || parsed.union_payload_labels.contains(&token)
            || parsed.qualifier_roots.contains(&token)
    }

    pub(crate) fn member_cannot_reference(
        &mut self,
        symbol: &Symbol,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Result<bool> {
        if !symbol.member {
            return Ok(false);
        }
        if parsed.anonymous_record_labels.contains(&token) {
            return Ok(true);
        }
        if let Some(access) = parsed.member_accesses.iter().find(|m| m.token == token) {
            if parsed
                .local_binding(access.receiver)
                .is_some_and(|d| parsed.anonymous_record_bindings.contains(&d.token))
            {
                return Ok(true);
            }
            let known = if let Some(source) = &access.source {
                self.source_type(path, parsed, token, source, 1)?
            } else {
                self.value_type(path, parsed, access.receiver, 1)?
            };
            if let Some(identity) = known {
                let loaded = if identity.path == path {
                    None
                } else {
                    Some(self.parsed(&identity.path)?)
                };
                let owner = loaded.as_ref().unwrap_or(parsed);
                if let Some(record) = owner
                    .record_types
                    .iter()
                    .find(|r| r.token == identity.token)
                {
                    // Nominal records do not inherit fields from a different
                    // record declaration; an extension cannot change that ID.
                    if record.kind == "record"
                        && (identity.path != symbol.path || record.name != symbol.owner)
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    fn source_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        binding: usize,
        source: &TypeSource,
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        match source {
            TypeSource::Named { name, token } => {
                return self.named_type(path, parsed, *token, name, depth + 1)
            }
            TypeSource::Alias { token } => return self.value_type(path, parsed, *token, depth + 1),
            TypeSource::Record { fields, copy } => {
                if let Some(receiver) = copy {
                    return self.value_type(path, parsed, *receiver, depth + 1);
                }
                // Qualified record labels supply an actual nominal type;
                // a globally unique spelling by itself supplies no proof.
                for field in fields {
                    if let Some(qualifier) = parsed.qualifier(*field) {
                        return self.named_type(path, parsed, *field, &qualifier, depth + 1);
                    }
                }
                return Ok(self.local_record_construction(path, parsed, binding, fields));
            }
            TypeSource::FunctionResult {
                function,
                arguments,
            } => {
                let Some(callee) = self.resolve(path, parsed, *function)? else {
                    let name = parsed
                        .qualifier(*function)
                        .map(|p| format!("{p}.{}", parsed.tokens[*function].text))
                        .unwrap_or_else(|| parsed.tokens[*function].text.clone());
                    let Some(identity) =
                        self.named_type(path, parsed, *function, &name, depth + 1)?
                    else {
                        return Ok(None);
                    };
                    let loaded = if identity.path == path {
                        None
                    } else {
                        Some(self.parsed(&identity.path)?)
                    };
                    let owner = loaded.as_ref().unwrap_or(parsed);
                    return Ok(owner
                        .record_types
                        .iter()
                        .find(|r| r.token == identity.token && r.kind == "class")
                        .map(|_| identity));
                };
                let owner = if callee.path == path {
                    None
                } else {
                    Some(self.parsed(&callee.path)?)
                };
                let owner = owner.as_ref().unwrap_or(parsed);
                if *arguments == 1
                    && owner
                        .method_declarations
                        .iter()
                        .any(|m| m.token == callee.token && m.supported && m.is_static)
                {
                    return self.return_type(&callee.path, owner, callee.token, depth + 1);
                }
                if owner
                    .function_returns
                    .iter()
                    .any(|r| r.binding == callee.token && r.arity == *arguments)
                    || owner
                        .function_results
                        .iter()
                        .any(|r| r.binding == callee.token && r.arity == *arguments)
                {
                    return self.return_type(&callee.path, owner, callee.token, depth + 1);
                }
            }
        }
        Ok(None)
    }

    // A fresh value initializer has no receiver type to guess. F# intersects
    // the ordered label candidate lists and prefers a complete-field match.
    // A local record introduced after every active import dominates older
    // assembly/import candidates in every label list. We require one such
    // accessible complete local shape, and refuse ambiguous or recursive RHSs.
    fn local_record_construction(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        binding: usize,
        fields: &[usize],
    ) -> Option<TypeIdentity> {
        let first = *fields.first()?;
        let offset = parsed.tokens[first].start;
        let method_result = parsed
            .record_expressions
            .iter()
            .find(|r| r.method_result == Some(binding) && r.fields == fields);
        if method_result.is_some()
            || parsed
                .method_declarations
                .iter()
                .any(|m| m.token == binding)
        {
            if !self
                .fresh_method_result_proven(path, parsed, method_result?, 1)
                .ok()?
            {
                return None;
            }
        } else {
            let declaration = parsed.declaration_at(binding)?;
            if declaration.visible_start <= offset {
                return None;
            }
        }
        if !label_environment_supported(parsed, offset) {
            return None;
        }
        let imports = self.effective_imports(path, parsed, first).ok()?;
        let current = parsed.module_at(first).unwrap_or("");
        let labels = fields
            .iter()
            .map(|i| parsed.tokens[*i].text.as_str())
            .collect::<std::collections::HashSet<_>>();
        if labels.len() != fields.len() {
            return None;
        }
        let mut matches = Vec::new();
        for record in &parsed.record_types {
            let module = parsed.module_at(record.token).unwrap_or("");
            if record.kind != "record"
                || record.accessibility != "public"
                || record.start >= offset
                || offset >= record.scope_end
                || (module != current && !current.starts_with(&format!("{module}.")))
            {
                continue;
            }
            let all = record
                .fields
                .iter()
                .filter(|f| f.kind == "field")
                .collect::<Vec<_>>();
            if all.len() != labels.len()
                || !all
                    .iter()
                    .all(|f| f.accessibility == "public" && labels.contains(f.name.as_str()))
            {
                continue;
            }
            if imports
                .iter()
                .any(|o| o.offset > record.start && o.offset < offset && offset < o.scope_end)
            {
                return None;
            }
            match self.record_qualification(path, parsed, record).ok()? {
                Some(false) => matches.push(record),
                Some(true) => {}
                None => return None,
            }
        }
        let record = matches.into_iter().max_by_key(|r| r.start)?;
        if !record.type_parameters.is_empty() {
            return None;
        }
        Some(TypeIdentity {
            path: path.into(),
            token: record.token,
            arguments: Vec::new(),
        })
    }

    // Unlike an arbitrary receiver that our engine cannot type, this syntax
    // certificate proves F# reaches the field lookup with a fresh formal's
    // inference variable. A newly declared local label dominates older imports
    // in the compiler's ordered field-label environment.
    fn fresh_parameter_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        receiver: usize,
    ) -> Option<TypeIdentity> {
        let fact = parsed
            .unconstrained_members
            .iter()
            .find(|f| f.token == token)?;
        let binding = parsed.local_binding(receiver)?;
        if binding.token != fact.parameter {
            return None;
        }
        let offset = parsed.tokens[token].start;
        if !label_environment_supported(parsed, offset) {
            return None;
        }
        let current = parsed.module_at(token).unwrap_or("");
        let name = &parsed.tokens[token].text;
        let mut candidates = parsed
            .record_types
            .iter()
            .filter(|r| {
                let owner = parsed.module_at(r.token).unwrap_or("");
                r.kind == "record"
                    && r.accessibility == "public"
                    && r.start < offset
                    && offset < r.scope_end
                    && (owner == current || current.starts_with(&format!("{owner}.")))
                    && r.fields.iter().any(|f| {
                        f.kind == "field" && f.name == *name && f.accessibility == "public"
                    })
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|r| std::cmp::Reverse(r.start));
        for record in candidates {
            match self.record_qualification(path, parsed, record).ok()? {
                Some(true) => continue,
                Some(false) => {}
                None => return None,
            }
            if self
                .effective_imports_since(path, parsed, token, record.start)
                .ok()?
                .iter()
                .any(|o| o.offset > record.start && o.offset < offset && offset < o.scope_end)
            {
                return None;
            }
            return Some(TypeIdentity {
                path: path.into(),
                token: record.token,
                arguments: record
                    .type_parameters
                    .iter()
                    .map(|_| TypeArgument {
                        path: path.into(),
                        token,
                        name: "'_".into(),
                    })
                    .collect(),
            });
        }
        None
    }

    // The logical type namespace already merges ordered source and external
    // imports. Reuse that selection for receiver annotations; interpreting a
    // selected source declaration is separate from selecting its name.
    fn imported_source_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        args: &[String],
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        if !self.proof_step(depth)
            || !self.imported_type_domain_closed(
                path,
                parsed,
                token,
                name,
                args.len(),
                depth + 1,
            )?
        {
            return Ok(None);
        }
        let Some(symbol) =
            self.logical_source_type(path, parsed, token, name.to_owned(), args.len())?
        else {
            return Ok(None);
        };
        let loaded = if symbol.path == path {
            None
        } else {
            Some(self.parsed(&symbol.path)?)
        };
        let owner = loaded.as_ref().unwrap_or(parsed);
        if owner.record_types.iter().any(|r| r.token == symbol.token) {
            return self.instantiate_type(
                TypeIdentity {
                    path: symbol.path.clone(),
                    token: symbol.token,
                    arguments: Vec::new(),
                },
                path,
                token,
                args,
                parsed,
            );
        }
        let Some(alias) = owner.type_aliases.iter().find(|a| a.token == symbol.token) else {
            return Ok(None);
        };
        self.expand_alias(
            &symbol.path,
            owner,
            alias,
            path,
            parsed,
            token,
            args,
            depth + 1,
        )
    }

    // This closes only the newly admitted external-import path. Direct name
    // absence is insufficient when opening a namespace can activate nested
    // AutoOpen modules. Until their type precedence is represented, retain
    // Unknown for any applicable implicit contribution instead of skipping it.
    fn imported_type_domain_closed(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        arity: usize,
        depth: usize,
    ) -> Result<bool> {
        if name.contains('.') || !self.proof_step(depth) || !self.ensure_builtins()? {
            return Ok(false);
        }
        let Some(entries) = self
            .builtin_environment
            .as_ref()
            .unwrap()
            .type_names(&self.index.db, name)?
        else {
            return Ok(false);
        };
        let Some(assembly_opens) = self
            .builtin_environment
            .as_ref()
            .unwrap()
            .assembly_auto_opens()
        else {
            return Ok(false);
        };
        let mut implicit = std::collections::HashSet::new();
        for open in assembly_opens {
            let Some(scope) = open["modulePath"].as_str() else {
                return Ok(false);
            };
            implicit.insert(scope.to_owned());
        }
        let offset = parsed.tokens[token].start;
        let mut active = std::collections::HashSet::from([String::new()]);
        let mut current = parsed.module_at(token);
        while let Some(scope) = current {
            active.insert(scope.to_owned());
            current = scope.rsplit_once('.').map(|p| p.0);
        }
        for open in self
            .effective_imports(path, parsed, token)?
            .iter()
            .filter(|o| o.offset < offset && offset < o.scope_end)
        {
            if open.name.is_empty() {
                return Ok(false);
            }
            let source_scopes = self.opened_modules(path, parsed, token, &open.name)?;
            let first = open.name.split('.').next().unwrap_or(&open.name);
            if parsed
                .aliases
                .iter()
                .any(|a| a.name == first && a.offset < offset && offset < a.scope_end)
            {
                if source_scopes.is_empty() {
                    return Ok(false);
                }
                active.extend(source_scopes);
                continue;
            }
            let choices = Self::module_choices(parsed, token, &open.name, false);
            let simple = open.name.rsplit('.').next().unwrap_or(&open.name);
            let Some(rows) = self
                .builtin_environment
                .as_ref()
                .unwrap()
                .scope_names(&self.index.db, simple)?
            else {
                return Ok(false);
            };
            let external_scopes = rows
                .iter()
                .filter_map(|row| {
                    row["sourceFullName"]
                        .as_str()
                        .filter(|s| choices.iter().any(|c| c == s))
                })
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if source_scopes.is_empty() && external_scopes.is_empty() {
                return Ok(false);
            }
            active.extend(source_scopes);
            active.extend(external_scopes);
            if active.len() + implicit.len() > 2048 {
                return Ok(false);
            }
        }
        for entry in entries
            .iter()
            .filter(|e| e["genericArity"].as_u64() == Some(arity as u64))
        {
            let Some(full) = entry["sourceFullName"].as_str() else {
                return Ok(false);
            };
            let scope = full.rsplit_once('.').map(|p| p.0).unwrap_or("");
            if self.external_scope_is_open(scope, &implicit, depth + 1)? != Some(false)
                || (!active.contains(scope)
                    && self.external_scope_is_open(scope, &active, depth + 1)? != Some(false))
            {
                return Ok(false);
            }
        }
        let visible = self.visible_files(path);
        for candidate in self.type_candidate_paths(name)? {
            if !visible.contains(&candidate)
                || candidate != path
                    && candidate.ends_with(".fs")
                    && visible.contains(&format!("{candidate}i"))
            {
                continue;
            }
            let owner = self.parsed(&candidate)?;
            if owner.tokens.is_empty() && !owner.supported {
                return Ok(false);
            }
            for declaration in owner.owned_types.iter().filter(|t| {
                t.name.rsplit('.').next() == Some(name) && (candidate != path || t.start <= offset)
            }) {
                let scope = declaration.name.rsplit_once('.').map(|p| p.0).unwrap_or("");
                if !active.contains(scope)
                    && self.source_scope_is_open(&candidate, &owner, scope, &active, depth + 1)?
                        != Some(false)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn named_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        text: &str,
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        if !self.proof_step(depth) || !supported(parsed, token) {
            return Ok(None);
        }
        let Some((mut name, args)) = nominal_application(text) else {
            return Ok(None);
        };
        let global = name.starts_with("global.");
        if global {
            name = name[7..].into();
        }
        let first = name.split('.').next().unwrap_or("");
        if !global {
            if parsed.aliases.iter().rev().any(|a| {
                a.name == first
                    && a.offset < parsed.tokens[token].start
                    && parsed.tokens[token].start < a.scope_end
            }) {
                if let Some((qualifier, member)) = name.rsplit_once('.') {
                    for module in self.opened_modules(path, parsed, token, qualifier)? {
                        match self.find_named_type(
                            path,
                            parsed,
                            token,
                            &format!("{module}.{member}"),
                            &args,
                            depth + 1,
                        )? {
                            TypeLookup::Resolved(found) => return Ok(Some(found)),
                            TypeLookup::Unknown => return Ok(None),
                            TypeLookup::Absent => {}
                        }
                    }
                }
                return Ok(None);
            }
        }
        let choices = if global {
            vec![name.clone()]
        } else {
            Self::module_choices(parsed, token, &name, true)
        };
        let local_start = choices
            .iter()
            .find_map(|choice| {
                parsed
                    .owned_types
                    .iter()
                    .filter(|r| {
                        r.name == *choice
                            && r.start <= parsed.tokens[token].start
                            && parsed.tokens[token].start < r.end
                    })
                    .map(|r| r.start)
                    .max()
            })
            .unwrap_or(0);
        let imports = if !name.contains('.') && !global {
            self.effective_imports_since(path, parsed, token, local_start)?
        } else {
            parsed.opens.clone()
        };
        // Same-file declared types make the common annotated-local case require
        // no repository name scan. Later opens remain negative dependencies.
        for choice in &choices {
            if let Some(local) = parsed
                .owned_types
                .iter()
                .filter(|r| {
                    r.name == *choice
                        && r.start <= parsed.tokens[token].start
                        && parsed.tokens[token].start < r.end
                })
                .max_by_key(|r| r.start)
            {
                if !name.contains('.') {
                    for open in imports.iter().rev().filter(|o| {
                        o.offset > local.start
                            && o.offset < parsed.tokens[token].start
                            && parsed.tokens[token].start < o.scope_end
                    }) {
                        if open.name.is_empty() {
                            return Ok(None);
                        }
                        if open.name.is_empty() {
                            return Ok(None);
                        }
                        let opened = self.opened_modules(path, parsed, token, &open.name)?;
                        if opened.is_empty() {
                            return self.imported_source_type(
                                path,
                                parsed,
                                token,
                                &name,
                                &args,
                                depth + 1,
                            );
                        }
                        for module in opened {
                            match self.find_named_type(
                                path,
                                parsed,
                                token,
                                &format!("{module}.{name}"),
                                &args,
                                depth + 1,
                            )? {
                                TypeLookup::Resolved(found) => return Ok(Some(found)),
                                TypeLookup::Unknown => return Ok(None),
                                TypeLookup::Absent => {}
                            }
                        }
                    }
                }
                if let Some(record) = parsed
                    .record_types
                    .iter()
                    .filter(|r| r.name == *choice && r.start <= parsed.tokens[token].start)
                    .max_by_key(|r| r.start)
                {
                    return self.instantiate_type(
                        TypeIdentity {
                            path: path.into(),
                            token: record.token,
                            arguments: Vec::new(),
                        },
                        path,
                        token,
                        &args,
                        parsed,
                    );
                }
                if let Some(alias) = parsed
                    .type_aliases
                    .iter()
                    .filter(|r| r.name == *choice && r.start <= parsed.tokens[token].start)
                    .max_by_key(|r| r.start)
                {
                    if !self.nominal_accessible(
                        path,
                        parsed,
                        alias.token,
                        &alias.accessibility,
                        path,
                        parsed,
                        token,
                    ) {
                        return Ok(None);
                    }
                    return self.expand_alias(
                        path,
                        parsed,
                        alias,
                        path,
                        parsed,
                        token,
                        &args,
                        depth + 1,
                    );
                }
                // A nearer union, generic alias, or otherwise unsupported type
                // shadows outer types even when it has no member summary.
                return Ok(None);
            }
        }
        if !self.context_exact(path) || self.ambiguous_instance(path) {
            return Ok(None);
        }
        if !name.contains('.') && !global {
            // Namespace types from earlier files entered scope before opens in
            // this file. F# merges ordered module/namespace candidates for an
            // open; resolve that ordered environment independently of the type.
            for open in imports.iter().rev().filter(|o| {
                o.offset < parsed.tokens[token].start && parsed.tokens[token].start < o.scope_end
            }) {
                if open.name.is_empty() {
                    return Ok(None);
                }
                let opened = self.opened_modules(path, parsed, token, &open.name)?;
                if opened.is_empty() {
                    return self.imported_source_type(path, parsed, token, &name, &args, depth + 1);
                }
                for module in opened {
                    match self.find_named_type(
                        path,
                        parsed,
                        token,
                        &format!("{module}.{name}"),
                        &args,
                        depth + 1,
                    )? {
                        TypeLookup::Resolved(found) => return Ok(Some(found)),
                        TypeLookup::Unknown => return Ok(None),
                        TypeLookup::Absent => {}
                    }
                }
            }
        }
        for choice in choices {
            match self.find_named_type(path, parsed, token, &choice, &args, depth + 1)? {
                TypeLookup::Resolved(found) => return Ok(Some(found)),
                TypeLookup::Unknown => return Ok(None),
                TypeLookup::Absent => {}
            }
        }
        Ok(None)
    }

    pub(crate) fn opened_modules(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
    ) -> Result<Vec<String>> {
        self.opened_modules_depth(path, parsed, token, name, 0)
    }
    fn opened_modules_depth(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        depth: usize,
    ) -> Result<Vec<String>> {
        if !self.proof_step(depth) {
            return Ok(Vec::new());
        }
        let first = name.split('.').next().unwrap_or(name);
        let offset = parsed.tokens[token].start;
        let aliases = parsed
            .aliases
            .iter()
            .rev()
            .filter(|a| a.name == first && a.offset < offset && offset < a.scope_end)
            .collect::<Vec<_>>();
        if !aliases.is_empty() {
            let mut result = Vec::new();
            // F# captures module identities where an abbreviation is declared.
            // Repeated abbreviations prepend rather than erase old members.
            for alias in aliases {
                let at = parsed
                    .tokens
                    .iter()
                    .position(|t| t.start >= alias.offset)
                    .unwrap_or(token);
                let target = format!("{}{}", alias.target, &name[first.len()..]);
                if at == token && target == name {
                    return Ok(Vec::new());
                }
                for module in self.opened_modules_depth(path, parsed, at, &target, depth + 1)? {
                    if !result.contains(&module) {
                        result.push(module);
                    }
                }
            }
            return Ok(result);
        }
        let simple = name.rsplit('.').next().unwrap_or(name);
        let visible = self.visible_files(path);
        let facts = self.module_facts(simple)?;
        let mut result = Vec::new();
        for choice in Self::module_choices(parsed, token, name, false) {
            let mut exists = false;
            for (candidate, module, valid) in &facts {
                self.cancellation.check()?;
                if !visible.contains(candidate) {
                    continue;
                }
                if module.name.is_empty() && !valid {
                    return Ok(Vec::new());
                }
                if module.name != choice || (candidate == path && module.start > offset) {
                    continue;
                }
                if !valid {
                    return Ok(Vec::new());
                }
                exists = true;
            }
            if exists && !result.contains(&choice) {
                result.push(choice);
            }
        }
        Ok(result)
    }

    fn find_named_type(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        args: &[String],
        depth: usize,
    ) -> Result<TypeLookup> {
        if depth >= MAX_PROOF_DEPTH {
            return Ok(TypeLookup::Unknown);
        }
        let simple = name.rsplit('.').next().unwrap_or(name);
        let visible = self.visible_files(path);
        let mut found = Vec::new();
        for candidate in self.type_candidate_paths(simple)? {
            self.cancellation.check()?;
            if !visible.contains(&candidate) {
                continue;
            }
            if self.ambiguous_instance(&candidate) {
                return Ok(TypeLookup::Unknown);
            }
            if candidate != path
                && candidate.ends_with(".fs")
                && visible.contains(&format!("{candidate}i"))
            {
                continue;
            }
            let owner = self.parsed(&candidate)?;
            if owner.tokens.is_empty() && !owner.supported {
                return Ok(TypeLookup::Unknown);
            }
            let owns = owner.owned_types.iter().any(|t| {
                t.name == name && (candidate != path || t.start <= parsed.tokens[token].start)
            });
            let has_record = owner.record_types.iter().any(|r| r.name == name);
            if owns && !has_record {
                if let Some(alias) = owner.type_aliases.iter().find(|r| r.name == name) {
                    if !self.nominal_accessible(
                        &candidate,
                        &owner,
                        alias.token,
                        &alias.accessibility,
                        path,
                        parsed,
                        token,
                    ) {
                        continue;
                    }
                    if let Some(identity) = self.expand_alias(
                        &candidate,
                        &owner,
                        alias,
                        path,
                        parsed,
                        token,
                        args,
                        depth + 1,
                    )? {
                        found.push(identity);
                    } else {
                        return Ok(TypeLookup::Unknown);
                    }
                } else {
                    return Ok(TypeLookup::Unknown);
                }
            }
            for record in owner.record_types.iter().filter(|r| r.name == name) {
                if candidate == path && record.start > parsed.tokens[token].start {
                    continue;
                }
                if !supported(&owner, record.token) {
                    return Ok(TypeLookup::Unknown);
                }
                if self.type_accessible(
                    &candidate,
                    &owner,
                    record,
                    path,
                    parsed,
                    token,
                    &record.accessibility,
                ) {
                    if let Some(identity) = self.instantiate_type(
                        TypeIdentity {
                            path: candidate.clone(),
                            token: record.token,
                            arguments: Vec::new(),
                        },
                        path,
                        token,
                        args,
                        parsed,
                    )? {
                        found.push(identity);
                    } else {
                        return Ok(TypeLookup::Unknown);
                    }
                }
            }
        }
        if found.len() == 1 {
            return Ok(TypeLookup::Resolved(found.pop().unwrap()));
        }
        if !found.is_empty() {
            return Ok(TypeLookup::Unknown);
        }
        Ok(match self.external_type_candidates(name, args.len())? {
            Lookup::Absent => TypeLookup::Absent,
            _ => TypeLookup::Unknown,
        })
    }

    fn instantiate_type(
        &mut self,
        mut identity: TypeIdentity,
        path: &str,
        token: usize,
        args: &[String],
        parsed: &ParsedFile,
    ) -> Result<Option<TypeIdentity>> {
        let loaded = if identity.path == path {
            None
        } else {
            Some(self.parsed(&identity.path)?)
        };
        let owner = loaded.as_ref().unwrap_or(parsed);
        let Some(record) = owner
            .record_types
            .iter()
            .find(|r| r.token == identity.token)
        else {
            return Ok(None);
        };
        if record.type_parameters.len() != args.len() {
            return Ok(None);
        }
        identity.arguments = args
            .iter()
            .map(|name| TypeArgument {
                path: path.into(),
                token,
                name: name.clone(),
            })
            .collect();
        Ok(Some(identity))
    }
    fn expand_alias(
        &mut self,
        owner_path: &str,
        owner: &ParsedFile,
        alias: &TypeAlias,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        args: &[String],
        depth: usize,
    ) -> Result<Option<TypeIdentity>> {
        if alias.type_parameters.len() != args.len() || !self.proof_step(depth) {
            return Ok(None);
        }
        if let Some(parameter) = alias
            .type_parameters
            .iter()
            .position(|p| p == alias.target.trim())
        {
            return self.named_type(path, parsed, token, &args[parameter], depth + 1);
        }
        let Some(mut target) = self.named_type(
            owner_path,
            owner,
            alias.target_token,
            &alias.target,
            depth + 1,
        )?
        else {
            return Ok(None);
        };
        for argument in &mut target.arguments {
            if let Some(parameter) = alias
                .type_parameters
                .iter()
                .position(|p| p == argument.name.trim())
            {
                *argument = TypeArgument {
                    path: path.into(),
                    token,
                    name: args[parameter].clone(),
                };
            }
        }
        Ok(Some(target))
    }

    fn type_accessible(
        &self,
        owner_path: &str,
        owner: &ParsedFile,
        record: &RecordType,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        access: &str,
    ) -> bool {
        self.nominal_accessible(
            owner_path,
            owner,
            record.token,
            &record.accessibility,
            path,
            parsed,
            token,
        ) && self.nominal_accessible(owner_path, owner, record.token, access, path, parsed, token)
    }
    fn nominal_accessible(
        &self,
        owner_path: &str,
        owner: &ParsedFile,
        owner_token: usize,
        access: &str,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> bool {
        if access == "private" {
            let module = owner.module_at(owner_token).unwrap_or("");
            let current = parsed.module_at(token).unwrap_or("");
            if owner_path != path
                || !(module == current || current.starts_with(&format!("{module}.")))
            {
                return false;
            }
        }
        if access == "internal" {
            if !self
                .model
                .project_for(path)
                .zip(self.model.project_for(owner_path))
                .is_some_and(|(a, b)| a.path == b.path)
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::{fs, path::PathBuf};

    fn workspace(source: &str) -> (PathBuf, Workspace) {
        let path = std::env::temp_dir().join(format!(
            "navigator-type-proof-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("Main.fs"), source).unwrap();
        fs::write(path.join("Main.fsproj"),r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
        let ws = Workspace::open(&json!({"root":path})).unwrap();
        (path, ws)
    }
    fn query(
        ws: &mut Workspace,
        path: &PathBuf,
        method: &str,
        line: usize,
        column: usize,
    ) -> Value {
        ws.dispatch(
            method,
            &json!({"path":path.join("Main.fs"),"line":line,"character":column}),
        )
        .unwrap()
    }
    fn certify_type_names(path: &PathBuf, ws: &mut Workspace, names: Value) {
        use sha2::{Digest, Sha256};
        ws.ensure_model().unwrap();
        let assembly = path.join("Certificate.dll");
        fs::write(&assembly, b"closed fixture reference domain").unwrap();
        for project in &mut ws.model.projects {
            project.evaluation = "design-time-fsc-arguments".into();
            project.reference_bindings =
                vec![json!({"path":assembly,"identity":"Certificate","isWorkspace":false})];
        }
        let environment = path.join("type-inventory.json");
        fs::write(&environment,crate::certified_metadata_fixture(json!({"schemaVersion":1,"factsVersion":2,"kind":"builtin-environment","completeness":"exact","typeNameInventory":"all-external-type-declarations","scopeNameInventory":"all-external-namespace-and-module-declarations","termNameInventory":"all-external-expression-bindings","scopeNames":{},"termNames":{},"assemblyAutoOpens":[],"certificateScope":"selected-reference-declaration-inventory","inputHashes":[{"path":assembly,"sha256":format!("{:x}",Sha256::digest(fs::read(&assembly).unwrap()))}],"referenceBindings":[{"path":assembly,"identity":"Certificate"}],"workspaceAssemblies":[],"types":{},"patterns":{},"typeNames":names})).to_string()).unwrap();
        ws.builtin_path = Some(environment.to_string_lossy().into_owned());
    }
    #[test]
    fn annotated_receiver_fields_have_distinct_identities_and_dirty_refs() {
        let source="module Main\ntype Person = { Name: string }\ntype Company = { Name: string }\nlet read (x: Person) = x.Name\nlet other (x: Company) = x.Name\n";
        let (path, mut ws) = workspace(source);
        let field_col = source.lines().nth(3).unwrap().rfind("Name").unwrap();
        let def = query(&mut ws, &path, "navigation/definition", 3, field_col);
        assert_eq!(def["completeness"], "exact", "{def}");
        assert_eq!(def["locations"][0]["range"]["start"]["line"], 1);
        assert_eq!(
            ws.index.counters.files_parsed, 1,
            "Annotated same-file definition should parse only its target file"
        );
        let refs = query(&mut ws, &path, "navigation/references", 1, 16);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 1, "{refs}");
        assert_eq!(refs["locations"][0]["range"]["start"]["line"], 3);
        let edited = source.replace("read (x: Person)", "read (x: Company)");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":edited,"version":1}),
        )
        .unwrap();
        let def = query(
            &mut ws,
            &path,
            "navigation/definition",
            3,
            edited.lines().nth(3).unwrap().rfind("Name").unwrap(),
        );
        assert_eq!(def["completeness"], "exact", "{def}");
        assert_eq!(def["locations"][0]["range"]["start"]["line"], 2);
        let refs = query(&mut ws, &path, "navigation/references", 1, 16);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 0, "{refs}");
        let refs = query(&mut ws, &path, "navigation/references", 2, 17);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn alias_update_qualified_construction_and_declared_return_proofs() {
        let source="module Main\ntype Person = { Name: string }\nlet original : Person = { Name = \"first\" }\nlet alias = original\nlet updated = { alias with Name = \"second\" }\nlet qualified = { Person.Name = \"third\" }\nlet make () : Person = original\nlet returned = make ()\nlet values = alias.Name, updated.Name, qualified.Name, returned.Name\n";
        let (path, mut ws) = workspace(source);
        let line = source.lines().nth(8).unwrap();
        for (column, _) in line.match_indices("Name") {
            let def = query(&mut ws, &path, "navigation/definition", 8, column);
            assert_eq!(def["completeness"], "exact", "column {column}: {def}");
            assert_eq!(def["locations"][0]["range"]["start"]["line"], 1);
        }
        let refs = query(&mut ws, &path, "navigation/references", 1, 16);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 7, "{refs}");
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn unique_source_field_does_not_guess_unknown_library_receiver() {
        let source="module Main\ntype Record = { Length: int }\nlet text = System.String.Empty\nlet value = text.Length\n";
        let (path, mut ws) = workspace(source);
        let def = query(&mut ws, &path, "navigation/definition", 3, 17);
        assert_eq!(def["completeness"], "unavailable", "{def}");
        let refs = query(&mut ws, &path, "navigation/references", 1, 16);
        assert_eq!(
            refs["completeness"], "unavailable",
            "Unknown Length receiver must prevent a complete answer: {refs}"
        );
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn function_return_constructor_labels_and_nested_fields_are_references() {
        let source="module Main\ntype Address = { City: string }\ntype Person = { Home: Address }\nlet make () : Person = { Home = { Address.City = \"x\" } }\nlet nested (person: Person) = person.Home.City\n";
        let (path, mut ws) = workspace(source);
        let def = query(
            &mut ws,
            &path,
            "navigation/definition",
            4,
            source.lines().nth(4).unwrap().rfind("City").unwrap(),
        );
        assert_eq!(def["completeness"], "exact", "{def}");
        assert_eq!(def["locations"][0]["range"]["start"]["line"], 1);
        let refs = query(&mut ws, &path, "navigation/references", 2, 16);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        let refs = query(&mut ws, &path, "navigation/references", 1, 17);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn record_field_references_exclude_import_and_type_path_components() {
        let source = "module Main\nmodule Artifact =\n    type ArtifactEvent = Event of int\nopen Artifact\ntype Holder = { Artifact:int }\ntype Envelope = Wrapped of Artifact.ArtifactEvent\nlet read (holder:Holder) = holder.Artifact\nlet label (holder:Holder) = nameof holder.Artifact\n";
        let (path, mut ws) = workspace(source);
        let refs = ws.dispatch("navigation/references", &json!({"path":path.join("Main.fs"),"line":4,"character":16,"includeDeclaration":true,"traceReferences":true})).unwrap();
        assert_eq!(refs["completeness"], "exact", "{refs}");
        let mut lines = refs["locations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|location| location["range"]["start"]["line"].as_u64().unwrap())
            .collect::<Vec<_>>();
        lines.sort();
        assert_eq!(
            lines,
            [4, 6, 7],
            "Import/type namespaces exclude fields; nameof receiver remains a field use: {refs}"
        );
        let parsed = crate::treesitter::parse(source);
        for line in [4, 6, 7] {
            let token = parsed
                .tokens
                .iter()
                .position(|t| t.line == line && t.text == "Artifact")
                .unwrap();
            assert!(!Workspace::record_field_role_disjoint(&parsed, token));
        }
        // FCS can still attach the instance field in this semantically invalid
        // nameof expression. A nominal-looking qualifier is not type syntax.
        let invalid_nameof = crate::treesitter::parse(
            "module Main\ntype Holder={Artifact:int}\nlet name=nameof Holder.Artifact\n",
        );
        let field = invalid_nameof
            .tokens
            .iter()
            .position(|t| t.line == 2 && t.text == "Artifact")
            .unwrap();
        assert!(!Workspace::record_field_role_disjoint(
            &invalid_nameof,
            field
        ));
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn later_open_unsupported_type_blocks_an_earlier_record() {
        let source="module Main\ntype Person = { Name: string }\nmodule Other =\n    type Person = Kind of string\nopen Other\nlet usePerson (person: Person) = person.Name\n";
        let (path, mut ws) = workspace(source);
        let def = query(
            &mut ws,
            &path,
            "navigation/definition",
            5,
            source.lines().nth(5).unwrap().rfind("Name").unwrap(),
        );
        assert_eq!(def["completeness"], "unavailable", "{def}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn local_complete_record_shape_and_simple_function_result() {
        let source="module Main\ntype Customer = { Name:string; Home:int }\ntype Supplier = { Name:string; Code:int }\nlet customer = { Name=\"x\"; Home=1 }\nlet make name = { Name=name; Home=2 }\nlet returned = make \"y\"\nlet result = customer.Name, returned.Name\n";
        let (path, mut ws) = workspace(source);
        for (col, _) in source.lines().nth(6).unwrap().match_indices("Name") {
            let def = query(&mut ws, &path, "navigation/definition", 6, col);
            assert_eq!(def["completeness"], "exact", "{def}");
            assert_eq!(def["locations"][0]["range"]["start"]["line"], 1);
        }
        let refs = query(&mut ws, &path, "navigation/references", 1, 18);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 4, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn logical_alias_definition_preserves_open_order_and_external_negative_dependencies() {
        use sha2::{Digest, Sha256};
        let source="namespace Shared\nmodule Use =\n    open First\n    open Second\n    type Selected = Label\n    let make = Label \"x\"\n";
        let (path, mut ws) = workspace(source);
        fs::write(path.join("Types.fs"),"namespace Shared\nmodule First =\n    type Label = string\nmodule Second =\n    type Label = string\n").unwrap();
        fs::write(path.join("Main.fsproj"),r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><Compile Include="Types.fs"/><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
        ws.ensure_model().unwrap();
        let assembly = path.join("External.dll");
        fs::write(&assembly, b"certificate input").unwrap();
        ws.model.projects[0].evaluation = "design-time-fsc-arguments".into();
        ws.model.projects[0].reference_bindings =
            vec![json!({"path":assembly,"identity":"External","isWorkspace":false})];
        let environment_path = path.join("environment.json");
        let environment = crate::certified_metadata_fixture(
            json!({"schemaVersion":1,"factsVersion":2,"kind":"builtin-environment","completeness":"exact","typeNameInventory":"all-external-type-declarations","certificateScope":"selected-reference-declaration-inventory","inputHashes":[{"path":assembly,"sha256":format!("{:x}",Sha256::digest(fs::read(&assembly).unwrap()))}],"referenceBindings":[{"path":assembly,"identity":"External"}],"workspaceAssemblies":[],"types":{},"patterns":{},"typeNames":{"Label":[{"identity":{"assemblyPath":assembly,"assemblyName":"External","fullName":"External.Label"},"sourceFullName":"External.Label","genericArity":0,"isAbbreviation":true,"isNominal":false}]}}),
        );
        fs::write(&environment_path, environment.to_string()).unwrap();
        ws.builtin_path = Some(environment_path.to_string_lossy().into_owned());
        let result = query(&mut ws, &path, "navigation/definition", 4, 20);
        assert_eq!(result["completeness"], "exact", "{result}");
        assert!(result["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("Types.fs"));
        assert_eq!(
            result["locations"][0]["range"]["start"]["line"], 4,
            "Must retain the selected logical alias, not expand it to string: {result}"
        );
        let constructor = query(&mut ws, &path, "navigation/definition", 5, 15);
        assert_eq!(
            constructor["completeness"], "unavailable",
            "A constructor target is not the logical alias declaration: {constructor}"
        );
        let refs = query(&mut ws, &path, "navigation/references", 4, 20);
        assert_eq!(
            refs["completeness"], "unavailable",
            "Constructor occurrence identity is not silently omitted: {refs}"
        );
        let annotated = source.replace(
            "let make = Label \"x\"",
            "let read (value:Label option)=value",
        );
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":annotated,"version":1}),
        )
        .unwrap();
        let refs=ws.dispatch("navigation/references",&json!({"path":path.join("Main.fs"),"line":4,"character":20,"includeDeclaration":true})).unwrap();
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(
            refs["locations"].as_array().unwrap().len(),
            3,
            "Logical alias uses include postfix type arguments: {refs}"
        );
        let swapped = source.replace("open First\n    open Second", "open Second\n    open First");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":swapped,"version":2}),
        )
        .unwrap();
        let result = query(&mut ws, &path, "navigation/definition", 4, 20);
        assert_eq!(result["completeness"], "exact", "{result}");
        assert_eq!(result["locations"][0]["range"]["start"]["line"], 2);
        let external = source.replace("open Second", "open External");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":external,"version":3}),
        )
        .unwrap();
        let result = query(&mut ws, &path, "navigation/definition", 4, 20);
        assert_eq!(result["completeness"],"unavailable","A known external declaration blocks the earlier source alias even without its catalog: {result}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn alias_constructor_references_keep_logical_identity_without_rewriting_definition() {
        let source = "module Main\ntype Sha=string\nlet typed:Sha=\"x\"\nlet made=Sha \"y\"\n";
        let (path, mut ws) = workspace(source);
        certify_type_names(&path, &mut ws, json!({}));
        for (line, column) in [(1, 5), (3, 9)] {
            let refs = query(&mut ws, &path, "navigation/references", line, column);
            assert_eq!(refs["completeness"], "exact", "{refs}");
            assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        }
        let definition = query(&mut ws, &path, "navigation/definition", 3, 9);
        assert_eq!(
            definition["completeness"], "unavailable",
            "Constructor goto is not alias-declaration goto: {definition}"
        );
        let changed = source.replace("let made", "let Sha value=value\nlet made");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":changed,"version":1}),
        )
        .unwrap();
        let refs = query(&mut ws, &path, "navigation/references", 1, 5);
        assert_eq!(
            refs["completeness"], "unavailable",
            "A competing source value cannot be absorbed into the alias group: {refs}"
        );
        assert_eq!(refs["locations"].as_array().unwrap().len(), 1, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn intrinsic_nameof_alias_reference_requires_captured_core_identity_and_no_shadow() {
        let source = "module Main\ntype Sha=string\nlet made=Sha \"y\"\nlet name=nameof Sha\n";
        let (path, mut ws) = workspace(source);
        fs::write(
            path.join("Other.fs"),
            "module Other\nlet (>>=) value f = f value\nlet run taskValue =\n    task {\n        let! item = taskValue\n        return item\n    }\nlet name = nameof int\n",
        )
        .unwrap();
        fs::write(path.join("Main.fsproj"), r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><Compile Include="Other.fs"/><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
        certify_type_names(&path, &mut ws, json!({}));
        let env_path = path.join("type-inventory.json");
        let mut env: Value = serde_json::from_slice(&fs::read(&env_path).unwrap()).unwrap();
        let assembly = path.join("Certificate.dll");
        env["types"]["string"] = json!([{"identity":{"assemblyPath":assembly,"assemblyName":"Certificate","fullName":"Microsoft.FSharp.Core.string"},"genericArity":0,"isBuiltin":true}]);
        env["termNames"]["nameof"] = json!([{"kind":"value","sourceFullName":"Microsoft.FSharp.Core.Operators.nameof","declaringScope":"Microsoft.FSharp.Core.Operators","enclosingScope":"Microsoft.FSharp.Core","identity":{"assemblyPath":assembly,"assemblyName":"Certificate","fullName":"Microsoft.FSharp.Core.Operators.NameOf"},"xmlDocId":"M:Microsoft.FSharp.Core.Operators.NameOf``1(``0)"}]);
        env["scopeNames"]["Operators"] = json!([{"kind":"module","sourceFullName":"Microsoft.FSharp.Core.Operators","enclosingScope":"Microsoft.FSharp.Core","autoOpen":true,"qualifiedOnly":false}]);
        env["assemblyAutoOpens"] = json!([{"modulePath":"Microsoft.FSharp.Core"}]);
        fs::write(&env_path, env.to_string()).unwrap();
        let refs = query(&mut ws, &path, "navigation/references", 1, 5);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        let definition = query(&mut ws, &path, "navigation/definition", 3, 16);
        assert_eq!(definition["completeness"], "exact", "{definition}");
        assert_eq!(
            definition["locations"][0]["range"]["start"]["line"], 1,
            "{definition}"
        );
        let changed = source.replace("let name=", "let nameof x=\"custom\"\nlet name=");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":changed,"version":1}),
        )
        .unwrap();
        let refs = query(&mut ws, &path, "navigation/references", 1, 5);
        assert_eq!(
            refs["completeness"], "unavailable",
            "Spelling cannot certify a shadowed nameof: {refs}"
        );
        assert_eq!(refs["locations"].as_array().unwrap().len(), 1, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn body_name_scope_does_not_support_in_body_inference_or_headers() {
        let source = "module Main\nlet work taskValue =\n    task {\n        let! local = taskValue\n        return local\n    }\nlet after = 1\n";
        let parsed = crate::treesitter::parse(source);
        let span = parsed
            .uncertain
            .iter()
            .find(|u| u.reason.contains("Computation-expression"))
            .unwrap();
        let [start, end] = span
            .local_binding_scope
            .expect("Complete function RHS bounds");
        assert!(span.name.is_none(), "General uncertainty remains unnamed");
        assert!(uncertainty_can_bind_name(
            span, "Main.fs", "Main.fs", span.start, "nameof"
        ));
        assert!(!uncertainty_can_bind_name(
            span, "Main.fs", "Main.fs", end, "nameof"
        ));
        assert!(!uncertainty_can_bind_name(
            span, "Main.fs", "Other.fs", start, "nameof"
        ));
        let local = parsed
            .tokens
            .iter()
            .position(|t| t.text == "local")
            .unwrap();
        assert!(
            !supported(&parsed, local),
            "In-body semantic uncertainty remains a barrier"
        );
        for source in [
            source.replace("let work", "let rec work"),
            source.replace("module Main", "module rec Main"),
            source.replace("let work", "[<System.Obsolete>]\nlet work"),
            source.replace("let work taskValue", "let work"),
            source.replace("return local", "return ("),
        ] {
            let p = crate::treesitter::parse(&source);
            assert!(
                p.uncertain.iter().all(|u| u.local_binding_scope.is_none()),
                "Uncertified boundary must remain unknown: {source}"
            );
        }
        let header = source
            .replace("taskValue =", "Unknown =")
            .replace("= taskValue", "= Unknown");
        let p = crate::treesitter::parse(&header);
        assert!(p
            .uncertain
            .iter()
            .any(|u| u.name.as_deref() == Some("Unknown")));
        assert!(
            p.uncertain
                .iter()
                .filter(|u| u.name.as_deref() == Some("Unknown"))
                .all(|u| u.local_binding_scope.is_none()),
            "Pattern-header uncertainty is not a body fact"
        );
    }

    #[test]
    fn instance_selectors_are_disjoint_from_aliases_but_new_and_type_syntax_are_not() {
        let source="module Main\nmodule n=\n    type Alias=System.Text.StringBuilder\ntype Holder={Alias:int}\nlet read (n:Holder)=n.Alias, new n.Alias(), typeof<n.Alias>\n";
        let parsed = crate::treesitter::parse(source);
        let tokens = parsed
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.line == 4 && t.text == "Alias")
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(tokens.len(), 3);
        assert!(Workspace::type_reference_role_disjoint(&parsed, tokens[0]));
        assert!(
            !Workspace::type_reference_role_disjoint(&parsed, tokens[1]),
            "Explicit new binds its head in the type namespace"
        );
        assert!(
            !Workspace::type_reference_role_disjoint(&parsed, tokens[2]),
            "Type syntax ignores term binders"
        );
        let (path, mut ws) = workspace(source);
        let field_refs = query(&mut ws, &path, "navigation/references", 3, 13);
        assert_eq!(
            field_refs["completeness"], "exact",
            "Explicit type syntax must not join the record field group: {field_refs}"
        );
        assert_eq!(
            field_refs["locations"].as_array().unwrap().len(),
            1,
            "{field_refs}"
        );
        let column = source.lines().nth(4).unwrap().find("new n.Alias").unwrap() + 6;
        let constructor = query(&mut ws, &path, "navigation/definition", 4, column);
        assert_eq!(
            constructor["completeness"], "unavailable",
            "A type constructor cannot fall through to the local receiver member: {constructor}"
        );
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn alias_rhs_candidate_is_not_a_competing_constructor_case() {
        let source = "module Main\ntype Sha=string\ntype Other=Sha\nlet made=Sha \"y\"\n";
        let (path, mut ws) = workspace(source);
        certify_type_names(&path, &mut ws, json!({}));
        let refs = query(&mut ws, &path, "navigation/references", 1, 5);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn alias_references_exclude_ordinary_record_label_roles() {
        let source="module Main\ntype Sha=string\ntype Other={Sha:int}\nlet made=Sha \"y\"\nlet ignored=ignore {Sha=1}\n";
        let (path, mut ws) = workspace(source);
        certify_type_names(&path, &mut ws, json!({}));
        let refs = query(&mut ws, &path, "navigation/references", 1, 5);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 1, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn later_alias_blocks_earlier_value_definition_at_constructor_and_nameof() {
        let source="module Main\nlet Sha (x:string)=x\ntype Sha=string\nlet made:Sha=Sha \"x\"\nlet name=nameof Sha\n";
        let (path, mut ws) = workspace(source);
        for line in [3, 4] {
            let column = source.lines().nth(line).unwrap().rfind("Sha").unwrap();
            let definition = query(&mut ws, &path, "navigation/definition", line, column);
            assert_eq!(
                definition["completeness"], "unavailable",
                "Earlier lexical value is not a proven winner over the later alias: {definition}"
            );
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn record_field_references_exclude_disjoint_type_and_root_roles() {
        let source="module Main\ntype Name=string\ntype Person={Name:string}\nlet typed:Name=\"x\"\nmodule Name=\n    let Empty=\"\"\nlet other=Name.Empty\nlet read (p:Person)=p.Name\n";
        let (path, mut ws) = workspace(source);
        let refs = query(&mut ws, &path, "navigation/references", 2, 13);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 1, "{refs}");
        assert_eq!(refs["locations"][0]["range"]["start"]["line"], 7, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn distinct_case_identity_does_not_close_an_opaque_factory_body() {
        let source = "module Main\ntype First={Name:string}\ntype Second={Name:string}\ntype Kind=Create=0\ntype Factory =\n    static member Create () =\n        let kind = Kind.Create\n        {Name=\"x\"}\n";
        let (path, mut ws) = workspace(source);
        certify_type_names(&path, &mut ws, json!({}));
        let result = query(&mut ws, &path, "navigation/definition", 7, 9);
        assert_eq!(
            result["completeness"], "unavailable",
            "A distinct case alone does not close all group constraints: {result}"
        );
        for (version, changed) in [
            (
                1,
                source.replace(
                    "type Factory =",
                    "module Kind =\n    let Create=0\ntype Factory =",
                ),
            ),
            (
                2,
                source.replace(
                    "type Factory =",
                    "type Factory =\n    static member Force () : First = Factory.Create()",
                ),
            ),
            (
                3,
                source.replace("let kind = Kind.Create", "let kind = Factory.Create()"),
            ),
        ] {
            ws.dispatch(
                "document/change",
                &json!({"path":path.join("Main.fs"),"text":changed,"version":version}),
            )
            .unwrap();
            let line = changed
                .lines()
                .position(|line| line.contains("{Name="))
                .unwrap();
            let result = query(&mut ws, &path, "navigation/definition", line, 9);
            assert_eq!(
                result["completeness"], "unavailable",
                "Opaque bodies and sibling constraints remain unproved: {result}"
            );
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn factory_signature_absence_requires_the_project_domain_not_a_neighbor_filename() {
        let source = "module Main\ntype First={Name:string}\ntype Second={Name:string}\ntype Factory =\n    static member Empty = {Name=\"x\"}\n";
        let (path, ws) = workspace(source);
        drop(ws);
        fs::write(path.join("Signature.fsi"), "module Main\ntype First={Name:string}\ntype Second={Name:string}\ntype Factory=\n    static member Empty:Second\n").unwrap();
        fs::write(path.join("Main.fsproj"), "<Project Sdk=\"Microsoft.NET.Sdk\"><ItemGroup><Compile Include=\"Signature.fsi\"/><Compile Include=\"Main.fs\"/></ItemGroup></Project>").unwrap();
        let mut ws = Workspace::open(&json!({"root":path})).unwrap();
        let result = query(&mut ws, &path, "navigation/definition", 4, 27);
        assert_eq!(
            result["completeness"], "unavailable",
            "Unmapped included signatures can contribute factory constraints: {result}"
        );
        let explicit = source.replace("Empty =", "Empty : Second =");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":explicit,"version":1}),
        )
        .unwrap();
        let result = query(&mut ws, &path, "navigation/definition", 4, 36);
        assert_eq!(
            result["completeness"], "exact",
            "Explicit return evidence is independent of signature absence: {result}"
        );
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn factory_literal_group_gate_blocks_hidden_trait_constraints_and_dirty_cached_answers() {
        let prefix = "module Main\ntype First={Name:string}\nmodule Helpers =\n    let inline force (x:^T) : First = ((^T):(static member Empty:First)())\n    let inline explicit< ^T when ^T:(static member Empty:First)> () = ((^T):(static member Empty:First)())\ntype Second={Name:string}\ntype Factory =\n    {Tag:int}\n";
        let plain = format!("{prefix}    static member Empty =\n        {{Name=\"x\"}}\n");
        let (path, mut ws) = workspace(&plain);
        let result = query(&mut ws, &path, "navigation/definition", 9, 9);
        assert_eq!(result["completeness"], "exact", "{result}");
        assert_eq!(
            result["locations"][0]["range"]["start"]["line"], 5,
            "{result}"
        );
        for (index, body) in [
            "    static member Empty =\n        let ignored = Helpers.explicit<Factory>()\n        {Name=\"x\"}\n",
            "    static member Empty =\n        let ignored = Helpers.force {Tag=0}\n        {Name=\"x\"}\n",
            "    static member Empty =\n        let ignored = fun () -> Helpers.force {Tag=0}\n        {Name=\"x\"}\n",
            "    member this.Force() = Helpers.force this\n    static member Empty =\n        {Name=\"x\"}\n",
            "    static member Empty =\n        let force seed =\n            let ignored = seed.Tag\n            Helpers.force seed\n        let ignored = force (List.head [])\n        {Name=\"x\"}\n",
        ].into_iter().enumerate() {
            let changed = format!("{prefix}{body}");
            ws.dispatch("document/change", &json!({"path":path.join("Main.fs"),"text":changed,"version":index+1})).unwrap();
            let line = changed.lines().position(|line| line.contains("{Name=\"x\"}")).unwrap();
            let result = query(&mut ws, &path, "navigation/definition", line, 9);
            assert_eq!(result["completeness"], "unavailable", "Opaque type-group constraints cannot become a latest-label proof: {result}");
            let refs = query(&mut ws, &path, "navigation/references", 5, 13);
            assert_eq!(refs["completeness"], "unavailable", "The unresolved field is not certified absent: {refs}");
        }
        for (index, body) in [
            "    static member Empty : First =\n        let ignored = Helpers.force {Tag=0}\n        {Name=\"x\"}\n",
            "    static member Empty =\n        let ignored = Helpers.force {Tag=0}\n        {First.Name=\"x\"}\n",
        ].into_iter().enumerate() {
            let changed = format!("{prefix}{body}");
            ws.dispatch("document/change", &json!({"path":path.join("Main.fs"),"text":changed,"version":index+10})).unwrap();
            let line = changed.lines().count()-1;
            let column = changed.lines().nth(line).unwrap().find("Name").unwrap();
            let result = query(&mut ws, &path, "navigation/definition", line, column);
            assert_eq!(result["completeness"], "exact", "Explicit nominal evidence is independent of unannotated freshness: {result}");
            assert_eq!(result["locations"][0]["range"]["start"]["line"], 1, "{result}");
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn qualified_source_union_payload_constrains_record_arguments_and_pattern_binders() {
        let source="module Main\ntype First={Name:string}\ntype Second={Name:string}\ntype Event=Wrap of First\nlet make ()=Event.Wrap {Name=\"x\"}\nlet read value =\n    match value with\n    | Event.Wrap item -> item.Name\n";
        let (path, mut ws) = workspace(source);
        certify_type_names(&path, &mut ws, json!({}));
        for line in [4, 7] {
            let col = source.lines().nth(line).unwrap().rfind("Name").unwrap();
            let result = query(&mut ws, &path, "navigation/definition", line, col);
            assert_eq!(result["completeness"], "exact", "line{line}: {result}");
            assert_eq!(
                result["locations"][0]["range"]["start"]["line"], 1,
                "{result}"
            );
        }
        let refs = query(&mut ws, &path, "navigation/references", 1, 12);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        let changed = source.replace("Wrap of First", "Wrap of Second");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":changed,"version":1}),
        )
        .unwrap();
        let result = query(
            &mut ws,
            &path,
            "navigation/definition",
            4,
            source.lines().nth(4).unwrap().rfind("Name").unwrap(),
        );
        assert_eq!(result["completeness"], "exact", "{result}");
        assert_eq!(result["locations"][0]["range"]["start"]["line"], 2);
        let shadow = source.replace(
            "let make",
            "module Event =\n    let Wrap (x:Second)=x\nlet make",
        );
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":shadow,"version":2}),
        )
        .unwrap();
        let result = query(
            &mut ws,
            &path,
            "navigation/definition",
            6,
            source.lines().nth(4).unwrap().rfind("Name").unwrap(),
        );
        assert_eq!(
            result["completeness"], "unavailable",
            "A qualified companion function must not be guessed as a DU case: {result}"
        );
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn qualified_union_payload_slots_do_not_flatten_tuple_fields_or_alias_owners() {
        let source="module Main\ntype First={Name:string}\ntype Second={Name:string}\ntype Event=Pair of First * Second\nlet pair=Event.Pair({Name=\"first\"},{Name=\"second\"})\n";
        let (path, mut ws) = workspace(source);
        certify_type_names(&path, &mut ws, json!({}));
        for (i, (col, _)) in source
            .lines()
            .nth(4)
            .unwrap()
            .match_indices("Name")
            .enumerate()
        {
            let result = query(&mut ws, &path, "navigation/definition", 4, col);
            assert_eq!(result["completeness"], "exact", "{result}");
            assert_eq!(result["locations"][0]["range"]["start"]["line"], i + 1);
        }
        for (version, replacement) in [
            (1, "type Event=Pair of (First * Second)"),
            (2, "type Event<'T>=Pair of 'T * Second"),
        ] {
            let changed = source.replace("type Event=Pair of First * Second", replacement);
            ws.dispatch(
                "document/change",
                &json!({"path":path.join("Main.fs"),"text":changed,"version":version}),
            )
            .unwrap();
            let col = source.lines().nth(4).unwrap().find("Name").unwrap();
            let result = query(&mut ws, &path, "navigation/definition", 4, col);
            assert_eq!(result["completeness"],"unavailable","Unsupported tuple payload/generic constraints cannot become a first-field proof: {result}");
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn term_qualifiers_require_complete_scoped_value_and_module_absence() {
        use sha2::{Digest, Sha256};
        let source = "module Main\nopen Shadow\nlet result = System.String.IsNullOrEmpty(\"\")\n";
        let (path, mut ws) = workspace(source);
        ws.ensure_model().unwrap();
        let file = path.join("Main.fs").to_str().unwrap().to_string();
        let parsed = crate::treesitter::parse(source);
        let token = parsed
            .tokens
            .iter()
            .position(|t| t.text == "IsNullOrEmpty")
            .unwrap();
        let assembly = path.join("External.dll");
        fs::write(&assembly, b"selected complete environment fixture").unwrap();
        ws.model.projects[0].evaluation = "design-time-fsc-arguments".into();
        ws.model.projects[0].reference_bindings =
            vec![json!({"path":assembly,"identity":"External","isWorkspace":false})];
        let base = crate::certified_metadata_fixture(
            json!({"schemaVersion":1,"factsVersion":2,"kind":"builtin-environment","completeness":"exact","typeNameInventory":"all-external-type-declarations","scopeNameInventory":"all-external-namespace-and-module-declarations","termNameInventory":"all-external-expression-bindings","certificateScope":"selected-reference-declaration-inventory","inputHashes":[{"path":assembly,"sha256":format!("{:x}",Sha256::digest(fs::read(&assembly).unwrap()))}],"referenceBindings":[{"path":assembly,"identity":"External"}],"workspaceAssemblies":[],"types":{},"patterns":{},"typeNames":{},"scopeNames":{"String":[{"kind":"module","sourceFullName":"Microsoft.FSharp.Core.String","enclosingScope":"Microsoft.FSharp.Core","autoOpen":false,"qualifiedOnly":true}]},"termNames":{"System":[{"kind":"enumCase","sourceFullName":"Unrelated.Mode.System","declaringScope":"Unrelated.Mode","enclosingScope":"Unrelated","qualifiedOnly":false}]},"assemblyAutoOpens":[{"modulePath":"Microsoft.FSharp.Core"}]}),
        );
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove("termNameInventory");
        let mut module = base.clone();
        module["scopeNames"]["System"] = json!([{"kind":"module","sourceFullName":"Shadow.System","enclosingScope":"Shadow","autoOpen":false,"qualifiedOnly":false}]);
        let mut value = base.clone();
        value["scopeNames"]["Values"] = json!([{"kind":"module","sourceFullName":"Shadow.Values","enclosingScope":"Shadow","autoOpen":true,"qualifiedOnly":false}]);
        value["termNames"]["System"] = json!([{"kind":"value","sourceFullName":"Shadow.Values.System","declaringScope":"Shadow.Values","enclosingScope":"Shadow","qualifiedOnly":false}]);
        let mut unopened = value.clone();
        unopened["scopeNames"]["Values"][0]["sourceFullName"] = json!("Unopened.Values");
        unopened["scopeNames"]["Values"][0]["enclosingScope"] = json!("Unopened");
        unopened["termNames"]["System"][0]["sourceFullName"] = json!("Unopened.Values.System");
        unopened["termNames"]["System"][0]["declaringScope"] = json!("Unopened.Values");
        let mut assembly_open = unopened.clone();
        assembly_open["assemblyAutoOpens"] = json!([{"modulePath":"Unopened"}]);
        for (name, data, expected) in [
            ("positive", base, true),
            ("missing", missing, false),
            ("module", module, false),
            ("value", value, false),
            ("unopened", unopened, true),
            ("assembly_open", assembly_open, false),
        ] {
            let environment = path.join(format!("{name}.json"));
            fs::write(&environment, serde_json::to_vec(&data).unwrap()).unwrap();
            ws.builtin_path = Some(environment.to_str().unwrap().into());
            ws.builtin_environment = None;
            ws.type_proofs = Default::default();
            assert_eq!(
                ws.term_qualification_clear(&file, &parsed, token, "System.String", 0)
                    .unwrap(),
                expected,
                "{name}"
            );
            assert!(
                ws.type_proofs.builtins_used,
                "Even negative candidate evidence is a proof dependency"
            );
            if name == "positive" {
                let shadow="module Main\ntype StringProxy={IsNullOrEmpty:string->bool}\ntype SystemProxy={String:StringProxy}\nlet System={String={IsNullOrEmpty=fun _->false}}\nlet result=System.String.IsNullOrEmpty(\"\")\n";
                let shadow = crate::treesitter::parse(shadow);
                let use_token = shadow
                    .tokens
                    .iter()
                    .rposition(|t| t.text == "IsNullOrEmpty")
                    .unwrap();
                assert!(
                    !ws.term_qualification_clear(&file, &shadow, use_token, "System.String", 0)
                        .unwrap(),
                    "An unresolved uppercase source binder cannot certify nominal qualification"
                );
            }
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn closed_module_unknown_uppercase_exports_are_scoped_to_active_imports() {
        let source="module Main\ntype StringProxy={IsNullOrEmpty:string->bool}\ntype SystemProxy={String:StringProxy}\nmodule Closed =\n    let System={String={IsNullOrEmpty=fun _->false}}\nlet result=System.String.IsNullOrEmpty(\"\")\n";
        let (path, mut ws) = workspace(source);
        certify_type_names(&path, &mut ws, json!({}));
        let file = path.join("Main.fs").to_str().unwrap().to_string();
        let parsed = ws.parsed(&file).unwrap();
        let token = parsed
            .tokens
            .iter()
            .rposition(|t| t.text == "IsNullOrEmpty")
            .unwrap();
        assert!(
            ws.term_qualification_clear(&file, &parsed, token, "System.String", 0)
                .unwrap(),
            "Unopened module's private name environment is irrelevant"
        );
        let opened = source.replace("let result=", "open Closed\nlet result=");
        ws.dispatch(
            "document/change",
            &json!({"path":file,"text":opened,"version":1}),
        )
        .unwrap();
        let parsed = ws.parsed(&file).unwrap();
        let token = parsed
            .tokens
            .iter()
            .rposition(|t| t.text == "IsNullOrEmpty")
            .unwrap();
        assert!(!ws.term_qualification_clear(&file,&parsed,token,"System.String",0).unwrap(),"Opening a module with an unresolved uppercase export cannot prove namespace qualification");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn source_pattern_absence_respects_exported_and_local_lifetimes() {
        let cases = [
            ("module Main\nmodule Closed =\n    let (|Some|_|) x = x\nlet read x = match x with Some item -> item\n", false),
            ("module Main\nmodule Closed =\n    let (|Some|_|) x = x\nopen Closed\nlet read x = match x with Some item -> item\n", true),
            ("module Main\nmodule Closed =\n    let private (|Some|_|) x = x\nopen Closed\nlet read x = match x with Some item -> item\n", false),
            ("module Main\nlet closed () =\n    let (|Some|_|) x = x\n    1\nlet read x = match x with Some item -> item\n", false),
            ("module Main\nlet read x =\n    let (|Some|_|) value = value\n    match x with Some item -> item\n", true),
        ];
        for (source, expected) in cases {
            let (path, mut ws) = workspace(source);
            ws.ensure_model().unwrap();
            let file = path.join("Main.fs").to_string_lossy().into_owned();
            let parsed = ws.parsed(&file).unwrap();
            let token = parsed
                .tokens
                .iter()
                .rposition(|t| t.text == "Some")
                .unwrap();
            assert_eq!(
                ws.source_name_shadow(&file, &parsed, token, "Some", true)
                    .unwrap(),
                expected,
                "{source}"
            );
            drop(ws);
            fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn source_pattern_exports_follow_cross_file_opens_and_auto_open_owners() {
        let source = "module Main\nopen Shared\nlet read x = match x with Some item -> item\n";
        let (path, mut ws) = workspace(source);
        let other = "module Shared\n[<AutoOpen>]\nmodule Patterns =\n    let (|Some|_|) x = x\n";
        fs::write(path.join("Other.fs"), other).unwrap();
        fs::write(path.join("Main.fsproj"), r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><Compile Include="Other.fs"/><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
        ws.ensure_model().unwrap();
        let other_path = path.join("Other.fs").to_string_lossy().into_owned();
        let owner = ws.parsed(&other_path).unwrap();
        // The independent attribute resolver supplies this already-proven
        // identity; this test checks its exported applicability across files.
        ws.type_proofs.attributes.insert(
            (other_path, owner.attributes[0].token),
            AttributeEffect::AutoOpen,
        );
        let file = path.join("Main.fs").to_string_lossy().into_owned();
        let parsed = ws.parsed(&file).unwrap();
        let token = parsed
            .tokens
            .iter()
            .rposition(|t| t.text == "Some")
            .unwrap();
        assert!(ws
            .source_name_shadow(&file, &parsed, token, "Some", true)
            .unwrap());
        let unopened = crate::treesitter::parse(&source.replace("open Shared\n", ""));
        let token = unopened
            .tokens
            .iter()
            .rposition(|t| t.text == "Some")
            .unwrap();
        assert!(!ws
            .source_name_shadow(&file, &unopened, token, "Some", true)
            .unwrap());
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn auto_open_source_module_at_global_scope_remains_applicable() {
        let source = "module Main\nlet read x = match x with Some item -> item\n";
        let (path, mut ws) = workspace(source);
        let owner =
            crate::treesitter::parse("[<AutoOpen>]\nmodule Patterns\nlet (|Some|_|) x = x\n");
        let attribute = owner
            .attributes
            .first()
            .expect("Top-level module attribute fact");
        ws.type_proofs.attributes.insert(
            ("Patterns.fs".into(), attribute.token),
            AttributeEffect::AutoOpen,
        );
        assert_eq!(
            ws.source_scope_is_open(
                "Patterns.fs",
                &owner,
                "Patterns",
                &std::collections::HashSet::from([String::new()]),
                0
            )
            .unwrap(),
            Some(true)
        );
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn source_qualified_static_member_hover_remains_repository_owned() {
        let source="namespace Main\nmodule LocalSystem =\n    module System =\n        type String =\n            static member IsNullOrEmpty (_value:string) = false\n    let value=System.String.IsNullOrEmpty \"\"\n";
        let (path, mut ws) = workspace(source);
        certify_type_names(&path, &mut ws, json!({}));
        let col = source
            .lines()
            .nth(5)
            .unwrap()
            .find("IsNullOrEmpty")
            .unwrap();
        let result = query(&mut ws, &path, "navigation/hover", 5, col);
        assert_eq!(result["completeness"], "exact", "{result}");
        assert_eq!(result["ownership"], "workspace", "{result}");
        assert!(result["hover"].is_null());
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn unique_static_method_propagates_a_proven_record_return() {
        let source="module Main\ntype Input={Text:string}\ntype First={Name:string}\ntype Second={Name:string}\ntype Factory =\n    static member Create(input:Input) : First = {Name=input.Text}\nlet read (input:Input)=\n    let created=Factory.Create input\n    created.Name\n";
        let (path, mut ws) = workspace(source);
        let method = query(&mut ws, &path, "navigation/definition", 7, 24);
        assert_eq!(method["completeness"], "exact", "{method}");
        assert_eq!(method["locations"][0]["range"]["start"]["line"], 5);
        let result = query(&mut ws, &path, "navigation/definition", 8, 12);
        assert_eq!(result["completeness"], "exact", "{result}");
        assert_eq!(result["locations"][0]["range"]["start"]["line"], 2);
        let refs = query(&mut ws, &path, "navigation/references", 2, 12);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        let fresh = source.replace(" : First", "");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":fresh,"version":1}),
        )
        .unwrap();
        let result = query(&mut ws, &path, "navigation/definition", 8, 12);
        assert_eq!(
            result["completeness"], "unavailable",
            "A receiver expression is outside the initial literal group envelope: {result}"
        );
        let literal = fresh.replace("input.Text", "\"fixed\"");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":literal,"version":2}),
        )
        .unwrap();
        let result = query(&mut ws, &path, "navigation/definition", 8, 12);
        assert_eq!(result["completeness"], "exact", "{result}");
        assert_eq!(result["locations"][0]["range"]["start"]["line"], 3);
        let constrained = fresh.replace(
            "type Factory =",
            "type Factory =\n    static member Force() : First = Factory.Create({Text=\"x\"})",
        );
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":constrained,"version":3}),
        )
        .unwrap();
        let result = query(&mut ws, &path, "navigation/definition", 9, 12);
        assert_eq!(
            result["completeness"], "unavailable",
            "Earlier constraints cannot be replaced by fresh label inference: {result}"
        );
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn physical_nominal_attribute_identity_needs_no_rendered_catalog() {
        use sha2::{Digest, Sha256};
        let source="module Main\nopen External\n[<Plain>]\ntype Record={Name:string}\nlet value={Name=\"x\"}\nlet read (item:Record)=item.Name\n";
        for nominal in [Some(true), Some(false), None] {
            let (path, mut ws) = workspace(source);
            ws.ensure_model().unwrap();
            let assembly = path.join("External.dll");
            fs::write(&assembly, b"certified fixture assembly").unwrap();
            ws.model.projects[0].evaluation = "design-time-fsc-arguments".into();
            ws.model.projects[0].reference_bindings =
                vec![json!({"path":assembly,"identity":"External","isWorkspace":false})];
            let mut entry = json!({"identity":{"assemblyPath":assembly,"assemblyName":"External","fullName":"External.PlainAttribute"},"sourceFullName":"External.PlainAttribute","genericArity":0,"isAbbreviation":false});
            if let Some(nominal) = nominal {
                entry["isNominal"] = json!(nominal);
            }
            let environment = crate::certified_metadata_fixture(
                json!({"schemaVersion":1,"factsVersion":2,"kind":"builtin-environment","completeness":"exact","typeNameInventory":"all-external-type-declarations","certificateScope":"selected-reference-declaration-inventory","inputHashes":[{"path":assembly,"sha256":format!("{:x}",Sha256::digest(fs::read(&assembly).unwrap()))}],"referenceBindings":[{"path":assembly,"identity":"External"}],"workspaceAssemblies":[],"types":{},"patterns":{},"typeNames":{"PlainAttribute":[entry]}}),
            );
            let environment_path = path.join("environment.json");
            fs::write(&environment_path, serde_json::to_vec(&environment).unwrap()).unwrap();
            ws.builtin_path = Some(environment_path.to_str().unwrap().into());
            let result = query(&mut ws, &path, "navigation/definition", 4, 11);
            assert_eq!(
                result["completeness"],
                if nominal == Some(true) {
                    "exact"
                } else {
                    "unavailable"
                },
                "{result}"
            );
            if nominal == Some(true) {
                assert_eq!(result["locations"][0]["range"]["start"]["line"], 3);
                assert_eq!(result["builtinDependent"], true);
            }
            assert!(
                !ws.catalog_loaded,
                "Nominal identity does not need rendered metadata"
            );
            let typed = query(
                &mut ws,
                &path,
                "navigation/definition",
                5,
                source.lines().nth(5).unwrap().rfind("Name").unwrap(),
            );
            assert_eq!(
                typed["completeness"], "exact",
                "An unknown attribute must not block explicit receiver identity: {typed}"
            );
            drop(ws);
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn closed_name_uncertainty_does_not_hide_later_fresh_record_labels() {
        let source="module Main\nmodule Constants =\n    let Algorithm = \"x\"\ntype First={Name:string}\ntype Second={Name:string}\ntype Factory =\n    static member Create() = {Name=\"value\"}\nlet fresh item = item.Name\n";
        let (path, mut ws) = workspace(source);
        for line in [6, 7] {
            let column = source.lines().nth(line).unwrap().rfind("Name").unwrap();
            let result = query(&mut ws, &path, "navigation/definition", line, column);
            assert_eq!(result["completeness"], "exact", "{result}");
            assert_eq!(
                result["locations"][0]["range"]["start"]["line"], 4,
                "{result}"
            );
        }
        let refs = query(&mut ws, &path, "navigation/references", 4, 13);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        let active = source.replace("module Constants =\n    let Algorithm", "let Algorithm");
        ws.dispatch(
            "document/change",
            &json!({"path":path.join("Main.fs"),"text":active,"version":1}),
        )
        .unwrap();
        let result = query(
            &mut ws,
            &path,
            "navigation/definition",
            5,
            active.lines().nth(5).unwrap().rfind("Name").unwrap(),
        );
        assert_eq!(
            result["completeness"], "unavailable",
            "Active ambiguity remains a barrier: {result}"
        );
        // An ended parser-recovery span has no such lexical-name guarantee.
        let mut parsed = crate::treesitter::parse(source);
        parsed.uncertain.push(crate::syntax::UncertainSpan {
            start: 0,
            end: 1,
            name: None,
            reason: "unknown recovered declaration".into(),
            binding_names: None,
            local_binding_scope: None,
        });
        let record = parsed
            .record_expressions
            .iter()
            .find(|r| r.fresh_method_result)
            .unwrap();
        assert!(ws
            .local_record_construction(
                path.join("Main.fs").to_str().unwrap(),
                &parsed,
                record.method_result.unwrap(),
                &record.fields
            )
            .is_none());
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn generic_record_field_substitution_keeps_nominal_identity() {
        let source="module Main\ntype Person = { Name:string }\ntype Company = { Name:int }\ntype Box<'T> = { Value:'T }\nlet p (box:Box<Person>) = box.Value.Name\nlet c (box:Box<Company>) = box.Value.Name\n";
        let (path, mut ws) = workspace(source);
        for (line, target) in [(4, 1), (5, 2)] {
            let def = query(
                &mut ws,
                &path,
                "navigation/definition",
                line,
                source.lines().nth(line).unwrap().rfind("Name").unwrap(),
            );
            assert_eq!(def["completeness"], "exact", "{def}");
            assert_eq!(def["locations"][0]["range"]["start"]["line"], target);
        }
        let refs = query(&mut ws, &path, "navigation/references", 1, 16);
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 1, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn long_alias_chain_uses_query_local_memo_without_reparsing() {
        let mut source =
            "module Main\ntype Record={Name:string}\nlet read (x:Record) =\n".to_string();
        let mut previous = "x".to_string();
        for i in 0..32 {
            let next = format!("a{i}");
            source.push_str(&format!("    let {next} = {previous}\n"));
            previous = next;
        }
        source.push_str(&format!("    {previous}.Name\n"));
        let (path, mut ws) = workspace(&source);
        let def = query(
            &mut ws,
            &path,
            "navigation/definition",
            35,
            source.lines().nth(35).unwrap().find("Name").unwrap(),
        );
        assert_eq!(def["completeness"], "exact", "{def}");
        assert_eq!(ws.index.counters.files_parsed, 1);
        assert!(!ws.type_proofs.exhausted);
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn genuinely_fresh_formal_uses_ordered_local_labels_but_prior_constraints_do_not() {
        let source="module Main\ntype R={Length:int}\nlet read value = value.Length\nlet constrained value =\n    System.String.IsNullOrEmpty(value) |> ignore\n    value.Length\n";
        let (path, mut ws) = workspace(source);
        let def = query(
            &mut ws,
            &path,
            "navigation/definition",
            2,
            source.lines().nth(2).unwrap().rfind("Length").unwrap(),
        );
        assert_eq!(def["completeness"], "exact", "{def}");
        assert_eq!(def["locations"][0]["range"]["start"]["line"], 1);
        let def = query(
            &mut ws,
            &path,
            "navigation/definition",
            5,
            source.lines().nth(5).unwrap().rfind("Length").unwrap(),
        );
        assert_eq!(
            def["completeness"], "unavailable",
            "Prior constraints must not guess R: {def}"
        );
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn literal_defines_preserve_cross_project_order_but_not_conditional_type_proofs() {
        let source = "module Main\nopen Library\nlet read (person:Person)=person.Name\n";
        let (path, ws) = workspace(source);
        drop(ws);
        let library = path.join("library");
        fs::create_dir(&library).unwrap();
        fs::write(
            library.join("Library.fs"),
            "module Library\ntype Person={Name:string}\n",
        )
        .unwrap();
        fs::write(library.join("Library.fsproj"),r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><DefineConstants>FIRST</DefineConstants></PropertyGroup><ItemGroup><Compile Include="Library.fs"/></ItemGroup></Project>"#).unwrap();
        fs::write(path.join("Main.fsproj"),r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><DefineConstants>SECOND</DefineConstants></PropertyGroup><ItemGroup><ProjectReference Include="library/Library.fsproj"/><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
        let mut ws = Workspace::open(&json!({"root":path})).unwrap();
        let def = query(
            &mut ws,
            &path,
            "navigation/definition",
            2,
            source.lines().nth(2).unwrap().rfind("Name").unwrap(),
        );
        assert_eq!(def["completeness"], "exact", "{def}");
        assert!(def["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/Library.fs"));
        assert!(ws.model.projects.iter().any(|p| p.defines == ["FIRST"]));
        drop(ws);
        fs::write(library.join("Library.fs"),"module Library\n#if FIRST\ntype Person={Name:string}\n#else\ntype Person={Name:int}\n#endif\n").unwrap();
        let mut ws = Workspace::open(&json!({"root":path})).unwrap();
        let def = query(
            &mut ws,
            &path,
            "navigation/definition",
            2,
            source.lines().nth(2).unwrap().rfind("Name").unwrap(),
        );
        assert_eq!(def["completeness"], "unavailable", "{def}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn cross_file_opened_type_precedes_namespace_type_and_merges_module_candidates() {
        for nearer_empty in [false, true] {
            let source = if nearer_empty {
                "namespace Shared\nmodule Use=\n    module Other=\n        type Widget={Other:int}\n    open Other\n    let read (p:Person)=p.Name\n"
            } else {
                "namespace Shared\nmodule Use=\n    open Other\n    let read (p:Person)=p.Name\n"
            };
            let (path, ws) = workspace(source);
            drop(ws);
            fs::write(path.join("A.fs"),"namespace Shared\ntype Person={Name:string}\nmodule Other=\n    type Person={Name:int}\n").unwrap();
            fs::write(path.join("Main.fsproj"),r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><Compile Include="A.fs"/><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
            let mut ws = Workspace::open(&json!({"root":path})).unwrap();
            let line = source.lines().count() - 1;
            if nearer_empty {
                let unknown = query(
                    &mut ws,
                    &path,
                    "navigation/definition",
                    line,
                    source.lines().nth(line).unwrap().rfind("Name").unwrap(),
                );
                assert_eq!(
                    unknown["completeness"], "unavailable",
                    "An open source module is not a closed external namespace: {unknown}"
                );
                certify_type_names(&path, &mut ws, json!({}));
            }
            let def = query(
                &mut ws,
                &path,
                "navigation/definition",
                line,
                source.lines().nth(line).unwrap().rfind("Name").unwrap(),
            );
            assert_eq!(def["completeness"], "exact", "{def}");
            assert_eq!(def["locations"][0]["range"]["start"]["line"], 3, "{def}");
            drop(ws);
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn merged_external_namespace_blocks_earlier_local_type_without_catalog() {
        let source="namespace Shared\nmodule Other=\n    type Widget={Other:int}\nmodule Use=\n    type Person={Name:string}\n    open Shared.Other\n    let read (p:Person)=p.Name\n";
        for external in [false, true] {
            let (path, mut ws) = workspace(source);
            let entries = if external {
                json!({"Person":[{"identity":{"assemblyPath":"/external/Person.dll","assemblyName":"External","fullName":"Shared.Other.Person"},"sourceFullName":"Shared.Other.Person","genericArity":0,"isAbbreviation":false,"isNominal":true}]})
            } else {
                json!({})
            };
            certify_type_names(&path, &mut ws, entries);
            let column = source.lines().nth(6).unwrap().rfind("Name").unwrap();
            let result = query(&mut ws, &path, "navigation/definition", 6, column);
            assert_eq!(
                result["completeness"],
                if external { "unavailable" } else { "exact" },
                "{result}"
            );
            if !external {
                assert_eq!(result["locations"][0]["range"]["start"]["line"], 4);
            }
            assert_eq!(result["builtinDependent"], true);
            drop(ws);
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn external_open_absence_preserves_nested_field_declaration_origin() {
        let source = "module Main\nmodule First=\n    type Scope={Name:string}\nmodule Second=\n    type Scope={Name:int}\nmodule Definition=\n    open First\n    open External\n    type Alias=Scope\n    type Holder={Value:Alias}\n    type Generic<'T>={Value:'T}\nmodule Caller=\n    open Definition\n    open Second\n    let read (x:Holder)=x.Value.Name\n    let generic (x:Generic<First.Scope>)=x.Value.Name\n";
        for variant in ["absent", "direct", "auto-open", "unknown-scope"] {
            let (path, mut ws) = workspace(source);
            let column = source.lines().nth(14).unwrap().rfind("Name").unwrap();
            let unknown = query(&mut ws, &path, "navigation/definition", 14, column);
            assert_eq!(unknown["completeness"], "unavailable", "{unknown}");
            let types = match variant {
                "direct" => {
                    json!({"Scope":[{"sourceFullName":"External.Scope","genericArity":0,"isNominal":true,"isAbbreviation":false,"identity":{"assemblyPath":"/external/Types.dll","assemblyName":"Types","fullName":"External.Scope"}}]})
                }
                "auto-open" => {
                    json!({"Scope":[{"sourceFullName":"External.Hidden.Scope","genericArity":0,"isNominal":true,"isAbbreviation":false,"identity":{"assemblyPath":"/external/Types.dll","assemblyName":"Types","fullName":"External.Hidden.Scope"}}]})
                }
                _ => json!({}),
            };
            certify_type_names(&path, &mut ws, types);
            let env_path = path.join("type-inventory.json");
            let mut env: Value =
                serde_json::from_str(&fs::read_to_string(&env_path).unwrap()).unwrap();
            env["scopeNames"] = json!({"External":[{"kind":"namespace","sourceFullName":"External","enclosingScope":"","autoOpen":false}],"Hidden":[{"kind":"module","sourceFullName":"External.Hidden","enclosingScope":"External","autoOpen":true}]});
            if variant == "unknown-scope" {
                env.as_object_mut().unwrap().remove("scopeNameInventory");
            }
            fs::write(&env_path, env.to_string()).unwrap();
            let result = query(&mut ws, &path, "navigation/definition", 14, column);
            assert_eq!(
                result["completeness"],
                if variant == "absent" {
                    "exact"
                } else {
                    "unavailable"
                },
                "{variant}: {result}"
            );
            if variant == "absent" {
                assert_eq!(result["locations"][0]["range"]["start"]["line"],2,"The field alias is resolved at its declaration, before Caller opens Second: {result}");
                assert_eq!(
                    result["builtinDependent"], true,
                    "Negative external-name proof must remain a dependency: {result}"
                );
                let generic = query(
                    &mut ws,
                    &path,
                    "navigation/definition",
                    15,
                    source.lines().nth(15).unwrap().rfind("Name").unwrap(),
                );
                assert_eq!(generic["completeness"], "exact", "{generic}");
                assert_eq!(generic["locations"][0]["range"]["start"]["line"], 2);
            }
            drop(ws);
            fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn module_abbreviations_capture_declaration_scope_and_remain_additive() {
        let source="module Main\nmodule First=\n    let value=1\n    let onlyFirst=11\nmodule Second=\n    let value=2\nmodule Alias=First\nmodule Alias=Second\nlet selected=Alias.value\nlet earlier=Alias.onlyFirst\nmodule Child=\n    module Second=\n        let value=3\n    let captured=Alias.value\n";
        let (path, mut ws) = workspace(source);
        for (line, col, target) in [(8, 19, 5), (9, 18, 3), (13, 23, 5)] {
            let def = query(&mut ws, &path, "navigation/definition", line, col);
            assert_eq!(def["completeness"], "exact", "{def}");
            assert_eq!(
                def["locations"][0]["range"]["start"]["line"], target,
                "{def}"
            );
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn source_type_positions_and_qualified_values_preserve_declaration_identity() {
        let source="module Main\ntype Person={Name:string}\ntype Alias=Person\nmodule Values=\n    let selected:Person={Name=\"x\"}\nlet read=Values.selected.Name\n";
        let (path, mut ws) = workspace(source);
        for (line, col, target) in [(2, 12, 1), (5, 26, 1)] {
            let def = query(&mut ws, &path, "navigation/definition", line, col);
            assert_eq!(def["completeness"], "exact", "{def}");
            assert_eq!(
                def["locations"][0]["range"]["start"]["line"], target,
                "{def}"
            );
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn virtual_declaration_routes_to_slot_while_calls_route_to_implementation() {
        let source="module Main\n[<AbstractClass>]\ntype Base()=\n    abstract Name:string\n    default _.Name=\"base\"\ntype Derived()=\n    inherit Base()\n    override _.Name=\"derived\"\nlet read (x:Derived)=x.Name\n";
        let (path, mut ws) = workspace(source);
        for (line, col, target) in [(4, 14, 3), (7, 15, 3), (8, 23, 7)] {
            let def = query(&mut ws, &path, "navigation/definition", line, col);
            assert_eq!(def["completeness"], "exact", "{def}");
            assert_eq!(
                def["locations"][0]["range"]["start"]["line"], target,
                "{def}"
            );
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn signature_pairs_separate_public_identity_from_definition_toggle() {
        let (path, ws) = workspace(
            "module Consumer\nopen Contracts\nlet created=create \"Ada\"\nlet read=created.Name\n",
        );
        drop(ws);
        fs::write(path.join("Contracts.fsi"),"module Contracts\ntype Person={Name:string;Age:int}\nval create:name:string -> Person\n").unwrap();
        fs::write(path.join("Contracts.fs"),"module Contracts\ntype Person={Name:string;Age:int}\nlet create name={Name=name;Age=1}\n").unwrap();
        fs::write(path.join("Main.fsproj"),r#"<Project Sdk="Microsoft.NET.Sdk"><ItemGroup><Compile Include="Contracts.fsi"/><Compile Include="Contracts.fs"/><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
        let mut ws = Workspace::open(&json!({"root":path})).unwrap();
        for (file, line, column, target) in [
            ("Contracts.fsi", 1, 13, "Contracts.fs"),
            ("Contracts.fs", 1, 13, "Contracts.fsi"),
            ("Contracts.fs", 2, 18, "Contracts.fsi"),
            ("Main.fs", 3, 17, "Contracts.fsi"),
            ("Contracts.fsi", 2, 5, "Contracts.fs"),
            ("Contracts.fs", 2, 5, "Contracts.fsi"),
        ] {
            let value = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":path.join(file),"line":line,"character":column}),
                )
                .unwrap();
            assert_eq!(
                value["completeness"], "exact",
                "{file}:{line}:{column} {value}"
            );
            assert!(
                value["locations"][0]["uri"]
                    .as_str()
                    .unwrap()
                    .ends_with(target),
                "{value}"
            );
        }
        let refs=ws.dispatch("navigation/references",&json!({"path":path.join("Contracts.fsi"),"line":1,"character":13,"includeDeclaration":true})).unwrap();
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 4, "{refs}");
        let refs=ws.dispatch("navigation/references",&json!({"path":path.join("Contracts.fsi"),"line":2,"character":5,"includeDeclaration":true})).unwrap();
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 3, "{refs}");
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
}
