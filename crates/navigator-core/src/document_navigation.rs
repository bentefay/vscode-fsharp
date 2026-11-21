//! Navigation whose complete dependency set is one acknowledged document.
use crate::{
    cancellation::CancellationToken,
    local_lexical::{self, Operation, Position, Snapshot},
    project, Overlay,
};
use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::VecDeque, io, mem::size_of};

const MAX_ENTRIES: usize = 32;
const MAX_BYTES: usize = 128 * 1024;

#[derive(PartialEq, Eq, Serialize)]
struct Key {
    uri: Box<str>,
    source_hash: Box<str>,
    version: i64,
    at: Position,
    references: bool,
    include_declaration: bool,
    engine: &'static str,
}

impl Key {
    fn heap_bytes(&self) -> usize {
        self.uri.len() + self.source_hash.len()
    }

    fn proof_id(&self) -> Result<String> {
        let identity = (
            local_lexical::PROOF_VERSION,
            local_lexical::PARSER_FINGERPRINT,
            self,
        );
        Ok(blake3::hash(&serde_json::to_vec(&identity)?)
            .to_hex()
            .to_string())
    }
}

struct Entry {
    key: Key,
    response: Box<[u8]>,
}

#[derive(Default)]
struct AnswerCache {
    entries: VecDeque<Entry>,
}

struct LimitedBytes {
    buffer: Vec<u8>,
    limit: usize,
}

impl io::Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit - self.buffer.len() {
            return Err(io::Error::other("Document answer exceeds cache budget"));
        }
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AnswerCache {
    fn overhead(&self) -> usize {
        size_of::<Self>() + self.entries.capacity() * size_of::<Entry>()
    }

    fn bytes(&self) -> usize {
        self.overhead()
            + self
                .entries
                .iter()
                .map(|e| e.key.heap_bytes() + e.response.len())
                .sum::<usize>()
    }

    fn get(&mut self, key: &Key) -> Result<Option<Value>> {
        let Some(index) = self.entries.iter().position(|entry| &entry.key == key) else {
            return Ok(None);
        };
        let entry = self.entries.remove(index).unwrap();
        let answer = serde_json::from_slice(&entry.response)?;
        self.entries.push_back(entry);
        Ok(Some(answer))
    }

    fn insert(&mut self, key: Key, value: &Value) {
        if self.entries.capacity() == 0 {
            self.entries.reserve_exact(MAX_ENTRIES);
        }
        let Some(limit) = MAX_BYTES.checked_sub(self.overhead() + key.heap_bytes()) else {
            return;
        };
        let mut bytes = LimitedBytes {
            buffer: Vec::new(),
            limit,
        };
        if serde_json::to_writer(&mut bytes, value).is_err() {
            return;
        }
        let cost = key.heap_bytes() + bytes.buffer.len();
        while self.entries.len() == MAX_ENTRIES || self.bytes() + cost > MAX_BYTES {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            key,
            response: bytes.buffer.into_boxed_slice(),
        });
    }
}

#[derive(Default)]
pub(super) struct DocumentNavigation {
    cache: AnswerCache,
    attempts: u64,
    hits: u64,
    cache_hits: u64,
    parsed_bytes: u64,
    visited_nodes: u64,
    parse_micros: u64,
    proof_micros: u64,
}

impl DocumentNavigation {
    pub fn clear(&mut self) {
        self.cache.entries.clear();
    }

    pub fn invalidate(&mut self, path: &str) {
        let uri = project::uri(path);
        self.cache.entries.retain(|e| e.key.uri.as_ref() != uri);
    }

    pub fn query(
        &mut self,
        overlay: &Overlay,
        at: Position,
        references: bool,
        include_declaration: bool,
        token: &CancellationToken,
    ) -> Result<Option<Value>> {
        token.check()?;
        self.attempts += 1;
        let key = Key {
            uri: project::uri(&overlay.path).into_boxed_str(),
            source_hash: overlay.source_hash.as_str().into(),
            version: overlay.version,
            at,
            references,
            include_declaration,
            engine: env!("FSHARP_NAVIGATOR_ENGINE_FINGERPRINT"),
        };
        if let Some(answer) = self.cache.get(&key)? {
            token.check()?;
            self.hits += 1;
            self.cache_hits += 1;
            return Ok(Some(answer));
        }

        let snapshot = Snapshot {
            uri: &key.uri,
            source: &overlay.text,
            version: overlay.version,
            source_hash: &key.source_hash,
        };
        let operation = if references {
            Operation::References {
                include_declaration,
            }
        } else {
            Operation::Definition
        };
        let mut metrics = local_lexical::Metrics::default();
        let answer =
            local_lexical::query_with_metrics(snapshot, at, operation, token, &mut metrics);
        self.parsed_bytes += metrics.parsed_bytes as u64;
        self.visited_nodes += metrics.visited_nodes as u64;
        self.parse_micros += metrics.parse_elapsed_micros;
        self.proof_micros += metrics.elapsed_micros;
        let Some(answer) = answer? else {
            return Ok(None);
        };
        let has_locations = !answer.locations.is_empty();
        let result = json!({
            "completeness": "exact",
            "source": "local-lexical",
            "proofScope": "document",
            "documentVersion": answer.document_version,
            "version": answer.document_version,
            "sourceHash": answer.source_hash,
            "documentProofId": key.proof_id()?,
            "proof": {
                "algorithm": answer.proof_version,
                "parser": answer.parser_fingerprint,
                "engine": key.engine,
                "declaration": answer.declaration,
                "bodyStartByte": answer.body_start,
                "bodyEndByte": answer.body_end,
            },
            "locations": answer.locations,
        });
        token.check()?;
        if has_locations {
            self.cache.insert(key, &result);
        }
        token.check()?;
        self.hits += 1;
        Ok(Some(result))
    }

    pub fn stats(&self, target: &mut Value) {
        target["localProofAttempts"] = json!(self.attempts);
        target["localProofHits"] = json!(self.hits);
        target["localProofCacheHits"] = json!(self.cache_hits);
        target["localProofCacheEntries"] = json!(self.cache.entries.len());
        target["localProofCacheBytes"] = json!(self.cache.bytes());
        target["localProofParsedBytes"] = json!(self.parsed_bytes);
        target["localProofVisitedNodes"] = json!(self.visited_nodes);
        target["localProofParseMicros"] = json!(self.parse_micros);
        target["localProofMicros"] = json!(self.proof_micros);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Workspace;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn key(line: u32) -> Key {
        Key {
            uri: "file:///Main.fs".into(),
            source_hash: "a".repeat(64).into_boxed_str(),
            version: 1,
            at: Position { line, character: 0 },
            references: true,
            include_declaration: true,
            engine: env!("FSHARP_NAVIGATOR_ENGINE_FINGERPRINT"),
        }
    }

    #[test]
    fn answer_cache_enforces_lru_entry_and_byte_bounds() {
        let mut cache = AnswerCache::default();
        let small = json!({"locations":[]});
        for line in 0..MAX_ENTRIES as u32 {
            cache.insert(key(line), &small);
        }
        assert_eq!(cache.get(&key(0)).unwrap(), Some(small.clone()));
        cache.insert(key(32), &small);
        assert!(cache.get(&key(1)).unwrap().is_none());
        assert!(cache.get(&key(0)).unwrap().is_some());
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        let large = json!("x".repeat(MAX_BYTES / 2));
        cache.insert(key(33), &large);
        cache.insert(key(34), &large);
        assert_eq!(cache.entries.len(), 1);
        assert!(cache.bytes() <= MAX_BYTES);
        cache.insert(key(35), &json!("x".repeat(MAX_BYTES)));
        assert!(cache.get(&key(35)).unwrap().is_none());
        assert_eq!(cache.get(&key(34)).unwrap(), Some(large));
    }

    #[test]
    fn document_proof_cache_tracks_acknowledgements_and_live_snapshots() {
        let root = std::env::temp_dir().join(format!(
            "fsnav-local-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let source = "module M\r\nlet values = List.map (fun x -> x+x) [1]\r\n";
        let path = root.join("Main.fs");
        let other = root.join("Other.fs");
        let options = json!({"root":root,"evaluatedModelPath":root.join("missing-model.json")});
        let query =
            json!({"path":path,"line":1,"character":32,"version":1,"includeDeclaration":true});
        let refs = "navigation/references";
        let open = "document/open";
        let mut ws = Workspace::open(&options).unwrap();
        ws.dispatch(open, &json!({"path":path,"text":source,"version":1}))
            .unwrap();
        let first = ws.dispatch(refs, &query).unwrap();
        assert_eq!(first["source"], "local-lexical");
        assert_eq!(first["locations"].as_array().unwrap().len(), 3);
        let stats = ws.stats().unwrap();
        assert_eq!(stats["localProofParsedBytes"], source.len() as u64);
        assert_eq!(stats["localProofCacheEntries"], 1);
        assert_eq!(stats["queryCount"], 1);
        assert_eq!(stats["localProofCacheHits"], 0);
        assert!(ws
            .dispatch(
                "document/change",
                &json!({"path":path,"text":"stale","version":1})
            )
            .is_err());
        ws.dispatch(open, &json!({"path":other,"text":source,"version":1}))
            .unwrap();
        let warm = ws.dispatch(refs, &query).unwrap();
        assert_eq!(first["documentProofId"], warm["documentProofId"]);
        assert_ne!(first["snapshotId"], warm["snapshotId"]);
        assert_eq!(
            ws.stats().unwrap()["localProofParsedBytes"],
            source.len() as u64
        );
        assert_eq!(ws.stats().unwrap()["localProofCacheHits"], 1);
        for (method, field, value) in [
            (refs, "path", json!(other)),
            (refs, "includeDeclaration", json!(false)),
            (refs, "character", json!(34)),
            ("navigation/definition", "character", json!(32)),
        ] {
            let mut changed = query.clone();
            changed[field] = value;
            let result = ws.dispatch(method, &changed).unwrap();
            assert_ne!(first["documentProofId"], result["documentProofId"]);
            assert_eq!(result["source"], "local-lexical");
        }
        let mut invalid = query.clone();
        invalid["line"] = json!(1u64 << 32);
        assert!(ws.dispatch(refs, &invalid).is_err());
        assert_eq!(ws.stats().unwrap()["localProofCacheEntries"], 5);
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":source,"version":2}),
        )
        .unwrap();
        assert_eq!(ws.stats().unwrap()["localProofCacheEntries"], 1);
        assert!(ws.dispatch(refs, &query).is_err());
        let mut v2 = query.clone();
        v2["version"] = json!(2);
        let next = ws.dispatch(refs, &v2).unwrap();
        assert_eq!(first["sourceHash"], next["sourceHash"]);
        assert_ne!(first["documentProofId"], next["documentProofId"]);

        // A token can be cancelled after dispatch, before the daemon publishes.
        let token = CancellationToken::new();
        ws.set_cancellation(token.clone());
        ws.dispatch(refs, &v2).unwrap();
        token.cancel();
        ws.set_cancellation(CancellationToken::new());
        assert_eq!(ws.stats().unwrap()["localProofCacheEntries"], 0);
        ws.dispatch(refs, &v2).unwrap();
        let token = CancellationToken::new();
        token.cancel();
        ws.set_cancellation(token);
        assert!(ws
            .dispatch(refs, &v2)
            .unwrap_err()
            .is::<crate::cancellation::Cancelled>());
        ws.set_cancellation(CancellationToken::new());
        assert_eq!(ws.stats().unwrap()["localProofCacheEntries"], 0);

        ws.dispatch(refs, &v2).unwrap();
        ws.dispatch("document/close", &json!({"path":path}))
            .unwrap();
        assert!(ws.dispatch(refs, &v2).is_err());
        assert_eq!(ws.stats().unwrap()["localProofCacheEntries"], 0);
        let changed = source.replace("x+x", "x*x+x");
        ws.dispatch(open, &json!({"path":path,"version":1,"text":changed}))
            .unwrap();
        let reopened = ws.dispatch(refs, &query).unwrap();
        assert_eq!(reopened["locations"].as_array().unwrap().len(), 4);
        assert_ne!(reopened["sourceHash"], first["sourceHash"]);
        assert_ne!(reopened["documentProofId"], first["documentProofId"]);
        let stats = ws.stats().unwrap();
        for counter in [
            "filesRead",
            "filesParsed",
            "modelLoadMicros",
            "catalogLoadMicros",
            "helperStarts",
        ] {
            assert_eq!(stats[counter], 0, "{counter}");
        }
        assert_eq!(stats["watcherActive"], false);
        ws.dispatch("workspace/mode", &json!({"mode":"lexical"}))
            .unwrap();
        assert_eq!(ws.stats().unwrap()["localProofCacheEntries"], 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
}
