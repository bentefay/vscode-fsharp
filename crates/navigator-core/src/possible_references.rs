//! Explicit, bounded spelling occurrences. This module never establishes binding
//! identity or a complete reference set and never populates a proof cache.
use super::*;
use std::fs;

const MAX_FILES: usize = 32;
const MAX_ROWS: usize = 4096;
const MAX_LOCATIONS: usize = 200;
const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;

impl Workspace {
    fn occurrence_source(&mut self, path: &str) -> Result<(String, Value)> {
        self.cancellation.check()?;
        let (source, version) = if let Some(overlay) = self.overlays.get(path) {
            anyhow::ensure!(
                overlay.text.len() <= MAX_SOURCE_BYTES,
                "Occurrence source exceeds file limit"
            );
            (overlay.text.to_string(), Some(overlay.version))
        } else {
            anyhow::ensure!(
                fs::metadata(path)?.len() <= MAX_SOURCE_BYTES as u64,
                "Occurrence source exceeds file limit"
            );
            self.ensure_path(path)?;
            (self.index.candidate_source(path)?, None)
        };
        anyhow::ensure!(
            source.len() <= MAX_SOURCE_BYTES,
            "Occurrence source exceeds file limit"
        );
        let identity = json!({"uri":uri(path),"sourceHash":blake3::hash(source.as_bytes()).to_hex().to_string(),"version":version});
        Ok((source, identity))
    }

    pub(super) fn possible_references(&mut self, p: &Value) -> Result<Value> {
        anyhow::ensure!(
            self.mode == Mode::Indexed,
            "Possible references require indexed mode"
        );
        let _budget = foreground::Scope::enter();
        let origin = path_param(p)?;
        let version = self.overlays.get(&origin).map(|overlay| overlay.version);
        for expected in [p.get("documentVersion"), p.get("version")]
            .into_iter()
            .flatten()
            .filter(|v| !v.is_null())
        {
            anyhow::ensure!(
                expected.as_i64() == version && expected.as_i64().is_some(),
                "Stale or invalid occurrence query document version"
            );
        }
        let line = u32::try_from(
            p.get("line")
                .or_else(|| p.get("position").and_then(|v| v.get("line")))
                .and_then(Value::as_u64)
                .context("Missing line")?,
        )?;
        let column = u32::try_from(
            p.get("character")
                .or_else(|| p.get("position").and_then(|v| v.get("character")))
                .and_then(Value::as_u64)
                .context("Missing character")?,
        )?;
        let (source, origin_identity) = self.occurrence_source(&origin)?;
        if let Some(expected) = p.get("expectedSourceText") {
            anyhow::ensure!(
                expected.as_str() == Some(source.as_str()),
                "Origin text changed; reload or retry the occurrence search"
            );
        }
        foreground::parse()?;
        let parsed = occurrence_parse(&origin, &source, &self.cancellation);
        self.cancellation.check()?;
        let name = parsed
            .token_at(line, column)
            .filter(|i| parsed.tokens[*i].ident)
            .map(|i| parsed.tokens[i].text.clone());
        // Preserve the ordinary cold-navigation policy: a small model may be
        // discovered exactly within the existing foreground cap. Larger scans
        // yield to background discovery without changing proof semantics.
        if self.evaluated_model_path.is_none() {
            self.background.requested = true;
            self.background.demand_origin = Some(origin.clone());
        }
        let preparation_pending = match self.ensure_model() {
            Ok(()) => {
                self.drain_events()?;
                !self.model_loaded
            }
            Err(error) if error.is::<foreground::Limit>() || error.is::<background::Pending>() => {
                if self.background.requested
                    && !self.model_loaded
                    && self.background.phase != "limited"
                {
                    self.background.phase = "scanning";
                }
                true
            }
            Err(error) => return Err(error),
        };
        let mut candidates = Vec::new();
        let mut identities = Vec::new();
        let mut scanned = usize::from(name.is_none());
        let mut skipped =
            usize::from(name.is_none() && parsed.tokens.is_empty() && !parsed.supported);
        let mut skipped_reads = 0usize;
        let mut skipped_syntax = skipped;
        let mut bytes = if name.is_none() { source.len() } else { 0 };
        let mut truncated = preparation_pending;
        let mut rows_examined = 0usize;
        let mut eligible_seen = 0usize;
        if let Some(name) = &name {
            let paths = if preparation_pending {
                Vec::new()
            } else {
                let domain = if self.model_loaded {
                    self.model.files.iter().cloned().collect::<HashSet<_>>()
                } else {
                    HashSet::new()
                };
                let root = Path::new(&self.root);
                let overlays = self
                    .overlays
                    .keys()
                    .filter(|path| {
                        domain.contains(*path)
                            || (Path::new(path).starts_with(root)
                                && project::is_source(Path::new(path)))
                    })
                    .cloned()
                    .collect::<HashSet<_>>();
                // Dirty bytes are authoritative even before a new name has a posting.
                let mut dirty = Vec::new();
                for path in &overlays {
                    self.cancellation.check()?;
                    if path.as_str() != origin
                        && self
                            .overlays
                            .get(path)
                            .is_some_and(|overlay| overlay.text.contains(name))
                    {
                        dirty.push(path.clone());
                    }
                }
                dirty.sort();
                let dirty_seen = dirty.len();
                if dirty.len() > MAX_FILES - 1 {
                    dirty.truncate(MAX_FILES - 1);
                    truncated = true;
                }
                let dirty_set = dirty.iter().cloned().collect::<HashSet<_>>();
                let selected = self.index.ranked_candidate_paths(
                    name,
                    MAX_FILES - 1 - dirty.len(),
                    MAX_ROWS,
                    |path| {
                        (path != origin
                            && !dirty_set.contains(path)
                            && (domain.contains(path) || overlays.contains(path)))
                        .then_some(1)
                    },
                )?;
                truncated |= selected.truncated;
                rows_examined = selected.rows_examined;
                eligible_seen = selected.eligible_seen + dirty_seen;
                dirty.into_iter().chain(selected.paths).collect::<Vec<_>>()
            };
            let mut first = Some((source, parsed, origin_identity.clone()));
            for path in std::iter::once(origin.clone()).chain(paths) {
                self.cancellation.check()?;
                let (source, parsed, identity) = if let Some(first) = first.take() {
                    first
                } else {
                    let (source, identity) = match self.occurrence_source(&path) {
                        Ok(value) => value,
                        Err(error) if error.is::<cancellation::Cancelled>() => return Err(error),
                        Err(error) if error.is::<foreground::Limit>() => {
                            truncated = true;
                            break;
                        }
                        Err(_) => {
                            skipped_reads += 1;
                            skipped += 1;
                            continue;
                        }
                    };
                    if bytes.saturating_add(source.len()) > MAX_TOTAL_BYTES {
                        truncated = true;
                        break;
                    }
                    foreground::parse()?;
                    let parsed = occurrence_parse(&path, &source, &self.cancellation);
                    self.cancellation.check()?;
                    (source, parsed, identity)
                };
                scanned += 1;
                bytes += source.len();
                if parsed.tokens.is_empty() && !parsed.supported {
                    skipped_syntax += 1;
                    skipped += 1;
                    continue;
                }
                let mut contributed = false;
                for token in parsed
                    .tokens
                    .iter()
                    .filter(|token| token.ident && token.text == *name)
                {
                    self.cancellation.check()?;
                    if candidates.len() == MAX_LOCATIONS {
                        truncated = true;
                        break;
                    }
                    candidates.push(json!({"uri":uri(&path),"range":token.range()}));
                    contributed = true;
                }
                if contributed {
                    identities.push(identity);
                }
                if candidates.len() == MAX_LOCATIONS {
                    // A full page cannot establish that no further match exists.
                    truncated = true;
                    break;
                }
            }
        }
        self.cancellation.check()?;
        let reason = "Possible references are same-name physical-source occurrences, including declarations and potentially inactive conditional branches. Binding identity and completeness are not established.";
        let mut result = json!({
            "locations":[],"completeness":"unavailable","source":"occurrence-search",
            "candidates":candidates,"candidateKind":"same-name-occurrences","candidateCompleteness":"incomplete",
            "queryName":name,"includeDeclarations":true,"reason":reason,"diagnostics":[reason],
            "candidateTruncated":truncated,"version":version,"documentVersion":version,
            "snapshotId":self.snapshot,"snapshot":self.epoch,
            "occurrenceIdentity":{"origin":origin_identity,"targets":identities},
            "occurrenceSearch":{"scannedFiles":scanned,"skippedFiles":skipped,"scannedBytes":bytes,
                "skipReasons":{"unreadableOrOversized":skipped_reads,"unsafeLexicalOrSyntax":skipped_syntax},
                "rowsExamined":rows_examined,"eligiblePathsSeen":eligible_seen,
                "maxFiles":MAX_FILES,"maxRows":MAX_ROWS,"maxLocations":MAX_LOCATIONS,
                "maxSourceBytes":MAX_SOURCE_BYTES,"maxTotalBytes":MAX_TOTAL_BYTES,
                "preparationPending":preparation_pending,"discoveryPhase":self.background.phase,"discoveryComplete":self.background.status()["discoveryComplete"],"modelAvailable":self.model_loaded,
                "indexComplete":self.background.phase=="ready","scope":"available-workspace-sources"}
        });
        if skipped > 0 {
            result["diagnostics"].as_array_mut().unwrap().push(json!("Some selected sources could not be read or safely tokenized and were omitted; the search is incomplete"));
        }
        if truncated {
            result["candidateTruncationReason"] = json!("Occurrence search reached a preparation, file, row, byte, or location bound; additional matches may exist");
        }
        Ok(result)
    }
}

fn occurrence_parse(
    path: &str,
    source: &str,
    cancellation: &cancellation::CancellationToken,
) -> ParsedFile {
    if let Err(error) = context::validate_candidate_lexical_source(source, cancellation) {
        let mut parsed = syntax::parse("");
        parsed.supported = false;
        parsed.reasons.push(error.to_string());
        return parsed;
    }
    // Raw physical syntax only: no effective defines, context facts, or proof cache.
    let mut parsed = if project::signature(path) {
        treesitter::parse_signature(source)
    } else {
        treesitter::parse(source)
    };
    // Recovery can reinterpret broken literal text as identifier leaves. Omit
    // that file rather than presenting recovered text as a lexical occurrence.
    if parsed
        .reasons
        .iter()
        .any(|reason| reason == "Tree-sitter recovered incomplete or unsupported syntax")
    {
        parsed.tokens.clear();
        parsed.supported = false;
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture(files: &[(&str, &str)]) -> (PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "fsnav-occurrences-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let mut includes = String::new();
        for (name, text) in files {
            fs::write(root.join(name), text).unwrap();
            includes.push_str(&format!("<Compile Include=\"{name}\" />"));
        }
        fs::write(
            root.join("Test.fsproj"),
            format!(
                "<Project Sdk=\"Microsoft.NET.Sdk\"><ItemGroup>{includes}</ItemGroup></Project>"
            ),
        )
        .unwrap();
        let mut ws = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        (root, ws)
    }
    fn ask(ws: &mut Workspace, path: &Path) -> Value {
        ws.dispatch(
            "navigation/possibleReferences",
            &json!({"path":path,"line":1,"character":5}),
        )
        .unwrap()
    }
    fn positions(answer: &Value) -> Vec<(String, u64, u64)> {
        answer["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["uri"].as_str().unwrap().to_owned(),
                    v["range"]["start"]["line"].as_u64().unwrap(),
                    v["range"]["start"]["character"].as_u64().unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn occurrences_are_explicit_spelling_matches_including_later_files_not_literals() {
        let first = "module First\nlet needle = 1\nlet a = needle\nlet text = \"needle\"\n// needle\n(* needle (* needle *) *)\nlet verbatim = @\"needle\"\nlet triple = \"\"\"needle\"\"\"\nlet escaped = '\\u006e'\n";
        let later = "module Later\nlet needle = 2\nlet b = First.needle\n#if UNKNOWN\nlet yes = needle\n#else\nlet no = needle\n#endif\n";
        let (root, mut ws) = fixture(&[("First.fs", first), ("Later.fs", later)]);
        let answer = ask(&mut ws, &root.join("First.fs"));
        assert_eq!(answer["completeness"], "unavailable");
        assert_eq!(answer["locations"], json!([]));
        assert_eq!(answer["candidateKind"], "same-name-occurrences");
        assert_eq!(answer["candidateCompleteness"], "incomplete");
        assert_eq!(answer["includeDeclarations"], true);
        let found = positions(&answer);
        assert_eq!(found.len(), 6, "{answer}");
        assert_eq!(
            found
                .iter()
                .filter(|(uri, _, _)| uri.ends_with("First.fs"))
                .count(),
            2
        );
        for line in [1, 2, 4, 6] {
            assert!(found
                .iter()
                .any(|(uri, l, _)| uri.ends_with("Later.fs") && *l == line));
        }
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn occurrences_refresh_dirty_names_rebinding_and_deleted_targets() {
        let first = "module First\nlet needle = 1\nlet x = needle\n";
        let (root, mut ws) = fixture(&[
            ("First.fs", first),
            ("Later.fs", "module Later\nlet x = First.needle\n"),
        ]);
        let origin = root.join("First.fs");
        let later = root.join("Later.fs");
        let before = ask(&mut ws, &origin);
        let rebound = "module Later\nlet needle = 42\nlet x = needle\n";
        fs::write(&later, rebound).unwrap();
        let after = ask(&mut ws, &origin);
        let identity = |v: &Value| {
            v["occurrenceIdentity"]["targets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["uri"].as_str().unwrap().ends_with("Later.fs"))
                .unwrap()["sourceHash"]
                .clone()
        };
        assert_ne!(identity(&before), identity(&after));
        assert_eq!(positions(&after).len(), 4);
        let same_ranges = positions(&after);
        fs::write(&later, rebound.replace("42", "99")).unwrap();
        let rebound_same_range = ask(&mut ws, &origin);
        assert_eq!(positions(&rebound_same_range), same_ranges);
        assert_ne!(identity(&rebound_same_range), identity(&after));
        ws.dispatch(
            "document/open",
            &json!({"path":origin,"version":1,"text":first}),
        )
        .unwrap();
        ws.dispatch(
            "document/open",
            &json!({"path":later,"version":2,"text":"module Later\nlet newName = needle\n"}),
        )
        .unwrap();
        let dirty = ask(&mut ws, &origin);
        let target = dirty["occurrenceIdentity"]["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["uri"].as_str().unwrap().ends_with("Later.fs"))
            .unwrap();
        assert_eq!(target["version"], 2);
        assert_ne!(identity(&dirty), identity(&after));
        ws.dispatch(
            "document/change",
            &json!({"path":origin,"version":3,"text":"module First\nlet newName = 1\n"}),
        )
        .unwrap();
        // The overlay itself must discover a newly typed name even when a
        // persisted posting is absent (e.g. before background materialization).
        ws.index
            .db
            .execute(
                "DELETE FROM occurrences WHERE file_id=(SELECT id FROM paths WHERE path=?1)",
                [normalize(&later)],
            )
            .unwrap();
        let renamed = ask(&mut ws, &origin);
        assert_eq!(renamed["queryName"], "newName");
        assert!(ws
            .dispatch(
                "navigation/possibleReferences",
                &json!({"path":origin,"line":1,"character":5,"expectedSourceText":first})
            )
            .is_err());

        assert!(positions(&renamed)
            .iter()
            .any(|(p, _, _)| p.ends_with("Later.fs")));
        assert!(ws
            .dispatch(
                "navigation/possibleReferences",
                &json!({"path":origin,"line":1,"character":5,"version":1})
            )
            .is_err());
        ws.dispatch("document/close", &json!({"path":later}))
            .unwrap();
        fs::write(&later, "module Later\nlet newName = 42\n").unwrap();
        // Posting discovery is asynchronous. This lifecycle test supplies the
        // file notification explicitly rather than racing the OS watcher.
        ws.dispatch("workspace/filesChanged", &json!({"paths":[later]}))
            .unwrap();
        let before_delete = ask(&mut ws, &origin);
        assert!(positions(&before_delete)
            .iter()
            .any(|(p, _, _)| p.ends_with("Later.fs")));
        let before_delete_origin = before_delete["occurrenceIdentity"]["origin"].clone();
        fs::remove_file(&later).unwrap();
        let deleted = ask(&mut ws, &origin);
        assert_eq!(
            deleted["occurrenceIdentity"]["origin"],
            before_delete_origin
        );
        assert!(!positions(&deleted)
            .iter()
            .any(|(p, _, _)| p.ends_with("Later.fs")));
        assert_eq!(deleted["completeness"], "unavailable");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn occurrences_include_reverse_consumers_and_current_unsaved_linked_source() {
        let (root, mut ws) = fixture(&[("First.fs", "module First\nlet needle = 1\n")]);
        let outside = root.with_extension("linked.fs");
        fs::write(&outside, "module Linked\nlet x = First.needle\n").unwrap();
        let consumer = root.join("Consumer");
        fs::create_dir(&consumer).unwrap();
        fs::write(
            consumer.join("Consumer.fs"),
            "module Consumer\nlet x = First.needle\n",
        )
        .unwrap();
        fs::write(consumer.join("Consumer.fsproj"),format!("<Project Sdk=\"Microsoft.NET.Sdk\"><ItemGroup><ProjectReference Include=\"../Test.fsproj\"/><Compile Include=\"{}\"/><Compile Include=\"Consumer.fs\"/></ItemGroup></Project>",outside.display())).unwrap();
        ws.dispatch(
            "workspace/filesChanged",
            &json!({"paths":[consumer.join("Consumer.fsproj")]}),
        )
        .unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        let before = ask(&mut ws, &root.join("First.fs"));
        assert!(positions(&before)
            .iter()
            .any(|(p, _, _)| p == &uri(&outside.to_string_lossy())));
        assert!(positions(&before)
            .iter()
            .any(|(p, _, _)| p.ends_with("Consumer.fs")));
        ws.dispatch("document/open",&json!({"path":outside,"version":7,"text":"module Linked\n\nlet moved = First.needle\n"})).unwrap();
        let dirty = ask(&mut ws, &root.join("First.fs"));
        let linked = positions(&dirty)
            .into_iter()
            .filter(|(p, _, _)| p == &uri(&outside.to_string_lossy()))
            .collect::<Vec<_>>();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].1, 2);
        let target = dirty["occurrenceIdentity"]["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["uri"] == uri(&outside.to_string_lossy()))
            .unwrap();
        assert_eq!(target["version"], 7);
        assert_eq!(dirty["completeness"], "unavailable");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn occurrences_reject_recovered_literal_text_and_bound_path_pages() {
        for text in [
            "module A\nlet needle = \"needle",
            "module A\n(* needle",
            "module A\nlet needle = @\"needle",
        ] {
            let parsed = occurrence_parse("A.fs", text, &cancellation::CancellationToken::new());
            assert!(parsed.tokens.is_empty(), "{text}: {:?}", parsed.reasons);
        }
        let source = "module First\nlet needle = 1\n";
        let owned = (0..40)
            .map(|n| (format!("File{n}.fs"), source.to_owned()))
            .collect::<Vec<_>>();
        let files = owned
            .iter()
            .map(|(p, s)| (p.as_str(), s.as_str()))
            .collect::<Vec<_>>();
        let (root, mut ws) = fixture(&files);
        let answer = ask(&mut ws, &root.join("File0.fs"));
        assert_eq!(answer["occurrenceSearch"]["scannedFiles"], 32);
        assert_eq!(answer["candidateTruncated"], true);
        assert!(positions(&answer)
            .iter()
            .any(|(p, _, _)| p.ends_with("File0.fs")));
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn possible_first_preserves_small_workspace_exact_far_and_large_pending_bounds() {
        let source = "module First\nlet needle = 1\nlet x = needle\n";
        let (root, original) = fixture(&[("First.fs", source)]);
        drop(original);
        let path = root.join("First.fs");
        let params = json!({"path":path,"line":1,"character":5,"includeDeclaration":true});
        let mut normal = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        let expected = normal.dispatch("navigation/references", &params).unwrap();
        assert_eq!(expected["completeness"], "exact", "{expected}");
        drop(normal);
        let mut possible = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        let occurrences = ask(&mut possible, &path);
        assert_eq!(occurrences["candidateCompleteness"], "incomplete");
        assert!(possible.model_loaded);
        assert!(!possible.background.raw_model);
        for _ in 0..20 {
            possible
                .background_step(cancellation::CancellationToken::new())
                .unwrap();
        }
        let actual = possible.dispatch("navigation/references", &params).unwrap();
        assert_eq!(actual["completeness"], "exact", "{actual}");
        assert_eq!(actual["locations"], expected["locations"]);
        possible.dispatch("workspace/index", &json!({})).unwrap();
        assert_eq!(
            possible.dispatch("navigation/references", &params).unwrap()["locations"],
            expected["locations"]
        );
        drop(possible);
        let noise = root.join("noise");
        fs::create_dir(&noise).unwrap();
        for n in 0..700 {
            fs::write(noise.join(format!("{n}.txt")), "noise").unwrap();
        }
        let mut large = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        let partial = ask(&mut large, &path);
        assert_eq!(partial["occurrenceSearch"]["scannedFiles"], 1);
        assert_eq!(partial["occurrenceSearch"]["rowsExamined"], 0);
        assert_eq!(partial["occurrenceSearch"]["preparationPending"], true);
        assert_eq!(partial["candidateTruncated"], true);
        assert_eq!(partial["candidates"].as_array().unwrap().len(), 2);
        assert_eq!(large.background.phase, "scanning");
        assert!(large.background.requested);
        assert_ne!(
            large.dispatch("navigation/references", &params).unwrap()["completeness"],
            "exact"
        );
        drop(large);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn occurrences_caps_and_cancellation_never_publish_complete_sets() {
        let mut text = "module First\nlet needle = 1\n".to_owned();
        for n in 0..220 {
            text.push_str(&format!("let use{n} = needle\n"));
        }
        let (root, mut ws) = fixture(&[("First.fs", &text)]);
        let result = ask(&mut ws, &root.join("First.fs"));
        assert_eq!(result["candidates"].as_array().unwrap().len(), 200);
        assert_eq!(result["candidateTruncated"], true);
        assert_eq!(result["occurrenceSearch"]["maxLocations"], 200);
        assert_eq!(result["locations"], json!([]));
        let token = cancellation::CancellationToken::new();
        token.cancel();
        ws.set_cancellation(token);
        assert!(ws
            .dispatch(
                "navigation/possibleReferences",
                &json!({"path":root.join("First.fs"),"line":1,"character":5})
            )
            .unwrap_err()
            .is::<cancellation::Cancelled>());
        ws.set_cancellation(cancellation::CancellationToken::new());
        assert_eq!(
            ask(&mut ws, &root.join("First.fs"))["candidateTruncated"],
            true
        );
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
}
