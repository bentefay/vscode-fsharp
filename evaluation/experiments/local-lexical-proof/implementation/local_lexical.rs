//! A source-local parameter proof. No workspace, project, index or library reads.
//! Unsupported roles return None; this is not the general lexical fallback.
use crate::cancellation::CancellationToken;
use serde::Serialize;
use std::{
    ops::ControlFlow,
    time::{Duration, Instant},
};
use tree_sitter::{Node, ParseOptions, Parser};

pub const PROOF_VERSION: &str = "document-local-parameter-v1";
pub const PARSER_FINGERPRINT: &str = "tree-sitter-fsharp-0.3.13:00943a441bd59446ed9d321f3e0e09402ec3069c623890fc0b1893717d001302:abi15";
const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_NODES: usize = 50_000;
const MAX_DEPTH: usize = 64;
const MAX_REFERENCES: usize = 4096;
const PARSE_TIME: Duration = Duration::from_millis(100);
const TOTAL_TIME: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug)]
pub struct Snapshot<'a> {
    pub uri: &'a str,
    pub source: &'a str,
    pub version: i64,
    /// Lowercase BLAKE3 of exactly the acknowledged UTF-8 source.
    pub source_hash: &'a str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Definition,
    References { include_declaration: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}
#[derive(Clone, Debug, Serialize)]
pub struct LocalAnswer {
    pub locations: Vec<Location>,
    pub declaration: Range,
    pub body_start: usize,
    pub body_end: usize,
    pub document_version: i64,
    pub source_hash: String,
    pub proof_version: &'static str,
    pub parser_fingerprint: &'static str,
    pub source_bytes: usize,
    pub visited_nodes: usize,
}

fn children(n: Node<'_>) -> Vec<Node<'_>> {
    let mut c = n.walk();
    n.named_children(&mut c)
        .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment" | "xml_doc"))
        .collect()
}
fn raw<'a>(n: Node<'_>, s: &'a str) -> &'a str {
    &s[n.byte_range()]
}
fn ordinary_name(s: &str) -> bool {
    !s.starts_with("op_")
        && s.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'\'')
}
fn token(n: Node<'_>, kind: &str) -> bool {
    let mut c = n.walk();
    let found = n.children(&mut c).any(|n| n.kind() == kind);
    found
}
fn simple_type(n: Node<'_>) -> bool {
    if n.kind() != "simple_type" || n.has_error() {
        return false;
    }
    let cs = children(n);
    cs.len() == 1
        && cs[0].kind() == "long_identifier"
        && !children(cs[0]).is_empty()
        && children(cs[0])
            .iter()
            .all(|n| n.kind() == "identifier" && !n.has_error())
}
fn pattern<'a>(mut n: Node<'a>, s: &str) -> Option<Node<'a>> {
    for _ in 0..8 {
        if n.has_error() || n.is_missing() {
            return None;
        }
        let cs = children(n);
        match n.kind() {
            "identifier" => return ordinary_name(raw(n, s)).then_some(n),
            "identifier_pattern"
            | "long_identifier_or_op"
            | "long_identifier"
            | "paren_pattern"
                if cs.len() == 1 =>
            {
                n = cs[0]
            }
            "typed_pattern" if cs.len() == 2 && simple_type(cs[1]) => n = cs[0],
            _ => return None,
        }
    }
    None
}
fn parameters<'a>(n: Node<'a>, s: &str) -> Option<Vec<Node<'a>>> {
    if n.kind() != "argument_patterns" {
        return None;
    }
    let args = children(n);
    if args.len() > 32 {
        return None;
    }
    let mut result = Vec::new();
    for arg in args {
        let id = pattern(arg, s)?;
        if result
            .iter()
            .any(|previous| raw(*previous, s) == raw(id, s))
        {
            return None;
        }
        result.push(id);
    }
    (!result.is_empty()).then_some(result)
}
#[derive(Clone)]
struct Binding<'a> {
    name: &'a str,
    id: usize,
}
#[derive(Clone)]
struct Candidate<'a> {
    node: Node<'a>,
    body: Node<'a>,
    parameters: Vec<Node<'a>>,
    local_refs: bool,
}
fn function<'a>(n: Node<'a>, s: &str) -> Option<(Node<'a>, Node<'a>, Vec<Node<'a>>)> {
    if n.kind() != "function_or_value_defn"
        || n.has_error()
        || token(n, "rec")
        || token(n, "let!")
        || token(n, "and")
        || !token(n, "let")
    {
        return None;
    }
    let cs = children(n);
    // Result attributes/annotations, recursive groups and unknown headers are
    // outside the initial envelope, even though some could be lexical too.
    if cs.len() != 2 || cs[0].kind() != "function_declaration_left" {
        return None;
    }
    let left = children(cs[0]);
    if left.len() != 2
        || left[0].kind() != "identifier"
        || !ordinary_name(raw(left[0], s))
        || token(cs[0], "inline")
    {
        return None;
    }
    let params = parameters(left[1], s)?;
    let body = n.child_by_field_name("body")?;
    (body.id() == cs[1].id()).then_some((left[0], body, params))
}
fn lambda<'a>(n: Node<'a>, s: &str) -> Option<(Node<'a>, Vec<Node<'a>>)> {
    let cs = children(n);
    if n.kind() != "fun_expression" || n.has_error() || cs.len() != 2 {
        return None;
    }
    Some((cs[1], parameters(cs[0], s)?))
}
fn local_function(n: Node<'_>) -> bool {
    let mut child = n;
    while let Some(parent) = child.parent() {
        if matches!(
            parent.kind(),
            "named_module" | "module_defn" | "namespace" | "file" | "type_extension_elements"
        ) {
            return false;
        }
        if parent.kind() == "fun_expression" {
            return children(parent).last().is_some_and(|body| {
                body.start_byte() <= n.start_byte() && n.end_byte() <= body.end_byte()
            });
        }
        if parent.kind() == "function_or_value_defn" {
            return parent.child_by_field_name("body").is_some_and(|body| {
                body.start_byte() <= n.start_byte() && n.end_byte() <= body.end_byte()
            });
        }
        child = parent;
    }
    false
}
fn argument_lambda(mut n: Node<'_>) -> bool {
    while let Some(parent) = n.parent() {
        if parent.kind() == "paren_expression" && children(parent).len() == 1 {
            n = parent;
            continue;
        }
        if parent.kind() != "application_expression" {
            return false;
        }
        let cs = children(parent);
        return cs.len() == 2 && cs[1].id() == n.id();
    }
    false
}
fn forbidden_context(mut n: Node<'_>) -> bool {
    let mut depth = 0;
    while let Some(p) = n.parent() {
        depth += 1;
        if depth > MAX_DEPTH {
            return true;
        }
        if matches!(
            p.kind(),
            "ce_expression"
                | "literal_expression"
                | "attribute"
                | "attributes"
                | "preproc_if"
                | "const"
                | "format_string"
                | "format_string_eval"
                | "format_triple_quoted_string"
        ) {
            return true;
        }
        n = p;
    }
    false
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Metrics {
    pub parse_started: bool,
    pub parsed_bytes: usize,
    pub visited_nodes: usize,
    pub parse_elapsed_micros: u64,
    pub elapsed_micros: u64,
}
struct Work<'a, 'm> {
    source: &'a str,
    token: &'a CancellationToken,
    started: Instant,
    metrics: &'m mut Metrics,
}
impl Work<'_, '_> {
    fn step(&mut self, depth: usize) -> bool {
        self.metrics.visited_nodes += 1;
        depth <= MAX_DEPTH
            && self.metrics.visited_nodes <= MAX_NODES
            && !self.token.is_cancelled()
            && self.started.elapsed() <= TOTAL_TIME
    }
    fn expr(
        &mut self,
        n: Node<'_>,
        env: &[Binding<'_>],
        wanted: usize,
        uses: &mut Vec<(usize, usize)>,
        depth: usize,
        named_argument: bool,
    ) -> bool {
        if !self.step(depth) || n.has_error() || n.is_missing() {
            return false;
        }
        let cs = children(n);
        match n.kind() {
            "long_identifier_or_op" | "long_identifier" => {
                if cs.len() == 1 && cs[0].kind() == "long_identifier" {
                    return self.expr(cs[0], env, wanted, uses, depth + 1, named_argument);
                }
                if cs.is_empty() || cs.iter().any(|id| id.kind() != "identifier") {
                    return false;
                }
                // Escaped names can alias ordinary names. Refuse rather than
                // silently omit a reference until their full range contract lands.
                if cs.iter().any(|id| raw(*id, self.source).starts_with("``")) {
                    return false;
                }
                let first = cs[0];
                if env
                    .iter()
                    .rev()
                    .find(|b| b.name == raw(first, self.source))
                    .is_some_and(|b| b.id == wanted)
                {
                    if uses.len() == MAX_REFERENCES {
                        return false;
                    }
                    uses.push((first.start_byte(), first.end_byte()));
                }
                true
            }
            "dot_expression" => {
                let (Some(base), Some(field)) = (
                    n.child_by_field_name("base"),
                    n.child_by_field_name("field"),
                ) else {
                    return false;
                };
                if cs.len() != 2
                    || !matches!(field.kind(), "long_identifier_or_op" | "long_identifier")
                    || children(field)
                        .iter()
                        .any(|n| n.kind() != "identifier" || raw(*n, self.source).starts_with("``"))
                {
                    return false;
                }
                self.expr(base, env, wanted, uses, depth + 1, false)
            }
            "const" => cs.len() == 1 && children(cs[0]).is_empty(),
            "unit" => cs.is_empty(),
            "paren_expression" => {
                cs.len() == 1 && self.expr(cs[0], env, wanted, uses, depth + 1, named_argument)
            }
            "tuple_expression" | "list_expression" | "array_expression" => cs
                .into_iter()
                .all(|child| self.expr(child, env, wanted, uses, depth + 1, named_argument)),
            "application_expression" => {
                cs.len() == 2
                    && self.expr(cs[0], env, wanted, uses, depth + 1, false)
                    && self.expr(cs[1], env, wanted, uses, depth + 1, true)
            }
            "infix_expression" => {
                if cs.len() != 3 || cs[1].kind() != "infix_op" {
                    return false;
                }
                let op = raw(cs[1], self.source);
                if op == "?" || op == "?<-" || (named_argument && op == "=") {
                    return false;
                }
                self.expr(cs[0], env, wanted, uses, depth + 1, false)
                    && self.expr(cs[2], env, wanted, uses, depth + 1, false)
            }
            "fun_expression" => {
                let Some((body, params)) = lambda(n, self.source) else {
                    return false;
                };
                let mut nested = env.to_vec();
                nested.extend(params.iter().map(|id| Binding {
                    name: raw(*id, self.source),
                    id: id.start_byte(),
                }));
                self.expr(body, &nested, wanted, uses, depth + 1, false)
            }
            "declaration_expression" => {
                let Some(next) = n.child_by_field_name("in") else {
                    return false;
                };
                if cs.len() != 2
                    || cs[1].id() != next.id()
                    || cs[0].kind() != "function_or_value_defn"
                {
                    return false;
                }
                let def = cs[0];
                if let Some((name, body, params)) = function(def, self.source) {
                    let mut function_env = env.to_vec();
                    function_env.extend(params.iter().map(|id| Binding {
                        name: raw(*id, self.source),
                        id: id.start_byte(),
                    }));
                    if !self.expr(body, &function_env, wanted, uses, depth + 1, false) {
                        return false;
                    }
                    let mut next_env = env.to_vec();
                    next_env.push(Binding {
                        name: raw(name, self.source),
                        id: name.start_byte(),
                    });
                    self.expr(next, &next_env, wanted, uses, depth + 1, false)
                } else {
                    if token(def, "rec") || token(def, "let!") || !token(def, "let") {
                        return false;
                    }
                    let ds = children(def);
                    if ds.len() != 2 || ds[0].kind() != "value_declaration_left" {
                        return false;
                    }
                    let ls = children(ds[0]);
                    if ls.len() != 1 || token(ds[0], "mutable") || token(ds[0], "inline") {
                        return false;
                    }
                    let Some(name) = pattern(ls[0], self.source) else {
                        return false;
                    };
                    let Some(body) = def.child_by_field_name("body") else {
                        return false;
                    };
                    if ds[1].id() != body.id()
                        || !self.expr(body, env, wanted, uses, depth + 1, false)
                    {
                        return false;
                    }
                    let mut next_env = env.to_vec();
                    next_env.push(Binding {
                        name: raw(name, self.source),
                        id: name.start_byte(),
                    });
                    self.expr(next, &next_env, wanted, uses, depth + 1, false)
                }
            }
            // No default recursive descent: unknown syntax can contain binders
            // or non-value identifier roles and must not become a complete set.
            _ => false,
        }
    }
}
fn byte_position(source: &str, p: Position) -> Option<usize> {
    let start = if p.line == 0 {
        0
    } else {
        source.match_indices('\n').nth(p.line as usize - 1)?.0 + 1
    };
    let line = source[start..].split('\n').next()?;
    let mut utf16 = 0;
    for (byte, ch) in line.char_indices() {
        if utf16 == p.character {
            return Some(start + byte);
        }
        utf16 += ch.len_utf16() as u32;
        if utf16 > p.character {
            return None;
        }
    }
    (utf16 == p.character).then_some(start + line.len())
}
#[cfg(test)]
fn position(source: &str, byte: usize) -> Position {
    let prefix = &source[..byte];
    let line = prefix.bytes().filter(|b| *b == b'\n').count() as u32;
    let start = prefix.rfind('\n').map_or(0, |i| i + 1);
    Position {
        line,
        character: source[start..byte].encode_utf16().count() as u32,
    }
}
fn ranges(source: &str, spans: &[(usize, usize)], work: &mut Work<'_, '_>) -> Option<Vec<Range>> {
    let mut boundaries: Vec<_> = spans.iter().flat_map(|(s, e)| [*s, *e]).collect();
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut positions = Vec::with_capacity(boundaries.len());
    let mut next = 0;
    let mut current = Position {
        line: 0,
        character: 0,
    };
    for (i, (byte, ch)) in source.char_indices().enumerate() {
        if i % 1024 == 0 && !work.step(0) {
            return None;
        }
        if next < boundaries.len() && boundaries[next] == byte {
            positions.push(current);
            next += 1;
        }
        if ch == '\n' {
            current.line += 1;
            current.character = 0;
        } else {
            current.character += ch.len_utf16() as u32;
        }
    }
    if next < boundaries.len() && boundaries[next] == source.len() {
        positions.push(current);
        next += 1;
    }
    if next != boundaries.len() {
        return None;
    }
    spans
        .iter()
        .map(|(s, e)| {
            Some(Range {
                start: positions[*&boundaries.binary_search(s).ok()?],
                end: positions[*&boundaries.binary_search(e).ok()?],
            })
        })
        .collect()
}

pub fn query(
    snapshot: Snapshot<'_>,
    at: Position,
    operation: Operation,
    cancellation: &CancellationToken,
) -> anyhow::Result<Option<LocalAnswer>> {
    query_with_metrics(
        snapshot,
        at,
        operation,
        cancellation,
        &mut Metrics::default(),
    )
}
/// Metrics include rejected work; a failed probe is not a zero-cost lookup.
pub fn query_with_metrics(
    snapshot: Snapshot<'_>,
    at: Position,
    operation: Operation,
    cancellation: &CancellationToken,
    metrics: &mut Metrics,
) -> anyhow::Result<Option<LocalAnswer>> {
    *metrics = Metrics::default();
    let started = Instant::now();
    let result = query_impl(snapshot, at, operation, cancellation, metrics);
    metrics.elapsed_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    result
}
fn cursor_name(source: &str, offset: usize) -> Option<&str> {
    let chars = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'\'';
    let bytes = source.as_bytes();
    let at = if bytes.get(offset).is_some_and(|b| chars(*b)) {
        offset
    } else {
        offset.checked_sub(1).filter(|i| chars(bytes[*i]))?
    };
    let mut start = at;
    while start > 0 && chars(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < bytes.len() && chars(bytes[end]) {
        end += 1;
    }
    let name = source.get(start..end)?;
    ordinary_name(name).then_some(name)
}
fn rss_bytes() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/status")
            .ok()?
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|s| s.parse::<usize>().ok())
                    .map(|kb| kb * 1024)
            })
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
fn query_impl(
    snapshot: Snapshot<'_>,
    at: Position,
    operation: Operation,
    cancellation: &CancellationToken,
    metrics: &mut Metrics,
) -> anyhow::Result<Option<LocalAnswer>> {
    cancellation.check()?;
    let source = snapshot.source;
    if source.len() > MAX_SOURCE_BYTES
        || snapshot.version < 0
        || snapshot.uri.is_empty()
        || !(snapshot.uri.ends_with(".fs") || snapshot.uri.ends_with(".fsx"))
        || source
            .lines()
            .any(|line| line.trim_start().starts_with('#'))
        || blake3::hash(source.as_bytes()).to_hex().as_str() != snapshot.source_hash
    {
        return Ok(None);
    }
    let Some(offset) = byte_position(source, at) else {
        return Ok(None);
    };
    if cursor_name(source, offset).is_none() {
        return Ok(None);
    }
    let started = Instant::now();
    let initial_rss = rss_bytes();
    if initial_rss.is_some_and(|n| n > 96 * 1024 * 1024) {
        return Ok(None);
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_fsharp::LANGUAGE_FSHARP.into())
        .expect("pinned F# grammar ABI");
    let mut checks = 0usize;
    let mut progress = |_: &tree_sitter::ParseState| {
        checks += 1;
        let memory_exhausted = checks % 32 == 0
            && rss_bytes().is_some_and(|rss| {
                rss > 96 * 1024 * 1024
                    || initial_rss
                        .is_some_and(|initial| rss.saturating_sub(initial) > 16 * 1024 * 1024)
            });
        if cancellation.is_cancelled() || started.elapsed() > PARSE_TIME || memory_exhausted {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let bytes = source.as_bytes();
    let mut input = |i: usize, _| bytes.get(i..).unwrap_or(&[]);
    metrics.parse_started = true;
    metrics.parsed_bytes = source.len();
    let parse_started = Instant::now();
    let tree = parser.parse_with_options(
        &mut input,
        None,
        Some(ParseOptions::new().progress_callback(&mut progress)),
    );
    metrics.parse_elapsed_micros = parse_started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    cancellation.check()?;
    let Some(tree) = tree else {
        return Ok(None);
    };
    let root = tree.root_node();
    if root.has_error() {
        return Ok(None);
    }
    let mut work = Work {
        source,
        token: cancellation,
        started,
        metrics,
    };
    let leaf = (offset < source.len())
        .then(|| {
            root.descendant_for_byte_range(
                offset,
                offset + source[offset..].chars().next().unwrap().len_utf8(),
            )
        })
        .flatten();
    let previous = offset
        .checked_sub(1)
        .and_then(|i| root.descendant_for_byte_range(i, offset));
    let selected = leaf
        .filter(|n| n.kind() == "identifier")
        .or_else(|| previous.filter(|n| n.kind() == "identifier" && n.end_byte() == offset));
    let Some(selected) = selected.filter(|n| ordinary_name(raw(*n, source))) else {
        return Ok(None);
    };
    let mut candidates = Vec::new();
    let mut ancestor = Some(selected);
    let mut depth = 0;
    while let Some(n) = ancestor {
        if !work.step(depth) || n.is_missing() {
            cancellation.check()?;
            return Ok(None);
        }
        if !forbidden_context(n) {
            if let Some((_, body, params)) = function(n, source) {
                candidates.push(Candidate {
                    node: n,
                    body,
                    parameters: params,
                    local_refs: local_function(n),
                });
            } else if let Some((body, params)) = lambda(n, source) {
                candidates.push(Candidate {
                    node: n,
                    body,
                    parameters: params,
                    local_refs: argument_lambda(n),
                });
            }
        }
        ancestor = n.parent();
        depth += 1;
    }
    candidates.sort_by_key(|c| c.node.end_byte() - c.node.start_byte());
    for candidate in candidates {
        if !candidate.local_refs {
            continue;
        }
        for parameter in &candidate.parameters {
            if raw(*parameter, source) != raw(selected, source)
                || !(selected.id() == parameter.id()
                    || (candidate.body.start_byte() <= selected.start_byte()
                        && selected.end_byte() <= candidate.body.end_byte()))
            {
                continue;
            }
            let env: Vec<_> = candidate
                .parameters
                .iter()
                .map(|id| Binding {
                    name: raw(*id, source),
                    id: id.start_byte(),
                })
                .collect();
            let mut uses = Vec::new();
            if !work.expr(
                candidate.body,
                &env,
                parameter.start_byte(),
                &mut uses,
                0,
                false,
            ) {
                cancellation.check()?;
                continue;
            }
            if selected.id() != parameter.id()
                && !uses
                    .iter()
                    .any(|(s, e)| *s == selected.start_byte() && *e == selected.end_byte())
            {
                continue;
            }
            let mut spans = match operation {
                Operation::Definition => vec![(parameter.start_byte(), parameter.end_byte())],
                Operation::References {
                    include_declaration,
                } => {
                    if include_declaration {
                        uses.push((parameter.start_byte(), parameter.end_byte()));
                    }
                    uses.sort_unstable();
                    uses.dedup();
                    uses
                }
            };
            spans.push((parameter.start_byte(), parameter.end_byte()));
            let Some(mut mapped) = ranges(source, &spans, &mut work) else {
                cancellation.check()?;
                return Ok(None);
            };
            let declaration = mapped.pop().unwrap();
            cancellation.check()?;
            return Ok(Some(LocalAnswer {
                locations: mapped
                    .into_iter()
                    .map(|range| Location {
                        uri: snapshot.uri.into(),
                        range,
                    })
                    .collect(),
                declaration,
                body_start: candidate.body.start_byte(),
                body_end: candidate.body.end_byte(),
                document_version: snapshot.version,
                source_hash: snapshot.source_hash.into(),
                proof_version: PROOF_VERSION,
                parser_fingerprint: PARSER_FINGERPRINT,
                source_bytes: source.len(),
                visited_nodes: work.metrics.visited_nodes,
            }));
        }
    }
    cancellation.check()?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    const MARK: &str = "(*cursor*)";
    fn ask(marked: &str, operation: Operation) -> Option<LocalAnswer> {
        let offset = marked.find(MARK).expect("cursor marker");
        let source = marked.replacen(MARK, "", 1);
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let result = query(
            Snapshot {
                uri: "file:///Main.fs",
                source: &source,
                version: 3,
                source_hash: &hash,
            },
            position(&source, offset),
            operation,
            &CancellationToken::new(),
        )
        .unwrap();
        result
    }
    const REFS: Operation = Operation::References {
        include_declaration: true,
    };
    #[test]
    fn real_grace_and_fable_lambda_bodies_are_local_without_receiver_types() {
        let grace="module M\nlet selected ranges=ranges |> Array.sortBy (fun range -> range.OrdinalStart, (*cursor*)range.PhysicalOffset, range.PhysicalLength)\n";
        let answer = ask(grace, REFS).unwrap();
        assert_eq!(answer.locations.len(), 4);
        assert_eq!(answer.declaration.start.character, 48);
        let fable="module M\nlet selected files extension=files |> Array.filter (fun x -> x.EndsWith(extension)) |> Array.map (fun x -> (*cursor*)x.Replace('\\\\', '/'))\n";
        let answer = ask(fable, REFS).unwrap();
        assert_eq!(answer.locations.len(), 2);
        assert!(answer.declaration.start.character > 90);
    }
    #[test]
    fn nested_let_function_parameter_and_returned_function_stay_local() {
        for source in [
            "module M\nlet outer value=let inner x=(*cursor*)x+x in inner value\n",
            "module M\nlet outer ()=let inner x=(*cursor*)x+x in inner\n",
        ] {
            let answer = ask(source, REFS).unwrap();
            assert_eq!(answer.locations.len(), 3);
        }
    }
    #[test]
    fn public_and_direct_returned_lambda_parameters_require_signature_context() {
        for source in [
            "module M\nlet publicFunction x=(*cursor*)x+x\n",
            "module M\nlet publicFunction=fun x -> (*cursor*)x+x\n",
            "module M\nlet publicFunction ()=fun x -> (*cursor*)x+x\n",
        ] {
            assert!(ask(source, Operation::Definition).is_none());
            assert!(ask(source, REFS).is_none());
        }
        let computed = "module M\nlet publicFunction=id (fun x -> (*cursor*)x+x)\n";
        assert_eq!(ask(computed, REFS).unwrap().locations.len(), 3);
    }
    #[test]
    fn shadowing_tracks_nonrecursive_rhs_and_lambda_scopes() {
        let source = "module M\nlet value=call(fun x -> let y=(*cursor*)x in let x=y+1 in x)\n";
        assert_eq!(ask(source, REFS).unwrap().locations.len(), 2);
        let local_value =
            "module M\nlet value=call(fun x -> let y=x in let x=y+1 in (*cursor*)x)\n";
        assert!(ask(local_value, Operation::Definition).is_none());
        let nested = "module M\nlet value=call(fun x -> call(fun x -> x+1) (*cursor*)x + x)\n";
        assert_eq!(ask(nested, REFS).unwrap().locations.len(), 3);
        let captured = "module M\nlet value=call(fun x -> call(fun y -> (*cursor*)x+y)+x)\n";
        assert_eq!(ask(captured, REFS).unwrap().locations.len(), 3);
    }
    #[test]
    fn named_argument_or_comparison_ambiguity_never_creates_a_false_complete_set() {
        for body in ["C.M(x=x)", "g(x=x)", "C.M(x = x, y = 2)"] {
            let source = format!("module M\nlet value=call(fun (*cursor*)x -> {body})\n");
            assert!(ask(&source, REFS).is_none(), "{source}");
        }
        let rhs = "module M\nlet value=call(fun x -> (*cursor*)x=1)\n";
        assert_eq!(ask(rhs, REFS).unwrap().locations.len(), 2);
    }
    #[test]
    fn member_selectors_are_not_parameter_references() {
        let source = "module M\nlet value=call(fun x -> (*cursor*)x.x)\n";
        assert_eq!(ask(source, REFS).unwrap().locations.len(), 2);
        let selector = "module M\nlet value=call(fun x -> x.(*cursor*)x)\n";
        assert!(ask(selector, Operation::Definition).is_none());
    }
    #[test]
    fn unknown_roles_or_binders_fall_through() {
        for body in [
            "{x=x}",
            "{|x=x|}",
            "typeof<x>,x",
            "new x.C()",
            "match x with Some x -> x | None -> 0",
            "seq {yield x}",
            "<@x@>",
            "$\"{x}\"",
            "x?x",
            "let rec f y=x+y in f 1",
            "let ``x``=1 in x",
            "(fun X -> x) 1",
        ] {
            let source = format!("module M\nlet value=call(fun (*cursor*)x -> {body})\n");
            assert!(ask(&source, REFS).is_none(), "{source}");
        }
        for source in [
            "module M\ntype C=static member M(x:int)=(*cursor*)x\n",
            "module M\nlet value=call(fun ``x`` -> (*cursor*)x)\n",
            "module M\nlet value=call(fun op_Addition -> (*cursor*)op_Addition 1 2)\n",
        ] {
            assert!(ask(source, Operation::Definition).is_none());
        }
    }
    #[test]
    fn comments_and_strings_are_not_uses_and_utf16_uses_original_text() {
        let source = "module M\nlet value=call(fun x -> (\"😀 x\",(*cursor*)x, \"x\" (* x *)))\n";
        let answer = ask(source, REFS).unwrap();
        assert_eq!(answer.locations.len(), 2);
        let clean = source.replacen(MARK, "", 1);
        let use_byte = clean.find(",x,").unwrap() + 1;
        assert_eq!(answer.locations[1].range.start, position(&clean, use_byte));
        assert_eq!(
            answer.locations[1].range.end.character,
            answer.locations[1].range.start.character + 1
        );
    }
    #[test]
    fn interpolation_and_quotation_ancestors_do_not_bypass_body_closure() {
        for source in [
            r#"let text = $"{List.map (fun x -> (*cursor*)x+1) [1]}""#,
            r#"let quoted = <@ List.map (fun x -> (*cursor*)x+1) [1] @>"#,
            r#"let value = seq { yield List.map (fun x -> (*cursor*)x+1) [1] }"#,
        ] {
            assert!(ask(source, REFS).is_none());
            assert!(ask(source, Operation::Definition).is_none());
        }
    }
    #[test]
    fn directives_errors_and_deep_bodies_fall_through() {
        for source in [
            "#if DEBUG\nlet value=call(fun x -> (*cursor*)x)\n#endif\n",
            "module M\nlet broken=[1\nlet value=call(fun x -> (*cursor*)x)\n",
            "#nowarn \"1\"\nlet value=call(fun x -> (*cursor*)x)\n",
        ] {
            assert!(ask(source, REFS).is_none());
        }
        let deep = format!(
            "module M\nlet value=call(fun x -> {}(*cursor*)x{})\n",
            "(".repeat(80),
            ")".repeat(80)
        );
        assert!(ask(&deep, REFS).is_none());
    }
    #[test]
    fn exact_source_version_hash_and_cancellation_are_required() {
        let source = "let value=call(fun x -> x)";
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let token = CancellationToken::new();
        let snap = Snapshot {
            uri: "file:///Main.fs",
            source,
            version: 7,
            source_hash: &hash,
        };
        let at = position(source, source.rfind('x').unwrap());
        let answer = query(snap, at, REFS, &token).unwrap().unwrap();
        assert_eq!(answer.document_version, 7);
        assert_eq!(answer.source_hash, hash);
        assert!(query(
            Snapshot {
                source_hash: "wrong",
                ..snap
            },
            at,
            REFS,
            &token
        )
        .unwrap()
        .is_none());
        assert!(query(
            Snapshot {
                version: -1,
                ..snap
            },
            at,
            REFS,
            &token
        )
        .unwrap()
        .is_none());
        assert!(query(
            Snapshot {
                uri: "file:///Main.fsi",
                ..snap
            },
            at,
            REFS,
            &token
        )
        .unwrap()
        .is_none());
        token.cancel();
        assert!(query(snap, at, REFS, &token)
            .unwrap_err()
            .is::<crate::cancellation::Cancelled>());
    }
    #[test]
    fn edits_recompute_identity_and_reference_sets() {
        let before = ask("let value=call(fun x -> (*cursor*)x+x)", REFS).unwrap();
        let after = ask("let value=call(fun renamed -> (*cursor*)renamed)", REFS).unwrap();
        assert_ne!(before.source_hash, after.source_hash);
        assert_eq!(before.locations.len(), 3);
        assert_eq!(after.locations.len(), 2);
        assert!(ask("let value=fun renamed -> (*cursor*)renamed", REFS).is_none());
        assert!(ask(
            "let value=call(fun renamed -> C.M(renamed=(*cursor*)renamed))",
            REFS
        )
        .is_none());
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    #[test]
    fn rejected_syntax_work_is_reported_but_uppercase_prefilter_does_not_parse() {
        let source = "let value=call(fun x -> C.M(x=x))";
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let snapshot = Snapshot {
            uri: "file:///Main.fs",
            source,
            version: 1,
            source_hash: &hash,
        };
        let mut metrics = Metrics::default();
        assert!(query_with_metrics(
            snapshot,
            position(source, source.rfind('x').unwrap()),
            Operation::References {
                include_declaration: true
            },
            &CancellationToken::new(),
            &mut metrics
        )
        .unwrap()
        .is_none());
        assert!(metrics.parse_started);
        assert_eq!(metrics.parsed_bytes, source.len());
        assert!(metrics.visited_nodes > 0);
        assert!(query_with_metrics(
            snapshot,
            position(source, source.find("C.M").unwrap()),
            Operation::Definition,
            &CancellationToken::new(),
            &mut metrics
        )
        .unwrap()
        .is_none());
        assert!(!metrics.parse_started);
        assert_eq!(metrics.parsed_bytes, 0);
    }
    #[test]
    fn include_declaration_and_crlf_utf16_are_explicit() {
        let source = "module M\r\nlet value=call(fun x -> (\"😀\",x))\r\n";
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let snap = Snapshot {
            uri: "file:///Main.fs",
            source,
            version: 1,
            source_hash: &hash,
        };
        let at = position(source, source.rfind('x').unwrap());
        let answer = query(
            snap,
            at,
            Operation::References {
                include_declaration: false,
            },
            &CancellationToken::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(answer.locations.len(), 1);
        assert_eq!(answer.locations[0].range.start, at);
        assert_eq!(answer.declaration.start.line, 1);
        let emoji = position(source, source.find('😀').unwrap());
        assert!(query(
            snap,
            Position {
                character: emoji.character + 1,
                ..emoji
            },
            Operation::Definition,
            &CancellationToken::new()
        )
        .unwrap()
        .is_none());
    }
    #[test]
    fn formal_count_is_bounded_before_duplicate_detection() {
        let names = (0..33)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let source = format!("let value=call(fun {names} -> a0)");
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let mut metrics = Metrics::default();
        let answer = query_with_metrics(
            Snapshot {
                uri: "file:///Main.fs",
                source: &source,
                version: 1,
                source_hash: &hash,
            },
            position(&source, source.rfind("a0").unwrap()),
            Operation::References {
                include_declaration: true,
            },
            &CancellationToken::new(),
            &mut metrics,
        )
        .unwrap();
        assert!(answer.is_none());
        assert!(metrics.parse_started);
    }
    #[test]
    fn reference_and_source_budgets_refuse_whole_results() {
        let source = format!(
            "let value=call(fun x -> ({}))",
            std::iter::repeat_n("x", MAX_REFERENCES + 1)
                .collect::<Vec<_>>()
                .join(",")
        );
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let snapshot = Snapshot {
            uri: "file:///Main.fs",
            source: &source,
            version: 1,
            source_hash: &hash,
        };
        assert!(query(
            snapshot,
            position(&source, source.find("fun x").unwrap() + 4),
            Operation::References {
                include_declaration: true
            },
            &CancellationToken::new()
        )
        .unwrap()
        .is_none());
        let oversized = " ".repeat(MAX_SOURCE_BYTES + 1);
        let hash = blake3::hash(oversized.as_bytes()).to_hex().to_string();
        let mut metrics = Metrics::default();
        assert!(query_with_metrics(
            Snapshot {
                source: &oversized,
                source_hash: &hash,
                ..snapshot
            },
            Position {
                line: 0,
                character: 0
            },
            Operation::Definition,
            &CancellationToken::new(),
            &mut metrics
        )
        .unwrap()
        .is_none());
        assert!(!metrics.parse_started);
    }
}
