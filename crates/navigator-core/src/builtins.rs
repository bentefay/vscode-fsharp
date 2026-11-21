//! Small explicit import-environment certificate, loaded only on a proof edge.
//! It contains names/identities, never source syntax or library documentation.
use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufReader, Read},
    path::Path,
};
#[path = "inventory.rs"]
mod inventory;

#[derive(Clone)]
struct Input {
    path: String,
    #[cfg_attr(unix, allow(dead_code))]
    hash: String,
    stamp: String,
}
#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoadMetrics {
    pub input_bytes: u64,
    pub source_json_bytes: u64,
    pub descriptor_bytes: u64,
    pub database_bytes: u64,
    pub artifact_bytes_hashed: u64,
    pub dependency_bytes_hashed: u64,
    pub reused: bool,
    pub prepared: bool,
    pub fingerprint_micros: u64,
    pub header_lookup_micros: u64,
    pub import_micros: u64,
    pub validation_micros: u64,
    pub commit_micros: u64,
    pub total_micros: u64,
}
pub(crate) struct Environment {
    data: Value,
    inputs: Vec<Input>,
    references: HashMap<String, HashSet<String>>,
    workspace: Vec<String>,
    pub fingerprint: String,
    pub load_metrics: LoadMetrics,
    scopes_complete: bool,
    terms_complete: bool,
    prepared: Option<crate::prepared::PreparedDatabase>,
}
fn stamp(path: &str) -> Option<String> {
    let m = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some("missing".into()),
        Err(_) => return None,
    };
    let time = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(format!(
            "{}:{time}:{}:{}:{}:{}",
            m.len(),
            m.dev(),
            m.ino(),
            m.ctime(),
            m.ctime_nsec()
        ))
    }
    #[cfg(not(unix))]
    {
        Some(format!("{}:{time}", m.len()))
    }
}
fn digest(path: &str) -> Result<String> {
    Ok(digest_counted(path)?.0)
}
fn digest_counted(path: &str) -> Result<(String, u64)> {
    if !Path::new(path).exists() {
        return Ok(("missing".into(), 0));
    }
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65536];
    let mut total = 0u64;
    loop {
        crate::cancellation::check()?;
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
        total += n as u64;
    }
    Ok((format!("{:x}", hash.finalize()), total))
}
impl Environment {
    pub fn load(path: &str, db: &mut rusqlite::Connection) -> Result<Self> {
        if crate::prepared::is_prepared_path(path) {
            return Self::load_prepared(path);
        }
        let started = std::time::Instant::now();
        let mut metrics = LoadMetrics {
            input_bytes: fs::metadata(path)?.len(),
            source_json_bytes: fs::metadata(path)?.len(),
            ..Default::default()
        };
        anyhow::ensure!(
            fs::metadata(path)?.len() <= inventory::INPUT_LIMIT,
            "Import environment exceeds its 64 MiB streamed input bound"
        );
        let before = stamp(path).context("Cannot stat import environment")?;
        inventory::schema(db)?;
        let phase = std::time::Instant::now();
        let (fingerprint, bytes_hashed) = digest_counted(path)?;
        metrics.artifact_bytes_hashed = bytes_hashed;
        metrics.fingerprint_micros = phase.elapsed().as_micros() as u64;
        let phase = std::time::Instant::now();
        let cached = inventory::header(db, &fingerprint)?;
        metrics.header_lookup_micros = phase.elapsed().as_micros() as u64;
        metrics.reused = cached.is_some();
        let mut transaction = None;
        let data = if let Some(header) = cached {
            header
        } else {
            let phase = std::time::Instant::now();
            let tx = db.transaction()?;
            let header = inventory::import(
                &tx,
                BufReader::with_capacity(
                    65536,
                    fs::File::open(path)?.take(inventory::INPUT_LIMIT + 1),
                ),
            )?;
            metrics.import_micros = phase.elapsed().as_micros() as u64;
            transaction = Some(tx);
            header
        };
        let phase = std::time::Instant::now();
        let (mut captured, dependency_bytes) = Self::validate_header(&data)?;
        metrics.dependency_bytes_hashed = dependency_bytes;
        anyhow::ensure!(
            stamp(path).as_ref() == Some(&before),
            "Import environment changed while loading"
        );
        // Also compare contents after import on platforms without ctime/file ID.
        #[cfg(not(unix))]
        {
            let (current, bytes_hashed) = digest_counted(path)?;
            metrics.artifact_bytes_hashed += bytes_hashed;
            anyhow::ensure!(
                current == fingerprint,
                "Import environment changed while loading"
            );
        }
        metrics.validation_micros = phase.elapsed().as_micros() as u64;
        let phase = std::time::Instant::now();
        if let Some(tx) = transaction {
            inventory::store_header(&tx, &fingerprint, &data)?;
            crate::cancellation::check()?;
            tx.commit()?;
        }
        metrics.commit_micros = phase.elapsed().as_micros() as u64;
        captured.push(Input {
            path: path.into(),
            hash: fingerprint.clone(),
            stamp: before,
        });
        metrics.total_micros = started.elapsed().as_micros() as u64;
        Self::from_header(data, captured, fingerprint, metrics, None)
    }
    fn validate_header(data: &Value) -> Result<(Vec<Input>, u64)> {
        anyhow::ensure!(
            data["schemaVersion"] == 2
                && data["kind"] == "builtin-environment"
                && data["completeness"] == "exact"
                && data["factsVersion"] == 2
                && data["typeNameInventory"] == "all-external-type-declarations"
                && data["certificateScope"] == "selected-reference-declaration-inventory",
            "Import environment is not a complete supported certificate"
        );
        crate::producer_contract::validate(&data)?;
        let inputs = data["inputHashes"]
            .as_array()
            .context("Import environment lacks input fingerprints")?;
        anyhow::ensure!(
            !inputs.is_empty(),
            "Import environment has no dependency certificate"
        );

        let mut captured = Vec::new();
        let mut dependency_bytes = 0u64;
        for input in inputs {
            crate::cancellation::check()?;
            let path = input["path"]
                .as_str()
                .context("Missing certificate input path")?
                .to_string();
            let expected = input["sha256"]
                .as_str()
                .context("Missing certificate input hash")?;
            let old = stamp(&path).context("Cannot stat certificate dependency")?;
            let (actual, bytes_hashed) = digest_counted(&path)?;
            dependency_bytes += bytes_hashed;
            anyhow::ensure!(
                actual == expected && stamp(&path).as_ref() == Some(&old),
                "Import environment is stale: {path}"
            );
            captured.push(Input {
                path,
                hash: expected.into(),
                stamp: old,
            });
        }
        // A producer file can also be a selected assembly. Its provenance
        // role never removes the independent live fact dependency.
        let captured_paths = inputs
            .iter()
            .filter_map(|input| Some((input["path"].as_str()?, input["sha256"].as_str()?)))
            .collect::<HashMap<_, _>>();
        for binding in data["referenceBindings"]
            .as_array()
            .context("Import environment lacks selected-reference bindings")?
        {
            if binding["isWorkspace"] == true {
                continue;
            }
            let selected = binding["path"]
                .as_str()
                .context("Selected reference lacks its input path")?;
            anyhow::ensure!(
                captured_paths.get(selected).is_some_and(
                    |hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
                ),
                "Selected reference lacks a live binary fact dependency: {selected}"
            );
        }
        for input in &captured {
            anyhow::ensure!(
                stamp(&input.path).as_ref() == Some(&input.stamp),
                "Import environment dependency changed while validating: {}",
                input.path
            );
        }
        Ok((captured, dependency_bytes))
    }
    fn from_header(
        data: Value,
        captured: Vec<Input>,
        fingerprint: String,
        metrics: LoadMetrics,
        prepared: Option<crate::prepared::PreparedDatabase>,
    ) -> Result<Self> {
        // Retain only small lookup facts. Input/reference certificates remain
        // on disk; their in-process forms do not keep a second JSON object tree.
        let mut references: HashMap<String, HashSet<String>> = HashMap::new();
        for entry in data["referenceBindings"].as_array().into_iter().flatten() {
            if let (Some(path), Some(identity)) =
                (entry["path"].as_str(), entry["identity"].as_str())
            {
                references
                    .entry(path.to_string())
                    .or_default()
                    .insert(identity.to_string());
            }
        }
        let workspace = data["workspaceAssemblies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().or_else(|| v["path"].as_str()))
            .map(str::to_string)
            .collect();
        let scopes_complete =
            data["scopeNameInventory"] == "all-external-namespace-and-module-declarations";
        let terms_complete = data["termNameInventory"] == "all-external-expression-bindings";
        let mut retained = serde_json::Map::new();
        let mut data = data;
        if let Some(object) = data.as_object_mut() {
            for key in ["types", "patterns", "assemblyAutoOpens"] {
                if let Some(value) = object.remove(key) {
                    retained.insert(key.into(), value);
                }
            }
        }
        Ok(Self {
            data: Value::Object(retained),
            inputs: captured,
            references,
            workspace,
            fingerprint,
            load_metrics: metrics,
            scopes_complete,
            terms_complete,
            prepared,
        })
    }
    fn load_prepared(path: &str) -> Result<Self> {
        let started = std::time::Instant::now();
        let mut metrics = LoadMetrics {
            input_bytes: fs::metadata(path)?.len(),
            descriptor_bytes: fs::metadata(path)?.len(),
            reused: true,
            prepared: true,
            ..Default::default()
        };
        let phase = std::time::Instant::now();
        let prepared =
            crate::prepared::PreparedDatabase::open(path, crate::prepared::Kind::Environment)?;
        metrics.fingerprint_micros = phase.elapsed().as_micros() as u64;
        metrics.database_bytes = prepared.descriptor.database_bytes;
        metrics.artifact_bytes_hashed = metrics.descriptor_bytes + metrics.database_bytes;
        let fingerprint = prepared.descriptor.source_sha256.clone();
        let phase = std::time::Instant::now();
        let data = inventory::header(&prepared.db, &fingerprint)?
            .context("Prepared import environment has no current inventory certificate")?;
        anyhow::ensure!(
            data == prepared.descriptor.certificate,
            "Prepared inventory header differs from its derivation certificate"
        );
        metrics.header_lookup_micros = phase.elapsed().as_micros() as u64;
        let phase = std::time::Instant::now();
        let (captured, dependency_bytes) = Self::validate_header(&data)?;
        metrics.dependency_bytes_hashed = dependency_bytes;
        anyhow::ensure!(
            prepared.valid()?,
            "Prepared inventory changed while validating"
        );
        metrics.validation_micros = phase.elapsed().as_micros() as u64;
        metrics.total_micros = started.elapsed().as_micros() as u64;
        Self::from_header(data, captured, fingerprint, metrics, Some(prepared))
    }
    pub(crate) fn prepare(source: &str, db: &mut rusqlite::Connection) -> Result<Value> {
        anyhow::ensure!(
            !crate::prepared::is_prepared_path(source),
            "Prepare an exported inventory JSON, not another prepared index"
        );
        let environment = Self::load(source, db)?;
        anyhow::ensure!(environment.valid()?, "Inventory changed while preparing");
        inventory::header(db, &environment.fingerprint)?
            .context("Prepared inventory certificate missing after import")
    }
    pub(crate) fn prepare_file(source: &str, out: &str) -> Result<Value> {
        crate::prepared::publish(
            source,
            out,
            crate::prepared::Kind::Environment,
            Self::prepare,
        )
    }
    fn lookup_db<'a>(
        &'a self,
        mutable_index: &'a rusqlite::Connection,
    ) -> Result<&'a rusqlite::Connection> {
        if let Some(prepared) = &self.prepared {
            anyhow::ensure!(
                prepared.valid()?,
                "Prepared inventory changed before lookup"
            );
            Ok(&prepared.db)
        } else {
            Ok(mutable_index)
        }
    }
    pub fn valid(&self) -> Result<bool> {
        if let Some(prepared) = &self.prepared {
            if !prepared.valid()? {
                return Ok(false);
            }
        }
        for input in &self.inputs {
            crate::cancellation::check()?;
            if stamp(&input.path).as_ref() != Some(&input.stamp) {
                return Ok(false);
            }
            #[cfg(not(unix))]
            if digest(&input.path)? != input.hash {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn unique_type(&self, name: &str, arity: usize) -> Option<Value> {
        let entries = self.data["types"][name].as_array()?;
        let entries = entries
            .iter()
            .filter(|v| v["genericArity"].as_u64() == Some(arity as u64))
            .collect::<Vec<_>>();
        if entries.len() == 1 && entries[0]["isBuiltin"] == true {
            Some(entries[0].clone())
        } else {
            None
        }
    }
    pub fn unique_pattern(&self, name: &str) -> Option<Value> {
        let entries = self.data["patterns"][name].as_array()?;
        if entries.len() == 1 && entries[0]["isBuiltin"] == true {
            Some(entries[0].clone())
        } else {
            None
        }
    }
    /// None means the certificate cannot establish negative nominal lookup.
    pub fn type_names(&self, db: &rusqlite::Connection, name: &str) -> Result<Option<Vec<Value>>> {
        inventory::lookup(self.lookup_db(db)?, &self.fingerprint, name)
    }
    /// None is unknown domain coverage; an empty vector certifies absence.
    pub fn scope_names(&self, db: &rusqlite::Connection, name: &str) -> Result<Option<Vec<Value>>> {
        if !self.scopes_complete {
            return Ok(None);
        }
        inventory::lookup_domain(
            self.lookup_db(db)?,
            &self.fingerprint,
            name,
            inventory::Domain::Scopes,
        )
    }
    pub fn term_names(&self, db: &rusqlite::Connection, name: &str) -> Result<Option<Vec<Value>>> {
        if !self.terms_complete {
            return Ok(None);
        }
        inventory::lookup_domain(
            self.lookup_db(db)?,
            &self.fingerprint,
            name,
            inventory::Domain::Terms,
        )
    }
    pub fn assembly_auto_opens(&self) -> Option<&Vec<Value>> {
        if !self.scopes_complete {
            return None;
        }
        self.data["assemblyAutoOpens"].as_array()
    }
    pub fn covers_reference(&self, path: &str, identity: &str) -> bool {
        self.references
            .get(path)
            .is_some_and(|identities| identity.is_empty() || identities.contains(identity))
    }
    pub fn workspace_assemblies(&self) -> Vec<String> {
        self.workspace.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "fsnav-builtin-inventory-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn complete_disk_inventory_reuses_rows_but_revalidates_dependency_contents() {
        let root = fixture();
        let input = root.join("Library.dll");
        let path = root.join("environment.json");
        fs::write(&input, b"assembly-v1").unwrap();
        let mut value = serde_json::json!({"schemaVersion":2,"kind":"builtin-environment","completeness":"exact","factsVersion":2,"typeNameInventory":"all-external-type-declarations","certificateScope":"selected-reference-declaration-inventory","inputHashes":[{"path":input,"sha256":digest(input.to_str().unwrap()).unwrap()}],"types":{"int":[{"genericArity":0,"isBuiltin":true,"identity":{"fullName":"System.Int32"}}]},"patterns":{},"referenceBindings":[],"workspaceAssemblies":[],"typeNames":{"Present":[{"genericArity":0,"identity":{"fullName":"Library.Present"}}]}});
        value.as_object_mut().unwrap().extend(
            crate::producer_contract::certificate()
                .as_object()
                .unwrap()
                .clone(),
        );
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let environment = Environment::load(path.to_str().unwrap(), &mut db).unwrap();
        assert!(environment.valid().unwrap());
        assert!(environment.data.get("typeNames").is_none());
        assert_eq!(
            environment
                .type_names(&db, "Present")
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        assert!(environment
            .type_names(&db, "Absent")
            .unwrap()
            .unwrap()
            .is_empty());
        assert!(environment.unique_type("int", 0).is_some());
        assert!(environment.scope_names(&db, "System").unwrap().is_none());
        assert!(environment.term_names(&db, "System").unwrap().is_none());
        assert!(environment.assembly_auto_opens().is_none());
        let oversized = root.join("oversized.json");
        fs::File::create(&oversized)
            .unwrap()
            .set_len(inventory::INPUT_LIMIT + 1)
            .unwrap();
        assert!(Environment::load(oversized.to_str().unwrap(), &mut db).is_err());
        assert_eq!(
            environment
                .type_names(&db, "Present")
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        db.execute_batch("CREATE TRIGGER reject_reimport BEFORE INSERT ON builtin_names BEGIN SELECT RAISE(FAIL,'unexpected repeated inventory import'); END;").unwrap();
        let reopened = Environment::load(path.to_str().unwrap(), &mut db).unwrap();
        assert_eq!(reopened.fingerprint, environment.fingerprint);
        assert!(reopened.valid().unwrap());
        let modified = fs::metadata(&input).unwrap().modified().unwrap();
        fs::write(&input, b"assembly-v2").unwrap();
        fs::File::options()
            .write(true)
            .open(&input)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert!(!reopened.valid().unwrap());
        assert!(Environment::load(path.to_str().unwrap(), &mut db).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    fn certified(input: &Path) -> Value {
        let mut value = serde_json::json!({"schemaVersion":2,"kind":"builtin-environment","completeness":"exact","factsVersion":2,"typeNameInventory":"all-external-type-declarations","certificateScope":"selected-reference-declaration-inventory","inputHashes":[{"path":input,"sha256":digest(input.to_str().unwrap()).unwrap()}],"types":{},"patterns":{},"workspaceAssemblies":[],"referenceBindings":[{"path":input,"identity":"Library, Version=1.0.0.0","isWorkspace":false}],"typeNames":{"Present":[{"identity":{"fullName":"Library.Present"}}]}});
        value.as_object_mut().unwrap().extend(
            crate::producer_contract::certificate()
                .as_object()
                .unwrap()
                .clone(),
        );
        value
    }
    #[test]
    fn prepared_inventory_keeps_all_domains_readonly_and_source_json_is_only_provenance() {
        let root = fixture();
        let library = root.join("Library.dll");
        let source = root.join("environment.json");
        let output = root.join("environment.fnmeta");
        fs::write(&library, b"library").unwrap();
        let mut value = certified(&library);
        value["scopeNameInventory"] =
            serde_json::json!("all-external-namespace-and-module-declarations");
        value["termNameInventory"] = serde_json::json!("all-external-expression-bindings");
        value["scopeNames"] = serde_json::json!({"Library":[{"sourceFullName":"Library","kind":"module"}],"Empty":[]});
        value["termNames"] = serde_json::json!({"answer":[{"sourceFullName":"Library.answer","kind":"value"}],"Empty":[]});
        value["assemblyAutoOpens"] = serde_json::json!([]);
        fs::write(&source, serde_json::to_vec(&value).unwrap()).unwrap();
        let publication =
            Environment::prepare_file(source.to_str().unwrap(), output.to_str().unwrap()).unwrap();
        let mut source_db = rusqlite::Connection::open_in_memory().unwrap();
        fs::remove_file(&source).unwrap();
        let environment = Environment::load(output.to_str().unwrap(), &mut source_db).unwrap();
        assert!(environment.valid().unwrap());
        assert!(environment.load_metrics.prepared);
        assert_eq!(environment.load_metrics.import_micros, 0);
        assert_eq!(environment.load_metrics.source_json_bytes, 0);
        assert_eq!(
            environment.load_metrics.descriptor_bytes,
            fs::metadata(&output).unwrap().len()
        );
        assert_eq!(
            environment.load_metrics.database_bytes,
            publication["databaseBytes"].as_u64().unwrap()
        );
        assert_eq!(
            environment.load_metrics.artifact_bytes_hashed,
            environment.load_metrics.descriptor_bytes + environment.load_metrics.database_bytes
        );
        assert_eq!(environment.load_metrics.dependency_bytes_hashed, 7);
        assert_eq!(
            source_db
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            0
        );
        for (domain, lookup) in [
            (
                "typeNames",
                Environment::type_names
                    as fn(&Environment, &rusqlite::Connection, &str) -> Result<Option<Vec<Value>>>,
            ),
            ("scopeNames", Environment::scope_names),
            ("termNames", Environment::term_names),
        ] {
            for (name, expected) in value[domain].as_object().unwrap() {
                assert_eq!(
                    serde_json::json!(lookup(&environment, &source_db, name).unwrap().unwrap()),
                    *expected
                );
            }
            assert!(lookup(&environment, &source_db, "NoSuchName")
                .unwrap()
                .unwrap()
                .is_empty());
        }
        assert!(environment
            .prepared
            .as_ref()
            .unwrap()
            .db
            .execute("DELETE FROM builtin_names", [])
            .is_err());
        let database = Path::new(publication["databasePath"].as_str().unwrap());
        assert!(!Path::new(&format!("{}-wal", database.display())).exists());
        let old_time = fs::metadata(&library).unwrap().modified().unwrap();
        fs::write(&library, b"changed").unwrap();
        fs::File::options()
            .write(true)
            .open(&library)
            .unwrap()
            .set_modified(old_time)
            .unwrap();
        assert!(!environment.valid().unwrap());
        assert!(Environment::load(output.to_str().unwrap(), &mut source_db).is_err());
        fs::write(&library, b"library").unwrap();
        assert!(Environment::load(output.to_str().unwrap(), &mut source_db).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn prepared_inventory_row_mutation_and_cancelled_build_never_certify_absence() {
        let root = fixture();
        let library = root.join("Library.dll");
        let source = root.join("environment.json");
        let output = root.join("environment.fnmeta");
        fs::write(&library, b"library").unwrap();
        fs::write(&source, serde_json::to_vec(&certified(&library)).unwrap()).unwrap();
        let token = crate::cancellation::CancellationToken::new();
        token.cancel();
        {
            let _scope = token.enter();
            assert!(
                Environment::prepare_file(source.to_str().unwrap(), output.to_str().unwrap())
                    .is_err()
            );
        }
        assert!(!output.exists());
        let publication =
            Environment::prepare_file(source.to_str().unwrap(), output.to_str().unwrap()).unwrap();
        let mut source_db = rusqlite::Connection::open_in_memory().unwrap();
        let environment = Environment::load(output.to_str().unwrap(), &mut source_db).unwrap();
        let database = Path::new(publication["databasePath"].as_str().unwrap());
        let modified = fs::metadata(database).unwrap().modified().unwrap();
        {
            let mutator = rusqlite::Connection::open(database).unwrap();
            mutator
                .execute("DELETE FROM builtin_names WHERE name='Present'", [])
                .unwrap();
        }
        fs::File::options()
            .write(true)
            .open(database)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert!(!environment.valid().unwrap());
        assert!(environment.type_names(&source_db, "Present").is_err());
        let error = Environment::load(output.to_str().unwrap(), &mut source_db)
            .err()
            .unwrap();
        assert!(error.to_string().contains("digest mismatch"), "{error:#}");
        assert!(
            Environment::prepare_file(source.to_str().unwrap(), output.to_str().unwrap()).is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn producer_role_is_portable_but_selected_dual_role_remains_live() {
        let root = fixture();
        let library = root.join("Library.dll");
        let producer = root.join("FSharp.Compiler.Service.dll");
        let path = root.join("environment.json");
        fs::write(&library, b"library").unwrap();
        fs::write(&producer, b"producer").unwrap();
        let producer_hash = digest(producer.to_str().unwrap()).unwrap();
        let mut normal = certified(&library);
        normal["producer"]["implementationHashes"][1]["sha256"] = serde_json::json!(producer_hash);
        fs::write(&path, serde_json::to_vec(&normal).unwrap()).unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let environment = Environment::load(path.to_str().unwrap(), &mut db).unwrap();
        fs::remove_file(&producer).unwrap();
        assert!(environment.valid().unwrap());
        assert!(
            Environment::load(path.to_str().unwrap(), &mut db)
                .unwrap()
                .load_metrics
                .reused
        );
        assert!(Environment::load(
            path.to_str().unwrap(),
            &mut rusqlite::Connection::open_in_memory().unwrap()
        )
        .is_ok());
        fs::write(&producer, b"producer").unwrap();
        let mut dual = normal.clone();
        dual["referenceBindings"].as_array_mut().unwrap().push(serde_json::json!({"path":producer,"identity":"FSharp.Compiler.Service","isWorkspace":false}));
        fs::write(&path, serde_json::to_vec(&dual).unwrap()).unwrap();
        assert!(Environment::load(path.to_str().unwrap(), &mut db)
            .err()
            .unwrap()
            .to_string()
            .contains("live binary fact dependency"));
        dual["inputHashes"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"path":producer,"sha256":producer_hash}));
        fs::write(&path, serde_json::to_vec(&dual).unwrap()).unwrap();
        let environment = Environment::load(path.to_str().unwrap(), &mut db).unwrap();
        fs::remove_file(&producer).unwrap();
        assert!(!environment.valid().unwrap());
        assert!(Environment::load(path.to_str().unwrap(), &mut db).is_err());
        assert!(Environment::load(
            path.to_str().unwrap(),
            &mut rusqlite::Connection::open_in_memory().unwrap()
        )
        .is_err());
        fs::write(&producer, b"producer").unwrap();
        assert!(Environment::load(path.to_str().unwrap(), &mut db).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cached_producer_policy_and_revoked_header_are_revalidated() {
        let root = fixture();
        let library = root.join("Library.dll");
        let path = root.join("environment.json");
        fs::write(&library, b"library").unwrap();
        fs::write(&path, serde_json::to_vec(&certified(&library)).unwrap()).unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let environment = Environment::load(path.to_str().unwrap(), &mut db).unwrap();
        db.execute(
            "UPDATE builtin_certificate SET value='obsolete-policy' WHERE key='producerPolicy'",
            [],
        )
        .unwrap();
        assert!(inventory::header(&db, &environment.fingerprint)
            .unwrap()
            .is_none());
        assert!(
            !Environment::load(path.to_str().unwrap(), &mut db)
                .unwrap()
                .load_metrics
                .reused
        );
        let mut header = inventory::header(&db, &environment.fingerprint)
            .unwrap()
            .unwrap();
        header["producer"]["algorithmId"] = serde_json::json!("binary-facts-v0");
        db.execute(
            "UPDATE builtin_certificate SET value=?1 WHERE key='header'",
            [serde_json::to_string(&header).unwrap()],
        )
        .unwrap();
        assert!(Environment::load(path.to_str().unwrap(), &mut db)
            .err()
            .unwrap()
            .to_string()
            .contains("revoked"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "requires FSHARP_METADATA_PROVENANCE_FIXTURE from actual helper export"]
    fn real_inventory_survives_producer_removal_with_current_fact_dependencies() {
        struct Restore(Vec<(std::path::PathBuf, std::path::PathBuf)>);
        impl Drop for Restore {
            fn drop(&mut self) {
                for (original, saved) in self.0.iter().rev() {
                    fs::rename(saved, original).expect("restore copied producer binary");
                }
            }
        }
        let root = std::path::PathBuf::from(
            std::env::var("FSHARP_METADATA_PROVENANCE_FIXTURE").expect("fixture path"),
        );
        let path = root.join("environment.json");
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let environment = Environment::load(path.to_str().unwrap(), &mut db).unwrap();
        let data: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut restore = Restore(Vec::new());
        for name in [
            "FSharpNavigator.Semantic.dll",
            "FSharp.Compiler.Service.dll",
        ] {
            let original = root.join("producer").join(name);
            assert!(original.is_file());
            assert!(!data["inputHashes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["path"].as_str() == original.to_str()));
            let saved = original.with_extension("dll.saved-for-inventory-test");
            assert!(!saved.exists());
            fs::rename(&original, &saved).unwrap();
            restore.0.push((original, saved));
        }
        assert!(environment.valid().unwrap());
        assert!(
            Environment::load(path.to_str().unwrap(), &mut db)
                .unwrap()
                .load_metrics
                .reused
        );
        let fresh = Environment::load(
            path.to_str().unwrap(),
            &mut rusqlite::Connection::open_in_memory().unwrap(),
        )
        .unwrap();
        assert!(fresh.valid().unwrap());
        drop(restore);
    }
    #[test]
    #[ignore = "requires genuine old and current metadata fixture paths"]
    fn real_legacy_and_revoked_inventory_are_rejected_by_new_reader() {
        let old =
            std::env::var("FSHARP_LEGACY_METADATA_ENVIRONMENT").expect("legacy artifact path");
        let legacy: Value = serde_json::from_slice(&fs::read(&old).unwrap()).unwrap();
        assert_eq!(legacy["schemaVersion"], 1);
        for input in legacy["inputHashes"].as_array().unwrap() {
            assert_eq!(
                digest(input["path"].as_str().unwrap()).unwrap(),
                input["sha256"].as_str().unwrap()
            );
        }
        let error = Environment::load(&old, &mut rusqlite::Connection::open_in_memory().unwrap())
            .err()
            .expect("legacy schema accepted");
        assert!(
            error.to_string().contains("supported certificate"),
            "{error}"
        );
        let current =
            std::path::PathBuf::from(std::env::var("FSHARP_METADATA_PROVENANCE_FIXTURE").unwrap())
                .join("environment.json");
        let mut value: Value = serde_json::from_slice(&fs::read(current).unwrap()).unwrap();
        assert_eq!(value["schemaVersion"], 2);
        value["producer"]["algorithmId"] = serde_json::json!("binary-facts-v1");
        let root = fixture();
        let negative = root.join("deliberately-revoked-producer.json");
        fs::write(&negative, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = Environment::load(
            negative.to_str().unwrap(),
            &mut rusqlite::Connection::open_in_memory().unwrap(),
        )
        .err()
        .expect("revoked algorithm accepted");
        assert!(error.to_string().contains("revoked"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }
}
