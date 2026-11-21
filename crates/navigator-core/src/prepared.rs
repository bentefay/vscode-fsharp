//! Explicit, standalone read-only metadata indexes. The source JSON is
//! derivation provenance; its certified assembly/XML inputs remain live.
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const DESCRIPTOR_LIMIT: u64 = 2 * 1024 * 1024;
const DATABASE_LIMIT: u64 = 1024 * 1024 * 1024;
const BUILDER: &str = env!("FSHARP_NAVIGATOR_ENGINE_FINGERPRINT");
const FORMAT: &str = "fsharp-navigator.prepared-metadata";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Catalog,
    Environment,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptor {
    kind: String,
    schema_version: u32,
    pub artifact_kind: Kind,
    builder_fingerprint: String,
    producer_policy: String,
    pub source_sha256: String,
    pub certificate: Value,
    pub semantic_sha256: String,
    derivation_key: String,
    database_file: String,
    pub database_sha256: String,
    pub database_bytes: u64,
}
#[derive(Clone, PartialEq, Eq)]
struct Identity {
    stamp: String,
    bytes: u64,
}
fn identity(path: &Path) -> Result<Identity> {
    let m = fs::metadata(path)?;
    anyhow::ensure!(
        m.is_file(),
        "Prepared artifact is not a regular file: {}",
        path.display()
    );
    let time = m.modified()?.duration_since(UNIX_EPOCH)?.as_nanos();
    #[cfg(unix)]
    let stamp = {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{time}:{}:{}:{}:{}",
            m.dev(),
            m.ino(),
            m.ctime(),
            m.ctime_nsec()
        )
    };
    #[cfg(not(unix))]
    let stamp = time.to_string();
    Ok(Identity {
        stamp,
        bytes: m.len(),
    })
}
fn regular(path: &Path) -> Result<File> {
    let mut open = fs::OpenOptions::new();
    open.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.custom_flags(libc::O_NONBLOCK);
    }
    let file = open.open(path)?;
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "Prepared input is not a regular file"
    );
    Ok(file)
}
pub(crate) fn digest(path: &Path, limit: u64) -> Result<String> {
    let before = identity(path)?;
    anyhow::ensure!(before.bytes <= limit, "Prepared input exceeds byte bound");
    let mut input = regular(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut total = 0u64;
    loop {
        crate::cancellation::check()?;
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        anyhow::ensure!(total <= limit, "Prepared input grew beyond byte bound");
        hash.update(&buffer[..n]);
    }
    anyhow::ensure!(
        identity(path)? == before,
        "Prepared input changed while hashing"
    );
    Ok(format!("{:x}", hash.finalize()))
}
fn hash_json(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn validate_live_inputs(certificate: &Value) -> Result<()> {
    let inputs = certificate["inputHashes"]
        .as_array()
        .context("Prepared certificate lacks live input hashes")?;
    anyhow::ensure!(
        !inputs.is_empty() && inputs.len() <= 10000,
        "Prepared live input count invalid"
    );
    let mut captured = Vec::new();
    for input in inputs {
        crate::cancellation::check()?;
        let path = Path::new(
            input["path"]
                .as_str()
                .context("Prepared live input path missing")?,
        );
        let expected = input["sha256"]
            .as_str()
            .context("Prepared live input hash missing")?;
        match (fs::metadata(path), expected) {
            (Err(error), "missing")
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            (Ok(meta), "directory") if meta.is_dir() => {}
            (Ok(meta), _) if meta.is_file() && hex(expected) => {
                let before = identity(path)?;
                anyhow::ensure!(
                    digest(path, DATABASE_LIMIT)? == expected,
                    "Prepared live input changed before publication: {}",
                    path.display()
                );
                captured.push((path.to_path_buf(), expected.to_string(), Some(before)));
                continue;
            }
            _ => anyhow::bail!(
                "Prepared live input changed kind or is unsupported: {}",
                path.display()
            ),
        }
        captured.push((path.to_path_buf(), expected.to_string(), None));
    }
    // Earlier hashed inputs must still describe the same generation after
    // validating later dependencies; directory contents remain irrelevant.
    for (path, expected, before) in captured {
        match before {
            Some(before) => anyhow::ensure!(
                identity(&path)? == before,
                "Prepared live input changed during final validation"
            ),
            None if expected == "directory" => anyhow::ensure!(
                fs::metadata(path)?.is_dir(),
                "Prepared XML directory witness changed"
            ),
            None => match fs::metadata(path) {
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) => {}
                _ => anyhow::bail!("Prepared missing input witness changed"),
            },
        }
    }
    Ok(())
}
fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
pub fn is_prepared_path(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|v| v == "fnmeta")
}
/// Explicit automation entry point; ordinary queries never invoke a builder.
pub fn prepare_file(source: &str, out: &str, kind: Kind) -> Result<Value> {
    match kind {
        Kind::Catalog => crate::metadata::prepare_file(source, out),
        Kind::Environment => crate::builtins::Environment::prepare_file(source, out),
    }
}
fn standalone(path: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        match fs::symlink_metadata(format!("{}{suffix}", path.display())) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => anyhow::bail!("Prepared database has an unexpected {suffix} sidecar"),
        }
    }
    Ok(())
}
// Expected column names/types/pk order. Primary-key nullability differs for
// rowid/WITHOUT ROWID; full exact SQL is also restricted to ordinary tables.
fn tables(kind: Kind) -> Vec<(&'static str, Vec<(&'static str, &'static str, i64)>)> {
    let mut tables = match kind {
        Kind::Environment => vec![
            (
                "builtin_names",
                vec![
                    ("name", "TEXT", 1),
                    ("ordinal", "INTEGER", 2),
                    ("payload", "TEXT", 0),
                ],
            ),
            (
                "builtin_scope_names",
                vec![
                    ("name", "TEXT", 1),
                    ("ordinal", "INTEGER", 2),
                    ("payload", "TEXT", 0),
                ],
            ),
            (
                "builtin_term_names",
                vec![
                    ("name", "TEXT", 1),
                    ("ordinal", "INTEGER", 2),
                    ("payload", "TEXT", 0),
                ],
            ),
            (
                "builtin_certificate",
                vec![("key", "TEXT", 1), ("value", "TEXT", 0)],
            ),
        ],
        Kind::Catalog => vec![
            (
                "metadata_symbols",
                vec![
                    ("full_name", "TEXT", 1),
                    ("symbol_id", "TEXT", 2),
                    ("symbol", "TEXT", 0),
                ],
            ),
            (
                "metadata_state",
                vec![("key", "TEXT", 1), ("value", "TEXT", 0)],
            ),
            (
                "metadata_inputs",
                vec![
                    ("path", "TEXT", 1),
                    ("stamp", "TEXT", 0),
                    ("len", "INTEGER", 0),
                    ("sha256", "TEXT", 0),
                ],
            ),
            (
                "metadata_artifacts",
                vec![("path", "TEXT", 1), ("sha256", "TEXT", 0)],
            ),
            (
                "metadata_producers",
                vec![("key", "TEXT", 1), ("value", "TEXT", 0)],
            ),
        ],
    };
    tables.push((
        "prepared_certificate",
        vec![("key", "TEXT", 1), ("value", "TEXT", 0)],
    ));
    tables
}
fn schema(db: &Connection, kind: Kind) -> Result<()> {
    let expected = tables(kind);
    let names = expected.iter().map(|(n, _)| *n).collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    let mut statement =
        db.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name")?;
    for row in statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })? {
        let (kind, name, owner, sql) = row?;
        if kind == "index" {
            anyhow::ensure!(
                names.contains(owner.as_str())
                    && name == format!("sqlite_autoindex_{owner}_1")
                    && sql.is_none(),
                "Unexpected prepared index"
            );
            continue;
        }
        anyhow::ensure!(
            kind == "table" && names.contains(name.as_str()) && name == owner,
            "Unexpected prepared schema object: {name}"
        );
        let sql = sql
            .context("Prepared table has no SQL")?
            .to_ascii_uppercase();
        anyhow::ensure!(
            sql.starts_with("CREATE TABLE ")
                && !sql.contains(" CHECK")
                && !sql.contains(" COLLATE")
                && !sql.contains(" REFERENCES")
                && !sql.contains(" GENERATED"),
            "Unsupported prepared table SQL"
        );
        actual.insert(name);
    }
    anyhow::ensure!(
        actual == names.iter().map(|v| v.to_string()).collect(),
        "Prepared database has missing tables"
    );
    for (table, columns) in expected {
        let mut pragma = db.prepare(&format!("PRAGMA table_xinfo({table})"))?;
        let rows = pragma
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        anyhow::ensure!(
            rows.len() == columns.len(),
            "Prepared table column count mismatch"
        );
        for ((name, ty, default, pk, hidden), (expected_name, expected_ty, expected_pk)) in
            rows.into_iter().zip(columns)
        {
            anyhow::ensure!(
                name == expected_name
                    && ty == expected_ty
                    && pk == expected_pk
                    && hidden == 0
                    && default.is_none(),
                "Prepared table column contract mismatch"
            );
        }
        anyhow::ensure!(
            !db.prepare(&format!("PRAGMA foreign_key_list({table})"))?
                .query([])?
                .next()?
                .is_some(),
            "Unexpected prepared foreign key"
        );
    }
    Ok(())
}
fn binding(descriptor: &Descriptor) -> Value {
    json!({"artifactKind":descriptor.artifact_kind,"builderFingerprint":descriptor.builder_fingerprint,"producerPolicy":descriptor.producer_policy,"sourceSha256":descriptor.source_sha256,"certificate":descriptor.certificate})
}
fn semantic_digest(db: &Connection, descriptor: &Descriptor) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&binding(descriptor))?);
    for (table, columns) in tables(descriptor.artifact_kind) {
        if table == "prepared_certificate" {
            continue;
        }
        let selected = columns
            .iter()
            .filter(|(name, _, _)| table != "metadata_inputs" || !matches!(*name, "stamp" | "len"))
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>();
        let sql = format!(
            "SELECT {} FROM {table} ORDER BY {}",
            selected.join(","),
            selected.join(",")
        );
        let mut query = db.prepare(&sql)?;
        let mut rows = query.query([])?;
        hash.update(table.as_bytes());
        hash.update([0]);
        while let Some(row) = rows.next()? {
            crate::cancellation::check()?;
            for i in 0..selected.len() {
                let value = row.get_ref(i)?;
                match value {
                    rusqlite::types::ValueRef::Text(bytes) => {
                        anyhow::ensure!(
                            bytes.len() <= 2 * 1024 * 1024,
                            "Prepared SQL row exceeds bound"
                        );
                        hash.update([1]);
                        hash.update((bytes.len() as u64).to_le_bytes());
                        hash.update(bytes);
                    }
                    rusqlite::types::ValueRef::Integer(n) => {
                        hash.update([2]);
                        hash.update(n.to_le_bytes());
                    }
                    _ => anyhow::bail!("Unexpected prepared row value"),
                }
            }
            hash.update([0]);
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub struct PreparedDatabase {
    pub db: Connection,
    pub descriptor: Descriptor,
    pub descriptor_bytes: u64,
    path: PathBuf,
    database_path: PathBuf,
    descriptor_identity: Identity,
    database_identity: Identity,
    #[cfg_attr(unix, allow(dead_code))]
    descriptor_sha256: String,
}
impl PreparedDatabase {
    pub fn open(path: &str, kind: Kind) -> Result<Self> {
        anyhow::ensure!(
            is_prepared_path(path),
            "Prepared descriptor must use .fnmeta suffix"
        );
        // Keep the configured path as a freshness witness: canonicalizing it
        // away would miss later retargeting of a descriptor symlink.
        let path = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            std::env::current_dir()?.join(path)
        };
        let resolved_descriptor = fs::canonicalize(&path)?;
        let before = identity(&path)?;
        anyhow::ensure!(
            before.bytes <= DESCRIPTOR_LIMIT,
            "Prepared descriptor exceeds bound"
        );
        let mut bytes = Vec::new();
        regular(&path)?
            .take(DESCRIPTOR_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() as u64 <= DESCRIPTOR_LIMIT,
            "Prepared descriptor exceeds bound"
        );
        let descriptor: Descriptor = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            descriptor.kind == FORMAT
                && descriptor.schema_version == 1
                && descriptor.artifact_kind == kind
                && descriptor.builder_fingerprint == BUILDER
                && descriptor.producer_policy == crate::producer_contract::policy_fingerprint(),
            "Prepared metadata format/builder/policy is incompatible; prepare again"
        );
        anyhow::ensure!(
            hex(&descriptor.source_sha256)
                && hex(&descriptor.semantic_sha256)
                && hex(&descriptor.database_sha256)
                && hex(&descriptor.derivation_key),
            "Prepared descriptor contains an invalid digest"
        );
        anyhow::ensure!(
            descriptor.derivation_key == hash_json(&binding(&descriptor))?,
            "Prepared derivation certificate mismatch"
        );
        anyhow::ensure!(
            descriptor.database_file == format!("{}.sqlite", descriptor.database_sha256),
            "Prepared database filename is not its content digest"
        );
        let database_path = resolved_descriptor
            .parent()
            .context("Prepared descriptor parent")?
            .join(&descriptor.database_file);
        standalone(&database_path)?;
        let database_identity = identity(&database_path)?;
        anyhow::ensure!(
            database_identity.bytes == descriptor.database_bytes
                && descriptor.database_bytes <= DATABASE_LIMIT,
            "Prepared DB byte bound/size mismatch"
        );
        anyhow::ensure!(
            digest(&database_path, DATABASE_LIMIT)? == descriptor.database_sha256,
            "Prepared database digest mismatch"
        );
        let db = Connection::open_with_flags(
            &database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON; PRAGMA cache_size=-8192; PRAGMA mmap_size=0;")?;
        schema(&db, kind)?;
        let saved: String = db.query_row(
            "SELECT value FROM prepared_certificate WHERE key='binding'",
            [],
            |r| r.get(0),
        )?;
        anyhow::ensure!(
            serde_json::from_str::<Value>(&saved)? == binding(&descriptor),
            "Prepared DB certificate mismatch"
        );
        let semantic: String = db.query_row(
            "SELECT value FROM prepared_certificate WHERE key='semanticSha256'",
            [],
            |r| r.get(0),
        )?;
        anyhow::ensure!(
            semantic == descriptor.semantic_sha256,
            "Prepared semantic digest mismatch"
        );
        anyhow::ensure!(
            identity(&path)? == before && identity(&database_path)? == database_identity,
            "Prepared artifact changed while opening"
        );
        standalone(&database_path)?;
        Ok(Self {
            db,
            descriptor,
            descriptor_bytes: before.bytes,
            path,
            database_path,
            descriptor_identity: before,
            database_identity,
            descriptor_sha256: format!("{:x}", Sha256::digest(bytes)),
        })
    }
    pub fn valid(&self) -> Result<bool> {
        crate::cancellation::check()?;
        if identity(&self.path).ok().as_ref() != Some(&self.descriptor_identity)
            || identity(&self.database_path).ok().as_ref() != Some(&self.database_identity)
            || standalone(&self.database_path).is_err()
        {
            return Ok(false);
        }
        #[cfg(not(unix))]
        if digest(&self.path, DESCRIPTOR_LIMIT)? != self.descriptor_sha256
            || digest(&self.database_path, DATABASE_LIMIT)? != self.descriptor.database_sha256
        {
            return Ok(false);
        }
        Ok(true)
    }
}
pub fn publish(
    source: &str,
    out: &str,
    kind: Kind,
    build: impl FnOnce(&str, &mut Connection) -> Result<Value>,
) -> Result<Value> {
    anyhow::ensure!(
        is_prepared_path(out),
        "Prepared descriptor must use .fnmeta suffix"
    );
    let output = Path::new(out);
    let parent = output
        .parent()
        .filter(|v| !v.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    anyhow::ensure!(
        !output.try_exists()?,
        "Prepared output already exists; choose a new generation"
    );
    let stage = parent.join(format!(
        ".fnmeta-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&stage)?;
    struct Clean(PathBuf);
    impl Drop for Clean {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _clean = Clean(stage.clone());
    let database = stage.join("index.sqlite");
    let source_hash = digest(Path::new(source), 1024 * 1024 * 1024)?;
    let mut db = Connection::open(&database)?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF; PRAGMA cache_size=-8192; PRAGMA mmap_size=0; PRAGMA temp_store=FILE;")?;
    let certificate = build(source, &mut db)?;
    anyhow::ensure!(
        serde_json::to_vec(&certificate)?.len() <= 1024 * 1024,
        "Prepared certificate exceeds1MiB"
    );
    anyhow::ensure!(
        digest(Path::new(source), 1024 * 1024 * 1024)? == source_hash,
        "Source changed during preparation"
    );
    let mut descriptor = Descriptor {
        kind: FORMAT.into(),
        schema_version: 1,
        artifact_kind: kind,
        builder_fingerprint: BUILDER.into(),
        producer_policy: crate::producer_contract::policy_fingerprint(),
        source_sha256: source_hash,
        certificate,
        semantic_sha256: String::new(),
        derivation_key: String::new(),
        database_file: String::new(),
        database_sha256: String::new(),
        database_bytes: 0,
    };
    db.execute_batch("CREATE TABLE prepared_certificate(key TEXT PRIMARY KEY,value TEXT NOT NULL) WITHOUT ROWID;")?;
    schema(&db, kind)?;
    descriptor.derivation_key = hash_json(&binding(&descriptor))?;
    descriptor.semantic_sha256 = semantic_digest(&db, &descriptor)?;
    db.execute(
        "INSERT INTO prepared_certificate VALUES('binding',?1)",
        [serde_json::to_string(&binding(&descriptor))?],
    )?;
    db.execute(
        "INSERT INTO prepared_certificate VALUES('semanticSha256',?1)",
        [&descriptor.semantic_sha256],
    )?;
    let integrity: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    anyhow::ensure!(integrity == "ok", "Prepared DB integrity check failed");
    db.close().map_err(|(_, e)| e)?;
    standalone(&database)?;
    File::open(&database)?.sync_all()?;
    descriptor.database_bytes = identity(&database)?.bytes;
    descriptor.database_sha256 = digest(&database, DATABASE_LIMIT)?;
    descriptor.database_file = format!("{}.sqlite", descriptor.database_sha256);
    let target = parent.join(&descriptor.database_file);
    match fs::hard_link(&database, &target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            anyhow::ensure!(
                digest(&target, DATABASE_LIMIT)? == descriptor.database_sha256,
                "Existing digest-named DB differs"
            );
            standalone(&target)?;
        }
        Err(e) => return Err(e.into()),
    }
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    let temporary = stage.join("descriptor.fnmeta");
    let bytes = serde_json::to_vec_pretty(&descriptor)?;
    anyhow::ensure!(
        bytes.len() as u64 <= DESCRIPTOR_LIMIT,
        "Prepared descriptor exceeds bound"
    );
    let mut file = File::create(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    // A successful importer check may predate SQLite finalization and hashing.
    // Recheck the actual live dependency roles immediately before publication.
    validate_live_inputs(&descriptor.certificate)?;
    crate::cancellation::check()?;
    // Hard-link publication is atomic and exclusive. A competing publisher
    // cannot replace a live generation; unreferenced digest files are harmless.
    fs::hard_link(&temporary, output)
        .context("Prepared descriptor publication failed (generation may already exist)")?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(
        json!({"outputPath":output,"databasePath":target,"databaseBytes":descriptor.database_bytes,"databaseSha256":descriptor.database_sha256,"semanticSha256":descriptor.semantic_sha256,"derivationKey":descriptor.derivation_key,"sourceSha256":descriptor.source_sha256,"preparation":"explicit native index build; runtime still validates actual DB and live dependencies"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "prepared-metadata-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn build(source: &str, db: &mut Connection) -> Result<Value> {
        for name in ["builtin_names", "builtin_scope_names", "builtin_term_names"] {
            db.execute_batch(&format!("CREATE TABLE {name}(name TEXT NOT NULL,ordinal INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(name,ordinal)) WITHOUT ROWID;"))?;
        }
        db.execute_batch("CREATE TABLE builtin_certificate(key TEXT PRIMARY KEY,value TEXT NOT NULL) WITHOUT ROWID;INSERT INTO builtin_names VALUES('Present',0,'{\"identity\":\"one\"}');")?;
        let dependency = Path::new(source).with_extension("dll");
        fs::write(&dependency, b"live facts")?;
        Ok(
            json!({"scope":"container-only-test; semantic consumer independently validates producer and inputs","workspaceAssemblies":[],"inputHashes":[{"path":dependency,"sha256":digest(&dependency,DATABASE_LIMIT)?}]}),
        )
    }
    fn fixture() -> (Temp, PathBuf, PathBuf) {
        let tmp = Temp::new();
        let source = tmp.0.join("source.json");
        fs::write(&source, b"source artifact").unwrap();
        let out = tmp.0.join("environment.fnmeta");
        (tmp, source, out)
    }
    #[test]
    fn read_only_publication_keeps_source_as_provenance_and_rejects_mutation() {
        let (_tmp, source, out) = fixture();
        let result = publish(
            source.to_str().unwrap(),
            out.to_str().unwrap(),
            Kind::Environment,
            build,
        )
        .unwrap();
        fs::remove_file(source).unwrap();
        let opened = PreparedDatabase::open(out.to_str().unwrap(), Kind::Environment).unwrap();
        assert!(opened.valid().unwrap());
        assert!(opened.db.execute("DELETE FROM builtin_names", []).is_err());
        assert_eq!(
            opened
                .db
                .query_row("SELECT count(*) FROM builtin_names", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let database = PathBuf::from(result["databasePath"].as_str().unwrap());
        let original = fs::read(&database).unwrap();
        let time = fs::metadata(&database).unwrap().modified().unwrap();
        let mut changed = original.clone();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        fs::write(&database, changed).unwrap();
        File::options()
            .write(true)
            .open(&database)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
        assert!(!opened.valid().unwrap());
        assert!(PreparedDatabase::open(out.to_str().unwrap(), Kind::Environment).is_err());
        fs::write(&database, &original).unwrap();
        let reopened = PreparedDatabase::open(out.to_str().unwrap(), Kind::Environment).unwrap();
        assert!(reopened.valid().unwrap());
        let wal = PathBuf::from(format!("{}-wal", database.display()));
        fs::write(&wal, b"").unwrap();
        assert!(!reopened.valid().unwrap());
        assert!(PreparedDatabase::open(out.to_str().unwrap(), Kind::Environment).is_err());
        fs::remove_file(wal).unwrap();
        let writable = Connection::open(&database).unwrap();
        writable.execute("DELETE FROM builtin_names", []).unwrap();
        drop(writable);
        assert!(PreparedDatabase::open(out.to_str().unwrap(), Kind::Environment).is_err());
        fs::write(&database, &original[..100]).unwrap();
        assert!(PreparedDatabase::open(out.to_str().unwrap(), Kind::Environment).is_err());
    }
    #[test]
    fn failed_cancelled_or_competing_build_never_publishes_partial_generation() {
        let (tmp, source, out) = fixture();
        assert!(publish(
            source.to_str().unwrap(),
            out.to_str().unwrap(),
            Kind::Environment,
            |s, db| {
                build(s, db)?;
                anyhow::bail!("injected build failure")
            }
        )
        .is_err());
        assert!(!out.exists());
        assert!(publish(
            source.to_str().unwrap(),
            out.to_str().unwrap(),
            Kind::Environment,
            |s, db| {
                let certificate = build(s, db)?;
                fs::write(Path::new(s).with_extension("dll"), b"changed facts")?;
                Ok(certificate)
            }
        )
        .is_err());
        assert!(!out.exists());
        let token = crate::cancellation::CancellationToken::new();
        token.cancel();
        {
            let _guard = token.enter();
            assert!(publish(
                source.to_str().unwrap(),
                out.to_str().unwrap(),
                Kind::Environment,
                build
            )
            .is_err());
        }
        assert!(!out.exists());
        publish(
            source.to_str().unwrap(),
            out.to_str().unwrap(),
            Kind::Environment,
            build,
        )
        .unwrap();
        let before = fs::read(&out).unwrap();
        assert!(publish(
            source.to_str().unwrap(),
            out.to_str().unwrap(),
            Kind::Environment,
            build
        )
        .is_err());
        assert_eq!(fs::read(out).unwrap(), before);
        assert!(!fs::read_dir(&tmp.0).unwrap().any(|p| p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".fnmeta-")));
    }
    #[test]
    fn ownership_policy_and_exact_schema_are_part_of_the_container_contract() {
        let (tmp, source, out) = fixture();
        let first = publish(
            source.to_str().unwrap(),
            out.to_str().unwrap(),
            Kind::Environment,
            build,
        )
        .unwrap();
        let second = publish(
            source.to_str().unwrap(),
            tmp.0.join("other.fnmeta").to_str().unwrap(),
            Kind::Environment,
            |s, db| {
                let mut header = build(s, db)?;
                header["workspaceAssemblies"] = json!(["owned.dll"]);
                Ok(header)
            },
        )
        .unwrap();
        assert_ne!(first["derivationKey"], second["derivationKey"]);
        assert_ne!(first["semanticSha256"], second["semanticSha256"]);
        #[cfg(unix)]
        {
            let link = tmp.0.join("selected.fnmeta");
            std::os::unix::fs::symlink(&out, &link).unwrap();
            let selected =
                PreparedDatabase::open(link.to_str().unwrap(), Kind::Environment).unwrap();
            assert!(selected.valid().unwrap());
            fs::remove_file(&link).unwrap();
            std::os::unix::fs::symlink(tmp.0.join("other.fnmeta"), &link).unwrap();
            assert!(!selected.valid().unwrap());
            assert!(
                PreparedDatabase::open(link.to_str().unwrap(), Kind::Environment)
                    .unwrap()
                    .valid()
                    .unwrap()
            );
        }
        let mut descriptor: Value = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
        descriptor["producerPolicy"] = json!("revoked-policy");
        fs::write(&out, serde_json::to_vec(&descriptor).unwrap()).unwrap();
        assert!(PreparedDatabase::open(out.to_str().unwrap(), Kind::Environment).is_err());
        for (name,sql) in [("view","CREATE VIEW forbidden AS SELECT * FROM builtin_names"),("trigger","CREATE TRIGGER forbidden AFTER INSERT ON builtin_names BEGIN DELETE FROM builtin_names; END"),("extra","CREATE TABLE extra(x TEXT)")]{let output=tmp.0.join(format!("{name}.fnmeta"));assert!(publish(source.to_str().unwrap(),output.to_str().unwrap(),Kind::Environment,|s,db|{let header=build(s,db)?;db.execute_batch(sql)?;Ok(header)}).is_err());assert!(!output.exists());}
    }
}
