//! Navigation whose complete dependency set is one acknowledged document.
use crate::{
    cancellation::CancellationToken,
    local_lexical::{self, Operation, Position, Snapshot},
    project, Overlay,
};
use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    mem::size_of,
};

const MAX_ENTRIES: usize = 32;
const MAX_BYTES: usize = 128 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize)]
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
    compact_references: bool,
}

// Compact serialized positive reference answer, sharing invariant URI,
// version, hash and engine fields with its key. Defer provider JSON and proof
// ID formatting until a references request actually reads this entry.
#[derive(serde::Serialize, serde::Deserialize)]
struct SingleUseReferences {
    declaration: (u32, u32),
    usage: (u32, u32),
    identifier_len: u32,
    body_start: u32,
    body_end: u32,
}

impl SingleUseReferences {
    fn result(&self, key: &Key) -> Result<Value> {
        let range = |(line, character)| local_lexical::Range {
            start: Position { line, character },
            end: Position {
                line,
                character: character + self.identifier_len,
            },
        };
        // A lexical formal precedes its uses inside its body.
        let declaration = range(self.declaration);
        response(
            key,
            declaration,
            self.body_start as usize,
            self.body_end as usize,
            &[
                local_lexical::Location {
                    uri: key.uri.to_string(),
                    range: declaration,
                },
                local_lexical::Location {
                    uri: key.uri.to_string(),
                    range: range(self.usage),
                },
            ],
        )
    }
}

fn response(
    key: &Key,
    declaration: local_lexical::Range,
    body_start: usize,
    body_end: usize,
    locations: &[local_lexical::Location],
) -> Result<Value> {
    Ok(json!({
        "completeness": "exact",
        "source": "local-lexical",
        "proofScope": "document",
        "documentVersion": key.version,
        "version": key.version,
        "sourceHash": key.source_hash,
        "documentProofId": key.proof_id()?,
        "proof": {
            "algorithm": local_lexical::PROOF_VERSION,
            "parser": local_lexical::PARSER_FINGERPRINT,
            "engine": key.engine,
            "declaration": declaration,
            "bodyStartByte": body_start,
            "bodyEndByte": body_end,
        },
        "locations": locations,
    }))
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
        let answer = if entry.compact_references {
            postcard::from_bytes::<SingleUseReferences>(&entry.response)?.result(key)?
        } else {
            serde_json::from_slice(&entry.response)?
        };
        self.entries.push_back(entry);
        Ok(Some(answer))
    }

    fn insert(&mut self, key: Key, value: &Value) -> bool {
        if self.entries.capacity() == 0 {
            self.entries.reserve_exact(MAX_ENTRIES);
        }
        let Some(limit) = MAX_BYTES.checked_sub(self.overhead() + key.heap_bytes()) else {
            return false;
        };
        let mut bytes = LimitedBytes {
            buffer: Vec::new(),
            limit,
        };
        if serde_json::to_writer(&mut bytes, value).is_err() {
            return false;
        }
        let cost = key.heap_bytes() + bytes.buffer.len();
        while self.entries.len() == MAX_ENTRIES || self.bytes() + cost > MAX_BYTES {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            key,
            response: bytes.buffer.into_boxed_slice(),
            compact_references: false,
        });
        true
    }

    fn insert_single_use(&mut self, answer: &local_lexical::LocalAnswer) {
        let Some(usage) = answer.sole_use else { return };
        // Opportunistic entries use spare space only and are first to evict.
        // Bound spare key allocation independently of the requested response.
        let Some(primary) = self.entries.back() else {
            return;
        };
        let definition = &primary.key;
        if definition.uri.len() > 512 || self.entries.len() >= MAX_ENTRIES {
            return;
        }
        let declaration = answer.declaration;
        let packed = SingleUseReferences {
            declaration: (declaration.start.line, declaration.start.character),
            usage: (usage.start.line, usage.start.character),
            identifier_len: declaration.end.character - declaration.start.character,
            body_start: answer.body_start as u32,
            body_end: answer.body_end as u32,
        };
        let Ok(bytes) = postcard::to_allocvec(&packed) else {
            return;
        };
        if self.bytes() + definition.heap_bytes() + bytes.len() > MAX_BYTES {
            return;
        }
        let key = Key {
            references: true,
            include_declaration: true,
            ..definition.clone()
        };
        if self.entries.iter().any(|entry| entry.key == key) {
            return;
        }
        self.entries.push_front(Entry {
            key,
            response: bytes.into_boxed_slice(),
            compact_references: true,
        });
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LastAttempt {
    attempt: u64,
    document_version: i64,
    source_hash: String,
    position: Position,
    operation: &'static str,
    outcome: &'static str,
    parse_micros: u64,
    total_micros: u64,
    visited_nodes: usize,
    parsed_bytes: usize,
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
    outcomes: BTreeMap<&'static str, u64>,
    last_attempt: Option<LastAttempt>,
}

impl DocumentNavigation {
    pub fn clear(&mut self) {
        self.cache.entries.clear();
    }

    pub fn invalidate(&mut self, path: &str) -> usize {
        let before = self.cache.entries.len();
        let uri = project::uri(path);
        self.cache.entries.retain(|e| e.key.uri.as_ref() != uri);
        before - self.cache.entries.len()
    }

    fn record_attempt(
        &mut self,
        overlay: &Overlay,
        at: Position,
        references: bool,
        metrics: &local_lexical::Metrics,
    ) {
        let outcome = metrics.outcome.unwrap_or("unsupported");
        *self.outcomes.entry(outcome).or_default() += 1;
        let attempt = self.last_attempt.get_or_insert_with(|| LastAttempt {
            attempt: 0,
            document_version: 0,
            source_hash: String::with_capacity(64),
            position: at,
            operation: "definition",
            outcome,
            parse_micros: 0,
            total_micros: 0,
            visited_nodes: 0,
            parsed_bytes: 0,
        });
        attempt.attempt = self.attempts;
        attempt.document_version = overlay.version;
        attempt.source_hash.clone_from(&overlay.source_hash);
        attempt.position = at;
        attempt.operation = if references {
            "references"
        } else {
            "definition"
        };
        attempt.outcome = outcome;
        attempt.parse_micros = metrics.parse_elapsed_micros;
        attempt.total_micros = metrics.elapsed_micros;
        attempt.visited_nodes = metrics.visited_nodes;
        attempt.parsed_bytes = metrics.parsed_bytes;
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
            self.record_attempt(
                overlay,
                at,
                references,
                &local_lexical::Metrics {
                    outcome: Some("cache-hit"),
                    ..Default::default()
                },
            );
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
        self.record_attempt(overlay, at, references, &metrics);
        let Some(answer) = answer? else {
            return Ok(None);
        };
        let has_locations = !answer.locations.is_empty();
        let result = response(
            &key,
            answer.declaration,
            answer.body_start,
            answer.body_end,
            &answer.locations,
        )?;
        token.check()?;
        if has_locations && self.cache.insert(key, &result) && answer.sole_use.is_some() {
            self.cache.insert_single_use(&answer);
        }
        token.check()?;
        self.hits += 1;
        Ok(Some(result))
    }

    pub fn attempt_count(&self) -> u64 {
        self.attempts
    }

    pub fn last_attempt(&self) -> Value {
        json!(self
            .last_attempt
            .as_ref()
            .filter(|attempt| attempt.attempt == self.attempts))
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
        target["localProofOutcomes"] = json!(self.outcomes);
        target["localProofLastAttempt"] = json!(self.last_attempt);
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

    fn overlay(text: String) -> Overlay {
        Overlay {
            path: "/Main.fs".into(),
            source_hash: blake3::hash(text.as_bytes()).to_hex().to_string(),
            version: 1,
            text,
        }
    }

    #[test]
    fn first_references_after_definition_reuse_only_the_proved_single_use() {
        let source =
            overlay("module M\r\nlet pair=(\"😀\", List.map (fun param -> param) [1])\r\n".into());
        let line = source.text.lines().nth(1).unwrap();
        let start = line[..line.rfind("param").unwrap()].encode_utf16().count() as u32;
        let token = CancellationToken::new();
        for offset in [0, 2, 5] {
            let at = Position {
                line: 1,
                character: start + offset,
            };
            let mut nav = DocumentNavigation::default();
            let expected = DocumentNavigation::default()
                .query(&source, at, true, true, &token)
                .unwrap()
                .unwrap();
            let definition = nav
                .query(&source, at, false, false, &token)
                .unwrap()
                .unwrap();
            assert_eq!(nav.cache.entries.len(), 2);
            assert!(nav.cache.bytes() <= MAX_BYTES);
            // The maintained varint codec must stay within the former
            // fixed-width budget; the full reference oracle below is unchanged.
            assert!(nav.cache.entries.front().unwrap().response.len() <= 28);
            let parsed = nav.parsed_bytes;
            let result = nav.query(&source, at, true, true, &token).unwrap().unwrap();
            assert_eq!(result, expected);
            assert_ne!(definition["documentProofId"], result["documentProofId"]);
            assert_eq!(nav.parsed_bytes, parsed);
            assert_eq!(nav.cache_hits, 1);
            let use_only = nav
                .query(&source, at, true, false, &token)
                .unwrap()
                .unwrap();
            assert_eq!(use_only["locations"].as_array().unwrap().len(), 1);
            assert_eq!(use_only["locations"][0], expected["locations"][1]);
            assert_ne!(use_only["documentProofId"], result["documentProofId"]);
            assert_eq!(nav.parsed_bytes, parsed * 2);
            nav.invalidate(&source.path);
            assert!(nav.cache.entries.is_empty());
        }
    }

    #[test]
    fn singleton_array_definition_does_not_seed_compact_references() {
        let source = overlay("module M\nlet f items =\n    match items with\n    | [| part |] -> part\n    | _ -> 0\n".into());
        let mut nav = DocumentNavigation::default();
        let token = CancellationToken::new();
        let at = Position {
            line: 3,
            character: 20,
        };
        let answer = nav
            .query(&source, at, false, true, &token)
            .unwrap()
            .unwrap();
        assert_eq!(answer["completeness"], "exact");
        assert_eq!(nav.cache.entries.len(), 1);
        assert!(nav
            .cache
            .entries
            .iter()
            .all(|e| !e.compact_references && !e.key.references));
        let refs = nav.query(&source, at, true, true, &token).unwrap().unwrap();
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2);
        assert!(nav.cache.entries.iter().all(|e| !e.compact_references));
    }

    #[test]
    fn unused_formals_multiple_uses_and_long_uris_never_seed_reference_entries() {
        let token = CancellationToken::new();
        for (body, position, long_uri) in [
            ("param", "fun ", false),
            ("param", "-> ", true),
            ("0", "fun ", false),
            ("param+param", "-> ", false),
        ] {
            let mut source = overlay(format!(
                "module M\nlet value = List.map (fun param -> {body}) [1]\n"
            ));
            let line = source.text.lines().nth(1).unwrap();
            let at = Position {
                line: 1,
                character: (line.find(position).unwrap() + position.len()) as u32,
            };
            if long_uri {
                source.path = format!("/{}.fs", "x".repeat(520));
            }
            let mut nav = DocumentNavigation::default();
            let result = nav
                .query(&source, at, false, true, &token)
                .unwrap()
                .unwrap();
            assert_eq!(result["completeness"], "exact");
            assert_eq!(nav.cache.entries.len(), 1);
            assert!(!nav.cache.entries.front().unwrap().compact_references);
        }
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
