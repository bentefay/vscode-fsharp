//! Streaming import of the complete external type-name inventory. The daemon
//! retains only a bounded certificate header and fetches individual names.
use anyhow::{Context, Result};
use rusqlite::{params, Connection, Statement};
use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::{cell::Cell, collections::HashSet, fmt, io::Read, rc::Rc};

pub const INPUT_LIMIT: u64 = 64 * 1024 * 1024;
pub const HEADER_LIMIT: usize = 1024 * 1024;
const ENTRY_LIMIT: usize = 128 * 1024;
const NAME_LIMIT: usize = 4096;
const LOOKUP_LIMIT: usize = 512;

pub fn schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS builtin_names(name TEXT NOT NULL,ordinal INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(name,ordinal)) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS builtin_certificate(key TEXT PRIMARY KEY,value TEXT NOT NULL) WITHOUT ROWID;")?;
    for table in ["builtin_scope_names", "builtin_term_names"] {
        db.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {table}(name TEXT NOT NULL,ordinal INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(name,ordinal)) WITHOUT ROWID;"))?;
    }
    Ok(())
}
#[derive(Clone, Copy)]
pub enum Domain {
    Types,
    Scopes,
    Terms,
}
impl Domain {
    fn table(self) -> &'static str {
        match self {
            Self::Types => "builtin_names",
            Self::Scopes => "builtin_scope_names",
            Self::Terms => "builtin_term_names",
        }
    }
}
/// Counts parser-consumed bytes, outside the buffered file reader. An oversized
/// JSON string cannot allocate the entire 64 MiB input before its row is checked.
struct BudgetReader<R> {
    inner: R,
    remaining: Rc<Cell<usize>>,
    total: u64,
}
impl<R: Read> Read for BudgetReader<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        crate::cancellation::check().map_err(std::io::Error::other)?;
        let available = self
            .remaining
            .get()
            .min((INPUT_LIMIT.saturating_sub(self.total)) as usize);
        if available == 0 {
            return Err(std::io::Error::other(
                "Inventory JSON token/input byte budget exceeded",
            ));
        }
        let length = out.len().min(available);
        let n = self.inner.read(&mut out[..length])?;
        self.remaining.set(self.remaining.get() - n);
        self.total += n as u64;
        Ok(n)
    }
}
pub fn header(db: &Connection, fingerprint: &str) -> Result<Option<Value>> {
    use rusqlite::OptionalExtension;
    let saved:Option<(String,String,String,String)>=db.query_row("SELECT f.value,e.value,p.value,h.value FROM builtin_certificate f,builtin_certificate e,builtin_certificate p,builtin_certificate h WHERE f.key='fingerprint' AND e.key='engine' AND p.key='producerPolicy' AND h.key='header'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((saved, engine, policy, header)) = saved else {
        return Ok(None);
    };
    if saved != fingerprint
        || engine != env!("FSHARP_NAVIGATOR_ENGINE_FINGERPRINT")
        || policy != crate::producer_contract::policy_fingerprint()
    {
        return Ok(None);
    }
    anyhow::ensure!(
        header.len() <= HEADER_LIMIT,
        "Persisted import environment header exceeds its bound"
    );
    Ok(Some(serde_json::from_str(&header)?))
}
pub fn store_header(db: &Connection, fingerprint: &str, header: &Value) -> Result<()> {
    let header = serde_json::to_string(header)?;
    anyhow::ensure!(
        header.len() <= HEADER_LIMIT,
        "Import environment header exceeds its bound"
    );
    let policy = crate::producer_contract::policy_fingerprint();
    for (key, value) in [
        ("fingerprint", fingerprint),
        ("engine", env!("FSHARP_NAVIGATOR_ENGINE_FINGERPRINT")),
        ("producerPolicy", policy.as_str()),
        ("header", header.as_str()),
    ] {
        db.execute(
            "INSERT OR REPLACE INTO builtin_certificate VALUES(?1,?2)",
            params![key, value],
        )?;
    }
    Ok(())
}

struct DocumentSeed<'a> {
    db: &'a Connection,
    budget: Rc<Cell<usize>>,
}
impl<'de> DeserializeSeed<'de> for DocumentSeed<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for DocumentSeed<'_> {
    type Value = Value;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an import environment certificate")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<Value, M::Error> {
        let mut header = Map::new();
        let mut seen = HashSet::new();
        let mut header_bytes = 0usize;
        loop {
            self.budget.set(4096);
            let Some(key) = map.next_key::<String>()? else {
                break;
            };
            crate::cancellation::check().map_err(M::Error::custom)?;
            if key.len() > 128 || seen.len() >= 64 || !seen.insert(key.clone()) {
                return Err(M::Error::custom(
                    "Invalid/duplicate environment certificate key",
                ));
            }
            let domain = match key.as_str() {
                "typeNames" => Some(Domain::Types),
                "scopeNames" => Some(Domain::Scopes),
                "termNames" => Some(Domain::Terms),
                _ => None,
            };
            if let Some(domain) = domain {
                let mut insert = self
                    .db
                    .prepare(&format!(
                        "INSERT INTO {}(name,ordinal,payload) VALUES(?1,?2,?3)",
                        domain.table()
                    ))
                    .map_err(M::Error::custom)?;
                self.budget.set(4096);
                map.next_value_seed(NamesSeed {
                    insert: &mut insert,
                    budget: self.budget.clone(),
                })?;
            } else {
                self.budget.set(4 * 1024 * 1024);
                let value = map.next_value::<Value>()?;
                header_bytes +=
                    key.len() + serde_json::to_vec(&value).map_err(M::Error::custom)?.len() + 8;
                if header_bytes > HEADER_LIMIT {
                    return Err(M::Error::custom(
                        "Import environment header exceeds 1 MiB bound",
                    ));
                }
                header.insert(key, value);
            }
        }
        if !seen.contains("typeNames") {
            return Err(M::Error::custom(
                "Import environment lacks complete type-name inventory",
            ));
        }
        for (marker, map) in [
            ("scopeNameInventory", "scopeNames"),
            ("termNameInventory", "termNames"),
        ] {
            if header.get(marker).is_some() && !seen.contains(map) {
                return Err(M::Error::custom(
                    "Complete inventory marker lacks its declaration map",
                ));
            }
        }
        Ok(Value::Object(header))
    }
}
struct NamesSeed<'a, 's> {
    insert: &'a mut Statement<'s>,
    budget: Rc<Cell<usize>>,
}
impl<'de> DeserializeSeed<'de> for NamesSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for NamesSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("complete external type-name map")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<(), M::Error> {
        let mut keys = 0usize;
        let mut entries = 0usize;
        loop {
            self.budget.set(32 * 1024);
            let Some(name) = map.next_key::<String>()? else {
                break;
            };
            crate::cancellation::check().map_err(M::Error::custom)?;
            keys += 1;
            if name.is_empty() || name.len() > NAME_LIMIT || keys > 100_000 {
                return Err(M::Error::custom(
                    "External type-name inventory exceeds key bound",
                ));
            }
            // A sentinel also certifies empty arrays and rejects duplicate JSON
            // name keys without retaining a second in-memory name inventory.
            self.insert
                .execute(params![name, -1i64, "null"])
                .map_err(M::Error::custom)?;
            entries += map.next_value_seed(EntriesSeed {
                insert: self.insert,
                name: &name,
                budget: self.budget.clone(),
            })?;
            if entries > 250_000 {
                return Err(M::Error::custom(
                    "External declaration inventory exceeds entry bound",
                ));
            }
        }
        Ok(())
    }
}
struct EntriesSeed<'a, 's> {
    insert: &'a mut Statement<'s>,
    name: &'a str,
    budget: Rc<Cell<usize>>,
}
impl<'de> DeserializeSeed<'de> for EntriesSeed<'_, '_> {
    type Value = usize;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<usize, D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for EntriesSeed<'_, '_> {
    type Value = usize;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("external declaration identity array")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> std::result::Result<usize, S::Error> {
        let mut count = 0usize;
        loop {
            self.budget.set(1024 * 1024);
            let Some(value) = seq.next_element::<Value>()? else {
                break;
            };
            crate::cancellation::check().map_err(S::Error::custom)?;
            let text = serde_json::to_string(&value).map_err(S::Error::custom)?;
            if text.len() > ENTRY_LIMIT {
                return Err(S::Error::custom(
                    "External declaration entry budget exceeded",
                ));
            }
            self.insert
                .execute(params![self.name, count as u64, text])
                .map_err(S::Error::custom)?;
            count += 1;
        }
        Ok(count)
    }
}

/// The caller owns a transaction, so malformed/cancelled/stale imports preserve
/// the previous complete inventory. No partial negative coverage is published.
pub fn import<R: Read>(db: &Connection, reader: R) -> Result<Value> {
    for table in [
        "builtin_names",
        "builtin_scope_names",
        "builtin_term_names",
        "builtin_certificate",
    ] {
        db.execute(&format!("DELETE FROM {table}"), [])?;
    }
    let budget = Rc::new(Cell::new(4096));
    let reader = BudgetReader {
        inner: reader,
        remaining: budget.clone(),
        total: 0,
    };
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let result = DocumentSeed {
        db,
        budget: budget.clone(),
    }
    .deserialize(&mut deserializer);
    crate::cancellation::check()?;
    let header = result.context("Read complete external declaration inventory")?;
    budget.set(4096);
    deserializer.end()?;
    Ok(header)
}
pub fn lookup(db: &Connection, fingerprint: &str, name: &str) -> Result<Option<Vec<Value>>> {
    lookup_domain(db, fingerprint, name, Domain::Types)
}
pub fn lookup_domain(
    db: &Connection,
    fingerprint: &str,
    name: &str,
    domain: Domain,
) -> Result<Option<Vec<Value>>> {
    use rusqlite::OptionalExtension;
    crate::cancellation::check()?;
    if name.len() > NAME_LIMIT {
        return Ok(None);
    }
    let current: Option<String> = db
        .query_row(
            "SELECT value FROM builtin_certificate WHERE key='fingerprint'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if current.as_deref() != Some(fingerprint) {
        return Ok(None);
    }
    let mut statement = db.prepare_cached(&format!(
        "SELECT payload FROM {} WHERE name=?1 AND ordinal>=0 ORDER BY ordinal LIMIT ?2",
        domain.table()
    ))?;
    let mut rows = statement.query(params![name, LOOKUP_LIMIT + 1])?;
    let mut values = Vec::new();
    let mut bytes = 0usize;
    while let Some(row) = rows.next()? {
        crate::cancellation::check()?;
        let text: String = row.get(0)?;
        bytes += text.len();
        anyhow::ensure!(
            values.len() < LOOKUP_LIMIT && bytes <= HEADER_LIMIT,
            "Declaration-name proof lookup exceeds its bounded result budget"
        );
        values.push(serde_json::from_str(&text)?);
    }
    Ok(Some(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(count: usize) -> Value {
        let mut names = Map::new();
        for n in 0..count {
            names.insert(format!("Type{n}"),serde_json::json!([{"identity":{"fullName":format!("Library.Type{n}"),"assemblyPath":"/Library.dll","assemblyName":"Library"},"sourceFullName":format!("Library.Type{n}"),"genericArity":0}]));
        }
        serde_json::json!({"schemaVersion":1,"typeNames":names,"small":"header"})
    }
    #[test]
    fn full_inventory_streams_to_disk_and_negative_lookup_is_certified() {
        let mut db = Connection::open_in_memory().unwrap();
        schema(&db).unwrap();
        let value = document(5000);
        let text = serde_json::to_vec(&value).unwrap();
        let tx = db.transaction().unwrap();
        let header = import(&tx, text.as_slice()).unwrap();
        assert!(header.get("typeNames").is_none());
        store_header(&tx, "certificate", &header).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM builtin_names WHERE ordinal>=0",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            5000
        );
        assert_eq!(
            lookup(&db, "certificate", "Type4999").unwrap().unwrap()[0]["sourceFullName"],
            "Library.Type4999"
        );
        assert!(lookup(&db, "certificate", "Absent")
            .unwrap()
            .unwrap()
            .is_empty());
        assert!(lookup(&db, "other-certificate", "Type1").unwrap().is_none());
    }
    #[test]
    fn invalid_import_rolls_back_complete_names_and_certificate() {
        let mut db = Connection::open_in_memory().unwrap();
        schema(&db).unwrap();
        {
            let tx = db.transaction().unwrap();
            let header = import(&tx, serde_json::to_vec(&document(3)).unwrap().as_slice()).unwrap();
            store_header(&tx, "old", &header).unwrap();
            tx.commit().unwrap();
        }
        {
            let tx = db.transaction().unwrap();
            assert!(import(
                &tx,
                b"{\"typeNames\":{\"Partial\":[{}]},\"bad\":".as_slice()
            )
            .is_err());
        }
        assert_eq!(lookup(&db, "old", "Type2").unwrap().unwrap().len(), 1);
        assert!(lookup(&db, "old", "Partial").unwrap().unwrap().is_empty());
    }
    #[test]
    fn cancellation_after_delete_and_partial_insert_rolls_back() {
        struct CancellingReader {
            source: std::io::Cursor<Vec<u8>>,
            token: crate::cancellation::CancellationToken,
            at: usize,
        }
        impl Read for CancellingReader {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let count = self.source.read(out)?;
                self.at += count;
                if self.at > 512 {
                    self.token.cancel();
                }
                Ok(count)
            }
        }
        let mut db = Connection::open_in_memory().unwrap();
        schema(&db).unwrap();
        {
            let tx = db.transaction().unwrap();
            let header = import(&tx, serde_json::to_vec(&document(3)).unwrap().as_slice()).unwrap();
            store_header(&tx, "old", &header).unwrap();
            tx.commit().unwrap();
        }
        let token = crate::cancellation::CancellationToken::new();
        {
            let _scope = token.enter();
            let tx = db.transaction().unwrap();
            let source = std::io::Cursor::new(serde_json::to_vec(&document(100)).unwrap());
            assert!(import(
                &tx,
                CancellingReader {
                    source,
                    token: token.clone(),
                    at: 0
                }
            )
            .unwrap_err()
            .is::<crate::cancellation::Cancelled>());
        }
        assert_eq!(lookup(&db, "old", "Type2").unwrap().unwrap().len(), 1);
        assert!(lookup(&db, "old", "Type99").unwrap().unwrap().is_empty());
        let tx = db.transaction().unwrap();
        let header = import(&tx, serde_json::to_vec(&document(100)).unwrap().as_slice()).unwrap();
        store_header(&tx, "retry", &header).unwrap();
        tx.commit().unwrap();
        assert_eq!(lookup(&db, "retry", "Type99").unwrap().unwrap().len(), 1);
    }
    #[test]
    fn large_name_collisions_are_retained_and_only_that_lookup_is_bounded() {
        let mut db = Connection::open_in_memory().unwrap();
        schema(&db).unwrap();
        let value = serde_json::json!({"typeNames":{"Common":(0..600).map(|n|serde_json::json!({"identity":{"fullName":format!("N{n}.Common")}})).collect::<Vec<_>>(),"Unique":[{"identity":{"fullName":"Unique"}}]}});
        let tx = db.transaction().unwrap();
        let header = import(&tx, serde_json::to_vec(&value).unwrap().as_slice()).unwrap();
        store_header(&tx, "all", &header).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM builtin_names WHERE ordinal>=0",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
            601
        );
        assert!(
            lookup(&db, "all", "Common").is_err(),
            "A partial collision set is not a complete result"
        );
        assert_eq!(lookup(&db, "all", "Unique").unwrap().unwrap().len(), 1);
    }
    fn complete_document() -> Value {
        serde_json::json!({"typeNames":{"String":[{"identity":{"fullName":"System.String"}}]},"scopeNameInventory":"all-external-namespace-and-module-declarations","scopeNames":{"System":[{"kind":"module","sourceFullName":"Shadow.System"}]},"termNameInventory":"all-external-expression-bindings","termNames":{"System":[{"kind":"value","sourceFullName":"Shadow.Values.System"}],"|>":[{"compiledName":"op_PipeRight"}]},"assemblyAutoOpens":[]})
    }
    #[test]
    fn all_domains_are_atomic_and_missing_maps_cannot_certify_absence() {
        let mut db = Connection::open_in_memory().unwrap();
        schema(&db).unwrap();
        {
            let tx = db.transaction().unwrap();
            let header = import(
                &tx,
                serde_json::to_vec(&complete_document()).unwrap().as_slice(),
            )
            .unwrap();
            store_header(&tx, "old", &header).unwrap();
            tx.commit().unwrap();
        }
        for malformed in [
            br#"{"typeNames":{},"scopeNames":{"Partial":[{}]},"termNames":{"Partial":[{}]},"bad":"#.as_slice(),
            br#"{"typeNames":{},"scopeNameInventory":"all-external-namespace-and-module-declarations"}"#.as_slice(),
            br#"{"typeNames":{},"termNameInventory":"all-external-expression-bindings"}"#.as_slice(),
        ] { let tx=db.transaction().unwrap(); assert!(import(&tx,malformed).is_err()); }
        assert_eq!(
            lookup_domain(&db, "old", "System", Domain::Scopes)
                .unwrap()
                .unwrap()[0]["sourceFullName"],
            "Shadow.System"
        );
        assert_eq!(
            lookup_domain(&db, "old", "System", Domain::Terms)
                .unwrap()
                .unwrap()[0]["sourceFullName"],
            "Shadow.Values.System"
        );
        assert_eq!(
            lookup_domain(&db, "old", "|>", Domain::Terms)
                .unwrap()
                .unwrap()[0]["compiledName"],
            "op_PipeRight"
        );
        for domain in [Domain::Types, Domain::Scopes, Domain::Terms] {
            assert!(lookup_domain(&db, "old", "Partial", domain)
                .unwrap()
                .unwrap()
                .is_empty());
        }
    }
    #[test]
    fn oversized_json_row_is_stopped_before_allocating_whole_input() {
        struct Count<R> {
            inner: R,
            bytes: Rc<Cell<usize>>,
        }
        impl<R: Read> Read for Count<R> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let n = self.inner.read(out)?;
                self.bytes.set(self.bytes.get() + n);
                Ok(n)
            }
        }
        let mut db = Connection::open_in_memory().unwrap();
        schema(&db).unwrap();
        let bytes = Rc::new(Cell::new(0));
        let prefix =
            std::io::Cursor::new(b"{\"typeNames\":{},\"termNames\":{\"Huge\":[\"".to_vec());
        let source = prefix.chain(std::io::repeat(b'x').take(16 * 1024 * 1024));
        let tx = db.transaction().unwrap();
        assert!(import(
            &tx,
            Count {
                inner: source,
                bytes: bytes.clone()
            }
        )
        .is_err());
        assert!(
            bytes.get() < 1100 * 1024,
            "Read {} bytes before rejecting oversized row",
            bytes.get()
        );
    }
    #[test]
    fn cancellation_inside_terms_preserves_all_domains_and_retry_commits() {
        struct CancelAt<R> {
            inner: R,
            token: crate::cancellation::CancellationToken,
            count: usize,
        }
        impl<R: Read> Read for CancelAt<R> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let n = self.inner.read(out)?;
                self.count += n;
                if self.count > 512 {
                    self.token.cancel();
                }
                Ok(n)
            }
        }
        let mut db = Connection::open_in_memory().unwrap();
        schema(&db).unwrap();
        {
            let tx = db.transaction().unwrap();
            let header = import(
                &tx,
                serde_json::to_vec(&complete_document()).unwrap().as_slice(),
            )
            .unwrap();
            store_header(&tx, "old", &header).unwrap();
            tx.commit().unwrap();
        }
        let replacement=format!("{{\"typeNames\":{{\"New\":[{{}}]}},\"scopeNames\":{{\"New\":[{{}}]}},\"termNames\":{{\"New\":[{}]}}}}",(0..100).map(|n|format!("{{\"n\":{n}}}")).collect::<Vec<_>>().join(","));
        let token = crate::cancellation::CancellationToken::new();
        {
            let _scope = token.enter();
            let tx = db.transaction().unwrap();
            assert!(import(
                &tx,
                CancelAt {
                    inner: replacement.as_bytes(),
                    token: token.clone(),
                    count: 0
                }
            )
            .unwrap_err()
            .is::<crate::cancellation::Cancelled>());
        }
        assert_eq!(
            lookup_domain(&db, "old", "String", Domain::Types)
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            lookup_domain(&db, "old", "System", Domain::Scopes)
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            lookup_domain(&db, "old", "System", Domain::Terms)
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        {
            let tx = db.transaction().unwrap();
            let header = import(&tx, replacement.as_bytes()).unwrap();
            store_header(&tx, "new", &header).unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(
            lookup_domain(&db, "new", "New", Domain::Terms)
                .unwrap()
                .unwrap()
                .len(),
            100
        );
    }
}
