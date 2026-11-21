//! Compact nominal facts. Caller proves source scope, current import coverage,
//! declaration identity and catalog freshness before using these operations.
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
const MAX_MATCHES: usize = 32;
const MAX_DEPTH: usize = 32;
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub assembly_path: String,
    pub assembly_name: String,
    pub full_name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TypeExpr {
    Named {
        identity: Identity,
        #[serde(default)]
        arguments: Vec<TypeExpr>,
    },
    TypeParameter {
        index: usize,
    },
    MethodParameter {
        index: usize,
    },
    Array {
        element: Box<TypeExpr>,
        rank: usize,
    },
    Function {
        domain: Box<TypeExpr>,
        range: Box<TypeExpr>,
    },
    /// Constructed only by a current source proof; forbidden in catalog facts.
    Source {
        path: String,
        token: usize,
        #[serde(default)]
        arguments: Vec<TypeExpr>,
    },
    Unsupported {
        #[serde(default)]
        reason: String,
    },
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeInfo {
    identity: Identity,
    #[serde(rename = "type")]
    expression: TypeExpr,
    generic_arity: usize,
    base_type: Option<TypeExpr>,
    #[serde(default)]
    declared_member_names: Vec<String>,
    supported: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemberInfo {
    declaring_type: Identity,
    name: String,
    is_static: bool,
    is_property: bool,
    parameter_count: usize,
    return_type: TypeExpr,
    supported: bool,
}
#[derive(Clone, Debug)]
pub struct LibraryMember {
    pub symbol: Value,
    pub return_type: TypeExpr,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallableInfo {
    callable_facts_version: u32,
    identity: Identity,
    declaring_module: Identity,
    source_full_name: String,
    xml_doc_id: String,
    generic_arity: usize,
    curried_arguments: Vec<Vec<TypeExpr>>,
    return_type: TypeExpr,
    supported: bool,
}
#[derive(Clone, Debug)]
pub struct LibraryCallable {
    pub symbol: Value,
    pub identity: Identity,
    pub declaring_module: Identity,
    pub source_full_name: String,
    pub xml_doc_id: String,
    pub generic_arity: usize,
    pub curried_arguments: Vec<Vec<TypeExpr>>,
    pub return_type: TypeExpr,
}
fn rows(conn: &Connection, name: &str, types_only: bool) -> Result<Option<Vec<Value>>> {
    crate::cancellation::check()?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='metadata_symbols' AND type='table')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(Some(vec![]));
    }
    let mut statement=conn.prepare("SELECT symbol FROM metadata_symbols WHERE full_name=?1 AND (?2=0 OR json_type(symbol,'$.typeInfo')='object') LIMIT 33")?;
    let mut out = Vec::new();
    for value in statement.query_map(params![name, types_only], |row| row.get::<_, String>(0))? {
        crate::cancellation::check()?;
        out.push(serde_json::from_str(&value?)?);
    }
    Ok((out.len() <= MAX_MATCHES).then_some(out))
}
fn external(v: &Value) -> bool {
    v.get("isExternal").and_then(Value::as_bool) == Some(true)
        && v.get("factsVersion").and_then(Value::as_u64) == Some(2)
}
fn portable(ty: &TypeExpr, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match ty {
        TypeExpr::Named {
            identity,
            arguments,
        } => {
            !identity.assembly_path.is_empty()
                && !identity.assembly_name.is_empty()
                && !identity.full_name.is_empty()
                && arguments.iter().all(|v| portable(v, depth + 1))
        }
        TypeExpr::TypeParameter { .. } => true,
        TypeExpr::Array { element, rank } => {
            (1..=32).contains(rank) && portable(element, depth + 1)
        }
        TypeExpr::Function { domain, range } => {
            portable(domain, depth + 1) && portable(range, depth + 1)
        }
        _ => false,
    }
}
fn substitute(ty: &TypeExpr, args: &[TypeExpr], depth: usize) -> Option<TypeExpr> {
    instantiate_at(ty, args, &[], depth)
}
/// Instantiate a certified portable scheme. Source identities can enter only
/// as already-proven arguments; catalog validation never accepts Source nodes.
pub fn instantiate(
    ty: &TypeExpr,
    type_args: &[TypeExpr],
    method_args: &[TypeExpr],
) -> Option<TypeExpr> {
    instantiate_at(ty, type_args, method_args, 0)
}
fn instantiate_at(
    ty: &TypeExpr,
    args: &[TypeExpr],
    method_args: &[TypeExpr],
    depth: usize,
) -> Option<TypeExpr> {
    if depth > MAX_DEPTH {
        return None;
    }
    match ty {
        TypeExpr::TypeParameter { index } => args.get(*index).cloned(),
        TypeExpr::MethodParameter { index } => method_args.get(*index).cloned(),
        TypeExpr::Named {
            identity,
            arguments,
        } => Some(TypeExpr::Named {
            identity: identity.clone(),
            arguments: arguments
                .iter()
                .map(|v| instantiate_at(v, args, method_args, depth + 1))
                .collect::<Option<Vec<_>>>()?,
        }),
        TypeExpr::Array { element, rank } if (1..=32).contains(rank) => Some(TypeExpr::Array {
            element: Box::new(instantiate_at(element, args, method_args, depth + 1)?),
            rank: *rank,
        }),
        TypeExpr::Function { domain, range } => Some(TypeExpr::Function {
            domain: Box::new(instantiate_at(domain, args, method_args, depth + 1)?),
            range: Box::new(instantiate_at(range, args, method_args, depth + 1)?),
        }),
        _ => None,
    }
}
pub fn lookup_type(conn: &Connection, name: &str) -> Result<Vec<TypeExpr>> {
    lookup_type_with_identity(conn, name, None)
}
/// Select declaration before expanding an alias. Same spelling in a stale or
/// unselected assembly cannot substitute for the current imported declaration.
pub fn lookup_type_for_declaration(
    conn: &Connection,
    name: &str,
    declaration: &Identity,
) -> Result<Vec<TypeExpr>> {
    lookup_type_with_identity(conn, name, Some(declaration))
}
fn lookup_type_with_identity(
    conn: &Connection,
    name: &str,
    declaration: Option<&Identity>,
) -> Result<Vec<TypeExpr>> {
    let primitive = matches!(
        name,
        "string"
            | "int"
            | "int32"
            | "int64"
            | "uint32"
            | "uint64"
            | "float"
            | "float32"
            | "double"
            | "decimal"
            | "bool"
            | "char"
            | "byte"
            | "sbyte"
            | "int16"
            | "uint16"
            | "obj"
            | "unit"
            | "nativeint"
            | "unativeint"
    );
    let key = if let Some(owner) = declaration {
        owner.full_name.clone()
    } else if primitive {
        format!("Microsoft.FSharp.Core.{name}")
    } else {
        name.into()
    };
    let Some(symbols) = rows(conn, &key, true)? else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for symbol in symbols {
        if !external(&symbol) {
            continue;
        }
        let Some(info) = symbol
            .get("typeInfo")
            .and_then(|v| serde_json::from_value::<TypeInfo>(v.clone()).ok())
        else {
            continue;
        };
        if declaration.is_none_or(|d| d == &info.identity)
            && info.supported
            && portable(&info.expression, 0)
            && !out.contains(&info.expression)
        {
            out.push(info.expression);
        }
    }
    Ok(out)
}
fn callable_type(ty: &TypeExpr, arity: usize, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match ty {
        TypeExpr::MethodParameter { index } => *index < arity,
        TypeExpr::Named {
            identity,
            arguments,
        } => {
            !identity.assembly_path.is_empty()
                && !identity.assembly_name.is_empty()
                && !identity.full_name.is_empty()
                && arguments.iter().all(|v| callable_type(v, arity, depth + 1))
        }
        TypeExpr::Array { element, rank } => {
            (1..=32).contains(rank) && callable_type(element, arity, depth + 1)
        }
        TypeExpr::Function { domain, range } => {
            callable_type(domain, arity, depth + 1) && callable_type(range, arity, depth + 1)
        }
        _ => false,
    }
}
/// The complete current term inventory selects declaration and XML identity
/// before this lookup. Catalog-subset uniqueness never establishes a binding.
pub fn lookup_callable(
    conn: &Connection,
    declaration: &Identity,
    xml_doc_id: &str,
) -> Result<Option<LibraryCallable>> {
    if xml_doc_id.is_empty() {
        return Ok(None);
    }
    let Some(symbols) = rows(conn, &declaration.full_name, false)? else {
        return Ok(None);
    };
    let mut selected = None;
    for symbol in symbols {
        if !external(&symbol) {
            continue;
        }
        let Some(info) = symbol
            .get("callableInfo")
            .and_then(|v| serde_json::from_value::<CallableInfo>(v.clone()).ok())
        else {
            continue;
        };
        if info.identity != *declaration || info.xml_doc_id != xml_doc_id {
            continue;
        }
        if selected.is_some()
            || !info.supported
            || info.callable_facts_version != 1
            || info.generic_arity > 64
            || info.source_full_name.is_empty()
            || info.declaring_module.assembly_path != declaration.assembly_path
            || info.declaring_module.assembly_name != declaration.assembly_name
            || !declaration
                .full_name
                .starts_with(&(info.declaring_module.full_name.clone() + "."))
            || symbol.get("xmlDocId").and_then(Value::as_str) != Some(xml_doc_id)
            || info.curried_arguments.is_empty()
            || info.curried_arguments.len() > 64
            || !info.curried_arguments.iter().all(|group| {
                !group.is_empty()
                    && group.len() <= 64
                    && group
                        .iter()
                        .all(|t| callable_type(t, info.generic_arity, 0))
            })
            || !callable_type(&info.return_type, info.generic_arity, 0)
        {
            return Ok(None);
        }
        selected = Some(LibraryCallable {
            symbol,
            identity: info.identity,
            declaring_module: info.declaring_module,
            source_full_name: info.source_full_name,
            xml_doc_id: info.xml_doc_id,
            generic_arity: info.generic_arity,
            curried_arguments: info.curried_arguments,
            return_type: info.return_type,
        });
    }
    Ok(selected)
}
pub fn lookup_property(
    conn: &Connection,
    receiver: &TypeExpr,
    name: &str,
    is_static: bool,
) -> Result<Option<LibraryMember>> {
    property(conn, receiver, name, is_static, 0, &mut HashSet::new())
}
fn property(
    conn: &Connection,
    receiver: &TypeExpr,
    name: &str,
    is_static: bool,
    depth: usize,
    seen: &mut HashSet<TypeExpr>,
) -> Result<Option<LibraryMember>> {
    crate::cancellation::check()?;
    if depth >= MAX_DEPTH || !seen.insert(receiver.clone()) {
        return Ok(None);
    }
    let TypeExpr::Named {
        identity,
        arguments,
    } = receiver
    else {
        return Ok(None);
    };
    let Some(rows) = rows(conn, &identity.full_name, true)? else {
        return Ok(None);
    };
    let owners = rows
        .into_iter()
        .filter(external)
        .filter_map(|v| serde_json::from_value::<TypeInfo>(v.get("typeInfo")?.clone()).ok())
        .filter(|i| i.identity == *identity && i.supported && portable(&i.expression, 0))
        .collect::<Vec<_>>();
    if owners.len() != 1 || owners[0].generic_arity != arguments.len() {
        return Ok(None);
    }
    let Some(symbols) = self::rows(conn, &format!("{}.{name}", identity.full_name), false)? else {
        return Ok(None);
    };
    let mut declared = Vec::new();
    for symbol in symbols {
        if !external(&symbol) {
            continue;
        }
        let Some(info) = symbol
            .get("memberInfo")
            .and_then(|v| serde_json::from_value::<MemberInfo>(v.clone()).ok())
        else {
            return Ok(None);
        };
        if info.declaring_type == *identity && info.name == name {
            declared.push((symbol, info));
        }
    }
    if !declared.is_empty() {
        if declared.len() != 1 {
            return Ok(None);
        }
        let (symbol, info) = declared.pop().unwrap();
        if !info.supported
            || !info.is_property
            || info.is_static != is_static
            || info.parameter_count != 0
            || !portable(&info.return_type, 0)
        {
            return Ok(None);
        }
        return Ok(
            substitute(&info.return_type, arguments, 0).map(|return_type| LibraryMember {
                symbol,
                return_type,
            }),
        );
    }
    if owners[0].declared_member_names.iter().any(|v| v == name) {
        return Ok(None);
    }
    if let Some(base) = &owners[0].base_type {
        if portable(base, 0) {
            if let Some(base) = substitute(base, arguments, 0) {
                return property(conn, &base, name, is_static, depth + 1, seen);
            }
        }
    }
    Ok(None)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn owner(path: &str, name: &str) -> Identity {
        Identity {
            assembly_path: path.into(),
            assembly_name: path.into(),
            full_name: name.into(),
        }
    }
    fn named(identity: Identity, arguments: Vec<TypeExpr>) -> TypeExpr {
        TypeExpr::Named {
            identity,
            arguments,
        }
    }
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE metadata_symbols(full_name TEXT,symbol_id TEXT,symbol TEXT)")
            .unwrap();
        c
    }
    fn insert(c: &Connection, name: &str, value: Value) {
        c.execute(
            "INSERT INTO metadata_symbols VALUES(?1,?2,?3)",
            params![name, value["id"].as_str().unwrap(), value.to_string()],
        )
        .unwrap();
    }
    #[test]
    fn declaration_identity_precedes_alias_expansion() {
        let c = conn();
        let a = owner("/a.dll", "Shared.Alias");
        let b = owner("/b.dll", "Shared.Alias");
        for (id, o, target) in [("a", &a, "First"), ("b", &b, "Second")] {
            insert(
                &c,
                "Shared.Alias",
                json!({"id":id,"isExternal":true,"factsVersion":2,"typeInfo":{"identity":o,"type":named(owner("/target.dll",target),vec![]),"genericArity":0,"supported":true}}),
            );
        }
        assert_eq!(lookup_type(&c, "Shared.Alias").unwrap().len(), 2);
        assert_eq!(
            lookup_type_for_declaration(&c, "Shared.Alias", &b).unwrap(),
            vec![named(owner("/target.dll", "Second"), vec![])]
        );
        assert!(lookup_type_for_declaration(
            &c,
            "Shared.Alias",
            &owner("/stale.dll", "Shared.Alias")
        )
        .unwrap()
        .is_empty());
    }
    #[test]
    fn legacy_or_future_fact_interpretations_do_not_prove_nominal_types() {
        let c = conn();
        let identity = owner("/library.dll", "Library.Type");
        for version in [serde_json::Value::Null, json!(1), json!(3)] {
            let row = json!({"id":"legacy","isExternal":true,"factsVersion":version,"typeInfo":{"identity":identity,"type":named(identity.clone(),vec![]),"genericArity":0,"supported":true}});
            insert(&c, "Library.Type", row);
        }
        assert!(lookup_type(&c, "Library.Type").unwrap().is_empty());
        assert!(
            lookup_property(&c, &named(identity, vec![]), "Length", false)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn nominal_substitution_rejects_ambiguous_indexed_and_wrong_owner() {
        let c = conn();
        let o = owner("/library.dll", "Library.Box`1");
        insert(
            &c,
            "Library.Box`1",
            json!({"id":"box","isExternal":true,"factsVersion":2,"typeInfo":{"identity":o,"type":named(o.clone(),vec![TypeExpr::TypeParameter{index:0}]),"genericArity":1,"supported":true}}),
        );
        let member = json!({"id":"value","isExternal":true,"factsVersion":2,"memberInfo":{"declaringType":o,"name":"Value","isStatic":false,"isProperty":true,"parameterCount":0,"returnType":{"kind":"typeParameter","index":0},"supported":true}});
        insert(&c, "Library.Box`1.Value", member.clone());
        let source = TypeExpr::Source {
            path: "/Person.fs".into(),
            token: 7,
            arguments: vec![],
        };
        let receiver = named(o.clone(), vec![source.clone()]);
        assert_eq!(
            lookup_property(&c, &receiver, "Value", false)
                .unwrap()
                .unwrap()
                .return_type,
            source
        );
        assert!(lookup_property(&c, &receiver, "Value", true)
            .unwrap()
            .is_none());
        assert!(lookup_property(&c, &named(o, vec![]), "Value", false)
            .unwrap()
            .is_none());
        assert!(lookup_property(&c, &source, "Value", false)
            .unwrap()
            .is_none());
        let mut second = member;
        second["id"] = json!("indexed");
        second["memberInfo"]["parameterCount"] = json!(1);
        insert(&c, "Library.Box`1.Value", second);
        assert!(lookup_property(&c, &receiver, "Value", false)
            .unwrap()
            .is_none());
    }
    fn callable_row(declaration: &Identity) -> Value {
        json!({"id":"callable","isExternal":true,"factsVersion":2,"xmlDocId":"M:Library.Transform.Select``2",
            "callableInfo":{"callableFactsVersion":1,"identity":declaration,"declaringModule":owner("/library.dll","Library.Transform"),
            "sourceFullName":"Library.Transform.select","xmlDocId":"M:Library.Transform.Select``2","genericArity":2,
            "curriedArguments":[[{"kind":"function","domain":{"kind":"methodParameter","index":0},"range":{"kind":"methodParameter","index":1}}],
                [{"kind":"array","rank":1,"element":{"kind":"methodParameter","index":0}}]],
            "returnType":{"kind":"array","rank":1,"element":{"kind":"methodParameter","index":1}},"supported":true}})
    }
    #[test]
    fn callable_requires_selected_identity_xml_and_current_scheme_version() {
        let c = conn();
        let declaration = owner("/library.dll", "Library.Transform.Select");
        let row = callable_row(&declaration);
        insert(&c, &declaration.full_name, row.clone());
        let xml = "M:Library.Transform.Select``2";
        assert!(
            lookup_callable(&c, &owner("/old.dll", &declaration.full_name), xml)
                .unwrap()
                .is_none()
        );
        assert!(lookup_callable(&c, &declaration, "M:Other.Select")
            .unwrap()
            .is_none());
        assert!(lookup_callable(&c, &declaration, xml).unwrap().is_some());
        for (field, invalid) in [
            ("callableFactsVersion", json!(2)),
            ("genericArity", json!(1)),
            ("supported", json!(false)),
        ] {
            c.execute("DELETE FROM metadata_symbols", []).unwrap();
            let mut bad = row.clone();
            bad["callableInfo"][field] = invalid;
            insert(&c, &declaration.full_name, bad);
            assert!(lookup_callable(&c, &declaration, xml).unwrap().is_none());
        }
        c.execute("DELETE FROM metadata_symbols", []).unwrap();
        let mut bad = row.clone();
        bad["callableInfo"]["returnType"] = json!({"kind":"source","path":"/Forged.fs","token":1});
        insert(&c, &declaration.full_name, bad);
        assert!(lookup_callable(&c, &declaration, xml).unwrap().is_none());
        c.execute("DELETE FROM metadata_symbols", []).unwrap();
        insert(&c, &declaration.full_name, row.clone());
        insert(&c, &declaration.full_name, row);
        assert!(lookup_callable(&c, &declaration, xml).unwrap().is_none());
    }
    #[test]
    fn callable_instantiation_preserves_source_identity_and_array_rank() {
        let c = conn();
        let declaration = owner("/library.dll", "Library.Transform.Select");
        insert(&c, &declaration.full_name, callable_row(&declaration));
        let callable = lookup_callable(&c, &declaration, "M:Library.Transform.Select``2")
            .unwrap()
            .unwrap();
        let source = TypeExpr::Source {
            path: "/Current.fs".into(),
            token: 42,
            arguments: vec![],
        };
        let target = named(owner("/runtime.dll", "System.Int32"), vec![]);
        let callback = instantiate(
            &callable.curried_arguments[0][0],
            &[],
            &[source.clone(), target.clone()],
        )
        .unwrap();
        assert_eq!(
            callback,
            TypeExpr::Function {
                domain: Box::new(source.clone()),
                range: Box::new(target.clone())
            }
        );
        assert_eq!(
            instantiate(
                &callable.return_type,
                &[],
                &[source.clone(), target.clone()]
            ),
            Some(TypeExpr::Array {
                element: Box::new(target),
                rank: 1
            })
        );
        assert_eq!(
            instantiate(
                &TypeExpr::Array {
                    element: Box::new(TypeExpr::TypeParameter { index: 0 }),
                    rank: 2
                },
                &[source.clone()],
                &[]
            ),
            Some(TypeExpr::Array {
                element: Box::new(source),
                rank: 2
            })
        );
        assert!(instantiate(&callable.return_type, &[], &[]).is_none());
        assert!(instantiate(
            &TypeExpr::Array {
                element: Box::new(TypeExpr::MethodParameter { index: 0 }),
                rank: 0
            },
            &[],
            &[]
        )
        .is_none());
    }
}
