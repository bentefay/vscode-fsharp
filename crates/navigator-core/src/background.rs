//! Idle, resumable work. A partial discovery never becomes the live project model.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Debug)]
pub(crate) struct Pending;
impl std::fmt::Display for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "Background discovery is in progress; retry navigation as source indexing advances",
        )
    }
}
impl std::error::Error for Pending {}

pub(crate) struct Background {
    pub requested: bool,
    pub raw_model: bool,
    pub phase: &'static str,
    pub demand_origin: Option<String>,
    index_order: Option<Vec<usize>>,
    scan: Option<project::Discovery>,
    watcher: Option<RecommendedWatcher>,
    events: Receiver<notify::Result<notify::Event>>,
    pub generation: u64,
    pub slices: u64,
    entries: usize,
    projects: usize,
    files: usize,
    indexed: usize,
    reused: usize,
    pub(crate) reason: Option<String>,
    overflow: Arc<AtomicBool>,
}
impl Default for Background {
    fn default() -> Self {
        Self {
            requested: false,
            raw_model: false,
            phase: "idle",
            demand_origin: None,
            index_order: None,
            scan: None,
            watcher: None,
            events: mpsc::channel().1,
            generation: 0,
            slices: 0,
            entries: 0,
            projects: 0,
            files: 0,
            indexed: 0,
            reused: 0,
            reason: None,
            overflow: Arc::new(AtomicBool::new(false)),
        }
    }
}
impl Background {
    pub fn status(&self) -> Value {
        json!({"phase":self.phase,"discoveryComplete":matches!(self.phase,"indexing"|"ready"),
            "indexComplete":self.phase=="ready","entriesVisited":self.entries,
            "projectsDiscovered":self.projects,"filesDiscovered":self.files,
            "filesIndexed":self.indexed,"filesReused":self.reused,"slices":self.slices,
            "generation":self.generation,"reason":self.reason,"modelProvisional":self.raw_model})
    }
    fn changed(&mut self, root: &str) -> bool {
        let mut changed = self.overflow.swap(false, Ordering::AcqRel);
        let mut events_drained = 0;
        for event in self.events.try_iter().take(1024) {
            events_drained += 1;
            let Ok(event) = event else {
                changed = true;
                continue;
            };
            if event.need_rescan() {
                changed = true;
            }
            if event.kind.is_access() {
                continue;
            }
            for path in event.paths {
                if path.strip_prefix(root).is_ok_and(|relative| {
                    relative
                        .components()
                        .any(|part| project::excluded(&part.as_os_str().to_string_lossy()))
                }) {
                    continue;
                }
                let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
                if !path.starts_with(root)
                    && !matches!(
                        name,
                        "Directory.Build.props" | "Directory.Build.targets" | "global.json"
                    )
                {
                    continue;
                }
                let configuration = matches!(
                    name,
                    "Directory.Build.props" | "Directory.Build.targets" | "global.json"
                ) || matches!(
                    path.extension().and_then(|v| v.to_str()),
                    Some("fsproj" | "props" | "targets" | "sln" | "slnx")
                );
                let membership = event.kind.is_create()
                    || event.kind.is_remove()
                    || matches!(
                        event.kind,
                        notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                    );
                if configuration || membership {
                    changed = true;
                }
            }
        }
        changed
            || events_drained == 1024 && self.events.try_recv().is_ok()
            || self.overflow.swap(false, Ordering::AcqRel)
    }
}
impl Workspace {
    pub(crate) fn reset_background_mode(&mut self) {
        let raw = self.background.raw_model;
        self.background = Default::default();
        if let Some(observation) = &mut self.demand_observation {
            observation.invalidate();
        }
        self.background.raw_model = raw;
        if self.mode == Mode::Indexed && raw {
            self.model_loaded = false;
            self.background.requested = true;
            self.background.phase = "scanning";
        }
    }
    pub(crate) fn finish_explicit_discovery(&mut self) {
        self.background.scan = None;
        self.background.index_order = None;
        if let Some(observation) = &mut self.demand_observation {
            observation.invalidate();
        }
        self.background.indexed = 0;
        if self.background.requested {
            self.background.phase = "indexing";
        }
    }
    pub fn background_pending(&self) -> bool {
        self.mode == Mode::Indexed
            && self.background.requested
            && matches!(self.background.phase, "idle" | "scanning" | "indexing")
    }
    pub(crate) fn restart_raw_discovery(&mut self) -> Result<()> {
        if self.evaluated_model_path.is_none() && self.mode == Mode::Indexed {
            self.invalidate_background_model()?;
        }
        Ok(())
    }
    fn invalidate_background_model(&mut self) -> Result<()> {
        self.background.scan = None;
        self.background.index_order = None;
        if let Some(observation) = &mut self.demand_observation {
            observation.invalidate();
        }
        self.background.watcher = None;
        self.background.phase = "scanning";
        self.background.reason = None;
        self.background.requested = true;
        self.model_loaded = false;
        self.model.exact = false;
        for project in &mut self.model.projects {
            project.exact = false;
        }
        self.source_contexts.clear();
        self.instance_contexts.clear();
        self.covered_names.clear();
        self.semantic_loaded = false;
        self.semantic_revision += 1;
        self.index.db.execute("DELETE FROM answers", [])?;
        self.refresh_snapshot()?;
        Ok(())
    }
    pub(crate) fn poll_background_changes(&mut self) -> Result<()> {
        if self.background.changed(&self.root) {
            self.invalidate_background_model()?;
        }
        Ok(())
    }
    pub(crate) fn demand_order_status(&self) -> Value {
        let observation = self.demand_observation.as_ref().unwrap();
        json!({"generation":self.background.generation,"phase":self.background.phase,
            "orderSerial":if observation.order_live { Some(observation.serial) } else { None },
            "orderOriginSlot":observation.order_origin_slot,
            "cursor":self.background.indexed,"slots":observation.watches.iter().map(|watch|
                json!({"modelOrdinal":watch.model,"orderOrdinal":watch.order,
                    "state":if !observation.order_live {"unconstructed"} else { match watch.order {Some(n) if n >= self.background.indexed => "pending", Some(_) => "consumed",None=>"absent"}}})
                ).collect::<Vec<_>>()})
    }
    fn prepare_background_index_order(&mut self) -> Result<()> {
        let lookup = self
            .model
            .files
            .iter()
            .enumerate()
            .map(|(i, file)| (file.as_str(), i))
            .collect::<HashMap<_, _>>();
        let mut seen = vec![false; self.model.files.len()];
        let mut order = Vec::with_capacity(self.model.files.len());
        // Capture observations inside the existing construction pass. Nothing is
        // published until all normal cancellation checks have succeeded.
        let watched_models = self.demand_observation.as_ref().map(|observation| {
            observation
                .watches
                .iter()
                .map(|watch| watch.model)
                .collect::<Vec<_>>()
        });
        let mut watched_positions = [None; 4];
        let mut add = |path: &str| {
            if let Some(&index) = lookup.get(path) {
                if !seen[index] {
                    seen[index] = true;
                    if let Some(models) = &watched_models {
                        for (slot, model) in models.iter().enumerate() {
                            if *model == index {
                                watched_positions[slot] = Some(order.len());
                            }
                        }
                    }
                    order.push(index);
                }
            }
        };
        if let Some(origin) = &self.background.demand_origin {
            add(origin);
            if let Some(project) = self.model.source_instance_project(origin, None) {
                let mut projects = vec![project.path.as_str()];
                let mut visited = HashSet::new();
                while let Some(path) = projects.pop() {
                    self.cancellation.check()?;
                    if !visited.insert(path) {
                        continue;
                    }
                    if let Some(project) = self
                        .model
                        .projects
                        .iter()
                        .find(|project| project.path == path)
                    {
                        for file in &project.files {
                            self.cancellation.check()?;
                            add(file);
                        }
                        projects.extend(project.references.iter().map(String::as_str));
                    }
                }
            }
        }
        for file in &self.model.files {
            self.cancellation.check()?;
            add(file);
        }
        if let Some(observation) = &mut self.demand_observation {
            let origin_model = self
                .background
                .demand_origin
                .as_deref()
                .and_then(|origin| lookup.get(origin).copied());
            observation.constructed(&watched_positions, &self.model.files, origin_model);
        }
        self.background.index_order = Some(order);
        Ok(())
    }

    /// One daemon idle quantum. A foreground token never owns autonomous work.
    pub fn background_step(&mut self, token: cancellation::CancellationToken) -> Result<()> {
        let old = std::mem::replace(&mut self.cancellation, token.clone());
        self.index.set_cancellation(token.clone());
        let _scope = token.enter();
        let result = self.background_step_inner();
        self.cancellation = old.clone();
        self.index.set_cancellation(old);
        match result {
            Err(error) if error.is::<cancellation::Cancelled>() => Ok(()),
            Err(error) => {
                self.background.phase = "limited";
                self.background.reason = Some(error.to_string());
                self.background.scan = None;
                Ok(())
            }
            Ok(()) => Ok(()),
        }
    }
    fn background_step_inner(&mut self) -> Result<()> {
        self.cancellation.check()?;
        self.poll_background_changes()?;
        // Small foreground-discovered models use the installed directory journal.
        // Observe it even after indexing is ready, so new projects do not wait
        // for a nonlocal navigation request to start their background rescan.
        if self.mode == Mode::Indexed && self.evaluated_model_path.is_none() && self.model_loaded {
            self.drain_events()?;
        }
        if !self.background_pending() {
            return Ok(());
        }
        self.background.slices += 1;
        if !self.model_loaded {
            if self.background.scan.is_none() {
                self.background.generation += 1;
                self.background.phase = "scanning";
                self.background.entries = 0;
                self.background.indexed = 0;
                self.background.reused = 0;
                let (tx, rx) = mpsc::sync_channel(1024);
                let overflow = Arc::new(AtomicBool::new(false));
                let callback_overflow = overflow.clone();
                let index_path = self.index.path.clone();
                let mut watcher = notify::recommended_watcher(
                    move |event: notify::Result<notify::Event>| {
                        if matches!(&event, Ok(event) if !event.need_rescan() && (event.kind.is_access() || index::owned_file_event(event, &index_path)))
                        {
                            return;
                        }
                        if matches!(tx.try_send(event), Err(mpsc::TrySendError::Full(_))) {
                            callback_overflow.store(true, Ordering::Release);
                        }
                    },
                )?;
                self.background.overflow = overflow;
                watcher.watch(Path::new(&self.root), RecursiveMode::NonRecursive)?;
                // Absence of ambient configuration is also a changing input.
                for ancestor in Path::new(&self.root).ancestors().skip(1) {
                    watcher.watch(ancestor, RecursiveMode::NonRecursive)?;
                }
                self.background.events = rx;
                self.background.watcher = Some(watcher);
                self.background.scan = Some(project::Discovery::new(&self.root)?);
            }
            let watcher = self.background.watcher.as_mut().unwrap();
            let scan = self.background.scan.as_mut().unwrap();
            let complete = scan.step(32, |directory| {
                Ok(watcher.watch(directory, RecursiveMode::NonRecursive)?)
            })?;
            self.background.entries = scan.entries;
            self.background.projects = scan.project_count();
            self.background.files = scan.source_count();
            if !complete {
                return Ok(());
            }
            // Events accepted during traversal restart it before publication.
            if self.background.changed(&self.root) {
                self.background.scan = None;
                self.background.watcher = None;
                return Ok(());
            }
            let mut model = self.background.scan.take().unwrap().finish()?;
            // Raw XML is a useful candidate domain, not a reusable compiler
            // membership certificate. Keep every compilation instance inexact.
            model.exact = false;
            for project in &mut model.projects {
                project.exact = false;
            }
            model.diagnostics.push("Background raw-project discovery is provisional; candidates are incomplete, not compiler-certified bindings".into());
            self.background.raw_model = true;
            self.install_model(model)?;
            self.background.phase = "indexing";
            self.background.files = self.model.files.len();
            self.background.projects = self.model.projects.len();
            return Ok(());
        }
        if self.background.phase == "idle" {
            self.background.phase = "indexing";
            self.background.files = self.model.files.len();
            self.background.projects = self.model.projects.len();
        }
        if self.background.index_order.is_none() {
            self.prepare_background_index_order()?;
        }
        let next = self
            .background
            .index_order
            .as_ref()
            .unwrap()
            .get(self.background.indexed)
            .and_then(|index| self.model.files.get(*index))
            .cloned();
        if let Some(path) = next {
            self.cancellation.check()?;
            let changed = self.refresh_file(&path, false)?;
            if !changed && !self.overlays.contains_key(&path) {
                self.background.reused += 1;
            }
            self.background.indexed += 1;
            if changed {
                self.covered_names.clear();
                self.refresh_snapshot()?;
            }
        } else {
            self.background.phase = if self.index_errors.is_empty() {
                "ready"
            } else {
                "limited"
            };
            if !self.index_errors.is_empty() {
                self.background.reason =
                    Some("Some source files could not be indexed; inspect indexErrors".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn benign_journal_events_only_invalidate_when_the_drain_limit_is_exceeded() {
        let mut background = Background::default();
        let (tx, rx) = mpsc::channel();
        background.events = rx;
        let event = notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(PathBuf::from("/repo/type-inventory.json"));
        tx.send(Ok(event.clone())).unwrap();
        assert!(!background.changed("/repo"));
        for _ in 0..1025 {
            tx.send(Ok(event.clone())).unwrap();
        }
        assert!(background.changed("/repo"));
        assert!(!background.changed("/repo"));
        tx.send(Ok(
            notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan)
        ))
        .unwrap();
        assert!(background.changed("/repo"));
    }

    #[test]
    fn dropped_events_and_dotted_directory_removal_invalidate_membership() {
        let mut background = Background::default();
        background.overflow.store(true, Ordering::Release);
        assert!(background.changed("/repo"));
        let (tx, rx) = mpsc::channel();
        background.events = rx;
        tx.send(Ok(notify::Event::new(notify::EventKind::Remove(
            notify::event::RemoveKind::Folder,
        ))
        .add_path(PathBuf::from("/repo/src.v1"))))
            .unwrap();
        assert!(background.changed("/repo"));
        tx.send(Ok(notify::Event::new(notify::EventKind::Create(
            notify::event::CreateKind::Folder,
        ))
        .add_path(PathBuf::from("/unrelated/directory"))))
            .unwrap();
        assert!(!background.changed("/repo"));
    }
}
