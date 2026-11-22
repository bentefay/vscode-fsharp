//! A physical source file may be compiled by several different projects.
//! Conditional projection is byte-preserving and precedes any syntax analysis.
use crate::{
    cancellation::CancellationToken,
    project::{self, Model, Project},
};
use anyhow::{bail, ensure, Result};
use std::{borrow::Cow, collections::HashSet, ops::Deref, sync::Arc};

#[derive(Clone, Debug)]
pub struct SourceContext(Arc<SourceContextData>);
#[derive(Debug)]
pub struct SourceContextData {
    pub id: String,
    pub project_path: String,
    pub defines: Vec<String>,
    pub target_framework: String,
    pub configuration: String,
    pub compiler_args: Vec<String>,
    pub defines_known: bool,
    pub supports_elif: bool,
}
impl Deref for SourceContext {
    type Target = SourceContextData;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl SourceContext {
    pub fn new(
        project_path: &str,
        mut defines: Vec<String>,
        target_framework: &str,
        configuration: &str,
        compiler_args: &[String],
        defines_known: bool,
    ) -> Self {
        for arg in compiler_args {
            if let Some(value) = arg
                .strip_prefix("--define:")
                .or_else(|| arg.strip_prefix("-d:"))
            {
                defines.extend(
                    value
                        .split([';', ','])
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                );
            }
        }
        defines.sort();
        defines.dedup();
        let lang = compiler_args
            .iter()
            .rev()
            .find_map(|arg| arg.strip_prefix("--langversion:"));
        let supports_elif =
            lang.is_some_and(|s| s == "preview" || s.parse::<f64>().is_ok_and(|n| n >= 11.0));
        let project_path = project::normalize(project_path);
        let id = blake3::hash(
            serde_json::to_string(&(
                &project_path,
                &defines,
                target_framework,
                configuration,
                compiler_args,
                defines_known,
                supports_elif,
            ))
            .unwrap()
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        Self(Arc::new(SourceContextData {
            id,
            project_path,
            defines,
            target_framework: target_framework.into(),
            configuration: configuration.into(),
            compiler_args: compiler_args.to_vec(),
            defines_known,
            supports_elif,
        }))
    }
    pub fn cache_key(&self) -> String {
        self.id.clone()
    }
}
impl Project {
    pub fn source_context(&self) -> SourceContext {
        let mut defines = self.defines.clone();
        // Match the foreground F# editor environment, not command-line build.
        defines.extend(["COMPILED".into(), "EDITING".into()]);
        SourceContext::new(
            &self.path,
            defines,
            &self.target_framework,
            &self.configuration,
            &self.compiler_args,
            self.exact
                && self.evaluation == "design-time-fsc-arguments"
                && !self.compiler_args.is_empty(),
        )
    }
}
impl Model {
    pub fn source_instance_project(
        &self,
        path: &str,
        preferred_project: Option<&str>,
    ) -> Option<&Project> {
        let unique_project = |name: &str| {
            let mut matches = self.projects.iter().filter(|p| p.path == name);
            let p = matches.next()?;
            if matches.next().is_some() {
                None
            } else {
                Some(p)
            }
        };
        if let Some(preferred) = preferred_project {
            let preferred = project::from_uri(preferred);
            let caller = unique_project(&preferred)?;
            if caller.files.iter().any(|f| f == path) {
                return Some(caller);
            }
            let mut reachable = HashSet::new();
            let mut queue = caller.references.clone();
            let mut result = None;
            while let Some(name) = queue.pop() {
                if !reachable.insert(name.clone()) {
                    continue;
                }
                let p = unique_project(&name)?;
                queue.extend(p.references.clone());
                if p.files.iter().any(|f| f == path) {
                    if result.is_some() {
                        return None;
                    }
                    result = Some(p);
                }
            }
            result
        } else {
            let mut owners = self
                .projects
                .iter()
                .filter(|p| p.files.iter().any(|f| f == path));
            let p = owners.next()?;
            if owners.next().is_some() {
                None
            } else {
                unique_project(&p.path)
            }
        }
    }
    pub fn source_instance(
        &self,
        path: &str,
        preferred_project: Option<&str>,
    ) -> Option<SourceContext> {
        self.source_instance_project(path, preferred_project)
            .map(Project::source_context)
    }
    pub fn source_instances(&self) -> impl Iterator<Item = (&str, &Project)> {
        self.projects
            .iter()
            .flat_map(|p| p.files.iter().map(move |f| (f.as_str(), p)))
    }
}

struct Expression<'a> {
    text: &'a str,
    at: usize,
    defines: &'a [String],
    depth: usize,
}
impl<'a> Expression<'a> {
    fn whitespace(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(|c| *c == b' ' || *c == b'\t')
        {
            self.at += 1;
        }
    }
    fn eat(&mut self, s: &str) -> bool {
        self.whitespace();
        if self.text[self.at..].starts_with(s) {
            self.at += s.len();
            true
        } else {
            false
        }
    }
    fn or(&mut self) -> Result<bool> {
        let mut value = self.and()?;
        while self.eat("||") {
            let right = self.and()?;
            value |= right;
        }
        Ok(value)
    }
    fn and(&mut self) -> Result<bool> {
        let mut value = self.unary()?;
        while self.eat("&&") {
            let right = self.unary()?;
            value &= right;
        }
        Ok(value)
    }
    fn unary(&mut self) -> Result<bool> {
        self.depth += 1;
        ensure!(
            self.depth <= 128,
            "Conditional expression exceeds nesting limit"
        );
        let value = if self.eat("!") {
            !self.unary()?
        } else if self.eat("(") {
            let v = self.or()?;
            ensure!(self.eat(")"), "Unclosed conditional expression");
            v
        } else {
            self.whitespace();
            let start = self.at;
            for c in self.text[self.at..].chars() {
                if (self.at == start && (unicode_ident::is_xid_start(c) || c == '_'))
                    || (self.at > start && (unicode_ident::is_xid_continue(c) || c == '\''))
                {
                    self.at += c.len_utf8();
                } else {
                    break;
                }
            }
            ensure!(self.at > start, "Expected conditional symbol");
            self.defines.iter().any(|d| d == &self.text[start..self.at])
        };
        self.depth -= 1;
        Ok(value)
    }
}
fn condition(text: &str, ctx: &SourceContext) -> Result<bool> {
    ensure!(
        ctx.defines_known,
        "Conditional source requires certified effective compiler defines"
    );
    ensure!(
        text.len() <= 65536,
        "Conditional expression exceeds byte budget"
    );
    let mut expression = Expression {
        text,
        at: 0,
        defines: &ctx.defines,
        depth: 0,
    };
    let value = expression.or()?;
    expression.whitespace();
    ensure!(
        expression.at == text.len() || text[expression.at..].starts_with("//"),
        "Unsupported trailing conditional syntax"
    );
    Ok(value)
}
#[derive(Clone, Copy, Debug)]
enum Lex {
    Code {
        hole: usize,
    },
    Comment,
    QuotedIdentifier,
    String {
        verbatim: bool,
        triple: bool,
        interpolated: bool,
    },
}
struct Lexer {
    stack: Vec<Lex>,
}
impl Lexer {
    fn new() -> Self {
        Self {
            stack: vec![Lex::Code { hole: 0 }],
        }
    }
    fn code(&self) -> bool {
        matches!(self.stack.as_slice(), [Lex::Code { hole: 0 }])
    }
    fn line(&mut self, line: &str, token: &CancellationToken) -> Result<()> {
        let bytes = line.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if at % 4096 == 0 {
                token.check()?;
            }
            ensure!(
                self.stack.len() <= 1024,
                "Conditional lexical nesting budget exceeded"
            );
            let rest = &bytes[at..];
            match *self.stack.last().unwrap() {
                Lex::QuotedIdentifier => {
                    if rest.starts_with(b"``") {
                        self.stack.pop();
                        at += 2;
                    } else {
                        at += 1;
                    }
                }
                Lex::String {
                    verbatim,
                    triple,
                    interpolated,
                } => {
                    if triple && rest.starts_with(b"\"\"\"") {
                        self.stack.pop();
                        at += 3;
                    } else if !triple && rest[0] == b'"' {
                        if verbatim && rest.starts_with(b"\"\"") {
                            at += 2;
                        } else {
                            self.stack.pop();
                            at += 1;
                        }
                    } else if !verbatim && !triple && rest[0] == b'\\' {
                        at = (at + 2).min(bytes.len());
                    } else if interpolated && rest[0] == b'{' {
                        if rest.starts_with(b"{{") {
                            at += 2;
                        } else {
                            self.stack.push(Lex::Code { hole: 1 });
                            at += 1;
                        }
                    } else {
                        at += 1;
                    }
                }
                state => {
                    let comment = matches!(state, Lex::Comment);
                    if rest.starts_with(b"(*") {
                        self.stack.push(Lex::Comment);
                        at += 2;
                        continue;
                    }
                    if comment && rest.starts_with(b"*)") {
                        self.stack.pop();
                        at += 2;
                        continue;
                    }
                    if !comment && rest.starts_with(b"//") {
                        break;
                    }
                    if !comment && rest.starts_with(b"``") {
                        self.stack.push(Lex::QuotedIdentifier);
                        at += 2;
                        continue;
                    }
                    // Strings within comments protect quoted comment delimiters.
                    let mut n = 0;
                    while rest.get(n) == Some(&b'$') {
                        n += 1;
                    }
                    // Extended raw interpolation uses a different brace grammar;
                    // refuse projection rather than misclassifying directives.
                    if n > 1 && (rest.get(n) == Some(&b'"') || rest.get(n) == Some(&b'@')) {
                        bail!("Extended interpolated strings require conditional lexer support");
                    }
                    let mut verbatim = rest.get(n) == Some(&b'@');
                    if verbatim {
                        n += 1;
                    }
                    let mut interpolated = rest.first() == Some(&b'$');
                    if n == 1 && verbatim && rest.get(n) == Some(&b'$') {
                        interpolated = true;
                        n += 1;
                    }
                    if rest.get(n) == Some(&b'"') {
                        let triple = rest[n..].starts_with(b"\"\"\"");
                        if triple {
                            verbatim = true;
                        }
                        self.stack.push(Lex::String {
                            verbatim,
                            triple,
                            interpolated: interpolated && !comment,
                        });
                        at += n + if triple { 3 } else { 1 };
                        continue;
                    }
                    // Skip a character literal, but not a generic type variable.
                    if rest[0] == b'\'' && rest.len() >= 3 {
                        let end = if rest[1] == b'\\' {
                            rest[2..].iter().position(|c| *c == b'\'').map(|i| i + 2)
                        } else {
                            std::str::from_utf8(&rest[1..])
                                .ok()
                                .and_then(|s| s.chars().next())
                                .map(|c| 1 + c.len_utf8())
                                .filter(|i| rest.get(*i) == Some(&b'\''))
                        };
                        if let Some(end) = end.filter(|end| *end <= 12) {
                            at += end + 1;
                            continue;
                        }
                    }
                    if let Lex::Code { hole } = state {
                        if hole > 0 && rest[0] == b'}' {
                            if hole == 1 {
                                self.stack.pop();
                            } else {
                                *self.stack.last_mut().unwrap() = Lex::Code { hole: hole - 1 };
                            }
                            at += 1;
                            continue;
                        }
                        if hole > 0 && rest[0] == b'{' {
                            *self.stack.last_mut().unwrap() = Lex::Code { hole: hole + 1 };
                            at += 1;
                            continue;
                        }
                    }
                    at += n.max(1);
                }
            }
        }
        Ok(())
    }
}
/// Candidate-only syntax may inspect the unconditional prefix without guessing
/// effective compiler defines. This does not establish binding or visibility.
pub(crate) fn unconditional_prefix<'a>(
    source: &'a str,
    token: &CancellationToken,
) -> Result<&'a str> {
    let mut lexer = Lexer::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        token.check()?;
        let content = line.trim_end_matches(['\r', '\n']);
        let content = if offset == 0 {
            content.strip_prefix('\u{feff}').unwrap_or(content)
        } else {
            content
        };
        if lexer.code() && content.trim_start_matches([' ', '\t']).starts_with('#') {
            return Ok(&source[..offset]);
        }
        lexer.line(line, token)?;
        offset += line.len();
    }
    ensure!(
        lexer.code(),
        "Unterminated lexical construct in candidate source"
    );
    Ok(source)
}

/// Candidate-only whole physical source validation, without selecting branches.
/// Reject uncertain literal boundaries before interpreting occurrence tokens.
pub(crate) fn validate_candidate_lexical_source(
    source: &str,
    token: &CancellationToken,
) -> Result<()> {
    let mut lexer = Lexer::new();
    for line in source.split_inclusive('\n') {
        token.check()?;
        lexer.line(line, token)?;
    }
    ensure!(
        lexer.code(),
        "Unterminated lexical construct in occurrence source"
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct Branch {
    parent: bool,
    matched: bool,
    active: bool,
    seen_else: bool,
}
/// Mask bytes, never concatenate active lines: token offsets and UTF16 positions
/// in retained code continue to address the original editor document.
pub fn project<'a>(
    source: &'a str,
    ctx: &SourceContext,
    token: &CancellationToken,
) -> Result<Cow<'a, str>> {
    token.check()?;
    if !source.contains('#') {
        return Ok(Cow::Borrowed(source));
    }
    let mut result = None::<Vec<u8>>;
    let mut branches = Vec::<Branch>::new();
    let mut lexer = Lexer::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        token.check()?;
        let active = branches.last().is_none_or(|b| b.active);
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = if offset == 0 {
            content.strip_prefix('\u{feff}').unwrap_or(content)
        } else {
            content
        }
        .trim_start_matches([' ', '\t']);
        let mut directive = false;
        if (!active || lexer.code()) && trimmed.starts_with('#') {
            let body = &trimmed[1..];
            let end = body
                .find(|c: char| c == ' ' || c == '\t')
                .unwrap_or(body.len());
            let keyword = &body[..end];
            let tail = body[end..].trim_start_matches([' ', '\t']);
            match keyword {
                "if" => {
                    ensure!(end < body.len(), "#if requires a condition");
                    let value = condition(tail, ctx)?;
                    ensure!(
                        branches.len() < 1024,
                        "Conditional branch nesting budget exceeded"
                    );
                    branches.push(Branch {
                        parent: active,
                        matched: value,
                        active: active && value,
                        seen_else: false,
                    });
                    directive = true;
                }
                "elif" => {
                    ensure!(
                        ctx.supports_elif,
                        "#elif requires certified F# 11 or preview language version"
                    );
                    let value = condition(tail, ctx)?;
                    let branch = branches
                        .last_mut()
                        .ok_or_else(|| anyhow::anyhow!("Unmatched #elif"))?;
                    ensure!(!branch.seen_else, "#elif after #else");
                    branch.active = branch.parent && !branch.matched && value;
                    branch.matched |= value;
                    directive = true;
                }
                "else" => {
                    ensure!(
                        tail.is_empty() || tail.starts_with("//"),
                        "Unexpected #else suffix"
                    );
                    let branch = branches
                        .last_mut()
                        .ok_or_else(|| anyhow::anyhow!("Unmatched #else"))?;
                    ensure!(!branch.seen_else, "Duplicate #else");
                    branch.active = branch.parent && !branch.matched;
                    branch.seen_else = true;
                    branch.matched = true;
                    directive = true;
                }
                "endif" => {
                    ensure!(
                        tail.is_empty() || tail.starts_with("//"),
                        "Unexpected #endif suffix"
                    );
                    ensure!(branches.pop().is_some(), "Unmatched #endif");
                    directive = true;
                }
                _ => {}
            }
        }
        if directive || !active {
            let output = result.get_or_insert_with(|| source.as_bytes().to_vec());
            for byte in &mut output[offset..offset + line.len()] {
                if *byte != b'\r' && *byte != b'\n' {
                    *byte = b' ';
                }
            }
        } else {
            lexer.line(line, token)?;
        }
        offset += line.len();
    }
    ensure!(branches.is_empty(), "Unclosed #if");
    Ok(match result {
        Some(bytes) => Cow::Owned(String::from_utf8(bytes)?),
        None => Cow::Borrowed(source),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(symbols: &[&str]) -> SourceContext {
        SourceContext::new(
            "/p.fsproj",
            symbols.iter().map(|s| s.to_string()).collect(),
            "net10.0",
            "Debug",
            &["--langversion:preview".into()],
            true,
        )
    }
    #[test]
    fn candidate_prefix_ignores_quoted_directives_and_stops_before_actual_directive() {
        let source = "module M\r\n(* #if COMMENT *)\r\nlet text = \"\"\"\n#if STRING\n\"\"\"\nlet before = 1\n#if REAL\nlet after = 2\n#endif\n";
        let prefix = unconditional_prefix(source, &CancellationToken::new()).unwrap();
        assert!(prefix.contains("let before = 1"));
        assert!(!prefix.contains("let after = 2"));
        assert_eq!(prefix.len(), source.find("#if REAL").unwrap());
        assert!(
            unconditional_prefix("let text = \"unterminated", &CancellationToken::new()).is_err()
        );
        assert!(unconditional_prefix(
            "let text = $$\"\"\"extended\"\"\"",
            &CancellationToken::new()
        )
        .is_err());
    }

    #[test]
    fn projection_keeps_offsets_and_ignores_inactive_lexical_garbage() {
        let source="module M\r\n#if A\r\nlet α = \"🦀\"\r\n#else\r\n\"unterminated (*\r\n#if B\r\nbroken\r\n#endif\r\n#endif\r\nlet after = α\r\n";
        let projected = project(source, &context(&["A"]), &CancellationToken::new()).unwrap();
        assert_eq!(projected.len(), source.len());
        assert!(projected.contains("let α = \"🦀\""));
        assert!(!projected.contains("unterminated"));
        assert_eq!(projected.find("let after"), source.find("let after"));
        assert_eq!(
            projected.matches('\r').count(),
            source.matches('\r').count()
        );
    }
    #[test]
    fn active_comments_strings_and_interpolation_do_not_create_directives() {
        let source="(* nested (* x *)\n#if FAKE\n\"*)\"\n*)\nlet s = @\"literal\n#if NOPE\n\"\nlet t = \"\"\"x\n#endif\n\"\"\"\nlet u = $\"{ \"x\" }\"\n#if A\nlet x = 1\n#else\nlet x = 2\n#endif\n";
        let projected = project(source, &context(&["A"]), &CancellationToken::new()).unwrap();
        assert!(projected.contains("#if FAKE"));
        assert!(projected.contains("#if NOPE"));
        assert!(projected.contains("let x = 1"));
        assert!(!projected.contains("let x = 2"));
    }
    #[test]
    fn condition_grammar_provenance_and_language_are_certified() {
        let ctx = context(&["A"]);
        assert!(condition("!B && A || C // comment", &ctx).unwrap());
        assert!(!condition("true", &ctx).unwrap());
        assert!(condition("true", &context(&["true"])).unwrap());
        assert!(condition("A (* comment *)", &ctx).is_err());
        let unknown = SourceContext::new("/p", vec![], "", "", &[], false);
        assert!(project(
            "#if DEBUG\na\n#endif\n",
            &unknown,
            &CancellationToken::new()
        )
        .is_err());
        assert!(project("let a=1", &unknown, &CancellationToken::new()).is_ok());
        let default =
            SourceContext::new("/p", vec![], "", "", &["--langversion:10.0".into()], true);
        assert!(project(
            "#if A\na\n#elif B\nb\n#endif",
            &default,
            &CancellationToken::new()
        )
        .is_err());
    }
    #[test]
    fn malformed_structure_never_certifies() {
        for source in [
            "#else",
            "#endif",
            "#if A",
            "#if A\n#else\n#else\n#endif",
            "#if A\n#else\n#elif B\n#endif",
            "#if A &&\nx\n#endif",
        ] {
            assert!(
                project(source, &context(&[]), &CancellationToken::new()).is_err(),
                "{source}"
            );
        }
    }
    #[test]
    fn source_instance_selection_never_picks_first_owner_or_variant() {
        let p = |path: &str, files: Vec<String>| Project {
            path: path.into(),
            name: path.into(),
            files,
            references: vec![],
            items: vec![],
            assembly_references: vec![],
            workspace_assembly_references: vec![],
            defines: vec![],
            target_framework: String::new(),
            configuration: String::new(),
            compiler_args: vec![],
            evaluation: String::new(),
            source_membership: vec![],
            membership_reusable: None,
            environment_certificate: None,
            membership_diagnostics: vec![],
            reference_bindings: vec![],
            exact: true,
        };
        let mut model = Model {
            validation_metrics: Default::default(),
            root: "/".into(),
            projects: vec![
                p("/A", vec!["/shared.fs".into()]),
                p("/B", vec!["/shared.fs".into(), "/B.fs".into()]),
            ],
            files: vec![],
            exact: true,
            diagnostics: vec![],
            inputs: vec![],
        };
        assert!(model.source_instance_project("/shared.fs", None).is_none());
        assert_eq!(
            model
                .source_instance_project("/shared.fs", Some("/B"))
                .unwrap()
                .path,
            "/B"
        );
        assert!(model
            .source_instance_project("/shared.fs", Some("/unknown"))
            .is_none());
        assert_eq!(
            model.source_instance_project("/B.fs", None).unwrap().path,
            "/B"
        );
        model.projects.push(model.projects[1].clone());
        assert!(model.source_instance_project("/B.fs", Some("/B")).is_none());
        assert!(model.source_instance_project("/B.fs", None).is_none());
    }
    #[test]
    fn context_identity_covers_effective_environment() {
        let a = context(&["A"]);
        let b = context(&["B"]);
        assert_ne!(a.id, b.id);
        assert_eq!(a.clone().cache_key(), a.id);
        let unknown = SourceContext::new(
            "/p.fsproj",
            a.defines.clone(),
            "net10.0",
            "Debug",
            &a.compiler_args,
            false,
        );
        assert_ne!(a.id, unknown.id);
    }
}
