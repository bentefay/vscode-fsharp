//! Opt-in, path-free observations. These never establish proof or availability.
use super::*;

pub(crate) struct Watch {
    pub model: usize,
    pub order: Option<usize>,
    pub path: Option<String>,
}
pub(crate) struct Observation {
    pub watches: Vec<Watch>,
    pub serial: u64,
    pub order_live: bool,
    pub order_origin_slot: Option<usize>,
    pub request: Option<Request>,
    pub last: Option<Value>,
}
pub(crate) struct Request {
    pub slot: usize,
    before: Value,
    seed_started: Option<Instant>,
    seed_micros: u64,
    seed_attempts: usize,
    seed_completed: usize,
    target_seeded: bool,
    target_selected: Option<bool>,
}
impl Observation {
    pub fn new(enabled: bool, p: &Value) -> Option<Self> {
        if !enabled {
            return None;
        }
        let values = p.get("diagnosticModelOrdinals")?.as_array()?;
        if values.is_empty() || values.len() > 4 {
            return None;
        }
        let mut watches = Vec::new();
        for value in values {
            let model = usize::try_from(value.as_u64()?).ok()?;
            if model >= 100_000 {
                return None;
            }
            watches.push(Watch {
                model,
                order: None,
                path: None,
            });
        }
        Some(Self {
            watches,
            serial: 0,
            order_live: false,
            order_origin_slot: None,
            request: None,
            last: None,
        })
    }
    pub fn invalidate(&mut self) {
        self.order_live = false;
        self.order_origin_slot = None;
        for watch in &mut self.watches {
            watch.order = None;
            watch.path = None;
        }
    }
    pub fn constructed(
        &mut self,
        positions: &[Option<usize>; 4],
        files: &[String],
        origin_model: Option<usize>,
    ) {
        self.serial += 1;
        self.order_live = true;
        self.order_origin_slot = origin_model
            .and_then(|model| self.watches.iter().position(|watch| watch.model == model));
        for (slot, watch) in self.watches.iter_mut().enumerate() {
            watch.order = positions[slot];
            watch.path = watch.order.and_then(|_| files.get(watch.model).cloned());
        }
    }
    pub fn seed_start(&mut self) {
        if let Some(request) = &mut self.request {
            request.seed_started = Some(Instant::now());
        }
    }
    pub fn seed_attempt(&mut self) {
        if let Some(request) = &mut self.request {
            request.seed_attempts += 1;
        }
    }
    pub fn seed_done(&mut self, path: &str) {
        if let Some(request) = &mut self.request {
            request.seed_completed += 1;
            request.target_seeded |= self.watches[request.slot].path.as_deref() == Some(path);
        }
    }
    pub fn seed_end(&mut self) {
        if let Some(request) = &mut self.request {
            if let Some(started) = request.seed_started.take() {
                request.seed_micros = started.elapsed().as_micros() as u64;
            }
        }
    }
    pub fn selected(&mut self, paths: &[String]) {
        if let Some(request) = &mut self.request {
            request.target_selected = self.watches[request.slot]
                .path
                .as_ref()
                .map(|path| paths.contains(path));
        }
    }
}
impl Workspace {
    pub fn demand_background_observation(&self) -> Option<Value> {
        self.demand_observation
            .as_ref()
            .map(|_| self.demand_order_status())
    }
    pub fn demand_observation(&self) -> Option<&Value> {
        self.demand_observation.as_ref()?.last.as_ref()
    }
    pub(crate) fn begin_demand_observation(&mut self, method: &str, p: &Value) {
        let Some(observation) = &mut self.demand_observation else {
            return;
        };
        observation.last = None;
        observation.request = None;
        if method != "navigation/definition" {
            return;
        }
        let Some(slot) = p
            .get("diagnosticWatchSlot")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n < observation.watches.len())
        else {
            return;
        };
        let origin = path_param(p).ok();
        let origin_slot = observation
            .watches
            .iter()
            .position(|watch| watch.path.as_ref() == origin.as_ref() && origin.is_some());
        let before = self.demand_order_status();
        let observation = self.demand_observation.as_mut().unwrap();
        observation.request = Some(Request {
            slot,
            before: json!({"order":before,"originSlot":origin_slot,"postingStatus":"unknown"}),
            seed_started: None,
            seed_micros: 0,
            seed_attempts: 0,
            seed_completed: 0,
            target_seeded: false,
            target_selected: None,
        });
    }
    pub(crate) fn finish_demand_observation(&mut self, result: &Result<Value>) {
        let after = if self
            .demand_observation
            .as_ref()
            .is_some_and(|o| o.request.is_some())
        {
            self.demand_order_status()
        } else {
            return;
        };
        let observation = self.demand_observation.as_mut().unwrap();
        let request = observation.request.take().unwrap();
        // Errors/cancellation produce no successful witness; no prior request survives.
        let Ok(result) = result else {
            return;
        };
        let target_uri = observation.watches[request.slot]
            .path
            .as_ref()
            .map(|p| uri(p));
        let mut matches = Vec::new();
        let mut truncated = false;
        if let (Some(target), Some(candidates)) = (
            target_uri,
            result.get("candidates").and_then(Value::as_array),
        ) {
            for (index, candidate) in candidates.iter().enumerate() {
                if candidate.get("uri").and_then(Value::as_str) == Some(target.as_str()) {
                    if matches.len() < 4 {
                        matches.push(index);
                    } else {
                        truncated = true;
                    }
                }
            }
        }
        let before = &request.before["order"];
        let old_target = &before["slots"][request.slot];
        let new_target = &after["slots"][request.slot];
        observation.last = Some(json!({"targetSlot":request.slot,
            "targetModelOrdinal":observation.watches[request.slot].model,
            "beforeGeneration":before["generation"],"afterGeneration":after["generation"],
            "beforeOrderSerial":before["orderSerial"],"afterOrderSerial":after["orderSerial"],
            "beforeOrderOriginSlot":before["orderOriginSlot"],"afterOrderOriginSlot":after["orderOriginSlot"],
            "beforeCursor":before["cursor"],"afterCursor":after["cursor"],
            "beforePhase":before["phase"],"afterPhase":after["phase"],
            "beforeTargetOrderOrdinal":old_target["orderOrdinal"],"afterTargetOrderOrdinal":new_target["orderOrdinal"],
            "beforeTargetState":old_target["state"],"afterTargetState":new_target["state"],
            "originSlot":request.before["originSlot"],
            "targetSelectedByInitialCandidateLookup":request.target_selected,
            "seedAttempts":request.seed_attempts,"seedCompleted":request.seed_completed,
            "targetSeedCompleted":request.target_seeded,"seedMicros":request.seed_micros,
            "candidateIndices":matches,"candidateIndicesTruncated":truncated,
            "candidateIndexLane":"result.candidates","identityVerified":false,
            "returnedIndexingHint":result.get("indexing").is_some(),"postingStatus":"unknown"}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn demand_observation_configuration_is_opt_in_and_capped() {
        for value in [
            json!([0]),
            json!([0, 1, 2, 3, 4]),
            json!([-1]),
            json!([100_000]),
            json!(["/private/source.fs"]),
        ] {
            assert!(Observation::new(false, &json!({"diagnosticModelOrdinals":value})).is_none());
        }
        for value in [
            json!([]),
            json!([0, 1, 2, 3, 4]),
            json!([-1]),
            json!([100_000]),
            json!(["secret"]),
        ] {
            assert!(Observation::new(true, &json!({"diagnosticModelOrdinals":value})).is_none());
        }
        let mut observer =
            Observation::new(true, &json!({"diagnosticModelOrdinals":[2,0,99]})).unwrap();
        observer.constructed(
            &[Some(0), Some(2), None, None],
            &["private-a".into(), "private-b".into(), "private-c".into()],
            Some(2),
        );
        assert_eq!(observer.serial, 1);
        assert_eq!(observer.order_origin_slot, Some(0));
        assert_eq!(
            observer.watches.iter().map(|w| w.order).collect::<Vec<_>>(),
            vec![Some(0), Some(2), None]
        );
        observer.invalidate();
        assert!(!observer.order_live);
        assert_eq!(observer.order_origin_slot, None);
        assert!(observer
            .watches
            .iter()
            .all(|w| w.path.is_none() && w.order.is_none()));
        observer.constructed(
            &[Some(1), Some(0), None, None],
            &["new-a".into(), "new-b".into(), "new-c".into()],
            Some(0),
        );
        assert_eq!(observer.serial, 2);
        assert_eq!(observer.order_origin_slot, Some(1));
        assert_eq!(observer.watches[0].order, Some(1));
    }

    #[test]
    fn demand_observation_matches_final_candidates_and_clears_between_requests() {
        let root = std::env::temp_dir().join(format!(
            "navigator-demand-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("Main.fs");
        fs::write(&file, "module Main\nlet value = 1\n").unwrap();
        fs::write(
            root.join("Test.fsproj"),
            "<Project><ItemGroup><Compile Include=\"Main.fs\"/></ItemGroup></Project>",
        )
        .unwrap();
        let mut ws = Workspace::open(&json!({"root":root})).unwrap();
        ws.demand_observation = Observation::new(true, &json!({"diagnosticModelOrdinals":[0]}));
        ws.dispatch("workspace/tree", &json!({})).unwrap();
        ws.background.requested = true;
        ws.background_step(cancellation::CancellationToken::new())
            .unwrap();
        let query = json!({"path":file,"line":1,"character":5,"diagnosticWatchSlot":0});
        ws.begin_demand_observation("navigation/definition", &query);
        let target = uri(file.to_str().unwrap());
        let mut candidates = vec![json!({"uri":"file:///unrelated","range":{}})];
        candidates.extend((0..6).map(|_| json!({"uri":target,"range":{}})));
        ws.finish_demand_observation(&Ok(
            json!({"candidates":candidates,"completeness":"unavailable"}),
        ));
        let observed = ws.demand_observation().unwrap();
        assert_eq!(observed["candidateIndices"], json!([1, 2, 3, 4]));
        assert_eq!(observed["candidateIndicesTruncated"], true);
        assert_eq!(observed["beforeGeneration"], observed["afterGeneration"]);
        assert_eq!(observed["beforeOrderSerial"], observed["afterOrderSerial"]);
        assert_eq!(observed["postingStatus"], "unknown");
        assert_eq!(observed["identityVerified"], false);
        assert!(!observed.to_string().contains("Main.fs"));
        assert!(!observed.to_string().contains(root.to_str().unwrap()));
        ws.begin_demand_observation("navigation/definition", &query);
        ws.finish_demand_observation(&Err(anyhow::anyhow!("private failure")));
        assert!(ws.demand_observation().is_none());
        ws.begin_demand_observation("navigation/definition", &query);
        ws.finish_demand_observation(&Ok(json!({"candidates":[]})));
        assert_eq!(
            ws.demand_observation().unwrap()["candidateIndices"],
            json!([])
        );
        let serial = ws.demand_observation.as_ref().unwrap().serial;
        ws.finish_explicit_discovery();
        let cancelled_step = cancellation::CancellationToken::new();
        cancelled_step.cancel();
        ws.background_step(cancelled_step).unwrap();
        assert_eq!(ws.demand_observation.as_ref().unwrap().serial, serial);
        assert!(!ws.demand_observation.as_ref().unwrap().order_live);
        assert_eq!(
            ws.demand_observation.as_ref().unwrap().order_origin_slot,
            None
        );
        ws.set_cancellation(cancellation::CancellationToken::new());
        ws.dispatch("workspace/stats", &json!({})).unwrap();
        assert!(ws.demand_observation().is_none());
        let cancelled = cancellation::CancellationToken::new();
        cancelled.cancel();
        ws.set_cancellation(cancelled);
        assert!(ws.dispatch("navigation/definition", &query).is_err());
        assert!(ws.demand_observation().is_none());
        assert!(ws.demand_observation.as_ref().unwrap().request.is_none());
        ws.set_cancellation(cancellation::CancellationToken::new());
        let enabled = ws.dispatch("navigation/definition", &query).unwrap();
        assert!(ws.demand_observation().is_some());
        ws.demand_observation = None;
        let disabled = ws.dispatch("navigation/definition", &query).unwrap();
        assert_eq!(enabled["locations"], disabled["locations"]);
        assert_eq!(enabled["completeness"], disabled["completeness"]);
        assert!(ws.demand_observation().is_none());
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn demand_observation_witnesses_pending_target_foreground_seed_without_changing_candidates() {
        let root = std::env::temp_dir().join(format!(
            "navigator-demand-seed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let origin = root.join("Z.fs");
        fs::write(
            root.join("A.fs"),
            "module Types\ntype Person = { Name: string }\n",
        )
        .unwrap();
        fs::write(&origin, "module Consumer\nlet read x = x.Name\n").unwrap();
        fs::write(root.join("Test.fsproj"),"<Project><ItemGroup><Compile Include=\"A.fs\"/><Compile Include=\"Z.fs\"/></ItemGroup></Project>").unwrap();
        let mut outcomes = Vec::new();
        for enabled in [true, false] {
            let mut ws=Workspace::open(&json!({"root":root,"cacheDir":root.join(if enabled {"cache-on"} else {"cache-off"})})).unwrap();
            ws.demand_observation =
                Observation::new(enabled, &json!({"diagnosticModelOrdinals":[0,1]}));
            ws.background.demand_origin = Some(normalize(&origin));
            ws.restart_raw_discovery().unwrap();
            for _ in 0..20 {
                if ws.background.phase == "indexing" {
                    break;
                }
                ws.background_step(cancellation::CancellationToken::new())
                    .unwrap();
            }
            assert_eq!(ws.background.phase, "indexing");
            ws.background_step(cancellation::CancellationToken::new())
                .unwrap();
            let result = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":origin,"line":1,"character":16,"diagnosticWatchSlot":0}),
                )
                .unwrap();
            assert!(
                !result["candidates"].as_array().unwrap().is_empty(),
                "{result}"
            );
            if enabled {
                let trace = ws.demand_observation().unwrap();
                assert_eq!(trace["beforeTargetState"], "pending", "{trace}");
                assert_eq!(trace["beforeOrderOriginSlot"], 1, "{trace}");
                assert_eq!(trace["afterOrderOriginSlot"], 1, "{trace}");
                assert_eq!(
                    trace["targetSelectedByInitialCandidateLookup"], false,
                    "{trace}"
                );
                assert_eq!(trace["targetSeedCompleted"], true, "{trace}");
                assert_eq!(trace["candidateIndices"], json!([0]), "{trace}");
                assert_eq!(trace["beforeGeneration"], trace["afterGeneration"]);
                assert_eq!(trace["beforeCursor"], trace["afterCursor"]);
                assert_eq!(trace["postingStatus"], "unknown");
            } else {
                assert!(ws.demand_observation().is_none());
            }
            outcomes.push((
                result,
                ws.index.counters.files_read,
                ws.index.counters.files_parsed,
            ));
        }
        for field in [
            "locations",
            "candidates",
            "likelyCandidates",
            "completeness",
            "candidateCompleteness",
            "candidateTruncated",
            "proofScope",
        ] {
            assert_eq!(outcomes[0].0[field], outcomes[1].0[field], "{field}");
        }
        assert_eq!(outcomes[0].1, outcomes[1].1);
        assert_eq!(outcomes[0].2, outcomes[1].2);
        fs::remove_dir_all(root).unwrap();
    }
}
