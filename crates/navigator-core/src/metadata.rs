//! Stream explicitly exported declaration catalogs into the existing disk index.
//! No complete catalog or assembly metadata graph is retained in this process.
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::{DeserializeSeed, Error, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

fn stamp(path: &str) -> Result<(String, u64)> {
    let metadata = fs::metadata(path)?;
    let modified = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos();
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec()
        )
    };
    #[cfg(not(unix))]
    let identity = String::new();
    Ok((format!("{modified}:{identity}"), metadata.len()))
}
fn input_stamp(path: &str, expected: &str) -> Result<(String, u64)> {
    match fs::metadata(path) {
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) && expected == "missing" =>
        {
            Ok(("missing".into(), 0))
        }
        Err(error) => Err(error.into()),
        Ok(metadata) if expected == "directory" && metadata.is_dir() => Ok(("directory".into(), 0)),
        Ok(metadata) if expected != "directory" && expected != "missing" && metadata.is_file() => {
            stamp(path)
        }
        Ok(_) => bail!("Catalog dependency kind changed or is unsupported: {path}"),
    }
}
fn open_regular(path: &Path) -> Result<File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "Catalog input is not a regular file: {}",
        path.display()
    );
    Ok(file)
}

struct HashReader<R> {
    reader: R,
    hash: Sha256,
}
impl<R: Read> Read for HashReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        crate::cancellation::check().map_err(std::io::Error::other)?;
        let count = self.reader.read(bytes)?;
        self.hash.update(&bytes[..count]);
        Ok(count)
    }
}

fn hash_file(path: &str) -> Result<String> {
    let mut reader = HashReader {
        reader: open_regular(Path::new(path))?,
        hash: Sha256::new(),
    };
    std::io::copy(&mut reader, &mut std::io::sink())?;
    Ok(format!("{:x}", reader.hash.finalize()))
}

fn state(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM metadata_state WHERE key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()?)
}

/// Checks captured stat identities, not every assembly's bytes, on reuse.
/// The caller should invoke this only at its invalidation boundary.
pub fn valid(conn: &Connection) -> Result<bool> {
    let initialized: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='metadata_state')",
        [],
        |row| row.get(0),
    )?;
    if !initialized {
        return Ok(false);
    }
    if state(conn, "completeness")?.as_deref() != Some("exact") {
        return Ok(false);
    }
    if state(conn, "producerPolicy")?.as_deref()
        != Some(crate::producer_contract::policy_fingerprint().as_str())
    {
        return Ok(false);
    }
    let mut query = conn.prepare("SELECT path,stamp,len,sha256 FROM metadata_inputs")?;
    for row in query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, u64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })? {
        crate::cancellation::check()?;
        let (path, previous, len, expected) = row?;
        if input_stamp(&path, &expected).ok() != Some((previous, len)) {
            return Ok(false);
        }
    }
    // Non-Unix targets do not expose a comparable change-time certificate.
    // Keep correctness by hashing rather than assuming preserved mtime is safe.
    #[cfg(not(unix))]
    if !verify_hashes(conn)? {
        return Ok(false);
    }
    Ok(true)
}

/// A catalog is loaded lazily once per Workspace. Reusing a persisted database
/// across sessions must verify bytes, not trust an old filesystem observation.
fn verify_hashes(conn: &Connection) -> Result<bool> {
    let mut query = conn.prepare("SELECT path,sha256 FROM metadata_inputs")?;
    for row in query.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        crate::cancellation::check()?;
        let (path, expected) = row?;
        let before = input_stamp(&path, &expected)?;
        if expected == "missing" || expected == "directory" {
            continue;
        }
        if !hash_file(&path)?.eq_ignore_ascii_case(&expected)
            || input_stamp(&path, &expected)? != before
        {
            return Ok(false);
        }
    }
    Ok(true)
}

struct Importer<'a> {
    conn: &'a Connection,
    active: HashSet<PathBuf>,
    files: HashSet<PathBuf>,
}
impl Importer<'_> {
    fn input(&mut self, path: &str, expected: &str) -> Result<()> {
        crate::cancellation::check()?;
        anyhow::ensure!(
            expected == "missing"
                || expected == "directory"
                || (expected.len() == 64 && expected.bytes().all(|byte| byte.is_ascii_hexdigit())),
            "Catalog dependency hash/kind is unsupported"
        );
        let before = input_stamp(path, expected)?;
        let previous: Option<(String, u64, String)> = self
            .conn
            .query_row(
                "SELECT stamp,len,sha256 FROM metadata_inputs WHERE path=?1",
                [path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((stamp, len, hash)) = previous {
            anyhow::ensure!(
                hash.eq_ignore_ascii_case(expected),
                "Catalog shards disagree about input {path}"
            );
            anyhow::ensure!(
                (stamp, len) == before,
                "Catalog input changed during shard import: {path}"
            );
            return Ok(());
        }
        let actual = if expected == "missing" || expected == "directory" {
            expected.to_string()
        } else {
            hash_file(path)?
        };
        anyhow::ensure!(
            actual.eq_ignore_ascii_case(expected),
            "Catalog input SHA256 changed: {path}"
        );
        anyhow::ensure!(
            input_stamp(path, expected)? == before,
            "Catalog input changed while importing: {path}"
        );
        self.remember(path, &before, &actual)
    }
    fn remember(&self, path: &str, value: &(String, u64), hash: &str) -> Result<()> {
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT sha256 FROM metadata_inputs WHERE path=?1",
                [path],
                |row| row.get(0),
            )
            .optional()?;
        anyhow::ensure!(
            existing.as_deref().is_none_or(|old| old == hash),
            "Catalog shards disagree about input {path}"
        );
        self.conn.execute(
            "INSERT OR REPLACE INTO metadata_inputs(path,stamp,len,sha256) VALUES(?1,?2,?3,?4)",
            params![path, value.0, value.1, hash],
        )?;
        Ok(())
    }
    fn load(&mut self, path: &Path) -> Result<()> {
        let path =
            fs::canonicalize(path).with_context(|| format!("Open catalog {}", path.display()))?;
        anyhow::ensure!(
            self.active.len() < 8,
            "Catalog manifest nesting exceeds eight levels"
        );
        anyhow::ensure!(
            !self.active.contains(&path),
            "Catalog manifest cycle: {}",
            path.display()
        );
        if self.files.contains(&path) {
            return Ok(());
        }
        self.active.insert(path.clone());
        let name = path.to_str().context("Catalog path is not UTF-8")?;
        let before = stamp(name)?;
        let reader = HashReader {
            reader: open_regular(&path)?,
            hash: Sha256::new(),
        };
        let mut reader = BufReader::with_capacity(64 * 1024, reader);
        let mut decoder = serde_json::Deserializer::from_reader(&mut reader);
        CatalogSeed {
            importer: self,
            directory: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        }
        .deserialize(&mut decoder)?;
        decoder.end()?;
        let hash = format!("{:x}", reader.into_inner().hash.finalize());
        anyhow::ensure!(
            stamp(name)? == before,
            "Catalog changed while importing: {name}"
        );
        self.remember(name, &before, &hash)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO metadata_artifacts(path,sha256) VALUES(?1,?2)",
            params![name, hash],
        )?;
        self.active.remove(&path);
        self.files.insert(path);
        Ok(())
    }
}

struct SymbolSeed<'a> {
    conn: &'a Connection,
}
impl<'de> DeserializeSeed<'de> for SymbolSeed<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<(), D::Error> {
        struct Symbols<'a>(&'a Connection);
        impl<'de> Visitor<'de> for Symbols<'_> {
            type Value = ();
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a streamed array of declaration symbols")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> std::result::Result<(), A::Error> {
                let mut insert=self.0.prepare("INSERT OR IGNORE INTO metadata_symbols(full_name,symbol_id,symbol) VALUES(?1,?2,?3)").map_err(A::Error::custom)?;
                while let Some(symbol) = sequence.next_element::<Value>()? {
                    crate::cancellation::check().map_err(A::Error::custom)?;
                    let serialized = serde_json::to_string(&symbol).map_err(A::Error::custom)?;
                    let identifier = symbol
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{:x}", Sha256::digest(serialized.as_bytes())));
                    let mut names = HashSet::new();
                    if let Some(name) = symbol.get("fullName").and_then(Value::as_str) {
                        names.insert(name);
                    }
                    for name in symbol
                        .get("qualifiedNames")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        names.insert(name);
                    }
                    for name in names {
                        if !name.is_empty() {
                            insert
                                .execute(params![name, identifier, serialized])
                                .map_err(A::Error::custom)?;
                        }
                    }
                }
                Ok(())
            }
        }
        decoder.deserialize_seq(Symbols(self.conn))
    }
}

#[derive(Deserialize)]
struct Input {
    path: String,
    sha256: String,
}
struct CatalogSeed<'a, 'b> {
    importer: &'a mut Importer<'b>,
    directory: PathBuf,
}
impl<'de> Visitor<'de> for CatalogSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an exact declaration catalog or shard manifest")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        let mut completeness = None;
        let mut schema = None;
        let mut facts_version = None;
        let mut certificate_version = None;
        let mut producer = None;
        let mut input_count = 0usize;
        let mut content = false;
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            crate::cancellation::check().map_err(A::Error::custom)?;
            if key.len() > 128 || seen.len() >= 64 || !seen.insert(key.clone()) {
                return Err(A::Error::custom("Invalid/duplicate catalog header key"));
            }
            match key.as_str() {
                "completeness" => completeness = Some(map.next_value::<String>()?),
                "schemaVersion" => schema = Some(map.next_value::<u64>()?),
                "factsVersion" => facts_version = Some(map.next_value::<u64>()?),
                "metadataCertificateVersion" => {
                    certificate_version = Some(map.next_value::<u64>()?)
                }
                "producer" => {
                    let value = map.next_value::<Value>()?;
                    if serde_json::to_vec(&value).map_err(A::Error::custom)?.len() > 16384 {
                        return Err(A::Error::custom(
                            "Catalog producer provenance exceeds its bound",
                        ));
                    }
                    producer = Some(value);
                }
                "symbols" => {
                    map.next_value_seed(SymbolSeed {
                        conn: self.importer.conn,
                    })?;
                    content = true;
                }
                "inputHashes" => {
                    for input in map.next_value::<Vec<Input>>()? {
                        input_count += 1;
                        if input_count > 10000 {
                            return Err(A::Error::custom(
                                "Catalog dependency count exceeds its bound",
                            ));
                        }
                        self.importer
                            .input(&input.path, &input.sha256)
                            .map_err(A::Error::custom)?;
                    }
                }
                "catalogs" => {
                    for path in map.next_value::<Vec<String>>()? {
                        self.importer
                            .load(&self.directory.join(path))
                            .map_err(A::Error::custom)?;
                    }
                    content = true;
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        if completeness.as_deref() != Some("exact")
            || schema != Some(2)
            || facts_version != Some(2)
            || !content
            || input_count == 0
        {
            return Err(A::Error::custom(
                "Catalog requires schemaVersion 2, factsVersion 2, exact completeness, live dependency hashes and symbols or catalogs; prepare again",
            ));
        }
        let certificate = serde_json::json!({"metadataCertificateVersion":certificate_version,"producer":producer});
        crate::producer_contract::validate(&certificate).map_err(A::Error::custom)?;
        let encoded = serde_json::to_string(&certificate).map_err(A::Error::custom)?;
        let key = format!("{:x}", Sha256::digest(encoded.as_bytes()));
        self.importer
            .conn
            .execute(
                "INSERT OR IGNORE INTO metadata_producers(key,value) VALUES(?1,?2)",
                params![key, encoded],
            )
            .map_err(A::Error::custom)?;
        Ok(())
    }
}
impl<'de> DeserializeSeed<'de> for CatalogSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<(), D::Error> {
        decoder.deserialize_map(self)
    }
}

/// Imports one symbol at a time. A small manifest may reference streamed shards.
/// Unchanged catalogs reuse the SQLite rows without opening or parsing JSON.
pub fn import(conn: &mut Connection, path: &str) -> Result<String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS metadata_symbols(full_name TEXT NOT NULL,symbol_id TEXT NOT NULL,symbol TEXT NOT NULL,PRIMARY KEY(full_name,symbol_id)); CREATE TABLE IF NOT EXISTS metadata_state(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS metadata_inputs(path TEXT PRIMARY KEY,stamp TEXT NOT NULL,len INTEGER NOT NULL,sha256 TEXT NOT NULL); CREATE TABLE IF NOT EXISTS metadata_artifacts(path TEXT PRIMARY KEY,sha256 TEXT NOT NULL); CREATE TABLE IF NOT EXISTS metadata_producers(key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
    let path = fs::canonicalize(path)?.to_string_lossy().to_string();
    if state(conn, "path")?.as_deref() == Some(&path) && valid(conn)? && verify_hashes(conn)? {
        if let Some(hash) = state(conn, "fingerprint")? {
            return Ok(hash);
        }
    }
    let transaction = conn.transaction()?;
    transaction.execute_batch(
        "DELETE FROM metadata_symbols; DELETE FROM metadata_state; DELETE FROM metadata_inputs; DELETE FROM metadata_artifacts; DELETE FROM metadata_producers;",
    )?;
    Importer {
        conn: &transaction,
        active: HashSet::new(),
        files: HashSet::new(),
    }
    .load(Path::new(&path))?;
    let mut fingerprint = Sha256::new();
    {
        let mut inputs =
            transaction.prepare("SELECT path,sha256 FROM metadata_inputs ORDER BY path")?;
        for row in inputs.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            crate::cancellation::check()?;
            let (path, hash) = row?;
            fingerprint.update(path.as_bytes());
            fingerprint.update([0]);
            fingerprint.update(hash.as_bytes());
            fingerprint.update([0]);
        }
    }
    let fingerprint = format!("{:x}", fingerprint.finalize());
    let producer_policy = crate::producer_contract::policy_fingerprint();
    for (key, value) in [
        ("path", path.as_str()),
        ("fingerprint", fingerprint.as_str()),
        ("completeness", "exact"),
        ("producerPolicy", producer_policy.as_str()),
    ] {
        transaction.execute(
            "INSERT INTO metadata_state(key,value) VALUES(?1,?2)",
            [key, value],
        )?;
    }
    if !valid(&transaction)? {
        bail!("Catalog inputs changed during import");
    }
    transaction.commit()?;
    Ok(fingerprint)
}

/// Build an explicit catalog index. Source artifact hashes remain provenance;
/// only actual binary/XML witnesses remain live after sealing the derived DB.
pub(crate) fn prepare_file(source: &str, out: &str) -> Result<Value> {
    crate::prepared::publish(source, out, crate::prepared::Kind::Catalog, |source, db| {
        let fingerprint = import(db, source)?;
        let source_artifacts = db
            .prepare("SELECT path,sha256 FROM metadata_artifacts ORDER BY path")?
            .query_map([], |r| {
                Ok(serde_json::json!({"path":r.get::<_,String>(0)?,"sha256":r.get::<_,String>(1)?}))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        anyhow::ensure!(
            !source_artifacts.is_empty(),
            "Prepared catalog has no source artifact certificate"
        );
        db.execute(
            "DELETE FROM metadata_inputs WHERE path IN (SELECT path FROM metadata_artifacts)",
            [],
        )?;
        let inputs = db
            .prepare("SELECT path,sha256 FROM metadata_inputs ORDER BY path")?
            .query_map([], |r| {
                Ok(serde_json::json!({"path":r.get::<_,String>(0)?,"sha256":r.get::<_,String>(1)?}))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let producers = db
            .prepare("SELECT value FROM metadata_producers ORDER BY key")?
            .query_map([], |r| r.get::<_, String>(0))?
            .map(|v| Ok(serde_json::from_str::<Value>(&v?)?))
            .collect::<Result<Vec<_>>>()?;
        anyhow::ensure!(
            !inputs.is_empty() && !producers.is_empty(),
            "Prepared catalog certificate is incomplete"
        );
        Ok(
            serde_json::json!({"kind":"library-catalog","fingerprint":fingerprint,"ownershipScope":"lookup-time-project-model","sourceArtifacts":source_artifacts,"inputHashes":inputs,"producers":producers}),
        )
    })
}

pub(crate) struct PreparedCatalog {
    pub database: crate::prepared::PreparedDatabase,
    pub fingerprint: String,
    pub dependency_bytes_hashed: u64,
    inputs: Vec<(String, String, (String, u64))>,
}
impl PreparedCatalog {
    pub fn load(path: &str) -> Result<Self> {
        let database =
            crate::prepared::PreparedDatabase::open(path, crate::prepared::Kind::Catalog)?;
        let header = &database.descriptor.certificate;
        anyhow::ensure!(
            header["kind"] == "library-catalog"
                && header["ownershipScope"] == "lookup-time-project-model",
            "Prepared catalog certificate scope mismatch"
        );
        let fingerprint = header["fingerprint"]
            .as_str()
            .context("Prepared catalog fingerprint missing")?
            .to_string();
        anyhow::ensure!(
            state(&database.db, "fingerprint")?.as_deref() == Some(&fingerprint)
                && state(&database.db, "producerPolicy")?.as_deref()
                    == Some(crate::producer_contract::policy_fingerprint().as_str()),
            "Prepared catalog stored policy/fingerprint mismatch"
        );
        let producers = header["producers"]
            .as_array()
            .context("Prepared catalog lacks producer contracts")?;
        anyhow::ensure!(
            !producers.is_empty(),
            "Prepared catalog lacks producer contracts"
        );
        for producer in producers {
            crate::producer_contract::validate(producer)?;
        }
        let certified = header["inputHashes"]
            .as_array()
            .context("Prepared catalog lacks dependencies")?;
        anyhow::ensure!(
            !certified.is_empty() && certified.len() <= 10000,
            "Prepared dependency count invalid"
        );
        let stored = database
            .db
            .prepare("SELECT path,sha256 FROM metadata_inputs ORDER BY path")?
            .query_map([], |r| {
                Ok(serde_json::json!({"path":r.get::<_,String>(0)?,"sha256":r.get::<_,String>(1)?}))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        anyhow::ensure!(
            *certified == stored,
            "Prepared dependency header/row mismatch"
        );
        let mut inputs = Vec::new();
        let mut dependency_bytes_hashed = 0;
        for input in certified {
            crate::cancellation::check()?;
            let path = input["path"]
                .as_str()
                .context("Missing prepared dependency path")?;
            let expected = input["sha256"]
                .as_str()
                .context("Missing prepared dependency digest")?;
            let before = input_stamp(path, expected)?;
            if expected != "missing" && expected != "directory" {
                dependency_bytes_hashed += before.1;
                anyhow::ensure!(
                    hash_file(path)? == expected,
                    "Prepared catalog dependency is stale: {path}"
                );
            }
            anyhow::ensure!(
                input_stamp(path, expected)? == before,
                "Prepared catalog dependency changed while validating"
            );
            inputs.push((path.to_string(), expected.to_string(), before));
        }
        anyhow::ensure!(
            database.valid()?,
            "Prepared database changed while validating dependencies"
        );
        Ok(Self {
            database,
            fingerprint,
            dependency_bytes_hashed,
            inputs,
        })
    }
    pub fn valid(&self) -> Result<bool> {
        if !self.database.valid()? {
            return Ok(false);
        }
        for (path, expected, previous) in &self.inputs {
            crate::cancellation::check()?;
            if input_stamp(path, expected).ok().as_ref() != Some(previous) {
                return Ok(false);
            }
            #[cfg(not(unix))]
            if expected != "missing" && expected != "directory" && hash_file(path)? != *expected {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

pub fn lookup(conn: &Connection, full_name: &str) -> Result<Vec<Value>> {
    let mut statement =
        conn.prepare("SELECT symbol FROM metadata_symbols WHERE full_name=?1 LIMIT 2")?;
    let mut symbols = Vec::new();
    for row in statement.query_map([full_name], |row| row.get::<_, String>(0))? {
        symbols.push(serde_json::from_str(&row?)?);
    }
    Ok(symbols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn certified(mut value: Value) -> Value {
        value["schemaVersion"] = json!(2);
        value["factsVersion"] = json!(2);
        for (key, field) in crate::producer_contract::certificate().as_object().unwrap() {
            value[key] = field.clone();
        }
        value
    }
    #[test]
    fn streamed_catalog_aliases_shards_reuse_and_invalidation() {
        let root = std::env::temp_dir().join(format!(
            "navigator-metadata-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let dependency = root.join("lib.dll");
        fs::write(&dependency, b"first").unwrap();
        let shard = root.join("shard.json");
        fs::write(&shard,certified(json!({"schemaVersion":1,"completeness":"exact","inputHashes":[{"path":dependency,"sha256":hash_file(dependency.to_str().unwrap()).unwrap()}],"symbols":[{"id":"one","fullName":"Library.value","qualifiedNames":["Library.value","Alias.value"],"documentation":"sample"}]})).to_string()).unwrap();
        let manifest = root.join("libraries.json");
        fs::write(
            &manifest,
            certified(json!({"schemaVersion":1,"completeness":"exact","inputHashes":[{"path":dependency,"sha256":hash_file(dependency.to_str().unwrap()).unwrap()}],"catalogs":["shard.json"]})).to_string(),
        )
        .unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        let fingerprint = import(&mut conn, manifest.to_str().unwrap()).unwrap();
        assert_eq!(lookup(&conn, "Library.value").unwrap().len(), 1);
        assert_eq!(lookup(&conn, "Alias.value").unwrap().len(), 1);
        assert_eq!(
            import(&mut conn, manifest.to_str().unwrap()).unwrap(),
            fingerprint
        );
        assert!(valid(&conn).unwrap());
        let original_time = fs::metadata(&shard).unwrap().modified().unwrap();
        let changed = fs::read_to_string(&shard)
            .unwrap()
            .replace("sample", "newdoc");
        fs::write(&shard, changed).unwrap();
        File::options()
            .write(true)
            .open(&shard)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(original_time))
            .unwrap();
        // Preserved size/mtime/inode must not reuse stale catalog text.
        assert!(!valid(&conn).unwrap());
        assert_ne!(
            import(&mut conn, manifest.to_str().unwrap()).unwrap(),
            fingerprint
        );
        assert_eq!(
            lookup(&conn, "Library.value").unwrap()[0]["documentation"],
            "newdoc"
        );
        let replacement = root.join("replacement.json");
        fs::write(&replacement,certified(json!({"schemaVersion":1,"completeness":"exact","inputHashes":[{"path":dependency,"sha256":hash_file(dependency.to_str().unwrap()).unwrap()}],"symbols":[{"id":"two","fullName":"Replacement.value"}]})).to_string()).unwrap();
        let token = crate::cancellation::CancellationToken::new();
        token.cancel();
        {
            let _scope = token.enter();
            assert!(import(&mut conn, replacement.to_str().unwrap()).is_err());
        }
        // Cancellation after starting replacement must roll back its deletes.
        assert_eq!(lookup(&conn, "Library.value").unwrap().len(), 1);
        assert!(lookup(&conn, "Replacement.value").unwrap().is_empty());
        let original_time = fs::metadata(&dependency).unwrap().modified().unwrap();
        fs::write(&dependency, b"other").unwrap();
        File::options()
            .write(true)
            .open(&dependency)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(original_time))
            .unwrap();
        assert!(!valid(&conn).unwrap());
        // Even if stored stat fields appear current, first-use validation must
        // compare actual bytes with the certified assembly hash.
        let (forged_stamp, forged_len) = stamp(dependency.to_str().unwrap()).unwrap();
        conn.execute(
            "UPDATE metadata_inputs SET stamp=?1,len=?2 WHERE path=?3",
            params![forged_stamp, forged_len, dependency.to_str().unwrap()],
        )
        .unwrap();
        assert!(import(&mut conn, manifest.to_str().unwrap()).is_err());
        assert_eq!(lookup(&conn, "Library.value").unwrap().len(), 1); // Failed import rolled back; caller still must reject invalid state.
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn producer_provenance_does_not_remove_selected_dual_role_dependency() {
        let root = std::env::temp_dir().join(format!(
            "catalog-roles-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let producer = root.join("Exporter.dll");
        let dependency = root.join("Library.dll");
        fs::write(&producer, b"producer bytes").unwrap();
        fs::write(&dependency, b"library bytes").unwrap();
        let producer_hash = hash_file(producer.to_str().unwrap()).unwrap();
        let mut value = certified(
            json!({"completeness":"exact","inputHashes":[{"path":dependency,"sha256":hash_file(dependency.to_str().unwrap()).unwrap()}],"symbols":[{"id":"one","fullName":"Library.value"}]}),
        );
        for role in value["producer"]["implementationHashes"]
            .as_array_mut()
            .unwrap()
        {
            if role["role"] == "exporter" {
                role["sha256"] = json!(producer_hash);
            }
        }
        let catalog = root.join("catalog.json");
        fs::write(&catalog, value.to_string()).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        import(&mut db, catalog.to_str().unwrap()).unwrap();
        fs::remove_file(&producer).unwrap();
        assert!(valid(&db).unwrap());
        import(&mut db, catalog.to_str().unwrap()).unwrap();
        fs::write(&producer, b"producer bytes").unwrap();
        value["inputHashes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"path":producer,"sha256":producer_hash}));
        fs::write(&catalog, value.to_string()).unwrap();
        import(&mut db, catalog.to_str().unwrap()).unwrap();
        fs::write(&producer, b"changed producer and library bytes").unwrap();
        assert!(!valid(&db).unwrap());
        assert!(import(&mut db, catalog.to_str().unwrap()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unknown_legacy_contracts_and_obsolete_cache_policy_never_reuse_facts() {
        let root = std::env::temp_dir().join(format!(
            "catalog-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let dependency = root.join("lib.dll");
        fs::write(&dependency, b"input").unwrap();
        let value = certified(
            json!({"completeness":"exact","inputHashes":[{"path":dependency,"sha256":hash_file(dependency.to_str().unwrap()).unwrap()}],"symbols":[{"id":"accepted","fullName":"Library.value"}]}),
        );
        let catalog = root.join("catalog.json");
        fs::write(&catalog, value.to_string()).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        import(&mut db, catalog.to_str().unwrap()).unwrap();
        db.execute(
            "UPDATE metadata_state SET value='obsolete-consumer-policy' WHERE key='producerPolicy'",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO metadata_symbols VALUES('Forged.absence','forged','{}')",
            [],
        )
        .unwrap();
        assert!(!valid(&db).unwrap());
        import(&mut db, catalog.to_str().unwrap()).unwrap();
        assert!(lookup(&db, "Forged.absence").unwrap().is_empty());
        for variant in 0..5 {
            let mut bad = value.clone();
            match variant {
                0 => {
                    bad.as_object_mut().unwrap().remove("producer");
                }
                1 => {
                    bad["producer"]["algorithmId"] = json!("unknown-future-algorithm");
                }
                2 => {
                    bad["factsVersion"] = json!(1);
                }
                3 => {
                    bad["inputHashes"] = json!([]);
                }
                _ => {
                    bad["metadataCertificateVersion"] = json!(2);
                }
            }
            let replacement = root.join(format!("bad-{variant}.json"));
            fs::write(&replacement, bad.to_string()).unwrap();
            assert!(
                import(&mut db, replacement.to_str().unwrap()).is_err(),
                "variant {variant}"
            );
            assert_eq!(lookup(&db, "Library.value").unwrap().len(), 1);
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn prepared_catalog_preserves_rows_without_live_source_json_but_keeps_inputs() {
        let root = std::env::temp_dir().join(format!(
            "prepared-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let library = root.join("Library.dll");
        fs::write(&library, b"library").unwrap();
        let source = root.join("catalog.json");
        let catalog = certified(
            json!({"completeness":"exact","inputHashes":[{"path":library,"sha256":hash_file(library.to_str().unwrap()).unwrap()}],"symbols":[{"id":"one","fullName":"Library.value","qualifiedNames":["Alias.value"],"documentation":"FIRST"}]}),
        );
        fs::write(&source, catalog.to_string()).unwrap();
        let mut original = Connection::open_in_memory().unwrap();
        import(&mut original, source.to_str().unwrap()).unwrap();
        let output = root.join("catalog.fnmeta");
        prepare_file(source.to_str().unwrap(), output.to_str().unwrap()).unwrap();
        let prepared = PreparedCatalog::load(output.to_str().unwrap()).unwrap();
        assert!(prepared.valid().unwrap());
        for name in ["Library.value", "Alias.value", "Absent"] {
            assert_eq!(
                lookup(&original, name).unwrap(),
                lookup(&prepared.database.db, name).unwrap()
            );
        }
        assert!(prepared
            .database
            .db
            .execute("DELETE FROM metadata_symbols", [])
            .is_err());
        fs::remove_file(source).unwrap();
        assert!(prepared.valid().unwrap());
        assert!(PreparedCatalog::load(output.to_str().unwrap()).is_ok());
        let time = fs::metadata(&library).unwrap().modified().unwrap();
        fs::write(&library, b"changed").unwrap();
        File::options()
            .write(true)
            .open(&library)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
        assert!(!prepared.valid().unwrap());
        assert!(PreparedCatalog::load(output.to_str().unwrap()).is_err());
        fs::write(&library, b"library").unwrap();
        assert!(PreparedCatalog::load(output.to_str().unwrap())
            .unwrap()
            .valid()
            .unwrap());
        drop(prepared);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "requires genuine semantic/tests/metadata_provenance.py fixture via FSHARP_METADATA_PROVENANCE_FIXTURE"]
    fn actual_export_survives_producer_removal_but_dual_role_rejects() {
        let root = PathBuf::from(
            std::env::var("FSHARP_METADATA_PROVENANCE_FIXTURE")
                .expect("actual provenance fixture path"),
        );
        let catalog = root.join("catalog.json");
        let dual_catalog = root.join("dual-role-catalog.json");
        let mut ordinary = Connection::open_in_memory().unwrap();
        let mut dual = Connection::open_in_memory().unwrap();
        import(&mut ordinary, catalog.to_str().unwrap()).unwrap();
        import(&mut dual, dual_catalog.to_str().unwrap()).unwrap();
        assert_eq!(
            lookup(&ordinary, "Provenance.Library.keep").unwrap().len(),
            1
        );
        assert_eq!(
            lookup(&dual, "Provenance.DualRole.keepSymbol")
                .unwrap()
                .len(),
            1
        );
        struct Restore(Vec<(PathBuf, PathBuf)>);
        impl Drop for Restore {
            fn drop(&mut self) {
                for (from, to) in &self.0 {
                    if to.exists() {
                        let _ = fs::rename(to, from);
                    }
                }
            }
        }
        let mut restore = Restore(vec![]);
        for name in [
            "FSharpNavigator.Semantic.dll",
            "FSharp.Compiler.Service.dll",
        ] {
            let from = root.join("producer").join(name);
            let to = from.with_extension("dll.absent");
            fs::rename(&from, &to).unwrap();
            restore.0.push((from, to));
        }
        assert!(valid(&ordinary).unwrap());
        import(&mut ordinary, catalog.to_str().unwrap()).unwrap();
        let mut fresh = Connection::open_in_memory().unwrap();
        import(&mut fresh, catalog.to_str().unwrap()).unwrap();
        assert_eq!(lookup(&fresh, "Provenance.Library.keep").unwrap().len(), 1);
        assert!(!valid(&dual).unwrap());
        assert!(import(&mut dual, dual_catalog.to_str().unwrap()).is_err());
        let mut fresh_dual = Connection::open_in_memory().unwrap();
        assert!(import(&mut fresh_dual, dual_catalog.to_str().unwrap()).is_err());
        drop(restore);
        import(&mut fresh_dual, dual_catalog.to_str().unwrap()).unwrap();
    }

    #[test]
    #[ignore = "requires genuine semantic/tests/xml_selection.py fixtures via FSHARP_XML_SELECTION_FIXTURE"]
    fn actual_xml_priority_cached_reopened_and_fresh_imports() {
        let root =
            PathBuf::from(std::env::var("FSHARP_XML_SELECTION_FIXTURE").expect("XML fixture"));
        let manifest: Value =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        let symbol = manifest["symbol"].as_str().unwrap();
        enum Saved {
            Missing,
            Directory,
            File(Vec<u8>, std::time::SystemTime),
        }
        struct Restore(Vec<(PathBuf, Saved)>);
        fn remove(path: &Path) {
            if let Ok(meta) = fs::symlink_metadata(path) {
                if meta.is_dir() {
                    fs::remove_dir_all(path).unwrap();
                } else {
                    fs::remove_file(path).unwrap();
                }
            }
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                for (path, state) in self.0.iter().rev() {
                    remove(path);
                    match state {
                        Saved::Missing => {}
                        Saved::Directory => {
                            fs::create_dir_all(path).unwrap();
                        }
                        Saved::File(bytes, time) => {
                            fs::write(path, bytes).unwrap();
                            File::options()
                                .write(true)
                                .open(path)
                                .unwrap()
                                .set_times(fs::FileTimes::new().set_modified(*time))
                                .unwrap();
                        }
                    }
                }
            }
        }
        let mut results = vec![];
        for case in manifest["cases"].as_array().unwrap() {
            let name = case["case"].as_str().unwrap();
            let before = case["beforeCatalog"].as_str().unwrap();
            let after = case["afterCatalog"].as_str().unwrap();
            let expected = case["validAfter"].as_bool().unwrap();
            let db_path = root.join(format!("replay-{}-{name}.sqlite", std::process::id()));
            remove(&db_path);
            let mut db = Connection::open(&db_path).unwrap();
            import(&mut db, before).unwrap();
            let prepared_before = root.join(format!(
                "prepared-before-{}-{name}.fnmeta",
                std::process::id()
            ));
            remove(&prepared_before);
            prepare_file(before, prepared_before.to_str().unwrap()).unwrap();
            let prepared = PreparedCatalog::load(prepared_before.to_str().unwrap()).unwrap();
            assert_eq!(
                lookup(&db, symbol).unwrap()[0]["documentation"],
                case["expectedBefore"],
                "{name}: initial"
            );
            let mut restore = Restore(vec![]);
            for change in case["changes"].as_array().unwrap() {
                let path = PathBuf::from(change["path"].as_str().unwrap());
                let saved = match fs::symlink_metadata(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Saved::Missing,
                    Ok(meta) if meta.is_dir() => Saved::Directory,
                    Ok(meta) if meta.is_file() => {
                        Saved::File(fs::read(&path).unwrap(), meta.modified().unwrap())
                    }
                    other => panic!("Unexpected fixture mutation target {path:?}: {other:?}"),
                };
                let old_time = if let Saved::File(_, time) = &saved {
                    Some(*time)
                } else {
                    None
                };
                restore.0.push((path.clone(), saved));
                remove(&path);
                match change["kind"].as_str().unwrap() {
                    "file" => {
                        fs::create_dir_all(path.parent().unwrap()).unwrap();
                        fs::write(&path, change["text"].as_str().unwrap()).unwrap();
                    }
                    "missing" => {}
                    "directory" => fs::create_dir_all(&path).unwrap(),
                    kind => panic!("Unsupported fixture mutation {kind}"),
                }
                if change["preserveMtime"] == true {
                    File::options()
                        .write(true)
                        .open(&path)
                        .unwrap()
                        .set_times(fs::FileTimes::new().set_modified(old_time.unwrap()))
                        .unwrap();
                }
            }
            assert_eq!(valid(&db).unwrap(), expected, "{name}: cached validity");
            assert_eq!(
                prepared.valid().unwrap(),
                expected,
                "{name}: prepared cached validity"
            );
            assert_eq!(
                PreparedCatalog::load(prepared_before.to_str().unwrap()).is_ok(),
                expected,
                "{name}: prepared reopened validity"
            );
            drop(db);
            let mut reopened = Connection::open(&db_path).unwrap();
            assert_eq!(
                valid(&reopened).unwrap(),
                expected,
                "{name}: reopened validity"
            );
            assert_eq!(
                import(&mut reopened, before).is_ok(),
                expected,
                "{name}: old certificate on reopened import"
            );
            let mut fresh_old = Connection::open_in_memory().unwrap();
            assert_eq!(
                import(&mut fresh_old, before).is_ok(),
                expected,
                "{name}: old certificate on fresh import"
            );
            let mut fresh_new = Connection::open_in_memory().unwrap();
            import(&mut fresh_new, after).unwrap();
            let prepared_after = root.join(format!(
                "prepared-after-{}-{name}.fnmeta",
                std::process::id()
            ));
            remove(&prepared_after);
            prepare_file(after, prepared_after.to_str().unwrap()).unwrap();
            let fresh_prepared = PreparedCatalog::load(prepared_after.to_str().unwrap()).unwrap();
            assert_eq!(
                lookup(&fresh_prepared.database.db, symbol).unwrap()[0]["documentation"],
                case["expectedAfter"],
                "{name}: prepared fresh positive"
            );
            assert_eq!(
                lookup(&fresh_new, symbol).unwrap()[0]["documentation"],
                case["expectedAfter"],
                "{name}: fresh positive"
            );
            drop(restore);
            results.push(json!({"case":name,"initial":true,"cached":true,"reopened":true,"freshOld":true,"freshNew":true,"preparedCached":true,"preparedReopened":true,"preparedFresh":true,"validAfter":expected}));
            drop(reopened);
            remove(&db_path);
        }
        let evidence = json!({"scope":"actual exported XML witnesses: native cached, reopened and fresh imports","helperSha256":manifest["helperSha256"],"fixtureRoot":root,"nativeTestBinarySha256":hash_file(std::env::current_exe().unwrap().to_str().unwrap()).unwrap(),"passed":results.len(),"cases":results});
        fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test-results/recovery/xml-selection-native.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
    }
}
