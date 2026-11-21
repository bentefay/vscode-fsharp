//! A deliberately bounded F# lexical binder. This is not a type checker.
//!
//! Exact results are restricted to simple lets, identifier parameters, and
//! indentation scopes. Unsupported binding constructs poison the proof rather
//! than silently becoming same-spelling references. Strings/comments are lexed.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Token {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub column: u32,
    pub end_column: u32,
    pub ident: bool,
}
impl Token {
    pub fn range(&self) -> Range {
        Range {
            start: Position {
                line: self.line,
                character: self.column,
            },
            end: Position {
                line: self.line,
                character: self.end_column,
            },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Declaration {
    pub name: String,
    pub token: usize,
    pub kind: String,
    pub visible_start: usize,
    pub scope_end: usize,
    pub depth: u32,
    pub top_level: bool,
    #[serde(default = "public_accessibility")]
    pub accessibility: String,
}
fn public_accessibility() -> String {
    "public".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Import {
    pub name: String,
    pub offset: usize,
    #[serde(default = "unbounded")]
    pub scope_end: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Alias {
    pub name: String,
    pub target: String,
    pub offset: usize,
    #[serde(default = "unbounded")]
    pub scope_end: usize,
}
fn unbounded() -> usize {
    usize::MAX
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UncertainSpan {
    pub start: usize,
    pub end: usize,
    pub reason: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Complete conservative source-name set for this binding uncertainty only.
    /// Consumers may prove ordinary identifier absence; this never weakens
    /// general syntax/type/label uncertainty. Generated `op_` names are excluded.
    #[serde(default)]
    pub binding_names: Option<Vec<String>>,
    /// Certified local RHS extent. Its uncertainties cannot introduce exported
    /// names outside this body; type inference and in-body lookup stay guarded.
    #[serde(default)]
    pub local_binding_scope: Option<[usize; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleScope {
    pub name: String,
    pub start: usize,
    pub end: usize,
}
/// Attribute spelling is syntax evidence, not the identity of a Core attribute.
/// Resolution must try the `Attribute` suffix before the original spelling,
/// expand aliases, and use the selected assembly/environment certificate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppliedAttribute {
    pub type_name: String,
    pub token: usize,
    pub owner: String,
    pub owner_kind: String,
    pub owner_start: usize,
    pub owner_end: usize,
    pub scope_end: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordField {
    pub name: String,
    pub token: usize,
    pub kind: String,
    pub type_name: Option<String>,
    pub type_token: Option<usize>,
    pub accessibility: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordType {
    pub name: String,
    pub token: usize,
    pub kind: String,
    pub type_parameters: Vec<String>,
    pub base_type: Option<String>,
    pub base_type_token: Option<usize>,
    pub blocked_members: Vec<String>,
    pub fields: Vec<RecordField>,
    pub accessibility: String,
    pub require_qualified_access: bool,
    pub start: usize,
    pub scope_end: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeAlias {
    pub name: String,
    pub token: usize,
    pub type_parameters: Vec<String>,
    pub target: String,
    pub target_token: usize,
    pub accessibility: String,
    pub start: usize,
    pub scope_end: usize,
}
/// Syntax evidence, not an inferred type. Resolve token identities and nominal
/// type names in their original scope before treating any fact as a proof.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TypeSource {
    Named {
        name: String,
        token: usize,
    },
    Alias {
        token: usize,
    },
    Record {
        fields: Vec<usize>,
        copy: Option<usize>,
    },
    FunctionResult {
        function: usize,
        arguments: u32,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BindingTypeFact {
    pub binding: usize,
    pub source: TypeSource,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionReturn {
    pub binding: usize,
    pub type_name: String,
    pub type_token: usize,
    pub arity: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionResultFact {
    pub binding: usize,
    pub source: TypeSource,
    pub arity: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberAccess {
    pub token: usize,
    pub receiver: usize,
    pub source: Option<TypeSource>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum IntrinsicLiteral {
    /// Syntax category only: contextual numeric conversion is unmodelled.
    UnsuffixedDecimalInt32,
    String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArgumentSource {
    IntrinsicLiteral(IntrinsicLiteral),
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeclaredParameterType {
    pub type_name: String,
    pub token: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MethodDeclaration {
    pub owner: String,
    pub type_token: usize,
    pub name: String,
    pub token: usize,
    pub is_static: bool,
    pub parameters: Vec<DeclaredParameterType>,
    pub accessibility: String,
    pub supported: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MethodCall {
    pub member: usize,
    pub receiver: usize,
    pub args: Vec<ArgumentSource>,
}
/// An explicit nominal type use. `token` identifies its final name component;
/// type variables, declaration names and expression constructor heads are absent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeUse {
    pub token: usize,
    pub name: String,
    pub generic_arity: Option<u32>,
}
/// An expression position that may refer to a logical source type. Syntax alone
/// does not distinguish a constructor from a shadowing value or member.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExpressionTypeCandidate {
    pub token: usize,
    pub name: String,
    /// None means no explicit type argument list, not a proved arity of zero.
    pub generic_arity: Option<u32>,
    pub role: ExpressionTypeRole,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ExpressionTypeRole {
    ApplicationHead,
    /// The actual callee token must resolve to the intrinsic, not its spelling.
    NameofArgument {
        callee: usize,
    },
}
/// Ordered pipeline syntax, not proof of the operator or callable identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PipelineApplication {
    pub operator: usize,
    pub input: TypeSource,
    pub callable: usize,
    pub arguments: Vec<PipelineArgument>,
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PipelineArgument {
    Value {
        source: TypeSource,
    },
    Lambda {
        parameters: Vec<usize>,
        body_start: usize,
        body_end: usize,
    },
    Unknown,
}
/// Ordered branch syntax only. Unsupported tails remain present as None.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchExpression {
    pub start: usize,
    pub end: usize,
    pub branches: Vec<MatchBranch>,
    pub expected: Option<TypeSource>,
    pub complete: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchBranch {
    pub pattern_start: usize,
    pub pattern_end: usize,
    pub guard_start: Option<usize>,
    pub guard_end: Option<usize>,
    pub body_start: usize,
    pub body_end: usize,
    pub tail: Option<TypeSource>,
}
/// Declared source DU payloads, keyed to PatternDeclaration's case identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnionCasePayload {
    pub case_token: usize,
    pub type_parameters: Vec<String>,
    pub fields: Vec<DeclaredParameterType>,
    pub supported: bool,
}
/// Immediate argument syntax only; the callee may be a shadowing value/function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallArgumentContext {
    pub callee: usize,
    pub position: u32,
    pub arity: u32,
    pub call_start: usize,
    pub call_end: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatternBinding {
    pub binding: usize,
    pub scrutinee: usize,
    pub constructor: usize,
    pub argument_index: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatternDeclaration {
    pub name: String,
    pub token: usize,
    pub module: String,
    pub kind: String,
    /// Module/type-owned declaration, rather than a function-local recognizer.
    /// This preserves export candidacy after the declaration's body scope ends;
    /// accessibility and actual import/qualification still require resolution.
    #[serde(default)]
    pub top_level: bool,
    pub scope_start: usize,
    pub scope_end: usize,
    pub type_owner: Option<String>,
    pub qualified_only: bool,
    pub accessibility: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordPatternBinding {
    pub binding: usize,
    pub field: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnconstrainedMember {
    pub token: usize,
    pub parameter: usize,
    pub function: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiteralProjectionSpan {
    pub start: usize,
    pub end: usize,
    pub first_start: usize,
    pub first_end: usize,
    pub element_count: usize,
    pub literal_kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordExpression {
    pub fields: Vec<usize>,
    pub copy: Option<usize>,
    pub binding: Option<usize>,
    pub type_name: Option<String>,
    pub type_token: Option<usize>,
    pub parent_field: Option<usize>,
    pub method_result: Option<usize>,
    /// Syntactic freshness only; the resolver must exclude .fsi constraints and
    /// prove the full ordered label environment before inferring a record.
    pub fresh_method_result: bool,
    /// Same structural checks as freshness, but same-spelling uses are separate.
    /// A resolver must prove every listed use cannot constrain this result.
    pub method_result_eligible: bool,
    /// Entire error-free checking group has one ordinary static member and a
    /// closed intrinsic-literal record RHS. No semantic leaf inference implied.
    #[serde(default)]
    pub method_result_literal_group: bool,
    /// All same-spelling tokens except the declaration, in the enclosing type
    /// group. If eligibility is false this list may be budget-truncated.
    pub method_result_uses: Vec<usize>,
    pub argument: Option<CallArgumentContext>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParsedFile {
    pub tokens: Vec<Token>,
    pub declarations: Vec<Declaration>,
    /// Ambiguous pattern identifiers that may bind values after semantic
    /// constructor/literal lookup. They are blockers, never proven bindings.
    #[serde(default)]
    pub potential_bindings: Vec<Declaration>,
    pub module: Option<String>,
    pub opens: Vec<Import>,
    pub aliases: Vec<Alias>,
    pub supported: bool,
    pub reasons: Vec<String>,
    pub source_len: usize,
    #[serde(default)]
    pub uncertain: Vec<UncertainSpan>,
    #[serde(default)]
    pub modules: Vec<ModuleScope>,
    #[serde(default)]
    pub owned_types: Vec<ModuleScope>,
    /// Modules whose public contents are implicitly imported when their parent
    /// namespace/module is opened. `start` is the module's declaration offset.
    #[serde(default)]
    pub auto_opens: Vec<ModuleScope>,
    #[serde(default)]
    pub attributes: Vec<AppliedAttribute>,
    #[serde(default)]
    pub non_value_tokens: Vec<usize>,
    /// Positive source-role facts, not parameter-name/signature slot mappings.
    /// These formals belong to callbacks or genuinely nested function bodies.
    #[serde(default)]
    pub signature_independent_formals: Vec<usize>,
    /// Ordinary source function formals, excluding method/constructor/AP roles.
    /// A paired signature can still correlate these unless independently tagged.
    #[serde(default)]
    pub ordinary_function_formals: Vec<usize>,
    #[serde(default)]
    pub record_types: Vec<RecordType>,
    #[serde(default)]
    pub binding_types: Vec<BindingTypeFact>,
    #[serde(default)]
    pub function_returns: Vec<FunctionReturn>,
    #[serde(default)]
    pub function_results: Vec<FunctionResultFact>,
    #[serde(default)]
    pub member_accesses: Vec<MemberAccess>,
    #[serde(default)]
    pub method_declarations: Vec<MethodDeclaration>,
    #[serde(default)]
    pub method_calls: Vec<MethodCall>,
    #[serde(default)]
    pub pipeline_applications: Vec<PipelineApplication>,
    #[serde(default)]
    pub type_uses: Vec<TypeUse>,
    #[serde(default)]
    pub expression_type_candidates: Vec<ExpressionTypeCandidate>,
    /// Final nominal head tokens of explicit `new` syntax. These use the type
    /// namespace and must not inherit an ordinary value receiver's identity.
    #[serde(default)]
    pub explicit_constructor_heads: Vec<usize>,
    /// First identifiers of ordinary, purely qualified expression names.
    /// Field selectors, nameof operands, patterns and type positions are absent.
    #[serde(default)]
    pub qualifier_roots: Vec<usize>,
    /// Namespace/module declaration names and module abbreviation LHS only.
    #[serde(default)]
    pub module_name_tokens: Vec<usize>,
    /// Identifier components of complete nominal type syntax, never selectors.
    #[serde(default)]
    pub type_name_tokens: Vec<usize>,
    /// Identifier components of complete ordinary `open` paths; excludes open type.
    #[serde(default)]
    pub import_name_tokens: Vec<usize>,
    /// Named DU payload labels before ':', excluding their type expressions.
    #[serde(default)]
    pub union_payload_labels: Vec<usize>,
    #[serde(default)]
    pub pattern_bindings: Vec<PatternBinding>,
    #[serde(default)]
    pub pattern_declarations: Vec<PatternDeclaration>,
    #[serde(default)]
    pub union_case_payloads: Vec<UnionCasePayload>,
    #[serde(default)]
    pub match_expressions: Vec<MatchExpression>,
    #[serde(default)]
    pub record_pattern_bindings: Vec<RecordPatternBinding>,
    #[serde(default)]
    pub type_aliases: Vec<TypeAlias>,
    #[serde(default)]
    pub record_expressions: Vec<RecordExpression>,
    #[serde(default)]
    pub unconstrained_members: Vec<UnconstrainedMember>,
    #[serde(default)]
    pub literal_islands: Vec<LiteralProjectionSpan>,
    #[serde(default)]
    pub anonymous_record_labels: Vec<usize>,
    #[serde(default)]
    pub anonymous_record_bindings: Vec<usize>,
}

fn ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
fn ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}
fn advance(src: &str, i: &mut usize, line: &mut u32, col: &mut u32) {
    let c = src[*i..].chars().next().unwrap();
    *i += c.len_utf8();
    if c == '\n' {
        *line += 1;
        *col = 0;
    } else {
        *col += c.len_utf16() as u32;
    }
}

fn lex(src: &str) -> (Vec<Token>, Vec<String>) {
    let mut ts = Vec::new();
    let mut reasons = Vec::new();
    let (mut i, mut line, mut col) = (0, 0, 0);
    while i < src.len() {
        let rest = &src[i..];
        let c = rest.chars().next().unwrap();
        if c.is_whitespace() {
            advance(src, &mut i, &mut line, &mut col);
            continue;
        }
        if rest.starts_with("//") {
            while i < src.len() && !src[i..].starts_with('\n') {
                advance(src, &mut i, &mut line, &mut col);
            }
            continue;
        }
        if rest.starts_with("(*") {
            let mut depth = 1;
            i += 2;
            col += 2;
            while i < src.len() && depth > 0 {
                if src[i..].starts_with("(*") {
                    depth += 1;
                    i += 2;
                    col += 2;
                } else if src[i..].starts_with("*)") {
                    depth -= 1;
                    i += 2;
                    col += 2;
                } else {
                    advance(src, &mut i, &mut line, &mut col);
                }
            }
            if depth != 0 {
                reasons.push("Unclosed comment".into());
            }
            continue;
        }
        if c == '"'
            || rest.starts_with("@\"")
            || rest.starts_with("$\"")
            || rest.starts_with("$@\"")
        {
            let interpolated = c == '$';
            if interpolated {
                reasons.push("Interpolated expressions require semantic parsing".into());
            }
            let verbatim = rest.starts_with("@\"") || rest.starts_with("$@\"");
            while i < src.len() && !src[i..].starts_with('"') {
                advance(src, &mut i, &mut line, &mut col);
            }
            let triple = src[i..].starts_with("\"\"\"");
            let n = if triple { 3 } else { 1 };
            i += n;
            col += n as u32;
            let mut closed = false;
            while i < src.len() {
                if triple && src[i..].starts_with("\"\"\"") {
                    i += 3;
                    col += 3;
                    closed = true;
                    break;
                }
                if !triple && src[i..].starts_with('"') {
                    if verbatim && src[i..].starts_with("\"\"") {
                        i += 2;
                        col += 2;
                        continue;
                    }
                    i += 1;
                    col += 1;
                    closed = true;
                    break;
                }
                if !verbatim && !triple && src[i..].starts_with('\\') {
                    advance(src, &mut i, &mut line, &mut col);
                    if i < src.len() {
                        advance(src, &mut i, &mut line, &mut col);
                    }
                } else {
                    advance(src, &mut i, &mut line, &mut col);
                }
            }
            if !closed {
                reasons.push("Unclosed string".into());
            }
            continue;
        }
        // Character literals, distinct from apostrophes in type variables.
        if c == '\'' {
            let mut p = i + 1;
            if p < src.len() {
                let first = src[p..].chars().next().unwrap();
                p += first.len_utf8();
                if first == '\\' && p < src.len() {
                    p += src[p..].chars().next().unwrap().len_utf8();
                }
                if p < src.len() && src[p..].starts_with('\'') {
                    while i <= p {
                        advance(src, &mut i, &mut line, &mut col);
                    }
                    continue;
                }
            }
        }
        let (start, start_line, start_col) = (i, line, col);
        let is_ident;
        if rest.starts_with("``") {
            i += 2;
            col += 2;
            while i < src.len() && !src[i..].starts_with("``") && !src[i..].starts_with('\n') {
                advance(src, &mut i, &mut line, &mut col);
            }
            if i < src.len() && src[i..].starts_with("``") {
                i += 2;
                col += 2;
            } else {
                reasons.push("Unclosed escaped identifier".into());
            }
            is_ident = true;
        } else if ident_start(c) {
            advance(src, &mut i, &mut line, &mut col);
            while i < src.len() && ident_continue(src[i..].chars().next().unwrap()) {
                advance(src, &mut i, &mut line, &mut col);
            }
            is_ident = true;
        } else {
            if c == '#' {
                reasons.push(
                    "Conditional compilation/script directives are not in the native proof subset"
                        .into(),
                );
            }
            advance(src, &mut i, &mut line, &mut col);
            is_ident = false;
        }
        let raw = &src[start..i];
        let text = if raw.starts_with("``") && raw.ends_with("``") {
            raw[2..raw.len() - 2].to_string()
        } else {
            raw.to_string()
        };
        ts.push(Token {
            text,
            start,
            end: i,
            line: start_line,
            column: start_col,
            end_column: col,
            ident: is_ident,
        });
    }
    (ts, reasons)
}

fn dotted(ts: &[Token], mut i: usize, line: u32) -> (String, usize) {
    let mut parts = Vec::new();
    while i < ts.len() && ts[i].line == line && ts[i].ident {
        parts.push(ts[i].text.clone());
        i += 1;
        if i < ts.len() && ts[i].text == "." {
            i += 1;
        } else {
            break;
        }
    }
    (parts.join("."), i)
}

pub fn parse(src: &str) -> ParsedFile {
    let (tokens, mut reasons) = lex(src);
    let mut declarations = Vec::new();
    let mut module = None;
    let mut opens = Vec::new();
    let mut aliases = Vec::new();
    // (indent, body end); scope lifetime is determined by offside boundaries.
    let mut lets: Vec<(usize, usize, usize, usize, u32)> = Vec::new();
    let ts = &tokens;
    for (i, t) in ts.iter().enumerate() {
        if [
            "match",
            "function",
            "fun",
            "for",
            "while",
            "try",
            "with",
            "use",
            "type",
            "member",
            "override",
            "abstract",
            "interface",
            "and",
            "in",
            "exception",
            "namespace",
            "inherit",
        ]
        .contains(&t.text.as_str())
        {
            reasons.push(format!("Unsupported binding/scope construct: {}", t.text));
        }
        if t.text == "module" {
            let (name, j) = dotted(ts, i + 1, t.line);
            if name.is_empty() {
                reasons.push("Incomplete module declaration".into());
                continue;
            }
            if j < ts.len() && ts[j].line == t.line && ts[j].text == "=" {
                let (target, k) = dotted(ts, j + 1, t.line);
                if !target.is_empty() && (k == ts.len() || ts[k].line > t.line) {
                    aliases.push(Alias {
                        name,
                        target,
                        offset: t.start,
                        scope_end: src.len(),
                    });
                } else {
                    reasons.push("Nested module scopes are not yet supported".into());
                }
            } else if module.is_none() && t.column == 0 {
                module = Some(name);
            } else {
                reasons.push("Multiple/nested modules are not yet supported".into());
            }
        }
        if t.text == "open" {
            let (name, _) = dotted(ts, i + 1, t.line);
            if t.column != 0 || name.is_empty() || name == "type" {
                reasons.push("Unsupported scoped open".into());
            } else {
                opens.push(Import {
                    name,
                    offset: t.start,
                    scope_end: src.len(),
                });
            }
        }
        if t.text != "let" {
            continue;
        }
        if i + 1 < ts.len() && ts[i + 1].text == "!" {
            reasons.push("Computation expression binding".into());
            continue;
        }
        let mut j = i + 1;
        let mut recursive = false;
        while j < ts.len()
            && ["rec", "mutable", "private", "internal", "public"].contains(&ts[j].text.as_str())
        {
            recursive |= ts[j].text == "rec";
            j += 1;
        }
        if j >= ts.len() || !ts[j].ident {
            reasons.push("Pattern/operator binding".into());
            continue;
        }
        let name_idx = j;
        j += 1;
        let mut params = Vec::new();
        let mut form_supported = true;
        while j < ts.len() && ts[j].line == t.line && ts[j].text != "=" {
            if ts[j].ident {
                params.push(j);
            } else if ts[j].text == "(" && j + 1 < ts.len() && ts[j + 1].text == ")" {
                j += 1;
            } else {
                form_supported = false;
            }
            j += 1;
        }
        if j >= ts.len() || ts[j].text != "=" || !form_supported {
            reasons.push(
                "Only identifier function parameters and simple let bindings are currently proven"
                    .into(),
            );
            continue;
        }
        let body_start = ts[j].end;
        let mut body_end = src.len();
        for k in j + 1..ts.len() {
            if ts[k].line > t.line && ts[k].column <= t.column {
                // Only the first real token on a later line participates in offside.
                if k == 0 || ts[k - 1].line < ts[k].line {
                    body_end = ts[k].start;
                    break;
                }
            }
        }
        let parent = lets
            .iter()
            .rev()
            .find(|(_, start, end, _, _)| *start <= t.start && t.start < *end);
        let scope_end = parent.map(|x| x.2).unwrap_or(src.len());
        let depth = parent.map(|x| x.4 + 1).unwrap_or(0);
        let top = parent.is_none() && t.column == 0;
        if parent.is_none() && !top {
            reasons.push("Unrecognized enclosing indentation scope".into());
        }
        declarations.push(Declaration {
            name: ts[name_idx].text.clone(),
            token: name_idx,
            kind: "value".into(),
            visible_start: if recursive { body_start } else { body_end },
            scope_end,
            depth,
            top_level: top,
            accessibility: ts[i + 1..name_idx]
                .iter()
                .find(|t| matches!(t.text.as_str(), "private" | "internal"))
                .map(|t| t.text.clone())
                .unwrap_or_else(public_accessibility),
        });
        for p in params {
            if ts[p].text != "_" {
                declarations.push(Declaration {
                    name: ts[p].text.clone(),
                    token: p,
                    kind: "parameter".into(),
                    visible_start: body_start,
                    scope_end: body_end,
                    depth: depth + 1,
                    top_level: false,
                    accessibility: public_accessibility(),
                });
            }
        }
        lets.push((i, body_start, body_end, t.column as usize, depth));
    }
    reasons.sort();
    reasons.dedup();
    let uncertain = if reasons.is_empty() {
        Vec::new()
    } else {
        vec![UncertainSpan {
            start: 0,
            end: src.len(),
            reason: reasons.join("; "),
            name: None,
            binding_names: None,
            local_binding_scope: None,
        }]
    };
    ParsedFile {
        tokens,
        declarations,
        potential_bindings: Vec::new(),
        module,
        opens,
        aliases,
        supported: reasons.is_empty(),
        reasons,
        source_len: src.len(),
        uncertain,
        modules: Vec::new(),
        owned_types: Vec::new(),
        auto_opens: Vec::new(),
        attributes: Vec::new(),
        non_value_tokens: Vec::new(),
        signature_independent_formals: Vec::new(),
        ordinary_function_formals: Vec::new(),
        record_types: Vec::new(),
        binding_types: Vec::new(),
        function_returns: Vec::new(),
        function_results: Vec::new(),
        member_accesses: Vec::new(),
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

impl ParsedFile {
    /// Parse uncertainty is local: unrelated declarations remain queryable.
    pub fn binding_supported(&self, token: usize) -> bool {
        self.tokens.get(token).is_some_and(|t| {
            !(token > 0
                && self.tokens[token - 1].text == "."
                && (token < 2 || !self.tokens[token - 2].ident))
                && self.non_value_tokens.binary_search(&token).is_err()
                && !self.uncertain.iter().any(|u| {
                    u.start <= t.start
                        && t.start < u.end
                        && u.name.as_ref().is_none_or(|name| name == &t.text)
                })
        })
    }
    pub fn references_supported(&self, d: &Declaration) -> bool {
        let start = self.tokens[d.token].start.min(d.visible_start);
        !self.uncertain.iter().any(|u| {
            u.start < d.scope_end
                && start < u.end
                && u.name.as_ref().is_none_or(|name| name == &d.name)
        })
    }
    pub fn module_at(&self, token: usize) -> Option<&str> {
        let p = self.tokens.get(token)?.start;
        self.modules
            .iter()
            .filter(|m| m.start <= p && p < m.end)
            .min_by_key(|m| m.end - m.start)
            .map(|m| m.name.as_str())
            .or(self.module.as_deref())
    }
    pub fn declaration_module(&self, d: &Declaration) -> Option<&str> {
        self.module_at(d.token)
    }
    pub fn token_at(&self, line: u32, character: u32) -> Option<usize> {
        self.tokens
            .iter()
            .position(|t| {
                t.ident && t.line == line && t.column <= character && character < t.end_column
            })
            .or_else(|| {
                self.tokens
                    .iter()
                    .position(|t| t.ident && t.line == line && t.end_column == character)
            })
    }
    pub fn declaration_at(&self, token: usize) -> Option<&Declaration> {
        self.declarations.iter().find(|d| d.token == token)
    }
    pub fn local_binding(&self, token: usize) -> Option<&Declaration> {
        if !self.binding_supported(token) {
            return None;
        }
        if let Some(d) = self.declaration_at(token) {
            return Some(d);
        }
        let t = &self.tokens[token];
        if token > 0 && self.tokens[token - 1].text == "." {
            return None;
        }
        self.declarations
            .iter()
            .filter(|d| d.name == t.text && d.visible_start <= t.start && t.start < d.scope_end)
            .max_by_key(|d| (d.depth, d.visible_start, self.tokens[d.token].start))
    }
    pub fn qualifier(&self, token: usize) -> Option<String> {
        if token < 2 || self.tokens[token - 1].text != "." {
            return None;
        }
        let mut i = token;
        let mut parts = Vec::new();
        while i >= 2 && self.tokens[i - 1].text == "." && self.tokens[i - 2].ident {
            parts.push(self.tokens[i - 2].text.clone());
            i -= 2;
        }
        parts.reverse();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("."))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_and_nonrecursive_rhs() {
        let p = parse(
            "module M\nlet f x =\n    let y = x\n    let g x = x + 1\n    y + x\nlet z = 1\n",
        );
        assert!(p.supported, "{:?}", p.reasons);
        let outer = p.token_at(1, 6).unwrap();
        let inner = p.token_at(3, 10).unwrap();
        assert_eq!(
            p.local_binding(p.token_at(2, 12).unwrap()).unwrap().token,
            outer
        );
        assert_eq!(
            p.local_binding(p.token_at(3, 14).unwrap()).unwrap().token,
            inner
        );
        assert_eq!(
            p.local_binding(p.token_at(4, 8).unwrap()).unwrap().token,
            outer
        );
        let p = parse("module M\nlet x = 1\nlet f () =\n    let x = x + 1\n    x\n");
        assert_eq!(
            p.local_binding(p.token_at(3, 12).unwrap()).unwrap().token,
            p.token_at(1, 4).unwrap()
        );
    }
    #[test]
    fn utf16_comments_strings() {
        let p = parse("module M\nlet x = \"😀 x\" // x\nlet y = x\n");
        assert_eq!(p.tokens.iter().filter(|x| x.text == "x").count(), 2);
        assert_eq!(
            p.token_at(2, 8).map(|i| p.tokens[i].text.as_str()),
            Some("x")
        );
    }
    #[test]
    fn refuses_unmodeled_binders() {
        assert!(!parse("let f x = match x with | Some x -> x | _ -> 0").supported);
    }
}
