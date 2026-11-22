use crate::{
    cancellation::CancellationToken,
    context::SourceContext,
    project,
    syntax::{self, ParsedFile},
};
use anyhow::{Context, Result};
use bincode::Options;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// Bounded suggestions, never a certificate that all declarations were visited.
pub struct CandidatePaths {
    pub paths: Vec<String>,
    pub truncated: bool,
    pub rows_examined: usize,
    pub eligible_seen: usize,
}

// Conservative name postings: inspect literal/comment text too. False positives
// cost a candidate parse; omitted interpolation/escaped identifiers would be
// an unsound reference filter. No syntax tree is constructed here.
fn raw_names(
    source: &str,
    cancellation: &CancellationToken,
) -> Result<std::collections::BTreeSet<String>> {
    let mut names = std::collections::BTreeSet::new();
    let mut i = 0;
    let mut next_check = 0;
    while i < source.len() {
        if i >= next_check {
            cancellation.check()?;
            next_check = i.saturating_add(4096);
        }
        let rest = &source[i..];
        if rest.starts_with("``") {
            if let Some(end) = rest[2..].find("``") {
                names.insert(rest[2..2 + end].to_string());
            }
            // Also scan interior words. Backticks in comments/strings must not
            // swallow a real identifier in the intervening source text.
            i += 2;
            continue;
        }
        let c = rest.chars().next().unwrap();
        if unicode_ident::is_xid_start(c) || c == '_' {
            let start = i;
            i += c.len_utf8();
            while i < source.len() {
                if i >= next_check {
                    cancellation.check()?;
                    next_check = i.saturating_add(4096);
                }
                let c = source[i..].chars().next().unwrap();
                if !(unicode_ident::is_xid_continue(c)
                    || c == '\''
                    || matches!(c, '\u{200c}' | '\u{200d}'))
                {
                    break;
                }
                i += c.len_utf8();
            }
            names.insert(source[start..i].into());
        } else {
            i += c.len_utf8();
        }
    }
    Ok(names)
}

#[derive(Default, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Counters {
    pub files_read: u64,
    pub bytes_read: u64,
    pub files_parsed: u64,
    pub indexed_files: u64,
    pub reindexed_files: u64,
    pub invalidated_files: u64,
    pub cache_hits: u64,
    pub index_hits: u64,
    pub helper_starts: u64,
    pub compiler_launches: u64,
    pub overlay_bytes_received: u64,
    pub query_count: u64,
    pub query_time_micros: u64,
    pub largest_token_count: u64,
    pub largest_summary_bytes: u64,
    pub type_proof_steps: u64,
    pub disk_validation_checks: u64,
    pub export_lookups: u64,
    pub export_cache_hits: u64,
    pub export_candidate_checks: u64,
}
#[derive(Clone, Default)]
struct ContextualExportFacts {
    declarations: Vec<(String, usize, String)>,
    auto_opens: Vec<String>,
    attributed_modules: Vec<(String, usize, String)>,
}
#[derive(Clone, Default)]
pub struct ContextualExports {
    pub declarations: Vec<(String, usize)>,
    pub possible_implicit: Vec<(String, usize)>,
}
#[derive(Default)]
struct RequestCache {
    validated: HashSet<String>,
    exports: HashMap<(String, String), Vec<(String, usize)>>,
    materialized_exports: HashSet<(String, String)>,
    contextual_facts: HashMap<(String, String, String), ContextualExportFacts>,
    contextual_exports: HashMap<(String, String, String), ContextualExports>,
    bytes: usize,
}
pub struct Index {
    pub db: Connection,
    pub path: PathBuf,
    pub counters: Counters,
    pub max_file_bytes: usize,
    cancellation: CancellationToken,
    validated: HashMap<String, (DiskIdentity, String)>,
    validated_bytes: usize,
    request_cache: Option<RequestCache>,
    overlay_trace: Option<OverlayUpdateTrace>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverlayUpdateTrace {
    pub lookup_micros: u64,
    pub transaction_begin_micros: u64,
    pub postings_delete_micros: u64,
    pub names_insert_micros: u64,
    pub state_write_micros: u64,
    pub commit_micros: u64,
    pub hash_micros: u64,
    pub name_scan_micros: u64,
    pub names_count: usize,
    pub index_update_micros: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
struct DiskIdentity {
    modified: u64,
    len: u64,
    #[cfg(unix)]
    change: (i64, i64, u64, u64),
}
fn disk_identity(path: &str) -> Result<DiskIdentity> {
    let metadata = fs::metadata(path)?;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(DiskIdentity {
        modified: metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
        len: metadata.len(),
        #[cfg(unix)]
        change: (
            metadata.ctime(),
            metadata.ctime_nsec(),
            metadata.dev(),
            metadata.ino(),
        ),
    })
}

fn stamp(path: &str) -> Result<(u64, u64)> {
    let m = fs::metadata(path)?;
    Ok((
        m.modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
        m.len(),
    ))
}
// Both modes persist ParsedFile in bincode; its layout is part of the key.
fn summary_revision(indexed: bool) -> &'static str {
    if indexed {
        "treesitter-fsharp-0.3.13-summary7:"
    } else {
        "lexical-summary4:"
    }
}
fn bounded_source(path: &str, limit: usize) -> Result<String> {
    crate::foreground::read_text(Path::new(path), limit as u64)
}
impl Index {
    pub fn open(dir: &Path, memory_mb: u64, max_file_bytes: usize) -> Result<Self> {
        fs::create_dir_all(dir)?;
        // A fresh schema namespace avoids interpreting legacy path-heavy rows
        // as file IDs. Old caches remain untouched and can be removed by users.
        let path = dir.join("index-v9.sqlite");
        let db = Connection::open(&path)?;
        db.execute_batch(&format!("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA temp_store=FILE; PRAGMA cache_size=-{}; PRAGMA mmap_size=0;
            CREATE TABLE IF NOT EXISTS paths(id INTEGER PRIMARY KEY,path TEXT NOT NULL UNIQUE);
            CREATE TABLE IF NOT EXISTS files(file_id INTEGER PRIMARY KEY, hash TEXT NOT NULL, stamp INTEGER NOT NULL, len INTEGER NOT NULL, dirty INTEGER NOT NULL, parsed BLOB, module TEXT);
            CREATE TABLE IF NOT EXISTS context_summaries(file_id INTEGER,context TEXT,hash TEXT,parsed BLOB,PRIMARY KEY(file_id,context)) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS occurrences(name TEXT NOT NULL,file_id INTEGER NOT NULL,PRIMARY KEY(name,file_id)) WITHOUT ROWID;
            CREATE INDEX IF NOT EXISTS occurrences_file ON occurrences(file_id);
            CREATE TABLE IF NOT EXISTS declarations(module TEXT,name TEXT,file_id INTEGER,token INTEGER,top_level INTEGER,proven INTEGER,accessibility TEXT);
            CREATE INDEX IF NOT EXISTS declaration_name ON declarations(name,module);
            CREATE INDEX IF NOT EXISTS declaration_file ON declarations(file_id,token);
            CREATE TABLE IF NOT EXISTS modules(name TEXT,file_id INTEGER,start INTEGER,end INTEGER,kind TEXT);
            CREATE INDEX IF NOT EXISTS module_name ON modules(name);
            CREATE INDEX IF NOT EXISTS module_file ON modules(file_id);
            CREATE TABLE IF NOT EXISTS answers(key TEXT PRIMARY KEY,snapshot TEXT NOT NULL,result TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS semantic_occurrences(symbol TEXT,path TEXT,start_line INTEGER,start_col INTEGER,end_line INTEGER,end_col INTEGER,is_definition INTEGER,declaration TEXT,context TEXT);
            CREATE INDEX IF NOT EXISTS semantic_position ON semantic_occurrences(path,start_line,start_col);
            CREATE INDEX IF NOT EXISTS semantic_symbol ON semantic_occurrences(symbol);
            CREATE TABLE IF NOT EXISTS semantic_meta(key TEXT PRIMARY KEY,value TEXT);
            CREATE TABLE IF NOT EXISTS semantic_inputs(path TEXT PRIMARY KEY,stamp INTEGER,len INTEGER,is_missing INTEGER NOT NULL DEFAULT 0,identity TEXT,sha256 TEXT);
            CREATE TABLE IF NOT EXISTS state(key TEXT PRIMARY KEY,value INTEGER);
            CREATE TABLE IF NOT EXISTS name_coverage(name TEXT,file_id INTEGER,stamp INTEGER,len INTEGER,hash TEXT,PRIMARY KEY(name,file_id)) WITHOUT ROWID;
            CREATE INDEX IF NOT EXISTS name_coverage_file ON name_coverage(file_id);
            INSERT OR IGNORE INTO state VALUES('generation',0);
            ",memory_mb.max(1)*1024))?;
        let has_missing_column = {
            let mut columns = db.prepare("PRAGMA table_info(semantic_inputs)")?;
            let names = columns
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|name| name == "is_missing")
        };
        if !has_missing_column {
            db.execute_batch(
                "ALTER TABLE semantic_inputs ADD COLUMN is_missing INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        let has_identity_column = {
            let mut columns = db.prepare("PRAGMA table_info(semantic_inputs)")?;
            let names = columns
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|name| name == "identity")
        };
        if !has_identity_column {
            db.execute_batch("ALTER TABLE semantic_inputs ADD COLUMN identity TEXT")?;
        }
        let has_hash_column = {
            let mut columns = db.prepare("PRAGMA table_info(semantic_inputs)")?;
            let names = columns
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|name| name == "sha256")
        };
        if !has_hash_column {
            db.execute_batch("ALTER TABLE semantic_inputs ADD COLUMN sha256 TEXT")?;
        }
        let has_coverage_hash = {
            let mut columns = db.prepare("PRAGMA table_info(name_coverage)")?;
            let names = columns
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|name| name == "hash")
        };
        if !has_coverage_hash {
            db.execute_batch("ALTER TABLE name_coverage ADD COLUMN hash TEXT")?;
        }
        let fingerprint = env!("FSHARP_NAVIGATOR_ENGINE_FINGERPRINT").to_string();
        let previous: Option<String> = db
            .query_row("SELECT value FROM state WHERE key='engine'", [], |r| {
                r.get(0)
            })
            .optional()?;
        if previous.as_deref() != Some(&fingerprint) {
            db.execute_batch("DELETE FROM context_summaries; DELETE FROM files; DELETE FROM occurrences; DELETE FROM declarations; DELETE FROM modules; DELETE FROM answers; DELETE FROM name_coverage; DELETE FROM paths; DELETE FROM semantic_occurrences; DELETE FROM semantic_meta; DELETE FROM semantic_inputs; UPDATE state SET value=value+1 WHERE key='generation';")?;
            db.execute(
                "INSERT OR REPLACE INTO state VALUES('engine',?1)",
                [fingerprint],
            )?;
        }
        // Prior-session overlays are not saved source facts. Missing-file
        // overlays have a zero original disk stamp/length and are removed.
        if db.query_row(
            "SELECT count(*) FROM files WHERE dirty=1 AND stamp=0 AND len=0",
            [],
            |r| r.get::<_, u64>(0),
        )? > 0
        {
            db.execute_batch("BEGIN IMMEDIATE;
                DELETE FROM context_summaries WHERE file_id IN (SELECT file_id FROM files WHERE dirty=1 AND stamp=0 AND len=0);
                DELETE FROM occurrences WHERE file_id IN (SELECT file_id FROM files WHERE dirty=1 AND stamp=0 AND len=0);
                DELETE FROM declarations WHERE file_id IN (SELECT file_id FROM files WHERE dirty=1 AND stamp=0 AND len=0);
                DELETE FROM modules WHERE file_id IN (SELECT file_id FROM files WHERE dirty=1 AND stamp=0 AND len=0);
                DELETE FROM name_coverage WHERE file_id IN (SELECT file_id FROM files WHERE dirty=1 AND stamp=0 AND len=0);
                DELETE FROM paths WHERE id IN (SELECT file_id FROM files WHERE dirty=1 AND stamp=0 AND len=0);
                DELETE FROM files WHERE dirty=1 AND stamp=0 AND len=0;
                DELETE FROM answers;
                UPDATE state SET value=value+1 WHERE key='generation';
                COMMIT;")?;
        }
        // The per-session validation cache starts empty: these disk-owned rows
        // must hash saved bytes on first demand, preserving zero startup reads.
        if db.execute("UPDATE files SET dirty=0 WHERE dirty=1", [])? > 0 {
            db.execute("DELETE FROM answers", [])?;
        }
        Ok(Self {
            db,
            path,
            counters: Counters::default(),
            max_file_bytes,
            cancellation: CancellationToken::new(),
            validated: HashMap::new(),
            validated_bytes: 0,
            request_cache: None,
            overlay_trace: None,
        })
    }
    pub fn set_cancellation(&mut self, cancellation: CancellationToken) {
        self.cancellation = cancellation;
    }
    pub fn begin_request(&mut self) {
        self.request_cache = Some(RequestCache::default());
    }
    pub fn end_request(&mut self) {
        self.request_cache = None;
    }
    fn invalidate_request_cache(&mut self) {
        if self.request_cache.is_some() {
            self.begin_request();
        }
    }
    pub fn forget_validation(&mut self) {
        self.validated.clear();
        self.validated_bytes = 0;
        self.invalidate_request_cache();
    }
    fn validated_hash(&self, path: &str, identity: &DiskIdentity) -> Option<String> {
        // Unix ctime + device/inode catches same-size, restored-mtime edits and
        // replacements within a session. Other platforms conservatively hash.
        if !cfg!(unix) {
            return None;
        }
        self.validated
            .get(path)
            .filter(|(previous, _)| previous == identity)
            .map(|(_, hash)| hash.clone())
    }
    fn remember_validation(&mut self, path: &str, identity: DiskIdentity, hash: String) {
        let bytes = path.len() + 256;
        if self.validated.contains_key(path) {
            self.validated_bytes -= bytes;
        }
        // Bounded metadata/hash cache; eviction causes revalidation, never a
        // completeness shortcut. No source text or AST is retained here.
        if self.validated_bytes.saturating_add(bytes) > 4 * 1024 * 1024 {
            self.forget_validation();
        }
        self.validated.insert(path.into(), (identity, hash));
        self.validated_bytes += bytes;
    }
    fn read_verified(&mut self, path: &str, identity: &DiskIdentity) -> Result<(String, String)> {
        self.cancellation.check()?;
        anyhow::ensure!(
            identity.len as usize <= self.max_file_bytes,
            "Source exceeds configured maxFileBytes: {path}"
        );
        let source =
            bounded_source(path, self.max_file_bytes).with_context(|| format!("read {path}"))?;
        self.counters.files_read += 1;
        self.counters.bytes_read += source.len() as u64;
        anyhow::ensure!(
            disk_identity(path)? == *identity,
            "File changed during validation; retry: {path}"
        );
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        self.cancellation.check()?;
        Ok((source, hash))
    }
    fn validate_disk(&mut self, path: &str) -> Result<()> {
        if self
            .request_cache
            .as_ref()
            .is_some_and(|cache| cache.validated.contains(path))
        {
            return Ok(());
        }
        let row: Option<(String, bool)> = self
            .db
            .query_row(
                "SELECT hash,dirty FROM files WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
                [path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((hash, false)) = row {
            self.sync_disk(path, !hash.starts_with("lexical-"), false)?;
        }
        if let Some(cache) = &mut self.request_cache {
            let bytes = path.len() + 128;
            if cache.bytes + bytes <= 2 * 1024 * 1024 {
                cache.validated.insert(path.into());
                cache.bytes += bytes;
            }
        }
        Ok(())
    }
    fn file_id(db: &Connection, path: &str) -> Result<i64> {
        db.execute("INSERT OR IGNORE INTO paths(path) VALUES(?1)", [path])?;
        Ok(db.query_row("SELECT id FROM paths WHERE path=?1", [path], |r| r.get(0))?)
    }
    pub fn sync_disk(&mut self, path: &str, indexed: bool, force: bool) -> Result<bool> {
        self.cancellation.check()?;
        self.counters.disk_validation_checks += 1;
        let identity = match disk_identity(path) {
            Ok(s) => s,
            Err(e) => {
                self.remove(path)?;
                return Err(e.into());
            }
        };
        let (mtime, len) = (identity.modified, identity.len);
        let old: Option<(u64, u64, bool, String)> = self
            .db
            .query_row(
                "SELECT stamp,len,dirty,hash FROM files WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let revision = summary_revision(indexed);
        if !force
            && old
                .is_some_and(|(m, l, d, h)| m == mtime && l == len && !d && h.starts_with(revision))
            && self.validated_hash(path, &identity).is_some()
        {
            return Ok(false);
        }
        anyhow::ensure!(
            len as usize <= self.max_file_bytes,
            "Source exceeds configured maxFileBytes: {path}"
        );
        let (source, hash) = self.read_verified(path, &identity)?;
        let (changed, _) = self.update(path, &source, mtime, len, false, indexed, false)?;
        self.remember_validation(path, identity, hash);
        Ok(changed)
    }
    pub(crate) fn overlay_traced(
        &mut self,
        path: &str,
        source: &str,
        indexed: bool,
        tracing: bool,
    ) -> Result<(bool, blake3::Hash, Option<OverlayUpdateTrace>)> {
        self.overlay_trace = tracing.then(OverlayUpdateTrace::default);
        let started = tracing.then(std::time::Instant::now);
        let result = self.overlay(path, source, indexed);
        let mut trace = self.overlay_trace.take();
        if let (Some(trace), Some(started)) = (&mut trace, started) {
            trace.index_update_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }
        result.map(|(changed, hash)| (changed, hash, trace))
    }

    pub fn overlay(
        &mut self,
        path: &str,
        source: &str,
        indexed: bool,
    ) -> Result<(bool, blake3::Hash)> {
        anyhow::ensure!(
            source.len() <= self.max_file_bytes,
            "Overlay exceeds maxFileBytes"
        );
        self.counters.overlay_bytes_received += source.len() as u64;
        let (mtime, len) = stamp(path).unwrap_or((0, 0));
        self.update(path, source, mtime, len, true, indexed, false)
    }
    pub fn materialize_overlay(&mut self, path: &str, source: &str, indexed: bool) -> Result<()> {
        let (mtime, len) = stamp(path).unwrap_or((0, 0));
        self.update(path, source, mtime, len, true, indexed, true)?;
        Ok(())
    }
    pub fn search_name(&mut self, path: &str, name: &str, indexed: bool) -> Result<()> {
        self.cancellation.check()?;
        self.counters.disk_validation_checks += 1;
        let identity = disk_identity(path)?;
        let (mtime, len) = (identity.modified, identity.len);
        let revision = summary_revision(indexed);
        let row: Option<(u64,u64,bool,String)> = self.db.query_row(
            "SELECT stamp,len,dirty,hash FROM files WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
            [path], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let covered: Option<String> = self.db.query_row(
            "SELECT hash FROM name_coverage WHERE name=?1 AND file_id=(SELECT id FROM paths WHERE path=?2)",
            params![name,path], |r| r.get::<_,Option<String>>(0)).optional()?.flatten();
        if let Some(hash) = self.validated_hash(path, &identity) {
            if row.as_ref().is_some_and(|(m, l, dirty, saved)| {
                *m == mtime && *l == len && !dirty && saved == &format!("{revision}{hash}")
            }) {
                return Ok(());
            }
            if row.is_none() && covered.as_deref() == Some(&hash) {
                return Ok(());
            }
        }
        let (source, hash) = self.read_verified(path, &identity)?;
        if row
            .as_ref()
            .is_some_and(|(_, _, _, saved)| saved == &format!("{revision}{hash}"))
        {
            self.update(path, &source, mtime, len, false, indexed, false)?;
        } else if source.contains(name) {
            self.update(path, &source, mtime, len, false, indexed, false)?;
        } else {
            if row.is_some() {
                self.remove(path)?;
            }
            if covered.as_deref() != Some(&hash) || row.is_some() {
                let tx = self.db.transaction()?;
                let file_id = Self::file_id(&tx, path)?;
                tx.execute("INSERT OR REPLACE INTO name_coverage(name,file_id,stamp,len,hash) VALUES(?1,?2,?3,?4,?5)",params![name,file_id,mtime,len,hash])?;
                tx.execute("UPDATE state SET value=value+1 WHERE key='generation'", [])?;
                self.cancellation.check()?;
                tx.commit()?;
                self.invalidate_request_cache();
            }
        }
        self.remember_validation(path, identity, hash);
        Ok(())
    }
    fn update(
        &mut self,
        path: &str,
        source: &str,
        mtime: u64,
        len: u64,
        dirty: bool,
        indexed: bool,
        eager: bool,
    ) -> Result<(bool, blake3::Hash)> {
        self.cancellation.check()?;
        let _scope = self.cancellation.enter();
        let hash_started = self
            .overlay_trace
            .as_ref()
            .map(|_| std::time::Instant::now());
        let source_hash = blake3::hash(source.as_bytes());
        if let (Some(trace), Some(started)) = (&mut self.overlay_trace, hash_started) {
            trace.hash_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }
        let hash = format!("{}{}", summary_revision(indexed), source_hash.to_hex());
        let lookup_started = self
            .overlay_trace
            .as_ref()
            .map(|_| std::time::Instant::now());
        let old: Option<String> = self
            .db
            .query_row(
                "SELECT hash FROM files WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
                [path],
                |r| r.get(0),
            )
            .optional()?;
        if let (Some(trace), Some(started)) = (&mut self.overlay_trace, lookup_started) {
            trace.lookup_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }
        if old.as_deref() == Some(&hash) {
            let state_write_started = self
                .overlay_trace
                .as_ref()
                .map(|_| std::time::Instant::now());
            self.db.execute(
                "UPDATE files SET stamp=?2,len=?3,dirty=?4 WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
                params![path, mtime, len, dirty],
            )?;
            if let (Some(trace), Some(started)) = (&mut self.overlay_trace, state_write_started) {
                trace.state_write_micros =
                    started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            }
            if eager && self.needs_summary(path)? {
                crate::foreground::parse()?;
                let parsed = if indexed {
                    if project::signature(path) {
                        crate::treesitter::parse_signature(source)
                    } else {
                        crate::parse_indexed(source)
                    }
                } else {
                    syntax::parse(source)
                };
                self.counters.files_parsed += 1;
                self.cancellation.check()?;
                if !(parsed.tokens.is_empty() && !parsed.supported) {
                    let tx = self.db.transaction()?;
                    Self::store_summary(&tx, path, &parsed)?;
                    tx.commit()?;
                    self.invalidate_request_cache();
                }
            }
            return Ok((false, source_hash));
        }
        let parsed = if !eager {
            None
        } else {
            crate::foreground::parse()?;
            Some(if indexed {
                if project::signature(path) {
                    crate::treesitter::parse_signature(source)
                } else {
                    crate::parse_indexed(source)
                }
            } else {
                syntax::parse(source)
            })
        };
        if parsed.is_some() {
            self.counters.files_parsed += 1;
        }
        self.cancellation.check()?;
        let transaction_begin_started = self
            .overlay_trace
            .as_ref()
            .map(|_| std::time::Instant::now());
        let tx = self.db.transaction()?;
        if let (Some(trace), Some(started)) = (&mut self.overlay_trace, transaction_begin_started) {
            trace.transaction_begin_micros =
                started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }
        let postings_delete_started = self
            .overlay_trace
            .as_ref()
            .map(|_| std::time::Instant::now());
        let file_id = Self::file_id(&tx, path)?;
        tx.execute("DELETE FROM context_summaries WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM occurrences WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM declarations WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM modules WHERE file_id=?1", [file_id])?;
        if let (Some(trace), Some(started)) = (&mut self.overlay_trace, postings_delete_started) {
            trace.postings_delete_micros =
                started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }
        {
            let mut insert =
                tx.prepare_cached("INSERT OR IGNORE INTO occurrences(name,file_id) VALUES(?1,?2)")?;
            let scan_started = self
                .overlay_trace
                .as_ref()
                .map(|_| std::time::Instant::now());
            let names = raw_names(source, &self.cancellation)?;
            if let (Some(trace), Some(started)) = (&mut self.overlay_trace, scan_started) {
                trace.name_scan_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
                trace.names_count = names.len();
            }
            let names_insert_started = self
                .overlay_trace
                .as_ref()
                .map(|_| std::time::Instant::now());
            for (position, name) in names.into_iter().enumerate() {
                if position % 256 == 0 {
                    self.cancellation.check()?;
                }
                insert.execute(params![name, file_id])?;
            }
            if let (Some(trace), Some(started)) = (&mut self.overlay_trace, names_insert_started) {
                trace.names_insert_micros =
                    started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            }
        }
        let state_write_started = self
            .overlay_trace
            .as_ref()
            .map(|_| std::time::Instant::now());
        tx.execute("INSERT OR REPLACE INTO files(file_id,hash,stamp,len,dirty,parsed,module) VALUES(?1,?2,?3,?4,?5,NULL,?6)",params![file_id,hash,mtime,len,dirty,parsed.as_ref().and_then(|p|p.module.as_ref())])?;
        if let Some(parsed) = &parsed {
            if !(parsed.tokens.is_empty() && !parsed.supported) {
                let bytes = Self::store_summary(&tx, path, parsed)?;
                self.counters.largest_summary_bytes =
                    self.counters.largest_summary_bytes.max(bytes);
                self.counters.largest_token_count = self
                    .counters
                    .largest_token_count
                    .max(parsed.tokens.len() as u64);
            }
        }
        tx.execute("DELETE FROM answers", [])?;
        tx.execute("UPDATE state SET value=value+1 WHERE key='generation'", [])?;
        if let (Some(trace), Some(started)) = (&mut self.overlay_trace, state_write_started) {
            trace.state_write_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }
        self.cancellation.check()?;
        let commit_started = self
            .overlay_trace
            .as_ref()
            .map(|_| std::time::Instant::now());
        tx.commit()?;
        if let (Some(trace), Some(started)) = (&mut self.overlay_trace, commit_started) {
            trace.commit_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }

        self.invalidate_request_cache();
        if old.is_some() {
            self.counters.reindexed_files += 1;
        } else {
            self.counters.indexed_files += 1;
        }
        self.counters.invalidated_files += 1;
        Ok((true, source_hash))
    }
    pub fn remove(&mut self, path: &str) -> Result<()> {
        self.invalidate_request_cache();
        if self.validated.remove(path).is_some() {
            self.validated_bytes -= path.len() + 256;
        }
        let tx = self.db.transaction()?;
        let file_id: Option<i64> = tx
            .query_row("SELECT id FROM paths WHERE path=?1", [path], |r| r.get(0))
            .optional()?;
        let Some(file_id) = file_id else {
            return Ok(());
        };
        let changed = tx.execute("DELETE FROM files WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM context_summaries WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM occurrences WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM declarations WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM modules WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM name_coverage WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM paths WHERE id=?1", [file_id])?;
        if changed > 0 {
            tx.execute("DELETE FROM answers", [])?;
            tx.execute("UPDATE state SET value=value+1 WHERE key='generation'", [])?;
        }
        tx.commit()?;
        self.invalidate_request_cache();
        Ok(())
    }
    /// Fresh bytes for candidate-only syntax recovery. Dirty overlays are owned
    /// by Workspace and must never be replaced by disk text here.
    pub fn candidate_source(&mut self, path: &str) -> Result<String> {
        self.cancellation.check()?;
        self.validate_disk(path)?;
        let (expected, dirty): (String, bool) = self.db.query_row(
            "SELECT hash,dirty FROM files WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
            [path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        anyhow::ensure!(
            !dirty,
            "Candidate source requires the current dirty overlay: {path}"
        );
        let identity = disk_identity(path)?;
        let (source, hash) = self.read_verified(path, &identity)?;
        anyhow::ensure!(
            expected.ends_with(&hash),
            "File changed while query was pending; refresh and retry: {path}"
        );
        Ok(source)
    }
    pub fn parsed(&mut self, path: &str) -> Result<ParsedFile> {
        self.cancellation.check()?;
        let _scope = self.cancellation.enter();
        self.validate_disk(path)?;
        let (s, hash, dirty): (Option<Vec<u8>>, String, bool) = self
            .db
            .query_row(
                "SELECT parsed,hash,dirty FROM files WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .with_context(|| format!("File not indexed: {path}"))?;
        self.counters.index_hits += 1;
        if let Some(s) = s {
            return Ok(bincode::DefaultOptions::new()
                .with_varint_encoding()
                .deserialize(&s)?);
        }
        anyhow::ensure!(!dirty,"Edited document could not be parsed within resource budget; retry or raise parser budget");
        let source = bounded_source(path, self.max_file_bytes)?;
        anyhow::ensure!(
            source.len() <= self.max_file_bytes,
            "Source exceeds maxFileBytes"
        );
        self.counters.files_read += 1;
        self.counters.bytes_read += source.len() as u64;
        anyhow::ensure!(
            hash.ends_with(blake3::hash(source.as_bytes()).to_hex().as_str()),
            "File changed while query was pending; refresh and retry: {path}"
        );
        crate::foreground::parse()?;
        let parsed = if hash.starts_with("lexical-") {
            syntax::parse(&source)
        } else if project::signature(path) {
            crate::treesitter::parse_signature(&source)
        } else {
            crate::parse_indexed(&source)
        };
        self.counters.files_parsed += 1;
        self.cancellation.check()?;
        if parsed.tokens.is_empty() && !parsed.supported {
            return Ok(parsed);
        }
        let tx = self.db.transaction()?;
        let bytes = Self::store_summary(&tx, path, &parsed)?;
        self.cancellation.check()?;
        tx.commit()?;
        self.invalidate_request_cache();
        self.counters.largest_summary_bytes = self.counters.largest_summary_bytes.max(bytes);
        self.counters.largest_token_count = self
            .counters
            .largest_token_count
            .max(parsed.tokens.len() as u64);
        Ok(parsed)
    }
    pub fn needs_summary_in(&self, path: &str, context: &SourceContext) -> Result<bool> {
        Ok(!self.db.query_row("SELECT 1 FROM context_summaries s JOIN files f ON f.file_id=s.file_id JOIN paths p ON p.id=f.file_id WHERE p.path=?1 AND s.context=?2 AND s.hash=f.hash",params![path,context.id],|_|Ok(())).optional()?.is_some())
    }
    fn cached_context(
        &mut self,
        path: &str,
        context: &SourceContext,
    ) -> Result<Option<ParsedFile>> {
        let bytes:Option<Vec<u8>>=self.db.query_row("SELECT s.parsed FROM context_summaries s JOIN files f ON f.file_id=s.file_id JOIN paths p ON p.id=f.file_id WHERE p.path=?1 AND s.context=?2 AND s.hash=f.hash",params![path,context.id],|r|r.get(0)).optional()?;
        if let Some(bytes) = bytes {
            self.counters.index_hits += 1;
            Ok(Some(
                bincode::DefaultOptions::new()
                    .with_varint_encoding()
                    .deserialize(&bytes)?,
            ))
        } else {
            Ok(None)
        }
    }
    fn parse_context(
        &mut self,
        path: &str,
        source: &str,
        context: &SourceContext,
    ) -> Result<ParsedFile> {
        self.cancellation.check()?;
        let _scope = self.cancellation.enter();
        crate::foreground::parse()?;
        let projected = crate::context::project(source, context, &self.cancellation)?;
        let mut parsed = if project::signature(path) {
            crate::treesitter::parse_signature(&projected)
        } else {
            crate::parse_indexed(&projected)
        };
        if projected != source {
            // Definition-only operator certificates never infer an active branch.
            parsed.operator_proofs_complete = false;
        }
        self.counters.files_parsed += 1;
        self.cancellation.check()?;
        if parsed.tokens.is_empty() && !parsed.supported {
            return Ok(parsed);
        }
        let bytes = bincode::DefaultOptions::new()
            .with_varint_encoding()
            .serialize(&parsed)?;
        self.counters.largest_summary_bytes =
            self.counters.largest_summary_bytes.max(bytes.len() as u64);
        self.counters.largest_token_count = self
            .counters
            .largest_token_count
            .max(parsed.tokens.len() as u64);
        // Projection never enters the physical declaration tables: only this
        // source instance owns its token indices and branch-specific facts.
        self.db.execute("INSERT OR REPLACE INTO context_summaries(file_id,context,hash,parsed) SELECT f.file_id,?2,f.hash,?3 FROM files f JOIN paths p ON p.id=f.file_id WHERE p.path=?1",params![path,context.id,bytes])?;
        Ok(parsed)
    }
    pub fn parsed_in(&mut self, path: &str, context: &SourceContext) -> Result<ParsedFile> {
        self.cancellation.check()?;
        self.validate_disk(path)?;
        if let Some(parsed) = self.cached_context(path, context)? {
            return Ok(parsed);
        }
        let (hash, dirty): (String, bool) = self
            .db
            .query_row(
                "SELECT hash,dirty FROM files WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .with_context(|| format!("File not indexed: {path}"))?;
        anyhow::ensure!(
            !dirty,
            "Current overlay must be materialized for the selected source context"
        );
        let source = bounded_source(path, self.max_file_bytes)?;
        self.counters.files_read += 1;
        self.counters.bytes_read += source.len() as u64;
        anyhow::ensure!(
            hash.ends_with(blake3::hash(source.as_bytes()).to_hex().as_str()),
            "File changed while contextual query was pending; retry: {path}"
        );
        self.parse_context(path, &source, context)
    }
    pub fn materialize_overlay_in(
        &mut self,
        path: &str,
        source: &str,
        context: &SourceContext,
    ) -> Result<ParsedFile> {
        anyhow::ensure!(
            source.len() <= self.max_file_bytes,
            "Overlay exceeds maxFileBytes"
        );
        let (mtime, len) = stamp(path).unwrap_or((0, 0));
        self.update(path, source, mtime, len, true, true, false)?;
        if let Some(parsed) = self.cached_context(path, context)? {
            return Ok(parsed);
        }
        self.parse_context(path, source, context)
    }
    /// Raw-name postings are conservative physical-file unions. These export
    /// facts come only from eligible, separately projected project instances.
    /// The caller filters visibility/order AFTER this stable-domain lookup.
    pub fn exported_in(
        &mut self,
        module: &str,
        name: &str,
        instances: &[(String, SourceContext)],
    ) -> Result<ContextualExports> {
        self.cancellation.check()?;
        self.counters.export_lookups += 1;
        let mut domain = blake3::Hasher::new();
        let mut selected = HashMap::new();
        for (path, context) in instances {
            self.cancellation.check()?;
            if let Some(old) = selected.insert(path.as_str(), context) {
                anyhow::ensure!(
                    old.id == context.id,
                    "Ambiguous source instances in export proof domain: {path}"
                );
            }
            domain.update(path.as_bytes());
            domain.update(&[0]);
            domain.update(context.id.as_bytes());
            domain.update(&[0]);
        }
        let domain = domain.finalize().to_hex().to_string();
        let key = (module.to_string(), name.to_string(), domain.clone());
        if let Some(result) = self
            .request_cache
            .as_ref()
            .and_then(|c| c.contextual_exports.get(&key))
        {
            self.counters.export_cache_hits += 1;
            return Ok(result.clone());
        }
        let part = module.rsplit('.').next().unwrap_or(module);
        let fact_key = (part.to_string(), name.to_string(), domain);
        let cached = self
            .request_cache
            .as_ref()
            .and_then(|c| c.contextual_facts.get(&fact_key))
            .cloned();
        let facts = if let Some(facts) = cached {
            facts
        } else {
            let candidates = {
                let mut s=self.db.prepare_cached("SELECT p.path FROM occurrences m CROSS JOIN occurrences n JOIN paths p ON p.id=m.file_id WHERE m.name=?2 AND n.name=?1 AND n.file_id=m.file_id ORDER BY p.path")?;
                let paths = s
                    .query_map(params![name, part], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                paths
            };
            let mut facts = ContextualExportFacts::default();
            for path in candidates {
                self.cancellation.check()?;
                let Some(context) = selected.get(path.as_str()) else {
                    continue;
                };
                self.counters.export_candidate_checks += 1;
                let parsed = self.parsed_in(&path, context)?;
                anyhow::ensure!(
                    !(parsed.tokens.is_empty() && !parsed.supported),
                    "Candidate analysis unavailable: {}: {}",
                    path,
                    parsed.reasons.join("; ")
                );
                for declaration in &parsed.declarations {
                    if declaration.top_level && declaration.name == name {
                        facts.declarations.push((
                            path.clone(),
                            declaration.token,
                            parsed
                                .declaration_module(declaration)
                                .unwrap_or_default()
                                .to_string(),
                        ));
                    }
                }
                // The attribute spelling alone cannot certify AutoOpen. An
                // attributed nested scope containing the requested spelling
                // may contribute it when a consumer opens its parent.
                let mut name_offsets = None;
                for attr in parsed
                    .attributes
                    .iter()
                    .filter(|a| a.owner_kind == "module")
                {
                    let offsets = name_offsets.get_or_insert_with(|| {
                        parsed
                            .tokens
                            .iter()
                            .filter(|t| t.ident && t.text == name)
                            .map(|t| t.start)
                            .collect::<Vec<_>>()
                    });
                    let first = offsets.partition_point(|&offset| offset < attr.owner_start);
                    if offsets
                        .get(first)
                        .is_some_and(|&offset| offset < attr.owner_end)
                    {
                        facts.attributed_modules.push((
                            path.clone(),
                            attr.owner_start,
                            attr.owner.clone(),
                        ));
                    }
                }
                facts
                    .auto_opens
                    .extend(parsed.auto_opens.iter().map(|m| m.name.clone()));
            }
            facts.attributed_modules.sort();
            facts.attributed_modules.dedup();
            facts.auto_opens.sort();
            facts.auto_opens.dedup();
            // Store compact requested-name facts, not ASTs. This prevents
            // full-module-prefix probes from decoding the same files N² times.
            if let Some(cache) = &mut self.request_cache {
                let bytes = 512
                    + fact_key.0.len()
                    + fact_key.1.len()
                    + fact_key.2.len()
                    + facts
                        .declarations
                        .iter()
                        .map(|(p, _, m)| p.len() + m.len() + 128)
                        .sum::<usize>()
                    + facts.auto_opens.iter().map(|m| m.len() + 48).sum::<usize>()
                    + facts
                        .attributed_modules
                        .iter()
                        .map(|(p, _, m)| p.len() + m.len() + 128)
                        .sum::<usize>();
                if cache.contextual_facts.len() < 64 && cache.bytes + bytes <= 2 * 1024 * 1024 {
                    cache.bytes += bytes;
                    cache.contextual_facts.insert(fact_key, facts.clone());
                }
            }
            facts
        };
        let mut modules = HashSet::from([module.to_string()]);
        let mut queue = vec![module.to_string()];
        while let Some(parent) = queue.pop() {
            self.cancellation.check()?;
            let prefix = format!("{parent}.");
            for child in &facts.auto_opens {
                if child
                    .strip_prefix(&prefix)
                    .is_some_and(|tail| !tail.contains('.'))
                    && modules.insert(child.clone())
                {
                    queue.push(child.clone());
                }
            }
        }
        let declarations = facts
            .declarations
            .iter()
            .filter(|(_, _, m)| modules.contains(m))
            .map(|(p, t, _)| (p.clone(), *t))
            .collect();
        let possible_implicit = facts
            .attributed_modules
            .iter()
            .filter(|(_, _, owner)| {
                owner
                    .strip_prefix(module)
                    .is_some_and(|tail| tail.starts_with('.'))
            })
            .map(|(path, start, _)| (path.clone(), *start))
            .collect();
        let result = ContextualExports {
            declarations,
            possible_implicit,
        };
        if let Some(cache) = &mut self.request_cache {
            let bytes = 512
                + key.0.len()
                + key.1.len()
                + key.2.len()
                + result
                    .declarations
                    .iter()
                    .chain(&result.possible_implicit)
                    .map(|(p, _)| p.len() + 128)
                    .sum::<usize>();
            if cache.contextual_exports.len() < 64 && cache.bytes + bytes <= 2 * 1024 * 1024 {
                cache.bytes += bytes;
                cache.contextual_exports.insert(key, result.clone());
            }
        }
        Ok(result)
    }
    fn store_summary(
        tx: &rusqlite::Transaction<'_>,
        path: &str,
        parsed: &ParsedFile,
    ) -> Result<u64> {
        let file_id = Self::file_id(tx, path)?;
        tx.execute("DELETE FROM declarations WHERE file_id=?1", [file_id])?;
        tx.execute("DELETE FROM modules WHERE file_id=?1", [file_id])?;
        {
            let mut insert=tx.prepare_cached("INSERT INTO declarations(module,name,file_id,token,top_level,proven,accessibility) VALUES(?1,?2,?3,?4,?5,?6,?7)")?;
            for d in &parsed.declarations {
                crate::cancellation::check()?;
                insert.execute(params![
                    parsed.declaration_module(d),
                    d.name,
                    file_id,
                    d.token,
                    d.top_level,
                    parsed.binding_supported(d.token),
                    d.accessibility
                ])?;
            }
        }
        {
            let mut insert = tx.prepare_cached("INSERT INTO modules VALUES(?1,?2,?3,?4,?5)")?;
            for m in &parsed.modules {
                crate::cancellation::check()?;
                insert.execute(params![m.name, file_id, m.start, m.end, "module"])?;
            }
            for m in &parsed.owned_types {
                crate::cancellation::check()?;
                insert.execute(params![m.name, file_id, m.start, m.end, "type"])?;
            }
        }
        {
            let mut insert =
                tx.prepare_cached("INSERT INTO modules VALUES(?1,?2,?3,?4,'autoopen')")?;
            for m in &parsed.auto_opens {
                crate::cancellation::check()?;
                insert.execute(params![m.name, file_id, m.start, m.end])?;
            }
        }
        let summary = bincode::DefaultOptions::new()
            .with_varint_encoding()
            .serialize(parsed)?;
        let bytes = summary.len() as u64;
        tx.execute(
            "UPDATE files SET parsed=?2,module=?3 WHERE file_id=?1",
            params![file_id, summary, parsed.module],
        )?;
        Ok(bytes)
    }
    pub fn candidate_paths(&self, name: &str) -> Result<Vec<String>> {
        let mut s = self
            .db
            .prepare_cached("SELECT p.path FROM occurrences o JOIN paths p ON p.id=o.file_id WHERE o.name=?1 ORDER BY p.path")?;
        let result = s
            .query_map([name], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(result)
    }
    pub fn candidate_paths_limited(&self, name: &str, limit: usize) -> Result<Vec<String>> {
        self.cancellation.check()?;
        let mut statement = self.db.prepare_cached(
            "SELECT p.path FROM occurrences o JOIN paths p ON p.id=o.file_id WHERE o.name=?1 ORDER BY p.path LIMIT ?2",
        )?;
        let result = statement
            .query_map(params![name, limit], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(result)
    }
    /// Filter eligibility and rank evidence before the retained-path cap. Memory
    /// is O(limit); scanning is bounded independently and cancellation is checked
    /// for every row. A stopped scan or omitted eligible path is explicit.
    pub fn ranked_candidate_paths(
        &self,
        name: &str,
        limit: usize,
        max_rows: usize,
        mut rank: impl FnMut(&str) -> Option<usize>,
    ) -> Result<CandidatePaths> {
        self.cancellation.check()?;
        let mut statement = self.db.prepare_cached(
            "SELECT p.path FROM occurrences o JOIN paths p ON p.id=o.file_id WHERE o.name=?1 ORDER BY o.file_id LIMIT ?2",
        )?;
        let mut rows = statement.query(params![name, max_rows.saturating_add(1)])?;
        let mut retained = std::collections::BinaryHeap::<(usize, String)>::new();
        let mut result = CandidatePaths {
            paths: Vec::new(),
            truncated: false,
            rows_examined: 0,
            eligible_seen: 0,
        };
        while let Some(row) = rows.next()? {
            self.cancellation.check()?;
            if result.rows_examined == max_rows {
                result.truncated = true;
                break;
            }
            result.rows_examined += 1;
            let path: String = row.get(0)?;
            let Some(priority) = rank(&path) else {
                continue;
            };
            result.eligible_seen += 1;
            if limit == 0 {
                continue;
            }
            let candidate = (priority, path);
            if retained.len() < limit {
                retained.push(candidate);
            } else if retained.peek().is_some_and(|last| &candidate < last) {
                retained.pop();
                retained.push(candidate);
            }
        }
        result.truncated |= result.eligible_seen > limit;
        result.paths = retained
            .into_sorted_vec()
            .into_iter()
            .map(|(_, path)| path)
            .collect();
        Ok(result)
    }
    pub fn needs_summary(&self, path: &str) -> Result<bool> {
        Ok(self
            .db
            .query_row(
                "SELECT parsed IS NULL FROM files WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
                [path],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(true))
    }
    pub fn declaration_candidates(&self, name: &str, limit: u64) -> Result<Vec<(String, usize)>> {
        let mut s = self
            .db
            .prepare_cached("SELECT p.path,d.token FROM declarations d JOIN paths p ON p.id=d.file_id WHERE d.name=?1 LIMIT ?2")?;
        let result = s
            .query_map(params![name, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(result)
    }
    pub fn exported(&mut self, module: &str, name: &str) -> Result<Vec<(String, usize)>> {
        self.cancellation.check()?;
        self.counters.export_lookups += 1;
        let key = (module.to_string(), name.to_string());
        if let Some(result) = self
            .request_cache
            .as_ref()
            .and_then(|cache| cache.exports.get(&key))
        {
            self.counters.export_cache_hits += 1;
            return Ok(result.clone());
        }
        let result = self.exported_uncached(module, name)?;
        if let Some(cache) = &mut self.request_cache {
            let bytes = key.0.len()
                + key.1.len()
                + 256
                + result
                    .iter()
                    .map(|(path, _)| path.len() + 128)
                    .sum::<usize>();
            if cache.exports.len() < 64 && cache.bytes + bytes <= 2 * 1024 * 1024 {
                cache.exports.insert(key, result.clone());
                cache.bytes += bytes;
            }
        }
        Ok(result)
    }
    fn exported_uncached(&mut self, module: &str, name: &str) -> Result<Vec<(String, usize)>> {
        // Qualified source modules must spell their module identifier somewhere
        // in the defining file. Alias resolution has already expanded aliases.
        // Name postings are only a prefilter; declarations are checked by AST.
        let part = module.rsplit('.').next().unwrap_or(module);
        let materialization = (part.to_string(), name.to_string());
        if !self
            .request_cache
            .as_ref()
            .is_some_and(|cache| cache.materialized_exports.contains(&materialization))
        {
            let candidates = {
                let mut s=self.db.prepare("SELECT p.path FROM occurrences m CROSS JOIN occurrences n JOIN paths p ON p.id=m.file_id WHERE m.name=?2 AND n.name=?1 AND n.file_id=m.file_id ORDER BY p.path")?;
                let paths = s
                    .query_map(params![name, part], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                paths
            };
            for path in candidates {
                self.cancellation.check()?;
                self.counters.export_candidate_checks += 1;
                self.validate_disk(&path)?;
                if self.needs_summary(&path)? {
                    let parsed = self.parsed(&path)?;
                    anyhow::ensure!(
                        !(parsed.tokens.is_empty() && !parsed.supported),
                        "Candidate analysis unavailable: {}: {}",
                        path,
                        parsed.reasons.join("; ")
                    );
                }
            }
            // Publish domain coverage only after every candidate validated.
            if let Some(cache) = &mut self.request_cache {
                let bytes = materialization.0.len() + materialization.1.len() + 256;
                if cache.bytes + bytes <= 2 * 1024 * 1024 {
                    cache.materialized_exports.insert(materialization);
                    cache.bytes += bytes;
                }
            }
        }
        let mut modules = vec![module.to_string()];
        let mut at = 0;
        while at < modules.len() {
            let prefix = format!("{}.", modules[at]);
            let children = {
                let mut stmt = self.db.prepare(
                    "SELECT DISTINCT name FROM modules WHERE kind='autoopen' AND name LIKE ?1",
                )?;
                let result = stmt
                    .query_map([format!("{}%", prefix)], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                result
            };
            for child in children {
                if !child[prefix.len()..].contains('.') && !modules.contains(&child) {
                    modules.push(child)
                }
            }
            at += 1;
        }
        let mut result: Vec<(String, usize)> = Vec::new();
        let mut s = self.db.prepare_cached(
            "SELECT p.path,d.token FROM declarations d JOIN paths p ON p.id=d.file_id WHERE d.module=?1 AND d.name=?2 AND d.top_level=1",
        )?;
        for module in modules {
            result.extend(
                s.query_map(params![module, name], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            );
        }
        Ok(result)
    }
    pub fn declaration_proven(&self, path: &str, token: usize) -> Result<bool> {
        Ok(self
            .db
            .query_row(
                "SELECT proven FROM declarations WHERE file_id=(SELECT id FROM paths WHERE path=?1) AND token=?2 LIMIT 1",
                params![path, token],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false))
    }
    pub fn declaration_accessibility(&self, path: &str, token: usize) -> Result<(String, String)> {
        Ok(self.db.query_row("SELECT accessibility,coalesce(module,'') FROM declarations WHERE file_id=(SELECT id FROM paths WHERE path=?1) AND token=?2 LIMIT 1",params![path,token],|r|Ok((r.get(0)?,r.get(1)?)))?)
    }
    pub fn has_module(&self, name: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT 1 FROM modules WHERE name=?1 UNION SELECT 1 FROM files WHERE module=?1 LIMIT 1",[name],|_|Ok(())).optional()?.is_some())
    }
    pub fn has_answer(&self, key: &str, snapshot: &str) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM answers WHERE key=?1 AND snapshot=?2)",
            params![key, snapshot],
            |row| row.get(0),
        )?)
    }
    pub fn answer(&mut self, key: &str, snapshot: &str) -> Result<Option<serde_json::Value>> {
        let r: Option<String> = self
            .db
            .query_row(
                "SELECT result FROM answers WHERE key=?1 AND snapshot=?2",
                params![key, snapshot],
                |r| r.get(0),
            )
            .optional()?;
        if r.is_some() {
            self.counters.cache_hits += 1;
        }
        r.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    pub fn put_answer(&self, key: &str, snapshot: &str, value: &serde_json::Value) -> Result<()> {
        self.cancellation.check()?;
        self.db.execute(
            "INSERT OR REPLACE INTO answers(key,snapshot,result) VALUES(?1,?2,?3)",
            params![key, snapshot, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
    pub fn count(&self) -> Result<u64> {
        Ok(self
            .db
            .query_row("SELECT count(*) FROM files", [], |r| r.get(0))?)
    }
    pub fn generation(&self) -> Result<u64> {
        Ok(self
            .db
            .query_row("SELECT value FROM state WHERE key='generation'", [], |r| {
                r.get(0)
            })?)
    }
    pub fn disk_bytes(&self) -> u64 {
        [
            self.path.clone(),
            PathBuf::from(format!("{}-wal", self.path.display())),
        ]
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
    }
    pub fn retain_paths(&mut self, paths: &[String]) -> Result<()> {
        let mut stmt = self
            .db
            .prepare("SELECT p.path FROM files f JOIN paths p ON p.id=f.file_id")?;
        let existing = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        self.cancellation.check()?;
        let set: std::collections::HashSet<&String> = paths.iter().collect();
        for p in existing {
            self.cancellation.check()?;
            if !set.contains(&p) {
                self.remove(&p)?;
            }
        }
        Ok(())
    }
    pub fn import_semantic(
        &mut self,
        value: &serde_json::Value,
        snapshot: &str,
        expected_projects: &[String],
    ) -> Result<u64> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        self.cancellation.check()?;
        anyhow::ensure!(
            value.get("kind").and_then(serde_json::Value::as_str) == Some("semantic-snapshot"),
            "Not a semantic snapshot export"
        );
        anyhow::ensure!(
            value
                .get("factsVersion")
                .and_then(serde_json::Value::as_u64)
                == Some(2),
            "Semantic snapshot facts version is unsupported; re-export with the current helper"
        );
        let projects = value
            .get("projects")
            .and_then(serde_json::Value::as_array)
            .context("Missing semantic projects")?;
        let mut fingerprints = Vec::new();
        let _scope = self.cancellation.enter();
        let mut memberships = Vec::<crate::project::SourceMembership>::new();
        let mut environments = std::collections::BTreeSet::new();
        let mut membership_keys = HashSet::new();
        let mut complete = true;
        let mut seen = std::collections::HashSet::new();
        for project in projects {
            self.cancellation.check()?;
            if let Some(args) = project.get("compilerArgs") {
                anyhow::ensure!(
                    args.as_array().is_some_and(|args| args.iter().all(|arg| {
                        arg.as_str().is_some_and(crate::project::modeled_compiler_argument)
                    })),
                    "Semantic snapshot includes an unmodeled compiler input; re-export with explicit source arguments"
                );
            }
            let environment = project.get("environmentCertificate").context(
                "Semantic snapshot lacks evaluated environment provenance; re-export it",
            )?;
            crate::project::environment::validate(environment)?;
            environments.insert(serde_json::to_string(environment)?);
            anyhow::ensure!(
                project
                    .get("membershipReusable")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true),
                "Semantic snapshot lacks reusable membership provenance; re-export it"
            );
            let domains: Vec<crate::project::SourceMembership> = serde_json::from_value(
                project
                    .get("sourceMembership")
                    .context("Semantic snapshot lacks membership domains")?
                    .clone(),
            )?;
            for domain in domains {
                self.cancellation.check()?;
                domain.validate()?;
                if membership_keys.insert(serde_json::to_string(&domain)?) {
                    memberships.push(domain);
                }
            }
            if let Some(p) = project
                .get("projectPath")
                .and_then(serde_json::Value::as_str)
            {
                seen.insert(p.to_string());
            }
            complete &= project
                .get("completeness")
                .and_then(serde_json::Value::as_str)
                == Some("exact");
            for key in ["sourceHashes", "inputHashes"] {
                for input in project
                    .get(key)
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    self.cancellation.check()?;
                    let path = input
                        .get("path")
                        .and_then(serde_json::Value::as_str)
                        .context("Missing fingerprint path")?;
                    let expected = input
                        .get("sha256")
                        .and_then(serde_json::Value::as_str)
                        .context("Missing SHA256")?;
                    if expected == "directory" {
                        anyhow::ensure!(
                            key == "inputHashes" && Path::new(path).is_dir(),
                            "Semantic snapshot expected an existing input directory: {path}"
                        );
                        fingerprints.push((
                            path.to_string(),
                            0,
                            0,
                            false,
                            "directory".to_string(),
                            "directory".to_string(),
                        ));
                        continue;
                    }
                    if expected == "missing" {
                        anyhow::ensure!(
                            fs::metadata(path)
                                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                            "Semantic snapshot expected an absent input: {path}"
                        );
                        fingerprints.push((
                            path.to_string(),
                            0,
                            0,
                            true,
                            String::new(),
                            String::new(),
                        ));
                        continue;
                    }
                    let identity = disk_identity(path)?;
                    crate::foreground::file(identity.len)?;
                    let mut f = fs::File::open(path)?.take(identity.len.saturating_add(1));
                    let mut hash = Sha256::new();
                    let mut buf = [0u8; 65536];
                    let mut size = 0;
                    loop {
                        self.cancellation.check()?;
                        let n = f.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        hash.update(&buf[..n]);
                        size += n as u64;
                    }
                    self.counters.files_read += 1;
                    self.counters.bytes_read += size;
                    anyhow::ensure!(
                        format!("{:x}", hash.finalize()).eq_ignore_ascii_case(expected),
                        "Semantic snapshot is stale: {path}"
                    );
                    anyhow::ensure!(
                        disk_identity(path)? == identity,
                        "Semantic input changed during certification: {path}"
                    );
                    fingerprints.push((
                        path.to_string(),
                        identity.modified,
                        identity.len,
                        false,
                        serde_json::to_string(&identity)?,
                        expected.to_string(),
                    ));
                }
            }
        }
        complete &= expected_projects.iter().all(|p| seen.contains(p));
        complete &= value
            .get("workspaceProjects")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .all(|p| seen.contains(p));
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM semantic_occurrences", [])?;
        tx.execute("DELETE FROM semantic_meta", [])?;
        tx.execute("DELETE FROM semantic_inputs", [])?;
        let mut count = 0;
        {
            let mut insert =
                tx.prepare("INSERT INTO semantic_occurrences VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)")?;
            for project in projects {
                self.cancellation.check()?;
                if project
                    .get("completeness")
                    .and_then(serde_json::Value::as_str)
                    != Some("exact")
                {
                    self.cancellation.check()?;
                    continue;
                }
                let context = project
                    .get("contextId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                for o in project
                    .get("occurrences")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    self.cancellation.check()?;
                    let symbol = o
                        .get("symbolId")
                        .and_then(serde_json::Value::as_str)
                        .context("Missing symbol identity")?;
                    let path = o
                        .get("path")
                        .and_then(serde_json::Value::as_str)
                        .context("Missing occurrence path")?;
                    let r = &o["range"];
                    let declaration = serde_json::to_string(&o["declaration"])?;
                    insert.execute(params![
                        symbol,
                        path,
                        r["start"]["line"].as_i64().unwrap_or(0),
                        r["start"]["character"].as_i64().unwrap_or(0),
                        r["end"]["line"].as_i64().unwrap_or(0),
                        r["end"]["character"].as_i64().unwrap_or(0),
                        o["isDefinition"].as_bool().unwrap_or(false),
                        declaration,
                        context
                    ])?;
                    count += 1;
                }
            }
        }
        for (path, stamp, len, missing, identity, hash) in fingerprints {
            self.cancellation.check()?;
            tx.execute(
                "INSERT OR REPLACE INTO semantic_inputs(path,stamp,len,is_missing,identity,sha256) VALUES(?1,?2,?3,?4,?5,?6)",
                params![path, stamp, len, missing, identity,hash],
            )?;
        }
        tx.execute(
            "INSERT INTO semantic_meta VALUES('environments',?1)",
            [serde_json::to_string(&environments)?],
        )?;
        tx.execute(
            "INSERT INTO semantic_meta VALUES('memberships',?1)",
            [serde_json::to_string(&memberships)?],
        )?;
        tx.execute(
            "INSERT INTO semantic_meta VALUES('snapshot',?1)",
            [snapshot],
        )?;
        tx.execute(
            "INSERT INTO semantic_meta VALUES('complete',?1)",
            [if complete { "true" } else { "false" }],
        )?;
        self.cancellation.check()?;
        tx.commit()?;
        self.invalidate_request_cache();
        Ok(count)
    }
    pub fn semantic_answer(
        &mut self,
        path: &str,
        line: u32,
        col: u32,
        snapshot: &str,
        references: bool,
        include_decl: bool,
    ) -> Result<Option<serde_json::Value>> {
        use serde_json::{json, Value};
        self.cancellation.check()?;
        let saved: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM semantic_meta WHERE key='snapshot'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if saved.as_deref() != Some(snapshot) {
            return Ok(None);
        }
        let _scope = self.cancellation.enter();
        let environments: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM semantic_meta WHERE key='environments'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(environments) = environments else {
            return Ok(None);
        };
        for certificate in serde_json::from_str::<Vec<String>>(&environments)? {
            match crate::project::environment::validate(&serde_json::from_str::<Value>(
                &certificate,
            )?) {
                Ok(()) => {}
                Err(error) if error.is::<crate::cancellation::Cancelled>() => return Err(error),
                Err(_) => return Ok(None),
            }
        }
        let memberships: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM semantic_meta WHERE key='memberships'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(memberships) = memberships else {
            return Ok(None);
        };
        for domain in serde_json::from_str::<Vec<crate::project::SourceMembership>>(&memberships)? {
            match domain.validate() {
                Ok(()) => {}
                Err(error) if error.is::<crate::cancellation::Cancelled>() => return Err(error),
                Err(_) => return Ok(None),
            }
        }
        // The import certified contents. Metadata changes invalidate the entire
        // semantic export; no claim of semantic-interface early cutoff is made.
        let mut inputs = self
            .db
            .prepare("SELECT path,stamp,len,is_missing,identity,sha256 FROM semantic_inputs")?;
        let mut rows = inputs.query([])?;
        while let Some(r) = rows.next()? {
            self.cancellation.check()?;
            let path: String = r.get(0)?;
            let expected = (r.get::<_, u64>(1)?, r.get::<_, u64>(2)?);
            let missing: bool = r.get(3)?;
            let identity: Option<String> = r.get(4)?;
            let expected_hash: Option<String> = r.get(5)?;
            let directory = expected_hash.as_deref() == Some("directory");
            let unchanged = if directory {
                Path::new(&path).is_dir()
            } else if missing {
                fs::metadata(&path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            } else {
                let current = disk_identity(&path).ok();
                current.as_ref().is_some_and(|current| {
                    (current.modified, current.len) == expected
                        && serde_json::to_string(current).ok() == identity
                })
            };
            if !unchanged {
                return Ok(None);
            }
            // Platforms lacking Unix ctime/file identity cannot reuse a stat
            // certificate for contents. Explicit semantic mode rehashes there.
            #[cfg(not(unix))]
            if !missing && !directory {
                use sha2::{Digest, Sha256};
                use std::io::Read;
                let len = expected.1;
                let expected: Option<String> = r.get(5)?;
                crate::foreground::file(len)?;
                let mut file = fs::File::open(&path)?.take(len.saturating_add(1));
                let mut hash = Sha256::new();
                let mut buffer = [0u8; 65536];
                let mut size = 0;
                loop {
                    self.cancellation.check()?;
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    size += count as u64;
                    hash.update(&buffer[..count]);
                }
                self.counters.files_read += 1;
                self.counters.bytes_read += size;
                if !expected.is_some_and(|expected| {
                    format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&expected)
                }) {
                    return Ok(None);
                }
            }
        }
        drop(rows);
        drop(inputs);
        let mut statement=self.db.prepare("SELECT DISTINCT symbol,declaration,context FROM semantic_occurrences WHERE path=?1 AND start_line=?2 AND start_col<=?3 AND (end_line>?2 OR end_col>?3)")?;
        let hits = statement
            .query_map(params![path, line, col], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if hits.len() != 1 {
            return Ok(None);
        }
        let (symbol, declaration, _) = &hits[0];
        let locations = if references {
            let complete: String = self.db.query_row(
                "SELECT value FROM semantic_meta WHERE key='complete'",
                [],
                |r| r.get(0),
            )?;
            if complete != "true" {
                return Ok(None);
            }
            let mut stmt=self.db.prepare("SELECT DISTINCT path,start_line,start_col,end_line,end_col FROM semantic_occurrences WHERE symbol=?1 AND (?2 OR is_definition=0)")?;
            let locations=stmt.query_map(params![symbol,include_decl],|r|Ok(json!({"uri":project::uri(&r.get::<_,String>(0)?),"range":{"start":{"line":r.get::<_,u32>(1)?,"character":r.get::<_,u32>(2)?},"end":{"line":r.get::<_,u32>(3)?,"character":r.get::<_,u32>(4)?}}})))?.collect::<rusqlite::Result<Vec<_>>>()?;
            locations
        } else {
            let mut d: Value = serde_json::from_str(declaration)?;
            if d.is_null() {
                return Ok(None);
            }
            if let Some(path) = d.get("path").and_then(Value::as_str) {
                d["uri"] = json!(project::uri(path));
            }
            vec![d]
        };
        self.counters.cache_hits += 1;
        Ok(Some(
            json!({"locations":locations,"completeness":"exact","complete":true,"source":"persisted-semantic","diagnostics":[]}),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture_dir() -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("fsnav-index-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn candidate_source_revalidates_disk_and_never_replaces_a_dirty_overlay() {
        let dir = fixture_dir();
        let path = dir.join("A.fs").to_string_lossy().into_owned();
        let before = "module A\nlet target = 1\n";
        let after = "module A\nlet absent = 1\n";
        fs::write(&path, before).unwrap();
        let mut index = Index::open(&dir.join("cache"), 1, 1024).unwrap();
        index.search_name(&path, "target", true).unwrap();
        assert_eq!(index.candidate_source(&path).unwrap(), before);
        fs::write(&path, after).unwrap();
        // Validation refreshes the indexed row before exposing current bytes.
        assert_eq!(index.candidate_source(&path).unwrap(), after);
        index.search_name(&path, "absent", true).unwrap();
        assert_eq!(index.candidate_source(&path).unwrap(), after);
        index.overlay(&path, before, true).unwrap();
        assert!(index.candidate_source(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn candidate_path_cap_follows_domain_and_evidence_with_explicit_truncation() {
        let dir = fixture_dir();
        let index = Index::open(&dir.join("cache"), 1, 1024).unwrap();
        for i in 0..80 {
            let path = format!("/repo/{i:03}.fs");
            index
                .db
                .execute("INSERT INTO paths(path) VALUES (?1)", [&path])
                .unwrap();
            index.db.execute("INSERT INTO occurrences(name,file_id) SELECT 'target',id FROM paths WHERE path=?1", [&path]).unwrap();
        }
        let plan = index.db.prepare("EXPLAIN QUERY PLAN SELECT p.path FROM occurrences o JOIN paths p ON p.id=o.file_id WHERE o.name=?1 ORDER BY o.file_id LIMIT ?2")
            .unwrap().query_map(params!["target", 101], |row| row.get::<_, String>(3))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert!(
            !plan.iter().any(|step| step.contains("TEMP B-TREE")),
            "{plan:?}"
        );
        let found = index
            .ranked_candidate_paths("target", 2, 100, |path| {
                if path.ends_with("079.fs") {
                    Some(0)
                } else if path.ends_with("078.fs") {
                    Some(1)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(found.paths, ["/repo/079.fs", "/repo/078.fs"]);
        assert!(!found.truncated);
        assert_eq!((found.rows_examined, found.eligible_seen), (80, 2));
        let capped = index
            .ranked_candidate_paths("target", 2, 100, |_| Some(0))
            .unwrap();
        assert!(capped.truncated);
        assert_eq!(capped.paths, ["/repo/000.fs", "/repo/001.fs"]);
        let scan_capped = index
            .ranked_candidate_paths("target", 2, 40, |_| None)
            .unwrap();
        assert!(scan_capped.truncated);
        assert_eq!(scan_capped.rows_examined, 40);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn raw_postings_do_not_swallow_comments_or_unicode() {
        let names=raw_names("module Other\n// ``\nlet result = Main.value\n// ``\nlet cafe\u{301}=3\nlet ``snow man``=cafe\u{301}\n", &CancellationToken::new()).unwrap();
        assert!(names.contains("value"));
        assert!(names.contains("cafe\u{301}"));
        assert!(names.contains("snow man"));
    }

    #[test]
    fn restored_mtime_edits_invalidate_positive_negative_and_parsed_candidates() {
        let dir = fixture_dir();
        let source = dir.join("A.fs");
        let cache = dir.join("cache");
        let path = source.to_string_lossy().into_owned();
        let positive = "module A\nlet target = 1\n\n";
        let negative = "module A\nlet absent = 1\n\n";
        assert_eq!(positive.len(), negative.len());
        fs::write(&source, positive).unwrap();
        let modified = fs::metadata(&source).unwrap().modified().unwrap();
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            index.search_name(&path, "target", true).unwrap();
            assert_eq!(index.candidate_paths("target").unwrap(), vec![path.clone()]);
            assert!(index
                .parsed(&path)
                .unwrap()
                .tokens
                .iter()
                .any(|token| token.text == "target" && token.line == 1));
        }
        fs::write(&source, negative).unwrap();
        fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            assert_eq!(index.counters.files_read, 0);
            index.search_name(&path, "target", true).unwrap();
            assert!(index.candidate_paths("target").unwrap().is_empty());
        }
        // The persisted negative certificate must be validated after restart.
        fs::write(&source, positive).unwrap();
        fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            index.search_name(&path, "target", true).unwrap();
            assert_eq!(index.candidate_paths("target").unwrap(), vec![path.clone()]);
            index.parsed(&path).unwrap();
            let shifted = "module A\n\nlet target = 1\n";
            assert_eq!(shifted.len(), positive.len());
            fs::write(&source, shifted).unwrap();
            fs::File::options()
                .write(true)
                .open(&source)
                .unwrap()
                .set_modified(modified)
                .unwrap();
            // The same-session Unix identity check also sees restored-mtime edits.
            assert!(index
                .parsed(&path)
                .unwrap()
                .tokens
                .iter()
                .any(|token| token.text == "target" && token.line == 2));
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cancelled_update_preserves_committed_postings_and_can_retry() {
        let dir = fixture_dir();
        let mut index = Index::open(&dir, 1, 1024 * 1024).unwrap();
        let path = dir.join("A.fs").to_string_lossy().into_owned();
        index
            .overlay(&path, "module A\nlet first = 1\n", true)
            .unwrap();
        let generation = index.generation().unwrap();
        let token = CancellationToken::new();
        token.cancel();
        index.set_cancellation(token);
        assert!(index
            .overlay(&path, "module A\nlet second = 2\n", true)
            .unwrap_err()
            .is::<crate::cancellation::Cancelled>());
        assert_eq!(index.candidate_paths("first").unwrap(), vec![path.clone()]);
        assert!(index.candidate_paths("second").unwrap().is_empty());
        assert_eq!(index.generation().unwrap(), generation);
        index.set_cancellation(CancellationToken::new());
        index
            .overlay(&path, "module A\nlet second = 2\n", true)
            .unwrap();
        assert_eq!(index.candidate_paths("second").unwrap(), vec![path]);
        drop(index);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn semantic_negative_input_fingerprint_invalidates_when_file_appears() {
        use serde_json::json;
        use sha2::{Digest, Sha256};
        let dir = fixture_dir();
        let source = dir.join("A.fs");
        let missing = dir.join("Directory.Build.props");
        let guard_directory = dir.join("conditional-directory");
        fs::create_dir(&guard_directory).unwrap();
        let text = "module A\nlet value = 1\n";
        fs::write(&source, text).unwrap();
        let project = dir.join("A.fsproj").to_string_lossy().into_owned();
        let path = source.to_string_lossy().into_owned();
        let mut membership = crate::project::SourceMembership {
            root: dir.to_string_lossy().into_owned(),
            recursive: false,
            extensions: vec!["fs".into()],
            excluded_directories: vec![],
            sha256: String::new(),
            entries: 0,
        };
        (membership.sha256, membership.entries) = membership.current().unwrap();
        let declaration = json!({"path":path,"range":{"start":{"line":1,"character":4},"end":{"line":1,"character":9}}});
        let snapshot = json!({"kind":"semantic-snapshot","factsVersion":2,"projects":[{"projectPath":project,"environmentCertificate":crate::project::environment::empty_certificate(),"membershipReusable":true,"sourceMembership":[membership],"completeness":"exact","contextId":"context","sourceHashes":[{"path":path,"sha256":format!("{:x}",Sha256::digest(text.as_bytes()))}],"inputHashes":[{"path":missing,"sha256":"missing"},{"path":guard_directory,"sha256":"directory"}],"occurrences":[{"symbolId":"value","path":path,"range":declaration["range"],"isDefinition":true,"declaration":declaration}]}]});
        let mut index = Index::open(&dir.join("cache"), 1, 1024 * 1024).unwrap();
        let mut missing_environment = snapshot.clone();
        missing_environment["projects"][0]
            .as_object_mut()
            .unwrap()
            .remove("environmentCertificate");
        assert!(index
            .import_semantic(&missing_environment, "snapshot", &[project.clone()])
            .unwrap_err()
            .to_string()
            .contains("environment provenance"));
        let mut obsolete = snapshot.clone();
        obsolete["factsVersion"] = json!(1);
        assert!(index
            .import_semantic(&obsolete, "snapshot", &[project.clone()])
            .unwrap_err()
            .to_string()
            .contains("facts version"));
        index
            .import_semantic(&snapshot, "snapshot", &[project.clone()])
            .unwrap();
        assert!(index
            .semantic_answer(&path, 1, 5, "snapshot", false, true)
            .unwrap()
            .is_some());
        // A persisted semantic snapshot independently retains and checks its
        // environment certificate, even before a Workspace model is consulted.
        let mut stale_environment = crate::project::environment::empty_certificate();
        stale_environment["inputs"] = json!([{"name":"PATH","sha256":"not-the-current-path-hash"}]);
        index
            .db
            .execute(
                "UPDATE semantic_meta SET value=?1 WHERE key='environments'",
                [
                    serde_json::to_string(
                        &vec![serde_json::to_string(&stale_environment).unwrap()],
                    )
                    .unwrap(),
                ],
            )
            .unwrap();
        assert!(index
            .semantic_answer(&path, 1, 5, "snapshot", true, true)
            .unwrap()
            .is_none());
        index
            .import_semantic(&snapshot, "snapshot", &[project.clone()])
            .unwrap();
        let added = dir.join("Added.fs");
        fs::write(&added, "module Added\nlet useValue=A.value\n").unwrap();
        assert!(
            index
                .semantic_answer(&path, 1, 5, "snapshot", true, true)
                .unwrap()
                .is_none(),
            "New glob source cannot reuse a complete old reference set"
        );
        fs::remove_file(added).unwrap();
        assert!(index
            .semantic_answer(&path, 1, 5, "snapshot", false, true)
            .unwrap()
            .is_some());
        fs::remove_dir(&guard_directory).unwrap();
        assert!(index
            .semantic_answer(&path, 1, 5, "snapshot", false, true)
            .unwrap()
            .is_none());
        fs::write(&guard_directory, "not a directory").unwrap();
        assert!(index
            .semantic_answer(&path, 1, 5, "snapshot", false, true)
            .unwrap()
            .is_none());
        fs::remove_file(&guard_directory).unwrap();
        fs::create_dir(&guard_directory).unwrap();
        assert!(index
            .semantic_answer(&path, 1, 5, "snapshot", false, true)
            .unwrap()
            .is_some());
        fs::write(&missing, "<Project />").unwrap();
        assert!(index
            .semantic_answer(&path, 1, 5, "snapshot", false, true)
            .unwrap()
            .is_none());
        assert!(index
            .import_semantic(&snapshot, "snapshot", &[project])
            .unwrap_err()
            .to_string()
            .contains("absent input"));
        fs::remove_file(&missing).unwrap();
        assert!(index
            .semantic_answer(&path, 1, 5, "snapshot", false, true)
            .unwrap()
            .is_some());
        let modified = fs::metadata(&source).unwrap().modified().unwrap();
        fs::write(&source, text.replace("value", "other")).unwrap();
        fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert!(
            index
                .semantic_answer(&path, 1, 5, "snapshot", false, true)
                .unwrap()
                .is_none(),
            "Restored mtime is not proof of unchanged semantic inputs"
        );
        drop(index);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn compact_postings_preserve_candidates_and_lazy_summaries_across_reopen() {
        let dir = fixture_dir();
        let a = dir.join("long-workspace-directory-with-repeated-project-prefix-A.fs");
        let b = dir.join("B.fs");
        fs::write(&a, "module A\nlet value = 1\nlet reuse = value + value\n").unwrap();
        fs::write(&b, "module B\nlet value = 2\n").unwrap();
        let a = a.to_string_lossy().into_owned();
        let b = b.to_string_lossy().into_owned();
        let cache = dir.join("cache");
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            assert!(index.sync_disk(&a, true, false).unwrap());
            assert!(index.sync_disk(&b, true, false).unwrap());
            let mut expected = vec![a.clone(), b.clone()];
            expected.sort();
            assert_eq!(index.candidate_paths("value").unwrap(), expected);
            assert_eq!(index.counters.files_parsed, 0);
            assert!(index.needs_summary(&a).unwrap());
            let exported = index.exported("A", "value").unwrap();
            assert_eq!(exported.len(), 1);
            assert_eq!(exported[0].0, a);
            assert!(index.declaration_proven(&a, exported[0].1).unwrap());
            assert_eq!(
                index
                    .declaration_accessibility(&a, exported[0].1)
                    .unwrap()
                    .1,
                "A"
            );
            assert!(index.has_module("A").unwrap());
            assert_eq!(index.counters.files_parsed, 1);
            assert!(index.needs_summary(&b).unwrap());
        }
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            assert_eq!(index.count().unwrap(), 2);
            assert!(!index.sync_disk(&a, true, false).unwrap());
            assert_eq!(
                index.counters.files_read, 1,
                "First demand after reopening verifies contents"
            );
            assert!(!index.sync_disk(&a, true, false).unwrap());
            assert_eq!(
                index.counters.files_read,
                if cfg!(unix) { 1 } else { 2 },
                "Unchanged Unix identity reuses session validation"
            );
            assert_eq!(index.exported("A", "value").unwrap().len(), 1);
            assert_eq!(index.counters.files_parsed, 0);
            index
                .overlay(&a, "module A\nlet changed = 3\n", true)
                .unwrap();
            assert_eq!(index.candidate_paths("value").unwrap(), vec![b.clone()]);
            assert_eq!(index.candidate_paths("changed").unwrap(), vec![a.clone()]);
            assert_eq!(index.counters.files_parsed, 0);
            index
                .materialize_overlay(&a, "module A\nlet changed = 3\n", true)
                .unwrap();
            assert_eq!(index.exported("A", "changed").unwrap().len(), 1);
            index.remove(&a).unwrap();
            assert!(!index.has_module("A").unwrap());
            assert!(index.candidate_paths("changed").unwrap().is_empty());
            assert!(index
                .declaration_candidates("changed", 2)
                .unwrap()
                .is_empty());
            assert_eq!(index.count().unwrap(), 1);
            index.retain_paths(&[]).unwrap();
            assert!(index.candidate_paths("value").unwrap().is_empty());
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn negative_coverage_and_legacy_cache_do_not_alias_compact_file_ids() {
        let dir = fixture_dir();
        let source = dir.join("A.fs");
        let path = source.to_string_lossy().into_owned();
        fs::write(&source, "module A\nlet first = 1\n").unwrap();
        let cache = dir.join("cache");
        fs::create_dir(&cache).unwrap();
        let legacy = cache.join("index-v8.sqlite");
        fs::write(&legacy, b"legacy cache is not read or modified").unwrap();
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            assert_eq!(index.path.file_name().unwrap(), "index-v9.sqlite");
            index.search_name(&path, "missing", true).unwrap();
            assert_eq!(index.count().unwrap(), 0);
            assert_eq!(index.counters.files_read, 1);
            index.search_name(&path, "missing", true).unwrap();
            assert_eq!(index.counters.files_read, 1);
            fs::write(&source, "module A\nlet missing = 123456789\n").unwrap();
            index.search_name(&path, "missing", true).unwrap();
            assert_eq!(
                index.candidate_paths("missing").unwrap(),
                vec![path.clone()]
            );
            assert_eq!(index.count().unwrap(), 1);
            assert_eq!(index.counters.files_parsed, 0);
            index.remove(&path).unwrap();
            assert_eq!(index.count().unwrap(), 0);
            index.search_name(&path, "missing", true).unwrap();
            assert_eq!(index.candidate_paths("missing").unwrap(), vec![path]);
        }
        assert_eq!(
            fs::read(&legacy).unwrap(),
            b"legacy cache is not read or modified"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn reopened_index_validates_saved_bytes_instead_of_old_overlay() {
        let dir = fixture_dir();
        let cache = dir.join("cache");
        let path = dir.join("A.fs").to_string_lossy().into_owned();
        fs::write(&path, "module A\nlet saved = 1\n").unwrap();
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            index
                .materialize_overlay(&path, "module A\n\nlet unsaved = 2\n", true)
                .unwrap();
            assert!(index
                .parsed(&path)
                .unwrap()
                .declarations
                .iter()
                .any(|d| d.name == "unsaved"));
        }
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            assert_eq!(index.counters.files_read, 0);
            let parsed = index.parsed(&path).unwrap();
            assert!(parsed.declarations.iter().any(|d| d.name == "saved"));
            assert!(!parsed.declarations.iter().any(|d| d.name == "unsaved"));
            assert!(index.candidate_paths("unsaved").unwrap().is_empty());
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn previous_never_saved_buffers_do_not_become_phantom_sources() {
        let dir = fixture_dir();
        let cache = dir.join("cache");
        let path = dir.join("NeverSaved.fs").to_string_lossy().into_owned();
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            index
                .materialize_overlay(&path, "module NeverSaved\nlet phantom = 1\n", true)
                .unwrap();
            assert_eq!(
                index.candidate_paths("phantom").unwrap(),
                vec![path.clone()]
            );
        }
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            assert_eq!(index.counters.files_read, 0);
            assert!(index.candidate_paths("phantom").unwrap().is_empty());
            assert_eq!(index.count().unwrap(), 0);
            assert!(
                index.sync_disk(&path, true, false).is_err(),
                "Real missing Compile inputs still produce an error"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn changed_engine_fingerprint_discards_persisted_proofs_and_summaries() {
        let dir = fixture_dir();
        let cache = dir.join("cache");
        let path = dir.join("A.fs").to_string_lossy().into_owned();
        fs::write(&path, "module A\nlet value = 1\n").unwrap();
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            index.sync_disk(&path, true, false).unwrap();
            index.parsed(&path).unwrap();
            index
                .put_answer(
                    "proof",
                    "snapshot",
                    &serde_json::json!({"completeness":"exact","locations":["old-wrong-answer"]}),
                )
                .unwrap();
            index
                .db
                .execute(
                    "UPDATE state SET value='previous-semantic-algorithm' WHERE key='engine'",
                    [],
                )
                .unwrap();
        }
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            assert_eq!(index.count().unwrap(), 0);
            assert_eq!(index.counters.files_read, 0);
            assert!(index.answer("proof", "snapshot").unwrap().is_none());
            assert!(index.candidate_paths("value").unwrap().is_empty());
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn contextual_summaries_keep_branch_identities_and_overlay_ownership() {
        let dir = fixture_dir();
        let cache = dir.join("cache");
        let path = dir.join("Shared.fs").to_string_lossy().into_owned();
        let source="module Shared\n#if A\nlet selected = 1\n#else\nlet selected = 2\n#endif\nlet useSelected = selected\n";
        fs::write(&path, source).unwrap();
        let context = |name: &str| {
            SourceContext::new(
                &format!("/{name}.fsproj"),
                vec![name.into()],
                "net10.0",
                "Debug",
                &["--langversion:preview".into()],
                true,
            )
        };
        let a = context("A");
        let b = context("B");
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            index.sync_disk(&path, true, false).unwrap();
            assert_eq!(index.counters.files_parsed, 0);
            for (ctx, line) in [(&a, 2), (&b, 4)] {
                let parsed = index.parsed_in(&path, ctx).unwrap();
                let d = parsed
                    .declarations
                    .iter()
                    .find(|d| d.name == "selected")
                    .unwrap();
                assert_eq!(parsed.tokens[d.token].line, line);
                assert!(!index.needs_summary_in(&path, ctx).unwrap());
            }
            let dirty = source.replace("let selected = 1", "\nlet selected = 10");
            let parsed = index.materialize_overlay_in(&path, &dirty, &a).unwrap();
            let d = parsed
                .declarations
                .iter()
                .find(|d| d.name == "selected")
                .unwrap();
            assert_eq!(parsed.tokens[d.token].line, 3);
            assert!(
                index.parsed_in(&path, &b).is_err(),
                "Never use saved text for another context of a dirty buffer"
            );
            let parsed = index.materialize_overlay_in(&path, &dirty, &b).unwrap();
            let d = parsed
                .declarations
                .iter()
                .find(|d| d.name == "selected")
                .unwrap();
            assert_eq!(parsed.tokens[d.token].line, 5);
        }
        {
            let mut index = Index::open(&cache, 1, 1024 * 1024).unwrap();
            assert_eq!(index.counters.files_read, 0);
            for (ctx, line) in [(&a, 2), (&b, 4)] {
                let parsed = index.parsed_in(&path, ctx).unwrap();
                let d = parsed
                    .declarations
                    .iter()
                    .find(|d| d.name == "selected")
                    .unwrap();
                assert_eq!(parsed.tokens[d.token].line, line);
            }
            assert_eq!(
                index.counters.files_parsed, 2,
                "Unsaved context summaries must be replaced after restart"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn contextual_export_memo_is_domain_specific_and_invalidated_by_source_changes() {
        let dir = fixture_dir();
        let path = dir.join("Shared.fs").to_string_lossy().into_owned();
        let source = "module Shared\n#if A\nlet one = 1\n#else\nlet two = 2\n#endif\n";
        fs::write(&path, source).unwrap();
        let context = |name: &str| {
            SourceContext::new(
                &format!("/{name}.fsproj"),
                vec![name.into()],
                "",
                "",
                &[],
                true,
            )
        };
        let a = context("A");
        let b = context("B");
        let mut index = Index::open(&dir.join("cache"), 1, 1024 * 1024).unwrap();
        index.sync_disk(&path, true, false).unwrap();
        index.begin_request();
        assert_eq!(
            index
                .exported_in("Shared", "one", &[(path.clone(), a.clone())])
                .unwrap()
                .declarations
                .len(),
            1
        );
        assert!(index
            .exported_in("Shared", "one", &[(path.clone(), b.clone())])
            .unwrap()
            .declarations
            .is_empty());
        let hits = index.counters.index_hits;
        assert_eq!(
            index
                .exported_in("Shared", "one", &[(path.clone(), a.clone())])
                .unwrap()
                .declarations
                .len(),
            1
        );
        assert_eq!(index.counters.index_hits, hits);
        index
            .materialize_overlay_in(&path, &source.replace("let one", "let changed"), &a)
            .unwrap();
        assert!(index
            .exported_in("Shared", "one", &[(path.clone(), a.clone())])
            .unwrap()
            .declarations
            .is_empty());
        assert_eq!(
            index
                .exported_in("Shared", "changed", &[(path.clone(), a.clone())])
                .unwrap()
                .declarations
                .len(),
            1
        );
        assert!(index
            .exported_in("Shared", "changed", &[(path.clone(), a), (path.clone(), b)])
            .is_err());
        index.end_request();
        fs::remove_dir_all(dir).unwrap();
    }
}

// Suppress only this connection's SQLite files before watcher queue admission.
// Mixed-path events and rescan signals must still invalidate normally.
pub(crate) fn owned_file_event(event: &notify::Event, database: &Path) -> bool {
    !event.need_rescan()
        && !event.paths.is_empty()
        && event.paths.iter().all(|path| {
            path == database
                || ["-wal", "-shm", "-journal"].iter().any(|suffix| {
                    let mut sibling = database.as_os_str().to_os_string();
                    sibling.push(suffix);
                    path.as_os_str() == sibling
                })
        })
}

#[cfg(test)]
mod owned_event_tests {
    use super::*;
    #[test]
    fn sqlite_filter_does_not_hide_sources_mixed_events_or_rescan() {
        let database = Path::new("/repo/.storage/index.sqlite");
        let event = notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(PathBuf::from("/repo/.storage/index.sqlite-wal"));
        assert!(owned_file_event(&event, database));
        let mixed = event
            .clone()
            .add_path(PathBuf::from("/repo/.storage/Actual.fs"));
        assert!(!owned_file_event(&mixed, database));
        let rescan = event.set_flag(notify::event::Flag::Rescan);
        assert!(!owned_file_event(&rescan, database));
    }
}
