//! Transient tree-sitter syntax analysis. Trees are dropped after extracting a
//! compact token/binding summary; no tree or source is retained by this module.
//! This resolves lexical names, not F# member overloads or inferred receiver types.
use crate::syntax::{
    Alias, AppliedAttribute, ArgumentSource, BindingTypeFact, CallArgumentContext,
    CallArgumentNameCandidate, Declaration, DeclaredParameterType, ExpressionTypeCandidate,
    ExpressionTypeRole, FactoryAtom, FunctionResultFact, FunctionReturn, Import, IntrinsicLiteral,
    LiteralProjectionSpan, MatchBranch, MatchExpression, MemberAccess, MethodCall,
    MethodDeclaration, ModuleScope, ParsedFile, PatternBinding, PatternDeclaration,
    PipelineApplication, PipelineArgument, RecordExpression, RecordField, RecordPatternBinding,
    RecordType, Token, TypeAlias, TypeSource, TypeUse, UncertainSpan, UnconstrainedMember,
    UnionCasePayload,
};
use std::collections::{HashMap, HashSet};
use std::{
    ops::ControlFlow,
    time::{Duration, Instant},
};
use tree_sitter::{Node, ParseOptions, Parser};
#[path = "literal_projection.rs"]
mod literal_projection;

#[derive(Clone)]
struct Context {
    module: String,
    end: usize,
    depth: u32,
    top: bool,
}

#[derive(Default)]
struct RecordMethodContext {
    token: Option<usize>,
    annotation: Option<(String, usize)>,
    eligible: bool,
    uses: Vec<usize>,
    literal_group: bool,
    factory_atoms: Vec<(usize, FactoryAtom)>,
}

struct Extractor<'a> {
    source: &'a str,
    literal_islands: &'a [literal_projection::LiteralIsland],
    parsed: ParsedFile,
    token_by_start: HashMap<usize, usize>,
    non_value: HashSet<usize>,
    method_owner_eligibility: HashMap<usize, bool>,
}

fn children(n: Node<'_>) -> Vec<Node<'_>> {
    let mut c = n.walk();
    n.named_children(&mut c).collect()
}
fn method_argument_groups(node: Node<'_>) -> Vec<Node<'_>> {
    field_children(node, "args")
        .into_iter()
        .map(|group| {
            // `member M(x:T) : R` stores the result annotation around the whole
            // parenthesized argument group. It is not an annotation on x.
            if group.kind() == "typed_pattern" {
                let cs = method_children(group);
                if cs.len() == 2 && cs[0].kind() == "paren_pattern" && is_type_node(cs[1].kind()) {
                    return cs[0];
                }
            }
            group
        })
        .collect()
}
fn preceding_leaf(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        if let Some(mut previous) = node.prev_sibling() {
            if previous.is_extra() {
                node = previous;
                continue;
            }
            while let Some(child) = previous.child(previous.child_count().saturating_sub(1)) {
                previous = child;
            }
            return Some(previous);
        }
        node = node.parent()?;
    }
}

fn method_children(n: Node<'_>) -> Vec<Node<'_>> {
    children(n)
        .into_iter()
        .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment" | "xml_doc"))
        .collect()
}
// These predicates certify only the source formal's syntactic domain. They
// never correlate signature names/slots or prove a complete reference set.
fn ordinary_source_function(node: Node<'_>, source: &str) -> bool {
    if node.kind() != "function_or_value_defn" || node.has_error() {
        return false;
    }
    let mut cursor = node.walk();
    let kinds: Vec<_> = node.children(&mut cursor).map(|n| n.kind()).collect();
    if !kinds.contains(&"let") || kinds.contains(&"let!") {
        return false;
    }
    let lefts: Vec<_> = method_children(node)
        .into_iter()
        .filter(|n| {
            matches!(
                n.kind(),
                "function_declaration_left" | "value_declaration_left"
            )
        })
        .collect();
    if lefts.len() != 1 {
        return false;
    }
    let left = lefts[0];
    let head = if left.kind() == "function_declaration_left" {
        let cs = method_children(left);
        if cs
            .iter()
            .any(|n| matches!(n.kind(), "active_pattern" | "op_identifier"))
        {
            return false;
        }
        cs.into_iter().find(|n| n.kind() == "identifier")
    } else {
        simple_pattern_name(left)
    };
    head.is_some_and(|n| {
        raw(n, source)
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_lowercase)
    })
}
fn nested_function_body(node: Node<'_>) -> bool {
    let mut ancestor = node.parent();
    let mut steps = 0;
    while let Some(parent) = ancestor {
        steps += 1;
        if steps > 128 || crate::cancellation::is_cancelled() {
            return false;
        }
        if matches!(
            parent.kind(),
            "file"
                | "named_module"
                | "module_defn"
                | "namespace"
                | "type_definition"
                | "type_extension_elements"
        ) {
            return false;
        }
        let body = if parent.kind() == "function_or_value_defn" {
            parent.child_by_field_name("body")
        } else if parent.kind() == "fun_expression" {
            method_children(parent)
                .into_iter()
                .find(|n| n.kind() != "argument_patterns")
        } else {
            None
        };
        if let Some(body) = body {
            return body.start_byte() <= node.start_byte() && node.end_byte() <= body.end_byte();
        }
        ancestor = parent.parent();
    }
    false
}
fn lambda_formal_domains(mut node: Node<'_>, source: &str) -> (bool, bool) {
    let mut steps = 0;
    loop {
        steps += 1;
        if steps > 128 || crate::cancellation::is_cancelled() {
            return (false, false);
        }
        let Some(parent) = node.parent() else {
            return (false, false);
        };
        if parent.has_error() {
            return (false, false);
        }
        match parent.kind() {
            "paren_expression" if method_children(parent).len() == 1 => node = parent,
            "fun_expression"
                if method_children(parent)
                    .last()
                    .is_some_and(|n| n.id() == node.id()) =>
            {
                node = parent
            }
            "application_expression" => {
                let cs = method_children(parent);
                let argument = cs.len() == 2 && cs[1].id() == node.id();
                return (argument, argument);
            }
            "function_or_value_defn" => {
                let ordinary = ordinary_source_function(parent, source)
                    && parent
                        .child_by_field_name("body")
                        .is_some_and(|n| n.id() == node.id());
                return (ordinary, ordinary && nested_function_body(parent));
            }
            "declaration_expression"
                if parent
                    .child_by_field_name("in")
                    .is_some_and(|n| n.id() == node.id()) =>
            {
                let cs = method_children(parent);
                let ordinary = cs.len() == 2 && ordinary_source_function(cs[0], source);
                // InferLambdaArgs stops at a real let continuation: this is a
                // computed function result, not a public curried formal chain.
                return (ordinary, ordinary);
            }
            _ => return (false, false),
        }
    }
}
fn raw<'a>(n: Node<'_>, s: &'a str) -> &'a str {
    &s[n.byte_range()]
}
fn name(n: Node<'_>, s: &str) -> String {
    raw(n, s)
        .split('.')
        .map(|x| x.trim().trim_matches('`'))
        .collect::<Vec<_>>()
        .join(".")
}
fn field_children<'a>(n: Node<'a>, field: &str) -> Vec<Node<'a>> {
    let mut c = n.walk();
    n.children_by_field_name(field, &mut c).collect()
}

/// `.fsi` files use the corresponding signature grammar via `parse_signature`.
pub fn parse(source: &str) -> ParsedFile {
    parse_impl(source, false)
}
pub fn parse_signature(source: &str) -> ParsedFile {
    parse_impl(source, true)
}

fn parse_impl(source: &str, signature: bool) -> ParsedFile {
    parse_impl_with_projection(source, signature, true)
}
fn parse_impl_with_projection(source: &str, signature: bool, allow_projection: bool) -> ParsedFile {
    let mut parser = Parser::new();
    let language = if signature {
        tree_sitter_fsharp::LANGUAGE_SIGNATURE
    } else {
        tree_sitter_fsharp::LANGUAGE_FSHARP
    };
    parser
        .set_language(&language.into())
        .expect("pinned F# grammar ABI");
    let started = Instant::now();
    let initial_rss = rss_bytes();
    let projection = if allow_projection && !signature && source.len() >= 64 * 1024 {
        literal_projection::project_integer_arrays_with_cancel(
            source,
            32,
            crate::cancellation::is_cancelled,
        )
    } else {
        None
    };
    let parser_source = projection
        .as_ref()
        .map(|p| p.text.as_str())
        .unwrap_or(source);
    let max_growth = env_limit(
        "FSHARP_NAVIGATOR_PARSE_GROWTH_MB",
        if projection.is_some() { 64 } else { 48 },
    ) * 1024
        * 1024;
    let max_rss = env_limit("FSHARP_NAVIGATOR_PARSE_RSS_MB", 96) * 1024 * 1024;
    let max_time = Duration::from_millis(env_limit(
        "FSHARP_NAVIGATOR_PARSE_MS",
        if projection.is_some() { 1500 } else { 250 },
    ));
    let mut checks = 0u32;
    let mut exhausted = None;
    let mut progress = |_: &tree_sitter::ParseState| {
        if crate::cancellation::is_cancelled() {
            exhausted = Some("Syntax parse was cancelled");
            return ControlFlow::Break(());
        }
        checks += 1;
        if checks % 32 != 0 {
            return ControlFlow::Continue(());
        }
        if started.elapsed() > max_time {
            exhausted = Some("Syntax parse exceeded its time budget");
            return ControlFlow::Break(());
        }
        if let Some(rss) = rss_bytes() {
            if rss > max_rss
                || initial_rss.is_some_and(|initial| rss.saturating_sub(initial) > max_growth)
            {
                exhausted = Some("Syntax parse exceeded its resident-memory budget");
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    };
    let bytes = parser_source.as_bytes();
    let mut input = |i: usize, _| if i < bytes.len() { &bytes[i..] } else { &[] };
    let Some(tree) = parser.parse_with_options(
        &mut input,
        None,
        Some(ParseOptions::new().progress_callback(&mut progress)),
    ) else {
        let reason = exhausted.unwrap_or("Syntax parser could not complete");
        return unavailable(source, reason);
    };
    let mut lines = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            lines.push(i + 1)
        }
    }
    let ascii: Vec<bool> = lines
        .iter()
        .enumerate()
        .map(|(i, start)| {
            source[*start..lines.get(i + 1).copied().unwrap_or(source.len())].is_ascii()
        })
        .collect();
    let mut tokens = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if crate::cancellation::is_cancelled() {
            return unavailable(source, "Syntax parse was cancelled");
        }
        if matches!(
            n.kind(),
            "line_comment" | "block_comment" | "comment" | "xml_doc"
        ) {
            continue;
        }
        // Named operator identifiers are atomic even when their grammar has children.
        if n.child_count() == 0 || n.kind() == "op_identifier" {
            if n.end_byte() == n.start_byte() {
                continue;
            }
            let ident = matches!(
                n.kind(),
                "identifier" | "op_identifier" | "active_pattern_op_name"
            ) || (n.kind() == "infix_op" && raw(n, source) == "|>");
            let dot = n.kind() == ".";
            // Preserve one delimiter between identifier/dot runs so e.g.
            // A.make().value can never be mistaken for A.make.value. Numeric
            // arrays can contain millions of punctuation leaves: those runs
            // are not millions of navigation records.
            let barrier = !ident && !dot;
            if barrier
                && tokens
                    .last()
                    .is_none_or(|t: &Token| !t.ident && t.text != ".")
            {
                continue;
            }
            let row = n.start_position().row;
            let column = if ascii.get(row).copied().unwrap_or(true) {
                n.start_position().column as u32
            } else {
                source[lines[row]..n.start_byte()].encode_utf16().count() as u32
            };
            let raw = raw(n, source);
            let text = if ident {
                raw.trim_matches('`').to_string()
            } else if dot {
                ".".into()
            } else {
                String::new()
            };
            tokens.push(Token {
                text,
                start: n.start_byte(),
                end: n.end_byte(),
                line: row as u32,
                column,
                end_column: column + raw.encode_utf16().count() as u32,
                ident,
            });
        } else {
            let mut c = n.walk();
            let mut cs: Vec<_> = n.children(&mut c).collect();
            cs.reverse();
            stack.extend(cs);
        }
    }
    let token_by_start = tokens
        .iter()
        .enumerate()
        .map(|(i, t)| (t.start, i))
        .collect();
    let parsed = ParsedFile {
        tokens,
        declarations: Vec::new(),
        potential_bindings: Vec::new(),
        module: None,
        opens: Vec::new(),
        aliases: Vec::new(),
        supported: true,
        reasons: Vec::new(),
        source_len: source.len(),
        uncertain: Vec::new(),
        modules: Vec::new(),
        owned_types: Vec::new(),
        auto_opens: Vec::new(),
        attributes: Vec::new(),
        non_value_tokens: Vec::new(),
        signature_independent_formals: Vec::new(),
        ordinary_function_formals: Vec::new(),
        call_argument_name_candidates: Vec::new(),
        function_value_heads: Vec::new(),
        record_types: Vec::new(),
        binding_types: Vec::new(),
        function_returns: Vec::new(),
        function_results: Vec::new(),
        member_accesses: Vec::new(),
        expression_receiver_starts: Vec::new(),
        method_declarations: Vec::new(),
        method_calls: Vec::new(),
        pipeline_applications: Vec::new(),
        type_uses: Vec::new(),
        expression_type_candidates: Vec::new(),
        explicit_constructor_heads: Vec::new(),
        qualifier_roots: Vec::new(),
        module_name_tokens: Vec::new(),
        type_name_tokens: Vec::new(),
        import_name_tokens: Vec::new(),
        union_payload_labels: Vec::new(),
        pattern_bindings: Vec::new(),
        pattern_declarations: Vec::new(),
        union_case_payloads: Vec::new(),
        match_expressions: Vec::new(),
        record_pattern_bindings: Vec::new(),
        type_aliases: Vec::new(),
        record_expressions: Vec::new(),
        unconstrained_members: Vec::new(),
        literal_islands: Vec::new(),
        anonymous_record_labels: Vec::new(),
        anonymous_record_bindings: Vec::new(),
    };
    let mut x = Extractor {
        source,
        literal_islands: projection
            .as_ref()
            .map(|p| p.islands.as_slice())
            .unwrap_or(&[]),
        parsed,
        token_by_start,
        non_value: HashSet::new(),
        method_owner_eligibility: HashMap::new(),
    };
    x.visit(
        tree.root_node(),
        &Context {
            module: String::new(),
            end: source.len(),
            depth: 0,
            top: true,
        },
    );
    x.extract_type_facts(tree.root_node());
    x.parsed
        .method_declarations
        .sort_by_key(|m| x.parsed.tokens[m.token].start);
    x.parsed
        .pattern_declarations
        .sort_by_key(|m| x.parsed.tokens[m.token].start);
    if crate::cancellation::is_cancelled() {
        return unavailable(source, "Syntax parse was cancelled");
    }
    x.parsed.literal_islands = x
        .literal_islands
        .iter()
        .map(|p| LiteralProjectionSpan {
            start: p.start,
            end: p.end,
            first_start: p.first_start,
            first_end: p.first_end,
            element_count: p.element_count,
            literal_kind: "unsuffixed-decimal-integer".into(),
        })
        .collect();
    x.parsed
        .declarations
        .sort_by_key(|d| x.parsed.tokens[d.token].start);
    x.parsed
        .potential_bindings
        .sort_by_key(|d| x.parsed.tokens[d.token].start);
    x.parsed.opens.sort_by_key(|o| o.offset);
    x.parsed.explicit_constructor_heads.sort_unstable();
    x.parsed.explicit_constructor_heads.dedup();
    let constructor_heads = &x.parsed.explicit_constructor_heads;
    x.parsed.expression_type_candidates.retain(|candidate| {
        !matches!(candidate.role, ExpressionTypeRole::ApplicationHead)
            || constructor_heads.binary_search(&candidate.token).is_err()
    });
    x.parsed.signature_independent_formals.sort_unstable();
    x.parsed.signature_independent_formals.dedup();
    x.parsed.ordinary_function_formals.sort_unstable();
    x.parsed.ordinary_function_formals.dedup();
    x.parsed.function_value_heads.sort_unstable();
    x.parsed.function_value_heads.dedup();
    // Inline match repair also extracts roles from a separately parsed branch.
    // If the two parses disagree about a callee, retain the possible label.
    x.parsed
        .call_argument_name_candidates
        .sort_by_key(|c| c.token);
    x.parsed
        .call_argument_name_candidates
        .dedup_by(|current, previous| {
            if current.token != previous.token {
                return false;
            }
            if current.callee != previous.callee {
                previous.callee = None;
            }
            previous.forwarded &= current.forwarded;
            true
        });
    x.parsed.qualifier_roots.sort_unstable();
    x.parsed.qualifier_roots.dedup();
    x.parsed.module_name_tokens.sort_unstable();
    x.parsed.module_name_tokens.dedup();
    x.parsed.type_name_tokens.sort_unstable();
    x.parsed.type_name_tokens.dedup();
    x.parsed.import_name_tokens.sort_unstable();
    x.parsed.import_name_tokens.dedup();
    x.parsed.union_payload_labels.sort_unstable();
    x.parsed.union_payload_labels.dedup();
    x.parsed.non_value_tokens = x.non_value.into_iter().collect();
    x.parsed.non_value_tokens.sort_unstable();
    x.parsed.reasons.sort();
    x.parsed.reasons.dedup();
    x.parsed.supported = x.parsed.uncertain.is_empty();
    x.parsed
}

fn env_limit(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
fn rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        status
            .lines()
            .find(|s| s.starts_with("VmRSS:"))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()
            .map(|kb| kb * 1024)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
fn unavailable(source: &str, reason: &str) -> ParsedFile {
    ParsedFile {
        tokens: Vec::new(),
        declarations: Vec::new(),
        potential_bindings: Vec::new(),
        module: None,
        opens: Vec::new(),
        aliases: Vec::new(),
        supported: false,
        reasons: vec![reason.into()],
        source_len: source.len(),
        uncertain: vec![UncertainSpan {
            start: 0,
            end: source.len().max(1),
            reason: reason.into(),
            name: None,
            binding_names: None,
            local_binding_scope: None,
        }],
        modules: Vec::new(),
        owned_types: Vec::new(),
        auto_opens: Vec::new(),
        attributes: Vec::new(),
        non_value_tokens: Vec::new(),
        signature_independent_formals: Vec::new(),
        ordinary_function_formals: Vec::new(),
        call_argument_name_candidates: Vec::new(),
        function_value_heads: Vec::new(),
        record_types: Vec::new(),
        binding_types: Vec::new(),
        function_returns: Vec::new(),
        function_results: Vec::new(),
        member_accesses: Vec::new(),
        expression_receiver_starts: Vec::new(),
        method_declarations: Vec::new(),
        method_calls: Vec::new(),
        pipeline_applications: Vec::new(),
        type_uses: Vec::new(),
        expression_type_candidates: Vec::new(),
        explicit_constructor_heads: Vec::new(),
        qualifier_roots: Vec::new(),
        module_name_tokens: Vec::new(),
        type_name_tokens: Vec::new(),
        import_name_tokens: Vec::new(),
        union_payload_labels: Vec::new(),
        pattern_bindings: Vec::new(),
        pattern_declarations: Vec::new(),
        union_case_payloads: Vec::new(),
        match_expressions: Vec::new(),
        record_pattern_bindings: Vec::new(),
        type_aliases: Vec::new(),
        record_expressions: Vec::new(),
        unconstrained_members: Vec::new(),
        literal_islands: Vec::new(),
        anonymous_record_labels: Vec::new(),
        anonymous_record_bindings: Vec::new(),
    }
}

impl Extractor<'_> {
    fn uncertain(&mut self, start: usize, end: usize, reason: &str) {
        let end = end.max(start + 1).min(self.source.len().max(start + 1));
        self.parsed.uncertain.push(UncertainSpan {
            start,
            end,
            reason: reason.into(),
            name: None,
            binding_names: None,
            local_binding_scope: None,
        });
        if !self.parsed.reasons.iter().any(|r| r == reason) {
            self.parsed.reasons.push(reason.into())
        }
    }
    fn uncertain_name(&mut self, node: Node<'_>, end: usize) {
        let name = name(node, self.source);
        self.parsed.uncertain.push(UncertainSpan {
            start: node.start_byte(),
            end,
            reason: "Uppercase pattern requires constructor/literal lookup".into(),
            name: Some(name),
            binding_names: None,
            local_binding_scope: None,
        });
        self.parsed
            .reasons
            .push("Uppercase pattern requires constructor/literal lookup".into());
    }
    fn non_value(&mut self, node: Node<'_>) {
        let a = self
            .parsed
            .tokens
            .partition_point(|t| t.start < node.start_byte());
        let b = self
            .parsed
            .tokens
            .partition_point(|t| t.start < node.end_byte());
        self.non_value
            .extend((a..b).filter(|i| self.parsed.tokens[*i].ident));
    }
    fn declaration(
        &mut self,
        node: Node<'_>,
        kind: &str,
        start: usize,
        end: usize,
        depth: u32,
        top: bool,
    ) {
        let Some(&token) = self.token_by_start.get(&node.start_byte()) else {
            return;
        };
        if !self.parsed.tokens[token].ident || self.parsed.tokens[token].text == "_" {
            return;
        }
        self.non_value.remove(&token);
        self.parsed.declarations.push(Declaration {
            name: self.parsed.tokens[token].text.clone(),
            token,
            kind: kind.into(),
            visible_start: start,
            scope_end: end,
            depth,
            top_level: top,
            accessibility: declaration_accessibility(node, self.source).into(),
        });
    }
    fn potential_binding(
        &mut self,
        node: Node<'_>,
        kind: &str,
        start: usize,
        end: usize,
        depth: u32,
        top: bool,
    ) {
        let Some(&token) = self.token_by_start.get(&node.start_byte()) else {
            return;
        };
        if !self.parsed.tokens[token].ident || self.parsed.tokens[token].text == "_" {
            return;
        }
        self.parsed.potential_bindings.push(Declaration {
            name: self.parsed.tokens[token].text.clone(),
            token,
            kind: kind.into(),
            visible_start: start,
            scope_end: end,
            depth,
            top_level: top,
            accessibility: declaration_accessibility(node, self.source).into(),
        });
    }
    fn bind_pattern(
        &mut self,
        node: Node<'_>,
        start: usize,
        end: usize,
        depth: u32,
        kind: &str,
        top: bool,
    ) {
        match node.kind() {
            "identifier" => self.declaration(node, kind, start, end, depth, top),
            "identifier_pattern" => {
                let cs = children(node);
                if let Some(head) = cs.first().copied() {
                    let ids = identifier_descendants(head);
                    let is_constructor = ids.len() != 1
                        || ids.first().is_some_and(|n| {
                            raw(*n, self.source)
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_uppercase())
                        })
                        || cs.len() > 1;
                    if is_constructor {
                        self.non_value(head);
                        if ids.len() == 1 && cs.len() == 1 {
                            self.uncertain_name(ids[0], end);
                            self.potential_binding(ids[0], kind, start, end, depth, top)
                        }
                    } else if let Some(id) = ids.first() {
                        self.declaration(*id, kind, start, end, depth, top)
                    }
                    for child in cs.iter().skip(1) {
                        self.bind_pattern(*child, start, end, depth, kind, top)
                    }
                }
            }
            "long_identifier" | "long_identifier_or_op" => {
                let ids = identifier_descendants(node);
                if ids.len() == 1 && raw(ids[0], self.source) != "_" {
                    let logical = name(ids[0], self.source);
                    if logical.chars().next().is_some_and(char::is_uppercase)
                        && logical.chars().count() > 1
                    {
                        self.non_value(node);
                        self.uncertain_name(ids[0], end);
                        self.potential_binding(ids[0], kind, start, end, depth, top)
                    } else {
                        self.declaration(ids[0], kind, start, end, depth, top)
                    }
                } else {
                    self.non_value(node);
                    self.uncertain(
                        start,
                        end,
                        "Qualified or operator pattern needs semantic binding",
                    )
                }
            }
            "typed_pattern" | "type_check_pattern" => {
                for c in children(node) {
                    if is_type_node(c.kind()) {
                        self.non_value(c)
                    } else {
                        self.bind_pattern(c, start, end, depth, kind, top)
                    }
                }
            }
            "field_pattern" | "named_field" => {
                let cs = children(node);
                if let Some(key) = cs.first() {
                    self.non_value(*key)
                }
                for c in cs.into_iter().skip(1) {
                    self.bind_pattern(c, start, end, depth, kind, top)
                }
            }
            "disjunct_pattern" => {
                self.non_value(node);
                if !self.binder_free_alternative(node) {
                    self.uncertain(
                        node.start_byte(),
                        end,
                        "Alternative patterns require a shared binding identity",
                    );
                }
            }
            "const"
            | "wildcard_pattern"
            | "attributes"
            | "access_modifier"
            | "type_arguments"
            | "type_argument_constraints" => self.non_value(node),
            "active_pattern" | "op_identifier" => {
                self.non_value(node);
                self.uncertain(
                    node.start_byte(),
                    end,
                    "Active/operator pattern binding is not yet modeled",
                )
            }
            _ => {
                for c in children(node) {
                    self.bind_pattern(c, start, end, depth, kind, top)
                }
            }
        }
    }
    fn binder_free_alternative(&self, node: Node<'_>) -> bool {
        let mut pending = vec![node];
        let mut budget = 512;
        while let Some(node) = pending.pop() {
            if budget == 0 || node.has_error() || crate::cancellation::is_cancelled() {
                return false;
            }
            budget -= 1;
            let cs = method_children(node);
            match node.kind() {
                "disjunct_pattern" if cs.len() == 2 => pending.extend(cs),
                "paren_pattern" if cs.len() == 1 => pending.push(cs[0]),
                "identifier_pattern" if cs.len() == 1 => {
                    // A syntactically qualified nullary case/literal/recognizer
                    // cannot introduce a local identifier. Its identity still
                    // requires ordinary pattern resolution elsewhere.
                    if !self
                        .expression_name(cs[0])
                        .is_some_and(|(names, arity)| names.len() >= 2 && arity.is_none())
                    {
                        return false;
                    }
                }
                "type_check_pattern" if cs.len() == 1 && is_type_node(cs[0].kind()) => {}
                "const"
                    if cs.len() == 1
                        && matches!(
                            cs[0].kind(),
                            "string" | "verbatim_string" | "triple_quoted_string"
                        )
                        && method_children(cs[0]).is_empty() =>
                {
                    // Ordinary string nodes contain only literal content.
                    // Interpolation has named format/expression children and
                    // must not be treated as this binder-free syntax.
                }
                _ => return false,
            }
        }
        true
    }
    fn visit(&mut self, node: Node<'_>, ctx: &Context) {
        if crate::cancellation::is_cancelled() {
            return;
        }
        if node.is_error() || node.is_missing() {
            self.uncertain(
                node.start_byte(),
                node.end_byte(),
                "Tree-sitter recovered incomplete or unsupported syntax",
            );
            self.non_value(node);
            return;
        }
        match node.kind() {
            "file" => {
                for c in children(node) {
                    self.visit(c, ctx)
                }
            }
            "named_module" | "namespace" | "module_defn" => {
                let name_node = node.child_by_field_name("name").or_else(|| {
                    children(node)
                        .into_iter()
                        .find(|c| matches!(c.kind(), "identifier" | "long_identifier"))
                });
                let Some(nn) = name_node else {
                    self.uncertain(
                        node.start_byte(),
                        node.end_byte(),
                        "Missing module/namespace name",
                    );
                    return;
                };
                let names = identifier_descendants(nn)
                    .into_iter()
                    .filter_map(|n| self.token(n))
                    .collect::<Vec<_>>();
                self.parsed.module_name_tokens.extend(names);
                let own = name(nn, self.source);
                if node.kind() == "module_defn" {
                    let mut cursor = node.walk();
                    let equals = node.children(&mut cursor).find(|n| n.kind() == "=");
                    if let Some(equals) = equals {
                        let rhs: Vec<_> = method_children(node)
                            .into_iter()
                            .filter(|n| n.start_byte() >= equals.end_byte())
                            .collect();
                        if rhs.len() == 1
                            && matches!(
                                rhs[0].kind(),
                                "identifier" | "long_identifier" | "long_identifier_or_op"
                            )
                        {
                            self.parsed.aliases.push(Alias {
                                name: own,
                                target: name(rhs[0], self.source),
                                offset: node.start_byte(),
                                scope_end: ctx.end,
                            });
                            self.non_value(node);
                            return;
                        }
                    }
                }
                let full = if node.kind() == "module_defn" && !ctx.module.is_empty() {
                    format!("{}.{}", ctx.module, own)
                } else {
                    own
                };
                self.extract_applied_attributes(node, &full, "module", ctx.end);
                self.non_value(nn);
                if self.parsed.module.is_none() && node.kind() != "namespace" {
                    self.parsed.module = Some(full.clone())
                }
                self.parsed.modules.push(ModuleScope {
                    name: full.clone(),
                    start: node.start_byte(),
                    end: node.end_byte(),
                });
                let next = Context {
                    module: full,
                    end: node.end_byte(),
                    depth: ctx.depth + 1,
                    top: true,
                };
                if raw(node, self.source)
                    .split_whitespace()
                    .take(3)
                    .any(|s| s == "rec")
                {
                    self.uncertain(
                        node.start_byte(),
                        node.end_byte(),
                        "Recursive modules need forward scope analysis",
                    )
                }
                for c in children(node) {
                    if c.id() != nn.id() {
                        self.visit(c, &next)
                    }
                }
            }
            "module_abbrev" => {
                let cs = children(node);
                let alias = cs.iter().find(|c| c.kind() == "identifier");
                let target = cs.iter().find(|c| c.kind() == "long_identifier");
                if let Some(token) = alias.and_then(|n| self.token(*n)) {
                    self.parsed.module_name_tokens.push(token);
                }
                if let (Some(a), Some(t)) = (alias, target) {
                    self.parsed.aliases.push(Alias {
                        name: name(*a, self.source),
                        target: name(*t, self.source),
                        offset: node.start_byte(),
                        scope_end: ctx.end,
                    })
                }
                self.non_value(node);
            }
            "import_decl" => {
                if let Some(n) = children(node)
                    .into_iter()
                    .find(|c| c.kind() == "long_identifier")
                {
                    let mut cursor = node.walk();
                    let opens_type = node
                        .children(&mut cursor)
                        .any(|child| !child.is_named() && child.kind() == "type");
                    if opens_type {
                        self.uncertain(
                            node.start_byte(),
                            ctx.end,
                            "open type requires static member resolution",
                        )
                    } else {
                        if !node.has_error() {
                            self.parsed.import_name_tokens.extend(
                                identifier_descendants(n).into_iter().filter_map(|n| {
                                    self.token_by_start.get(&n.start_byte()).copied()
                                }),
                            );
                        }
                        self.parsed.opens.push(Import {
                            name: name(n, self.source),
                            offset: node.start_byte(),
                            scope_end: ctx.end,
                        })
                    }
                }
                self.non_value(node);
            }
            "declaration_expression" | "value_declaration" => {
                let cs = children(node);
                let defs: Vec<_> = cs
                    .iter()
                    .filter(|c| c.kind() == "function_or_value_defn")
                    .copied()
                    .collect();
                if defs.is_empty() && raw(node, self.source).trim_start().starts_with("use ") {
                    let continuation = node.child_by_field_name("in");
                    let binder = cs
                        .iter()
                        .find(|c| matches!(c.kind(), "identifier" | "paren_pattern"))
                        .copied();
                    if let (Some(b), Some(rest)) = (binder, continuation) {
                        self.bind_pattern(
                            b,
                            rest.start_byte(),
                            node.end_byte(),
                            ctx.depth,
                            "value",
                            false,
                        );
                        for c in cs {
                            if c.id() != b.id() {
                                self.visit(
                                    c,
                                    &Context {
                                        end: node.end_byte(),
                                        top: false,
                                        ..ctx.clone()
                                    },
                                )
                            }
                        }
                    } else {
                        self.uncertain(
                            node.start_byte(),
                            node.end_byte(),
                            "Unrecognized resource binding",
                        )
                    }
                    return;
                }
                if defs.is_empty()
                    && (raw(node, self.source).trim_start().starts_with("let!")
                        || raw(node, self.source).trim_start().starts_with("use!"))
                {
                    self.uncertain(
                        node.start_byte(),
                        node.end_byte(),
                        "Computation-expression binding requires builder scope analysis",
                    );
                    return;
                }
                let end = if ctx.top { ctx.end } else { node.end_byte() };
                let inner = Context { end, ..ctx.clone() };
                let recursive = defs.first().is_some_and(|d| {
                    let prefix = &self.source[d.start_byte()
                        ..d.child_by_field_name("body")
                            .map(|b| b.start_byte())
                            .unwrap_or(d.end_byte())];
                    prefix.split_whitespace().take(4).any(|s| s == "rec")
                });
                let group_start = if recursive {
                    defs.first()
                        .and_then(|d| d.child_by_field_name("body"))
                        .map(|b| b.start_byte())
                } else {
                    None
                };
                for d in &defs {
                    self.definition(*d, &inner, group_start)
                }
                for c in cs {
                    if c.kind() != "function_or_value_defn" {
                        self.visit(
                            c,
                            &Context {
                                top: false,
                                ..inner.clone()
                            },
                        )
                    }
                }
            }
            "function_or_value_defn" => self.definition(node, ctx, None),
            "value_definition" => {
                for child in children(node) {
                    if child.kind() == "value_declaration_left" {
                        if let Some(active) = active_pattern_header(child) {
                            self.non_value(child);
                            self.declare_active_pattern(active, node.end_byte(), ctx);
                        } else {
                            self.bind_pattern(
                                child,
                                node.end_byte(),
                                ctx.end,
                                ctx.depth,
                                "value",
                                ctx.top,
                            )
                        }
                    } else {
                        self.non_value(child)
                    }
                }
            }
            "fun_expression" => {
                let cs = children(node);
                let args = cs.iter().find(|c| c.kind() == "argument_patterns").copied();
                if let Some(body) = cs
                    .iter()
                    .rev()
                    .find(|c| c.kind() != "argument_patterns")
                    .copied()
                {
                    if let Some(a) = args {
                        let before = self.parsed.declarations.len();
                        self.bind_pattern(
                            a,
                            body.start_byte(),
                            body.end_byte(),
                            ctx.depth + 1,
                            "parameter",
                            false,
                        );
                        let (ordinary, independent) = lambda_formal_domains(node, self.source);
                        self.mark_formal_domains(before, ordinary, independent);
                    }
                    self.visit(
                        body,
                        &Context {
                            end: body.end_byte(),
                            depth: ctx.depth + 1,
                            top: false,
                            ..ctx.clone()
                        },
                    );
                }
            }
            "rule" => {
                if self.repair_inline_rules(node, ctx) {
                    return;
                }
                if let (Some(pattern), Some(body)) = (
                    node.child_by_field_name("pattern"),
                    node.child_by_field_name("block"),
                ) {
                    let guard = node.child_by_field_name("guard");
                    let start = guard.map(|g| g.start_byte()).unwrap_or(body.start_byte());
                    self.bind_pattern(
                        pattern,
                        start,
                        node.end_byte(),
                        ctx.depth + 1,
                        "pattern",
                        false,
                    );
                    let next = Context {
                        end: node.end_byte(),
                        depth: ctx.depth + 1,
                        top: false,
                        ..ctx.clone()
                    };
                    if let Some(g) = guard {
                        self.visit(g, &next)
                    }
                    self.visit(body, &next);
                } else {
                    self.uncertain(node.start_byte(), node.end_byte(), "Incomplete match rule")
                }
            }
            "for_expression" => {
                let cs = children(node);
                if cs.len() >= 3 {
                    let binder = cs[0];
                    let body = *cs.last().unwrap();
                    self.bind_pattern(
                        binder,
                        body.start_byte(),
                        body.end_byte(),
                        ctx.depth + 1,
                        "loop",
                        false,
                    );
                    for c in cs.into_iter().skip(1) {
                        self.visit(
                            c,
                            &Context {
                                top: false,
                                depth: ctx.depth + 1,
                                ..ctx.clone()
                            },
                        )
                    }
                } else {
                    self.uncertain(
                        node.start_byte(),
                        node.end_byte(),
                        "Unrecognized loop scope",
                    )
                }
            }
            "type_definition" => {
                let mut pending = children(node);
                while let Some(n) = pending.pop() {
                    if n.kind() == "type_name" {
                        if let Some(t) = n.child_by_field_name("type_name") {
                            let own = name(t, self.source);
                            let full = if ctx.module.is_empty() {
                                own
                            } else {
                                format!("{}.{}", ctx.module, own)
                            };
                            self.parsed.owned_types.push(ModuleScope {
                                name: full,
                                start: node.start_byte(),
                                end: ctx.end,
                            });
                        }
                    } else if !matches!(
                        n.kind(),
                        "member_defn" | "method_or_prop_defn" | "function_or_value_defn"
                    ) {
                        pending.extend(children(n))
                    }
                }
                // Types do not poison unrelated lexical scopes. Their members need
                // semantic dispatch, but method/constructor parameters are lexical.
                let next = Context {
                    end: node.end_byte(),
                    depth: ctx.depth + 1,
                    top: false,
                    ..ctx.clone()
                };
                for c in children(node) {
                    self.visit(c, &next)
                }
            }
            "primary_constr_args" => self.bind_pattern(
                node,
                node.end_byte(),
                ctx.end,
                ctx.depth + 1,
                "parameter",
                false,
            ),
            "class_as_reference" => {
                for c in children(node) {
                    self.declaration(
                        c,
                        node.kind(),
                        node.end_byte(),
                        ctx.end,
                        ctx.depth + 1,
                        false,
                    )
                }
            }
            "method_or_prop_defn" => {
                let mut bodies = field_children(node, "body");
                if bodies.is_empty() {
                    let mut cursor = node.walk();
                    let equal = node
                        .children(&mut cursor)
                        .find(|c| c.kind() == "=")
                        .map(|c| c.end_byte());
                    if let Some(after) = equal {
                        bodies = children(node)
                            .into_iter()
                            .filter(|c| c.start_byte() >= after)
                            .collect()
                    }
                }
                let start = bodies
                    .first()
                    .map(|b| b.start_byte())
                    .unwrap_or(node.end_byte());
                if let Some(n) = node.child_by_field_name("name") {
                    let ids = identifier_descendants(n);
                    self.non_value(n);
                    if ids.len() == 2 {
                        self.declaration(
                            ids[0],
                            "receiver",
                            start,
                            node.end_byte(),
                            ctx.depth + 1,
                            false,
                        )
                    }
                }
                for a in field_children(node, "args") {
                    self.bind_pattern(a, start, node.end_byte(), ctx.depth + 1, "parameter", false)
                }
                if bodies.is_empty() {
                    self.uncertain(
                        node.start_byte(),
                        node.end_byte(),
                        "Property accessors require additional scope handling",
                    )
                }
                for b in bodies {
                    self.visit(
                        b,
                        &Context {
                            end: b.end_byte(),
                            depth: ctx.depth + 1,
                            top: false,
                            ..ctx.clone()
                        },
                    )
                }
            }
            "field_initializer" => {
                if let Some(f) = node.child_by_field_name("field") {
                    self.non_value(f)
                }
                if let Some(v) = node.child_by_field_name("value") {
                    self.visit(
                        v,
                        &Context {
                            top: false,
                            ..ctx.clone()
                        },
                    )
                }
            }
            "member_signature" | "type_name" | "record_field" | "union_type_field"
            | "union_type_case" | "enum_type_case" | "attributes" => self.non_value(node),
            "preproc_if" | "fsi_directive_decl" => {
                self.non_value(node);
                self.uncertain(
                    node.start_byte(),
                    ctx.end,
                    "Conditional/script context must be evaluated before binding",
                )
            }
            "srtp_call_expression" => {
                self.non_value(node);
                self.uncertain(
                    node.start_byte(),
                    node.end_byte(),
                    "SRTP call requires semantic constraints",
                )
            }
            k if is_type_node(k) => self.non_value(node),
            _ => {
                // Explicit scoped expressions carry their own lifetime. Generic
                // expression traversal never turns a nested let into an export.
                let next = Context {
                    top: false,
                    ..ctx.clone()
                };
                for c in children(node) {
                    self.visit(c, &next)
                }
            }
        }
    }
    // The pinned grammar can parse same-line case separators as infix `|`
    // and `->` inside one rule. Recover only that recognizable flat spine,
    // reparse each case independently, and map its compact summary back to the
    // original bytes. This keeps branch binders from leaking into later cases.
    fn repair_inline_rules(&mut self, node: Node<'_>, ctx: &Context) -> bool {
        let (Some(pattern), Some(body)) = (
            node.child_by_field_name("pattern"),
            node.child_by_field_name("block"),
        ) else {
            return false;
        };
        let mut ops = Vec::new();
        let mut stack = vec![body];
        while let Some(n) = stack.pop() {
            if n.kind() == "infix_op" && matches!(raw(n, self.source), "|" | "->") {
                ops.push(n)
            } else if n.kind() == "infix_expression" {
                stack.extend(children(n))
            }
        }
        ops.sort_by_key(|n| n.start_byte());
        if !ops.iter().any(|n| raw(*n, self.source) == "|") {
            return false;
        }
        let mut boundaries = Vec::new();
        for (i, op) in ops.iter().enumerate() {
            if raw(*op, self.source) == "|" {
                if !ops.get(i + 1).is_some_and(|n| raw(*n, self.source) == "->") {
                    self.uncertain(
                        node.start_byte(),
                        node.end_byte(),
                        "Unrecognized inline match branch",
                    );
                    return true;
                }
                boundaries.push((op.start_byte(), op.end_byte()));
            }
        }
        let mut spans = Vec::new();
        let mut start = pattern.start_byte();
        for (before, after) in boundaries {
            spans.push((start, before));
            start = after
        }
        spans.push((start, body.end_byte()));
        const PREFIX: &str = "let __branch = function | ";
        for (start, end) in spans {
            let fragment = format!("{}{}", PREFIX, &self.source[start..end]);
            let summary = parse_impl(&fragment, false);
            let map = |byte: usize| start + byte.saturating_sub(PREFIX.len());
            for d in &summary.declarations {
                let t = &summary.tokens[d.token];
                if t.start < PREFIX.len() {
                    continue;
                }
                let Some(&token) = self.token_by_start.get(&map(t.start)) else {
                    continue;
                };
                let mut d = d.clone();
                d.token = token;
                d.visible_start = map(d.visible_start);
                d.scope_end = map(d.scope_end).min(end);
                d.depth = ctx.depth + d.depth.saturating_sub(1);
                d.top_level = false;
                self.parsed.declarations.push(d);
            }
            let map_token = |source_token: usize| {
                let byte = summary.tokens[source_token].start;
                (byte >= PREFIX.len())
                    .then(|| self.token_by_start.get(&map(byte)).copied())
                    .flatten()
            };
            self.parsed.function_value_heads.extend(
                summary
                    .function_value_heads
                    .iter()
                    .filter_map(|&t| map_token(t)),
            );
            for candidate in &summary.call_argument_name_candidates {
                if let Some(token) = map_token(candidate.token) {
                    self.parsed
                        .call_argument_name_candidates
                        .push(CallArgumentNameCandidate {
                            token,
                            callee: candidate.callee.and_then(map_token),
                            forwarded: false, // Fragment prefixes do not witness a module-level function.
                        });
                }
            }
            for (tokens, independent) in [
                (&summary.ordinary_function_formals, false),
                (&summary.signature_independent_formals, true),
            ] {
                for source_token in tokens {
                    let t = &summary.tokens[*source_token];
                    if t.start < PREFIX.len() {
                        continue;
                    }
                    if let Some(&mapped) = self.token_by_start.get(&map(t.start)) {
                        if independent {
                            self.parsed.signature_independent_formals.push(mapped);
                        } else {
                            self.parsed.ordinary_function_formals.push(mapped);
                        }
                    }
                }
            }
            for d in &summary.potential_bindings {
                let t = &summary.tokens[d.token];
                if t.start < PREFIX.len() {
                    continue;
                }
                let Some(&token) = self.token_by_start.get(&map(t.start)) else {
                    continue;
                };
                let mut d = d.clone();
                d.token = token;
                d.visible_start = map(d.visible_start);
                d.scope_end = map(d.scope_end).min(end);
                d.depth = ctx.depth + d.depth.saturating_sub(1);
                d.top_level = false;
                self.parsed.potential_bindings.push(d);
            }
            for declaration in &summary.pattern_declarations {
                let t = &summary.tokens[declaration.token];
                if t.start < PREFIX.len() {
                    continue;
                }
                if let Some(&token) = self.token_by_start.get(&map(t.start)) {
                    let mut d = declaration.clone();
                    d.token = token;
                    d.scope_start = map(d.scope_start);
                    d.scope_end = map(d.scope_end).min(end);
                    d.module = ctx.module.clone();
                    d.top_level = false;
                    self.parsed.pattern_declarations.push(d);
                }
            }
            for token in summary.non_value_tokens {
                let t = &summary.tokens[token];
                if t.start >= PREFIX.len() {
                    if let Some(&i) = self.token_by_start.get(&map(t.start)) {
                        self.non_value.insert(i);
                    }
                }
            }
            for u in summary.uncertain {
                if u.end > PREFIX.len() {
                    self.parsed.uncertain.push(UncertainSpan {
                        start: map(u.start).max(start),
                        end: map(u.end).min(end),
                        reason: u.reason,
                        name: u.name,
                        binding_names: u.binding_names,
                        // The fragment has a synthetic function prefix. Only
                        // wholly original RHS ranges can certify a local scope;
                        // map those ranges just like the uncertainty itself.
                        local_binding_scope: u.local_binding_scope.and_then(|[a, b]| {
                            (a >= PREFIX.len() && a < b && b <= fragment.len())
                                .then(|| [map(a).max(start), map(b).min(end)])
                        }),
                    })
                }
            }
            self.parsed.reasons.extend(summary.reasons);
        }
        true
    }
    fn declare_active_pattern(&mut self, node: Node<'_>, visible: usize, ctx: &Context) {
        self.non_value(node);
        let mut pending = vec![node];
        while let Some(n) = pending.pop() {
            if n.kind() == "active_pattern_op_name" {
                if let Some(token) = self.token(n) {
                    let case = self.parsed.tokens[token].text.clone();
                    if case != "_" {
                        self.parsed.pattern_declarations.push(PatternDeclaration {
                            name: case.clone(),
                            token,
                            module: ctx.module.clone(),
                            kind: "active-pattern".into(),
                            top_level: ctx.top,
                            scope_start: visible,
                            scope_end: ctx.end,
                            type_owner: None,
                            qualified_only: false,
                            accessibility: declaration_accessibility(node, self.source).into(),
                        });
                        self.parsed.uncertain.push(UncertainSpan {
                            start: visible,
                            end: ctx.end,
                            reason: "Active pattern declaration requires scoped pattern resolution"
                                .into(),
                            name: Some(case),
                            binding_names: None,
                            local_binding_scope: None,
                        });
                    }
                }
            } else {
                pending.extend(children(n));
            }
        }
    }
    fn operator_binding_names(
        &self,
        node: Node<'_>,
        left: Node<'_>,
        recursive: bool,
    ) -> Option<Vec<String>> {
        if recursive || node.has_error() || left.kind() != "function_declaration_left" {
            return None;
        }
        if field_children(node, "body").len() != 1
            || method_children(left)
                .iter()
                .filter(|n| n.kind() == "op_identifier")
                .count()
                != 1
        {
            return None;
        }
        let mut ancestor = Some(node);
        while let Some(n) = ancestor {
            let mut cursor = n.walk();
            if n.children(&mut cursor)
                .any(|c| matches!(c.kind(), "rec" | "and"))
            {
                return None;
            }
            if matches!(
                n.kind(),
                "named_module" | "module_defn" | "namespace" | "file"
            ) {
                // Module recursion matters even outside the declaration group.
                ancestor = n.parent();
                continue;
            }
            if matches!(n.kind(), "type_definition" | "type_extension")
                || method_children(n).iter().any(|c| c.kind() == "attributes")
            {
                return None;
            }
            if n.kind() == "declaration_expression"
                && method_children(n)
                    .iter()
                    .filter(|c| c.kind() == "function_or_value_defn")
                    .count()
                    > 1
            {
                return None;
            }
            ancestor = n.parent();
        }
        let mut pending = vec![node];
        let mut budget = 4096;
        let mut operators = Vec::new();
        while let Some(n) = pending.pop() {
            if budget == 0 || n.has_error() || crate::cancellation::is_cancelled() {
                return None;
            }
            budget -= 1;
            if matches!(
                n.kind(),
                "attributes"
                    | "import_decl"
                    | "named_module"
                    | "module_defn"
                    | "module_abbrev"
                    | "namespace"
                    | "type_definition"
                    | "type_extension"
                    | "fsi_directive_decl"
                    | "preproc_if"
                    | "ce_expression"
                    | "comp_declaration_expression"
                    | "short_comp_expression"
            ) {
                return None;
            }
            if matches!(n.kind(), "op_identifier" | "infix_op" | "prefix_op") {
                operators.push(raw(n, self.source));
            }
            pending.extend(method_children(n));
        }
        let start = self
            .parsed
            .tokens
            .partition_point(|t| t.start < node.start_byte());
        let end = self
            .parsed
            .tokens
            .partition_point(|t| t.start < node.end_byte());
        let mut names = HashSet::new();
        let mut bytes = 0;
        for text in self.parsed.tokens[start..end]
            .iter()
            .filter(|token| token.ident)
            .map(|token| token.text.as_str())
            .chain(operators)
        {
            for name in [
                text,
                text.strip_prefix('(')
                    .and_then(|n| n.strip_suffix(')'))
                    .unwrap_or(text)
                    .trim(),
            ] {
                if names.insert(name.to_owned()) {
                    bytes += name.len();
                }
            }
            if names.len() > 256 || bytes > 32 * 1024 {
                return None;
            }
        }
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort();
        Some(names)
    }
    fn ordinary_function_body_scope(&self, node: Node<'_>, grouped: bool) -> Option<[usize; 2]> {
        if grouped || node.has_error() {
            return None;
        }
        let lefts = method_children(node)
            .into_iter()
            .filter(|n| {
                matches!(
                    n.kind(),
                    "function_declaration_left" | "value_declaration_left"
                )
            })
            .collect::<Vec<_>>();
        let bodies = field_children(node, "body");
        if lefts.len() != 1 || bodies.len() != 1 || lefts[0].kind() != "function_declaration_left" {
            return None;
        }
        let header = method_children(lefts[0]);
        if header.iter().filter(|n| n.kind() == "identifier").count() != 1
            || !header.iter().any(|n| n.kind() == "argument_patterns")
            || header
                .iter()
                .any(|n| matches!(n.kind(), "op_identifier" | "active_pattern" | "attributes"))
        {
            return None;
        }
        let mut ancestor = Some(node);
        while let Some(n) = ancestor {
            let mut cursor = n.walk();
            if n.children(&mut cursor)
                .any(|c| matches!(c.kind(), "rec" | "and"))
            {
                return None;
            }
            if matches!(
                n.kind(),
                "named_module" | "module_defn" | "namespace" | "file"
            ) {
                ancestor = n.parent();
                continue;
            }
            if matches!(n.kind(), "type_definition" | "type_extension")
                || method_children(n).iter().any(|c| c.kind() == "attributes")
            {
                return None;
            }
            if n.kind() == "declaration_expression"
                && method_children(n)
                    .iter()
                    .filter(|c| c.kind() == "function_or_value_defn")
                    .count()
                    > 1
            {
                return None;
            }
            ancestor = n.parent();
        }
        Some([bodies[0].start_byte(), bodies[0].end_byte()])
    }

    fn mark_formal_domains(&mut self, before: usize, ordinary: bool, independent: bool) {
        for d in &self.parsed.declarations[before..] {
            if d.kind != "parameter" {
                continue;
            }
            if ordinary {
                self.parsed.ordinary_function_formals.push(d.token);
            }
            if independent {
                self.parsed.signature_independent_formals.push(d.token);
            }
        }
    }
    fn definition(&mut self, node: Node<'_>, ctx: &Context, group_start: Option<usize>) {
        let cs = children(node);
        let lefts: Vec<_> = cs
            .iter()
            .filter(|c| {
                matches!(
                    c.kind(),
                    "function_declaration_left" | "value_declaration_left"
                )
            })
            .copied()
            .collect();
        let bodies = field_children(node, "body");
        let Some(first_body) = bodies.first().copied() else {
            self.uncertain(node.start_byte(), node.end_byte(), "Missing binding body");
            return;
        };
        let prefix = &self.source[node.start_byte()..first_body.start_byte()];
        if prefix.contains("let!") || prefix.contains("use!") || prefix.contains("and!") {
            self.uncertain(
                node.start_byte(),
                ctx.end,
                "Computation-expression binding requires builder scope analysis",
            )
        }
        let recursive = prefix.split_whitespace().take(4).any(|s| s == "rec");
        let visible = group_start.unwrap_or(if recursive {
            first_body.start_byte()
        } else {
            node.end_byte()
        });
        if lefts.len() != bodies.len() {
            self.uncertain(
                node.start_byte(),
                ctx.end,
                "Unrecognized recursive binding group",
            );
            return;
        }
        for (left, body) in lefts.into_iter().zip(bodies.iter().copied()) {
            if ordinary_source_function(node, self.source) {
                let head = if left.kind() == "function_declaration_left" {
                    method_children(left)
                        .into_iter()
                        .find(|n| n.kind() == "identifier")
                } else if matches!(body.kind(), "fun_expression" | "long_identifier_or_op") {
                    simple_pattern_name(left)
                } else {
                    None
                };
                if let Some(token) = head.and_then(|n| self.token(n)) {
                    self.parsed.function_value_heads.push(token);
                }
            }
            if left.kind() == "function_declaration_left" {
                for c in children(left) {
                    match c.kind() {
                        "identifier" => {
                            self.declaration(c, "value", visible, ctx.end, ctx.depth, ctx.top)
                        }
                        "active_pattern" => {
                            self.declare_active_pattern(c, visible, ctx);
                        }
                        "op_identifier" => {
                            self.non_value(c);
                            let names = self.operator_binding_names(
                                node,
                                left,
                                recursive || group_start.is_some(),
                            );
                            self.uncertain(node.start_byte(),ctx.end,"Custom operator/active-pattern declaration needs semantic identity");
                            self.parsed.uncertain.last_mut().unwrap().binding_names = names;
                        }
                        "argument_patterns" => {
                            let before = self.parsed.declarations.len();
                            self.bind_pattern(
                                c,
                                body.start_byte(),
                                body.end_byte(),
                                ctx.depth + 1,
                                "parameter",
                                false,
                            );
                            let ordinary = ordinary_source_function(node, self.source);
                            self.mark_formal_domains(
                                before,
                                ordinary,
                                ordinary && nested_function_body(node),
                            );
                        }
                        _ => self.non_value(c),
                    }
                }
            } else {
                self.bind_pattern(left, visible, ctx.end, ctx.depth, "value", ctx.top)
            }
        }
        for c in cs {
            if is_type_node(c.kind()) {
                self.non_value(c)
            }
        }
        let local_scope =
            self.ordinary_function_body_scope(node, recursive || group_start.is_some());
        for b in bodies {
            let before = self.parsed.uncertain.len();
            self.visit(
                b,
                &Context {
                    end: b.end_byte(),
                    depth: ctx.depth + 1,
                    top: false,
                    ..ctx.clone()
                },
            );
            if let Some([start, end]) = local_scope {
                for span in &mut self.parsed.uncertain[before..] {
                    if span.start >= start && span.end <= end && span.local_binding_scope.is_none()
                    {
                        span.local_binding_scope = Some([start, end]);
                    }
                }
            }
        }
    }
}

// Additional navigation facts are collected while the transient tree is alive.
// They describe syntax and token identity, never a whole-program inferred graph.
impl Extractor<'_> {
    fn token(&self, node: Node<'_>) -> Option<usize> {
        self.token_by_start
            .get(&node.start_byte())
            .copied()
            .filter(|i| self.parsed.tokens[*i].ident)
    }
    fn named_type(&self, node: Node<'_>) -> Option<(String, usize)> {
        let first = identifier_descendants(node).into_iter().next()?;
        Some((raw(node, self.source).trim().to_owned(), self.token(first)?))
    }
    fn simple_value_token(&self, node: Node<'_>) -> Option<usize> {
        if !matches!(
            node.kind(),
            "identifier" | "long_identifier" | "long_identifier_or_op"
        ) {
            return None;
        }
        let ids = identifier_descendants(node);
        self.token(*ids.last()?)
    }
    fn nominal_owner(&self, token: usize) -> String {
        let own = &self.parsed.tokens[token].text;
        match self.parsed.module_at(token) {
            Some(module) if !module.is_empty() => format!("{module}.{own}"),
            _ => own.clone(),
        }
    }
    fn owner_end(&self, token: usize) -> usize {
        let offset = self.parsed.tokens[token].start;
        self.parsed
            .modules
            .iter()
            .filter(|m| m.start <= offset && offset < m.end)
            .min_by_key(|m| m.end - m.start)
            .map(|m| m.end)
            .unwrap_or(self.source.len())
    }
    fn intersects_literal_island(&self, node: Node<'_>) -> bool {
        let i = self
            .literal_islands
            .partition_point(|island| island.end <= node.start_byte());
        self.literal_islands
            .get(i)
            .is_some_and(|island| island.start < node.end_byte())
    }
    fn expression_type_source(&self, node: Node<'_>) -> Option<TypeSource> {
        if self.intersects_literal_island(node) {
            return None;
        }
        let node = unwrap_expression(node);
        if let Some(token) = self.simple_value_token(node) {
            return Some(TypeSource::Alias { token });
        }
        match node.kind() {
            "typecast_expression" => {
                let ty = children(node)
                    .into_iter()
                    .find(|n| is_type_node(n.kind()))?;
                let (name, token) = self.named_type(ty)?;
                Some(TypeSource::Named { name, token })
            }
            "brace_expression" => {
                let mut body = children(node)
                    .into_iter()
                    .find(|n| matches!(n.kind(), "field_initializers" | "with_field_expression"))?;
                let mut copy = None;
                if body.kind() == "with_field_expression" {
                    let cs = children(body);
                    copy = Some(self.simple_value_token(unwrap_expression(*cs.first()?))?);
                    body = cs.into_iter().find(|n| n.kind() == "field_initializers")?;
                }
                let mut fields = Vec::new();
                for field in method_children(body) {
                    if field.kind() != "field_initializer" {
                        return None;
                    }
                    let label = field.child_by_field_name("field")?;
                    fields.push(self.token(*identifier_descendants(label).last()?)?);
                }
                if fields.is_empty() {
                    None
                } else {
                    Some(TypeSource::Record { fields, copy })
                }
            }
            "application_expression" => {
                let (function, arguments) = self.application_target(node)?;
                Some(TypeSource::FunctionResult {
                    function,
                    arguments,
                })
            }
            "prefixed_expression" if node.child(0).is_some_and(|n| n.kind() == "new") => {
                let mut target = children(node)
                    .into_iter()
                    .find(|n| n.kind() == "application_expression")?;
                while target.kind() == "application_expression" {
                    target = *children(target).first()?;
                }
                let token = if target.kind() == "typed_expression" {
                    self.simple_value_token(*children(target).first()?)
                } else {
                    self.simple_value_token(target)
                }?;
                let name = raw(target, self.source).trim().to_owned();
                Some(TypeSource::Named { name, token })
            }
            _ => None,
        }
    }
    fn application_target(&self, mut node: Node<'_>) -> Option<(usize, u32)> {
        let mut arguments = 0;
        loop {
            node = unwrap_expression(node);
            if node.kind() != "application_expression" {
                return self.simple_value_token(node).map(|t| (t, arguments));
            }
            let cs = children(node);
            if cs.len() != 2 {
                return None;
            }
            arguments += 1;
            node = cs[0];
        }
    }
    fn branch_tail_source(&self, mut node: Node<'_>) -> Option<TypeSource> {
        loop {
            if crate::cancellation::is_cancelled() {
                return None;
            }
            if node.has_error() || self.intersects_literal_island(node) {
                return None;
            }
            let cs = method_children(node);
            match node.kind() {
                "paren_expression" if cs.len() == 1 => node = cs[0],
                "sequential_expression" => node = *cs.last()?,
                "declaration_expression" => node = node.child_by_field_name("in")?,
                _ => return self.expression_type_source(node),
            }
        }
    }
    fn explicit_tail_annotation(&self, mut expression: Node<'_>) -> Option<TypeSource> {
        loop {
            if crate::cancellation::is_cancelled() {
                return None;
            }
            let parent = expression.parent()?;
            let cs = method_children(parent);
            match parent.kind() {
                "paren_expression" if cs.len() == 1 => expression = parent,
                "sequential_expression" if cs.last().is_some_and(|n| n.id() == expression.id()) => {
                    expression = parent
                }
                "declaration_expression"
                    if parent
                        .child_by_field_name("in")
                        .is_some_and(|n| n.id() == expression.id()) =>
                {
                    expression = parent
                }
                "typecast_expression" => {
                    let mut cursor = parent.walk();
                    if !parent.children(&mut cursor).any(|n| n.kind() == ":") {
                        return None;
                    }
                    let ty = cs.iter().find(|n| is_type_node(n.kind()))?;
                    let (name, token) = self.named_type(*ty)?;
                    return Some(TypeSource::Named { name, token });
                }
                "function_or_value_defn" => {
                    let lefts = cs.iter().filter(|n| {
                        matches!(
                            n.kind(),
                            "value_declaration_left" | "function_declaration_left"
                        )
                    });
                    for (left, body) in lefts.zip(field_children(parent, "body")) {
                        if body.id() != expression.id() {
                            continue;
                        }
                        let ty = cs.iter().find(|n| {
                            is_type_node(n.kind())
                                && n.start_byte() >= left.end_byte()
                                && n.end_byte() <= body.start_byte()
                        })?;
                        let (name, token) = self.named_type(*ty)?;
                        return Some(TypeSource::Named { name, token });
                    }
                    return None;
                }
                "method_or_prop_defn" => {
                    let mut cursor = parent.walk();
                    let equals = parent.children(&mut cursor).find(|n| n.kind() == "=")?;
                    if cs
                        .iter()
                        .filter(|n| n.start_byte() >= equals.end_byte())
                        .any(|n| n.id() != expression.id())
                    {
                        return None;
                    }
                    let ty = cs
                        .iter()
                        .filter(|n| n.end_byte() <= equals.start_byte())
                        .find_map(|n| {
                            if is_type_node(n.kind()) {
                                Some(*n)
                            } else if n.kind() == "typed_pattern" {
                                method_children(*n)
                                    .into_iter()
                                    .find(|n| is_type_node(n.kind()))
                            } else {
                                None
                            }
                        })?;
                    let (name, token) = self.named_type(ty)?;
                    return Some(TypeSource::Named { name, token });
                }
                _ => return None,
            }
        }
    }
    fn extract_match_expression(&mut self, node: Node<'_>) {
        let groups = method_children(node)
            .into_iter()
            .filter(|n| n.kind() == "rules")
            .collect::<Vec<_>>();
        let mut complete = !node.has_error()
            && !self.intersects_literal_island(node)
            && !raw(node, self.source).trim_start().starts_with("match!")
            && groups.len() == 1;
        let mut branches = Vec::new();
        for group in groups {
            for rule in method_children(group) {
                if crate::cancellation::is_cancelled() {
                    return;
                }
                if rule.kind() != "rule" {
                    complete = false;
                    continue;
                }
                let pattern = rule.child_by_field_name("pattern");
                let body = rule.child_by_field_name("block");
                let guard = rule.child_by_field_name("guard");
                let mut inline_separator = false;
                let mut pending = body.into_iter().collect::<Vec<_>>();
                while let Some(n) = pending.pop() {
                    if crate::cancellation::is_cancelled() {
                        return;
                    }
                    if n.kind() == "infix_op" && matches!(raw(n, self.source), "|" | "->") {
                        inline_separator = true;
                    } else if n.kind() == "infix_expression" {
                        pending.extend(method_children(n));
                    }
                }
                complete &=
                    pattern.is_some() && body.is_some() && !rule.has_error() && !inline_separator;
                branches.push(MatchBranch {
                    pattern_start: pattern.map(|n| n.start_byte()).unwrap_or(rule.start_byte()),
                    pattern_end: pattern.map(|n| n.end_byte()).unwrap_or(rule.start_byte()),
                    guard_start: guard.map(|n| n.start_byte()),
                    guard_end: guard.map(|n| n.end_byte()),
                    body_start: body.map(|n| n.start_byte()).unwrap_or(rule.end_byte()),
                    body_end: body.map(|n| n.end_byte()).unwrap_or(rule.end_byte()),
                    tail: if inline_separator {
                        None
                    } else {
                        body.and_then(|n| self.branch_tail_source(n))
                    },
                });
            }
        }
        complete &= !branches.is_empty();
        self.parsed.match_expressions.push(MatchExpression {
            start: node.start_byte(),
            end: node.end_byte(),
            branches,
            expected: self.explicit_tail_annotation(node),
            complete,
        });
    }
    /// The grammar associates `x=a+b` and `x=f a` differently from the
    /// compiler. Locate the opening delimiter or comma before the identifier
    /// instead of relying on the equality being the argument's root node.
    fn extract_argument_name(&mut self, node: Node<'_>) {
        let cs = method_children(node);
        if cs.len() != 3 || raw(cs[1], self.source) != "=" {
            return;
        }
        let mut left = cs[0];
        while matches!(left.kind(), "long_identifier_or_op" | "long_identifier") {
            let names = method_children(left);
            let [only] = names.as_slice() else { return };
            left = *only;
        }
        if left.kind() != "identifier" {
            return;
        }
        let Some(token) = self.token(left) else {
            return;
        };
        let Some(mut separator) = preceding_leaf(node) else {
            return;
        };
        let optional = raw(separator, self.source) == "?";
        if optional {
            let Some(before) = preceding_leaf(separator) else {
                return;
            };
            separator = before;
        }
        if !matches!(separator.kind(), "(" | "," | "[" | ".[") {
            return;
        }
        let mut ancestor = separator.parent();
        while let Some(group) = ancestor {
            let mut call = group;
            let head = match group.kind() {
                "application_expression" | "generic_new_expression" => {
                    let mut cursor = group.walk();
                    if !group
                        .children(&mut cursor)
                        .any(|c| c.kind() == "(" && c.start_byte() <= separator.start_byte())
                    {
                        ancestor = group.parent();
                        continue;
                    }
                    method_children(group).first().copied()
                }
                "paren_expression" | "list_expression" => {
                    let Some(parent) = group.parent() else { return };
                    call = parent;
                    let parts = method_children(call);
                    if call.kind() == "class_inherits_decl" {
                        None
                    } else if call.kind() == "application_expression"
                        && parts.len() == 2
                        && parts[1].id() == group.id()
                    {
                        let mut cursor = call.walk();
                        if call.children(&mut cursor).any(|n| n.kind() == "(") {
                            return; // A second, inner pair of parentheses groups the equality.
                        }
                        if group.kind() == "list_expression" {
                            if parts[0].end_byte() != group.start_byte() {
                                return;
                            }
                            None // Dotless indexer.
                        } else {
                            Some(parts[0])
                        }
                    } else {
                        return;
                    }
                }
                "index_expression" => None,
                "array_expression" | "brace_expression" | "anon_record_expression" | "file" => {
                    return
                }
                _ => {
                    ancestor = group.parent();
                    continue;
                }
            };
            let callee = if optional {
                None
            } else {
                head.filter(|n| n.kind() == "long_identifier_or_op")
                    .and_then(|n| self.expression_name(n))
                    .and_then(|(names, _)| names.last().copied())
            };
            self.parsed
                .call_argument_name_candidates
                .push(CallArgumentNameCandidate {
                    token,
                    callee,
                    forwarded: !optional && self.forwards_single_argument(call, node, cs[2]),
                });
            return;
        }
    }

    // let f arg = Owner.Method(label=arg). The caller formal has no other
    // constraint or use. Compound arguments can have different precedence
    // in FCS and the CST and cannot certify a parameter label here.
    fn forwards_single_argument(
        &self,
        call: Node<'_>,
        assignment: Node<'_>,
        rhs: Node<'_>,
    ) -> bool {
        let Some(function) = call.parent() else {
            return false;
        };
        if function.kind() != "function_or_value_defn"
            || !ordinary_source_function(function, self.source)
        {
            return false;
        }
        let mut cursor = function.walk();
        let pieces: Vec<_> = function
            .children(&mut cursor)
            .filter(|n| !n.is_extra())
            .collect();
        if pieces.len() != 4
            || pieces[0].kind() != "let"
            || pieces[1].kind() != "function_declaration_left"
            || pieces[2].kind() != "="
            || pieces[3].id() != call.id()
        {
            return false;
        }
        let Some(container) = function.parent() else {
            return false;
        };
        let container = if container.kind() == "declaration_expression" {
            if method_children(container).len() != 1 {
                return false;
            }
            container.parent()
        } else {
            Some(container)
        };
        if !container.is_some_and(|n| n.kind() == "named_module") {
            return false;
        }
        let left = method_children(pieces[1]);
        if left.len() != 2
            || left[0].kind() != "identifier"
            || left[1].kind() != "argument_patterns"
        {
            return false;
        }
        let formals = method_children(left[1]);
        let Some(formal) = formals.first().copied().filter(|_| formals.len() == 1) else {
            return false;
        };
        if formal.kind() != "long_identifier"
            || method_children(formal).len() != 1
            || rhs.kind() != "long_identifier_or_op"
            || raw(rhs, self.source) != raw(formal, self.source)
        {
            return false;
        }
        let cs = method_children(call);
        if cs.len() != 2 || cs[0].kind() != "long_identifier_or_op" {
            return false;
        }
        let arg = if cs[1].kind() == "paren_expression" {
            method_children(cs[1]).first().copied()
        } else {
            Some(cs[1])
        };
        arg.is_some_and(|n| n.id() == assignment.id())
    }

    fn expression_name(&self, node: Node<'_>) -> Option<(Vec<usize>, Option<u32>)> {
        let mut node = unwrap_expression(node);
        if node.has_error() || self.intersects_literal_island(node) {
            return None;
        }
        let mut arity = None;
        if node.kind() == "typed_expression" {
            let cs = method_children(node);
            if cs.len() != 2 || cs[1].kind() != "types" {
                return None;
            }
            let args = method_children(cs[1]);
            if args.is_empty() || args.iter().any(|n| !is_type_node(n.kind())) {
                return None;
            }
            arity = Some(args.len() as u32);
            node = cs[0];
        }
        let mut names = Vec::new();
        let mut pending = vec![node];
        let mut budget = 512;
        while let Some(n) = pending.pop() {
            if budget == 0 || names.len() >= 128 {
                return None;
            }
            budget -= 1;
            match n.kind() {
                "identifier" => names.push(self.token(n)?),
                "long_identifier" | "long_identifier_or_op" => {
                    let mut cs = method_children(n);
                    cs.reverse();
                    pending.extend(cs);
                }
                "dot_expression" => {
                    pending.push(n.child_by_field_name("field")?);
                    pending.push(n.child_by_field_name("base")?);
                }
                _ => return None,
            }
        }
        // A dot inside one escaped identifier is not a qualifier separator.
        // The compact name schema cannot safely represent that distinction.
        (!names.is_empty()
            && self.parsed.tokens[names[0]].text != "_"
            && names
                .iter()
                .all(|t| !self.parsed.tokens[*t].text.contains('.')))
        .then_some((names, arity))
    }
    fn extract_explicit_constructor_head(&mut self, node: Node<'_>) {
        if node.has_error() {
            return;
        }
        let mut cursor = node.walk();
        if !node
            .children(&mut cursor)
            .any(|n| !n.is_named() && n.kind() == "new")
        {
            return;
        }
        let Some(mut head) = method_children(node).first().copied() else {
            return;
        };
        for _ in 0..64 {
            head = unwrap_expression(head);
            if matches!(head.kind(), "application_expression" | "tuple_expression") {
                // The grammar can include a following comma-separated expression
                // under the prefix. Only the first component follows `new`.
                let Some(first) = method_children(head).first().copied() else {
                    return;
                };
                head = first;
                continue;
            }
            if head.kind() == "typed_expression" {
                let Some(first) = method_children(head).first().copied() else {
                    return;
                };
                head = first;
            }
            // This is a token role, not a dotted logical name. Escaped names
            // must remain protected even when candidate-name encoding declines.
            if let Some(token) = self.method_callee_token(head) {
                self.parsed.explicit_constructor_heads.push(token);
            }
            return;
        }
    }
    fn extract_expression_type_candidates(&mut self, node: Node<'_>) {
        let cs = method_children(node);
        if node.has_error() || cs.len() != 2 || self.intersects_literal_island(node) {
            return;
        }
        let Some((head, generic_arity)) = self.expression_name(cs[0]) else {
            return;
        };
        let token = *head.last().unwrap();
        let name = head
            .iter()
            .map(|t| self.parsed.tokens[*t].text.as_str())
            .collect::<Vec<_>>()
            .join(".");
        self.parsed
            .expression_type_candidates
            .push(ExpressionTypeCandidate {
                token,
                name,
                generic_arity,
                role: ExpressionTypeRole::ApplicationHead,
            });
        // Spelling selects only a candidate edge. The resolver must establish
        // the intrinsic's actual binding before treating its operand as a type.
        if self.parsed.tokens[token].text == "nameof" {
            if let Some((argument, generic_arity)) = self.expression_name(cs[1]) {
                let name = argument
                    .iter()
                    .map(|t| self.parsed.tokens[*t].text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                self.parsed
                    .expression_type_candidates
                    .push(ExpressionTypeCandidate {
                        token: *argument.last().unwrap(),
                        name,
                        generic_arity,
                        role: ExpressionTypeRole::NameofArgument { callee: token },
                    });
            }
        }
    }
    fn extract_qualifier_root(&mut self, node: Node<'_>, declarations: &HashSet<usize>) {
        let Some(parent) = node.parent() else { return };
        // Only original expression slots, never a generic long identifier in
        // a declaration, type, pattern, member signature or record label.
        let expression_slot = match parent.kind() {
            "dot_expression" => false,
            "function_or_value_defn" => field_children(parent, "body")
                .iter()
                .any(|n| n.id() == node.id()),
            "field_initializer" => parent
                .child_by_field_name("value")
                .is_some_and(|n| n.id() == node.id()),
            "rule" => ["block", "guard"].iter().any(|f| {
                parent
                    .child_by_field_name(f)
                    .is_some_and(|n| n.id() == node.id())
            }),
            "method_or_prop_defn" => {
                let mut cursor = parent.walk();
                let after_equals = parent
                    .children(&mut cursor)
                    .find(|n| n.kind() == "=")
                    .is_some_and(|n| n.end_byte() <= node.start_byte());
                after_equals
            }
            k => k.ends_with("_expression") && k != "srtp_call_expression",
        };
        if !expression_slot {
            return;
        }
        let Some((names, _)) = self.expression_name(node) else {
            return;
        };
        if self
            .parsed
            .explicit_constructor_heads
            .contains(names.last().unwrap())
        {
            return;
        }
        if names.len() < 2 {
            return;
        }
        let root = names[0];
        if self.non_value.contains(&root) || declarations.contains(&root) {
            return;
        }
        let mut operand = node;
        while let Some(p) = operand
            .parent()
            .filter(|p| matches!(p.kind(), "paren_expression" | "typed_expression"))
        {
            if method_children(p)
                .first()
                .is_none_or(|n| n.id() != operand.id())
            {
                break;
            }
            operand = p;
        }
        if let Some(call) = operand
            .parent()
            .filter(|p| p.kind() == "application_expression")
        {
            let cs = method_children(call);
            if cs.len() == 2
                && cs[1].id() == operand.id()
                && self.expression_name(cs[0]).is_some_and(|(head, _)| {
                    self.parsed.tokens[*head.last().unwrap()].text == "nameof"
                })
            {
                return;
            }
        }
        self.parsed.qualifier_roots.push(root);
    }
    fn extract_type_use(&mut self, node: Node<'_>) {
        // Only direct nominal heads: nested type arguments are visited separately.
        // A prefixed `new` head is a constructor navigation role, not this role.
        if node.parent().is_some_and(|p| {
            p.kind() == "prefixed_expression" && p.child(0).is_some_and(|n| n.kind() == "new")
        }) {
            return;
        }
        for head in method_children(node)
            .into_iter()
            .filter(|n| n.kind() == "long_identifier")
        {
            // A child head may be intact inside a type application with a
            // missing delimiter. The complete owning type spine must be sound.
            let mut owner = node;
            while let Some(parent) = owner.parent().filter(|p| {
                is_type_node(p.kind()) || matches!(p.kind(), "type_attribute" | "type_attributes")
            }) {
                owner = parent;
            }
            if !owner.has_error() {
                let head_tokens: Vec<_> = identifier_descendants(head)
                    .into_iter()
                    .filter_map(|n| self.token(n))
                    .collect();
                self.parsed.type_name_tokens.extend(head_tokens);
            }
            if let Some(token) = identifier_descendants(head)
                .last()
                .and_then(|n| self.token(*n))
            {
                let generic_arity = if node.has_error() {
                    None
                } else {
                    match node.kind() {
                        "simple_type" => Some(0),
                        "generic_type" => method_children(node)
                            .iter()
                            .find(|n| n.kind() == "type_attributes")
                            .and_then(|n| {
                                let arguments = method_children(*n);
                                (!arguments.is_empty()
                                    && arguments.iter().all(|n| n.kind() == "type_attribute"))
                                .then_some(arguments.len() as u32)
                            }),
                        "postfix_type" => Some(1),
                        _ => None,
                    }
                };
                self.parsed.type_uses.push(TypeUse {
                    token,
                    name: name(head, self.source),
                    generic_arity,
                });
            }
        }
    }
    fn pipeline_argument(&self, node: Node<'_>) -> PipelineArgument {
        if node.has_error() || self.intersects_literal_island(node) {
            return PipelineArgument::Unknown;
        }
        let node = unwrap_expression(node);
        if node.kind() != "fun_expression" {
            return self
                .expression_type_source(node)
                .map(|source| PipelineArgument::Value { source })
                .unwrap_or(PipelineArgument::Unknown);
        }
        let cs = method_children(node);
        let Some(args) = cs.iter().find(|n| n.kind() == "argument_patterns").copied() else {
            return PipelineArgument::Unknown;
        };
        let bodies = cs
            .iter()
            .filter(|n| n.kind() != "argument_patterns")
            .copied()
            .collect::<Vec<_>>();
        if bodies.len() != 1 {
            return PipelineArgument::Unknown;
        }
        let body = bodies[0];
        let mut parameters = Vec::new();
        for pattern in method_children(args) {
            let Some(token) = simple_pattern_name(pattern).and_then(|n| self.token(n)) else {
                return PipelineArgument::Unknown;
            };
            if !self.parsed.declarations.iter().any(|d| {
                d.token == token
                    && d.kind == "parameter"
                    && d.visible_start == body.start_byte()
                    && d.scope_end == body.end_byte()
            }) {
                return PipelineArgument::Unknown;
            }
            parameters.push(token);
        }
        if parameters.is_empty() {
            return PipelineArgument::Unknown;
        }
        PipelineArgument::Lambda {
            parameters,
            body_start: body.start_byte(),
            body_end: body.end_byte(),
        }
    }
    fn extract_pipeline_application(&mut self, node: Node<'_>) {
        if node.has_error() || self.intersects_literal_island(node) {
            return;
        }
        // The grammar can place the application around an infix expression.
        // Only flatten a literal application spine: parentheses are a boundary.
        if node.parent().is_some_and(|p| {
            p.kind() == "application_expression"
                && method_children(p)
                    .first()
                    .is_some_and(|c| c.id() == node.id())
        }) {
            return;
        }
        let mut target = node;
        let mut arguments = Vec::new();
        while target.kind() == "application_expression" {
            let cs = method_children(target);
            if cs.len() != 2 {
                return;
            }
            arguments.push(cs[1]);
            target = cs[0];
        }
        arguments.reverse();
        if target.kind() != "infix_expression" {
            return;
        }
        let cs = method_children(target);
        if cs.len() != 3 || cs[1].kind() != "infix_op" || raw(cs[1], self.source) != "|>" {
            return;
        }
        let Some(operator) = self.token(cs[1]) else {
            return;
        };
        let Some(input) = self.expression_type_source(cs[0]) else {
            return;
        };
        let mut callable = cs[2];
        let mut inner_arguments = Vec::new();
        while callable.kind() == "application_expression" {
            let cs = method_children(callable);
            if cs.len() != 2 {
                return;
            }
            inner_arguments.push(cs[1]);
            callable = cs[0];
        }
        inner_arguments.reverse();
        inner_arguments.extend(arguments);
        let Some(callable) = self.simple_value_token(callable) else {
            return;
        };
        let arguments = inner_arguments
            .into_iter()
            .map(|n| self.pipeline_argument(n))
            .collect();
        self.parsed.pipeline_applications.push(PipelineApplication {
            operator,
            input,
            callable,
            arguments,
            start: node.start_byte(),
            end: node.end_byte(),
        });
    }
    fn extract_type_facts(&mut self, root: Node<'_>) {
        let declaration_tokens: HashSet<usize> =
            self.parsed.declarations.iter().map(|d| d.token).collect();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if crate::cancellation::is_cancelled() {
                return;
            }
            if node.is_error() || node.is_missing() {
                continue;
            }
            if matches!(node.kind(), "long_identifier_or_op" | "dot_expression") {
                self.extract_qualifier_root(node, &declaration_tokens);
            }
            if matches!(
                node.kind(),
                "record_type_defn"
                    | "anon_type_defn"
                    | "union_type_defn"
                    | "enum_type_defn"
                    | "type_abbrev_defn"
            ) {
                if let Some(token) = method_children(node)
                    .into_iter()
                    .find(|n| n.kind() == "type_name")
                    .and_then(type_declaration_name)
                    .and_then(|n| self.token(n))
                {
                    self.extract_applied_attributes(
                        node,
                        &self.nominal_owner(token),
                        "type",
                        self.owner_end(token),
                    );
                }
            }
            match node.kind() {
                "prefixed_expression" | "object_expression" => {
                    self.extract_explicit_constructor_head(node)
                }
                "record_type_defn" | "anon_type_defn" => self.extract_record_type(node),
                "application_expression" => {
                    self.extract_expression_type_candidates(node);
                    self.extract_method_call(node);
                    self.extract_pipeline_application(node);
                }
                "infix_expression" => {
                    self.extract_argument_name(node);
                    self.extract_pipeline_application(node);
                }
                "match_expression" => self.extract_match_expression(node),
                "simple_type" | "generic_type" | "postfix_type" => self.extract_type_use(node),
                "union_type_case" => self.extract_union_case(node),
                "union_type_field" => self.extract_union_payload_label(node),
                "enum_type_case" => self.extract_enum_case(node),
                "rule" => self.extract_constructor_pattern(node, &declaration_tokens),
                "type_abbrev_defn" => self.extract_type_alias(node),
                "union_type_defn" => self.extract_ambiguous_alias(node),
                "dot_expression" => self.extract_expression_receiver(node),
                "record_pattern" => self.extract_record_pattern(node),
                "anon_record_expression" => self.extract_anonymous_record(node),
                "typed_pattern" => {
                    let cs = children(node);
                    // The grammar attaches `Some x : R option` and `x,y : T`
                    // to their final subpattern. Without explicit parentheses,
                    // that does not prove an annotation on the final binder.
                    let compound_parent = node.parent().is_some_and(|n| {
                        matches!(
                            n.kind(),
                            "identifier_pattern"
                                | "repeat_pattern"
                                | "conjunct_pattern"
                                | "disjunct_pattern"
                        )
                    });
                    let method_result_wrapper = node.parent().is_some_and(|parent| {
                        parent.kind() == "method_or_prop_defn"
                            && field_children(parent, "args")
                                .iter()
                                .any(|arg| arg.id() == node.id())
                    });
                    let method_tuple_parameter = node
                        .parent()
                        .filter(|p| p.kind() == "repeat_pattern")
                        .and_then(|p| p.parent())
                        .filter(|p| p.kind() == "paren_pattern")
                        .is_some_and(|group| {
                            let mut owner = group.parent();
                            if owner.is_some_and(|n| n.kind() == "typed_pattern") {
                                owner = owner.and_then(|n| n.parent());
                            }
                            owner.is_some_and(|owner| {
                                owner.kind() == "method_or_prop_defn"
                                    && method_argument_groups(owner)
                                        .iter()
                                        .any(|g| g.id() == group.id())
                            })
                        });
                    if (!compound_parent || method_tuple_parameter) && !method_result_wrapper {
                        if let (Some(pattern), Some(ty)) = (
                            cs.iter().find(|n| !is_type_node(n.kind())),
                            cs.iter().find(|n| is_type_node(n.kind())),
                        ) {
                            if let (Some(binding), Some((name, token))) = (
                                simple_pattern_name(*pattern).and_then(|n| self.token(n)),
                                self.named_type(*ty),
                            ) {
                                if declaration_tokens.contains(&binding) {
                                    self.parsed.binding_types.push(BindingTypeFact {
                                        binding,
                                        source: TypeSource::Named { name, token },
                                    });
                                }
                            }
                        }
                    }
                }
                "brace_expression" => self.extract_record_expression(node),
                "function_or_value_defn" => {
                    self.extract_unconstrained_member(node, &declaration_tokens);
                    let cs = children(node);
                    let lefts: Vec<_> = cs
                        .iter()
                        .filter(|n| {
                            matches!(
                                n.kind(),
                                "value_declaration_left" | "function_declaration_left"
                            )
                        })
                        .copied()
                        .collect();
                    let bodies = field_children(node, "body");
                    if lefts.len() == bodies.len() {
                        for (left, body) in lefts.into_iter().zip(bodies) {
                            let annotation = cs
                                .iter()
                                .find(|n| {
                                    is_type_node(n.kind())
                                        && n.start_byte() >= left.end_byte()
                                        && n.end_byte() <= body.start_byte()
                                })
                                .and_then(|n| self.named_type(*n));
                            if left.kind() == "function_declaration_left" {
                                let parts = children(left);
                                if let Some(binding) = parts
                                    .iter()
                                    .find(|n| n.kind() == "identifier")
                                    .and_then(|n| self.token(*n))
                                {
                                    let arity = parts
                                        .iter()
                                        .find(|n| n.kind() == "argument_patterns")
                                        .map(|n| children(*n).len() as u32)
                                        .unwrap_or(0);
                                    if declaration_tokens.contains(&binding) && arity > 0 {
                                        if let Some((type_name, type_token)) = annotation {
                                            self.parsed.function_returns.push(FunctionReturn {
                                                binding,
                                                type_name,
                                                type_token,
                                                arity,
                                            });
                                        }
                                        if let Some(source) = self.expression_type_source(body) {
                                            self.parsed.function_results.push(FunctionResultFact {
                                                binding,
                                                source,
                                                arity,
                                            });
                                        }
                                    }
                                }
                            } else if let Some(binding) = simple_pattern_name(left)
                                .and_then(|n| self.token(n))
                                .filter(|t| declaration_tokens.contains(t))
                            {
                                if let Some((name, token)) = annotation {
                                    self.parsed.binding_types.push(BindingTypeFact {
                                        binding,
                                        source: TypeSource::Named { name, token },
                                    });
                                }
                                if let Some(source) = self.expression_type_source(body) {
                                    self.parsed
                                        .binding_types
                                        .push(BindingTypeFact { binding, source });
                                }
                            }
                        }
                    }
                }
                "value_definition" => {
                    let cs = children(node);
                    let binding = cs
                        .iter()
                        .find(|n| n.kind() == "value_declaration_left")
                        .and_then(|n| simple_pattern_name(*n))
                        .and_then(|n| self.token(n));
                    let spec = cs.iter().find(|n| n.kind() == "curried_spec");
                    if let (Some(binding), Some(spec)) = (binding, spec) {
                        let types = children(*spec);
                        if let Some((type_name, type_token)) =
                            types.last().and_then(|n| self.named_type(*n))
                        {
                            let arity = types.len().saturating_sub(1) as u32;
                            if arity > 0 {
                                self.parsed.function_returns.push(FunctionReturn {
                                    binding,
                                    type_name,
                                    type_token,
                                    arity,
                                });
                            } else {
                                self.parsed.binding_types.push(BindingTypeFact {
                                    binding,
                                    source: TypeSource::Named {
                                        name: type_name,
                                        token: type_token,
                                    },
                                });
                            }
                        }
                    }
                }
                "method_or_prop_defn" | "class_as_reference" => {
                    self.extract_receiver_type(node, &declaration_tokens)
                }
                _ => {}
            }
            let mut cs = children(node);
            cs.reverse();
            stack.extend(cs);
        }
        let expression_members: HashSet<_> = self
            .parsed
            .member_accesses
            .iter()
            .map(|a| a.token)
            .collect();
        for token in 2..self.parsed.tokens.len() {
            if self.parsed.tokens[token].ident
                && self.parsed.tokens[token - 1].text == "."
                && self.parsed.tokens[token - 2].ident
                && !self.non_value.contains(&token)
            {
                if !expression_members.contains(&token) {
                    self.parsed.member_accesses.push(MemberAccess {
                        token,
                        receiver: token - 2,
                        source: None,
                    });
                }
            }
        }
    }
    fn extract_applied_attributes(
        &mut self,
        node: Node<'_>,
        owner: &str,
        owner_kind: &str,
        scope_end: usize,
    ) {
        let mut groups: Vec<_> = method_children(node)
            .into_iter()
            .filter(|n| n.kind() == "attributes")
            .collect();
        if owner_kind == "type" {
            if let Some(parent) = node.parent().filter(|n| n.kind() == "type_definition") {
                let siblings = method_children(parent);
                groups.extend(siblings.iter().copied().filter(|n| {
                    n.kind() == "attributes"
                        && n.end_byte() <= node.start_byte()
                        && !siblings.iter().any(|other| {
                            other.id() != node.id()
                                && other.start_byte() > n.end_byte()
                                && other.start_byte() < node.start_byte()
                                && other.kind().ends_with("_defn")
                        })
                }));
            }
        }
        for group in groups {
            for attribute in method_children(group) {
                let Some(ty) = method_children(attribute).into_iter().next() else {
                    continue;
                };
                let Some(token) = identifier_descendants(ty)
                    .into_iter()
                    .next()
                    .and_then(|n| self.token(n))
                else {
                    continue;
                };
                self.parsed.attributes.push(AppliedAttribute {
                    type_name: raw(ty, self.source).trim().to_owned(),
                    token,
                    owner: owner.to_owned(),
                    owner_kind: owner_kind.to_owned(),
                    owner_start: node.start_byte(),
                    owner_end: node.end_byte(),
                    scope_end,
                });
            }
        }
    }
    fn extract_expression_receiver(&mut self, node: Node<'_>) {
        let (Some(base), Some(field)) = (
            node.child_by_field_name("base"),
            node.child_by_field_name("field"),
        ) else {
            return;
        };
        // Delimiters are deliberately coalesced in the token stream. Keep the
        // parser's actual receiver start even when its type is still unknown.
        if let Some((names, _)) = self.expression_name(field) {
            let first = names[0];
            if first >= 2
                && self.parsed.tokens[first - 1].text == "."
                && !self.parsed.tokens[first - 2].ident
            {
                let point = base.start_position();
                let byte = base.start_byte();
                let column = self.source[byte - point.column..byte]
                    .encode_utf16()
                    .count();
                self.parsed.expression_receiver_starts.push((
                    first,
                    crate::syntax::Position {
                        line: point.row as u32,
                        character: column as u32,
                    },
                ));
            }
        }
        if !matches!(
            base.kind(),
            "paren_expression"
                | "application_expression"
                | "typecast_expression"
                | "prefixed_expression"
        ) {
            return;
        }
        let ids = identifier_descendants(field);
        if ids.len() != 1 {
            return;
        }
        let Some(token) = self.token(ids[0]) else {
            return;
        };
        let Some(source) = self.expression_type_source(base) else {
            return;
        };
        let receiver = match &source {
            TypeSource::Named { token, .. } | TypeSource::Alias { token } => *token,
            TypeSource::FunctionResult { function, .. } => *function,
            TypeSource::Record { fields, copy } => {
                copy.or_else(|| fields.first().copied()).unwrap_or(token)
            }
        };
        self.parsed.member_accesses.push(MemberAccess {
            token,
            receiver,
            source: Some(source),
        });
    }
    fn extract_method_call(&mut self, node: Node<'_>) {
        if self.intersects_literal_island(node) {
            return;
        }
        let cs = method_children(node);
        if cs.len() != 2 {
            return;
        }
        let Some(member) = self.method_callee_token(cs[0]) else {
            return;
        };
        if member < 2
            || self.parsed.tokens[member - 1].text != "."
            || !self.parsed.tokens[member - 2].ident
        {
            return;
        }
        let arg = unwrap_expression(cs[1]);
        let args = if arg.kind() == "tuple_expression" {
            method_children(arg)
        } else {
            vec![arg]
        };
        let args = args.into_iter().map(|n| self.literal_argument(n)).collect();
        self.parsed.method_calls.push(MethodCall {
            member,
            receiver: member - 2,
            args,
        });
    }
    fn method_callee_token(&self, mut node: Node<'_>) -> Option<usize> {
        let mut member = None;
        // Longer written qualifiers may be represented as dot_expression
        // rather than one long_identifier. Preserve only this name chain;
        // an expression receiver requires a separate type proof.
        for _ in 0..64 {
            if let Some(token) = self.simple_value_token(node) {
                return Some(member.unwrap_or(token));
            }
            if node.kind() != "dot_expression" {
                return None;
            }
            let field = node.child_by_field_name("field")?;
            let token = self.simple_value_token(field)?;
            member.get_or_insert(token);
            node = node.child_by_field_name("base")?;
        }
        None
    }
    fn literal_argument(&self, node: Node<'_>) -> ArgumentSource {
        let node = unwrap_expression(node);
        if node.kind() != "const" {
            return ArgumentSource::Unknown;
        }
        let cs = method_children(node);
        if cs.len() != 1 {
            return ArgumentSource::Unknown;
        }
        let value = raw(cs[0], self.source);
        match cs[0].kind() {
            "int"
                if !value.starts_with('+')
                    && value
                        .strip_prefix('-')
                        .unwrap_or(value)
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == '_')
                    && value.replace('_', "").parse::<i32>().is_ok() =>
            {
                ArgumentSource::IntrinsicLiteral(IntrinsicLiteral::UnsuffixedDecimalInt32)
            }
            "string" if !value.starts_with('$') && !value.contains("${") => {
                ArgumentSource::IntrinsicLiteral(IntrinsicLiteral::String)
            }
            _ => ArgumentSource::Unknown,
        }
    }
    fn extract_method_headers(&mut self, node: Node<'_>, type_token: usize) {
        let owner = self.nominal_owner(type_token);
        let generic_owner = method_children(node)
            .into_iter()
            .find(|n| n.kind() == "type_name")
            .is_some_and(|n| !type_parameters(n, self.source).is_empty());
        let mut pending = vec![(node, false)];
        while let Some((container, in_interface)) = pending.pop() {
            if crate::cancellation::is_cancelled() {
                return;
            }
            for child in method_children(container) {
                if child.kind() == "member_defn" {
                    let mut cursor = child.walk();
                    let is_static = child.children(&mut cursor).any(|n| n.kind() == "static");
                    let mut cursor = child.walk();
                    let forbidden = child.children(&mut cursor).any(|n| {
                        matches!(
                            n.kind(),
                            "inline" | "abstract" | "override" | "default" | "attributes"
                        )
                    });
                    let attributed = method_children(container)
                        .iter()
                        .any(|n| n.kind() == "attributes");
                    for header in method_children(child) {
                        let name = match header.kind() {
                            "method_or_prop_defn" => {
                                header.child_by_field_name("name").and_then(|n| {
                                    n.child_by_field_name("method").or_else(|| {
                                        method_children(n)
                                            .into_iter()
                                            .find(|n| n.kind() == "identifier")
                                    })
                                })
                            }
                            "member_signature" | "property_or_ident" => method_children(header)
                                .into_iter()
                                .find(|n| n.kind() == "identifier"),
                            _ => None,
                        };
                        let Some(token) = name.and_then(|n| self.token(n)) else {
                            continue;
                        };
                        let mut supported = header.kind() == "method_or_prop_defn"
                            && is_static
                            && !generic_owner
                            && !in_interface
                            && !forbidden
                            && !attributed
                            && !header.has_error()
                            && !method_children(header).iter().any(|n| {
                                matches!(
                                    n.kind(),
                                    "type_arguments" | "type_constraints" | "attributes"
                                )
                            });
                        let groups = method_argument_groups(header);
                        let mut parameters = Vec::new();
                        if groups.len() != 1 || groups[0].kind() != "paren_pattern" {
                            supported = false;
                        } else {
                            let parts = method_children(groups[0]);
                            let patterns =
                                if parts.len() == 1 && parts[0].kind() == "repeat_pattern" {
                                    method_children(parts[0])
                                } else {
                                    parts
                                };
                            if patterns.is_empty() {
                                supported = false;
                            }
                            for pattern in patterns {
                                let parts = method_children(pattern);
                                if pattern.kind() != "typed_pattern"
                                    || parts.len() != 2
                                    || parts[0].kind() != "identifier_pattern"
                                    || simple_pattern_name(parts[0]).is_none()
                                    || parts[1].kind() != "simple_type"
                                {
                                    supported = false;
                                    continue;
                                }
                                if let Some((type_name, token)) = self.named_type(parts[1]) {
                                    parameters.push(DeclaredParameterType {
                                        type_name,
                                        token,
                                        binding: simple_pattern_name(parts[0])
                                            .and_then(|n| self.token(n)),
                                    });
                                } else {
                                    supported = false;
                                }
                            }
                        }
                        self.parsed.method_declarations.push(MethodDeclaration {
                            owner: owner.clone(),
                            type_token,
                            name: self.parsed.tokens[token].text.clone(),
                            token,
                            is_static,
                            parameters,
                            supported,
                            accessibility: declaration_accessibility(header, self.source).into(),
                        });
                    }
                } else if matches!(
                    child.kind(),
                    "type_extension_elements" | "interface_implementation"
                ) {
                    pending.push((
                        child,
                        in_interface || child.kind() == "interface_implementation",
                    ));
                }
            }
        }
    }
    fn extract_constructor_pattern(&mut self, node: Node<'_>, declarations: &HashSet<usize>) {
        let Some(pattern) = node
            .child_by_field_name("pattern")
            .filter(|n| n.kind() == "identifier_pattern")
        else {
            return;
        };
        let Some(matched) = node
            .parent()
            .and_then(|n| n.parent())
            .filter(|n| n.kind() == "match_expression")
        else {
            return;
        };
        let Some(scrutinee) = method_children(matched)
            .into_iter()
            .find(|n| n.kind() != "rules")
            .and_then(|n| self.simple_value_token(n))
        else {
            return;
        };
        let parts = method_children(pattern);
        let Some(first) = parts.first() else {
            return;
        };
        let Some(constructor) = self.simple_value_token(*first) else {
            return;
        };
        for (argument_index, arg) in parts.into_iter().skip(1).enumerate() {
            if let Some(binding) = simple_pattern_name(arg)
                .and_then(|n| self.token(n))
                .filter(|t| declarations.contains(t))
            {
                self.parsed.pattern_bindings.push(PatternBinding {
                    binding,
                    scrutinee,
                    constructor,
                    argument_index: argument_index as u32,
                });
            }
        }
    }
    fn extract_enum_case(&mut self, node: Node<'_>) {
        if node.has_error() {
            return;
        }
        let Some(token) = method_children(node)
            .into_iter()
            .find(|n| n.kind() == "identifier")
            .and_then(|n| self.token(n))
        else {
            return;
        };
        let mut owner = node.parent();
        while owner.is_some_and(|n| n.kind() != "enum_type_defn") {
            owner = owner.and_then(|n| n.parent());
        }
        let Some(owner) = owner else {
            return;
        };
        let Some(header) = method_children(owner)
            .into_iter()
            .find(|n| n.kind() == "type_name")
        else {
            return;
        };
        let Some(type_token) = type_declaration_name(header).and_then(|n| self.token(n)) else {
            return;
        };
        self.parsed.pattern_declarations.push(PatternDeclaration {
            name: self.parsed.tokens[token].text.clone(),
            token,
            module: self.parsed.module_at(token).unwrap_or("").to_owned(),
            kind: "enum-case".into(),
            top_level: true,
            scope_start: owner.start_byte(),
            scope_end: self.owner_end(token),
            type_owner: Some(self.nominal_owner(type_token)),
            qualified_only: true,
            accessibility: declaration_accessibility(header, self.source).into(),
        });
    }
    fn extract_union_payload_label(&mut self, node: Node<'_>) {
        if node.has_error() {
            return;
        }
        let mut cursor = node.walk();
        let Some(colon) = node
            .children(&mut cursor)
            .find(|n| !n.is_named() && n.kind() == ":")
        else {
            return;
        };
        let labels = method_children(node)
            .into_iter()
            .filter(|n| n.kind() == "identifier" && n.end_byte() <= colon.start_byte())
            .collect::<Vec<_>>();
        if labels.len() == 1 {
            if let Some(token) = self.token(labels[0]) {
                self.parsed.union_payload_labels.push(token);
            }
        }
    }
    fn extract_union_case(&mut self, node: Node<'_>) {
        let Some(token) = method_children(node)
            .into_iter()
            .find(|n| n.kind() == "identifier")
            .and_then(|n| self.token(n))
        else {
            return;
        };
        let mut owner = node.parent();
        while owner.is_some_and(|n| n.kind() != "union_type_defn") {
            owner = owner.and_then(|n| n.parent());
        }
        let Some(owner) = owner else {
            return;
        };
        let Some(header) = method_children(owner)
            .into_iter()
            .find(|n| n.kind() == "type_name")
        else {
            return;
        };
        let Some(type_token) = type_declaration_name(header).and_then(|n| self.token(n)) else {
            return;
        };
        let mut fields = Vec::new();
        let mut supported = !node.has_error();
        if let Some(payload) = method_children(node)
            .into_iter()
            .find(|n| n.kind() == "union_type_fields")
        {
            for field in method_children(payload) {
                let types = method_children(field)
                    .into_iter()
                    .filter(|n| is_type_node(n.kind()))
                    .collect::<Vec<_>>();
                if field.kind() != "union_type_field" || types.len() != 1 {
                    supported = false;
                    continue;
                }
                if let Some((type_name, token)) = self.named_type(types[0]) {
                    fields.push(DeclaredParameterType {
                        type_name,
                        token,
                        binding: None,
                    });
                } else {
                    supported = false;
                }
            }
            supported &= !fields.is_empty();
        }
        self.parsed.union_case_payloads.push(UnionCasePayload {
            case_token: token,
            type_parameters: type_parameters(header, self.source),
            fields,
            supported,
        });
        self.parsed.pattern_declarations.push(PatternDeclaration {
            name: self.parsed.tokens[token].text.clone(),
            token,
            module: self.parsed.module_at(token).unwrap_or("").to_owned(),
            kind: "union-case".into(),
            top_level: true,
            scope_start: owner.start_byte(),
            scope_end: self.owner_end(token),
            type_owner: Some(self.nominal_owner(type_token)),
            qualified_only: false,
            accessibility: declaration_accessibility(node, self.source).into(),
        });
    }
    fn extract_anonymous_record(&mut self, node: Node<'_>) {
        if self.intersects_literal_island(node) {
            return;
        }
        let mut pending: Vec<_> = children(node)
            .into_iter()
            .filter(|n| matches!(n.kind(), "field_initializers" | "with_field_expression"))
            .collect();
        while let Some(n) = pending.pop() {
            if n.kind() == "field_initializer" {
                if let Some(token) = n
                    .child_by_field_name("field")
                    .and_then(|n| identifier_descendants(n).last().copied())
                    .and_then(|n| self.token(n))
                {
                    self.parsed.anonymous_record_labels.push(token);
                }
            } else {
                pending.extend(
                    children(n)
                        .into_iter()
                        .filter(|n| matches!(n.kind(), "field_initializers" | "field_initializer")),
                );
            }
        }
        let mut expression = node;
        while expression
            .parent()
            .is_some_and(|n| n.kind() == "paren_expression")
        {
            expression = expression.parent().unwrap();
        }
        if let Some(definition) = expression
            .parent()
            .filter(|n| n.kind() == "function_or_value_defn")
        {
            let cs = children(definition);
            let bodies = field_children(definition, "body");
            if bodies.len() != 1
                || bodies[0].id() != expression.id()
                || cs
                    .iter()
                    .any(|n| is_type_node(n.kind()) && n.end_byte() <= expression.start_byte())
            {
                return;
            }
            let left = cs.iter().find(|n| n.kind() == "value_declaration_left");
            if left.is_some_and(|left| {
                let mut nodes = vec![*left];
                while let Some(n) = nodes.pop() {
                    if is_type_node(n.kind()) {
                        return true;
                    }
                    nodes.extend(children(n));
                }
                false
            }) {
                return;
            }
            if let Some(binding) = left
                .and_then(|n| simple_pattern_name(*n))
                .and_then(|n| self.token(n))
            {
                self.parsed.anonymous_record_bindings.push(binding);
            }
        }
    }
    fn extract_record_pattern(&mut self, node: Node<'_>) {
        if self.intersects_literal_island(node) {
            return;
        }
        let mut annotation = None;
        let mut copy = None;
        if let Some(parent) = node.parent() {
            if parent.kind() == "typed_pattern" {
                annotation = children(parent)
                    .into_iter()
                    .find(|n| is_type_node(n.kind()))
                    .and_then(|n| self.named_type(n));
            } else if parent.kind() == "rule"
                && parent
                    .child_by_field_name("pattern")
                    .is_some_and(|n| n.id() == node.id())
            {
                let matched = parent
                    .parent()
                    .and_then(|n| n.parent())
                    .filter(|n| n.kind() == "match_expression");
                copy = matched
                    .and_then(|n| children(n).into_iter().find(|n| n.kind() != "rules"))
                    .and_then(|n| self.simple_value_token(n));
            }
        }
        if annotation.is_none() && copy.is_none() {
            return;
        }
        let mut fields = Vec::new();
        for field in children(node) {
            if field.kind() != "field_pattern" {
                return;
            }
            let Some(label) = children(field)
                .into_iter()
                .find(|n| n.kind() == "long_identifier")
            else {
                return;
            };
            let Some(token) = identifier_descendants(label)
                .last()
                .and_then(|n| self.token(*n))
            else {
                return;
            };
            fields.push(token);
            if let Some(binding) = children(field)
                .into_iter()
                .skip(1)
                .find_map(simple_pattern_name)
                .and_then(|n| self.token(n))
            {
                self.parsed
                    .record_pattern_bindings
                    .push(RecordPatternBinding {
                        binding,
                        field: token,
                    });
            }
        }
        if fields.is_empty() {
            return;
        }
        self.parsed.record_expressions.push(RecordExpression {
            fields,
            copy,
            binding: None,
            parent_field: None,
            method_result: None,
            fresh_method_result: false,
            method_result_eligible: false,
            method_result_literal_group: false,
            factory_atoms: Vec::new(),
            method_result_uses: Vec::new(),
            argument: None,
            type_name: annotation.as_ref().map(|x| x.0.clone()),
            type_token: annotation.map(|x| x.1),
        });
    }
    fn extract_ambiguous_alias(&mut self, node: Node<'_>) {
        // The grammar may attach the next declaration's XML documentation to
        // this definition. Trivia does not turn a bare alias candidate into a
        // different type shape; semantic RHS resolution is still required.
        let cs = method_children(node);
        if cs
            .iter()
            .any(|n| !matches!(n.kind(), "type_name" | "union_type_cases"))
        {
            return;
        }
        let (Some(header), Some(cases)) = (
            cs.iter().find(|n| n.kind() == "type_name"),
            cs.iter().find(|n| n.kind() == "union_type_cases"),
        ) else {
            return;
        };
        if children(*header)
            .iter()
            .any(|n| matches!(n.kind(), "type_argument_constraints" | "type_arguments"))
        {
            return;
        }
        if raw(*cases, self.source).trim_start().starts_with('|') {
            return;
        }
        let cases = method_children(*cases);
        if cases.len() != 1 || cases[0].kind() != "union_type_case" {
            return;
        }
        let target = method_children(cases[0]);
        if target.len() != 1 || target[0].kind() != "identifier" {
            return;
        }
        let (Some(token), Some(target_token)) = (
            type_declaration_name(*header).and_then(|n| self.token(n)),
            self.token(target[0]),
        ) else {
            return;
        };
        // This is a candidate only: F# resolves this ambiguous syntax as an
        // abbreviation only if the RHS resolves to an existing nominal type.
        self.parsed.type_aliases.push(TypeAlias {
            name: self.nominal_owner(token),
            token,
            type_parameters: Vec::new(),
            target: raw(target[0], self.source).to_owned(),
            target_token,
            accessibility: declaration_accessibility(*header, self.source).into(),
            start: node.start_byte(),
            scope_end: self.owner_end(token),
        });
    }
    fn extract_type_alias(&mut self, node: Node<'_>) {
        let cs = children(node);
        let Some(name_node) = cs
            .iter()
            .find(|n| n.kind() == "type_name")
            .and_then(|n| type_declaration_name(*n))
        else {
            return;
        };
        let Some(token) = self.token(name_node) else {
            return;
        };
        let Some((target, target_token)) = cs
            .iter()
            .find(|n| is_type_node(n.kind()))
            .and_then(|n| self.named_type(*n))
        else {
            return;
        };
        self.parsed.type_aliases.push(TypeAlias {
            name: self.nominal_owner(token),
            token,
            type_parameters: cs
                .iter()
                .find(|n| n.kind() == "type_name")
                .map(|n| type_parameters(*n, self.source))
                .unwrap_or_default(),
            target,
            target_token,
            accessibility: declaration_accessibility(name_node, self.source).into(),
            start: node.start_byte(),
            scope_end: self.owner_end(token),
        });
    }
    fn extract_record_expression(&mut self, node: Node<'_>) {
        let Some(TypeSource::Record { fields, copy }) = self.expression_type_source(node) else {
            return;
        };
        let mut expression = node;
        while expression
            .parent()
            .is_some_and(|n| n.kind() == "paren_expression")
        {
            expression = expression.parent().unwrap();
        }
        let mut binding = None;
        let mut annotation = None;
        let mut parent_field = None;
        if let Some(parent) = expression.parent() {
            if parent.kind() == "typecast_expression" {
                annotation = children(parent)
                    .into_iter()
                    .find(|n| is_type_node(n.kind()))
                    .and_then(|n| self.named_type(n));
            } else if parent.kind() == "field_initializer"
                && parent
                    .child_by_field_name("value")
                    .is_some_and(|n| n.id() == expression.id())
            {
                parent_field = parent
                    .child_by_field_name("field")
                    .and_then(|n| identifier_descendants(n).last().copied())
                    .and_then(|n| self.token(n));
            } else if parent.kind() == "function_or_value_defn" {
                let cs = children(parent);
                let lefts: Vec<_> = cs
                    .iter()
                    .filter(|n| {
                        matches!(
                            n.kind(),
                            "value_declaration_left" | "function_declaration_left"
                        )
                    })
                    .copied()
                    .collect();
                let bodies = field_children(parent, "body");
                for (left, body) in lefts.into_iter().zip(bodies) {
                    if body.id() == expression.id() {
                        if left.kind() == "value_declaration_left" {
                            binding = simple_pattern_name(left).and_then(|n| self.token(n));
                        } else {
                            binding = children(left)
                                .into_iter()
                                .find(|n| n.kind() == "identifier")
                                .and_then(|n| self.token(n));
                        }
                        annotation = cs
                            .iter()
                            .find(|n| {
                                is_type_node(n.kind())
                                    && n.start_byte() >= left.end_byte()
                                    && n.end_byte() <= body.start_byte()
                            })
                            .and_then(|n| self.named_type(*n));
                        break;
                    }
                }
            }
        }
        let context = self.record_method_result(expression).unwrap_or_default();
        let method_result_eligible = annotation.is_none() && context.eligible;
        let fresh_method_result = method_result_eligible && context.uses.is_empty();
        if annotation.is_none() {
            annotation = context.annotation;
        }
        self.parsed.record_expressions.push(RecordExpression {
            fields,
            copy,
            binding,
            parent_field,
            method_result: context.token,
            fresh_method_result,
            method_result_eligible,
            method_result_literal_group: context.literal_group,
            factory_atoms: context.factory_atoms,
            method_result_uses: context.uses,
            argument: self.record_argument_context(expression),
            type_name: annotation.as_ref().map(|x| x.0.clone()),
            type_token: annotation.map(|x| x.1),
        });
    }
    fn record_argument_context(&self, mut expression: Node<'_>) -> Option<CallArgumentContext> {
        let mut position = 0;
        let mut arity = 1;
        if expression
            .parent()
            .is_some_and(|n| n.kind() == "tuple_expression")
        {
            let argument = expression;
            while expression
                .parent()
                .is_some_and(|n| n.kind() == "tuple_expression")
            {
                expression = expression.parent()?;
            }
            // Commas form a binary grammar spine. Parenthesized tuple values
            // remain one argument; no curried application or branch is flattened.
            let mut pending = vec![expression];
            let mut slots = Vec::new();
            while let Some(node) = pending.pop() {
                if slots.len() + pending.len() > 32 {
                    return None;
                }
                if node.kind() == "tuple_expression" {
                    let mut cs = method_children(node);
                    if cs.len() != 2 {
                        return None;
                    }
                    cs.reverse();
                    pending.extend(cs);
                } else {
                    slots.push(node);
                }
            }
            position = slots.iter().position(|n| n.id() == argument.id())? as u32;
            arity = slots.len() as u32;
        }
        while expression
            .parent()
            .is_some_and(|n| n.kind() == "paren_expression" && method_children(n).len() == 1)
        {
            expression = expression.parent()?;
        }
        let call = expression.parent()?;
        if call.kind() != "application_expression"
            || call.has_error()
            || self.intersects_literal_island(call)
        {
            return None;
        }
        let cs = method_children(call);
        if cs.len() != 2 || cs[1].id() != expression.id() {
            return None;
        }
        let callee = self.simple_value_token(cs[0])?;
        Some(CallArgumentContext {
            callee,
            position,
            arity,
            call_start: call.start_byte(),
            call_end: call.end_byte(),
        })
    }
    fn literal_value_tree(&self, root: Node<'_>) -> bool {
        let mut pending = vec![root];
        let mut budget = 2048usize;
        while let Some(node) = pending.pop() {
            if budget == 0
                || node.has_error()
                || node.is_missing()
                || self.intersects_literal_island(node)
                || crate::cancellation::is_cancelled()
            {
                return false;
            }
            budget -= 1;
            let cs = method_children(node);
            match node.kind() {
                "brace_expression" => {
                    if cs.len() != 1 || cs[0].kind() != "field_initializers" {
                        return false;
                    }
                    pending.push(cs[0]);
                }
                "field_initializers" => {
                    if cs.is_empty() || cs.iter().any(|n| n.kind() != "field_initializer") {
                        return false;
                    }
                    pending.extend(cs);
                }
                "field_initializer" => {
                    let (Some(label), Some(value)) = (
                        node.child_by_field_name("field"),
                        node.child_by_field_name("value"),
                    ) else {
                        return false;
                    };
                    if cs.len() != 2
                        || label.kind() != "long_identifier"
                        || label.has_error()
                        || method_children(label)
                            .iter()
                            .any(|n| n.kind() != "identifier")
                    {
                        return false;
                    }
                    pending.push(value);
                }
                "paren_expression" => {
                    if cs.len() != 1 {
                        return false;
                    }
                    pending.push(cs[0]);
                }
                "tuple_expression" => {
                    if cs.len() < 2 {
                        return false;
                    }
                    pending.extend(cs);
                }
                "list_expression" | "array_expression" => pending.extend(cs),
                "const" => {
                    if cs.len() != 1 {
                        return false;
                    }
                    let literal = cs[0];
                    // The string wrappers may contain format/interpolation
                    // nodes. Only a genuinely opaque literal is admitted.
                    if !method_children(literal).is_empty() {
                        return false;
                    }
                    match literal.kind() {
                        "string" | "verbatim_string" | "triple_quoted_string" | "bool" | "unit" => {
                        }
                        "int" => {
                            let text = raw(literal, self.source).as_bytes();
                            if !text.first().is_some_and(u8::is_ascii_digit)
                                || !text.last().is_some_and(u8::is_ascii_digit)
                                || text.iter().any(|b| !b.is_ascii_digit() && *b != b'_')
                            {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                }
                _ => return false,
            }
        }
        true
    }
    // This additional envelope admits only plain attribute constructors with
    // no arguments or one decimal constant. A computed/type argument could
    // itself participate in inference of the checking group.
    fn data_attributes(&self, node: Node<'_>) -> bool {
        if node.kind() != "attributes" {
            return false;
        }
        let attrs = method_children(node);
        !attrs.is_empty()
            && attrs.len() <= 16
            && attrs.iter().all(|a| {
                let parts = method_children(*a);
                if a.kind() != "attribute" || parts.is_empty() || parts[0].kind() != "simple_type" {
                    return false;
                }
                let name = raw(parts[0], self.source);
                if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return false;
                }
                match parts.as_slice() {
                    [_] => true,
                    [_, arg] if arg.kind() == "paren_expression" => {
                        let value = unwrap_expression(*arg);
                        let number = raw(value, self.source);
                        let digits = number.strip_suffix('u').unwrap_or(number);
                        value.kind() == "const"
                            && !digits.is_empty()
                            && digits.bytes().all(|b| b.is_ascii_digit())
                    }
                    _ => false,
                }
            })
    }

    fn forwarding_atoms(&self, body: Node<'_>) -> Option<Vec<(usize, FactoryAtom)>> {
        if body.kind() != "brace_expression" || self.intersects_literal_island(body) {
            return None;
        }
        let children = method_children(body);
        let [initializers] = children.as_slice() else {
            return None;
        };
        if initializers.kind() != "field_initializers" {
            return None;
        }
        let fields = method_children(*initializers);
        if fields.is_empty() || fields.len() > 32 {
            return None;
        }
        let mut atoms = Vec::new();
        for field in fields {
            if field.kind() != "field_initializer" || method_children(field).len() != 2 {
                return None;
            }
            let label = field.child_by_field_name("field")?;
            let value = field.child_by_field_name("value")?;
            let label_token = self.simple_value_token(label)?;
            if self.parsed.tokens[label_token].text != raw(label, self.source) {
                return None;
            }
            let atom = match self.literal_argument(value) {
                ArgumentSource::IntrinsicLiteral(lit) => FactoryAtom::Literal(lit),
                ArgumentSource::Unknown => {
                    let token = self.simple_value_token(value)?;
                    let name = raw(value, self.source);
                    if name != self.parsed.tokens[token].text
                        || !name.as_bytes().first()?.is_ascii_lowercase()
                        || !name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'\'')
                    {
                        return None;
                    }
                    FactoryAtom::Identifier(token)
                }
            };
            atoms.push((label_token, atom));
        }
        atoms
            .iter()
            .any(|(_, a)| matches!(a, FactoryAtom::Identifier(_)))
            .then_some(atoms)
    }

    fn closed_method_group(
        &self,
        group: Node<'_>,
        owner: Node<'_>,
        member: Node<'_>,
        body: Node<'_>,
        allow_data_attributes: bool,
    ) -> bool {
        if group.kind() != "type_definition"
            || group.has_error()
            || group.is_missing()
            || !matches!(owner.kind(), "record_type_defn" | "anon_type_defn")
        {
            return false;
        }
        let groups = method_children(group);
        if !groups.iter().any(|n| n.id() == owner.id())
            || groups.iter().any(|n| {
                n.id() != owner.id() && !(allow_data_attributes && self.data_attributes(*n))
            })
        {
            return false;
        }
        let parts = method_children(owner);
        let Some(owner_name) = parts
            .iter()
            .find(|n| n.kind() == "type_name")
            .and_then(|n| type_declaration_name(*n))
        else {
            return false;
        };
        let Some(owner_name) = self
            .token(owner_name)
            .map(|t| self.parsed.tokens[t].text.as_str())
        else {
            return false;
        };
        let mut names = 0;
        let mut extensions = 0;
        for part in parts {
            match part.kind() {
                "type_name" => {
                    names += 1;
                    if method_children(part).iter().any(|n| {
                        !matches!(
                            n.kind(),
                            "identifier" | "long_identifier" | "access_modifier"
                        )
                    }) {
                        return false;
                    }
                }
                "record_fields" => {
                    let mut pending = vec![part];
                    let mut budget = 2048usize;
                    while let Some(node) = pending.pop() {
                        if budget == 0
                            || node.has_error()
                            || node.is_missing()
                            || crate::cancellation::is_cancelled()
                        {
                            return false;
                        }
                        budget -= 1;
                        if allow_data_attributes && self.data_attributes(node) {
                            continue;
                        }
                        // Closed declaration syntax only. Generic constraints,
                        // attributes/static arguments and opaque type forms
                        // are deliberately outside this first certificate.
                        if !matches!(
                            node.kind(),
                            "record_fields"
                                | "record_field"
                                | "identifier"
                                | "long_identifier"
                                | "simple_type"
                        ) {
                            return false;
                        }
                        if node.kind() == "identifier"
                            && self
                                .token(node)
                                .is_some_and(|t| self.parsed.tokens[t].text == owner_name)
                        {
                            return false;
                        }
                        pending.extend(method_children(node));
                    }
                }
                "type_extension_elements" => {
                    extensions += 1;
                    let members = method_children(part);
                    if members.len() != 1
                        || members[0].id() != member.id()
                        || member.kind() != "member_defn"
                    {
                        return false;
                    }
                    // Parameter type syntax can transmit constraints too. This
                    // envelope admits only ordinary static headers with simple
                    // nominal parameter types, excluding the owner itself.
                    let mut pending = vec![member];
                    let mut budget = 2048usize;
                    while let Some(node) = pending.pop() {
                        if node.id() == body.id() {
                            continue;
                        }
                        if budget == 0
                            || node.has_error()
                            || node.is_missing()
                            || crate::cancellation::is_cancelled()
                        {
                            return false;
                        }
                        budget -= 1;
                        if !matches!(
                            node.kind(),
                            "member_defn"
                                | "method_or_prop_defn"
                                | "property_or_ident"
                                | "identifier"
                                | "long_identifier"
                                | "argument_patterns"
                                | "paren_pattern"
                                | "repeat_pattern"
                                | "typed_pattern"
                                | "identifier_pattern"
                                | "long_identifier_or_op"
                                | "simple_type"
                                | "const"
                                | "unit"
                                | "access_modifier"
                        ) {
                            return false;
                        }
                        if node.kind() == "identifier"
                            && self
                                .token(node)
                                .is_some_and(|t| self.parsed.tokens[t].text == owner_name)
                        {
                            return false;
                        }
                        pending.extend(method_children(node));
                    }
                }
                "access_modifier" => {}
                _ => return false,
            }
        }
        names == 1 && extensions == 1
    }
    fn record_method_result(&mut self, mut expression: Node<'_>) -> Option<RecordMethodContext> {
        let method = loop {
            let parent = expression.parent()?;
            match parent.kind() {
                "paren_expression" if method_children(parent).len() == 1 => expression = parent,
                "sequential_expression"
                    if method_children(parent)
                        .last()
                        .is_some_and(|n| n.id() == expression.id()) =>
                {
                    expression = parent
                }
                "declaration_expression"
                    if parent
                        .child_by_field_name("in")
                        .is_some_and(|n| n.id() == expression.id()) =>
                {
                    expression = parent
                }
                "method_or_prop_defn" => break parent,
                _ => return None,
            }
        };
        let mut cursor = method.walk();
        let equals = method.children(&mut cursor).find(|n| n.kind() == "=")?;
        let bodies: Vec<_> = method_children(method)
            .into_iter()
            .filter(|n| n.start_byte() >= equals.end_byte())
            .collect();
        if bodies.len() != 1 || bodies[0].id() != expression.id() {
            return None;
        }
        let name = method.child_by_field_name("name")?;
        let name = name.child_by_field_name("method").or_else(|| {
            method_children(name)
                .into_iter()
                .find(|n| n.kind() == "identifier")
        })?;
        let token = self.token(name)?;
        let annotation = method_children(method)
            .into_iter()
            .filter(|n| n.end_byte() <= equals.start_byte())
            .find_map(|n| {
                if is_type_node(n.kind()) {
                    self.named_type(n)
                } else if n.kind() == "typed_pattern" {
                    method_children(n)
                        .into_iter()
                        .find(|n| is_type_node(n.kind()))
                        .and_then(|n| self.named_type(n))
                } else {
                    None
                }
            });
        let member = method.parent()?;
        let mut owner = member;
        while !matches!(owner.kind(), "record_type_defn" | "anon_type_defn") {
            owner = owner.parent()?;
            if matches!(owner.kind(), "file" | "named_module" | "type_extension") {
                return None;
            }
        }
        let group = owner.parent()?;
        let mut fresh = annotation.is_none()
            && member.kind() == "member_defn"
            && !method.has_error()
            && group.kind() == "type_definition"
            && method_children(group)
                .iter()
                .filter(|n| n.kind().ends_with("_type_defn") || n.kind() == "type_abbrev_defn")
                .count()
                == 1;
        let mut cursor = group.walk();
        fresh &= !group.children(&mut cursor).any(|n| n.kind() == "and");
        let mut ancestor = group.parent();
        while let Some(node) = ancestor {
            if matches!(node.kind(), "named_module" | "module_defn" | "namespace") {
                let mut cursor = node.walk();
                fresh &= !node.children(&mut cursor).any(|n| n.kind() == "rec");
            }
            ancestor = node.parent();
        }
        let mut cursor = member.walk();
        fresh &= member.children(&mut cursor).any(|n| n.kind() == "static");
        let mut cursor = member.walk();
        fresh &= !member.children(&mut cursor).any(|n| {
            matches!(
                n.kind(),
                "inline" | "abstract" | "override" | "default" | "attributes"
            )
        });
        fresh &= !member
            .parent()
            .is_some_and(|n| method_children(n).iter().any(|n| n.kind() == "attributes"));
        fresh &= !method_children(method).iter().any(|n| {
            matches!(
                n.kind(),
                "type_arguments" | "type_constraints" | "attributes"
            )
        });
        fresh &= method_children(owner)
            .into_iter()
            .find(|n| n.kind() == "type_name")
            .is_some_and(|n| type_parameters(n, self.source).is_empty());
        let mut cursor = owner.walk();
        fresh &= owner.children(&mut cursor).any(|n| n.kind() == "=");
        let groups = method_argument_groups(method);
        let unit = groups.len() == 1
            && groups[0].kind() == "const"
            && method_children(groups[0])
                .first()
                .is_some_and(|n| n.kind() == "unit");
        fresh &=
            groups.is_empty() || unit || (groups.len() == 1 && groups[0].kind() == "paren_pattern");
        if fresh && !groups.is_empty() && !unit {
            let parts = method_children(groups[0]);
            let patterns = if parts.len() == 1 && parts[0].kind() == "repeat_pattern" {
                method_children(parts[0])
            } else {
                parts
            };
            fresh &= patterns.iter().all(|p| {
                let cs = method_children(*p);
                p.kind() == "typed_pattern"
                    && cs.len() == 2
                    && cs[0].kind() == "identifier_pattern"
                    && simple_pattern_name(cs[0]).is_some()
                    && cs[1].kind() == "simple_type"
            });
        }
        let start = self
            .parsed
            .tokens
            .partition_point(|t| t.start < group.start_byte());
        let end = self
            .parsed
            .tokens
            .partition_point(|t| t.start < group.end_byte());
        const MAX_RESULT_USES: usize = 64;
        let mut uses = Vec::new();
        for (i, t) in self.parsed.tokens[start..end].iter().enumerate() {
            let i = start + i;
            if i != token && t.ident && t.text == self.parsed.tokens[token].text {
                if uses.len() == MAX_RESULT_USES {
                    fresh = false;
                    break;
                }
                uses.push(i);
            }
        }
        if fresh {
            if let Some(eligible) = self.method_owner_eligibility.get(&owner.id()) {
                fresh &= *eligible;
            } else {
                let mut eligible = true;
                let mut pending = vec![owner];
                while let Some(node) = pending.pop() {
                    if crate::cancellation::is_cancelled() {
                        eligible = false;
                        break;
                    }
                    if matches!(
                        node.kind(),
                        "inherit"
                            | "inherit_decl"
                            | "inherit_declaration"
                            | "interface_implementation"
                    ) {
                        eligible = false;
                        break;
                    }
                    let mut cursor = node.walk();
                    pending.extend(node.children(&mut cursor));
                }
                self.method_owner_eligibility.insert(owner.id(), eligible);
                fresh &= eligible;
            }
        }
        let body = bodies[0];
        let literal_group = fresh
            && self.literal_value_tree(body)
            && self.closed_method_group(group, owner, member, body, false);
        let atoms = if fresh
            && uses.is_empty()
            && !literal_group
            && owner.kind() == "record_type_defn"
            && self.closed_method_group(group, owner, member, body, true)
        {
            self.forwarding_atoms(body).unwrap_or_default()
        } else {
            Vec::new()
        };
        Some(RecordMethodContext {
            token: Some(token),
            annotation,
            eligible: fresh,
            uses,
            literal_group,
            factory_atoms: atoms,
        })
    }
    fn extract_unconstrained_member(&mut self, node: Node<'_>, declarations: &HashSet<usize>) {
        let cs = children(node);
        let lefts: Vec<_> = cs
            .iter()
            .filter(|n| {
                matches!(
                    n.kind(),
                    "function_declaration_left" | "value_declaration_left"
                )
            })
            .copied()
            .collect();
        let bodies = field_children(node, "body");
        if lefts.len() != 1 || bodies.len() != 1 || lefts[0].kind() != "function_declaration_left" {
            return;
        }
        let left = lefts[0];
        let body = bodies[0];
        let prefix = &self.source[node.start_byte()..body.start_byte()];
        if prefix
            .split_whitespace()
            .any(|w| matches!(w, "rec" | "inline" | "and" | "mutable"))
            || cs
                .iter()
                .any(|n| is_type_node(n.kind()) && n.end_byte() <= body.start_byte())
        {
            return;
        }
        let parts = children(left);
        if parts.iter().any(|n| {
            !matches!(
                n.kind(),
                "identifier" | "argument_patterns" | "access_modifier"
            )
        }) {
            return;
        }
        let Some(function) = parts
            .iter()
            .find(|n| n.kind() == "identifier")
            .and_then(|n| self.token(*n))
        else {
            return;
        };
        let Some(arguments) = parts.iter().find(|n| n.kind() == "argument_patterns") else {
            return;
        };
        let arguments = children(*arguments);
        if arguments.len() != 1
            || !matches!(
                arguments[0].kind(),
                "identifier" | "long_identifier" | "identifier_pattern"
            )
        {
            return;
        }
        let Some(parameter) = simple_pattern_name(arguments[0])
            .and_then(|n| self.token(n))
            .filter(|t| declarations.contains(t))
        else {
            return;
        };
        let body = unwrap_expression(body);
        if !matches!(body.kind(), "long_identifier" | "long_identifier_or_op") {
            return;
        }
        let ids = identifier_descendants(body);
        if ids.len() != 2 {
            return;
        }
        let (Some(receiver), Some(token)) = (self.token(ids[0]), self.token(ids[1])) else {
            return;
        };
        if self.parsed.tokens[receiver].text != self.parsed.tokens[parameter].text
            || !self
                .parsed
                .local_binding(receiver)
                .is_some_and(|d| d.token == parameter)
        {
            return;
        }
        self.parsed.unconstrained_members.push(UnconstrainedMember {
            token,
            parameter,
            function,
        });
    }
    fn extract_receiver_type(&mut self, node: Node<'_>, declarations: &HashSet<usize>) {
        let receiver = if node.kind() == "method_or_prop_defn" {
            node.child_by_field_name("name")
                .and_then(|n| n.child_by_field_name("instance"))
        } else {
            children(node)
                .into_iter()
                .find(|n| n.kind() == "identifier")
        };
        let Some(binding) = receiver
            .and_then(|n| self.token(n))
            .filter(|t| declarations.contains(t))
        else {
            return;
        };
        let mut ancestor = node.parent();
        while let Some(parent) = ancestor {
            if matches!(parent.kind(), "anon_type_defn" | "record_type_defn") {
                let header = children(parent)
                    .into_iter()
                    .find(|n| n.kind() == "type_name");
                if let Some(header) = header {
                    if let Some(token) = type_declaration_name(header).and_then(|n| self.token(n)) {
                        let mut name = self.nominal_owner(token);
                        if let Some(arguments) = children(header)
                            .into_iter()
                            .find(|n| n.kind() == "type_arguments")
                        {
                            name.push_str(raw(arguments, self.source));
                        }
                        self.parsed.binding_types.push(BindingTypeFact {
                            binding,
                            source: TypeSource::Named { name, token },
                        });
                    }
                }
                return;
            }
            ancestor = parent.parent();
        }
    }
    fn extract_record_type(&mut self, node: Node<'_>) {
        let cs = children(node);
        let Some(name_node) = cs
            .iter()
            .find(|n| n.kind() == "type_name")
            .and_then(|n| type_declaration_name(*n))
        else {
            return;
        };
        let Some(token) = self.token(name_node) else {
            return;
        };
        self.extract_method_headers(node, token);
        let is_record = node.kind() == "record_type_defn";
        let mut fields = Vec::new();
        let mut pending = cs;
        while let Some(child) = pending.pop() {
            if child.kind() == "record_field" {
                let parts = children(child);
                if let Some(field_token) = parts
                    .iter()
                    .find(|n| n.kind() == "identifier")
                    .and_then(|n| self.token(*n))
                {
                    let ty = parts
                        .iter()
                        .find(|n| is_type_node(n.kind()))
                        .and_then(|n| self.named_type(*n));
                    let visibility = record_field_accessibility(child, node, self.source);
                    fields.push(RecordField {
                        name: self.parsed.tokens[field_token].text.clone(),
                        token: field_token,
                        kind: "field".into(),
                        type_name: ty.as_ref().map(|x| x.0.clone()),
                        type_token: ty.map(|x| x.1),
                        accessibility: visibility.into(),
                    });
                }
            } else if child.kind() == "method_or_prop_defn" {
                // A property has no argument pattern; indexers/method overloads
                // are deliberately excluded from these field-identity facts.
                let property = child.child_by_field_name("name");
                let static_property = child.parent().is_some_and(|parent| {
                    self.source[parent.start_byte()..child.start_byte()]
                        .split_whitespace()
                        .any(|word| word == "static")
                });
                let modifier = child.parent().and_then(|parent| {
                    self.source[parent.start_byte()..child.start_byte()]
                        .split_whitespace()
                        .find(|word| matches!(*word, "override" | "default"))
                });
                if (static_property
                    || property.is_some_and(|n| n.child_by_field_name("instance").is_some()))
                    && field_children(child, "args").is_empty()
                    && children(child).iter().all(|n| {
                        !matches!(
                            n.kind(),
                            "property_accessor" | "property_getter" | "property_setter"
                        )
                    })
                {
                    if let Some(field_token) = property
                        .and_then(|n| {
                            n.child_by_field_name("method").or_else(|| {
                                if static_property {
                                    children(n).into_iter().find(|n| n.kind() == "identifier")
                                } else {
                                    None
                                }
                            })
                        })
                        .and_then(|n| self.token(n))
                    {
                        let mut walk = child.walk();
                        let equal = child
                            .children(&mut walk)
                            .find(|n| n.kind() == "=")
                            .map(|n| n.start_byte());
                        if let Some(equal) = equal {
                            let ty = children(child)
                                .iter()
                                .find(|n| is_type_node(n.kind()) && n.end_byte() < equal)
                                .and_then(|n| self.named_type(*n));
                            fields.push(RecordField {
                                name: self.parsed.tokens[field_token].text.clone(),
                                token: field_token,
                                kind: if static_property {
                                    "static-property"
                                } else if modifier == Some("override") {
                                    "override-property"
                                } else if modifier == Some("default") {
                                    "default-property"
                                } else {
                                    "property"
                                }
                                .into(),
                                type_name: ty.as_ref().map(|x| x.0.clone()),
                                type_token: ty.map(|x| x.1),
                                accessibility: declaration_accessibility(child, self.source).into(),
                            });
                        }
                    }
                }
            } else if child.kind() == "member_defn" {
                let mut cursor = child.walk();
                let auto = child.children(&mut cursor).any(|n| n.kind() == "val");
                if auto {
                    let parts = children(child);
                    if let Some(property) = parts.iter().find(|n| n.kind() == "property_or_ident") {
                        if let Some(field_token) = children(*property)
                            .into_iter()
                            .find(|n| n.kind() == "identifier")
                            .and_then(|n| self.token(n))
                        {
                            let mut walk = child.walk();
                            let equal = child
                                .children(&mut walk)
                                .find(|n| n.kind() == "=")
                                .map(|n| n.start_byte());
                            if let Some(equal) = equal {
                                let ty = parts
                                    .iter()
                                    .find(|n| is_type_node(n.kind()) && n.end_byte() < equal)
                                    .and_then(|n| self.named_type(*n));
                                let static_property = self.source
                                    [child.start_byte()..property.start_byte()]
                                    .split_whitespace()
                                    .any(|word| word == "static");
                                fields.push(RecordField {
                                    name: self.parsed.tokens[field_token].text.clone(),
                                    token: field_token,
                                    kind: if static_property {
                                        "static-property"
                                    } else {
                                        "property"
                                    }
                                    .into(),
                                    type_name: ty.as_ref().map(|x| x.0.clone()),
                                    type_token: ty.map(|x| x.1),
                                    accessibility: declaration_accessibility(child, self.source)
                                        .into(),
                                });
                                self.non_value.insert(field_token);
                            }
                        }
                    }
                } else {
                    for signature in children(child)
                        .into_iter()
                        .filter(|n| n.kind() == "member_signature")
                    {
                        let parts = children(signature);
                        let property_type = parts
                            .iter()
                            .find(|n| n.kind() == "curried_spec")
                            .and_then(|n| {
                                let types = children(*n);
                                if types.len() == 1 && is_type_node(types[0].kind()) {
                                    Some(types[0])
                                } else {
                                    None
                                }
                            });
                        let static_property = self.source
                            [child.start_byte()..signature.start_byte()]
                            .split_whitespace()
                            .any(|word| word == "static");
                        if !static_property {
                            if let (Some(field_token), Some((type_name, type_token))) = (
                                parts
                                    .iter()
                                    .find(|n| n.kind() == "identifier")
                                    .and_then(|n| self.token(*n)),
                                property_type.and_then(|n| self.named_type(n)),
                            ) {
                                fields.push(RecordField {
                                    name: self.parsed.tokens[field_token].text.clone(),
                                    token: field_token,
                                    kind: "abstract-property".into(),
                                    type_name: Some(type_name),
                                    type_token: Some(type_token),
                                    accessibility: declaration_accessibility(
                                        signature,
                                        self.source,
                                    )
                                    .into(),
                                });
                            }
                        }
                    }
                    pending.extend(children(child));
                }
            } else if matches!(child.kind(), "record_fields" | "type_extension_elements") {
                pending.extend(children(child));
            }
        }
        fields.sort_by_key(|f| self.parsed.tokens[f.token].start);
        // Core attribute semantics are proved by the resolver, never spelling.
        let require_qualified_access = false;
        let inherit = children(node)
            .into_iter()
            .find(|n| n.kind() == "class_inherits_decl");
        let base = inherit
            .and_then(|n| children(n).into_iter().find(|n| is_type_node(n.kind())))
            .and_then(|n| self.named_type(n));
        let represented: HashSet<_> = fields.iter().map(|f| f.token).collect();
        let mut blocked_members = Vec::new();
        let mut pending = children(node);
        while let Some(n) = pending.pop() {
            if n.kind() == "member_defn" {
                for header in children(n) {
                    let name_node = match header.kind() {
                        "method_or_prop_defn" => header.child_by_field_name("name").and_then(|n| {
                            n.child_by_field_name("method").or_else(|| {
                                children(n).into_iter().find(|n| n.kind() == "identifier")
                            })
                        }),
                        "member_signature" | "property_or_ident" => children(header)
                            .into_iter()
                            .find(|n| n.kind() == "identifier"),
                        _ => None,
                    };
                    if let Some(token) = name_node
                        .and_then(|n| self.token(n))
                        .filter(|t| !represented.contains(t))
                    {
                        blocked_members.push(self.parsed.tokens[token].text.clone());
                    }
                }
            } else if matches!(
                n.kind(),
                "type_extension_elements" | "interface_implementation"
            ) {
                pending.extend(children(n));
            }
        }
        blocked_members.sort();
        blocked_members.dedup();
        self.parsed.record_types.push(RecordType {
            name: self.nominal_owner(token),
            token,
            kind: if is_record { "record" } else { "class" }.into(),
            base_type: base
                .as_ref()
                .map(|x| x.0.clone())
                .or_else(|| inherit.map(|_| "<unsupported-base>".into())),
            base_type_token: base.map(|x| x.1),
            blocked_members,
            type_parameters: children(node)
                .into_iter()
                .find(|n| n.kind() == "type_name")
                .map(|header| {
                    let mut pending: Vec<_> = children(header)
                        .into_iter()
                        .filter(|n| n.kind() == "type_arguments")
                        .collect();
                    let mut parameters = Vec::new();
                    while let Some(n) = pending.pop() {
                        if n.kind() == "type_argument" {
                            parameters.push((n.start_byte(), raw(n, self.source).to_owned()));
                        } else {
                            pending.extend(children(n));
                        }
                    }
                    parameters.sort_by_key(|x| x.0);
                    parameters.into_iter().map(|x| x.1).collect()
                })
                .unwrap_or_default(),
            fields,
            accessibility: declaration_accessibility(name_node, self.source).into(),
            require_qualified_access,
            start: node.start_byte(),
            scope_end: self.owner_end(token),
        });
    }
}
fn unwrap_expression(mut node: Node<'_>) -> Node<'_> {
    while node.kind() == "paren_expression" {
        let cs = children(node);
        if cs.len() != 1 {
            break;
        }
        node = cs[0];
    }
    node
}
fn type_parameters(header: Node<'_>, source: &str) -> Vec<String> {
    let mut pending: Vec<_> = children(header)
        .into_iter()
        .filter(|n| n.kind() == "type_arguments")
        .collect();
    let mut parameters = Vec::new();
    while let Some(n) = pending.pop() {
        if n.kind() == "type_argument" {
            parameters.push((n.start_byte(), raw(n, source).to_owned()));
        } else {
            pending.extend(children(n));
        }
    }
    parameters.sort_by_key(|x| x.0);
    parameters.into_iter().map(|x| x.1).collect()
}
fn type_declaration_name(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("type_name").or_else(|| {
        children(node)
            .into_iter()
            .find(|n| matches!(n.kind(), "identifier" | "long_identifier"))
    })
}
fn active_pattern_header(node: Node<'_>) -> Option<Node<'_>> {
    let mut pending = vec![node];
    for _ in 0..64 {
        let n = pending.pop()?;
        if n.has_error() {
            return None;
        }
        match n.kind() {
            "active_pattern" => return Some(n),
            "value_declaration_left"
            | "identifier_pattern"
            | "long_identifier_or_op"
            | "paren_pattern" => pending.extend(method_children(n)),
            _ => {}
        }
    }
    None
}
fn simple_pattern_name(node: Node<'_>) -> Option<Node<'_>> {
    if node.kind() == "identifier" {
        return Some(node);
    }
    if !matches!(
        node.kind(),
        "value_declaration_left"
            | "identifier_pattern"
            | "long_identifier_or_op"
            | "long_identifier"
            | "paren_pattern"
            | "typed_pattern"
    ) {
        return None;
    }
    let cs: Vec<_> = children(node)
        .into_iter()
        .filter(|n| {
            !matches!(n.kind(), "attributes" | "access_modifier") && !is_type_node(n.kind())
        })
        .collect();
    if cs.len() == 1 {
        simple_pattern_name(cs[0])
    } else {
        None
    }
}
fn record_field_accessibility(field: Node<'_>, owner: Node<'_>, source: &str) -> &'static str {
    let own = declaration_accessibility(field, source);
    let representation = children(owner)
        .into_iter()
        .find(|n| n.kind() == "access_modifier")
        .map(|n| raw(n, source));
    if own == "private" || representation == Some("private") {
        "private"
    } else if own == "internal" || representation == Some("internal") {
        "internal"
    } else {
        "public"
    }
}

// Visibility is the intersection of the binding and its enclosing declarations.
// Inspect only declaration headers, never sibling bodies: scanning a complete
// module for every binding would make extraction quadratic on large files.
fn header_accessibility(node: Node<'_>, source: &str) -> Option<&'static str> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "attributes" => continue,
            "access_modifier" => {
                return match raw(child, source).trim() {
                    "private" => Some("private"),
                    "internal" => Some("internal"),
                    _ => None,
                }
            }
            "type_name" => return header_accessibility(child, source),
            _ => break,
        }
    }
    None
}

fn declaration_accessibility(mut node: Node<'_>, source: &str) -> &'static str {
    let mut visibility = "public";
    loop {
        if matches!(
            node.kind(),
            "function_declaration_left"
                | "value_declaration_left"
                | "value_definition"
                | "named_module"
                | "module_defn"
                | "anon_type_defn"
                | "record_type_defn"
                | "type_abbrev_defn"
                | "type_name"
                | "member_defn"
                | "method_or_prop_defn"
        ) {
            match header_accessibility(node, source) {
                Some("private") => return "private",
                Some("internal") => visibility = "internal",
                _ => {}
            }
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => return visibility,
        }
    }
}

fn identifier_descendants(node: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" {
            out.push(n)
        } else {
            let mut cs = children(n);
            cs.reverse();
            stack.extend(cs)
        }
    }
    out
}
fn is_type_node(k: &str) -> bool {
    k.ends_with("_type")
        || matches!(
            k,
            "type_argument" | "type_arguments" | "type_argument_constraints" | "types" | "measure"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nominal_type_and_import_components_are_separate_from_field_expression_roles() {
        let source="module M\nopen Outer.Artifact\ntype Event = Case of Artifact:Artifact.ArtifactEvent\ntype Holder = {Artifact:int}\nlet create Artifact = {Artifact=Artifact}\nlet read (holder:Holder) = holder.Artifact, nameof holder.Artifact\nlet literal = nameof Artifact\nlet typed (value:Outer.Artifact.Box<Inner.Artifact>) = value\n";
        let p = parse(source);
        let imports: Vec<_> = p
            .import_name_tokens
            .iter()
            .map(|i| p.tokens[*i].text.as_str())
            .collect();
        assert_eq!(imports, ["Outer", "Artifact"]);
        let types: Vec<_> = p
            .type_name_tokens
            .iter()
            .map(|i| (p.tokens[*i].line, p.tokens[*i].text.as_str()))
            .collect();
        assert!(types.contains(&(2, "Artifact")) && types.contains(&(2, "ArtifactEvent")));
        assert!(
            types.contains(&(7, "Outer"))
                && types.contains(&(7, "Box"))
                && types.contains(&(7, "Inner")),
            "{types:?}; {:?}",
            p.uncertain
        );
        let payload_label = p
            .union_payload_labels
            .iter()
            .copied()
            .find(|i| p.tokens[*i].text == "Artifact")
            .unwrap();
        assert!(!p.type_name_tokens.contains(&payload_label));
        for (i, t) in p
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.text == "Artifact" && (3..=6).contains(&t.line))
        {
            assert!(
                !p.type_name_tokens.contains(&i) && !p.import_name_tokens.contains(&i),
                "{}:{}",
                t.line,
                t.column
            );
        }
    }
    #[test]
    fn unsupported_record_puns_never_become_nominal_type_or_import_roles() {
        let p = parse("module M\nlet pun Artifact = {Artifact}\n");
        assert!(p.type_name_tokens.is_empty() && p.import_name_tokens.is_empty());
    }
    #[test]
    fn nominal_type_and_import_components_work_in_signatures_without_open_type_shortcuts() {
        let p=parse_signature("module M\nopen Outer.Artifact\nopen type Hidden.Artifact\nval value: Outer.Artifact.Box<Inner.Artifact>\n");
        assert_eq!(
            p.import_name_tokens
                .iter()
                .map(|i| p.tokens[*i].text.as_str())
                .collect::<Vec<_>>(),
            ["Outer", "Artifact"]
        );
        assert_eq!(
            p.type_name_tokens
                .iter()
                .map(|i| p.tokens[*i].text.as_str())
                .collect::<Vec<_>>(),
            ["Outer", "Artifact", "Box", "Inner", "Artifact"]
        );
        assert!(p
            .uncertain
            .iter()
            .any(|u| u.reason == "open type requires static member resolution"));
        let p = parse("module M\nlet broken (x:Artifact.Box<Inner.Artifact) = x\n");
        assert!(
            p.type_name_tokens.is_empty(),
            "{:?}",
            p.type_name_tokens
                .iter()
                .map(|i| &p.tokens[*i])
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn reverse_index_grammar_retains_operand_binding_and_original_ranges() {
        let source = "module M\nlet select (values:int array) offset =\n    let a=values[^offset]\n    let b=values.[^offset]\n    let c=values[..^offset]\n    let d=values[^3..^offset]\n    let e=values[^offset..]\n    let f=values[0..^offset]\n    let g=values.[0..^offset]\n    let h=values[0..^(offset + 1)]\n    a+b\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_fsharp::LANGUAGE_FSHARP.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        let p = parse(source);
        assert!(p.uncertain.is_empty(), "{:?}", p.uncertain);
        for name in ["values", "offset"] {
            let declaration = p.declarations.iter().find(|d| d.name == name).unwrap();
            let uses: Vec<_> = p
                .tokens
                .iter()
                .enumerate()
                .filter(|(_, t)| t.text == name && t.line > 1)
                .collect();
            assert_eq!(uses.len(), 8, "{name}");
            for (index, token) in uses {
                assert_eq!(
                    p.local_binding(index).map(|d| d.token),
                    Some(declaration.token)
                );
                assert_eq!(&source[token.start..token.end], name);
                assert_eq!(
                    token.column as usize,
                    source.lines().nth(token.line as usize).unwrap()[..token.column as usize]
                        .encode_utf16()
                        .count()
                );
            }
        }
    }
    #[test]
    fn reverse_index_grammar_keeps_srtp_infix_and_signature_boundaries() {
        let source = "module M\nlet (^) (a:string) b = a+b\nlet infix a b = a ^ b\nlet inline single (x:^T) = (^T : (static member Name : ^T -> string) x)\nlet inline multiple (x:^T) (y:^U) = ((^T or ^U) : (static member Join : ^T * ^U -> string) (x,y))\nlet slice (matrix:int[,]) offset = matrix.[*, ^offset..^0]\nlet text = \"matrix[0..^offset]\"\n// matrix[0..^offset]\n";
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_fsharp::LANGUAGE_FSHARP.into();
        assert_eq!(language.abi_version(), 15);
        parser.set_language(&language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        let mut pending = vec![tree.root_node()];
        let mut traits = 0;
        let mut caret_infix = 0;
        let mut reverse = 0;
        while let Some(node) = pending.pop() {
            traits += usize::from(node.kind() == "srtp_call_expression");
            caret_infix += usize::from(node.kind() == "infix_op" && raw(node, source) == "^");
            reverse += usize::from(
                node.kind() == "prefixed_expression" && raw(node, source).starts_with('^'),
            );
            pending.extend(children(node));
        }
        assert_eq!((traits, caret_infix, reverse), (2, 1, 2));
        let signature: tree_sitter::Language = tree_sitter_fsharp::LANGUAGE_SIGNATURE.into();
        assert_eq!(signature.abi_version(), 15);
        parser.set_language(&signature).unwrap();
        let tree = parser.parse("module M\nval inline single: x: ^T -> string when ^T : (static member Name: ^T -> string)\n", None).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
    }
    #[test]
    fn reverse_index_prefix_operand_matches_compiler_arithmetic_precedence() {
        // FCS parses IndexFromEnd(offset) + 1 in the first/dotted cases,
        // and rejects their use as indices. Parentheses put addition inside
        // IndexFromEnd and make the second case a valid reverse index.
        let source="module M\nlet first (values:int array) offset=values[^offset + 1]\nlet second (values:int array) offset=values[^(offset + 1)]\nlet third (values:int array) offset=values.[^offset + 1]\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_fsharp::LANGUAGE_FSHARP.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        assert!(!tree.root_node().has_error());
        let mut pending = vec![tree.root_node()];
        let mut caret_operands = Vec::new();
        while let Some(node) = pending.pop() {
            if node.kind() == "prefixed_expression" && raw(node, source).starts_with('^') {
                caret_operands.push((node.start_byte(), raw(node, source)));
            }
            pending.extend(children(node));
        }
        caret_operands.sort_by_key(|x| x.0);
        assert_eq!(
            caret_operands.iter().map(|x| x.1).collect::<Vec<_>>(),
            ["^offset", "^(offset + 1)", "^offset"]
        );
    }
    #[test]
    fn reverse_index_grammar_preserves_errors_for_missing_operands_and_delimiters() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_fsharp::LANGUAGE_FSHARP.into())
            .unwrap();
        for expression in ["values[0..^]", "values[^]", "values[0..^offset"] {
            let source = format!("module M\nlet select (values:int array) offset = {expression}\n");
            let tree = parser.parse(&source, None).unwrap();
            assert!(
                tree.root_node().has_error(),
                "{expression}: {}",
                tree.root_node().to_sexp()
            );
            // Function-body absence certificates require the complete CST to
            // be error-free, including missing anonymous closing delimiters.
            let parsed = parse(&source);
            assert!(
                parsed
                    .uncertain
                    .iter()
                    .all(|u| u.local_binding_scope.is_none()),
                "{expression}: {:?}",
                parsed.uncertain
            );
        }
    }
    #[test]
    fn operator_name_certificates_bound_names_without_weakening_the_uncertain_scope() {
        let source = "module M\nlet (>>=) input apply = apply input\nlet later = nameof M\n";
        let p = parse(source);
        let span = p
            .uncertain
            .iter()
            .find(|u| {
                u.reason == "Custom operator/active-pattern declaration needs semantic identity"
            })
            .unwrap();
        let names = span.binding_names.as_ref().unwrap();
        for name in ["(>>=)", ">>=", "input", "apply"] {
            assert!(names.iter().any(|n| n == name), "{name}: {names:?}");
        }
        assert!(!names.iter().any(|n| n == "nameof" || n == "later"));
        assert!(span.name.is_none());
        assert_eq!(span.end, source.trim_end().len());
        let later = p.tokens.iter().position(|t| t.text == "later").unwrap();
        assert!(!p.binding_supported(later));

        let escaped =
            parse("module M\nlet (>>=) ``nameof`` input = ``nameof`` input\nlet later=1\n");
        let names = escaped
            .uncertain
            .iter()
            .find_map(|u| u.binding_names.as_ref())
            .unwrap();
        assert!(names.iter().any(|n| n == "nameof"));
        assert!(!names.iter().any(|n| n == "``nameof``"));
    }
    #[test]
    fn operator_name_certificates_refuse_recursive_attributed_recovered_and_builder_shapes() {
        for source in [
            "module M\nlet rec (>>=) x f = f x and nameof x = x\n",
            "module rec M\nlet (>>=) x f = f x\n",
            "module M\n[<CompiledName(\"nameof\")>]\nlet (>>=) x f = f x\n",
            "module M\nlet (>>=) x f = task { let! value = x\n                                    return f value }\n",
            "module M\nlet (>>=) x f = f (\n",
        ] {
            let p = parse(source);
            assert!(p.uncertain.iter().all(|u| u.binding_names.is_none()), "{source}\n{:?}", p.uncertain);
        }
        let names = (0..260)
            .map(|i| format!("value{i}"))
            .collect::<Vec<_>>()
            .join("; ");
        let p = parse(&format!("module M\nlet (>>=) x f = [| {names} |]\n"));
        assert!(p.uncertain.iter().all(|u| u.binding_names.is_none()));
    }
    #[test]
    fn record_initializer_trivia_preserves_field_roles_offsets_and_method_guards() {
        for comment in [
            "// Ignored field documentation",
            "(* Ignored field documentation *)",
            "/// Ignored field documentation",
        ] {
            let source=format!("module M\ntype R={{Class:string; RelativePath:string; Sha256Hash:string}}\ntype Factory=\n    static member Make(relativePath:string)(sha256Hash:string)=\n        {{\n            Class=\"Local\"\n            {comment}\n            RelativePath = $\"{{relativePath}}\"\n            Sha256Hash=sha256Hash\n        }}\nlet update existing sha=\n    {{existing with\n        {comment}\n        Sha256Hash=sha}}\n");
            let p = parse(&source);
            assert_eq!(p.record_expressions.len(), 2, "{comment}");
            let initial = &p.record_expressions[0];
            assert_eq!(
                initial
                    .fields
                    .iter()
                    .map(|t| p.tokens[*t].text.as_str())
                    .collect::<Vec<_>>(),
                ["Class", "RelativePath", "Sha256Hash"]
            );
            let field = *initial.fields.last().unwrap();
            assert_eq!((p.tokens[field].line, p.tokens[field].column), (8, 12));
            assert_eq!(
                &source[p.tokens[field].start..p.tokens[field].end],
                "Sha256Hash"
            );
            assert!(initial.method_result.is_some());
            assert!(!initial.method_result_eligible && !initial.fresh_method_result);
            let update = &p.record_expressions[1];
            assert_eq!(update.fields.len(), 1);
            assert_eq!(p.tokens[update.fields[0]].text, "Sha256Hash");
            assert_eq!(p.tokens[update.copy.unwrap()].text, "existing");
        }
        let computation = parse("module M\nlet values=seq {yield 1}\n");
        assert!(computation.record_expressions.is_empty());
    }
    #[test]
    fn signature_active_pattern_headers_retain_exported_case_identity_without_inference() {
        let source="module Patterns\nval (|Some|_|): Customer option -> Supplier option\nval private (|Hidden|_|): int -> int option\nval (|Left|Right|): int -> Choice<int,int>\n";
        let p = parse_signature(source);
        let names: Vec<_> = p
            .pattern_declarations
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(names, ["Some", "Hidden", "Left", "Right"]);
        assert!(p.pattern_declarations.iter().all(|d| d.top_level
            && d.module == "Patterns"
            && d.kind == "active-pattern"
            && d.type_owner.is_none()));
        assert_eq!(p.pattern_declarations[0].accessibility, "public");
        assert_eq!(p.pattern_declarations[1].accessibility, "private");
        for d in &p.pattern_declarations {
            assert!(p
                .uncertain
                .iter()
                .any(|u| u.name.as_deref() == Some(&d.name)
                    && u.start == d.scope_start
                    && u.end == d.scope_end));
            assert!(p.declaration_at(d.token).is_none());
        }
        assert!(
            p.function_returns.is_empty()
                && p.function_results.is_empty()
                && p.binding_types.is_empty()
        );
        let hidden = parse_signature("module Patterns\nval marker:int\n");
        assert!(hidden.pattern_declarations.is_empty());
        assert!(hidden.uncertain.is_empty());
    }
    #[test]
    fn signature_active_patterns_in_closed_modules_preserve_export_and_module_privacy() {
        let p=parse_signature("namespace N\nmodule Public=\n    val (|Some|_|): int -> int option\nmodule private Private=\n    val (|Other|_|): int -> int option\n");
        assert_eq!(p.pattern_declarations.len(), 2);
        let public = &p.pattern_declarations[0];
        assert_eq!(public.module, "N.Public");
        assert!(public.top_level);
        assert_eq!(public.scope_start, public.scope_end);
        assert_eq!(public.accessibility, "public");
        let private = &p.pattern_declarations[1];
        assert_eq!(private.module, "N.Private");
        assert!(private.top_level);
        assert_eq!(private.accessibility, "private");
    }
    #[test]
    fn plain_string_alternatives_are_binder_free_across_literal_spellings() {
        let p = parse(
            r####"module M
let outer=42
let choose value=
    match value with
    | "Grace.Types.Library+LibraryBootstrapPageDto"
    | @"Grace.Types.Library+LibraryChangePageDto"
    | """Grace.Types.Library+LibraryContentAvailable"""
    | "${ordinary literal content}" -> outer
    | _ -> 0
"####,
        );
        assert!(p.uncertain.is_empty(), "{:?}", p.uncertain);
        assert!(p.potential_bindings.is_empty());
        assert_eq!(
            p.declarations.iter().filter(|d| d.name == "outer").count(),
            1
        );
    }
    #[test]
    fn interpolation_as_bindings_and_damaged_literal_patterns_keep_uncertainty() {
        for pattern in [
            r####"$"{value}" | "other""####,
            r####"$"""{value}""" | "other""####,
            r####"("one" as value) | ("other" as value)"####,
        ] {
            let p=parse(&format!("module M\nlet choose input=\n    match input with\n    | {pattern} -> 1\n    | _ -> 0\n"));
            assert!(!p.uncertain.is_empty(), "{pattern}");
        }
        let damaged = parse(
            "module M\nlet choose input=\n    match input with\n    | \"one\" | \"unterminated\n",
        );
        assert!(!damaged.uncertain.is_empty());
    }
    #[test]
    fn active_pattern_roles_keep_export_and_accessibility_separate_from_local_lifetime() {
        let source="module M\nmodule Closed=\n    let (|Maybe|_|) value=Some value\nmodule Private=\n    let private (|Maybe|_|) value=Some value\nlet run value=\n    let (|Maybe|_|) input=Some input\n    match value with | Maybe found -> found | _ -> 0\nlet later value=value\n";
        let p = parse(source);
        let cases: Vec<_> = p
            .pattern_declarations
            .iter()
            .filter(|d| d.kind == "active-pattern")
            .collect();
        assert_eq!(cases.len(), 3);
        let exported = cases.iter().find(|d| d.module == "M.Closed").unwrap();
        assert!(exported.top_level);
        assert_eq!(exported.accessibility, "public");
        assert_eq!(exported.scope_start, exported.scope_end);
        let private = cases.iter().find(|d| d.module == "M.Private").unwrap();
        assert!(private.top_level);
        assert_eq!(private.accessibility, "private");
        let local = cases.iter().find(|d| d.module == "M").unwrap();
        assert!(!local.top_level);
        assert!(local.scope_start < local.scope_end);
        assert!(local.scope_end <= source.find("let later").unwrap());
        assert_eq!(p.tokens[local.token].line, 6);
    }
    #[test]
    fn qualified_nullary_and_unbound_type_test_alternatives_do_not_create_binding_uncertainty() {
        let source="module M\nlet accepts value=\n    match value with\n    | Cases.First\n    | (Cases.Second) -> true\n    | _ -> false\nlet catches (error:exn)=\n    match error with\n    | :? System.FormatException\n    | :? System.Text.DecoderFallbackException -> true\n    | _ -> false\n";
        let p = parse(source);
        assert!(!p
            .uncertain
            .iter()
            .any(|u| u.reason == "Alternative patterns require a shared binding identity"));
        assert!(p.declarations.iter().all(|d| !matches!(
            d.name.as_str(),
            "First" | "Second" | "FormatException" | "DecoderFallbackException"
        )));
        assert!(p.potential_bindings.is_empty());
        for (token, t) in p.tokens.iter().enumerate().filter(|(_, t)| {
            matches!(
                t.text.as_str(),
                "FormatException" | "DecoderFallbackException"
            )
        }) {
            assert!(p.non_value_tokens.contains(&token), "{}", t.text);
        }
    }
    #[test]
    fn binding_or_ambiguous_alternatives_keep_the_shared_identity_guard() {
        for pattern in [
            "Cases.First value | Cases.Second value",
            "Some value | Other value",
            "Bare | OtherBare",
            "(Cases.First as value) | (Cases.Second as value)",
            "(:? System.Exception as value) | (:? System.Exception as value)",
            "(Cases.First, value) | (Cases.Second, value)",
        ] {
            let p=parse(&format!("module M\nlet choose input=\n    match input with\n    | {pattern} -> 1\n    | _ -> 0\n"));
            assert!(
                p.uncertain
                    .iter()
                    .any(|u| u.reason == "Alternative patterns require a shared binding identity"),
                "{pattern}"
            );
        }
    }
    #[test]
    fn explicit_new_heads_keep_type_namespace_role_despite_local_receiver_and_tuple_parse() {
        let source="module Main\nmodule n=\n    type Alias=System.Text.StringBuilder\ntype Holder={Alias:int}\nlet read(n:Holder)=n.Alias, new n.Alias(), typeof<n.Alias>\n";
        let p = parse(source);
        let aliases: Vec<_> = p
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.line == 4 && t.text == "Alias")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(aliases.len(), 3);
        assert_eq!(p.explicit_constructor_heads, [aliases[1]]);
        assert!(p.type_uses.iter().any(|t| t.token == aliases[2]));
        assert!(!p.type_uses.iter().any(|t| t.token == aliases[1]));
        assert!(!p.expression_type_candidates.iter().any(
            |t| t.token == aliases[1] && matches!(t.role, ExpressionTypeRole::ApplicationHead)
        ));
        assert!(p.member_accesses.iter().any(|a| a.token == aliases[0]));
        assert_eq!(
            p.qualifier_roots
                .iter()
                .filter(|t| p.tokens[**t].line == 4)
                .count(),
            1
        );
    }
    #[test]
    fn explicit_new_roles_preserve_generic_nested_commented_and_object_heads_only() {
        let source="module M\nlet first=new Outer.Box<int>()\nlet second=new (* gap *) Outer.Alias()\nlet third=new Outer.Wrapper(new n.Alias())\nlet fourth={new Outer.Interface with member _.Member=1}\nlet ordinary=Outer.Alias()\nlet generic<'T when 'T : (new : unit -> 'T)> ()=new 'T()\nlet escaped=new n.``A.B``()\n";
        let p = parse(source);
        let heads: Vec<_> = p
            .explicit_constructor_heads
            .iter()
            .map(|t| (p.tokens[*t].line, p.tokens[*t].text.as_str()))
            .collect();
        assert_eq!(
            heads,
            [
                (1, "Box"),
                (2, "Alias"),
                (3, "Wrapper"),
                (3, "Alias"),
                (4, "Interface"),
                (7, "A.B")
            ]
        );
        assert!(p.explicit_constructor_heads.iter().all(|t| !p
            .expression_type_candidates
            .iter()
            .any(|c| c.token == *t && matches!(c.role, ExpressionTypeRole::ApplicationHead))));
        assert!(p
            .expression_type_candidates
            .iter()
            .any(|c| p.tokens[c.token].line == 5 && c.name == "Outer.Alias"));
    }
    #[test]
    fn module_name_roles_keep_only_declaration_and_abbreviation_left_names() {
        for signature in [false, true] {
            let member = if signature {
                "val Name:int"
            } else {
                "let Name=1"
            };
            let source=format!("namespace Outer.Inner\n[<Marker(\"Name\")>]\nmodule Name =\n    type Record={{Name:int}}\n    {member}\nmodule Alias=Name\nmodule QualifiedAlias=Outer.Inner.Name\n");
            let p = if signature {
                parse_signature(&source)
            } else {
                parse(&source)
            };
            let names: Vec<_> = p
                .module_name_tokens
                .iter()
                .map(|t| (p.tokens[*t].text.as_str(), p.tokens[*t].line))
                .collect();
            assert_eq!(
                names,
                [
                    ("Outer", 0),
                    ("Inner", 0),
                    ("Name", 2),
                    ("Alias", 5),
                    ("QualifiedAlias", 6)
                ],
                "signature={signature}"
            );
            assert!(p
                .module_name_tokens
                .iter()
                .all(|t| !p.declarations.iter().any(|d| d.token == *t)));
        }
    }
    #[test]
    fn union_payload_labels_are_disjoint_from_same_spelled_type_tokens_and_record_fields() {
        let source="module M\ntype Sha256Hash=string\ntype Record={Sha256Hash:Sha256Hash}\ntype Packet=\n    | Pack of Sha256Hash:Sha256Hash * other:Outer.Sha256Hash * int\n";
        for p in [parse(source), parse_signature(source)] {
            let labels: Vec<_> = p
                .union_payload_labels
                .iter()
                .map(|t| p.tokens[*t].text.as_str())
                .collect();
            assert_eq!(labels, ["Sha256Hash", "other"]);
            assert!(p
                .union_payload_labels
                .iter()
                .all(|t| p.tokens[*t].line == 4));
            assert!(p
                .union_payload_labels
                .iter()
                .all(|t| !p.type_uses.iter().any(|u| u.token == *t)));
            assert_eq!(
                p.type_uses
                    .iter()
                    .filter(
                        |u| p.tokens[u.token].line == 4 && p.tokens[u.token].text == "Sha256Hash"
                    )
                    .count(),
                2
            );
            assert!(p.union_payload_labels.iter().all(|t| !p
                .record_types
                .iter()
                .flat_map(|r| &r.fields)
                .any(|f| f.token == *t)));
        }
    }
    #[test]
    fn expression_type_candidates_preserve_call_heads_and_explicit_generic_arity() {
        let source = "module M\nlet a=Alias value\nlet b=Outer.Inner.Alias value\nlet c=Box<int>(value)\nlet d=Outer.Box<Inner<int>,string>(value)\nlet e=(Alias) value\nlet f=(make()).Alias value\nlet Alias value=value\nlet shadow=Alias value\nlet unicode=\"😀\"; ``Ålias`` value\n";
        let p = parse(source);
        let heads: Vec<_> = p
            .expression_type_candidates
            .iter()
            .filter(|c| matches!(c.role, ExpressionTypeRole::ApplicationHead))
            .collect();
        let at = |line| {
            heads
                .iter()
                .copied()
                .find(|c| p.tokens[c.token].line == line)
                .unwrap()
        };
        assert_eq!(at(1).name, "Alias");
        assert_eq!(at(1).generic_arity, None);
        assert_eq!(at(2).name, "Outer.Inner.Alias");
        assert_eq!(at(3).generic_arity, Some(1));
        assert_eq!(at(4).name, "Outer.Box");
        assert_eq!(at(4).generic_arity, Some(2));
        assert_eq!(at(5).name, "Alias");
        assert!(!heads
            .iter()
            .any(|c| p.tokens[c.token].line == 6 && c.name.ends_with("Alias")));
        assert_eq!(
            p.local_binding(at(8).token).map(|d| d.name.as_str()),
            Some("Alias")
        );
        let unicode = at(9);
        assert_eq!(unicode.name, "Ålias");
        assert_eq!(
            p.tokens[unicode.token].column,
            source
                .lines()
                .nth(9)
                .unwrap()
                .split("``")
                .next()
                .unwrap()
                .encode_utf16()
                .count() as u32
        );
    }
    #[test]
    fn nameof_arguments_keep_the_actual_callee_and_never_assert_intrinsic_identity() {
        let p=parse("module M\nlet first=nameof Alias\nlet qualified=Microsoft.FSharp.Core.Operators.nameof(Outer.Alias)\nlet nameof x=x\nlet shadow=nameof Alias\nlet unrelated=consume Alias\nlet computed=nameof(make())\nlet field=nameof x.Field\n");
        let names: Vec<_> = p
            .expression_type_candidates
            .iter()
            .filter_map(|c| match c.role {
                ExpressionTypeRole::NameofArgument { callee } => Some((c, callee)),
                _ => None,
            })
            .collect();
        assert_eq!(names.len(), 4);
        assert!(names
            .iter()
            .all(|(_, callee)| p.tokens[*callee].text == "nameof"));
        let (qualified, callee) = names
            .iter()
            .find(|(c, _)| p.tokens[c.token].line == 2)
            .unwrap();
        assert_eq!(qualified.name, "Outer.Alias");
        assert_eq!(
            p.qualifier(*callee).as_deref(),
            Some("Microsoft.FSharp.Core.Operators")
        );
        let (_, shadow) = names
            .iter()
            .find(|(c, _)| p.tokens[c.token].line == 4)
            .unwrap();
        assert_eq!(
            p.local_binding(*shadow).map(|d| d.name.as_str()),
            Some("nameof")
        );
        assert!(!p
            .qualifier_roots
            .iter()
            .any(|t| matches!(p.tokens[*t].line, 2 | 7) && p.tokens[*t].text != "Microsoft"));
        let escaped = parse("module M\nlet first=``A.B`` value\nlet second=nameof ``A.B``\n");
        assert!(!escaped
            .expression_type_candidates
            .iter()
            .any(|c| c.name == "A.B"));
    }
    #[test]
    fn qualifier_roots_preserve_member_selectors_and_exclude_type_label_and_nameof_roles() {
        let source="module M\ntype R={Field:int}\ntype DU=Case of Field:R\nlet read (x:Outer.R)=x.Field.Child\nlet create x={R.Field=x.Field}\nlet root=RepositoryId.Empty\nlet named=nameof x.Field\nlet shorthand=_.Field\nlet pattern ({Field=value}:R)=value\nlet computed=(make()).Field.Child\nlet pun x={|x|}\n";
        let p = parse(source);
        let roots: Vec<_> = p
            .qualifier_roots
            .iter()
            .map(|t| (p.tokens[*t].line, p.tokens[*t].text.as_str()))
            .collect();
        assert_eq!(roots, [(3, "x"), (4, "x"), (5, "RepositoryId")]);
        let selectors: Vec<_> = p
            .member_accesses
            .iter()
            .filter(|a| p.tokens[a.token].line == 3)
            .map(|a| p.tokens[a.token].text.as_str())
            .collect();
        assert!(selectors.contains(&"Field") && selectors.contains(&"Child"));
        assert!(!p
            .expression_type_candidates
            .iter()
            .any(|c| p.tokens[c.token].line == 2));
    }
    #[test]
    fn enum_case_facts_keep_nominal_owner_accessibility_and_no_payload_inference() {
        let source = "namespace N\nmodule Usage =\n    [<System.Flags>]\n    type UsageFactKind =\n        | RepositoryStorageBytesMinute = 1\n    type private HiddenKind =\n        | Hidden = 2\n    module private Closed =\n        type Kind =\n            | Other = 3\n    let selected = UsageFactKind.RepositoryStorageBytesMinute\n";
        let p = parse(source);
        let cases: Vec<_> = p
            .pattern_declarations
            .iter()
            .filter(|d| d.kind == "enum-case")
            .collect();
        assert_eq!(cases.len(), 3);
        let visible = cases
            .iter()
            .find(|d| d.name == "RepositoryStorageBytesMinute")
            .unwrap();
        assert_eq!(visible.type_owner.as_deref(), Some("N.Usage.UsageFactKind"));
        assert_eq!(visible.accessibility, "public");
        assert_eq!(p.tokens[visible.token].line, 4);
        assert_eq!(
            cases
                .iter()
                .find(|d| d.name == "Hidden")
                .unwrap()
                .accessibility,
            "private"
        );
        assert_eq!(
            cases
                .iter()
                .find(|d| d.name == "Other")
                .unwrap()
                .accessibility,
            "private"
        );
        assert!(cases.iter().all(|d| d.qualified_only));
        assert!(p.union_case_payloads.is_empty());
        assert_eq!(p.attributes.len(), 1);
        assert_eq!(p.attributes[0].owner, "N.Usage.UsageFactKind");
        assert!(p
            .declarations
            .iter()
            .all(|d| !cases.iter().any(|c| c.token == d.token)));
    }
    #[test]
    fn projection_ranges_follow_receiver_grouping() {
        for (expr, child_range, last_range) in [
            ("parent.Child.P", "Child", "parent.Child.P"),
            ("(parent.Child).P", "parent.Child", "(parent.Child).P"),
            ("((parent.Child)).P", "parent.Child", "((parent.Child)).P"),
        ] {
            let line = format!("let read (parent:Wrapper) = {expr}");
            let parsed = parse(&format!("module Main\n{line}\n"));
            for (name, expected) in [("Child", child_range), ("P", last_range)] {
                let token = parsed
                    .token_at(1, line.rfind(name).unwrap() as u32)
                    .unwrap();
                let range = parsed.member_use_range(token).unwrap();
                assert_eq!(
                    &line[range.start.character as usize..range.end.character as usize],
                    expected,
                    "{expr}"
                );
            }
        }
    }

    #[test]
    fn method_calls_keep_long_qualified_name_chains_and_empty_string_arguments() {
        let source = "module M\nlet empty=System.String.IsNullOrEmpty \"\"\nlet text=Outer.Inner.Picker.Pick \"value\"\nlet short=Picker.Pick \"\"\nlet computed=(make()).Pick \"\"\n";
        let p = parse(source);
        assert_eq!(p.method_calls.len(), 3);
        for call in &p.method_calls {
            assert!(matches!(
                call.args.as_slice(),
                [ArgumentSource::IntrinsicLiteral(IntrinsicLiteral::String)]
            ));
            assert_ne!(p.tokens[call.member].line, 4);
        }
        let empty = p
            .method_calls
            .iter()
            .find(|c| p.tokens[c.member].line == 1)
            .unwrap();
        assert_eq!(p.tokens[empty.member].text, "IsNullOrEmpty");
        assert_eq!(p.tokens[empty.receiver].text, "String");
        assert_eq!(p.qualifier(empty.member).as_deref(), Some("System.String"));
        let text = p
            .method_calls
            .iter()
            .find(|c| p.tokens[c.member].line == 2)
            .unwrap();
        assert_eq!(
            p.qualifier(text.member).as_deref(),
            Some("Outer.Inner.Picker")
        );
    }
    #[test]
    fn potential_uppercase_bindings_distinguish_closed_exports_from_ended_locals() {
        let source = "module M\nmodule Closed=\n    let System=1\nmodule private Hidden=\n    let System=2\nlet run ()=\n    let System:int=3\n    System\nlet later=System\n";
        let p = parse(source);
        let bindings: Vec<_> = p
            .potential_bindings
            .iter()
            .filter(|d| d.name == "System")
            .collect();
        assert_eq!(bindings.len(), 3);
        let exported = bindings
            .iter()
            .find(|d| p.tokens[d.token].line == 2)
            .unwrap();
        assert!(exported.top_level);
        assert_eq!(exported.accessibility, "public");
        assert_eq!(p.declaration_module(exported), Some("M.Closed"));
        let private = bindings
            .iter()
            .find(|d| p.tokens[d.token].line == 4)
            .unwrap();
        assert!(private.top_level);
        assert_eq!(private.accessibility, "private");
        assert_eq!(p.declaration_module(private), Some("M.Hidden"));
        let local = bindings
            .iter()
            .find(|d| p.tokens[d.token].line == 6)
            .unwrap();
        assert!(!local.top_level);
        assert_eq!(local.kind, "value");
        let use_token = p.token_at(7, 4).unwrap();
        let later_token = p.token_at(8, 10).unwrap();
        assert!(local.visible_start <= p.tokens[use_token].start);
        assert!(p.tokens[use_token].start < local.scope_end);
        assert!(local.scope_end <= p.tokens[later_token].start);
        for binding in bindings {
            assert!(p.declaration_at(binding.token).is_none());
            assert!(p
                .uncertain
                .iter()
                .any(|u| u.start == p.tokens[binding.token].start
                    && u.name.as_deref() == Some("System")));
        }
    }
    #[test]
    fn potential_pattern_bindings_keep_tuple_active_and_inline_branch_scopes() {
        let source = "module M\nlet tuple input=\n    let (System, other)=input\n    System\nlet choose input=\n    match input with\n    | Extract System -> System\n    | _ -> 0\nlet inlineCases input=match input with | System -> System | _ -> 0\n";
        let p = parse(source);
        let bindings: Vec<_> = p
            .potential_bindings
            .iter()
            .filter(|d| d.name == "System")
            .collect();
        assert_eq!(bindings.len(), 3);
        for binding in &bindings {
            assert!(!binding.top_level);
            assert!(p.declaration_at(binding.token).is_none());
            assert!(binding.visible_start < binding.scope_end);
        }
        let branch = bindings
            .iter()
            .find(|d| p.tokens[d.token].line == 6)
            .unwrap();
        assert_eq!(branch.kind, "pattern");
        assert!(branch.scope_end <= source.find("| _ -> 0").unwrap());
        let inline = bindings
            .iter()
            .find(|d| p.tokens[d.token].line == 8)
            .unwrap();
        assert_eq!(inline.kind, "pattern");
        assert!(inline.scope_end <= source.rfind("| _ -> 0").unwrap());
    }
    #[test]
    fn formal_domains_separate_public_metadata_from_callbacks_and_nested_functions() {
        let source="module M\nlet publicFunction (publicArg:int)=publicArg\nlet direct=fun directArg -> directArg\nlet returned ()=fun returnedArg -> returnedArg\nlet mapped=List.map (fun callbackArg -> callbackArg+1) [1]\nlet outer source=let inner nestedArg=nestedArg+1 in inner source\nlet computed ()=let constant=1 in fun computedArg -> computedArg+constant\n";
        let p = parse(source);
        for name in [
            "publicArg",
            "directArg",
            "returnedArg",
            "source",
            "callbackArg",
            "nestedArg",
            "computedArg",
        ] {
            let d = p
                .declarations
                .iter()
                .find(|d| d.name == name && d.kind == "parameter")
                .unwrap_or_else(|| panic!("Missing {name}"));
            assert!(p.ordinary_function_formals.contains(&d.token), "{name}");
            assert_eq!(
                p.signature_independent_formals.contains(&d.token),
                matches!(name, "callbackArg" | "nestedArg" | "computedArg"),
                "{name}"
            );
        }
    }
    #[test]
    fn methods_constructors_active_patterns_and_unknown_formals_get_no_function_domain_certificate()
    {
        let p=parse("module M\ntype C(ctorArg:int)=\n    member _.M(methodArg:int)=ctorArg+methodArg\nlet (|Active|_|) activeArg = Some activeArg\nlet (>>=) operatorArg f=f operatorArg\nlet Upper ambiguousArg=ambiguousArg\n");
        for name in [
            "ctorArg",
            "methodArg",
            "activeArg",
            "operatorArg",
            "ambiguousArg",
        ] {
            let d = p
                .declarations
                .iter()
                .find(|d| d.name == name && d.kind == "parameter")
                .unwrap_or_else(|| panic!("Missing {name}"));
            assert!(!p.ordinary_function_formals.contains(&d.token), "{name}");
            assert!(
                !p.signature_independent_formals.contains(&d.token),
                "{name}"
            );
        }
        let p = parse_signature("module M\nval f : publicName:int -> int\n");
        assert!(
            p.ordinary_function_formals.is_empty() && p.signature_independent_formals.is_empty()
        );
    }
    #[test]
    fn repaired_inline_branch_maps_nested_function_uncertainty_scope() {
        let source = "module LongOriginalModuleName\nlet precedingValue = 12345\nlet choose input = match input with | 0 -> 0 | _ -> (let nested value = task { let! item = value in return item } in nested input)\nlet after = 42\n";
        let p = parse(source);
        let span = p
            .uncertain
            .iter()
            .find(|u| u.reason.contains("Computation-expression"))
            .expect("Nested task binding retains inference uncertainty");
        let [start, end] = span
            .local_binding_scope
            .expect("Complete nested function RHS has a local binding scope");
        assert_eq!(
            &source[start..end],
            "task { let! item = value in return item }"
        );
        assert!(span.start >= start && span.end <= end, "{span:?}");
        assert!(end < source.find("let after").unwrap());
        let formal = p
            .declarations
            .iter()
            .find(|d| d.name == "value" && d.kind == "parameter")
            .unwrap();
        assert_eq!(
            p.tokens[formal.token].start,
            source.find("nested value").unwrap() + "nested ".len()
        );
        assert!(p.ordinary_function_formals.contains(&formal.token));
        assert!(p.signature_independent_formals.contains(&formal.token));
    }
    #[test]
    fn open_type_uncertainty_uses_the_keyword_not_whitespace_or_comment_spelling() {
        for declaration in [
            "open type External.Holder",
            "open  type External.Holder",
            "open\ttype External.Holder",
            "open (* gap *) type External.Holder",
        ] {
            let source = format!(
                "module M\n{declaration}\nlet result = System.String.IsNullOrEmpty(\"\")\n"
            );
            let p = parse(&source);
            assert!(p.opens.is_empty(), "{declaration}");
            assert_eq!(p.uncertain.len(), 1, "{declaration}");
            assert_eq!(p.uncertain[0].start, "module M\n".len());
            assert_eq!(p.uncertain[0].end, source.trim_end().len());
        }
        for declaration in ["open (* type *) External.Holder", "open ``type``.Holder"] {
            let p = parse(&format!("module M\n{declaration}\nlet result = 1\n"));
            assert!(p.uncertain.is_empty(), "{declaration}");
            assert_eq!(p.opens.len(), 1, "{declaration}");
        }
    }
    #[test]
    fn applied_attribute_aliases_and_custom_names_keep_original_scope_tokens() {
        let p=parse("module M\ntype AO=Microsoft.FSharp.Core.AutoOpenAttribute\ntype RQA=Microsoft.FSharp.Core.RequireQualifiedAccessAttribute\ntype AOAttribute()=inherit System.Attribute()\n[<AO>]\nmodule Inner=\n    let value=1\n[<RQA>]\ntype Hidden={Name:string}\nlet explicit (x:Hidden)=x.Name\n");
        assert_eq!(p.attributes.len(), 2);
        let module = p
            .attributes
            .iter()
            .find(|a| a.owner_kind == "module")
            .unwrap();
        assert_eq!(module.type_name, "AO");
        assert_eq!(p.tokens[module.token].line, 4);
        assert_eq!(module.owner, "M.Inner");
        let record = p
            .attributes
            .iter()
            .find(|a| a.owner_kind == "type")
            .unwrap();
        assert_eq!(record.type_name, "RQA");
        assert_eq!(record.owner, "M.Hidden");
        assert!(p.auto_opens.is_empty());
        assert!(
            !p.record_types
                .iter()
                .find(|r| r.name == "M.Hidden")
                .unwrap()
                .require_qualified_access
        );
        assert!(p
            .member_accesses
            .iter()
            .any(|a| p.tokens[a.token].text == "Name"));
    }
    #[test]
    fn module_definition_shaped_abbreviations_do_not_create_empty_modules() {
        let p=parse("module Shared\nmodule Other =\n    type Widget={Name:string}\nmodule Use =\n    module Alias=Other\n    module Qualified=Shared.Other\n    let f (x:Alias.Widget)=x.Name\n");
        assert_eq!(
            p.aliases
                .iter()
                .map(|a| (a.name.as_str(), a.target.as_str()))
                .collect::<Vec<_>>(),
            [("Alias", "Other"), ("Qualified", "Shared.Other")]
        );
        assert!(!p
            .modules
            .iter()
            .any(|m| m.name.ends_with(".Alias") || m.name.ends_with(".Qualified")));
    }
    #[test]
    fn nested_record_expected_types_link_only_direct_field_values() {
        let p =
            parse("module M\nlet direct={Home={City=\"x\"}}\nlet viaCall={Home=id {City=\"y\"}}\n");
        let inner: Vec<_> = p
            .record_expressions
            .iter()
            .filter(|r| p.tokens[r.fields[0]].text == "City")
            .collect();
        assert_eq!(inner.len(), 2);
        assert_eq!(
            inner[0].parent_field.map(|i| p.tokens[i].text.as_str()),
            Some("Home")
        );
        assert!(inner[1].parent_field.is_none());
    }
    #[test]
    fn ordered_match_facts_keep_guard_ranges_and_copy_receiver_identity() {
        let source="module M\nlet update event current =\n    match event with\n    | Created -> First.Default\n    | Changed when current.Name <> \"é\" -> {current with Name=\"x\"}\n";
        let p = parse(source);
        let m = &p.match_expressions[0];
        assert!(m.complete);
        assert_eq!(m.branches.len(), 2);
        assert!(m.expected.is_none());
        let TypeSource::Alias { token } = m.branches[0].tail.as_ref().unwrap() else {
            panic!("expected exact tail token");
        };
        assert_eq!(p.tokens[*token].text, "Default");
        assert_eq!(p.qualifier(*token).as_deref(), Some("First"));
        let b = &m.branches[1];
        assert_eq!(
            &source[b.guard_start.unwrap()..b.guard_end.unwrap()],
            "current.Name <> \"é\""
        );
        assert!(b.pattern_end < b.guard_start.unwrap() && b.guard_end.unwrap() < b.body_start);
        let TypeSource::Record {
            copy: Some(receiver),
            fields,
        } = b.tail.as_ref().unwrap()
        else {
            panic!("expected copy fact");
        };
        assert_eq!(p.tokens[*receiver].text, "current");
        assert_eq!(
            p.local_binding(*receiver).map(|d| d.name.as_str()),
            Some("current")
        );
        assert_eq!(p.tokens[fields[0]].text, "Name");
    }
    #[test]
    fn match_tail_unwraps_only_sequence_and_let_continuation_and_keeps_unknown_branches() {
        let source="module M\nlet update event current items =\n    match event with\n    | Created ->\n        let mutable local = First.Default\n        for item in items do\n            local <- local\n        local\n    | Unknown -> if true then First.Default else Second.Default\n    | Changed -> {current with Name=\"x\"}\n";
        let p = parse(source);
        let m = &p.match_expressions[0];
        assert!(m.complete);
        assert_eq!(m.branches.len(), 3);
        let TypeSource::Alias { token } = m.branches[0].tail.as_ref().unwrap() else {
            panic!("tail must remain actual mutable-local use");
        };
        assert_eq!(p.tokens[*token].text, "local");
        assert_eq!(
            p.local_binding(*token).map(|d| d.name.as_str()),
            Some("local")
        );
        assert!(
            m.branches[1].tail.is_none(),
            "unknown earlier branch must not disappear"
        );
        assert!(matches!(
            m.branches[2].tail,
            Some(TypeSource::Record { .. })
        ));
        assert!(m
            .branches
            .windows(2)
            .all(|b| b[0].body_end <= b[1].pattern_start));
    }
    #[test]
    fn match_expected_type_comes_only_from_immediate_plain_source_annotations() {
        let p=parse("module M\ntype First={Name:string}\nlet f event current : First =\n    match event with\n    | Created -> current\n    | Changed -> current\ntype Factory=\n    static member Update(event:int,current:First) : First =\n        match event with\n        | 0 -> current\n        | _ -> current\nlet cast event current = (match event with | 0 -> current | _ -> current) :> obj\n");
        assert_eq!(p.match_expressions.len(), 3);
        for m in &p.match_expressions[..2] {
            let Some(TypeSource::Named { name, token }) = &m.expected else {
                panic!("explicit result annotation missing");
            };
            assert_eq!(name, "First");
            assert_eq!(p.tokens[*token].text, "First");
            assert!(p.tokens[*token].start < m.start);
        }
        assert!(
            p.match_expressions[2].expected.is_none(),
            "a result upcast is not a branch input annotation"
        );
    }
    #[test]
    fn ambiguous_inline_and_computation_matches_do_not_claim_complete_branch_order() {
        let p=parse("module M\nlet f value = match value with Some x -> x | None -> 0\nlet g value = async { match! value with | Some x -> return x | None -> return 0 }\n");
        assert!(!p.match_expressions.is_empty());
        let inline = &p.match_expressions[0];
        assert!(
            !inline.complete || inline.branches.len() == 2,
            "packed inline branches cannot pretend to be one complete branch"
        );
        for m in p.match_expressions.iter().skip(1) {
            assert!(!m.complete);
        }
    }
    #[test]
    fn explicit_method_return_annotation_does_not_replace_parameter_type() {
        let p=parse("module M\ntype Input={Name:int}\ntype Output={Name:string}\ntype Factory=\n    static member Create(input:Input) : Output = {Name=string input.Name}\n    static member Pair(left:Input,right:Input) : Output = {Name=string left.Name}\n");
        let create = p
            .method_declarations
            .iter()
            .find(|m| m.name == "Create")
            .unwrap();
        assert!(create.supported);
        assert_eq!(create.parameters.len(), 1);
        assert_eq!(create.parameters[0].type_name, "Input");
        let pair = p
            .method_declarations
            .iter()
            .find(|m| m.name == "Pair")
            .unwrap();
        assert!(pair.supported);
        assert_eq!(pair.parameters.len(), 2);
        assert!(pair.parameters.iter().all(|p| p.type_name == "Input"));
        for parameter in p.declarations.iter().filter(|d| d.kind == "parameter") {
            let named = p
                .binding_types
                .iter()
                .filter(|f| f.binding == parameter.token)
                .filter_map(|f| match &f.source {
                    TypeSource::Named { name, .. } => Some(name.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(named, ["Input"], "{}", parameter.name);
        }
        assert!(p
            .record_expressions
            .iter()
            .all(|r| r.type_name.as_deref() == Some("Output")
                && !r.fresh_method_result
                && !r.method_result_eligible));
    }
    #[test]
    fn method_eligibility_separates_qualified_name_uses_without_certifying_their_owners() {
        let p=parse("module M\ntype Mode=Build | Other\ntype Result=\n    {Kind:Mode}\n    static member Build(value:string)={Kind=Mode.Build}\n");
        let result = &p.record_expressions[0];
        assert!(result.method_result_eligible);
        assert!(!result.fresh_method_result);
        assert_eq!(result.method_result_uses.len(), 1);
        let usage = result.method_result_uses[0];
        assert_eq!(p.tokens[usage].text, "Build");
        assert_eq!(p.qualifier(usage).as_deref(), Some("Mode"));
        assert_ne!(Some(usage), result.method_result);
    }
    #[test]
    fn method_eligibility_retains_sibling_overload_and_nested_recursive_self_constraints() {
        let p=parse("module M\ntype First={Name:string}\ntype Second={Name:string}\ntype Factory=\n    static member Force():First=Factory.Create(\"x\")\n    static member Create(value:string)={Name=value}\n    static member Create(value:int)={Name=string value}\n");
        for result in &p.record_expressions {
            assert!(result.method_result_eligible);
            assert!(!result.fresh_method_result);
            assert_eq!(result.method_result_uses.len(), 2);
            assert!(result
                .method_result_uses
                .iter()
                .any(|t| p.qualifier(*t).as_deref() == Some("Factory")));
            assert!(result
                .method_result_uses
                .iter()
                .any(|t| p.method_declarations.iter().any(|m| m.token == *t)));
        }
        let p=parse("module M\ntype First={Name:string}\ntype Second={Name:string}\ntype Factory=\n    static member Create(value:string)=\n        let rec constrain():First=Factory.Create(value)\n        {Name=value}\n");
        let result = &p.record_expressions[0];
        assert!(result.method_result_eligible);
        assert!(!result.fresh_method_result);
        assert_eq!(result.method_result_uses.len(), 1);
        assert_eq!(p.tokens[result.method_result_uses[0]].line, 5);
    }
    #[test]
    fn method_constraint_inventory_is_bounded_to_the_type_group_and_retains_structural_guards() {
        let p=parse("module M\ntype Record={Name:string}\ntype Factory=\n    static member Create(value:string)={Name=value}\nlet later=Factory.Create(\"x\")\ntype Consumer=\n    static member Use()=Factory.Create(\"y\")\n");
        let result = &p.record_expressions[0];
        assert!(result.method_result_eligible && result.fresh_method_result);
        assert!(result.method_result_uses.is_empty());
        for source in [
            "module rec M\ntype R={Name:string}\ntype F=\n    static member Create(value:string)={Name=value}\n",
            "module M\ntype R={Name:string}\ntype F=\n    static member Create(value:string)={Name=value}\nand G=\n    static member Value=1\n",
            "module M\ntype R={Name:string}\ntype F=\n    inherit obj()\n    static member Create(value:string)={Name=value}\n",
        ] {let p=parse(source);assert!(p.record_expressions.iter().all(|r|!r.method_result_eligible&&!r.fresh_method_result));}
    }
    #[test]
    fn excessive_method_constraint_uses_make_eligibility_explicitly_unavailable() {
        let uses = "        ignore Factory.Create\n".repeat(65);
        let p=parse(&format!("module M\ntype R={{Name:string}}\ntype Factory=\n    static member Create(value:string)=\n{uses}        {{Name=value}}\n"));
        let result = &p.record_expressions[0];
        assert!(!result.method_result_eligible && !result.fresh_method_result);
        assert_eq!(result.method_result_uses.len(), 64);
    }
    #[test]
    fn union_payload_annotations_and_record_argument_slots_keep_source_identities() {
        let p=parse("module M\ntype First={Name:string}\ntype Second={Name:string}\ntype Event =\n    | Wrap of First\n    | Pair of left:First * right:Second\nlet one=Event.Wrap {Name=\"one\"}\nlet two=Event.Pair({Name=\"left\"},{Name=\"right\"})\n");
        let wrap = p
            .pattern_declarations
            .iter()
            .find(|d| d.name == "Wrap")
            .unwrap();
        let payload = p
            .union_case_payloads
            .iter()
            .find(|d| d.case_token == wrap.token)
            .unwrap();
        assert!(payload.supported);
        assert_eq!(payload.fields.len(), 1);
        assert_eq!(payload.fields[0].type_name, "First");
        let pair = p
            .pattern_declarations
            .iter()
            .find(|d| d.name == "Pair")
            .unwrap();
        let payload = p
            .union_case_payloads
            .iter()
            .find(|d| d.case_token == pair.token)
            .unwrap();
        assert_eq!(
            payload
                .fields
                .iter()
                .map(|f| f.type_name.as_str())
                .collect::<Vec<_>>(),
            ["First", "Second"]
        );
        let records = &p.record_expressions;
        assert_eq!(records.len(), 3);
        let one = records[0].argument.as_ref().unwrap();
        assert_eq!(p.tokens[one.callee].text, "Wrap");
        assert_eq!(p.qualifier(one.callee).as_deref(), Some("Event"));
        assert_eq!((one.position, one.arity), (0, 1));
        assert_ne!(one.callee, wrap.token);
        let left = records[1].argument.as_ref().unwrap();
        let right = records[2].argument.as_ref().unwrap();
        assert_eq!(left.callee, right.callee);
        assert_eq!(p.tokens[left.callee].text, "Pair");
        assert_eq!((left.position, left.arity), (0, 2));
        assert_eq!((right.position, right.arity), (1, 2));
        assert_eq!(
            (left.call_start, left.call_end),
            (right.call_start, right.call_end)
        );
    }
    #[test]
    fn record_argument_context_does_not_guess_shadowed_calls_or_propagate_through_branches() {
        let p=parse("module M\ntype First={Name:string}\ntype Event=Wrap of First\nlet Wrap value=value\nlet shadow=Wrap {Name=\"shadow\"}\nlet viaCall=Event.Wrap(id {Name=\"inner\"})\nlet branch=Event.Wrap(if true then {Name=\"yes\"} else {Name=\"no\"})\nlet curried=f argument {Name=\"last\"}\nlet named=Event.Wrap(value={Name=\"named\"})\n");
        let by_line = |line| {
            p.record_expressions
                .iter()
                .find(|r| p.tokens[r.fields[0]].line == line)
                .unwrap()
        };
        let shadow = by_line(4).argument.as_ref().unwrap();
        assert_eq!(
            p.local_binding(shadow.callee).map(|d| d.name.as_str()),
            Some("Wrap")
        );
        assert_eq!(
            p.tokens[by_line(5).argument.as_ref().unwrap().callee].text,
            "id"
        );
        assert!(p
            .record_expressions
            .iter()
            .filter(|r| p.tokens[r.fields[0]].line == 6)
            .all(|r| r.argument.is_none()));
        assert!(by_line(7).argument.is_none());
        assert!(by_line(8).argument.is_none());
    }
    #[test]
    fn generic_union_payloads_keep_type_variables_without_inventing_instantiation() {
        let p=parse("module M\ntype Envelope<'T> =\n    | Wrap of payload:'T\ntype Pair =\n    | Pair of first:string * second:int\nlet value=Envelope.Wrap {Name=\"x\"}\n");
        let payload = p
            .union_case_payloads
            .iter()
            .find(|p0| p.tokens[p0.case_token].text == "Wrap")
            .unwrap();
        assert_eq!(payload.type_parameters, ["'T"]);
        assert_eq!(payload.fields[0].type_name, "'T");
        assert!(payload.supported);
        let context = p.record_expressions[0].argument.as_ref().unwrap();
        assert_eq!(p.tokens[context.callee].text, "Wrap");
        assert!(p.record_expressions[0].type_name.is_none());
    }
    #[test]
    fn documented_static_factory_result_and_call_keep_distinct_linkable_tokens() {
        let p=parse("namespace Grace.Types\nmodule Artifact =\n    type Metadata={ArtifactId:string}\n    /// Created payload.\n    type Created=\n        {ArtifactId:string}\n        /// Converts metadata.\n        static member FromMetadata(artifact:Metadata)=\n            let normalized=artifact.ArtifactId\n            {ArtifactId=normalized}\n\n    /// Event payload.\n    type Event=\n        {ArtifactId:string}\n        static member FromMetadata(artifact:Metadata)=\n            let created=Created.FromMetadata artifact\n            {ArtifactId=created.ArtifactId}\n");
        let method = p
            .method_declarations
            .iter()
            .find(|m| m.owner == "Grace.Types.Artifact.Created" && m.name == "FromMetadata")
            .unwrap();
        assert!(method.is_static && method.supported);
        assert_eq!(method.parameters[0].type_name, "Metadata");
        let result = p
            .record_expressions
            .iter()
            .find(|r| r.method_result == Some(method.token))
            .unwrap();
        assert!(result.fresh_method_result);
        let created = p.declarations.iter().find(|d| d.name == "created").unwrap();
        let source = p
            .binding_types
            .iter()
            .find(|f| f.binding == created.token)
            .unwrap();
        let TypeSource::FunctionResult {
            function,
            arguments,
        } = source.source
        else {
            panic!("expected method application result fact");
        };
        assert_eq!(arguments, 1);
        assert_ne!(function, method.token);
        assert_eq!(p.tokens[function].text, "FromMetadata");
        assert_eq!(p.qualifier(function).as_deref(), Some("Created"));
        let use_site = p
            .tokens
            .iter()
            .enumerate()
            .find(|(_, t)| t.text == "created" && t.line == 16)
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(
            p.local_binding(use_site).map(|d| d.token),
            Some(created.token)
        );
    }
    #[test]
    fn type_use_inventory_covers_nested_nominals_and_separates_values_and_declarations() {
        let p=parse("module M\ntype Alias = System.Guid\ntype Box<'T>={Value:'T}\ntype Payload = Wrap of (Alias option) * System.Collections.Generic.List<Alias>\nlet make (value:Alias) : Box<Alias> = {Value=value}\nlet useValue = Alias(1)\nlet constructed = new Box<Alias>(1)\nlet typed (x:Alias array) = x\n");
        let uses = p
            .type_uses
            .iter()
            .map(|u| {
                (
                    p.tokens[u.token].line,
                    p.tokens[u.token].text.as_str(),
                    u.name.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert!(uses.contains(&(1, "Guid", "System.Guid")), "{uses:?}");
        assert!(uses.contains(&(3, "option", "option")), "{uses:?}");
        assert!(
            uses.contains(&(3, "List", "System.Collections.Generic.List")),
            "{uses:?}"
        );
        assert!(p
            .type_uses
            .iter()
            .any(|u| u.name == "System.Collections.Generic.List" && u.generic_arity == Some(1)));
        assert!(p
            .type_uses
            .iter()
            .any(|u| u.name == "option" && u.generic_arity == Some(1)));
        assert!(p
            .type_uses
            .iter()
            .filter(|u| u.name == "Alias")
            .all(|u| u.generic_arity == Some(0)));
        assert_eq!(
            uses.iter().filter(|u| u.0 == 3 && u.1 == "Alias").count(),
            2,
            "{uses:?}"
        );
        assert_eq!(
            uses.iter().filter(|u| u.0 == 4 && u.1 == "Alias").count(),
            2,
            "{uses:?}"
        );
        assert!(
            !uses.iter().any(|u| u.0 == 5),
            "constructor value expression is not a type position: {uses:?}"
        );
        assert!(
            !uses.iter().any(|u| u.0 == 6 && u.1 == "Box"),
            "constructor head is not a type navigation role: {uses:?}"
        );
        assert!(
            uses.iter().any(|u| u.0 == 6 && u.1 == "Alias"),
            "explicit constructor generic argument remains a type use: {uses:?}"
        );
        assert!(
            !uses
                .iter()
                .any(|u| u.1 == "T" || (u.0 == 1 && u.1 == "Alias")),
            "declaration and type parameter are absent: {uses:?}"
        );
    }
    #[test]
    fn type_use_inventory_covers_signatures_and_unicode_qualified_names() {
        let p=parse_signature("module Api\ntype Résultat = { Valeur: Modèle.Identité option }\nval convert: Modèle.Identité -> Résultat\n");
        let uses = p
            .type_uses
            .iter()
            .map(|u| {
                (
                    p.tokens[u.token].line,
                    p.tokens[u.token].text.as_str(),
                    u.name.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert!(
            uses.contains(&(1, "Identité", "Modèle.Identité")),
            "{uses:?}"
        );
        assert!(
            uses.contains(&(2, "Identité", "Modèle.Identité")),
            "{uses:?}"
        );
        assert!(uses.contains(&(2, "Résultat", "Résultat")), "{uses:?}");
        let ty = p
            .type_uses
            .iter()
            .find(|u| u.name == "Modèle.Identité")
            .unwrap();
        assert_eq!(p.tokens[ty.token].column, 33);
    }
    #[test]
    fn pipeline_facts_preserve_operator_callee_lambda_scope_and_member_input() {
        let p=parse("module M\nlet select metadata = metadata.Ranges |> Array.choose (fun range -> Some range.OrdinalCount)\nlet typed (xs:Person array) = xs |> Array.choose (fun (person:Person) -> Some person.Name)\n");
        assert_eq!(
            p.pipeline_applications.len(),
            2,
            "{:?}",
            p.pipeline_applications
        );
        let first = &p.pipeline_applications[0];
        assert_eq!(p.tokens[first.operator].text, "|>");
        assert_eq!(p.tokens[first.callable].text, "choose");
        assert_eq!(p.qualifier(first.callable).as_deref(), Some("Array"));
        let TypeSource::Alias { token } = first.input else {
            panic!("member input must retain its token");
        };
        assert_eq!(p.tokens[token].text, "Ranges");
        let PipelineArgument::Lambda {
            parameters,
            body_start,
            body_end,
        } = &first.arguments[0]
        else {
            panic!("expected lambda fact");
        };
        assert_eq!(parameters.len(), 1);
        assert_eq!(p.tokens[parameters[0]].text, "range");
        let d = p.declaration_at(parameters[0]).unwrap();
        assert_eq!((d.visible_start, d.scope_end), (*body_start, *body_end));
        assert!(p.binding_types.iter().any(
            |f| matches!(&f.source,TypeSource::Named{name,..}if name=="Person")
                && p.tokens[f.binding].text == "person"
        ));
    }
    #[test]
    fn pipeline_facts_do_not_cross_application_parentheses_or_infer_direct_call_order() {
        let p=parse("module M\nlet direct xs = Array.choose (fun x -> Some x.Name) xs\nlet partial xs = (xs |> Array.choose) (fun y -> Some y.Name)\nlet correct xs = xs |> Array.choose (fun z -> Some z.Name)\nlet tuple xs = xs |> Array.choose (fun (a,b) -> Some a.Name)\nlet shadow xs = xs |> MyArray.choose (fun q -> Some q.Name)\n");
        assert!(!p
            .pipeline_applications
            .iter()
            .any(|f| p.tokens[f.operator].line == 1));
        let partial = p
            .pipeline_applications
            .iter()
            .find(|f| p.tokens[f.operator].line == 2)
            .unwrap();
        assert!(partial.arguments.is_empty());
        let correct = p
            .pipeline_applications
            .iter()
            .find(|f| p.tokens[f.operator].line == 3)
            .unwrap();
        assert!(matches!(
            &correct.arguments[..],
            [PipelineArgument::Lambda { .. }]
        ));
        let tuple = p
            .pipeline_applications
            .iter()
            .find(|f| p.tokens[f.operator].line == 4)
            .unwrap();
        assert!(matches!(&tuple.arguments[..], [PipelineArgument::Unknown]));
        let shadow = p
            .pipeline_applications
            .iter()
            .find(|f| p.tokens[f.operator].line == 5)
            .unwrap();
        assert_eq!(p.qualifier(shadow.callable).as_deref(), Some("MyArray"));
    }
    #[test]
    fn pipeline_unicode_nested_lambdas_and_unknown_input_remain_scoped() {
        let p=parse("module M\nlet f données = données |> Array.choose (fun élément -> let inner=fun élément -> élément.Name in Some élément.Name)\nlet unknown xs = (if true then xs else xs) |> Array.choose (fun x -> Some x.Name)\n");
        assert_eq!(p.pipeline_applications.len(), 1);
        let f = &p.pipeline_applications[0];
        let PipelineArgument::Lambda {
            parameters,
            body_start,
            ..
        } = &f.arguments[0]
        else {
            panic!("expected lambda");
        };
        assert_eq!(p.tokens[parameters[0]].text, "élément");
        let uses: Vec<_> = p
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.text == "élément" && t.start >= *body_start)
            .map(|(i, _)| p.local_binding(i).map(|d| d.token))
            .collect();
        assert_eq!(uses.last().copied().flatten(), Some(parameters[0]));
        assert!(uses.iter().any(|x| x.is_some_and(|t| t != parameters[0])));
    }

    #[test]
    fn factory_forwarding_requires_direct_fields_and_closed_attribute_arguments() {
        let source = "module M\n[<First; Second>]\ntype Item =\n    { [<Marker(3u)>] Name:string; Number:int }\n    static member Build(value:string) = {Name=value; Number=3}\n";
        let p = parse(source);
        let fields = &p.record_expressions.last().unwrap().factory_atoms;
        assert!(matches!(
            fields.as_slice(),
            [
                (_, FactoryAtom::Identifier(_)),
                (
                    _,
                    FactoryAtom::Literal(IntrinsicLiteral::UnsuffixedDecimalInt32)
                )
            ]
        ));
        for changed in [
            source.replace("3u", "Constants.Value"),
            source.replace("3u", "typeof<Item>"),
            source.replace("First; Second", "First(typeof<Item>); Second"),
            source.replace("Name=value", "Name=(fun () -> value)()"),
            source.replace("Number=3", "Number=1+2"),
            source.replace("{Name=value", "{Item.Name=value"),
            source.replace(
                "static member Build",
                "static member Count=0\n    static member Build",
            ),
        ] {
            let p = parse(&changed);
            assert!(
                p.record_expressions
                    .iter()
                    .all(|r| r.factory_atoms.is_empty()),
                "{changed}"
            );
        }
    }

    #[test]
    fn literal_method_group_admits_only_closed_intrinsic_value_trees() {
        let bodies = [
            (r#"{Name="literal $ { inert }"}"#, "string"),
            (r#"{Name=@"literal $ { inert }"}"#, "string"),
            (r####"{Name="""literal $ { inert }"""}"####, "string"),
            (r#"({Name="x"})"#, "string"),
            (r#"{Name={Child="x"}}"#, "Inner"),
            (
                r#"{Name=("x",true,(),123_456)}"#,
                "string * bool * unit * int",
            ),
            (r#"{Name=["x";"y"]}"#, "string list"),
            (r#"{Name=[|"x";"y"|]}"#, "string array"),
            (r#"{Name=[]}"#, "string list"),
            (r#"{Name=[||]}"#, "string array"),
        ];
        for (body, field_type) in bodies {
            let source=format!("module M\ntype Inner={{Child:string}}\ntype Result={{Name:{field_type}}}\ntype Factory=\n    static member Empty={body}\n");
            let p = parse(&source);
            let result = p
                .record_expressions
                .iter()
                .find(|r| r.method_result.is_some())
                .expect(&source);
            assert!(result.method_result_literal_group, "{source}");
        }
        for header in [
            "static member Empty",
            "static member Create()",
            "static member Create(input:Input)",
        ] {
            let source=format!("module M\ntype Input={{Text:string}}\ntype Factory=\n    {{Name:string}}\n    {header}={{Name=\"fixed\"}}\n");
            let p = parse(&source);
            let result = p
                .record_expressions
                .iter()
                .find(|r| r.method_result.is_some())
                .expect(&source);
            assert!(result.method_result_literal_group, "{source}");
        }
    }
    #[test]
    fn literal_method_group_refuses_nonliteral_value_constraints() {
        for value in [
            "value",
            "nameof Factory",
            "Helpers.force<Factory>()",
            "String.Empty",
            "Factory.Empty",
            "(\"x\" : string)",
            "(let x=1 in \"x\")",
            "(if true then \"x\" else \"y\")",
            "(fun () -> \"x\")",
            "<@ Helpers.force<Factory>() @>",
            "task { return \"x\" }",
            "$\"{Helpers.force<Factory>()}\"",
            "$@\"{Factory.Empty}\"",
            "@$\"{Factory.Empty}\"",
            "$\"\"\"{Factory.Empty}\"\"\"",
            "$$\"\"\"{{Factory.Empty}}\"\"\"",
            "1I",
            "1Q",
            "1R",
            "1Z",
            "1N",
            "1G",
            "1L",
            "1.0",
            "1m",
            "0x1",
            "-1",
            "+1",
            "[1..10]",
            "[for x in [1] -> x]",
            "[|for x in [1] -> x|]",
            "[|1..10|]",
            "{|Child=\"x\"|}",
            "{other with Child=\"x\"}",
            "null",
        ] {
            let source =
                format!("module M\ntype Factory=\n    static member Empty={{Name={value}}}\n");
            let p = parse(&source);
            assert!(
                p.record_expressions
                    .iter()
                    .all(|r| !r.method_result_literal_group),
                "{source}"
            );
        }
    }
    #[test]
    fn literal_method_group_refuses_unclosed_checking_group_headers() {
        for tail in [
            "type Factory()=\n    static member Empty={Name=\"x\"}",
            "type Factory() as self=\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    static member Other=1\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    member this.Other=1\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    {Tag:int}\n    static let cached={Tag=0}\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    inherit obj()\n    static member Empty={Name=\"x\"}",
            "type Factory<'T>=\n    static member Empty={Name=\"x\"}",
            "[<SomeAttribute>]\ntype Factory=\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    [<SomeAttribute>]\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    static member Empty<'T>()={Name=\"x\"}",
            "type Factory=\n    static member Empty(?x:string)={Name=\"x\"}",
            "type Factory=\n    static member Empty([<SomeAttribute>] x:string)={Name=\"x\"}",
            "type Factory=\n    static member Empty(x:Factory)={Name=\"x\"}",
            "type Factory=\n    static member Empty(x:Factory list)={Name=\"x\"}",
            "type Factory=\n    {Slot:Factory}\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    {Slot:``Factory``}\n    static member Empty={Name=\"x\"}",
            "type ``Factory``=\n    static member Empty(x:Factory)={Name=\"x\"}",
            "type Factory=\n    {Slot:Factory Constrained}\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    {Slot:Constrained<Factory>}\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    {Slot:int -> string}\n    static member Empty={Name=\"x\"}",
            "type Factory=\n    static member Empty={Name=\"x\"}\nand Other={Tag:int}",
        ] {
            let source=format!("module M\ntype Result={{Name:string}}\n{tail}\n");
            let p=parse(&source);
            assert!(p.record_expressions.iter().all(|r|!r.method_result_literal_group),"{source}");
        }
        let source = "module rec M\ntype Factory=\n    static member Empty={Name=\"x\"}\n";
        assert!(parse(source)
            .record_expressions
            .iter()
            .all(|r| !r.method_result_literal_group));
        let malformed = "module M\ntype Factory=\n    static member Empty={Name=[\"x\"}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_fsharp::LANGUAGE_FSHARP.into())
            .unwrap();
        assert!(parser
            .parse(malformed, None)
            .unwrap()
            .root_node()
            .has_error());
        assert!(parse(malformed)
            .record_expressions
            .iter()
            .all(|r| !r.method_result_literal_group));
    }

    #[test]
    fn literal_method_group_rejects_reviewed_srtp_constraint_channels() {
        // Actual FCS source controls retained in factory-constraint-envelope-oracle.json.
        for (name, source, expected) in [
            (
                "plain_positive",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        { Name = "x" }
"#####,
                true,
            ),
            (
                "unconstrained_defaultof",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let ignored = Helpers.force Unchecked.defaultof<_>
        { Name = "x" }
"#####,
                false,
            ),
            (
                "constrained_generic_return",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let force seed =
            let ignored = seed.Tag
            Helpers.force seed
        let ignored = force (List.head [])
        { Name = "x" }
"#####,
                false,
            ),
            (
                "earlier_instance_self",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    member this.Force() = Helpers.force this
    static member Empty =
        { Name = "x" }
"#####,
                false,
            ),
            (
                "later_instance_self",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        { Name = "x" }
    member this.Force() = Helpers.force this
"#####,
                false,
            ),
            (
                "earlier_instance_self_alias",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    member this.Force() =
        let item = this
        Helpers.force item
    static member Empty =
        { Name = "x" }
"#####,
                false,
            ),
            (
                "uninvoked_lambda",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let ignored = fun () -> Helpers.force { Tag = 0 }
        { Name = "x" }
"#####,
                false,
            ),
            (
                "quoted_application",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let ignored = <@ Helpers.force { Tag = 0 } @>
        { Name = "x" }
"#####,
                false,
            ),
            (
                "unreachable_branch",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let ignored = if false then Some(Helpers.force { Tag = 0 }) else None
        { Name = "x" }
"#####,
                false,
            ),
            (
                "operator_application",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
    let inline ( !@ ) (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let ignored = Helpers.(!@) { Tag = 0 }
        { Name = "x" }
"#####,
                false,
            ),
            (
                "partial_explicit_application",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let ignored = Helpers.explicit<Factory>
        { Name = "x" }
"#####,
                false,
            ),
            (
                "earlier_member_partial_explicit",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Force = Helpers.explicit<Factory>
    static member Empty =
        { Name = "x" }
"#####,
                false,
            ),
            (
                "staticlet_in_record",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static let cached = { Tag = 0 }
    static let forced = Helpers.force cached
    static member Empty =
        { Name = "x" }
"#####,
                false,
            ),
            (
                "nested_local_function",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let forceLocal x = Helpers.force x
        let ignored = forceLocal { Tag = 0 }
        { Name = "x" }
"#####,
                false,
            ),
            (
                "ordinary_unconstrained_call_positive",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let ignored = System.String.IsNullOrEmpty "x"
        { Name = "x" }
"#####,
                false,
            ),
            (
                "later_outside_call",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        { Name = "x" }
let ignored = Helpers.force { Tag = 0 }
"#####,
                true,
            ),
            (
                "lambda_value_escaped",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let ignored = [fun () -> Helpers.force { Tag = 0 }]
        { Name = "x" }
"#####,
                false,
            ),
            (
                "field_projection_inferred_self",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Empty =
        let force seed =
            let ignored = seed.Tag
            Helpers.force seed
        let ignored = force Unchecked.defaultof<_>
        { Name = "x" }
"#####,
                false,
            ),
            (
                "earlier_field_projection_sibling",
                r#####"type First = { Name: string }
module Helpers =
    let inline force (x: ^T) : First =
        ((^T) : (static member Empty : First) ())
    let inline explicit< ^T when ^T : (static member Empty : First)> () =
        ((^T) : (static member Empty : First) ())
type Second = { Name: string }
type Factory =
    { Tag: int }
    static member Force(seed) =
        let ignored = seed.Tag
        Helpers.force seed
    static member Empty =
        { Name = "x" }
"#####,
                false,
            ),
        ] {
            let p = parse(source);
            let result = p
                .record_expressions
                .iter()
                .rev()
                .find(|r| r.method_result.is_some_and(|t| p.tokens[t].text == "Empty"))
                .unwrap_or_else(|| panic!("No Empty result: {name}"));
            assert_eq!(
                result.method_result_literal_group, expected,
                "{name}: {source}"
            );
        }
    }
    #[test]
    fn documented_static_record_tails_retain_method_and_nested_expected_type_facts() {
        let p=parse("namespace Grace.Types\nmodule Contracts =\n    /// Public data.\n    [<CLIMutable>]\n    type Jwk =\n        { Kty:string; Crv:string }\n        /// Builds the data.\n        static member Create(kty:string, crv:string)={Kty=kty;Crv=crv}\n\n    /// Next declaration's documentation can be a trailing xml_doc method child.\n    type Queue = { OnFailure:string }\n    /// Defaults.\n    type Policy =\n        { Queue:Queue }\n        /// A static property.\n        static member Default={Queue={OnFailure=\"pause\"}}\n\n    /// Following record.\n    type Last={Name:string}\n");
        let create = p
            .record_expressions
            .iter()
            .find(|r| p.tokens[r.fields[0]].text == "Kty")
            .unwrap();
        assert_eq!(
            create.method_result.map(|t| p.tokens[t].text.as_str()),
            Some("Create")
        );
        assert!(create.fresh_method_result);
        let root = p
            .record_expressions
            .iter()
            .find(|r| p.tokens[r.fields[0]].text == "Queue")
            .unwrap();
        assert_eq!(
            root.method_result.map(|t| p.tokens[t].text.as_str()),
            Some("Default")
        );
        assert!(root.fresh_method_result);
        let nested = p
            .record_expressions
            .iter()
            .find(|r| p.tokens[r.fields[0]].text == "OnFailure")
            .unwrap();
        assert_eq!(nested.parent_field, Some(root.fields[0]));
        assert!(nested.method_result.is_none());
    }
    #[test]
    fn static_record_tail_facts_cover_guard_sequences_and_let_continuations() {
        let p=parse("module M\ntype First={Name:string}\ntype Second=\n    {Name:string}\n    static member Create(value:string)=\n        if value=\"\" then failwith \"required\"\n        let normalized=value\n        {Name=normalized}\n    static member Typed(value:string):First=\n        if value=\"\" then failwith \"required\"\n        {Name=value}\n");
        assert_eq!(p.record_expressions.len(), 2);
        let a = &p.record_expressions[0];
        assert_eq!(
            a.method_result.map(|i| p.tokens[i].text.as_str()),
            Some("Create")
        );
        assert!(a.fresh_method_result);
        assert!(a.binding.is_none() && a.type_name.is_none());
        let b = &p.record_expressions[1];
        assert_eq!(b.type_name.as_deref(), Some("First"));
        assert!(!b.fresh_method_result);
    }
    #[test]
    fn static_record_freshness_rejects_prior_constraints_and_recursive_type_groups() {
        for tail in [
            "type Factory=\n    static member Force():First=Factory.Create(\"x\")\n    static member Create(value:string)={Name=value}\n",
            "type Consumer=\n    static member Force():First=Factory.Create(\"x\")\nand Factory=\n    static member Create(value:string)={Name=value}\n",
            "type Factory=\n    inherit obj()\n    static member Create(value:string)={Name=value}\n",
            "type Factory<'T>=\n    static member Create(value:string)={Name=value}\n",
            "type Factory=\n    static member Create(value:string)={Name=value}\n    static member Create(value:int)={Name=string value}\n",
            "type Factory=\n    static member inline Create(value:string)={Name=value}\n",
        ]{let p=parse(&format!("module M\ntype First={{Name:string}}\ntype Second={{Name:string}}\n{tail}"));assert!(!p.record_expressions.is_empty(),"{tail}");assert!(p.record_expressions.iter().all(|r|!r.fresh_method_result),"{tail}");}
    }
    #[test]
    fn static_properties_retain_fresh_nested_records_but_recursive_modules_do_not() {
        let source="module M\ntype Inner={Name:string}\ntype Outer=\n    {Inner:Inner}\n    static member Default={Inner={Name=\"x\"}}\n    static member Create()={Inner={Name=\"y\"}}\n";
        let p = parse(source);
        let roots: Vec<_> = p
            .record_expressions
            .iter()
            .filter(|r| r.parent_field.is_none())
            .collect();
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().all(|r| r.fresh_method_result));
        for prefix in ["module rec M", "namespace rec M"] {
            let p = parse(&source.replacen("module M", prefix, 1));
            assert!(
                p.record_expressions.iter().all(|r| !r.fresh_method_result),
                "{prefix}"
            );
        }
    }
    #[test]
    fn record_method_results_do_not_cross_branches_calls_or_nested_fields() {
        let p=parse("module M\ntype A={Name:string}\ntype Factory=\n    static member Create(value:string)=if value=\"\" then {Name=\"x\"} else {Name=value}\n    static member Wrapped(value:string)=id {Name=value}\n    static member Nested(value:string)={Inner={Name=value}}\n");
        assert_eq!(p.record_expressions.len(), 5);
        assert!(p
            .record_expressions
            .iter()
            .filter(|r| p.tokens[r.fields[0]].text == "Name")
            .all(|r| r.method_result.is_none()));
        assert!(
            p.record_expressions
                .iter()
                .find(|r| p.tokens[r.fields[0]].text == "Inner")
                .unwrap()
                .fresh_method_result
        );
    }
    #[test]
    fn alias_facts_preserve_declaration_and_rhs_type_tokens_separately() {
        let p=parse("module Common\ntype BranchId=System.Guid\ntype ParentBranchId=BranchId\nlet BranchId=1\ntype Alias<'T>=System.Collections.Generic.List<'T>\n");
        let a = p
            .type_aliases
            .iter()
            .find(|a| a.name == "Common.ParentBranchId")
            .unwrap();
        assert_eq!(p.tokens[a.token].text, "ParentBranchId");
        assert_eq!(p.tokens[a.target_token].text, "BranchId");
        assert!(p.declaration_at(a.target_token).is_none());
        assert_eq!(p.tokens[a.target_token].line, 2);
    }
    #[test]
    fn pattern_payload_facts_preserve_constructor_and_scrutinee_identity() {
        let p=parse("module M\ntype Customer={Name:string}\nlet f (opt:Customer option)=\n    match opt with\n    | Some customer -> customer.Name\n    | None -> \"none\"\nlet g ({Name=name}:Customer)=name\n");
        assert_eq!(p.pattern_bindings.len(), 1);
        let f = &p.pattern_bindings[0];
        assert_eq!(p.tokens[f.binding].text, "customer");
        assert_eq!(p.tokens[f.scrutinee].text, "opt");
        assert_eq!(p.tokens[f.constructor].text, "Some");
        assert_eq!(p.record_pattern_bindings.len(), 1);
    }
    #[test]
    fn no_base_marker_is_not_used_for_an_unresolved_nominal_base() {
        let p =
            parse("module M\ntype Derived()=\n    inherit Base<int>()\n    member _.Name=\"x\"\n");
        assert!(p
            .record_types
            .iter()
            .find(|t| t.name == "M.Derived")
            .unwrap()
            .base_type
            .is_some());
    }
    #[test]
    fn static_method_facts_preserve_scoped_annotations_and_literal_categories() {
        let p=parse("module M\ntype Picker=\n    static member Pick(value:int)=string value\n    static member Pick(value:string)=value\nlet a=Picker.Pick(1)\nlet b=Picker.Pick(\"one\")\n");
        assert_eq!(p.method_declarations.len(), 2);
        assert!(p
            .method_declarations
            .iter()
            .all(|m| m.supported && m.is_static));
        assert_eq!(p.method_declarations[0].parameters[0].type_name, "int");
        assert_eq!(p.method_declarations[1].parameters[0].type_name, "string");
        assert_eq!(p.method_calls.len(), 2);
        assert_eq!(
            p.method_calls[0].args,
            [ArgumentSource::IntrinsicLiteral(
                IntrinsicLiteral::UnsuffixedDecimalInt32
            )]
        );
        assert_eq!(
            p.method_calls[1].args,
            [ArgumentSource::IntrinsicLiteral(IntrinsicLiteral::String)]
        );
    }
    #[test]
    fn unsupported_overload_headers_remain_family_blockers() {
        let p=parse("module M\ntype Picker=\n    static member Pick(value:int)=value\n    static member Pick<'T>(value:'T)=value\n    static member Pick(?value:int)=value\n    static member Pick([<System.ParamArray>] values:int array)=values\n    member _.Pick(value:string)=value\n    static member Pick value other=value\n    static member Pick=1\n");
        let methods: Vec<_> = p
            .method_declarations
            .iter()
            .filter(|m| m.name == "Pick")
            .collect();
        assert_eq!(methods.len(), 7);
        assert_eq!(methods.iter().filter(|m| m.supported).count(), 1);
    }
    #[test]
    fn method_literal_facts_reject_operators_suffixes_interpolation_and_unknown_arguments() {
        let p=parse("module M\nlet a=C.Pick(-1)\nlet b=C.Pick(- 1)\nlet c=C.Pick(1L)\nlet d=C.Pick($\"x\")\nlet e=C.Pick(x)\nlet f=C.Pick(2147483648)\nlet g=C.Pick(1,\"x\")\nlet h=C.Pick((1))\n");
        let calls = &p.method_calls;
        assert_eq!(calls.len(), 8);
        assert_eq!(
            calls[0].args,
            [ArgumentSource::IntrinsicLiteral(
                IntrinsicLiteral::UnsuffixedDecimalInt32
            )]
        );
        for call in &calls[1..6] {
            assert_eq!(call.args, [ArgumentSource::Unknown]);
        }
        assert_eq!(
            calls[6].args,
            [
                ArgumentSource::IntrinsicLiteral(IntrinsicLiteral::UnsuffixedDecimalInt32),
                ArgumentSource::IntrinsicLiteral(IntrinsicLiteral::String)
            ]
        );
        assert_eq!(calls[7].args, calls[0].args);
    }
    #[test]
    fn pattern_inventory_distinguishes_declarations_uses_and_scopes() {
        let p=parse("module M\n[<RequireQualifiedAccess>]\ntype Custom = | Some of int | None\nlet useCase value = match value with | Custom.Some x -> x | Custom.None -> 0\nlet outer value =\n    let (|Some|_|) x = Some x\n    match value with | Some y -> y | _ -> 0\nlet untouched x = x\n");
        let cases: Vec<_> = p
            .pattern_declarations
            .iter()
            .filter(|d| d.kind == "union-case")
            .collect();
        assert_eq!(cases.len(), 2);
        assert!(cases
            .iter()
            .all(|d| !d.qualified_only && d.type_owner.as_deref() == Some("M.Custom")));
        let active: Vec<_> = p
            .pattern_declarations
            .iter()
            .filter(|d| d.kind == "active-pattern")
            .collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "Some");
        assert!(active[0].scope_end < p.source_len);
        assert!(p.binding_supported(p.token_at(7, 18).unwrap()));
    }
    #[test]
    fn explicit_inheritance_and_generic_alias_parameters_are_preserved() {
        let p=parse("module M\ntype Base<'T>(value:'T) =\n    member _.Value:'T = value\ntype Child() =\n    inherit Base<string>(\"x\")\ntype Alias<'T> = Base<'T>\nlet constructed = new Base<string>(\"x\")\n");
        let child = p.record_types.iter().find(|t| t.name == "M.Child").unwrap();
        assert_eq!(child.base_type.as_deref(), Some("Base<string>"));
        assert_eq!(p.tokens[child.base_type_token.unwrap()].text, "Base");
        let alias = p.type_aliases.iter().find(|t| t.name == "M.Alias").unwrap();
        assert_eq!(alias.type_parameters, ["'T"]);
        assert_eq!(alias.target, "Base<'T>");
        assert!(p
            .binding_types
            .iter()
            .any(|f| f.binding == declaration_token(&p, "constructed")
                && matches!(&f.source,TypeSource::Named{name,..} if name=="Base<string>")));
    }
    #[test]
    fn anonymous_record_facts_exclude_rhs_uses_and_retain_nested_label_identity() {
        let p=parse("module M\nlet value = \"value\"\nlet structural = {| Name=value; Other={|Name=value|} |}\nlet result = structural.Name\n");
        assert_eq!(p.anonymous_record_labels.len(), 3);
        assert_eq!(
            p.anonymous_record_bindings,
            [declaration_token(&p, "structural")]
        );
        let value_uses: Vec<_> = p
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.text == "value" && t.line == 2)
            .filter(|(i, _)| !p.anonymous_record_labels.contains(i))
            .collect();
        assert_eq!(value_uses.len(), 2);
        assert!(value_uses.iter().all(|(i, _)| p
            .local_binding(*i)
            .is_some_and(|d| d.token == declaration_token(&p, "value"))));
    }
    #[test]
    fn class_identity_auto_properties_and_static_properties_are_distinct() {
        let p=parse("module M\nlet Name = 1\ntype Empty() = class end\ntype C() =\n    member val Name = \"x\" with get,set\n    member val Age:int = 1 with get\n    static member Code:string = \"c\"\n");
        let empty = p.record_types.iter().find(|t| t.name == "M.Empty").unwrap();
        assert_eq!(empty.kind, "class");
        assert!(empty.fields.is_empty());
        let c = p.record_types.iter().find(|t| t.name == "M.C").unwrap();
        assert_eq!(
            c.fields
                .iter()
                .map(|f| (f.name.as_str(), f.kind.as_str()))
                .collect::<Vec<_>>(),
            [
                ("Name", "property"),
                ("Age", "property"),
                ("Code", "static-property")
            ]
        );
        assert_eq!(c.fields[1].type_name.as_deref(), Some("int"));
        assert!(p.local_binding(c.fields[0].token).is_none());
    }
    #[test]
    fn literal_projection_preserves_navigation_identity_and_records_provenance() {
        let source = format!(
            "// {}\nmodule M\nlet target x = x\nlet data = [|{}1|]\nlet answer = target 1\n",
            "padding".repeat(10000),
            "0;".repeat(1024)
        );
        let projected = parse(&source);
        let original = parse_impl_with_projection(&source, false, false);
        assert_eq!(projected.literal_islands.len(), 1);
        assert_eq!(projected.literal_islands[0].element_count, 1025);
        assert_eq!(
            serde_json::to_value(&projected.tokens).unwrap(),
            serde_json::to_value(&original.tokens).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&projected.declarations).unwrap(),
            serde_json::to_value(&original.declarations).unwrap()
        );
        let usage = projected
            .tokens
            .iter()
            .enumerate()
            .find(|(_, t)| t.text == "target" && t.line == 4)
            .unwrap()
            .0;
        let declaration = projected.local_binding(usage).unwrap();
        assert_eq!(projected.tokens[declaration.token].line, 2);
        assert_eq!(
            projected.local_binding(usage).unwrap().token,
            original.local_binding(usage).unwrap().token
        );
    }
    #[test]
    fn projected_expressions_withhold_shape_facts_but_keep_original_annotations() {
        let source=format!("// {}\nmodule M\ntype P = {{Name:string}}\nlet make (values:int array) : P = {{Name=\"x\"}}\nlet value:P = make [|{}1|]\nlet result = value.Name\n","padding".repeat(10000),"0;".repeat(1024));
        let p = parse(&source);
        let value = declaration_token(&p, "value");
        let facts: Vec<_> = p
            .binding_types
            .iter()
            .filter(|f| f.binding == value)
            .collect();
        assert_eq!(facts.len(), 1);
        assert!(matches!(&facts[0].source,TypeSource::Named{name,..} if name=="P"));
        assert!(!p.literal_islands.is_empty());
    }
    #[test]
    fn expression_receivers_and_record_patterns_preserve_explicit_evidence() {
        let p=parse("module M\ntype Person = {Name:string; Age:int}\nlet make name : Person = {Name=name; Age=1}\nlet f ({Name=name}:Person) = name\nlet g (p:Person) = match p with | {Name=name} -> name\nlet returned = (make \"x\").Name\nlet casted (p:Person) = (p).Name\n");
        assert!(p.member_accesses.iter().any(|a|matches!(&a.source,Some(TypeSource::FunctionResult{function,arguments:1}) if p.tokens[*function].text=="make")));
        assert!(p.member_accesses.iter().any(
            |a| matches!(&a.source,Some(TypeSource::Alias{token}) if p.tokens[*token].text=="p")
        ));
        assert!(p
            .record_expressions
            .iter()
            .any(|r| r.type_name.as_deref() == Some("Person") && p.tokens[r.fields[0]].line == 3));
        assert!(p
            .record_expressions
            .iter()
            .any(|r| r.copy.is_some_and(|t| p.tokens[t].text == "p")
                && p.tokens[r.fields[0]].line == 4));
    }
    #[test]
    fn bare_type_alias_candidates_exclude_explicit_union_cases_and_payloads() {
        let p=parse("module M\ntype Person = {Name:string}\ntype Alias = Person\ntype Explicit = | Person\ntype Payload = Person of int\n");
        assert_eq!(p.type_aliases.len(), 1);
        assert_eq!(p.type_aliases[0].name, "M.Alias");
        assert_eq!(p.type_aliases[0].target, "Person");
    }
    #[test]
    fn trailing_xml_docs_preserve_bare_alias_candidates_without_reclassifying_union_shapes() {
        let source = "namespace Grace.Types\nmodule Policy =\n    /// Snapshot identity.\n    type PolicySnapshotId = Sha256Hash\n\n    /// Controls token budgets.\n    [<GenerateSerializer>]\n    type Settings = { Maximum: int }\n\n    type Explicit = | Sha256Hash\n    /// A payload is not an alias.\n    type Payload = Sha256Hash of int\n    /// This declaration remains independent.\n    type End = { Name: string }\n";
        let p = parse(source);
        assert!(p.supported);
        assert!(p.uncertain.is_empty());
        assert_eq!(p.type_aliases.len(), 1);
        let alias = &p.type_aliases[0];
        assert_eq!(alias.name, "Grace.Types.Policy.PolicySnapshotId");
        assert_eq!(alias.target, "Sha256Hash");
        assert_eq!(p.tokens[alias.target_token].line, 3);
        assert_eq!(p.tokens[alias.target_token].column, 28);
        assert_eq!(p.tokens[alias.token].text, "PolicySnapshotId");
        assert_eq!(p.attributes.len(), 1);
        assert_eq!(p.attributes[0].type_name, "GenerateSerializer");
    }
    #[test]
    fn fresh_parameter_member_facts_require_an_unconstrained_whole_body() {
        let p=parse("module M\ntype P = {Name:string}\nlet direct x = (x.Name)\nlet typed (x:P) = x.Name\nlet result x : string = x.Name\nlet rec recursive x = x.Name\nlet inline inlined x = x.Name\nlet statements x = ignore x; x.Name\nlet chained x = x.Name.Length\nlet extra x y = x.Name\n");
        assert_eq!(p.unconstrained_members.len(), 1);
        let fact = &p.unconstrained_members[0];
        assert_eq!(p.tokens[fact.function].text, "direct");
        assert_eq!(p.tokens[fact.parameter].text, "x");
        assert_eq!(p.tokens[fact.token].text, "Name");
    }
    #[test]
    fn cancellation_refuses_a_partial_syntax_summary() {
        let cancellation = crate::cancellation::CancellationToken::new();
        let _scope = cancellation.enter();
        cancellation.cancel();
        let p = parse("module M\nlet value = 1\nlet result = value\n");
        assert!(p.tokens.is_empty());
        assert!(!p.supported);
        assert!(p.reasons.iter().any(|s| s.contains("cancelled")));
    }
    fn declaration_token(p: &ParsedFile, name: &str) -> usize {
        p.declarations
            .iter()
            .find(|d| d.name == name)
            .unwrap()
            .token
    }
    #[test]
    fn record_facts_preserve_owners_annotations_and_receiver_chains() {
        let p=parse("namespace N\nmodule A =\n    type Address = { City:string }\n    type Person = { Name:string; Address:Address }\n    let get (person:Person) = person.Address.City\nmodule B =\n    type Person = { Name:int }\n    let get (person:Person) = person.Name\n");
        assert_eq!(
            p.record_types
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["N.A.Address", "N.A.Person", "N.B.Person"]
        );
        assert_eq!(
            p.record_types[1].fields[1].type_name.as_deref(),
            Some("Address")
        );
        assert_eq!(
            p.record_types[1].fields[0].name,
            p.record_types[2].fields[0].name
        );
        let facts: Vec<_> = p
            .binding_types
            .iter()
            .filter(|f| p.tokens[f.binding].text == "person")
            .collect();
        assert_eq!(facts.len(), 2);
        assert_ne!(facts[0].binding, facts[1].binding);
        for fact in facts {
            assert!(matches!(&fact.source,TypeSource::Named{name,..} if name=="Person"));
        }
        let city = p
            .tokens
            .iter()
            .position(|t| t.text == "City" && t.line == 4)
            .unwrap();
        let access = p.member_accesses.iter().find(|a| a.token == city).unwrap();
        assert_eq!(p.tokens[access.receiver].text, "Address");
        let address = p
            .member_accesses
            .iter()
            .find(|a| a.token == access.receiver)
            .unwrap();
        assert_eq!(p.tokens[address.receiver].text, "person");
    }
    #[test]
    fn record_initializers_aliases_updates_and_return_annotations_are_separate_facts() {
        let p=parse("module M\ntype Person = { Name:string; Age:int }\nlet original: Person = { Person.Name=\"A\"; Age=1 }\nlet alias = original\nlet copied = { alias with Name=\"B\" }\nlet make (name,age) extra : Person = {Name=name; Age=age+extra}\nlet value = make (\"C\",1) 2\nlet partial = make (\"D\",1)\n");
        let original = declaration_token(&p, "original");
        let facts: Vec<_> = p
            .binding_types
            .iter()
            .filter(|f| f.binding == original)
            .collect();
        assert_eq!(facts.len(), 2);
        assert!(facts
            .iter()
            .any(|f| matches!(&f.source,TypeSource::Named{name,..} if name=="Person")));
        let labels = facts
            .iter()
            .find_map(|f| match &f.source {
                TypeSource::Record { fields, copy: None } => Some(fields),
                _ => None,
            })
            .unwrap();
        assert_eq!(p.qualifier(labels[0]).as_deref(), Some("Person"));
        let copied = declaration_token(&p, "copied");
        assert!(p.binding_types.iter().any(|f|f.binding==copied&&matches!(&f.source,TypeSource::Record{copy:Some(token),..} if p.tokens[*token].text=="alias")));
        assert_eq!(p.function_returns[0].arity, 2);
        assert_eq!(p.function_returns[0].type_name, "Person");
        let returned = p
            .record_expressions
            .iter()
            .find(|e| {
                e.binding == Some(declaration_token(&p, "make"))
                    && e.type_name.as_deref() == Some("Person")
            })
            .unwrap();
        assert_eq!(returned.fields.len(), 2);
        assert!(!p
            .binding_types
            .iter()
            .any(|f| f.binding == declaration_token(&p, "make")));
        for (name, arity) in [("value", 2), ("partial", 1)] {
            let binding = declaration_token(&p, name);
            assert!(p.binding_types.iter().any(|f|f.binding==binding&&matches!(&f.source,TypeSource::FunctionResult{arguments,..} if *arguments==arity)));
        }
    }
    #[test]
    fn record_metadata_keeps_generic_syntax_and_distinguishes_static_properties() {
        let p=parse("module M\n[<RequireQualifiedAccess>]\ntype Secret = private { Name:string }\ntype Box<'a> = { Value:'a; Values:'a list }\ntype Alias = Box<int>\ntype C() =\n    static member Name = \"static\"\n    member _.Value : string = \"instance\"\n    member _.Method(x:int) = x\n");
        let secret = p
            .record_types
            .iter()
            .find(|t| t.name == "M.Secret")
            .unwrap();
        assert!(!secret.require_qualified_access);
        assert!(p
            .attributes
            .iter()
            .any(|a| a.owner == secret.name && a.type_name == "RequireQualifiedAccess"));
        assert_eq!(secret.accessibility, "public");
        assert_eq!(secret.fields[0].accessibility, "private");
        let generic = p.record_types.iter().find(|t| t.name == "M.Box").unwrap();
        assert_eq!(generic.fields[0].type_name.as_deref(), Some("'a"));
        assert_eq!(generic.fields[1].type_name.as_deref(), Some("'a list"));
        assert_eq!(p.type_aliases[0].target, "Box<int>");
        let class = p.record_types.iter().find(|t| t.name == "M.C").unwrap();
        assert_eq!(class.kind, "class");
        assert_eq!(class.fields.len(), 2);
        assert_eq!(class.fields[0].name, "Name");
        assert_eq!(class.fields[0].kind, "static-property");
        assert_eq!(class.fields[1].name, "Value");
        assert_eq!(class.fields[1].kind, "property");
    }
    #[test]
    fn record_facts_use_utf16_ranges_and_do_not_turn_constructor_patterns_into_annotations() {
        let p=parse("module M\ntype Person = { ``😀 Name``:string }\nlet f (person:Person) = person.``😀 Name``\nlet g (Some person:Person option) = person\n");
        let field = &p.record_types[0].fields[0];
        let token = &p.tokens[field.token];
        assert_eq!(token.end_column - token.column, 11);
        let uses: Vec<_> = p
            .member_accesses
            .iter()
            .filter(|a| p.tokens[a.token].text == "😀 Name")
            .collect();
        assert_eq!(uses.len(), 1);
        let person_facts: Vec<_> = p
            .binding_types
            .iter()
            .filter(|f| p.tokens[f.binding].text == "person")
            .collect();
        assert_eq!(person_facts.len(), 1);
        assert_eq!(p.tokens[person_facts[0].binding].line, 2);
    }
    #[test]
    fn signature_function_results_preserve_curried_arity() {
        let p=parse_signature("module M\ntype Person = { Name:string }\nval make: string -> int -> Person\nval current: Person\n");
        assert_eq!(p.function_returns.len(), 1);
        assert_eq!(p.function_returns[0].arity, 2);
        assert_eq!(p.function_returns[0].type_name, "Person");
        let current = declaration_token(&p, "current");
        assert!(p.binding_types.iter().any(|f| f.binding == current
            && matches!(&f.source,TypeSource::Named{name,..} if name=="Person")));
    }
    #[test]
    fn class_contract_facts_distinguish_virtual_and_explicit_interface_dispatch() {
        let p=parse("module M\ntype I =\n    abstract Name:string\ntype C() =\n    member _.Own = \"own\"\n    abstract Virtual:string\n    default _.Virtual = \"virtual\"\n    interface I with\n        member _.Name = \"interface\"\ntype D() =\n    inherit C()\n    override _.Virtual = \"override\"\n");
        let c = p.record_types.iter().find(|t| t.name == "M.C").unwrap();
        assert_eq!(
            c.fields
                .iter()
                .map(|f| (f.name.as_str(), f.kind.as_str()))
                .collect::<Vec<_>>(),
            [
                ("Own", "property"),
                ("Virtual", "abstract-property"),
                ("Virtual", "default-property")
            ]
        );
        assert!(c.blocked_members.iter().any(|name| name == "Name"));
        assert!(!c.fields.iter().any(|f| f.name == "Name"));
        let d = p.record_types.iter().find(|t| t.name == "M.D").unwrap();
        assert_eq!(d.fields[0].kind, "override-property");
        let i = p.record_types.iter().find(|t| t.name == "M.I").unwrap();
        assert_eq!(i.fields[0].kind, "abstract-property");
    }
    #[test]
    fn nested_alias_facts_retain_the_lexical_receiver_identity() {
        let p=parse("module M\ntype A = {Name:string}\ntype B = {Name:int}\nlet f (x:A) =\n    let outer = x\n    let inner = (fun (x:B) -> let alias = x in alias.Name)\n    outer.Name\n");
        let outer = declaration_token(&p, "outer");
        let alias = declaration_token(&p, "alias");
        let target = |binding| {
            p.binding_types
                .iter()
                .find_map(|f| {
                    if f.binding == binding {
                        match f.source {
                            TypeSource::Alias { token } => p.local_binding(token),
                            _ => None,
                        }
                    } else {
                        None
                    }
                })
                .unwrap()
        };
        assert_ne!(target(outer).token, target(alias).token);
        assert_eq!(p.tokens[target(outer).token].line, 3);
        assert_eq!(p.tokens[target(alias).token].line, 5);
    }
    #[test]
    fn direct_function_results_and_implicit_receivers_are_syntax_facts() {
        let p=parse("module M\ntype Customer = {Name:string}\nlet make name = {Customer.Name=name}\nlet forward (person:Customer) = person\ntype Box<'T>(value:'T) =\n    member this.Value = value\n    member this.Read() = this.Value\n");
        let make = declaration_token(&p, "make");
        assert!(p.function_results.iter().any(|f| f.binding == make
            && f.arity == 1
            && matches!(f.source, TypeSource::Record { .. })));
        assert!(p.record_expressions.iter().any(|e| e.binding == Some(make)));
        assert!(!p.binding_types.iter().any(|f| f.binding == make));
        let forward = declaration_token(&p, "forward");
        assert!(p
            .function_results
            .iter()
            .any(|f| f.binding == forward && matches!(f.source, TypeSource::Alias { .. })));
        let own = p.record_types.iter().find(|t| t.name == "M.Box").unwrap();
        assert_eq!(own.type_parameters, ["'T"]);
        let receivers: Vec<_> = p
            .binding_types
            .iter()
            .filter(|f| p.tokens[f.binding].text == "this")
            .collect();
        assert_eq!(receivers.len(), 2);
        assert!(receivers
            .iter()
            .all(|f| matches!(&f.source,TypeSource::Named{name,..} if name=="M.Box<'T>")));
    }
    #[test]
    fn accessibility_inherits_module_restrictions_and_signature_modifiers() {
        let p = parse("module Main\nlet private hidden = 1\nlet internal restricted = 2\nlet visible = 3\nmodule private Inner =\n    let public nested = 4\nmodule internal Shared =\n    let nestedInternal = 5\ntype private C(value:int) =\n    member _.M arg = value + arg\n");
        let access = |name: &str| {
            p.declarations
                .iter()
                .find(|d| d.name == name)
                .unwrap()
                .accessibility
                .as_str()
        };
        assert_eq!(access("hidden"), "private");
        assert_eq!(access("restricted"), "internal");
        assert_eq!(access("visible"), "public");
        assert_eq!(access("nested"), "private");
        assert_eq!(access("nestedInternal"), "internal");
        assert_eq!(access("arg"), "private");
        let p = parse_signature("module Main\nval private hidden: int\nval internal restricted: int\nval visible: int\n");
        for (name, expected) in [
            ("hidden", "private"),
            ("restricted", "internal"),
            ("visible", "public"),
        ] {
            assert_eq!(
                p.declarations
                    .iter()
                    .find(|d| d.name == name)
                    .unwrap()
                    .accessibility,
                expected
            );
        }
    }
    fn binding<'a>(p: &'a ParsedFile, line: u32, col: u32) -> &'a Declaration {
        p.local_binding(
            p.token_at(line, col)
                .unwrap_or_else(|| panic!("no token at {line}:{col}")),
        )
        .unwrap_or_else(|| panic!("no binding at {line}:{col}; decls={:?}", p.declarations))
    }
    #[test]
    fn namespace_types_do_not_poison_values() {
        let p=parse("namespace Demo\nmodule A =\n    type Person = { Name: string }\n    let add x y = x + y\n    let answer = add 1 2\n");
        assert!(p.supported, "{:?}", p.reasons);
        assert_eq!(binding(&p, 4, 17).name, "add");
        assert_eq!(p.declaration_module(binding(&p, 4, 17)), Some("Demo.A"));
    }
    #[test]
    fn lambda_pattern_shadowing_and_nonrecursive_rhs() {
        let p=parse("module M\nlet x = 1\nlet f x =\n    let y = x\n    let x = x + 1\n    let z = List.map (fun (x,y) -> x + y) [(1,2)]\n    y + x\n");
        assert!(p.supported, "{:?}", p.reasons);
        assert_eq!(p.tokens[binding(&p, 4, 12).token].line, 2);
        assert_eq!(p.tokens[binding(&p, 5, 35).token].line, 5);
        assert_eq!(p.tokens[binding(&p, 6, 8).token].line, 4);
    }
    #[test]
    fn argument_name_candidates_follow_call_slots_and_preserve_equality() {
        let cases = [
            ("C.M(x=x)", true),
            ("C.M (x=x)", true),
            ("C(x=x).M(x=x)", true),
            ("C.M(false,x=x)", true),
            ("C.M(false,x=x,y=x)", true),
            ("C.M(?y=x,x=x)", true),
            ("C.M(?x=x)", true),
            ("C.M(x=x+1)", true),
            ("C.M(x=id x)", true),
            ("C.M((*c*)x=x)", true),
            ("f(x=x)", false),
            ("f (x=x)", false),
            ("alias(x=x)", false),
            ("Module.f(x=x)", true),
            ("C.M((x=x))", false),
            ("C.M((x)=x)", false),
            ("index.[x=x]", true),
            ("index[x=x]", true),
            ("f [x=x]", false),
            ("$\"{C.M(x=x)}\"", true),
            ("(match x with | true -> C.M(x=x) | _ -> false)", true),
        ];
        for (body, ambiguous) in cases {
            let source = format!("module Example\nmodule Module =\n    let f b=b\nlet f b=b\nlet alias=f\nlet caller x = {body}\n");
            let p = parse(&source);
            let byte = source
                .rfind("x=x")
                .or_else(|| source.rfind("x)=x"))
                .or_else(|| source.rfind("x=id"))
                .unwrap();
            let left = p.tokens.iter().position(|t| t.start == byte).unwrap();
            assert_eq!(
                p.possible_named_argument(left),
                ambiguous,
                "{body}: {:?}",
                p.call_argument_name_candidates
            );
        }

        for (source, spelling) in [
            ("type B(x:int)=class end\ntype C(x:int)=inherit B(x=x)", "x"),
            ("let f ``x.y``=C.M(``x.y``=``x.y``)", "``x.y``"),
            ("let f ``_``=C.M(``_``=``_``)", "``_``"),
            ("module Example\nlet g b=b\nmodule B=\n    type g(x:int)=class end\nopen B\nlet f x=g(x=x)", "x"),
        ] {
            let p=parse(source);
            let byte=source.rfind(&format!("{spelling}=")).unwrap();
            let left=p.tokens.iter().position(|t|t.start==byte).unwrap();
            assert!(p.possible_named_argument(left),"{source}: {:?}",p.call_argument_name_candidates);
        }
    }

    #[test]
    fn match_guards_and_interpolations() {
        let p=parse("module M\nlet f x =\n    match Some x with\n    | Some y when y > 0 -> $\"{y} {x}\"\n    | _ -> string x\n");
        assert!(p.supported, "{:?}", p.reasons);
        assert_eq!(binding(&p, 3, 18).name, "y");
        assert_eq!(binding(&p, 3, 30).name, "y");
        assert_eq!(binding(&p, 3, 34).name, "x");
    }
    #[test]
    fn nested_module_scope_and_alias() {
        let p=parse("module M\nmodule A =\n    let x = 1\nmodule B =\n    let x = 2\n    let y = x\nlet z = A.x\n");
        assert!(p.supported, "{:?}", p.reasons);
        assert_eq!(p.declaration_module(binding(&p, 5, 12)), Some("M.B"));
        assert!(p.local_binding(p.token_at(6, 10).unwrap()).is_none());
    }
    #[test]
    fn constructor_and_method_parameters() {
        let p=parse("module M\ntype Thing(value: int) =\n    member this.Get(x: int) = value + x\nlet after x = x\n");
        assert!(p.supported, "{:?}", p.reasons);
        assert_eq!(binding(&p, 2, 30).name, "value");
        assert_eq!(binding(&p, 2, 39).name, "x");
        assert_eq!(binding(&p, 3, 14).name, "x");
    }
    #[test]
    fn utf16_and_string_comments() {
        let p = parse("module M\nlet ``snow man`` x = $\"😀 {x}\" // x\nlet a = ``snow man`` 1\n");
        assert!(p.supported, "{:?}", p.reasons);
        assert_eq!(binding(&p, 1, 27).name, "x");
        assert_eq!(binding(&p, 2, 10).name, "snow man");
    }
    #[test]
    fn recursive_group_parameters_and_explicit_in() {
        let p=parse("module M\nlet rec even n = if n = 0 then true else odd (n-1)\nand odd n = if n = 0 then false else even (n-1)\nlet outer x = (let x = x + 1 in x) + x\n");
        assert!(p.supported, "{:?}", p.reasons);
        assert_eq!(p.tokens[binding(&p, 1, 41).token].line, 2);
        assert_eq!(p.tokens[binding(&p, 2, 15).token].line, 2);
        assert_eq!(p.tokens[binding(&p, 3, 23).token].column, 10);
        assert_eq!(p.tokens[binding(&p, 3, 32).token].column, 19);
        assert_eq!(p.tokens[binding(&p, 3, 37).token].column, 10);
    }
    #[test]
    fn signature_values_export_and_resource_scope() {
        let p = parse_signature("namespace N\nmodule M =\n    val f: int -> int\n");
        assert!(p.supported, "{:?}", p.reasons);
        let d = p.declarations.iter().find(|d| d.name == "f").unwrap();
        assert!(d.top_level);
        assert_eq!(p.declaration_module(d), Some("N.M"));
        let p = parse("module M\nlet f x =\n    use stream = x\n    stream\n");
        assert!(p.supported, "{:?}", p.reasons);
        assert_eq!(binding(&p, 3, 4).name, "stream");
    }

    #[test]
    fn ambiguous_uppercase_pattern_does_not_poison_other_names() {
        let p = parse("module M\nlet f x = match x with | Unknown -> x + Unknown\n");
        assert!(!p.binding_supported(p.token_at(1, 40).unwrap()));
        assert_eq!(binding(&p, 1, 36).name, "x");
    }

    #[test]
    fn inline_function_cases_have_separate_scopes() {
        let p = parse("module Main\nlet x = 1\nlet f = function Some x -> x | None -> x\n");
        assert_eq!(p.tokens[binding(&p, 2, 27).token].line, 2);
        assert_eq!(p.tokens[binding(&p, 2, 39).token].line, 1);
        let outer = p
            .declarations
            .iter()
            .find(|d| d.name == "x" && d.top_level)
            .unwrap();
        assert!(p.references_supported(outer));
    }

    #[test]
    fn punctuation_is_compacted_without_joining_qualifiers() {
        let p = parse("module M\nlet f x = A.make().value + x\nlet values = [1;2;3;4;5;6;7;8;9]\n");
        let member = p.token_at(1, 19).unwrap();
        assert!(p.qualifier(member).is_none());
        assert!(!p.binding_supported(member));
        assert!(
            p.tokens
                .iter()
                .filter(|t| !t.ident && t.text != ".")
                .count()
                < 15
        );
    }

    #[test]
    fn attribute_spellings_are_retained_without_assigning_core_semantics() {
        let source = "module M\nlet x=1\n[<AutoOpen>]\nmodule Inner=\n    let x=2\nlet result=x\n";
        let p = parse(source);
        assert!(p.supported, "{:?}", p.reasons);
        assert!(p.auto_opens.is_empty());
        assert!(!p.opens.iter().any(|o| o.name == "M.Inner"));
        assert_eq!(p.attributes.len(), 1);
        let a = &p.attributes[0];
        assert_eq!(
            (
                a.owner.as_str(),
                a.owner_kind.as_str(),
                a.type_name.as_str()
            ),
            ("M.Inner", "module", "AutoOpen")
        );
        assert_eq!(a.scope_end, p.modules[0].end);
        assert_eq!(p.tokens[a.token].text, "AutoOpen");
        let qualified=parse("namespace N\n[<Microsoft.FSharp.Core.AutoOpenAttribute>]\nmodule Helpers=\n    let x=1\n");
        assert!(qualified.auto_opens.is_empty());
        assert_eq!(
            qualified.attributes[0].type_name,
            "Microsoft.FSharp.Core.AutoOpenAttribute"
        );
        assert_eq!(qualified.attributes[0].owner, "N.Helpers");
    }
}
