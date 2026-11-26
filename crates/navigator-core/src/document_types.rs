//! Same-document nominal name proofs. No project, assembly, or inference fallback.
use super::*;

use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Default)]
struct Environment<'a> {
    types: BTreeMap<(String, usize), Node<'a>>,
    modules: BTreeMap<String, Rc<Environment<'a>>>,
}

fn ids<'a>(node: Node<'a>, source: &str) -> Option<Vec<String>> {
    let nodes = if node.kind() == "identifier" {
        vec![node]
    } else {
        children(node)
    };
    nodes
        .into_iter()
        .map(|id| {
            (id.kind() == "identifier" && name(raw(id, source))).then(|| raw(id, source).to_owned())
        })
        .collect()
}

fn module<'a>(env: &Environment<'a>, path: &[String]) -> Option<Rc<Environment<'a>>> {
    let mut result = env.modules.get(path.first()?)?.clone();
    for part in &path[1..] {
        result = result.modules.get(part)?.clone();
    }
    Some(result)
}

fn resolve<'a>(env: &Environment<'a>, path: &[String], arity: usize) -> Option<Node<'a>> {
    let name = path.last()?.clone();
    if path.len() == 1 {
        return env.types.get(&(name, arity)).copied();
    }
    // Nested types take precedence over module paths. Their resolution is not
    // covered by this route; a known competing type must withhold the proof.
    if env.types.keys().any(|(name, _)| name == &path[0]) {
        return None;
    }
    module(env, &path[..path.len() - 1])?
        .types
        .get(&(name, arity))
        .copied()
}

fn nominal_use(selected: Node<'_>, source: &str) -> Option<(Vec<String>, usize)> {
    let name = selected.parent()?;
    // `type Alias = Existing` is syntactically a singleton union until name
    // resolution proves Existing is a type. An explicit bar/payload is a case.
    if name.kind() == "union_type_case" && children(name).len() == 1 {
        let cases = name.parent()?;
        let declaration = cases.parent()?;
        if cases.kind() == "union_type_cases"
            && children(cases).len() == 1
            && !token(cases, "|")
            && declaration.kind() == "union_type_defn"
            && children(declaration).len() == 2
        {
            return Some((vec![raw(selected, source).to_owned()], 0));
        }
        return None;
    }
    if name.kind() != "long_identifier" || children(name).last()?.id() != selected.id() {
        return None;
    }
    let owner = name.parent()?;
    let arity = match owner.kind() {
        "simple_type" => 0,
        "generic_type" => {
            let arguments = children(owner)
                .into_iter()
                .find(|n| n.kind() == "type_attributes")?;
            children(arguments).len()
        }
        "postfix_type" if children(owner).last()?.id() == name.id() => 1,
        _ => return None,
    };
    Some((ids(name, source)?, arity))
}

fn type_head<'a>(node: Node<'a>, source: &str) -> Option<(String, usize, Node<'a>, bool)> {
    if node.kind() == "type_extension" {
        return None;
    }
    let head = children(node)
        .into_iter()
        .find(|n| n.kind() == "type_name")?;
    let name = head.child_by_field_name("type_name")?;
    let parts = ids(name, source)?;
    if parts.len() != 1 {
        return None;
    }
    let target = if name.kind() == "identifier" {
        name
    } else {
        children(name)[0]
    };
    let mut arity = 0;
    for child in children(head) {
        if child.kind() == "type_argument" {
            arity += 1;
        }
        if child.kind() == "type_arguments" {
            arity += children(child)
                .iter()
                .filter(|n| n.kind() == "type_argument_defn")
                .count();
        }
    }
    let public = !children(head)
        .iter()
        .any(|n| n.kind() == "access_modifier" && raw(*n, source) == "private");
    Some((parts[0].clone(), arity, target, public))
}

// Walk declarations in source order. Unknown opens invalidate the visible
// tables, never fabricate absence of a matching external name. A subsequent
// local declaration or a fully known local open establishes fresh evidence.
fn scope<'a>(
    node: Node<'a>,
    selected: Node<'a>,
    path: &[String],
    arity: usize,
    inherited: Environment<'a>,
    work: &mut Work<'_, '_>,
    depth: usize,
) -> Option<(Environment<'a>, Option<Node<'a>>)> {
    if !work.step(depth) || token(node, "rec") {
        return None;
    }
    let mut visible = inherited;
    let mut exported = Environment::default();
    for item in children(node) {
        if !work.step(depth + 1) {
            return None;
        }
        let contains =
            item.start_byte() <= selected.start_byte() && selected.end_byte() <= item.end_byte();
        if item.start_byte() > selected.start_byte() {
            break;
        }
        match item.kind() {
            "named_module" | "namespace" => {
                if contains {
                    return scope(
                        item,
                        selected,
                        path,
                        arity,
                        Environment::default(),
                        work,
                        depth + 1,
                    );
                }
                // Other namespace groups may merge with external sources.
                visible = Environment::default();
            }
            "module_defn" => {
                if children(item).iter().any(|n| n.kind() == "attributes") || token(item, "rec") {
                    // Auto-open/recursive modules may introduce or shadow exported
                    // names too. Withhold the whole module table, not just visibility.
                    return None;
                }
                let head = children(item)
                    .into_iter()
                    .find(|n| n.kind() == "identifier")?;
                let name = raw(head, work.source).to_owned();
                if !self::name(&name) {
                    return None;
                }
                let body: Vec<_> = children(item)
                    .into_iter()
                    .filter(|n| n.start_byte() > head.end_byte() && n.kind() != "access_modifier")
                    .collect();
                let alias = body.len() == 1
                    && matches!(
                        body[0].kind(),
                        "identifier" | "long_identifier" | "long_identifier_or_op"
                    );
                if alias {
                    let target = ids(body[0], work.source).and_then(|p| module(&visible, &p));
                    visible.modules.remove(&name);
                    if let Some(target) = target {
                        visible.modules.insert(name, target);
                    }
                    if contains {
                        return Some((exported, None));
                    }
                } else {
                    let (contents, found) = scope(
                        item,
                        selected,
                        path,
                        arity,
                        visible.clone(),
                        work,
                        depth + 1,
                    )?;
                    if contains {
                        return Some((exported, found));
                    }
                    let contents = Rc::new(contents);
                    visible.modules.insert(name.clone(), contents.clone());
                    if !children(item)
                        .iter()
                        .any(|n| n.kind() == "access_modifier" && raw(*n, work.source) == "private")
                    {
                        exported.modules.insert(name, contents);
                    }
                }
            }
            "module_abbrev" => {
                let cs = children(item);
                let head = cs.iter().find(|n| n.kind() == "identifier")?;
                let target = cs
                    .iter()
                    .find(|n| n.kind() == "long_identifier")
                    .and_then(|n| ids(*n, work.source))
                    .and_then(|p| module(&visible, &p));
                let name = raw(*head, work.source).to_owned();
                visible.modules.remove(&name);
                if let Some(target) = target {
                    visible.modules.insert(name, target);
                }
            }
            "import_decl" => {
                let target = if token(item, "type") {
                    None
                } else {
                    children(item)
                        .into_iter()
                        .find(|n| n.kind() == "long_identifier")
                        .and_then(|n| ids(n, work.source))
                        .and_then(|p| module(&visible, &p))
                };
                if let Some(target) = target {
                    visible.types.extend(target.types.clone());
                    visible.modules.extend(target.modules.clone());
                } else {
                    visible = Environment::default();
                }
            }
            "type_definition" => {
                for declaration in children(item) {
                    // Unsupported heads (notably escaped names) can shadow an
                    // ordinary spelling. Never retain an outer exact binding
                    // or export a misleadingly empty module in that case.
                    let (name, count, target, public) = type_head(declaration, work.source)?;
                    visible.types.insert((name.clone(), count), target);
                    if public {
                        exported.types.insert((name, count), target);
                    }
                }
                if contains {
                    return Some((exported, resolve(&visible, path, arity)));
                }
            }
            "compiler_directive_decl" => return None,
            _ if contains => return Some((exported, resolve(&visible, path, arity))),
            _ => {}
        }
    }
    Some((exported, None))
}

pub(super) fn name(s: &str) -> bool {
    s.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'\'')
}

pub(super) fn definition<'a>(
    root: Node<'a>,
    selected: Node<'a>,
    work: &mut Work<'_, '_>,
) -> Option<Node<'a>> {
    let (path, arity) = nominal_use(selected, work.source)?;
    scope(
        root,
        selected,
        &path,
        arity,
        Environment::default(),
        work,
        0,
    )?
    .1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(marked: &str, exact: bool) {
        let target = marked.find('«').unwrap();
        let end = marked.find('»').unwrap();
        let name = &marked[target + '«'.len_utf8()..end];
        let source = marked.replace(['«', '»'], "");
        let query = source.find('§').unwrap();
        let source = source.replace('§', "");
        let source_hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let snapshot = Snapshot {
            uri: "file:///Main.fs",
            source: &source,
            version: 1,
            source_hash: &source_hash,
        };
        let at = position(&source, query);
        let result = query_with_metrics(
            snapshot,
            at,
            Operation::Definition,
            &CancellationToken::default(),
            &mut Metrics::default(),
        )
        .unwrap();
        if exact {
            let target = target - usize::from(marked[..target].contains('§')) * '§'.len_utf8();
            assert_eq!(
                result.expect(marked).locations,
                [Location {
                    uri: snapshot.uri.into(),
                    range: Range {
                        start: position(&source, target),
                        end: position(&source, target + name.len())
                    }
                }],
                "{marked}"
            );
        } else {
            assert!(result.is_none(), "{marked}: {result:?}");
        }
        // Exported nominal types do not have a complete document-local reference domain.
        assert!(query_with_metrics(
            snapshot,
            at,
            Operation::References {
                include_declaration: true
            },
            &CancellationToken::default(),
            &mut Metrics::default()
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn document_types_positive_matrix() {
        for source in [
            "module M\ntype «Id» = int\nlet f (x:§Id) = x\n",
            "module M\nmodule Types =\n    type «SagaId» = int\nopen Types\nlet eventToSagaId () : Option<§SagaId> = None\n",
            "[<RequireQualifiedAccess>]\nmodule M\nopen External\nmodule Types =\n    type «SagaId» = int\nopen Types\nlet eventToSagaId () : Option<§SagaId> = None\n",
            "module M\nmodule Types =\n    type «Id» = int\nlet f (x:Types.§Id) = x\n",
            "module M\nmodule Types =\n    type «Id» = int\nmodule Alias = Types\nlet f (x:Alias.§Id) = x\n",
            "module M\ntype «Record» = { Value:int }\nlet f (x:§Record) = x\n",
            "module M\ntype «Choice» = A | B\nlet f (x:§Choice) = x\n",
            "module M\ntype «Class»() = member _.Value = 1\nlet f (x:§Class) = x\n",
            "module M\ntype «Box»<'a> = { Value:'a }\nlet f (x:§Box<int>) = x\n",
            "module M\ntype «Id» = int\ntype Next = §Id\n",
            "module M\ntype «Id» = int\ntype Record = { Value:§Id }\n",
            "module M\ntype «Id» = int\nmodule Inner =\n    let f (x:§Id) = x\n",
            "module M\nmodule First =\n    type Id = int\nmodule Second =\n    type «Id» = string\nopen First\nopen Second\nlet f (x:§Id) = x\n",
            "module M\ntype Id = int\nmodule Inner =\n    type «Id» = string\n    let f (x:§Id) = x\n",
        ] { check(source, true); }
    }

    #[test]
    fn document_types_refuse_unproven_binding_matrix() {
        for source in [
            "module M\ntype «Id» = int\nmodule Inner =\n    type ``Id`` = string\n    let f (x:§Id) = x\n",
            "module M\ntype «Id» = int\nmodule Inner =\n    type ``Id`` = string\nopen Inner\nlet f (x:§Id) = x\n",
            "module M\ntype «Id» = int\nopen External\nlet f (x:§Id) = x\n",
            "module M\nmodule Types =\n    type «Id» = int\nopen External\nopen Types\nlet f (x:§Id) = x\n",
            "module M\nmodule Types =\n    type «Id» = int\nlet f (x:§Id) = x\n",
            "module M\ntype «Box»<'a> = { Value:'a }\nlet f (x:§Box) = x\n",
            "module M\ntype «Id» = int\nlet Id = 1\nlet x = §Id\n",
            "module M\ntype «Class»() = class end\nlet x = §Class()\n",
            "module M\nlet f (x:§Id) = x\ntype «Id» = int\n",
            "module M\nmodule Outer =\n    type «Id» = int\n    [<AutoOpen>]\n    module Inner =\n        type Id = string\nopen Outer\nlet f (x:§Id) = x\n",
            "module M\nmodule Outer =\n    type private «Id» = int\nlet f (x:Outer.§Id) = x\n",
            "module M\n#if DEBUG\ntype «Id» = int\n#endif\nlet f (x:§Id) = x\n",
        ] { check(source, false); }
    }
}
