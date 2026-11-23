mod background;
mod builtins;
pub mod cancellation;
mod candidate_ranking;
pub mod context;
pub mod decompiler;
mod demand_observation;
mod document_navigation;
mod foreground;
mod helper;
mod index;
mod library_types;
mod local_lexical;
mod metadata;
mod possible_references;
pub mod prepared;
pub mod producer_contract;
pub mod project;
pub mod syntax;
pub mod treesitter;
mod typeflow;
pub mod validation;
pub fn parse_indexed(source: &str) -> syntax::ParsedFile {
    treesitter::parse(source)
}

#[cfg(test)]
fn certified_metadata_fixture(mut value: serde_json::Value) -> serde_json::Value {
    value.as_object_mut().unwrap().extend(
        producer_contract::certificate()
            .as_object()
            .unwrap()
            .clone(),
    );
    value
}

use anyhow::{bail, Context, Result};
use index::Index;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use project::{from_uri, normalize, uri, Model};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::Instant,
};
use syntax::{Declaration, ParsedFile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Lexical,
    Semantic,
    Indexed,
}
impl Mode {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "lexical" | "syntax" => Ok(Self::Lexical),
            "semantic" => Ok(Self::Semantic),
            "indexed" => Ok(Self::Indexed),
            _ => bail!("Unknown mode {s}; expected lexical, semantic, indexed"),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Semantic => "semantic",
            Self::Indexed => "indexed",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Overlay {
    path: String,
    text: String,
    version: i64,
    #[serde(skip)]
    source_hash: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Symbol {
    path: String,
    token: usize,
    name: String,
    local: bool,
    member: bool,
    context: String,
    owner: String,
}
pub struct Workspace {
    background: background::Background,
    demand_observation: Option<demand_observation::Observation>,
    root: String,
    mode: Mode,
    model: Model,
    index: Index,
    overlays: HashMap<String, Overlay>,
    document_navigation: document_navigation::DocumentNavigation,
    trace_updates: bool,
    document_update_trace: Option<Value>,
    epoch: u64,
    snapshot: String,
    session_id: String,
    context_hash: String,
    model_hash: String,
    events: Receiver<notify::Result<notify::Event>>,
    pending_refresh: HashSet<String>,
    discovery_directories: Vec<PathBuf>,
    _watcher: Option<RecommendedWatcher>,
    watcher_reliable: bool,
    watcher_overflow: std::sync::Arc<std::sync::atomic::AtomicBool>,
    decompiler_path: Option<String>,
    decompiler_args: Vec<String>,
    decompiler_starts: u64,
    decompiler_hits: u64,
    decompiler_peak: u64,
    helper_path: Option<String>,
    helper_args: Vec<String>,
    helper: Option<helper::Helper>,
    helper_limit_mb: u64,
    helper_timeout_ms: u64,
    helper_peak_rss: u64,
    overlay_budget: usize,
    index_cache_mb: u64,
    catalog_fingerprint: String,
    catalog_path: Option<String>,
    catalog_loaded: bool,
    prepared_catalog: Option<metadata::PreparedCatalog>,
    model_loaded: bool,
    model_load_micros: u64,
    catalog_load_micros: u64,
    evaluated_model_path: Option<String>,
    semantic_path: Option<String>,
    semantic_loaded: bool,
    semantic_revision: u64,
    covered_names: HashSet<String>,
    index_errors: HashMap<String, String>,
    type_proofs: typeflow::ProofState,
    cancellation: cancellation::CancellationToken,
    active_project: Option<String>,
    source_contexts: HashMap<String, context::SourceContext>,
    instance_contexts: HashMap<String, Option<context::SourceContext>>,
    query_domain: HashSet<String>,
    request_source_candidates: Option<(Vec<Value>, bool, candidate_ranking::Ranking)>,
    request_definition_can_progress: bool,
    builtin_path: Option<String>,
    builtin_environment: Option<builtins::Environment>,
}

fn candidate_declaration_tokens(parsed: &ParsedFile) -> impl Iterator<Item = usize> + '_ {
    parsed
        .declarations
        .iter()
        .chain(&parsed.potential_bindings)
        .map(|declaration| declaration.token)
        .chain(
            parsed
                .record_types
                .iter()
                .flat_map(|owner| owner.fields.iter().map(|field| field.token)),
        )
        .chain(parsed.type_aliases.iter().map(|alias| alias.token))
        .chain(parsed.method_declarations.iter().map(|method| method.token))
        .chain(
            parsed
                .pattern_declarations
                .iter()
                .map(|pattern| pattern.token),
        )
}

fn string<'a>(p: &'a Value, name: &str) -> Result<&'a str> {
    p.get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing string parameter {name}"))
}
fn path_param(p: &Value) -> Result<String> {
    Ok(from_uri(
        p.get("path")
            .or_else(|| p.get("uri"))
            .and_then(Value::as_str)
            .context("Missing path or uri")?,
    ))
}
fn num(p: &Value, name: &str, default: u64) -> u64 {
    p.get(name).and_then(Value::as_u64).unwrap_or(default)
}
fn token_supported(p: &ParsedFile, token: usize) -> bool {
    p.binding_supported(token)
}
fn refs_supported(p: &ParsedFile, decl: &Declaration) -> bool {
    p.references_supported(decl)
}
fn module_at(p: &ParsedFile, token: usize) -> Option<&str> {
    p.module_at(token)
}

impl Workspace {
    pub fn open(p: &Value) -> Result<Self> {
        let root = normalize(string(p, "root")?);
        anyhow::ensure!(
            Path::new(&root).is_dir(),
            "Workspace root is not a directory"
        );
        let mode = Mode::parse(p.get("mode").and_then(Value::as_str).unwrap_or("indexed"))?;
        let model = Model {
            validation_metrics: Default::default(),
            root: root.clone(),
            projects: Vec::new(),
            files: Vec::new(),
            exact: false,
            diagnostics: vec![
                "Project discovery is deferred until a tree or navigation request".into(),
            ],
            inputs: Vec::new(),
        };
        let cache = p
            .get("cacheDir")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(&root).join(".fsharp-navigator"));
        let index_cache_mb = num(p, "indexCacheMb", 8).clamp(1, 1024);
        let index = Index::open(
            &cache,
            index_cache_mb,
            num(p, "maxFileBytes", 8 * 1024 * 1024) as usize,
        )?;
        let (_tx, events) = mpsc::channel();
        let catalog_path = p
            .get("metadataPath")
            .or_else(|| p.get("libraryCatalogPath"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut ws = Self {
            background: Default::default(),
            demand_observation: demand_observation::Observation::new(
                std::env::var("FSHARP_NAVIGATOR_TRACE").as_deref() == Ok("1"),
                p,
            ),
            watcher_reliable: false,
            watcher_overflow: Default::default(),
            session_id: format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            cancellation: Default::default(),
            active_project: None,
            source_contexts: HashMap::new(),
            instance_contexts: HashMap::new(),
            query_domain: HashSet::new(),
            request_source_candidates: None,
            request_definition_can_progress: false,
            builtin_path: p
                .get("builtinEnvironmentPath")
                .and_then(Value::as_str)
                .map(str::to_string),
            builtin_environment: None,
            type_proofs: Default::default(),
            root,
            mode,
            model,
            index,
            overlays: HashMap::new(),
            document_navigation: Default::default(),
            trace_updates: std::env::var("FSHARP_NAVIGATOR_TRACE").as_deref() == Ok("1"),
            document_update_trace: None,
            epoch: 0,
            snapshot: String::new(),
            context_hash: String::new(),
            model_hash: String::new(),
            events,
            pending_refresh: HashSet::new(),
            discovery_directories: Vec::new(),
            _watcher: None,
            decompiler_path: p
                .get("decompilerPath")
                .and_then(Value::as_str)
                .map(str::to_string),
            decompiler_args: p
                .get("decompilerArgs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            decompiler_starts: 0,
            decompiler_hits: 0,
            decompiler_peak: 0,
            helper_path: p
                .get("helperPath")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| std::env::var("FSHARP_NAVIGATOR_HELPER").ok()),
            helper_args: p
                .get("helperArgs")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            helper: None,
            helper_limit_mb: num(p, "helperMemoryLimitMb", num(p, "memoryLimitMb", 256)),
            helper_timeout_ms: num(p, "helperTimeoutMs", 30_000),
            helper_peak_rss: 0,
            overlay_budget: num(p, "overlayBudgetMb", 32) as usize * 1024 * 1024,
            index_cache_mb,
            catalog_fingerprint: String::new(),
            catalog_path,
            catalog_loaded: false,
            prepared_catalog: None,
            index_errors: HashMap::new(),
            model_loaded: false,
            model_load_micros: 0,
            catalog_load_micros: 0,
            evaluated_model_path: p
                .get("evaluatedModelPath")
                .and_then(Value::as_str)
                .map(str::to_string),
            semantic_path: p
                .get("semanticSnapshotPath")
                .and_then(Value::as_str)
                .map(str::to_string),
            semantic_loaded: false,
            semantic_revision: 0,
            covered_names: HashSet::new(),
        };
        ws.refresh_context()?;
        ws.refresh_snapshot()?;
        Ok(ws)
    }
    // All source-summary accesses cross this boundary so compilation-instance
    // selection and dirty overlays cannot be bypassed by a type proof edge.
    fn select_project(&mut self, project: Option<String>) {
        if self.active_project != project {
            self.active_project = project;
            self.instance_contexts.clear();
            self.type_proofs.clear_context();
        }
        self.query_domain.clear();
        if let Some(path) = &self.active_project {
            let mut todo = vec![path.clone()];
            let mut seen = HashSet::new();
            while let Some(p) = todo.pop() {
                if !seen.insert(p.clone()) {
                    continue;
                }
                if let Some(project) = self.model.projects.iter().find(|v| v.path == p) {
                    self.query_domain.extend(project.files.iter().cloned());
                    todo.extend(project.references.iter().cloned());
                }
            }
        } else {
            self.query_domain.extend(self.model.files.iter().cloned());
        }
    }
    fn source_context(&mut self, path: &str) -> Option<context::SourceContext> {
        if let Some(result) = self.instance_contexts.get(path) {
            return result.clone();
        }
        let result = self
            .model
            .source_instance_project(path, self.active_project.as_deref())
            .map(|p| {
                self.source_contexts
                    .entry(p.path.clone())
                    .or_insert_with(|| p.source_context())
                    .clone()
            });
        if self.instance_contexts.len() < 20_000 {
            self.instance_contexts.insert(path.into(), result.clone());
        }
        result
    }
    fn visible_files(&self, path: &str) -> Vec<String> {
        let Some(project) = self
            .model
            .source_instance_project(path, self.active_project.as_deref())
        else {
            return vec![path.into()];
        };
        let mut files = Vec::new();
        let mut seen = HashSet::new();
        let mut todo = project.references.clone();
        while let Some(reference) = todo.pop() {
            if !seen.insert(reference.clone()) {
                continue;
            }
            if let Some(p) = self.model.projects.iter().find(|p| p.path == reference) {
                files.extend(p.files.iter().cloned());
                todo.extend(p.references.iter().cloned());
            }
        }
        for f in &project.files {
            files.push(f.clone());
            if f == path {
                break;
            }
        }
        files
    }
    fn context_exact(&self, path: &str) -> bool {
        let Some(project) = self
            .model
            .source_instance_project(path, self.active_project.as_deref())
        else {
            return false;
        };
        let mut todo = vec![project.path.clone()];
        let mut seen = HashSet::new();
        while let Some(reference) = todo.pop() {
            if !seen.insert(reference.clone()) {
                continue;
            }
            let mut matches = self.model.projects.iter().filter(|p| p.path == reference);
            let Some(p) = matches.next() else {
                return false;
            };
            if matches.next().is_some() || !p.exact {
                return false;
            }
            todo.extend(p.references.iter().cloned());
        }
        true
    }
    // Signatures can pair with differently named implementations. A factory
    // proof retains the whole-project guard. For an ordinary implementation
    // formal, the compiler checks only previously encountered signatures.
    fn possible_signature_context(&self, path: &str, ordinary_formal: bool) -> bool {
        if !self.context_exact(path) {
            return true;
        }
        let Some(project) = self
            .model
            .source_instance_project(path, self.active_project.as_deref())
        else {
            return true;
        };
        let Some(first_non_implementation) = project.files.iter().position(|file| {
            !Path::new(file)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("fs"))
        }) else {
            return false;
        };
        // Only the evaluated fsc input order can narrow the domain. Linked
        // project instances and duplicate occurrences need separate identities.
        let before_first_signature = ordinary_formal
            && project.evaluation == "design-time-fsc-arguments"
            && project.membership_reusable == Some(true)
            && !project.compiler_args.is_empty()
            && project.files[first_non_implementation..]
                .iter()
                .all(|file| project::explicit_source(Path::new(file)))
            && project.files[..first_non_implementation]
                .iter()
                .any(|file| file == path)
            && project
                .files
                .iter()
                .filter(|file| file.as_str() == path)
                .count()
                == 1
            && self.model.source_instance_project(path, None).is_some();
        !before_first_signature
    }
    fn ambiguous_instance(&self, path: &str) -> bool {
        self.model
            .projects
            .iter()
            .any(|p| p.files.iter().any(|f| f == path))
            && self
                .model
                .source_instance_project(path, self.active_project.as_deref())
                .is_none()
    }
    fn parsed(&mut self, path: &str) -> Result<ParsedFile> {
        if self.mode == Mode::Indexed {
            if let Some(context) = self.source_context(path) {
                let result = if let Some(overlay) = self.overlays.get(path) {
                    self.index
                        .materialize_overlay_in(path, &overlay.text, &context)
                } else {
                    self.index.sync_disk(path, true, false)?;
                    self.index.parsed_in(path, &context)
                };
                return match result {
                    Ok(parsed) => {
                        self.type_proofs.observe_namespace_root(path, &parsed);
                        Ok(parsed)
                    }
                    Err(e) if e.is::<cancellation::Cancelled>() || e.is::<foreground::Limit>() => {
                        Err(e)
                    }
                    Err(e) => {
                        let mut empty = syntax::parse("");
                        empty.supported = false;
                        empty.reasons = vec![e.to_string()];
                        empty.uncertain.push(syntax::UncertainSpan {
                            start: 0,
                            end: usize::MAX,
                            reason: e.to_string(),
                            name: None,
                            binding_names: None,
                            local_binding_scope: None,
                        });
                        Ok(empty)
                    }
                };
            }
            if self.ambiguous_instance(path) {
                bail!("Select a project context for linked source {path}");
            }
        }
        if let Some(overlay) = self.overlays.get(path) {
            if self.index.needs_summary(path)? {
                self.index
                    .materialize_overlay(path, &overlay.text, self.mode != Mode::Lexical)?;
            }
        }
        self.index.parsed(path)
    }
    fn source_symbol(
        &self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        name: &str,
        local: bool,
        member: bool,
    ) -> Symbol {
        let owner = if member {
            parsed
                .record_types
                .iter()
                .find(|r| r.fields.iter().any(|f| f.token == token))
                .map(|r| r.name.as_str())
                .or_else(|| {
                    parsed
                        .method_declarations
                        .iter()
                        .find(|m| m.token == token)
                        .map(|m| m.owner.as_str())
                })
        } else {
            parsed.module_at(token)
        }
        .unwrap_or("")
        .to_string();
        let context = self
            .model
            .source_instance_project(path, self.active_project.as_deref())
            .and_then(|p| self.source_contexts.get(&p.path))
            .map(|c| c.id.clone())
            .unwrap_or_default();
        Symbol {
            path: path.into(),
            token,
            name: name.into(),
            local,
            member,
            context,
            owner,
        }
    }
    fn ensure_model(&mut self) -> Result<()> {
        if self.model_loaded {
            return Ok(());
        }
        if matches!(self.background.phase, "scanning" | "limited") && foreground::is_active() {
            return Err(background::Pending.into());
        }
        let started = Instant::now();
        let model = if let Some(path) = &self.evaluated_model_path {
            Model::from_evaluated(
                &self.root,
                &serde_json::from_str(&foreground::read_text(Path::new(path), u64::MAX)?)?,
            )?
        } else {
            let (model, directories) = project::discover_with_directories(&self.root)?;
            self.discovery_directories = directories;
            model
        };
        self.install_model(model)?;
        self.finish_explicit_discovery();
        self.model_load_micros += started.elapsed().as_micros() as u64;
        Ok(())
    }
    fn prepare_explicit_index_model(&mut self) -> Result<()> {
        let replacing_provisional = self.background.raw_model;
        if replacing_provisional {
            // An explicit index must use the same fresh discovery/validation
            // path as an explicit index before background publication. Never
            // promote the existing provisional projects in place.
            self.model_loaded = false;
            self.background.phase = "scanning";
        }
        self.ensure_model()?;
        self.drain_events()?;
        if !self.model_loaded {
            return Err(background::Pending.into());
        }
        if replacing_provisional {
            // Keep the guard on cancellation, discovery, installation errors,
            // or input events that invalidate the newly installed membership.
            self.background.raw_model = false;
        }
        Ok(())
    }

    fn install_model(&mut self, model: Model) -> Result<()> {
        self.model = model;
        if let Some(observation) = &mut self.demand_observation {
            observation.invalidate();
        }
        self.source_contexts.clear();
        self.instance_contexts.clear();
        self.covered_names.clear();
        self.index.db.execute("DELETE FROM answers", [])?;
        self.index.retain_paths(&self.model.files)?;
        self.index_errors
            .retain(|path, _| self.model.files.contains(path));
        // Register only known source/configuration directories, not a recursive
        // crawl through .git, toolchains, node_modules or build outputs.
        let (tx, rx) = mpsc::sync_channel(1024);
        self.events = rx;
        self.watcher_overflow = Default::default();
        let overflow = self.watcher_overflow.clone();
        let index_path = self.index.path.clone();
        if let Ok(mut watcher) = notify::recommended_watcher(
            move |event: notify::Result<notify::Event>| {
                if matches!(&event, Ok(event) if !event.need_rescan() && (event.kind.is_access() || index::owned_file_event(event, &index_path)))
                {
                    return;
                }
                if matches!(tx.try_send(event), Err(mpsc::TrySendError::Full(_))) {
                    overflow.store(true, std::sync::atomic::Ordering::Release);
                }
            },
        ) {
            self.watcher_reliable = true;
            let mut dirs = HashSet::from([PathBuf::from(&self.root)]);
            if self.evaluated_model_path.is_none() {
                dirs.extend(self.discovery_directories.iter().cloned());
            }
            for path in self
                .model
                .files
                .iter()
                .chain(self.model.projects.iter().map(|p| &p.path))
                .chain(self.model.inputs.iter())
            {
                self.cancellation.check()?;
                if let Some(parent) = Path::new(path).parent() {
                    dirs.insert(parent.to_path_buf());
                }
            }
            let mut watches = HashMap::new();
            for mut directory in dirs {
                self.cancellation.check()?;
                while !directory.is_dir() && directory.pop() {
                    self.cancellation.check()?;
                }
                watches.insert(directory, false);
            }
            for (root, recursive) in self.model.membership_roots() {
                self.cancellation.check()?;
                let mut path = PathBuf::from(root);
                let root_exists = path.is_dir();
                while !path.is_dir() && path.pop() {}
                // A missing recursive domain is watched at its nearest existing
                // parent only. Creation of the missing ancestor invalidates the
                // model before any deeper contents can be used as exact facts.
                let recursive = recursive && root_exists;
                watches
                    .entry(path)
                    .and_modify(|r| *r |= recursive)
                    .or_insert(recursive);
            }
            for (dir, recursive) in watches {
                self.cancellation.check()?;
                let mode = if recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                if watcher.watch(&dir, mode).is_err() {
                    self.watcher_reliable = false;
                }
            }
            // Close the evaluation-to-watcher registration race without reading
            // source contents; filename membership is a separate certificate.
            if let Err(error) = self.model.validate_membership() {
                self.watcher_reliable = false;
                if error.is::<foreground::Limit>() || error.is::<cancellation::Cancelled>() {
                    return Err(error);
                }
            }
            self._watcher = Some(watcher);
        }
        if !self.watcher_reliable {
            self.model.exact = false;
            for project in &mut self.model.projects {
                project.exact = false;
            }
            self.model.diagnostics.push("Filesystem observation is incomplete; project-wide proofs are unavailable until a reliable watcher is established".into());
        }
        self.refresh_context()?;
        self.refresh_snapshot()?;
        self.model_loaded = true;
        Ok(())
    }
    fn ensure_builtins(&mut self) -> Result<bool> {
        if self.type_proofs.builtins_used {
            return Ok(true);
        }
        if self.model.projects.is_empty() {
            return Ok(false);
        }
        let Some(path) = self.builtin_path.clone() else {
            return Ok(false);
        };
        if self.builtin_environment.is_none() {
            self.builtin_environment =
                Some(builtins::Environment::load(&path, &mut self.index.db)?);
        }
        let environment = self.builtin_environment.as_ref().unwrap();
        if !environment.valid()? {
            return Ok(false);
        }
        let owned = self
            .model
            .projects
            .iter()
            .flat_map(|p| p.workspace_assembly_references.iter())
            .collect::<HashSet<_>>();
        if environment
            .workspace_assemblies()
            .iter()
            .any(|p| !owned.contains(p))
        {
            return Ok(false);
        }
        for project in &self.model.projects {
            if !project.exact
                || project.evaluation != "design-time-fsc-arguments"
                || project.reference_bindings.is_empty()
            {
                return Ok(false);
            }
            for reference in &project.reference_bindings {
                if reference["isWorkspace"] == true {
                    continue;
                }
                let Some(path) = reference["path"].as_str() else {
                    return Ok(false);
                };
                let identity = reference["identity"].as_str().unwrap_or("");
                if !environment.covers_reference(path, identity) {
                    return Ok(false);
                }
            }
        }
        self.type_proofs.builtins_used = true;
        Ok(true)
    }
    fn catalog_db(&self) -> &rusqlite::Connection {
        self.prepared_catalog
            .as_ref()
            .map(|catalog| &catalog.database.db)
            .unwrap_or(&self.index.db)
    }
    fn catalog_valid(&self) -> Result<bool> {
        if let Some(catalog) = &self.prepared_catalog {
            catalog.valid()
        } else {
            metadata::valid(&self.index.db)
        }
    }
    fn ensure_metadata(&mut self) -> Result<bool> {
        if self.type_proofs.metadata_used && self.catalog_loaded {
            return Ok(true);
        }
        let Some(path) = self.catalog_path.clone() else {
            return Ok(false);
        };
        if !self.catalog_loaded {
            let started = Instant::now();
            self.prepared_catalog = None;
            if prepared::is_prepared_path(&path) {
                let catalog = metadata::PreparedCatalog::load(&path)?;
                self.catalog_fingerprint = catalog.fingerprint.clone();
                self.prepared_catalog = Some(catalog);
            } else {
                self.catalog_fingerprint = metadata::import(&mut self.index.db, &path)?;
            }
            self.catalog_load_micros += started.elapsed().as_micros() as u64;
            self.catalog_loaded = true;
            self.refresh_context()?;
            self.refresh_snapshot()?;
        }
        let valid = self.catalog_valid()?;
        self.type_proofs.metadata_used = valid;
        Ok(valid)
    }
    fn ensure_name_coverage(&mut self, name: &str) -> Result<()> {
        self.cancellation.check()?;
        if self.covered_names.contains(name) {
            return Ok(());
        }
        let before = self.index.generation()?;
        for path in self.model.files.clone() {
            self.cancellation.check()?;
            if self.overlays.contains_key(&path) {
                // didOpen/didChange already maintain conservative raw postings.
                // Scanning a spelling must not eagerly parse every open buffer.
                continue;
            }
            if let Err(e) = self
                .index
                .search_name(&path, name, self.mode != Mode::Lexical)
            {
                if e.is::<cancellation::Cancelled>() || e.is::<foreground::Limit>() {
                    self.refresh_snapshot()?;
                    return Err(e);
                }
                self.index_errors.insert(path, e.to_string());
            }
        }
        self.covered_names.insert(name.into());
        if self.index.generation()? != before {
            self.refresh_snapshot()?;
        }
        Ok(())
    }
    fn refresh_context(&mut self) -> Result<()> {
        self.model_hash = blake3::hash(serde_json::to_string(&self.model)?.as_bytes())
            .to_hex()
            .to_string();
        self.context_hash = blake3::hash(
            format!(
                "{}:{}:{}",
                self.mode.name(),
                self.model_hash,
                self.catalog_fingerprint
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        Ok(())
    }
    fn refresh_snapshot(&mut self) -> Result<()> {
        self.epoch += 1;
        // A durable, transactional generation replaces O(repository) hashing on
        // every keystroke. File content hashes are computed only for changed files.
        // Native answer proofs are re-established on first demand after a
        // restart. Persistent syntax/postings remain reusable after validating
        // their source inputs; a bare old answer has no complete dependency
        // certificate and therefore cannot cross this session boundary.
        self.snapshot = format!(
            "{}:{}:{}",
            self.session_id,
            self.context_hash,
            self.index.generation()?
        );
        Ok(())
    }
    fn refresh_file(&mut self, path: &str, force: bool) -> Result<bool> {
        if self.overlays.contains_key(path) {
            return Ok(false);
        }
        match self
            .index
            .sync_disk(path, self.mode != Mode::Lexical, force)
        {
            Ok(changed) => {
                self.index_errors.remove(path);
                Ok(changed)
            }
            Err(e) => {
                if e.is::<cancellation::Cancelled>() || e.is::<foreground::Limit>() {
                    return Err(e);
                }
                self.index_errors.insert(path.into(), e.to_string());
                Ok(true)
            }
        }
    }
    fn drain_events(&mut self) -> Result<()> {
        let mut paths = std::mem::take(&mut self.pending_refresh);
        let mut context_changed = self
            .watcher_overflow
            .swap(false, std::sync::atomic::Ordering::AcqRel);
        if context_changed {
            self.watcher_reliable = false;
        }
        let mut semantic_changed = context_changed;
        let mut events_drained = 0;
        for event in self.events.try_iter().take(1024) {
            events_drained += 1;
            let e = match event {
                Ok(e) if !e.need_rescan() => e,
                _ => {
                    self.watcher_reliable = false;
                    context_changed = true;
                    continue;
                }
            };
            if !e.kind.is_modify() && !e.kind.is_create() && !e.kind.is_remove() {
                continue;
            }
            let membership_event = e.kind.is_create()
                || e.kind.is_remove()
                || matches!(
                    e.kind,
                    notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                );
            for p in e.paths {
                if self.evaluated_model_path.is_none()
                    && p.strip_prefix(&self.root).is_ok_and(|relative| {
                        relative
                            .components()
                            .any(|part| project::excluded(&part.as_os_str().to_string_lossy()))
                    })
                {
                    continue;
                }
                if membership_event
                    && self.evaluated_model_path.is_none()
                    && p.starts_with(&self.root)
                    && (p.is_dir()
                        || self.discovery_directories.contains(&p)
                        || matches!(
                            e.kind,
                            notify::EventKind::Create(notify::event::CreateKind::Folder)
                                | notify::EventKind::Remove(notify::event::RemoveKind::Folder)
                        ))
                {
                    context_changed = true;
                }
                if self.model.inputs.contains(&normalize(&p))
                    || membership_event
                        && self
                            .model
                            .inputs
                            .iter()
                            .any(|input| Path::new(input).starts_with(&p))
                {
                    context_changed = true;
                }
                if membership_event && self.model.membership_affects(&p) {
                    context_changed = true;
                }
                let path = normalize(&p);
                if self.model.files.contains(&path) {
                    // Compile items identify live source inputs even with a
                    // nonstandard filename or a mixed-case compiler suffix.
                    paths.insert(path);
                } else if project::is_source(&p) {
                    context_changed |= self.evaluated_model_path.is_none();
                } else if p.extension().and_then(|x| x.to_str()) == Some("dll") {
                    // Rebuilding a workspace binary does not change source
                    // compile order. Semantic snapshots depend on binaries;
                    // imported older model schemas may additionally list them
                    // as project-shape inputs and remain conservative.
                    semantic_changed = true;
                    context_changed |= self.model.inputs.contains(&normalize(&p));
                } else if matches!(
                    p.extension().and_then(|x| x.to_str()),
                    Some("fsproj" | "props" | "targets" | "sln" | "slnx")
                ) || p.file_name().and_then(|x| x.to_str()) == Some("project.assets.json")
                {
                    context_changed = true;
                }
            }
        }
        if events_drained == 1024 && self.events.try_recv().is_ok()
            || self
                .watcher_overflow
                .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            context_changed = true;
            semantic_changed = true;
            self.watcher_reliable = false;
        }
        if context_changed {
            self.index.db.execute("DELETE FROM answers", [])?;
            self.covered_names.clear();
            self.source_contexts.clear();
            self.instance_contexts.clear();
            self.model.exact = false;
            for p in &mut self.model.projects {
                p.exact = false;
            }
            self.model.diagnostics.push("Project/configuration input changed; explicit workspace/evaluate or reopen is required".into());
            self.refresh_context()?;
            self.restart_raw_discovery()?;
        }
        let mut changed = false;
        let mut paths = paths.into_iter();
        while let Some(path) = paths.next() {
            match self.refresh_file(&path, true) {
                Ok(updated) => changed |= updated,
                Err(error) => {
                    self.pending_refresh.insert(path);
                    self.pending_refresh.extend(paths);
                    self.semantic_revision += 1;
                    self.covered_names.clear();
                    self.refresh_snapshot()?;
                    return Err(error);
                }
            }
        }
        if changed || context_changed || semantic_changed {
            self.semantic_revision += 1;
            self.covered_names.clear();
            self.refresh_snapshot()?;
        }
        Ok(())
    }
    fn ensure_path(&mut self, path: &str) -> Result<()> {
        if let Some(o) = self.overlays.get(path) {
            if self.mode == Mode::Lexical && self.index.needs_summary(path)? {
                self.index.materialize_overlay(path, &o.text, false)?;
            }
            return Ok(());
        }
        if self.refresh_file(path, false)? {
            self.refresh_snapshot()?;
        }
        if let Some(e) = self.index_errors.get(path) {
            bail!("{e}")
        }
        Ok(())
    }
    pub fn set_cancellation(&mut self, token: cancellation::CancellationToken) {
        // Transport cancellation may win after dispatch returns but before the
        // daemon commits completion under its admission lock.
        if self.cancellation.is_cancelled() {
            self.document_navigation.clear();
        }
        self.index.set_cancellation(token.clone());
        self.cancellation = token;
    }
    pub fn dispatch(&mut self, method: &str, p: &Value) -> Result<Value> {
        self.document_update_trace = None;
        self.begin_demand_observation(method, p);
        let result = self.dispatch_observed_body(method, p);
        self.finish_demand_observation(&result);
        result
    }
    fn dispatch_observed_body(&mut self, method: &str, p: &Value) -> Result<Value> {
        let _scope = self.cancellation.enter();
        if self.cancellation.is_cancelled() {
            self.document_navigation.clear();
            self.cancellation.check()?;
        }
        self.poll_background_changes()?;
        self.index.begin_request();
        let result = self.dispatch_inner(method, p);
        self.index.end_request();
        if self.cancellation.is_cancelled()
            || result
                .as_ref()
                .err()
                .is_some_and(|e| e.is::<cancellation::Cancelled>())
        {
            self.document_navigation.clear();
            // A cancelled scan may have committed valid per-file facts, but
            // never a complete name-domain certificate or reusable answer.
            self.covered_names.clear();
            self.index.db.execute("DELETE FROM answers", [])?;
            self.semantic_revision += 1;
            self.refresh_snapshot()?;
            self.cancellation.check()?;
        }
        result
    }
    fn dispatch_inner(&mut self, method: &str, p: &Value) -> Result<Value> {
        match method {
            "workspace/tree" | "project/tree" => {
                if !self.model_loaded && matches!(self.background.phase, "scanning" | "limited") {
                    return Ok(
                        json!({"nodes":[],"projects":[],"completeness":"unavailable","source":"project-model","background":self.background.status(),"diagnostics":["Background discovery has not established the project membership; retry when discovery completes"]}),
                    );
                }
                self.ensure_model()?;
                self.drain_events()?;
                if !self.model_loaded {
                    return Ok(
                        json!({"nodes":[],"projects":[],"completeness":"unavailable","source":"project-model","background":self.background.status(),"diagnostics":["Project membership changed; background discovery is pending"]}),
                    );
                }
                Ok(self.model.tree())
            }
            "workspace/stats" => self.stats(),
            "workspace/mode" => {
                let mode = Mode::parse(string(p, "mode")?)?;
                if self.mode != mode {
                    self.helper = None;
                    self.document_navigation.clear();
                    self.mode = mode;
                    self.reset_background_mode();
                    self.covered_names.clear();
                    for o in self.overlays.values() {
                        self.index
                            .overlay(&o.path, &o.text, mode != Mode::Lexical)?;
                    }
                    self.refresh_context()?;
                    self.refresh_snapshot()?;
                }
                Ok(json!({"mode":mode.name(),"snapshotId":self.snapshot}))
            }
            "document/open" | "document/change" | "update" => self.change(p),
            "document/close" | "close" => {
                let path = path_param(p)?;
                self.document_navigation.invalidate(&path);
                self.overlays.remove(&path);
                if self.model_loaded || self.mode != Mode::Indexed {
                    self.refresh_file(&path, true)?;
                } else {
                    // Discard dirty postings now. The first disk query will
                    // validate the saved source if it is ever needed.
                    self.index.remove(&path)?;
                }
                self.semantic_revision += 1;
                self.covered_names.clear();
                self.refresh_snapshot()?;
                Ok(json!({"closed":true,"snapshotId":self.snapshot}))
            }
            "workspace/index" => {
                self.prepare_explicit_index_model()?;
                let mut changed = false;
                for path in self.model.files.clone() {
                    changed |= self.refresh_file(
                        &path,
                        p.get("force").and_then(Value::as_bool).unwrap_or(false),
                    )?;
                }
                if changed {
                    self.refresh_snapshot()?;
                }
                if let Some(path) = self.semantic_path.clone() {
                    self.dispatch_inner("semantic/import", &json!({"path":path}))?;
                }
                self.stats()
            }
            "workspace/filesChanged" => {
                let mut changed = false;
                let mut context_changed = false;
                for path in p
                    .get("paths")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    let path = from_uri(path);
                    if self.model.files.contains(&path) {
                        changed |= self.refresh_file(&path, true)?;
                    } else {
                        context_changed = true;
                    }
                }
                if context_changed {
                    self.model.exact = false;
                    for project in &mut self.model.projects {
                        project.exact = false;
                    }
                    self.model
                        .diagnostics
                        .push("Project inputs changed; reevaluate or reopen workspace".into());
                    self.refresh_context()?;
                    self.restart_raw_discovery()?;
                    changed = true;
                }
                if changed {
                    self.refresh_snapshot()?;
                }
                Ok(json!({"snapshotId":self.snapshot}))
            }
            "workspace/evaluate" => self.evaluate(p),
            "metadata/import" => {
                self.catalog_path = Some(string(p, "path")?.into());
                self.catalog_loaded = false;
                self.prepared_catalog = None;
                self.type_proofs.metadata_used = false;
                let valid = self.ensure_metadata()?;
                Ok(json!({"valid":valid,"snapshotId":self.snapshot}))
            }
            "semantic/import" => {
                self.ensure_model()?;
                for o in self.overlays.values() {
                    let disk = foreground::read_text(
                        Path::new(&o.path),
                        self.index.max_file_bytes as u64,
                    )?;
                    self.index.counters.files_read += 1;
                    self.index.counters.bytes_read += disk.len() as u64;
                    anyhow::ensure!(
                        disk == o.text,
                        "Import a semantic snapshot only with clean buffers"
                    );
                }
                let value: Value = serde_json::from_str(&foreground::read_text(
                    Path::new(string(p, "path")?),
                    u64::MAX,
                )?)?;
                let projects = self
                    .model
                    .projects
                    .iter()
                    .map(|p| p.path.clone())
                    .collect::<Vec<_>>();
                let semantic_snapshot =
                    format!("{}:semantic:{}", self.model_hash, self.semantic_revision);
                let count = self
                    .index
                    .import_semantic(&value, &semantic_snapshot, &projects)?;
                self.semantic_loaded = true;
                Ok(
                    json!({"occurrences":count,"snapshotId":self.snapshot,"invalidation":"All imported results invalid after any source/context edit; no semantic early cutoff claimed"}),
                )
            }
            "navigation/documentDefinition" => self.document_definition(p),
            "navigation/possibleReferences" => self.possible_references(p),
            "navigation/definition" | "navigation/references" | "navigation/hover" => {
                if self.mode != Mode::Indexed {
                    return self.navigation(method, p);
                }
                self.request_source_candidates = None;
                self.request_definition_can_progress = false;
                let _scope = foreground::Scope::enter();
                let answer = self.navigation(method, p);
                self.cancellation.check()?;
                if let Err(limit) = foreground::check() {
                    // Some speculative proof edges intentionally suppress errors.
                    // Never publish a positive answer or a name-absence certificate
                    // if part of its required work was declined.
                    self.covered_names.clear();
                    self.refresh_snapshot()?;
                    if self.background.requested && !self.model_loaded {
                        self.background.phase = "scanning";
                    }
                    let reason = if self.background_pending() {
                        "Indexed navigation reached its foreground work limit; background discovery/indexing is in progress; retry navigation".to_string()
                    } else {
                        limit.to_string()
                    };
                    let mut unavailable = self.empty(method == "navigation/hover", &reason);
                    self.attach_source_candidates(&mut unavailable);
                    unavailable["background"] = self.background.status();
                    let version = self.overlays.get(&path_param(p)?).map(|o| o.version);
                    unavailable["documentVersion"] = json!(version);
                    unavailable["version"] = json!(version);
                    unavailable["snapshotId"] = json!(self.snapshot);
                    unavailable["snapshot"] = json!(self.epoch);
                    self.attach_indexing_hint(method, &mut unavailable);
                    Ok(unavailable)
                } else if answer
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.is::<background::Pending>())
                {
                    let reason = if self.background.phase == "limited" {
                        format!(
                            "Background preparation stopped: {}",
                            self.background
                                .reason
                                .as_deref()
                                .unwrap_or("resource limit reached")
                        )
                    } else {
                        background::Pending.to_string()
                    };
                    let mut unavailable = self.empty(method == "navigation/hover", &reason);
                    self.attach_source_candidates(&mut unavailable);
                    unavailable["background"] = self.background.status();
                    let version = self.overlays.get(&path_param(p)?).map(|o| o.version);
                    unavailable["version"] = json!(version);
                    unavailable["documentVersion"] = json!(version);
                    self.attach_indexing_hint(method, &mut unavailable);
                    Ok(unavailable)
                } else {
                    answer.map(|mut result| {
                        self.attach_indexing_hint(method, &mut result);
                        result
                    })
                }
            }
            "$/cancelRequest" => Ok(
                json!({"cancelled":false,"reason":"The daemon routes cancellation by request ID; direct library callers cancel the installed request token"}),
            ),
            "shutdown" => {
                self.helper = None;
                Ok(json!({"shutdown":true}))
            }
            _ => bail!("Unknown method {method}"),
        }
    }
    fn change(&mut self, p: &Value) -> Result<Value> {
        let started = self.trace_updates.then(Instant::now);
        let path = path_param(p)?;
        let text = string(p, "text")?;
        let version = p
            .get("version")
            .and_then(Value::as_i64)
            .context("Missing document version")?;
        if let Some(old) = self.overlays.get(&path) {
            anyhow::ensure!(
                version > old.version || (version == old.version && old.text == text),
                "Stale document version: {version} <= {}",
                old.version
            );
        }
        let bytes = self
            .overlays
            .values()
            .filter(|o| o.path != path)
            .map(|o| o.text.len())
            .sum::<usize>()
            + text.len();
        anyhow::ensure!(bytes<=self.overlay_budget,"Open document text exceeds configured overlayBudgetMb; close another document or raise budget");
        let validation_micros = started.map(|started| started.elapsed().as_micros() as u64);
        let (changed, source_hash, index_trace) = self.index.overlay_traced(
            &path,
            text,
            self.mode != Mode::Lexical,
            self.trace_updates,
        )?;
        let store_started = self.trace_updates.then(Instant::now);
        let document_key = self
            .trace_updates
            .then(|| blake3::hash(path.as_bytes()).to_hex().to_string());
        let proof_cache_evicted = self.document_navigation.invalidate(&path);
        self.overlays.insert(
            path.clone(),
            Overlay {
                path,
                text: text.into(),
                version,
                source_hash: source_hash.to_hex().to_string(),
            },
        );
        let overlay_store_micros =
            store_started.map(|started| started.elapsed().as_micros() as u64);
        let snapshot_started = self.trace_updates.then(Instant::now);
        if changed {
            self.semantic_revision += 1;
            self.covered_names.clear();
            self.refresh_snapshot()?;
        }
        if let (Some(started), Some(index_trace)) = (started, index_trace) {
            self.document_update_trace = Some(json!({
                "documentKey":document_key,"version":version,"sourceHash":source_hash.to_hex().to_string(),
                "bytes":text.len(),"changed":changed,"validationMicros":validation_micros,
                "indexUpdateMicros":index_trace.index_update_micros,"hashMicros":index_trace.hash_micros,
                "nameScanMicros":index_trace.name_scan_micros,"namesCount":index_trace.names_count,
                "lookupMicros":index_trace.lookup_micros,
                "transactionBeginMicros":index_trace.transaction_begin_micros,
                "postingsDeleteMicros":index_trace.postings_delete_micros,
                "namesInsertMicros":index_trace.names_insert_micros,
                "stateWriteMicros":index_trace.state_write_micros,
                "commitMicros":index_trace.commit_micros,

                "overlayStoreMicros":overlay_store_micros,"proofCacheEvicted":proof_cache_evicted,
                "snapshotMicros":snapshot_started.map(|started|started.elapsed().as_micros() as u64),
                "totalMicros":started.elapsed().as_micros() as u64,
            }));
        }
        Ok(
            json!({"accepted":true,"version":version,"documentVersion":version,"snapshot":self.epoch,"snapshotId":self.snapshot}),
        )
    }
    fn resolve(&mut self, path: &str, p: &ParsedFile, token: usize) -> Result<Option<Symbol>> {
        if p.possible_named_argument(token) {
            return self.call_argument_binding(path, p, token);
        }
        // An ordered open can replace an earlier function with a constructor,
        // including one with a lowercase name. A value-only export lookup
        // cannot establish which binding owns the call or its references.
        if p.call_argument_name_candidates
            .iter()
            .any(|argument| argument.callee == Some(token))
            && p.qualifier(token).is_none()
            && !p.proven_function_value(token)
        {
            return Ok(None);
        }
        if Self::has_member_syntax(p, token) {
            if let Some(member) = self.member_binding(path, p, token)? {
                return Ok(Some(member));
            }
        }
        if !token_supported(p, token) {
            return Ok(None);
        }
        let t = &p.tokens[token];
        let visible = self.visible_files(path);
        let local = p.local_binding(token);
        if local.is_none() && p.qualifier(token).is_none() {
            self.track_application_namespace(p, token);
            self.type_proofs.observe_namespace_root(path, p);
        }
        let imports = if p.qualifier(token).is_none()
            && local.is_none_or(|d| d.top_level && d.token != token)
        {
            self.effective_imports_since(
                path,
                p,
                token,
                local.map(|d| p.tokens[d.token].start).unwrap_or(0),
            )?
        } else {
            p.opens.clone()
        };
        if let Some(d) = p.local_binding(token) {
            if d.top_level
                && d.token != token
                && p.expression_type_candidates.iter().any(|c| {
                    c.token == token
                        && matches!(c.role, syntax::ExpressionTypeRole::ApplicationHead)
                })
                && !p.uncontested_application_binding(token, d)
            {
                return Ok(None);
            }
            // An `open` later in the same module can shadow an earlier module
            // value; lexical parameters/local lets still take precedence.
            if d.top_level && d.token != token {
                if imports.iter().any(|o| {
                    o.offset > p.tokens[d.token].start
                        && o.offset < t.start
                        && t.start < o.scope_end
                }) {
                    self.ensure_name_coverage(&t.text)?;
                }
                for open in imports.iter().rev().filter(|o| {
                    o.offset > p.tokens[d.token].start
                        && o.offset < t.start
                        && t.start < o.scope_end
                }) {
                    if open.name.is_empty() {
                        return Ok(None);
                    }
                    let choices =
                        self.opened_modules(path, p, open.environment_token(p, token), &open.name)?;
                    let mut known = false;
                    for module in choices {
                        known = true;
                        let Some(matches) =
                            self.visible_exports(&module, &t.text, path, p, token, &visible)?
                        else {
                            return Ok(None);
                        };
                        if matches.len() == 1 {
                            let (f, i) = &matches[0];
                            let owner = self.parsed(f)?;
                            return Ok(Some(
                                self.source_symbol(f, &owner, *i, &t.text, false, false),
                            ));
                        }
                        if matches.len() > 1 {
                            return Ok(None);
                        }
                    }
                    if !known {
                        return Ok(None);
                    }
                }
            }
            return Ok(Some(self.source_symbol(
                path,
                p,
                d.token,
                &d.name,
                !d.top_level,
                false,
            )));
        }
        let qualifier = p.qualifier(token);
        let mut modules = Vec::new();
        let mut unknown_opens = Vec::new();
        if let Some(q) = qualifier {
            let first = q.split('.').next().unwrap_or("");
            // F# resolves an expression value before a same-spelled module alias.
            if p.declarations
                .iter()
                .any(|d| d.name == first && d.visible_start <= t.start && t.start < d.scope_end)
            {
                return Ok(None);
            }
            if p.method_calls.iter().any(|call| call.member == token)
                && !self.module_call_root_clear(path, p, token, &q)?
            {
                return Ok(None);
            }
            modules.extend(self.opened_modules(path, p, token, &q)?);
        } else {
            if let Some(m) = module_at(p, token) {
                modules.push(m.to_string());
            }
            for open in imports
                .iter()
                .rev()
                .filter(|o| o.offset < t.start && t.start < o.scope_end)
            {
                if open.name.is_empty() {
                    return Ok(None);
                }
                let opened =
                    self.opened_modules(path, p, open.environment_token(p, token), &open.name)?;
                if opened.is_empty() {
                    // This ordered import has an unknown name environment. Do
                    // not skip it then certify an older same-spelled binding.
                    unknown_opens.push(open.name.as_str());
                    modules.push(String::new());
                } else {
                    modules.extend(opened);
                }
            }
        }
        if !self.context_exact(path) || self.ambiguous_instance(path) {
            return Ok(None);
        }
        // Receiver proofs may reach a qualified value/function through a fact
        // edge without a top-level navigation prefilter for that other name.
        self.ensure_name_coverage(&t.text)?;
        for module in modules {
            if module.is_empty() {
                return self.unique_imported_application(path, p, token, &visible, &unknown_opens);
            }
            let Some(matches) = self.visible_exports(&module, &t.text, path, p, token, &visible)?
            else {
                return Ok(None);
            };
            if matches.len() == 1 {
                let (f, i) = &matches[0];
                let owner = self.parsed(f)?;
                if token_supported(&owner, *i) {
                    return Ok(Some(
                        self.source_symbol(f, &owner, *i, &t.text, false, false),
                    ));
                }
                return Ok(None);
            }
            if matches.len() > 1 {
                return Ok(None);
            }
        }
        Ok(None)
    }
    fn visible_exports(
        &mut self,
        module: &str,
        name: &str,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
        visible: &[String],
    ) -> Result<Option<Vec<(String, usize)>>> {
        let mut matches = Vec::new();
        let exports = if self.mode == Mode::Indexed {
            let part = module.rsplit('.').next().unwrap_or(module);
            let modules = self
                .index
                .candidate_paths(part)?
                .into_iter()
                .collect::<HashSet<_>>();
            let mut instances = Vec::new();
            for candidate in self.index.candidate_paths(name)? {
                if !self.query_domain.contains(&candidate) {
                    continue;
                }
                if let Some(context) = self.source_context(&candidate) {
                    if let Some(overlay) = self.overlays.get(&candidate) {
                        if self.index.needs_summary_in(&candidate, &context)? {
                            self.index.materialize_overlay_in(
                                &candidate,
                                &overlay.text,
                                &context,
                            )?;
                        }
                    }
                    instances.push((candidate, context));
                } else if modules.contains(&candidate) {
                    bail!("Export requires an unambiguous referenced-project context: {candidate}");
                }
            }
            let exports = self.index.exported_in(module, name, &instances)?;
            if exports.possible_implicit.iter().any(|(file, start)| {
                visible.contains(file) && (file != path || *start <= parsed.tokens[token].start)
            }) {
                return Ok(None);
            }
            exports.declarations
        } else {
            self.index.exported(module, name)?
        };
        for (file, index) in exports {
            if !visible.contains(&file) {
                continue;
            }
            if file == path
                && !parsed
                    .declarations
                    .iter()
                    .any(|d| d.token == index && d.visible_start <= parsed.tokens[token].start)
            {
                continue;
            }
            // A compiled .fsi is the consumer-visible contract. Implementation
            // declarations absent from that signature cannot participate in an
            // importing module's name lookup.
            if file != path && project::has_signature(&file, visible) {
                continue;
            }
            let summary = self.parsed(&file)?;
            let Some(declaration) = summary.declaration_at(index) else {
                continue;
            };
            let access = declaration.accessibility.clone();
            let owner = summary
                .declaration_module(declaration)
                .unwrap_or("")
                .to_string();
            if access == "private" {
                let current = parsed.module_at(token).unwrap_or("");
                if file != path || !(current == owner || current.starts_with(&format!("{owner}.")))
                {
                    continue;
                }
            }
            if access == "internal" {
                let same = self
                    .model
                    .project_for(path)
                    .zip(self.model.project_for(&file))
                    .is_some_and(|(a, b)| a.path == b.path);
                if !same {
                    continue;
                }
            }
            matches.push((file, index));
        }
        Ok(Some(matches))
    }
    fn module_choices(
        p: &ParsedFile,
        token: usize,
        name: &str,
        include_opens: bool,
    ) -> Vec<String> {
        let mut modules = Vec::new();
        let offset = p.tokens[token].start;
        if let Some(current) = module_at(p, token) {
            let mut prefix = Some(current);
            while let Some(m) = prefix {
                modules.push(format!("{m}.{name}"));
                prefix = m.rsplit_once('.').map(|x| x.0);
            }
        }
        if include_opens {
            modules.extend(
                p.opens
                    .iter()
                    .rev()
                    .filter(|o| o.offset < offset && offset < o.scope_end)
                    .map(|o| format!("{}.{}", o.name, name)),
            );
        }
        modules.push(name.into());
        modules
    }
    fn location(&mut self, s: &Symbol) -> Result<Value> {
        let parsed = self.parsed(&s.path)?;
        Ok(json!({"uri":uri(&s.path),"range":parsed.tokens[s.token].range()}))
    }
    fn empty(&self, hover: bool, reason: &str) -> Value {
        if hover {
            json!({"hover":null,"contents":null,"ownership":"unknown","completeness":"unavailable","source":self.mode.name(),"reason":reason,"diagnostics":[reason]})
        } else {
            json!({"locations":[],"completeness":"unavailable","source":self.mode.name(),"reason":reason,"diagnostics":[reason]})
        }
    }
    /// Later implementation files cannot bind an earlier use in this deliberately
    /// narrow certified compilation. Unknown ordering, signatures and linked or
    /// referenced projects retain the broader suggestions instead.
    fn later_candidate_files(&self, origin: &str) -> HashSet<String> {
        if self.model.projects.len() != 1 || self.possible_signature_context(origin, false) {
            return HashSet::new();
        }
        let Some(project) = self.model.source_instance_project(origin, None) else {
            return HashSet::new();
        };
        if project.evaluation != "design-time-fsc-arguments"
            || project.membership_reusable != Some(true)
            || project.compiler_args.is_empty()
            || !project.references.is_empty()
            || project.files.iter().collect::<HashSet<_>>().len() != project.files.len()
        {
            return HashSet::new();
        }
        let Some(position) = project.files.iter().position(|path| path == origin) else {
            return HashSet::new();
        };
        project.files[position + 1..].iter().cloned().collect()
    }
    // Prefer an existing contextual summary. The raw fallback below is used
    // only for suggestions and never enters contextual or binding-proof caches.
    // Unknown conditionals retain the unconditional physical-source prefix.
    fn candidate_parsed(&mut self, path: &str) -> Result<ParsedFile> {
        if !self.ambiguous_instance(path) {
            let parsed = self.parsed(path)?;
            if !parsed.tokens.is_empty() || parsed.supported {
                return Ok(parsed);
            }
        }
        self.raw_candidate_parsed(path)
    }
    fn raw_candidate_parsed(&mut self, path: &str) -> Result<ParsedFile> {
        let source = if let Some(overlay) = self.overlays.get(path) {
            overlay.text.to_string()
        } else {
            self.index.candidate_source(path)?
        };
        let prefix = match context::unconditional_prefix(&source, &self.cancellation) {
            Ok(prefix) => prefix,
            Err(error)
                if error.is::<cancellation::Cancelled>() || error.is::<foreground::Limit>() =>
            {
                return Err(error)
            }
            Err(_) => return Ok(syntax::parse("")),
        };
        foreground::parse()?;
        let parsed = if project::signature(path) {
            treesitter::parse_signature(prefix)
        } else {
            parse_indexed(prefix)
        };
        self.index.counters.files_parsed += 1;
        self.cancellation.check()?;
        Ok(parsed)
    }

    fn attach_indexing_hint(&self, method: &str, result: &mut Value) {
        if method == "navigation/definition"
            && self.request_definition_can_progress
            && self.background_pending()
            && matches!(self.background.phase, "scanning" | "indexing")
            && result["completeness"] != "exact"
            && result
                .get("locations")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
            && result
                .get("candidates")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
        {
            result["indexing"] = json!({"phase":self.background.phase,"generation":self.background.generation,"retryAfterMs":100});
        }
    }

    fn attach_source_candidates(&mut self, result: &mut Value) {
        if let Some((candidates, truncated, ranking)) = self.request_source_candidates.take() {
            ranking.apply(result);
            if truncated {
                result["candidateTruncated"] = json!(true);
                result["candidateTruncationReason"] = json!("Source candidate search reached its 200-location limit; additional plausible targets may exist");
            }
            result["candidates"] = json!(candidates);
            result["candidateKind"] = json!("same-source-declarations");
            result["candidateCompleteness"] = json!("incomplete");
        }
    }
    /// Recover physical query spelling and candidate-only origin facts after an
    /// unknown conditional. These facts never enter context or proof caches.
    fn candidate_query(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        line: u32,
        col: u32,
    ) -> Result<Option<(String, candidate_ranking::UseEvidence, Option<ParsedFile>)>> {
        if let Some(token) = parsed.token_at(line, col) {
            return Ok(parsed.tokens[token].ident.then(|| {
                (
                    parsed.tokens[token].text.clone(),
                    candidate_ranking::UseEvidence::new(parsed, token),
                    None,
                )
            }));
        }
        const MAX_PHYSICAL_SOURCE: usize = 1024 * 1024;
        self.cancellation.check()?;
        let source = if let Some(overlay) = self.overlays.get(path) {
            if overlay.text.len() > MAX_PHYSICAL_SOURCE || parsed.source_len >= overlay.text.len() {
                return Ok(None);
            }
            overlay.text.to_string()
        } else {
            let len = std::fs::metadata(path)?.len();
            if len > MAX_PHYSICAL_SOURCE as u64 || parsed.source_len as u64 >= len {
                return Ok(None);
            }
            self.index.candidate_source(path)?
        };
        if source.len() > MAX_PHYSICAL_SOURCE {
            return Ok(None);
        }
        let prefix = match context::unconditional_prefix(&source, &self.cancellation) {
            Ok(prefix) => prefix,
            Err(error)
                if error.is::<cancellation::Cancelled>() || error.is::<foreground::Limit>() =>
            {
                return Err(error)
            }
            Err(_) => return Ok(None),
        };
        let suffix = source[prefix.len()..].trim_start_matches([' ', '\t']);
        if !suffix
            .strip_prefix("#if")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
            || (line as usize) < prefix.bytes().filter(|byte| *byte == b'\n').count()
        {
            return Ok(None);
        }
        match context::validate_candidate_lexical_source(&source, &self.cancellation) {
            Ok(()) => (),
            Err(error)
                if error.is::<cancellation::Cancelled>() || error.is::<foreground::Limit>() =>
            {
                return Err(error)
            }
            Err(_) => return Ok(None),
        }
        foreground::parse()?;
        let physical = if project::signature(path) {
            treesitter::parse_signature(&source)
        } else {
            treesitter::parse(&source)
        };
        self.index.counters.files_parsed += 1;
        self.cancellation.check()?;
        if physical
            .reasons
            .iter()
            .any(|reason| reason == "Tree-sitter recovered incomplete or unsupported syntax")
        {
            return Ok(None);
        }
        let Some(token) = physical.token_at(line, col).filter(|token| {
            physical.tokens[*token].ident && physical.tokens[*token].start >= prefix.len()
        }) else {
            return Ok(None);
        };
        Ok(Some((
            physical.tokens[token].text.clone(),
            candidate_ranking::UseEvidence::conditional_qualifier(physical.qualifier(token)),
            Some(physical),
        )))
    }

    fn prepare_source_candidates(&mut self, path: &str, line: u32, col: u32) -> Result<()> {
        self.ensure_path(path)?;
        let parsed = self.raw_candidate_parsed(path)?;
        let Some((name, hint, physical)) = self.candidate_query(path, &parsed, line, col)? else {
            self.request_definition_can_progress = false;
            return Ok(());
        };
        self.request_definition_can_progress = true;
        self.stash_source_candidates(path, &parsed, physical.as_ref(), &name, &hint)
    }
    // Retain only current-dispatch origin facts before broader candidate work.
    // A foreground limit may discard that broader work, never these as exact.
    fn stash_source_candidates(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        physical: Option<&ParsedFile>,
        name: &str,
        hint: &candidate_ranking::UseEvidence,
    ) -> Result<()> {
        let mut ranking = candidate_ranking::Ranking::default();
        let mut seen = HashSet::new();
        let mut locations = Vec::new();
        let mut truncated = false;
        for parsed in std::iter::once(parsed).chain(physical) {
            for ordinal in candidate_declaration_tokens(parsed) {
                self.cancellation.check()?;
                let Some(token) = parsed.tokens.get(ordinal) else {
                    continue;
                };
                if token.text != name || !seen.insert((token.start, token.end)) {
                    continue;
                }
                if locations.len() == 200 {
                    truncated = true;
                    break;
                }
                let location = json!({"uri":uri(path),"range":token.range()});
                ranking.observe(&location, hint.rank(parsed, ordinal, true, false));
                locations.push(location);
            }
        }
        if !locations.is_empty() {
            self.request_source_candidates = Some((locations, truncated, ranking));
        }
        Ok(())
    }

    /// Bounded suggestions, deliberately separate from binding proofs and answer caches.
    /// Reparse current source rather than trusting declaration token ordinals from SQLite:
    /// an edit can move or replace the declaration occupying an old ordinal.
    fn possible_definitions(
        &mut self,
        origin: &str,
        name: &str,
        hint: Option<&candidate_ranking::UseEvidence>,
        physical_origin: Option<&ParsedFile>,
    ) -> Result<(Vec<Value>, bool, candidate_ranking::Ranking)> {
        const MAX_FILES: usize = 32;
        const MAX_LOCATIONS: usize = 200;
        let later_files = self.later_candidate_files(origin);
        let selected_files = self
            .model
            .source_instance_project(origin, self.active_project.as_deref())
            .map(|project| project.files.iter().cloned().collect::<HashSet<_>>())
            .unwrap_or_default();
        let rank = |path: &str, domain: &HashSet<String>| {
            if path == origin || !domain.contains(path) || later_files.contains(path) {
                None
            } else {
                Some(if selected_files.contains(path) { 0 } else { 1 })
            }
        };
        let mut selected =
            self.index
                .ranked_candidate_paths(name, MAX_FILES - 1, 4096, |path| {
                    rank(path, &self.query_domain)
                })?;
        let mut truncated = selected.truncated;
        if let Some(observation) = &mut self.demand_observation {
            observation.selected(&selected.paths);
        }
        // Seed at most eight nearby compilation inputs only when the bounded
        // indexed search has no eligible path. Warm suggestions scan rows once.
        if selected.paths.is_empty() {
            let nearby = self
                .model
                .source_instance_project(origin, self.active_project.as_deref())
                .map(|project| {
                    let at = project
                        .files
                        .iter()
                        .position(|file| file == origin)
                        .unwrap_or(0);
                    project.files[..at]
                        .iter()
                        .rev()
                        .chain(project.files[at..].iter())
                        .filter(|file| file.as_str() != origin)
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if let Some(observation) = &mut self.demand_observation {
                observation.seed_start();
            }
            for path in nearby {
                self.cancellation.check()?;
                if let Some(observation) = &mut self.demand_observation {
                    observation.seed_attempt();
                }
                self.refresh_file(&path, false)?;
                if let Some(observation) = &mut self.demand_observation {
                    observation.seed_done(&path);
                }
            }
            if let Some(observation) = &mut self.demand_observation {
                observation.seed_end();
            }
            selected = self
                .index
                .ranked_candidate_paths(name, MAX_FILES - 1, 4096, |path| {
                    rank(path, &self.query_domain)
                })?;
            truncated |= selected.truncated;
        }
        let mut ranking = candidate_ranking::Ranking::default();
        let mut locations = Vec::new();
        let mut seen = HashSet::new();
        for path in std::iter::once(origin.to_owned()).chain(selected.paths) {
            self.cancellation.check()?;
            self.ensure_path(&path)?;
            let parsed = self.candidate_parsed(&path)?;
            let tokens = candidate_declaration_tokens(&parsed);
            for ordinal in tokens {
                let Some(token) = parsed.tokens.get(ordinal) else {
                    continue;
                };
                if token.text == name && seen.insert((path.clone(), ordinal)) {
                    if locations.len() == MAX_LOCATIONS {
                        return Ok((locations, true, ranking));
                    }
                    let location = json!({"uri":uri(&path),"range":token.range()});
                    if let Some(hint) = hint {
                        ranking.observe(
                            &location,
                            hint.rank(
                                &parsed,
                                ordinal,
                                path == origin,
                                selected_files.contains(&path),
                            ),
                        );
                    }
                    locations.push(location);
                }
            }
        }
        // Preserve every existing bounded alternative and its order before
        // adding declarations exposed by the existing physical parse after conditionals.
        if let Some(parsed) = physical_origin {
            for ordinal in candidate_declaration_tokens(parsed) {
                self.cancellation.check()?;
                let Some(token) = parsed.tokens.get(ordinal) else {
                    continue;
                };
                if token.text != name {
                    continue;
                }
                let location = json!({"uri":uri(origin),"range":token.range()});
                if locations.contains(&location) {
                    continue;
                }
                if locations.len() == MAX_LOCATIONS {
                    truncated = true;
                    break;
                }
                if let Some(hint) = hint {
                    ranking.observe(&location, hint.rank(parsed, ordinal, true, false));
                }
                locations.push(location);
            }
        }
        Ok((locations, truncated, ranking))
    }
    /// Definition-only ordinary custom infix binding. No operator reference domain.
    fn operator_definition(
        &self,
        path: &str,
        parsed: &ParsedFile,
        line: u32,
        col: u32,
    ) -> Option<syntax::Token> {
        if !parsed.operator_proofs_complete {
            return None;
        }
        let usage = parsed
            .operator_uses
            .iter()
            .find(|token| token.line == line && token.column <= col && col < token.end_column)?;
        if self.possible_signature_context(path, false) {
            return None;
        }
        let module = parsed
            .modules
            .iter()
            .filter(|module| module.start <= usage.start && usage.start < module.end)
            .min_by_key(|module| module.end - module.start)
            .map(|module| module.name.as_str())
            .or(parsed.module.as_deref())?;
        let binding = parsed
            .operator_bindings
            .iter()
            .filter(|binding| {
                binding.operator.text == usage.text
                    && binding.module == module
                    && binding.visible_start <= usage.start
                    && usage.start < binding.scope_end
            })
            .max_by_key(|binding| binding.visible_start)?;
        if parsed.opens.iter().any(|open| {
            binding.declaration_start < open.offset
                && open.offset < usage.start
                && usage.start < open.scope_end
        }) || parsed.aliases.iter().any(|alias| {
            binding.declaration_start < alias.offset
                && alias.offset < usage.start
                && usage.start < alias.scope_end
        }) || parsed
            .modules
            .iter()
            .any(|module| binding.visible_start <= module.start && module.start < usage.start)
            || parsed.uncertain.iter().any(|uncertain| {
                uncertain.start <= usage.start
                    && usage.start < uncertain.end
                    && !(uncertain.start == binding.declaration_start
                        && uncertain.end == binding.scope_end
                        && uncertain.reason
                            == "Custom operator/active-pattern declaration needs semantic identity")
            })
        {
            return None;
        }
        Some(binding.operator.clone())
    }
    /// Single-partial active case definition only; never a complete reference domain.
    fn partial_pattern_definition(
        &self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Option<syntax::Token> {
        if !parsed.partial_pattern_proofs_complete
            || !parsed.partial_pattern_heads.contains(&token)
            || parsed.qualifier(token).is_some()
            || self.possible_signature_context(path, false)
        {
            return None;
        }
        let usage = &parsed.tokens[token];
        let module = parsed.module_at(token)?;
        let selected = parsed
            .pattern_declarations
            .iter()
            .filter(|declaration| {
                declaration.kind == "active-pattern"
                    && declaration.name == usage.text
                    && declaration.module == module
                    && declaration.top_level
                    && declaration.scope_start <= usage.start
                    && usage.start < declaration.scope_end
            })
            .max_by_key(|declaration| declaration.scope_start)?;
        if !parsed.partial_pattern_definitions.contains(&selected.token) {
            return None;
        }
        let after = selected.scope_start;
        if parsed
            .partial_pattern_barriers
            .iter()
            .any(|offset| after < *offset && *offset < usage.start)
            || parsed.opens.iter().any(|open| {
                after < open.offset && open.offset < usage.start && usage.start < open.scope_end
            })
            || parsed.aliases.iter().any(|alias| {
                after < alias.offset && alias.offset < usage.start && usage.start < alias.scope_end
            })
            || parsed
                .modules
                .iter()
                .any(|scope| after < scope.start && scope.start < usage.start)
            || parsed
                .owned_types
                .iter()
                .any(|scope| after < scope.start && scope.start < usage.start)
            || parsed.pattern_declarations.iter().any(|declaration| {
                declaration.token != selected.token
                    && after < declaration.scope_start
                    && declaration.scope_start <= usage.start
                    && usage.start < declaration.scope_end
            })
            || parsed
                .declarations
                .iter()
                .chain(&parsed.potential_bindings)
                .any(|declaration| {
                    declaration.name == usage.text
                        && after < declaration.visible_start
                        && declaration.visible_start <= usage.start
                        && usage.start < declaration.scope_end
                })
            || parsed.uncertain.iter().any(|uncertain| {
                uncertain.start <= usage.start
                    && usage.start < uncertain.end
                    && !(uncertain.start == selected.scope_start
                        && uncertain.end == selected.scope_end
                        && uncertain.name.as_deref() == Some(selected.name.as_str())
                        && uncertain.reason
                            == "Active pattern declaration requires scoped pattern resolution")
            })
        {
            return None;
        }
        Some(parsed.tokens[selected.token].clone())
    }
    /// Strict document-only ABI. A miss must never start contextual navigation.
    fn document_definition(&mut self, p: &Value) -> Result<Value> {
        anyhow::ensure!(
            self.mode == Mode::Indexed,
            "Document definitions require indexed mode"
        );
        let path = path_param(p)?;
        let overlay = self
            .overlays
            .get(&path)
            .context("Document definition requires an acknowledged document")?;
        let mut version_supplied = false;
        for value in [p.get("documentVersion"), p.get("version")]
            .into_iter()
            .flatten()
        {
            let version = value
                .as_i64()
                .context("Document definition requires an integer document version")?;
            anyhow::ensure!(
                version == overlay.version,
                "Stale document definition version"
            );
            version_supplied = true;
        }
        anyhow::ensure!(
            version_supplied,
            "Document definition requires a document version"
        );
        let position = |key: &str| -> Result<u32> {
            let value = p
                .get(key)
                .or_else(|| p.get("position").and_then(|position| position.get(key)))
                .and_then(Value::as_u64)
                .with_context(|| format!("Invalid document definition {key}"))?;
            u32::try_from(value).with_context(|| format!("Document definition {key} exceeds u32"))
        };
        let at = local_lexical::Position {
            line: position("line")?,
            character: position("character")?,
        };
        let mut result=self.document_navigation.query(overlay,at,false,false,&self.cancellation)?
            .unwrap_or_else(||json!({"locations":[],"completeness":"unavailable","source":"local-lexical",
                "documentProofMiss":true,"documentVersion":overlay.version,"version":overlay.version,
                "reason":"No document-local definition proof is available"}));
        result["snapshot"] = json!(self.epoch);
        result["snapshotId"] = json!(self.snapshot);
        self.cancellation.check()?;
        Ok(result)
    }
    fn navigation(&mut self, method: &str, p: &Value) -> Result<Value> {
        let started = Instant::now();
        self.index.counters.query_count += 1;
        let path = path_param(p)?;
        let actual = self.overlays.get(&path).map(|overlay| overlay.version);
        // A null optional version is sent by unversioned protocol clients.
        // If both aliases carry a version, each must match the acknowledged text.
        let mut versioned = false;
        for requested in [p.get("documentVersion"), p.get("version")]
            .into_iter()
            .flatten()
            .filter(|v| !v.is_null())
        {
            versioned = true;
            let v = requested
                .as_i64()
                .context("Invalid query document version")?;
            anyhow::ensure!(
                actual == Some(v),
                "Stale query document version: requested {v}, current {actual:?}"
            );
        }
        let line = u32::try_from(
            p.get("line")
                .or_else(|| p.get("position").and_then(|v| v.get("line")))
                .and_then(Value::as_u64)
                .context("Missing line")?,
        )
        .context("Line exceeds u32")?;
        let col = u32::try_from(
            p.get("character")
                .or_else(|| p.get("position").and_then(|v| v.get("character")))
                .and_then(Value::as_u64)
                .context("Missing character")?,
        )
        .context("Character exceeds u32")?;
        let hover = method == "navigation/hover";
        let mode = self.mode.name();
        let answer_key = |project: Option<&str>| {
            serde_json::to_string(&json!([
                method,
                path,
                line,
                col,
                p.get("includeDeclaration"),
                p.get("traceReferences"),
                mode,
                project
            ]))
        };
        // A prior general proof is only a routing hint. The normal path below
        // still selects the project, drains events and validates dependencies.
        // Repeating a global query should not reparse a failed local proof.
        let local_candidate = self.mode == Mode::Indexed && !hover && versioned;
        let indexed_answer = local_candidate
            && self.model_loaded
            && self
                .index
                .has_answer(&answer_key(self.active_project.as_deref())?, &self.snapshot)?;
        if local_candidate && !indexed_answer {
            if let Some(overlay) = self.overlays.get(&path) {
                let at = local_lexical::Position {
                    line,
                    character: col,
                };
                let include_declaration = p
                    .get("includeDeclaration")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(mut answer) = self.document_navigation.query(
                    overlay,
                    at,
                    method == "navigation/references",
                    include_declaration,
                    &self.cancellation,
                )? {
                    answer["snapshot"] = json!(self.epoch);
                    answer["snapshotId"] = json!(self.snapshot);
                    self.index.counters.query_time_micros += started.elapsed().as_micros() as u64;
                    self.cancellation.check()?;
                    return Ok(answer);
                }
            }
        }
        self.request_definition_can_progress = method == "navigation/definition"
            && p.get("projectPath").and_then(Value::as_str).is_none();
        if self.mode == Mode::Indexed
            && self.evaluated_model_path.is_none()
            && !self.model_loaded
            && matches!(self.background.phase, "scanning" | "limited")
            && p.get("projectPath").and_then(Value::as_str).is_none()
            && method == "navigation/definition"
        {
            self.prepare_source_candidates(&path, line, col)?;
        }
        self.type_proofs = Default::default();
        if self.mode == Mode::Indexed && self.evaluated_model_path.is_none() {
            self.background.requested = true;
            self.background.demand_origin = Some(path.clone());
        }
        self.ensure_model()?;
        self.drain_events()?;
        if !self.model_loaded {
            return Err(background::Pending.into());
        }
        let requested_project = p
            .get("projectPath")
            .and_then(Value::as_str)
            .map(project::from_uri);
        let selected = self
            .model
            .source_instance_project(&path, requested_project.as_deref())
            .map(|p| p.path.clone());
        if requested_project.is_some() && selected.is_none() {
            self.request_definition_can_progress = false;
            return Ok(self.empty(
                method == "navigation/hover",
                "The selected project does not identify this source instance",
            ));
        }
        self.select_project(selected);
        if self.ambiguous_instance(&path) {
            self.request_definition_can_progress = false;
            return Ok(self.empty(
                method == "navigation/hover",
                "Select a project context for this linked source file",
            ));
        }
        self.ensure_path(&path)?;
        let semantic_snapshot = format!("{}:semantic:{}", self.model_hash, self.semantic_revision);
        let semantic_cached = if self.mode == Mode::Indexed
            && !hover
            && self.semantic_loaded
            && !self.model.ambiguous_instance(&path)
        {
            self.index.semantic_answer(
                &path,
                line,
                col,
                &semantic_snapshot,
                method == "navigation/references",
                p.get("includeDeclaration")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )?
        } else {
            None
        };
        let parsed = if self.mode != Mode::Semantic && semantic_cached.is_none() {
            Some(self.parsed(&path)?)
        } else {
            None
        };
        let raw_unproven = self.mode == Mode::Indexed && self.background.raw_model && !hover;
        if let Some(parsed) = &parsed {
            if let Some(token) = parsed.token_at(line, col) {
                let local = parsed.local_binding(token);
                let shadowable_open = local.is_some_and(|d| {
                    d.top_level
                        && d.token != token
                        && parsed.opens.iter().any(|o| {
                            o.offset > parsed.tokens[d.token].start
                                && o.offset < parsed.tokens[token].start
                                && parsed.tokens[token].start < o.scope_end
                        })
                });
                let needs_name_coverage = (local.is_none()
                    && !(Self::has_member_syntax(parsed, token)
                        && method != "navigation/references"))
                    || shadowable_open
                    || (method == "navigation/references" && local.is_some_and(|d| d.top_level));
                if !raw_unproven && needs_name_coverage && !parsed.possible_named_argument(token) {
                    self.ensure_name_coverage(&parsed.tokens[token].text)?;
                }
            }
        }
        let key = answer_key(self.active_project.as_deref())?;
        let mut result = if self.mode == Mode::Semantic {
            self.semantic(method, p)?
        } else if let Some(cached) = semantic_cached {
            cached
        } else if raw_unproven {
            self.empty(false, "Background project membership is provisional; no exact project-wide proof is available")
        } else if let Some(mut cached) = self.index.answer(&key, &self.snapshot)? {
            // Catalog artifacts and their assembly/XML inputs may change while
            // source files remain unchanged. A cached external answer carries
            // that additional dependency; local source answers do not.
            if (cached.get("metadataDependent") == Some(&Value::Bool(true))
                || cached.get("source").and_then(Value::as_str) == Some("metadata-catalog"))
                && !self.ensure_metadata()?
                || cached.get("builtinDependent") == Some(&Value::Bool(true))
                    && !self.ensure_builtins()?
            {
                self.empty(hover, "External metadata is missing or stale")
            } else {
                cached["source"] = json!(format!("{}-cache", self.mode.name()));
                cached
            }
        } else {
            let parsed = parsed.unwrap();
            let operator = if method == "navigation/definition" {
                self.operator_definition(&path, &parsed, line, col)
            } else {
                None
            };
            let mut value = if let Some(operator) = operator {
                json!({"locations":[{"uri":uri(&path),"range":operator.range()}],"completeness":"exact","source":self.mode.name(),"proof":{"kind":"scoped-custom-infix-definition","invalidation":"source-and-project-snapshot"}})
            } else if let Some(token) = parsed.token_at(line, col) {
                if let Some(active_case) = (method == "navigation/definition")
                    .then(|| self.partial_pattern_definition(&path, &parsed, token))
                    .flatten()
                {
                    json!({"locations":[{"uri":uri(&path),"range":active_case.range()}],"completeness":"exact","source":self.mode.name(),"proof":{"kind":"scoped-single-partial-pattern-definition","invalidation":"source-and-project-snapshot"}})
                } else if hover {
                    self.hover_native(&path, &parsed, token)?
                } else if let Some(symbol) = if method == "navigation/references" {
                    self.reference_binding(&path, &parsed, token)?
                } else {
                    self.definition_binding(&path, &parsed, token)?
                } {
                    if method == "navigation/definition" {
                        if let Some(target) =
                            self.definition_target(&path, &parsed, token, symbol.clone())?
                        {
                            let mut answer = json!({"locations":[self.location(&target)?],"completeness":"exact","source":self.mode.name()});
                            if symbol.member {
                                answer["proof"] = json!({"kind":"nominal-receiver-facts","declarationPath":symbol.path,"invalidation":"source-and-project-snapshot"});
                            }
                            answer
                        } else {
                            self.empty(false, "Declaration-to-contract routing is not proven")
                        }
                    } else {
                        self.references(
                            &symbol,
                            p.get("includeDeclaration")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            p.get("traceReferences")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        )?
                    }
                } else if parsed.possible_named_argument(token) {
                    self.empty(
                        false,
                        "The binding of this call argument name is not proven",
                    )
                } else if method == "navigation/definition" {
                    self.external_definition(&path, &parsed, token)?
                } else {
                    let mut value = self.empty(
                        false,
                        "Binding requires unsupported syntax, project context, or type inference",
                    );
                    let candidates = self
                        .index
                        .declaration_candidates(&parsed.tokens[token].text, 200)?;
                    let mut locations = Vec::new();
                    for (f, t) in candidates {
                        let q = self.parsed(&f)?;
                        if let Some(d) = q.declaration_at(t) {
                            locations
                                .push(json!({"uri":uri(&f),"range":q.tokens[d.token].range()}));
                        }
                    }
                    if !locations.is_empty() {
                        value["candidates"] = json!(locations);
                    }
                    value
                }
            } else {
                let reason = if parsed.reasons.is_empty() {
                    "No identifier at the requested position".into()
                } else {
                    parsed.reasons.join("; ")
                };
                self.empty(hover, &reason)
            };
            if self.mode == Mode::Indexed
                && self.background.raw_model
                && value["completeness"] == "exact"
            {
                value = self.empty(hover, "Background project membership is provisional; no exact project-wide proof is available");
            }
            if self.type_proofs.metadata_used {
                value["metadataDependent"] = json!(true);
            }
            if self.type_proofs.builtins_used {
                value["builtinDependent"] = json!(true);
            }
            // Persist only the native proof result; semantic oracle output never
            // leaks into indexed answers with weaker freshness guarantees.
            if self.type_proofs.metadata_used && !self.catalog_valid()?
                || self.type_proofs.builtins_used
                    && !self
                        .builtin_environment
                        .as_ref()
                        .is_some_and(|e| e.valid().unwrap_or(false))
            {
                value=self.empty(hover,"A proof dependency changed during this query; retry after preparing current inputs");
            }
            if value.get("completeness").and_then(Value::as_str) == Some("exact")
                && !(method == "navigation/definition"
                    && self.decompiler_path.is_some()
                    && value.get("symbol").is_some())
            {
                self.cancellation.check()?;
                foreground::check()?;
                self.index.put_answer(&key, &self.snapshot, &value)?;
            }
            value
        };
        // Explicitly supplied offline semantic snapshots are another lazy
        // provider. A local source query does not deserialize/validate them.
        if self.mode == Mode::Indexed
            && !hover
            && !self.model.ambiguous_instance(&path)
            && result.get("completeness").and_then(Value::as_str) != Some("exact")
        {
            if !self.semantic_loaded {
                if let Some(snapshot_path) = self.semantic_path.clone() {
                    if let Err(e) = self.dispatch("semantic/import", &json!({"path":snapshot_path}))
                    {
                        result["diagnostics"] = json!([e.to_string()]);
                    }
                }
            }
            if self.semantic_loaded {
                let key = format!("{}:semantic:{}", self.model_hash, self.semantic_revision);
                if let Some(answer) = self.index.semantic_answer(
                    &path,
                    line,
                    col,
                    &key,
                    method == "navigation/references",
                    p.get("includeDeclaration")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                )? {
                    result = answer;
                }
            }
        }
        if method == "navigation/definition"
            && self.mode != Mode::Semantic
            && result.get("completeness").and_then(Value::as_str) != Some("exact")
        {
            let origin = self.candidate_parsed(&path)?;
            if let Some((name, hint, physical)) = self.candidate_query(&path, &origin, line, col)? {
                self.request_definition_can_progress = true;
                if hint.physical_conditional() {
                    result["candidateOrigin"] = json!("physical-conditional-source");
                }
                self.stash_source_candidates(&path, &origin, physical.as_ref(), &name, &hint)?;
                let (candidates, truncated, ranking) =
                    self.possible_definitions(&path, &name, Some(&hint), physical.as_ref())?;
                if truncated {
                    result["candidateTruncated"] = json!(true);
                    result["candidateTruncationReason"] = json!("Candidate search reached its bounded path, row, or location limit; additional plausible targets may exist");
                }
                if !candidates.is_empty() {
                    // Never promote even a singleton spelling match to an exact answer.
                    ranking.apply(&mut result);
                    result["candidates"] = json!(candidates);
                    result["candidateKind"] = json!("same-name-declarations");
                    result["candidateCompleteness"] = json!("incomplete");
                }
            } else {
                self.request_definition_can_progress = false;
            }
        }
        result["projectContextId"] = json!(self.source_context(&path).map(|c| c.id.clone()));
        result["snapshot"] = json!(self.epoch);
        result["snapshotId"] = json!(self.snapshot);
        result["version"] = json!(actual);
        result["documentVersion"] = json!(actual);
        if self.type_proofs.exhausted {
            result["typeProofBudgetExceeded"] = json!(true);
            result["diagnostics"] =
                json!(["Nominal receiver proof exceeded its bounded query work budget"]);
        }
        self.index.counters.query_time_micros += started.elapsed().as_micros() as u64;
        self.cancellation.check()?;
        Ok(result)
    }
    /// A static formal can be used by name outside its declaration body. Close
    /// that domain only when every call to this method is accounted for, and
    /// no other compiled source can contain another use of the formal's name.
    fn static_parameter_labels(
        &mut self,
        symbol: &Symbol,
        origin: &ParsedFile,
    ) -> Result<Option<HashSet<usize>>> {
        if self.mode != Mode::Indexed
            || self.builtin_path.is_none()
            || self.model.projects.len() != 1
            || self.possible_signature_context(&symbol.path, false)
            || !origin.uncertain.is_empty()
        {
            return Ok(None);
        }
        let Some(method) = origin.method_declarations.iter().find(|m| {
            m.supported
                && m.is_static
                && m.parameters.len() == 1
                && m.parameters[0].binding == Some(symbol.token)
        }) else {
            return Ok(None);
        };
        let mut labels = HashSet::new();
        let mut heads = HashSet::from([method.token]);
        for arg in &origin.call_argument_name_candidates {
            let Some(head) = arg.callee else { continue };
            let Some(head_token) = origin.tokens.get(head) else {
                return Ok(None);
            };
            if head_token.text != method.name {
                continue;
            }
            self.cancellation.check()?;
            if self
                .call_argument_binding(&symbol.path, origin, arg.token)?
                .as_ref()
                != Some(symbol)
            {
                return Ok(None);
            }
            labels.insert(arg.token);
            heads.insert(head);
        }
        if labels.is_empty()
            || origin
                .tokens
                .iter()
                .enumerate()
                .any(|(i, t)| t.ident && t.text == method.name && !heads.contains(&i))
        {
            return Ok(None);
        }
        self.ensure_name_coverage(&symbol.name)?;
        if !self.index_errors.is_empty()
            || self
                .index
                .candidate_paths(&symbol.name)?
                .iter()
                .any(|p| p != &symbol.path)
        {
            return Ok(None);
        }
        Ok(Some(labels))
    }
    fn references(&mut self, symbol: &Symbol, include_decl: bool, trace: bool) -> Result<Value> {
        if !symbol
            .name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Ok(self.empty(false, "Operator reference equivalence is not certified"));
        }
        let canonical = self.canonical_symbol(symbol)?;
        let paired = self.paired_symbol(symbol)?;
        let origin = self.parsed(&symbol.path)?;
        let origin_range = origin.tokens[symbol.token].range();
        if origin.record_types.iter().flat_map(|r| &r.fields).any(|f| {
            f.token == symbol.token
                && matches!(
                    f.kind.as_str(),
                    "abstract-property" | "default-property" | "override-property"
                )
        }) {
            return Ok(self.empty(
                false,
                "Virtual/interface reference equivalence needs a certified slot-identity rule",
            ));
        }
        let declaration = origin.declaration_at(symbol.token);
        let method = origin
            .method_declarations
            .iter()
            .find(|m| m.token == symbol.token);
        let module_value = declaration.is_some_and(|d| d.top_level && d.kind == "value");
        let is_member = origin
            .record_types
            .iter()
            .any(|r| r.fields.iter().any(|f| f.token == symbol.token))
            || method.is_some();
        let is_type = origin.type_aliases.iter().any(|a| a.token == symbol.token)
            || origin.record_types.iter().any(|r| r.token == symbol.token);
        let is_record_field = origin.record_types.iter().any(|record| {
            record.kind == "record"
                && record
                    .fields
                    .iter()
                    .any(|field| field.kind == "field" && field.token == symbol.token)
        });
        if declaration.is_none() && !is_member && !is_type {
            return Ok(self.empty(false, "Missing declaration scope"));
        }
        if declaration.is_some_and(|d| !refs_supported(&origin, d)) {
            return Ok(self.empty(
                false,
                "Declaration scope contains unsupported reference syntax",
            ));
        }
        let mut labels = None;
        let parameter_gap = if symbol.local
            && declaration.is_some_and(|d| d.kind == "parameter")
            && !origin.signature_independent_formals.contains(&symbol.token)
        {
            if !origin.ordinary_function_formals.contains(&symbol.token) {
                labels = self.static_parameter_labels(symbol, &origin)?;
                labels.is_none().then_some(
                    "Callable parameter references may include named arguments outside the body",
                )
            } else if self.possible_signature_context(&symbol.path, true) {
                Some("Function parameter reference identity may include an included signature")
            } else {
                None
            }
        } else {
            None
        };
        let selected = self.active_project.clone();
        let paths = if symbol.local {
            vec![symbol.path.clone()]
        } else {
            self.index.candidate_paths(&symbol.name)?
        };
        let mut locations = Vec::new();
        let mut complete = parameter_gap.is_none();
        self.type_proofs.trace_references = trace;
        let mut gap_count = 0usize;
        let mut examples = Vec::new();
        let mut gap = |value: Value| {
            gap_count += 1;
            if trace && examples.len() < 16 {
                examples.push(value);
            }
        };
        if let Some(reason) = parameter_gap {
            gap(
                json!({"path":symbol.path,"kind":"unclosed-parameter-domain","range":origin_range,"reason":reason}),
            );
        }
        for path in paths {
            self.cancellation.check()?;
            let projects = if symbol.local {
                vec![selected.clone()]
            } else {
                let mut projects = self
                    .model
                    .projects
                    .iter()
                    .filter(|p| p.files.contains(&path))
                    .map(|p| Some(p.path.clone()))
                    .collect::<Vec<_>>();
                projects.sort();
                projects.dedup();
                if projects.is_empty() {
                    projects.push(None);
                }
                projects
            };
            for project in projects {
                self.select_project(project);
                if !symbol.local {
                    let visible = self.visible_files(&path);
                    if !visible.contains(&symbol.path)
                        && !paired.as_ref().is_some_and(|p| visible.contains(&p.path))
                    {
                        continue;
                    }
                }
                if self.ambiguous_instance(&path) {
                    complete = false;
                    if trace {
                        gap(json!({"path":path,"kind":"ambiguous-source-context"}));
                    }
                    continue;
                }
                let parsed = self.parsed(&path)?;
                if parsed.tokens.is_empty() && !parsed.supported {
                    complete = false;
                    if trace {
                        gap(
                            json!({"path":path,"kind":"unavailable-summary","reasons":parsed.reasons}),
                        );
                    }
                    continue;
                }
                if !symbol.local
                    && parsed
                        .uncertain
                        .iter()
                        .any(|u| u.name.as_ref().is_none_or(|n| n == &symbol.name))
                {
                    complete = false;
                    if trace {
                        if let Some(u) = parsed
                            .uncertain
                            .iter()
                            .find(|u| u.name.as_ref().is_none_or(|n| n == &symbol.name))
                        {
                            gap(
                                json!({"path":path,"kind":"syntax-uncertainty","startByte":u.start,"endByte":u.end,"name":u.name,"reason":u.reason}),
                            );
                        }
                    }
                }
                for token in parsed
                    .tokens
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.ident && t.text == symbol.name)
                    .map(|(i, _)| i)
                {
                    self.cancellation.check()?;
                    if symbol.local {
                        let d = declaration.expect("Local symbols have declarations");
                        let t = &parsed.tokens[token];
                        if token != symbol.token
                            && !labels
                                .as_ref()
                                .is_some_and(|labels| labels.contains(&token))
                            && !(d.visible_start <= t.start && t.start < d.scope_end)
                        {
                            continue;
                        }
                    }
                    let argument_binding = if labels
                        .as_ref()
                        .is_some_and(|labels| labels.contains(&token))
                    {
                        Some(symbol.clone())
                    } else if parsed.possible_named_argument(token) {
                        let Some(binding) = self.call_argument_binding(&path, &parsed, token)?
                        else {
                            complete = false;
                            if trace {
                                gap(
                                    json!({"path":path,"kind":"unproven-call-argument-name","range":parsed.tokens[token].range()}),
                                );
                            }
                            continue;
                        };
                        Some(binding)
                    } else {
                        None
                    };
                    if is_record_field && Self::record_field_role_disjoint(&parsed, token)
                        || is_type && Self::type_reference_role_disjoint(&parsed, token)
                    {
                        continue;
                    }
                    let found = if argument_binding.is_some() {
                        argument_binding
                    } else if is_type || parsed.explicit_constructor_heads.contains(&token) {
                        self.reference_binding(&path, &parsed, token)?
                    } else {
                        self.resolve(&path, &parsed, token)?
                    };
                    if let Some(found) = found {
                        let found_canonical = self.canonical_symbol(&found)?;
                        let mut equal = found == *symbol || found_canonical == canonical;
                        if !equal
                            && !symbol.local
                            && found.path == symbol.path
                            && found.name == symbol.name
                            && found.owner == symbol.owner
                            && found.member == symbol.member
                            && found.local == symbol.local
                        {
                            let definition = self.parsed(&found.path)?;
                            equal = definition
                                .tokens
                                .get(found.token)
                                .is_some_and(|t| t.range() == origin_range);
                        }
                        let is_declaration = (path == found.path && token == found.token)
                            || path == symbol.path && parsed.tokens[token].range() == origin_range;
                        if equal && (include_decl || !is_declaration) {
                            let range = if !is_declaration && module_value {
                                parsed.qualified_use_range(token)
                            } else if !is_declaration && is_member {
                                let Some(range) = parsed.member_use_range(token) else {
                                    complete = false;
                                    if trace {
                                        gap(
                                            json!({"path":path,"kind":"unproven-receiver-range","range":parsed.tokens[token].range()}),
                                        );
                                    }
                                    continue;
                                };
                                range
                            } else {
                                parsed.tokens[token].range()
                            };
                            locations.push(json!({"uri":uri(&path),"range":range}));
                            if locations.len() >= 100_000 {
                                complete = false;
                                break;
                            }
                        }
                    } else if self.member_cannot_reference(symbol, &path, &parsed, token)? {
                        continue;
                    } else if !symbol.local
                        || !token_supported(&parsed, token)
                        || parsed.qualifier(token).is_none()
                    {
                        complete = false;
                        if trace {
                            gap(
                                json!({"path":path,"kind":"unresolved-occurrence","range":parsed.tokens[token].range(),"reason":self.type_proofs.reference_notes.get(&(path.clone(),token))}),
                            );
                        }
                    }
                }
                if locations.len() >= 100_000 {
                    break;
                }
            }
            if locations.len() >= 100_000 {
                break;
            }
        }
        self.select_project(selected);
        if !symbol.local
            && (!self.context_exact(&symbol.path)
                || !self.index_errors.is_empty()
                || self.ambiguous_instance(&symbol.path))
        {
            complete = false;
        }
        locations.sort_by_key(|v| serde_json::to_string(v).unwrap_or_default());
        locations.dedup();
        let mut diagnostics = Vec::new();
        if !complete {
            diagnostics.push(
                "Reference domain contains unresolved occurrences or source instances".to_string(),
            );
            if let Some(reason) = parameter_gap {
                diagnostics.push(reason.to_string());
            }
        }
        let mut value = json!({"locations":locations,"completeness":if complete{"exact"}else{"unavailable"},"complete":complete,"source":self.mode.name(),"diagnostics":diagnostics});
        if trace {
            value["proofGaps"] = json!({"count":gap_count,"examples":examples,"exampleLimit":16});
        }
        Ok(value)
    }
    fn external_hover_value(&self, symbol: &Value, range: syntax::Range) -> Value {
        let signature = symbol["signature"]
            .as_str()
            .unwrap_or("Library declaration");
        let docs = symbol["documentation"].as_str().unwrap_or("");
        let markdown = format!("```fsharp\n{signature}\n```\n\n{docs}");
        json!({"hover":{"markdown":markdown,"range":range,"isExternal":true},"contents":markdown,"ownership":"external","completeness":"exact","source":"metadata-catalog","symbol":symbol})
    }
    fn hover_native(&mut self, path: &str, p: &ParsedFile, token: usize) -> Result<Value> {
        if p.possible_named_argument(token) && self.call_argument_binding(path, p, token)?.is_none()
        {
            return Ok(self.empty(true, "Call argument label identity is unproven"));
        }
        if p.explicit_constructor_heads.contains(&token) {
            return Ok(self.empty(
                true,
                "Explicit constructor binding requires a constructor-specific proof",
            ));
        }
        if self.definition_binding(path, p, token)?.is_some() {
            return Ok(
                json!({"hover":null,"contents":null,"ownership":"workspace","completeness":"exact","source":self.mode.name(),"reason":"Repository-owned symbols intentionally have no hover"}),
            );
        }
        if let Some(member) = self.library_member(path, p, token, 0)? {
            let mut symbol = member.symbol;
            if let Some(result) = self.display_proven_type(&member.return_type, 0)? {
                symbol["declarationSignature"] = symbol["signature"].clone();
                symbol["signature"] =
                    json!(format!("property {} : {}", p.tokens[token].text, result));
                symbol["resolvedReturnType"] = serde_json::to_value(&member.return_type)?;
            }
            return Ok(self.external_hover_value(&symbol, p.tokens[token].range()));
        }
        if !token_supported(p, token) {
            return Ok(self.empty(true, "Use-site binding is outside supported scope"));
        }
        let t = &p.tokens[token];
        let Some(qualifier) = p.qualifier(token) else {
            return Ok(self.empty(
                true,
                "External hover currently requires a qualified catalog symbol",
            ));
        };
        let first = qualifier.split('.').next().unwrap_or("");
        if p.declarations
            .iter()
            .any(|d| d.name == first && d.visible_start <= t.start && t.start < d.scope_end)
        {
            return Ok(self.empty(true, "Qualified receiver is a workspace value"));
        }
        let (full, assembly, xml) = match self
            .qualified_nominal_receiver(path, p, token, &qualifier, 0)?
        {
            typeflow::Lookup::Resolved(library_types::TypeExpr::Source { .. }) => {
                return Ok(
                    json!({"hover":null,"contents":null,"ownership":"workspace","completeness":"exact","source":self.mode.name(),"reason":"The qualified nominal owner is declared in the workspace"}),
                )
            }
            typeflow::Lookup::Resolved(library_types::TypeExpr::Named { identity, .. }) => (
                format!("{}.{}", identity.full_name, t.text),
                identity.assembly_path,
                None,
            ),
            _ => {
                let Some((identity, xml)) =
                    self.external_root_module_value(path, p, token, &qualifier)?
                else {
                    return Ok(self.empty(true, "The external owner of this use is not proven"));
                };
                (identity.full_name, identity.assembly_path, Some(xml))
            }
        };
        if !self.ensure_metadata()? {
            return Ok(self.empty(true, "External metadata is missing or stale"));
        }
        let matches = metadata::lookup(self.catalog_db(), &full)?
            .into_iter()
            .filter(|s| {
                s.get("isExternal").and_then(Value::as_bool) == Some(true)
                    && s.get("assemblyPath").and_then(Value::as_str) == Some(assembly.as_str())
                    && xml
                        .as_deref()
                        .is_none_or(|id| s["xmlDocId"] == id && s["fullName"] == full)
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Ok(self.empty(true, "Missing or ambiguous external library metadata"));
        }
        let symbol = &matches[0];
        if symbol
            .get("assemblyPath")
            .and_then(Value::as_str)
            .is_some_and(|s| {
                self.model
                    .projects
                    .iter()
                    .any(|p| p.workspace_assembly_references.iter().any(|r| r == s))
            })
        {
            return Ok(self.empty(true, "Catalog assembly belongs to a workspace project"));
        }
        if symbol
            .get("assemblyPath")
            .and_then(Value::as_str)
            .map(|s| Path::new(s).starts_with(&self.root))
            .unwrap_or(false)
        {
            return Ok(self.empty(
                true,
                "Library ownership cannot be proven external to this repository",
            ));
        }
        Ok(self.external_hover_value(symbol, t.range()))
    }
    fn external_definition(
        &mut self,
        path: &str,
        parsed: &ParsedFile,
        token: usize,
    ) -> Result<Value> {
        let hover = self.hover_native(path, parsed, token)?;
        if hover.get("ownership").and_then(Value::as_str) != Some("external") {
            return Ok(self.empty(
                false,
                "Binding requires type inference or an unambiguous external catalog symbol",
            ));
        }
        if let Some(executable) = &self.decompiler_path {
            let symbol = &hover["symbol"];
            if let (Some(assembly), Some(xml)) =
                (symbol["assemblyPath"].as_str(), symbol["xmlDocId"].as_str())
            {
                let references = self
                    .model
                    .projects
                    .iter()
                    .flat_map(|p| p.assembly_references.iter().cloned())
                    .collect::<std::collections::BTreeSet<_>>();
                let declaring = symbol["memberInfo"]["declaringType"]["fullName"]
                    .as_str()
                    .or_else(|| symbol["declaringType"].as_str());
                let request = json!({"assemblyPath":assembly,"xmlDocId":xml,"declaringType":declaring,"referencePaths":references});
                let output = decompiler::invoke_cached(
                    &self.index.db,
                    Path::new(executable),
                    &self.decompiler_args,
                    &request,
                    &self.cancellation,
                )?;
                self.decompiler_starts += u64::from(output.started);
                self.decompiler_hits += u64::from(output.cache_hit);
                self.decompiler_peak = self.decompiler_peak.max(output.peak_rss_bytes);
                if output.result["completeness"] == "exact" {
                    let mut result = output.result;
                    result["symbol"] = symbol.clone();
                    result["source"] = json!("decompiled-library");
                    return Ok(result);
                }
            }
        }
        let markdown = hover["hover"]["markdown"].as_str().unwrap_or("");
        let signature = markdown
            .strip_prefix("```fsharp\n")
            .and_then(|s| s.split_once("\n```"))
            .map(|p| p.0)
            .unwrap_or(markdown);
        let text = format!(
            "// External library signature (metadata; implementation is not included)\n{}\n",
            signature
        );
        let target = format!(
            "fsharp-navigator://metadata/{}.fsi",
            blake3::hash(format!("{}:{markdown}", self.catalog_fingerprint).as_bytes()).to_hex()
        );
        let range = json!({"start":{"line":1,"character":0},"end":{"line":1,"character":signature.lines().next().unwrap_or("").encode_utf16().count()}});
        Ok(
            json!({"locations":[{"uri":target,"range":range}],"documents":[{"uri":target,"language":"fsharp","text":text}],"completeness":"exact","source":"metadata-catalog","targetKind":"metadata-signature","symbol":hover["symbol"]}),
        )
    }
    fn start_helper(&mut self) -> Result<()> {
        if self.helper.is_none() {
            let path=self.helper_path.as_ref().context("Explicit semantic operation requires helperPath and helperArgs; indexed mode never starts FCS automatically")?;
            self.helper = Some(helper::Helper::spawn(
                path,
                &self.helper_args,
                self.helper_limit_mb,
                self.helper_timeout_ms,
            )?);
            self.index.counters.helper_starts += 1;
        }
        Ok(())
    }
    fn semantic(&mut self, method: &str, p: &Value) -> Result<Value> {
        if path_param(p)
            .ok()
            .is_some_and(|path| self.model.ambiguous_instance(&path))
        {
            return Ok(self.empty(
                method == "navigation/hover",
                "The explicit semantic backend has no certified linked-source instance routing",
            ));
        }
        if let Err(e) = self.start_helper() {
            return Ok(self.empty(method == "navigation/hover", &e.to_string()));
        }
        self.index.counters.compiler_launches = self.index.counters.helper_starts;
        let mut params = p.clone();
        params["root"] = json!(self.root);
        params["documents"] = json!(self.overlays.values().collect::<Vec<_>>());
        let result = self.helper.as_mut().unwrap().call(method, params);
        self.helper_peak_rss = self
            .helper_peak_rss
            .max(self.helper.as_ref().unwrap().peak_rss_bytes);
        match result {
            Ok(mut value) => {
                if let Some(locations) = value.get_mut("locations").and_then(Value::as_array_mut) {
                    for loc in locations {
                        if loc.get("uri").is_none() {
                            if let Some(path) = loc.get("path").and_then(Value::as_str) {
                                loc["uri"] = json!(uri(path));
                            }
                        }
                    }
                }
                value["source"] = json!("semantic-oracle");
                if value.get("completeness").is_none() {
                    value["completeness"] = json!("exact");
                }
                if method == "navigation/hover" {
                    let external = value
                        .get("hover")
                        .and_then(|h| h.get("isExternal"))
                        .and_then(Value::as_bool)
                        == Some(true);
                    if !external {
                        value["hover"] = Value::Null;
                        value["contents"] = Value::Null;
                        value["ownership"] = json!("workspace");
                    } else {
                        value["ownership"] = json!("external");
                        value["contents"] = value["hover"]["markdown"].clone();
                    }
                }
                Ok(value)
            }
            Err(e) => {
                self.helper = None;
                Ok(self.empty(method == "navigation/hover", &e.to_string()))
            }
        }
    }
    fn evaluate(&mut self, p: &Value) -> Result<Value> {
        self.start_helper()?;
        let mut params = p.clone();
        params["root"] = json!(self.root);
        let result = self
            .helper
            .as_mut()
            .unwrap()
            .call("workspace/evaluate", params);
        self.helper_peak_rss = self
            .helper_peak_rss
            .max(self.helper.as_ref().unwrap().peak_rss_bytes);
        self.helper = None;
        let value = result?;
        self.model = Model::from_evaluated(&self.root, &value)?;
        if let Some(observation) = &mut self.demand_observation {
            observation.invalidate();
        }
        self.background.raw_model = false;
        for path in self.model.files.clone() {
            self.refresh_file(&path, false)?;
        }
        self.refresh_context()?;
        self.refresh_snapshot()?;
        Ok(self.model.tree())
    }
    pub fn document_update_trace(&self) -> Option<&Value> {
        self.document_update_trace.as_ref()
    }

    /// Diagnostic identity only; a local proof can succeed before transport cancellation.
    pub fn local_proof_attempt_count(&self) -> u64 {
        self.document_navigation.attempt_count()
    }

    pub fn last_local_proof_attempt(&self) -> Value {
        self.document_navigation.last_attempt()
    }

    pub fn stats(&self) -> Result<Value> {
        let mut stats = serde_json::to_value(&self.index.counters)?;
        self.document_navigation.stats(&mut stats);
        stats["mode"] = json!(self.mode.name());
        stats["modelLoadMicros"] = json!(self.model_load_micros);
        stats["modelValidation"] = json!(self.model.validation_metrics);
        stats["catalogLoadMicros"] = json!(self.catalog_load_micros);
        stats["catalogPrepared"] = json!(self.prepared_catalog.is_some());
        stats["catalogPreparedDatabaseBytesHashed"] = json!(self
            .prepared_catalog
            .as_ref()
            .map_or(0, |c| c.database.descriptor.database_bytes));
        stats["catalogPreparedDescriptorBytesRead"] = json!(self
            .prepared_catalog
            .as_ref()
            .map_or(0, |c| c.database.descriptor_bytes));
        stats["catalogPreparedDependencyBytesHashed"] = json!(self
            .prepared_catalog
            .as_ref()
            .map_or(0, |c| c.dependency_bytes_hashed));

        stats["snapshot"] = json!(self.epoch);
        stats["snapshotId"] = json!(self.snapshot);
        stats["sourceFileCount"] = json!(self.model.files.len());
        stats["indexFileCount"] = json!(self.index.count()?);
        stats["indexBytes"] = json!(self.index.disk_bytes());
        stats["rssBytes"] = json!(helper::rss(std::process::id()));
        stats["helperRssBytes"] = json!(self
            .helper
            .as_ref()
            .map(|h| helper::tree_rss(h.pid()))
            .unwrap_or(0));
        stats["peakHelperRssBytes"] = json!(self.helper_peak_rss);
        stats["decompilerStarts"] = json!(self.decompiler_starts);
        stats["decompilerCacheHits"] = json!(self.decompiler_hits);
        stats["decompilerPeakRssBytes"] = json!(self.decompiler_peak);
        stats["builtinEnvironmentLoaded"] = json!(self.builtin_environment.is_some());
        stats["builtinEnvironmentLoad"] =
            json!(self.builtin_environment.as_ref().map(|e| &e.load_metrics));
        stats["builtinEnvironmentFingerprint"] =
            json!(self.builtin_environment.as_ref().map(|e| &e.fingerprint));
        stats["memoryPolicy"] = json!({"indexCacheMb":self.index_cache_mb,"maxFileBytes":self.index.max_file_bytes,"overlayBudgetBytes":self.overlay_budget,"helperMemoryLimitMb":self.helper_limit_mb,"helperEnforcement":"sampled process-tree RSS; kill on overrun during requests","nativeHardRssCap":false});
        stats["indexErrors"] = json!(self.index_errors);
        stats["background"] = self.background.status();
        stats["watcherActive"] = json!(self._watcher.is_some());
        stats["watcherReliable"] = json!(self.watcher_reliable);
        Ok(stats)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::fs;
    fn fixture() -> (PathBuf, Workspace) {
        let path = std::env::temp_dir().join(format!(
            "navigator-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("Test.fsproj"),r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><Compile Include="A.fs"/><Compile Include="B.fs"/><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
        fs::write(path.join("A.fs"), "module LibraryA\nlet value = 1\n").unwrap();
        fs::write(path.join("B.fs"), "module LibraryB\nlet value = 2\n").unwrap();
        fs::write(path.join("Main.fs"),"module Consumer\nopen LibraryA\nopen LibraryB\nlet result = value\nlet first input =\n    let shared = input + result\n    shared\nlet second input =\n    let shared = input + 1\n    shared\n").unwrap();
        let ws = Workspace::open(&json!({"root":path,"mode":"indexed"})).unwrap();
        (path, ws)
    }
    #[test]
    fn document_update_trace_is_opt_in_current_and_does_not_parse_or_reuse_stale_proofs() {
        let (root, mut ws) = fixture();
        ws.trace_updates = false;
        let path = root.join("Main.fs");
        let text = "module Consumer\nlet mapped = List.map (fun local -> local + 1) [1]\n";
        let query =
            |version, shift| json!({"path":path,"line":1,"character":36+shift,"version":version});
        ws.dispatch(
            "document/open",
            &json!({"path":path,"text":text,"version":1}),
        )
        .unwrap();
        assert!(ws.document_update_trace().is_none());
        let first = ws.dispatch("navigation/definition", &query(1, 0)).unwrap();
        assert_eq!(first["proofScope"], "document", "{first}");
        ws.trace_updates = true;
        let parsed_before = ws.index.counters.files_parsed;
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":text,"version":2}),
        )
        .unwrap();
        let unchanged = ws.document_update_trace().unwrap().clone();
        assert_eq!(unchanged["changed"], false);
        assert_eq!(unchanged["namesCount"], 0);
        for phase in [
            "transactionBeginMicros",
            "postingsDeleteMicros",
            "nameScanMicros",
            "namesInsertMicros",
            "commitMicros",
        ] {
            assert_eq!(unchanged[phase], 0, "unchanged contents must skip {phase}");
        }
        assert!(unchanged["proofCacheEvicted"].as_u64().unwrap() > 0);
        assert_eq!(unchanged["version"], 2);
        assert_eq!(
            unchanged["sourceHash"],
            blake3::hash(text.as_bytes()).to_hex().as_str()
        );
        assert_eq!(
            ws.index.counters.files_parsed, parsed_before,
            "overlay acknowledgement must not construct an AST"
        );
        let second = ws.dispatch("navigation/definition", &query(2, 0)).unwrap();
        assert_eq!(second["locations"], first["locations"]);
        assert_eq!(second["documentVersion"], 2);
        assert_ne!(second["documentProofId"], first["documentProofId"]);
        assert!(
            ws.document_update_trace().is_none(),
            "never expose the prior request trace"
        );
        let edited = text.replace("let mapped", " let mapped");
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":edited,"version":3}),
        )
        .unwrap();
        let changed = ws.document_update_trace().unwrap().clone();
        assert_eq!(changed["changed"], true);
        assert!(changed["namesCount"].as_u64().unwrap() > 0);
        assert_ne!(changed["sourceHash"], unchanged["sourceHash"]);
        assert_eq!(changed["documentKey"], unchanged["documentKey"]);
        assert_eq!(changed["bytes"], edited.len());
        assert!(
            changed["totalMicros"].as_u64().unwrap()
                >= changed["indexUpdateMicros"].as_u64().unwrap()
        );
        assert!(
            changed["indexUpdateMicros"].as_u64().unwrap()
                >= changed["hashMicros"].as_u64().unwrap()
                    + changed["nameScanMicros"].as_u64().unwrap()
                    + changed["lookupMicros"].as_u64().unwrap()
                    + changed["transactionBeginMicros"].as_u64().unwrap()
                    + changed["postingsDeleteMicros"].as_u64().unwrap()
                    + changed["namesInsertMicros"].as_u64().unwrap()
                    + changed["stateWriteMicros"].as_u64().unwrap()
                    + changed["commitMicros"].as_u64().unwrap()
        );
        assert_eq!(ws.index.counters.files_parsed, parsed_before);
        let current = ws.dispatch("navigation/definition", &query(3, 1)).unwrap();
        assert_eq!(current["proofScope"], "document");
        assert_eq!(current["documentVersion"], 3);
        assert_eq!(
            current["locations"][0]["range"]["start"]["character"]
                .as_u64()
                .unwrap(),
            first["locations"][0]["range"]["start"]["character"]
                .as_u64()
                .unwrap()
                + 1
        );
        assert!(ws
            .dispatch(
                "document/change",
                &json!({"path":path,"text":text,"version":2})
            )
            .is_err());
        assert!(ws.document_update_trace().is_none());
        ws.trace_updates = false;
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":edited,"version":4}),
        )
        .unwrap();
        assert!(ws.document_update_trace().is_none());
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Known conditional-branch binder candidate recall gap; post-endif repair does not fix it"]
    fn conditional_origin_retains_physical_tuple_candidates() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let text = "module Main\nlet idx = 0\n#if UNKNOWN\nlet branch idx = idx\n#else\nlet other idx = idx\n#endif\nlet lookup value =\n    match value with\n    | true, idx -> idx\n    | _ -> 0\n";
        fs::write(&path, text).unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        let query = json!({"path":path,"line":9,"character":20});
        let answer = ws.dispatch("navigation/definition", &query).unwrap();
        assert_eq!(answer["locations"], json!([]), "{answer}");
        assert_eq!(answer["candidateCompleteness"], "incomplete", "{answer}");
        for (line, column) in [(1, 4), (3, 11), (5, 10), (9, 12)] {
            let expected = json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":line,"character":column},"end":{"line":line,"character":column+3}}});
            assert!(
                answer["candidates"].as_array().unwrap().contains(&expected),
                "missing {expected}: {answer}"
            );
        }
        assert_ne!(
            ws.dispatch("navigation/references", &query).unwrap()["completeness"],
            "exact"
        );
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conditional_origin_retains_post_endif_tuple_candidates() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let text = "module Main\nlet idx = 0\n#if UNKNOWN\nlet branch idx = idx\n#else\nlet other idx = idx\n#endif\nlet lookup value =\n    match value with\n    | true, idx -> idx\n    | _ -> 0\n";
        fs::write(&path, text).unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        let query = json!({"path":path,"line":9,"character":20});
        let answer = ws.dispatch("navigation/definition", &query).unwrap();
        assert_eq!(answer["locations"], json!([]), "{answer}");
        assert_eq!(answer["candidateCompleteness"], "incomplete", "{answer}");
        for (line, column) in [(1, 4), (9, 12)] {
            let expected = json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":line,"character":column},"end":{"line":line,"character":column+3}}});
            assert!(
                answer["candidates"].as_array().unwrap().contains(&expected),
                "missing {expected}: {answer}"
            );
        }
        assert_ne!(
            ws.dispatch("navigation/references", &query).unwrap()["completeness"],
            "exact"
        );
        let moved = format!("// shifted\n{}", text.replace("idx", "slot"));
        ws.dispatch(
            "document/open",
            &json!({"path":path,"text":moved,"version":2}),
        )
        .unwrap();
        let dirty_query = json!({"path":path,"line":10,"character":21,"version":2});
        let dirty = ws.dispatch("navigation/definition", &dirty_query).unwrap();
        assert_eq!(dirty["locations"], json!([]));
        assert_eq!(dirty["candidateCompleteness"], "incomplete");
        assert_eq!(dirty["candidates"].as_array().unwrap().len(), 2, "{dirty}");
        for (line, column) in [(2, 4), (10, 12)] {
            let expected = json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":line,"character":column},"end":{"line":line,"character":column+4}}});
            assert!(
                dirty["candidates"].as_array().unwrap().contains(&expected),
                "{dirty}"
            );
        }
        for old in answer["candidates"].as_array().unwrap() {
            assert!(
                !dirty["candidates"].as_array().unwrap().contains(old),
                "{dirty}"
            );
        }
        let mut stale = dirty_query.clone();
        stale["version"] = json!(1);
        assert!(ws.dispatch("navigation/definition", &stale).is_err());
        ws.dispatch("document/close", &json!({"path":path}))
            .unwrap();
        assert_eq!(
            ws.dispatch("navigation/definition", &query).unwrap()["candidates"],
            answer["candidates"]
        );
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conditional_origin_additions_do_not_replace_existing_capped_alternatives() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let mut text = String::from("module Main\n");
        for i in 0..200 {
            text.push_str(&format!("let idx = {i}\n"));
        }
        text.push_str("#if UNKNOWN\n#endif\nlet idx = 201\nlet useIt = idx\n");
        fs::write(&path, &text).unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        let result = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":path,"line":204,"character":12}),
            )
            .unwrap();
        assert_eq!(result["locations"], json!([]));
        assert_eq!(result["candidateCompleteness"], "incomplete");
        assert_eq!(result["candidateTruncated"], true, "{result}");
        let candidates = result["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 200);
        for (i, candidate) in candidates.iter().enumerate() {
            assert_eq!(candidate["uri"], uri(path.to_str().unwrap()));
            assert_eq!(
                candidate["range"],
                json!({"start":{"line":i+1,"character":4},"end":{"line":i+1,"character":7}})
            );
        }
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conditional_origin_candidates_keep_competing_targets_and_context_guards() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let text="module Consumer\nmodule LibraryA = LibraryB\n#if UNKNOWN\nlet result = LibraryA.value\n#else\nlet result = LibraryB.value\n#endif\n";
        fs::write(&path, text).unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        for line in [3, 5] {
            let column = text.lines().nth(line).unwrap().find("value").unwrap();
            let params = json!({"path":path,"line":line,"character":column});
            let answer = ws.dispatch("navigation/definition", &params).unwrap();
            assert_ne!(answer["completeness"], "exact", "{answer}");
            assert_eq!(answer["locations"], json!([]));
            assert_eq!(answer["candidateOrigin"], "physical-conditional-source");
            assert_eq!(answer["candidateCompleteness"], "incomplete");
            let values = answer["candidates"].as_array().unwrap();
            for target in ["/A.fs", "/B.fs"] {
                assert!(
                    values
                        .iter()
                        .any(|v| v["uri"].as_str().unwrap().ends_with(target)),
                    "{answer}"
                );
            }
            assert_ne!(
                ws.dispatch("navigation/references", &params).unwrap()["completeness"],
                "exact"
            );
            let mut invalid = params.clone();
            invalid["projectPath"] = json!(root.join("Missing.fsproj"));
            let rejected = ws.dispatch("navigation/definition", &invalid).unwrap();
            assert!(rejected.get("candidates").is_none());
        }
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conditional_origin_candidates_refuse_literal_comments_malformed_and_oversized_sources() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        for (version, text, line) in [
            (
                1,
                "module Consumer\n#if X\nlet text = \"LibraryA.value\"\n#endif\n",
                2,
            ),
            (2, "module Consumer\n#if X\n// LibraryA.value\n#endif\n", 2),
            (
                3,
                "module Consumer\n#if X\nlet text = \"LibraryA.value\n#endif\n",
                2,
            ),
            (
                4,
                "module Consumer\n#nowarn \"40\"\n#if X\nlet result = LibraryA.value\n#endif\n",
                3,
            ),
        ] {
            ws.dispatch(
                "document/open",
                &json!({"path":path,"version":version,"text":text}),
            )
            .unwrap();
            let prefix = ws.raw_candidate_parsed(&normalize(&path)).unwrap();
            let column = text.lines().nth(line).unwrap().find("value").unwrap() as u32;
            assert!(
                ws.candidate_query(&normalize(&path), &prefix, line as u32, column)
                    .unwrap()
                    .is_none(),
                "{text}"
            );
        }
        let oversized = format!(
            "module Consumer\n#if X\nlet result = LibraryA.value\n#endif\n// {}",
            "x".repeat(1024 * 1024)
        );
        ws.dispatch(
            "document/open",
            &json!({"path":path,"version":5,"text":oversized}),
        )
        .unwrap();
        let prefix = ws.raw_candidate_parsed(&normalize(&path)).unwrap();
        let before = ws.index.counters.files_parsed;
        assert!(ws
            .candidate_query(&normalize(&path), &prefix, 2, 22)
            .unwrap()
            .is_none());
        assert_eq!(ws.index.counters.files_parsed, before);
        let cancelled = cancellation::CancellationToken::new();
        cancelled.cancel();
        ws.set_cancellation(cancelled);
        assert!(ws
            .candidate_query(&normalize(&path), &prefix, 2, 22)
            .err()
            .unwrap()
            .is::<cancellation::Cancelled>());
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conditional_origin_pending_queries_keep_continuation_and_dirty_identity() {
        let (root, original) = fixture();
        drop(original);
        let noise = root.join("noise");
        fs::create_dir(&noise).unwrap();
        for n in 0..700 {
            fs::write(noise.join(format!("{n}.txt")), "noise").unwrap();
        }
        let path = root.join("Main.fs");
        let text = "module Consumer\n#if X\nlet result = LibraryA.value\n#endif\n";
        let mut ws = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        ws.dispatch(
            "document/open",
            &json!({"path":path,"version":1,"text":text}),
        )
        .unwrap();
        let params = json!({"path":path,"line":2,"character":22,"version":1});
        ws.dispatch("navigation/definition", &params).unwrap();
        assert_eq!(ws.background.phase, "scanning");
        let pending = ws.dispatch("navigation/definition", &params).unwrap();
        assert_eq!(pending["indexing"]["phase"], "scanning", "{pending}");
        let changed = text.replace("value", "other");
        ws.dispatch(
            "document/change",
            &json!({"path":path,"version":2,"text":changed}),
        )
        .unwrap();
        assert!(ws.dispatch("navigation/definition", &params).is_err());
        ws.dispatch("document/open",&json!({"path":root.join("A.fs"),"version":3,"text":"module LibraryA\nlet other = 1\n"})).unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        let mut current = params;
        current["version"] = json!(2);
        let answer = ws.dispatch("navigation/definition", &current).unwrap();
        assert_eq!(answer["documentVersion"], 2);
        assert_eq!(answer["candidateCompleteness"], "incomplete");
        assert!(answer["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["uri"].as_str().unwrap().ends_with("A.fs")));
        assert_ne!(answer["completeness"], "exact");
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_compile_order_requires_an_unshared_certified_signature_free_project() {
        let (root, mut ws) = fixture();
        let origin = normalize(root.join("Main.fs"));
        let earlier = normalize(root.join("A.fs"));
        let later = normalize(root.join("B.fs"));
        fs::write(&origin, "module Consumer\nlet value = 0\n").unwrap();
        ws.ensure_model().unwrap();
        ws.select_project(Some(ws.model.projects[0].path.clone()));
        ws.model.projects[0].files = vec![earlier.clone(), origin.clone(), later.clone()];
        assert!(
            ws.later_candidate_files(&origin).is_empty(),
            "Provisional XML order"
        );
        for path in ws.model.files.clone() {
            ws.refresh_file(&path, false).unwrap();
        }
        assert_eq!(
            ws.possible_definitions(&origin, "value", None, None)
                .unwrap()
                .0
                .len(),
            3
        );
        let project = &mut ws.model.projects[0];
        project.evaluation = "design-time-fsc-arguments".into();
        project.membership_reusable = Some(true);
        project.compiler_args = project.files.clone();
        let trusted = project.clone();
        assert_eq!(
            ws.later_candidate_files(&origin),
            HashSet::from([later.clone()])
        );
        let narrowed = ws
            .possible_definitions(&origin, "value", None, None)
            .unwrap()
            .0;
        assert_eq!(narrowed.len(), 2);
        for path in [&earlier, &origin] {
            assert!(narrowed.iter().any(|location| location["uri"] == uri(path)));
        }
        // Each missing premise restores all suggestions, never an empty exact result.
        for guard in [
            "inexact",
            "unevaluated",
            "membership",
            "arguments",
            "signature",
            "script",
            "duplicate",
            "reference",
            "shared",
        ] {
            ws.model.projects = vec![trusted.clone()];
            match guard {
                "inexact" => ws.model.projects[0].exact = false,
                "unevaluated" => ws.model.projects[0].evaluation.clear(),
                "membership" => ws.model.projects[0].membership_reusable = None,
                "arguments" => ws.model.projects[0].compiler_args.clear(),
                "signature" => ws.model.projects[0]
                    .files
                    .push(normalize(root.join("Contract.FsI"))),
                "script" => ws.model.projects[0]
                    .files
                    .push(normalize(root.join("Loaded.fsx"))),
                "duplicate" => ws.model.projects[0].files.push(later.clone()),
                "reference" => ws.model.projects[0]
                    .references
                    .push(normalize(root.join("Other.fsproj"))),
                "shared" => {
                    let mut other = trusted.clone();
                    other.path = normalize(root.join("Other.fsproj"));
                    ws.model.projects.push(other);
                }
                _ => unreachable!(),
            }
            assert!(ws.later_candidate_files(&origin).is_empty(), "{guard}");
        }
        ws.model.projects = vec![trusted];
        assert!(ws
            .later_candidate_files(&normalize(root.join("Missing.fs")))
            .is_empty());
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unresolved_definitions_offer_current_candidates_without_promoting_singletons() {
        let (root, mut ws) = fixture();
        let main = root.join("Main.fs");
        let text = "module Consumer\nlet result x = x.value\n";
        ws.dispatch(
            "document/open",
            &json!({"path":main,"version":1,"text":text}),
        )
        .unwrap();
        let request = json!({"path":main,"line":1,"character":18,"version":1});
        let answer = ws.dispatch("navigation/definition", &request).unwrap();
        assert_ne!(answer["completeness"], "exact", "{answer}");
        assert_eq!(answer["locations"], json!([]));
        assert_eq!(answer["candidateCompleteness"], "incomplete");
        let candidates = answer["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 2, "{answer}");
        for file in ["/A.fs", "/B.fs"] {
            assert!(candidates
                .iter()
                .any(|location| location["uri"].as_str().unwrap().ends_with(file)));
        }
        // Rename one declaration without saving and move the other in an overlay.
        ws.dispatch("document/open", &json!({"path":root.join("A.fs"),"version":1,"text":"module LibraryA\nlet other = 1\n"})).unwrap();
        ws.dispatch("document/open", &json!({"path":root.join("B.fs"),"version":1,"text":"module LibraryB\n\nlet value = 2\n"})).unwrap();
        let current = ws.dispatch("navigation/definition", &request).unwrap();
        assert_ne!(current["completeness"], "exact", "{current}");
        let candidates = current["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{current}");
        assert!(candidates[0]["uri"].as_str().unwrap().ends_with("/B.fs"));
        assert_eq!(candidates[0]["range"]["start"]["line"], 2);
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unresolved_record_projection_includes_both_field_declarations() {
        let (root, mut ws) = fixture();
        let main = root.join("Main.fs");
        let text = "module Consumer\ntype Person = { Name: string }\ntype Company = { Name: string }\nlet read (people: Person list) = (List.head people).Name\n";
        ws.dispatch(
            "document/open",
            &json!({"path":main,"version":1,"text":text}),
        )
        .unwrap();
        let request = json!({"path":main,"line":3,"character":text.lines().nth(3).unwrap().rfind("Name").unwrap(),"version":1});
        let answer = ws.dispatch("navigation/definition", &request).unwrap();
        assert_ne!(answer["completeness"], "exact", "{answer}");
        assert_eq!(answer["locations"], json!([]));
        let candidates = answer["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 2, "{answer}");
        // The first record is the known target; the second is an explicit extra candidate.
        assert!(candidates
            .iter()
            .any(|location| location["range"]["start"]["line"] == 1));
        assert!(candidates
            .iter()
            .any(|location| location["range"]["start"]["line"] == 2));
        assert_eq!(ws.index.counters.helper_starts, 0);
        let cancelled = cancellation::CancellationToken::new();
        cancelled.cancel();
        ws.set_cancellation(cancelled);
        assert!(ws
            .dispatch("navigation/definition", &request)
            .unwrap_err()
            .is::<cancellation::Cancelled>());
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn alias_rhs_tier_keeps_field_and_ambiguous_case_alternatives_in_the_union() {
        let (root, mut ws) = fixture();
        fs::write(root.join("A.fs"), "module Types\ntype Payload = string\n").unwrap();
        fs::write(
            root.join("B.fs"),
            "module Other\ntype Holder = { Payload: string }\n",
        )
        .unwrap();
        fs::write(
            root.join("Main.fs"),
            "module Consumer\nopen Types\ntype Wrapper = Payload\n",
        )
        .unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        ws.background.raw_model = true;
        let result = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":root.join("Main.fs"),"line":2,"character":16}),
            )
            .unwrap();
        assert_eq!(result["completeness"], "unavailable", "{result}");
        assert_eq!(result["locations"], json!([]));
        assert_eq!(
            result["candidates"].as_array().unwrap().len(),
            3,
            "{result}"
        );
        for file in ["A.fs", "B.fs", "Main.fs"] {
            assert!(result["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|location| location["uri"] == uri(root.join(file).to_str().unwrap())));
        }
        assert_eq!(result["likelyCandidates"].as_array().unwrap().len(), 1);
        assert_eq!(
            result["likelyCandidates"][0]["uri"],
            uri(root.join("A.fs").to_str().unwrap())
        );
        assert!(result["candidateEvidence"][0]["evidence"]
            .as_array()
            .unwrap()
            .contains(&json!("alias-rhs-declaration-shape")));
        assert_eq!(result["candidateCompleteness"], "incomplete");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn heuristic_qualifier_can_be_wrong_but_other_tier_keeps_the_alias_target() {
        let (root, mut ws) = fixture();
        fs::write(root.join("A.fs"), "module Chosen\nlet target = 1\n").unwrap();
        fs::write(root.join("B.fs"), "module Other\nlet target = 2\n").unwrap();
        fs::write(
            root.join("Main.fs"),
            "module Consumer\nmodule Chosen = Other\nlet x = Chosen.target\n",
        )
        .unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        ws.background.raw_model = true;
        let result = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":root.join("Main.fs"),"line":2,"character":17}),
            )
            .unwrap();
        assert_ne!(result["completeness"], "exact");
        assert_eq!(
            result["candidates"].as_array().unwrap().len(),
            2,
            "{result}"
        );
        assert!(result["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|location| location["uri"] == uri(root.join("B.fs").to_str().unwrap())));
        assert_eq!(
            result["likelyCandidates"][0]["uri"],
            uri(root.join("A.fs").to_str().unwrap())
        );
        // Ready-model punctuation cannot become usable merely by indexing more files.
        ws.background.requested = true;
        ws.background.phase = "indexing";
        let punctuation = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":root.join("Main.fs"),"line":2,"character":6}),
            )
            .unwrap();
        assert!(punctuation.get("indexing").is_none(), "{punctuation}");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn same_source_tier_ties_keep_every_collected_candidate_available() {
        let (root, mut ws) = fixture();
        ws.background.requested = true;
        ws.background.phase = "scanning";
        let mut source = String::from("module Consumer\n");
        for n in 0..7 {
            source.push_str(&format!("type T{n} = {{ Name: string }}\n"));
        }
        source.push_str("let read x = x.Name\n");
        let path = root.join("Main.fs");
        ws.dispatch(
            "document/open",
            &json!({"path":path,"version":1,"text":source}),
        )
        .unwrap();
        let result = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":path,"line":8,"character":16,"version":1}),
            )
            .unwrap();
        assert_eq!(
            result["candidates"].as_array().unwrap().len(),
            7,
            "{result}"
        );
        assert_eq!(result["likelyCandidates"].as_array().unwrap().len(), 5);
        assert_eq!(result["likelyCandidateTotal"], 7);
        assert_eq!(result["candidateCompleteness"], "incomplete");
        assert_eq!(result["completeness"], "unavailable");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_candidates_rerank_dirty_annotations_without_losing_the_union() {
        let (root, mut ws) = fixture();
        ws.background.requested = true;
        ws.background.phase = "scanning";
        let path = root.join("Main.fs");
        let source = "module Consumer\ntype Person = { Name: string }\ntype Company = { Name: string }\nlet read (x: Person) = x.Name\n";
        for (version, source, expected_line) in [
            (1, source.to_string(), 1),
            (2, source.replace("x: Person", "x: Company"), 2),
        ] {
            let column = source.lines().nth(3).unwrap().find("Name").unwrap();
            ws.dispatch(
                "document/open",
                &json!({"path":path,"version":version,"text":source}),
            )
            .unwrap();
            let result = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":path,"line":3,"character":column,"version":version}),
                )
                .unwrap();
            assert_eq!(result["completeness"], "unavailable", "{result}");
            assert_eq!(result["locations"], json!([]));
            assert_eq!(result["candidates"].as_array().unwrap().len(), 2);
            assert_eq!(result["likelyCandidates"].as_array().unwrap().len(), 1);
            assert_eq!(
                result["likelyCandidates"][0]["range"]["start"]["line"],
                expected_line
            );
            assert!(
                result.get("indexing").is_none(),
                "Positive suggestions must not wait: {result}"
            );
        }
        let missing = json!({"path":path,"line":3,"character":0,"version":2});
        assert!(ws
            .dispatch("navigation/definition", &missing)
            .unwrap()
            .get("indexing")
            .is_none());
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_source_alias_and_unconditional_crossfile_candidates_stay_incomplete_and_current() {
        let (root, mut ws) = fixture();
        let noise = root.join("noise");
        fs::create_dir(&noise).unwrap();
        for n in 0..700 {
            fs::write(noise.join(format!("{n}.txt")), "noise").unwrap();
        }
        let main = root.join("Main.fs");
        let source = "module Consumer\ntype Hash = string\nlet useHash (value: Hash) = value\nlet call value = LibraryA.serialize value\nlet branch = LibraryA.hidden\n";
        let dependency = root.join("A.fs");
        fs::write(
            &dependency,
            "module LibraryA\nlet serialize value = value\n#if DEBUG\nlet hidden = 1\n#endif\n",
        )
        .unwrap();
        ws.dispatch(
            "document/open",
            &json!({"path":main,"version":1,"text":source}),
        )
        .unwrap();
        let alias_request = json!({"path":main,"line":2,"character":21,"version":1});
        let first = ws
            .dispatch("navigation/definition", &alias_request)
            .unwrap();
        assert_eq!(first["completeness"], "unavailable");
        assert!(!ws.model_loaded);
        let early = ws
            .dispatch("navigation/definition", &alias_request)
            .unwrap();
        assert_eq!(
            early["candidateKind"], "same-source-declarations",
            "{early}"
        );
        assert_eq!(early["candidateCompleteness"], "incomplete");
        assert_eq!(early["locations"], json!([]));
        assert_eq!(early["candidates"][0]["range"]["start"]["line"], 1);
        let pending = ws
            .dispatch("navigation/definition", &alias_request)
            .unwrap();
        assert_eq!(pending["candidates"], early["candidates"]);
        assert!(!ws.model_loaded);
        for _ in 0..2000 {
            ws.background_step(cancellation::CancellationToken::new())
                .unwrap();
            if !ws.background_pending() {
                break;
            }
        }
        assert_eq!(ws.background.phase, "ready");
        let request = json!({"path":main,"line":3,"character":29,"version":1});
        let answer = ws.dispatch("navigation/definition", &request).unwrap();
        assert_eq!(answer["completeness"], "unavailable", "{answer}");
        assert_eq!(answer["candidateCompleteness"], "incomplete");
        assert!(answer["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |location| location["uri"] == uri(dependency.to_str().unwrap())
                    && location["range"]["start"]["line"] == 1
            ));
        let hidden = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":4,"character":24,"version":1}),
            )
            .unwrap();
        assert!(
            hidden.get("candidates").is_none(),
            "conditional branch must not be guessed: {hidden}"
        );
        ws.dispatch("document/open", &json!({"path":dependency,"version":1,"text":"module LibraryA\nlet renamed value = value\n#if DEBUG\nlet hidden = 1\n#endif\n"})).unwrap();
        let edited = ws.dispatch("navigation/definition", &request).unwrap();
        assert!(
            edited.get("candidates").is_none(),
            "stale declaration must disappear: {edited}"
        );
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_index_rebuilds_a_completed_provisional_model_before_exact_navigation() {
        let (root, mut ws) = fixture();
        let noise = root.join("noise");
        fs::create_dir(&noise).unwrap();
        for n in 0..700 {
            fs::write(noise.join(format!("{n}.txt")), "noise").unwrap();
        }
        let path = root.join("A.fs");
        ws.dispatch(
            "document/open",
            &json!({"path":path,"version":1,"text":fs::read_to_string(&path).unwrap()}),
        )
        .unwrap();
        let request = json!({"path":path,"line":1,"character":5,"version":1});
        ws.dispatch("navigation/definition", &request).unwrap();
        for _ in 0..2000 {
            ws.background_step(cancellation::CancellationToken::new())
                .unwrap();
            if !ws.background_pending() {
                break;
            }
        }
        assert_eq!(ws.background.phase, "ready");
        assert!(ws.background.raw_model);
        assert_ne!(
            ws.dispatch("navigation/definition", &request).unwrap()["completeness"],
            "exact"
        );
        ws.dispatch("workspace/index", &json!({})).unwrap();
        let answer = ws.dispatch("navigation/definition", &request).unwrap();
        assert_eq!(answer["completeness"], "exact", "{answer}");
        assert_eq!(
            answer["locations"],
            json!([{"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":1,"character":4},"end":{"line":1,"character":9}}}])
        );
        assert!(!ws.background.raw_model);
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_provisional_rebuild_keeps_cancellation_and_unknown_imports_guarded() {
        let (root, mut ws) = fixture();
        ws.ensure_model().unwrap();
        ws.background.raw_model = true;
        ws.background.requested = true;
        ws.background.phase = "ready";
        for project in &mut ws.model.projects {
            project.exact = false;
        }
        let cancelled = cancellation::CancellationToken::new();
        cancelled.cancel();
        ws.set_cancellation(cancelled.clone());
        {
            let _scope = cancelled.enter();
            let error = ws.prepare_explicit_index_model().unwrap_err();
            assert!(error.is::<cancellation::Cancelled>());
        }
        assert!(!ws.model_loaded);
        assert!(ws.background.raw_model);
        assert_eq!(ws.background.phase, "scanning");
        ws.set_cancellation(cancellation::CancellationToken::new());
        let project = root.join("Test.fsproj");
        fs::write(
            &project,
            fs::read_to_string(&project).unwrap().replace(
                "</Project>",
                r#"<Import Project="Unknown.targets" /></Project>"#,
            ),
        )
        .unwrap();
        ws.prepare_explicit_index_model().unwrap();
        assert!(!ws.background.raw_model);
        assert!(ws.model.projects.iter().all(|project| !project.exact));
        assert!(!ws.context_exact(root.join("A.fs").to_str().unwrap()));
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn small_raw_discovery_observes_empty_directories_and_rescans_new_projects() {
        let (root, mut ws) = fixture();
        let empty = root.join("nested/empty.v1");
        fs::create_dir_all(&empty).unwrap();
        ws.ensure_model().unwrap();
        assert!(ws.discovery_directories.contains(&empty));
        let exact_before = ws.model.projects[0].exact;
        ws.background.requested = true;
        for _ in 0..20 {
            ws.background_step(cancellation::CancellationToken::new())
                .unwrap();
        }
        assert_eq!(ws.model.projects[0].exact, exact_before);
        assert!(!ws.background.raw_model);
        let (tx, rx) = mpsc::channel();
        ws.events = rx;
        tx.send(Ok(notify::Event::new(notify::EventKind::Create(
            notify::event::CreateKind::File,
        ))
        .add_path(empty.join("Injected.fsproj"))))
            .unwrap();
        let first = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":root.join("Main.fs"),"line":3,"character":13}),
            )
            .unwrap();
        assert_eq!(first["completeness"], "unavailable", "{first}");
        assert!(!ws.model_loaded);
        let tree = ws.dispatch("workspace/tree", &json!({})).unwrap();
        assert_eq!(tree["completeness"], "unavailable");
        assert!(tree["projects"].as_array().unwrap().is_empty());
        // Restore the original foreground model and its actual watchers.
        ws.background = Default::default();
        ws.ensure_model().unwrap();
        fs::write(
            empty.join("New.fsproj"),
            r#"<Project><ItemGroup><Compile Include="New.fs" /></ItemGroup></Project>"#,
        )
        .unwrap();
        fs::write(empty.join("New.fs"), "module New\nlet added = 1\n").unwrap();
        // Observe the real nonrecursive watcher, not a fabricated notification.
        for _ in 0..100 {
            ws.background_step(cancellation::CancellationToken::new())
                .unwrap();
            if !ws.model_loaded || ws.background.raw_model {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            !ws.model_loaded,
            "new project in previously empty directory must invalidate"
        );
        assert_eq!(ws.background.phase, "scanning");
        for _ in 0..100 {
            ws.background_step(cancellation::CancellationToken::new())
                .unwrap();
            if !ws.background_pending() {
                break;
            }
        }
        assert_eq!(ws.background.phase, "ready");
        assert_eq!(ws.model.projects.len(), 2);
        assert!(ws.model.projects.iter().all(|project| !project.exact));
        ws.dispatch(
            "workspace/filesChanged",
            &json!({"paths":[empty.join("Later.fsproj")]}),
        )
        .unwrap();
        assert!(!ws.model_loaded);
        assert_eq!(ws.background.phase, "scanning");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn progressive_discovery_preserves_local_proofs_and_revalidates_restart_rows() {
        let (root, original) = fixture();
        drop(original);
        let noise = root.join("noise");
        fs::create_dir(&noise).unwrap();
        for n in 0..700 {
            fs::write(noise.join(format!("{n}.txt")), "noise").unwrap();
        }
        let main = root.join("Main.fs");
        let text = fs::read_to_string(&main).unwrap()
            + "let mapped = List.map (fun local -> local + 1) [1]\n";
        fs::write(&main, &text).unwrap();
        let request = json!({"path":main,"line":3,"character":13});
        for restart in [false, true] {
            let mut ws = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
            let cold = ws.dispatch("navigation/definition", &request).unwrap();
            assert_eq!(cold["completeness"], "unavailable");
            assert_eq!(ws.background.phase, "scanning");
            ws.dispatch(
                "document/open",
                &json!({"path":main,"text":text,"version":1}),
            )
            .unwrap();
            let local = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":main,"line":10,"character":37,"version":1}),
                )
                .unwrap();
            assert_eq!(local["completeness"], "exact", "{local}");
            assert_eq!(local["proofScope"], "document");
            let cancelled = cancellation::CancellationToken::new();
            cancelled.cancel();
            ws.background_step(cancelled).unwrap();
            assert_eq!(ws.background.phase, "scanning");
            for _ in 0..2000 {
                if !ws.background_pending() {
                    break;
                }
                ws.background_step(cancellation::CancellationToken::new())
                    .unwrap();
                if ws.background.phase == "scanning" {
                    assert!(!ws.model_loaded);
                }
            }
            let stats = ws.stats().unwrap();
            assert_eq!(stats["background"]["phase"], "ready", "{stats}");
            assert!(stats["background"]["entriesVisited"].as_u64().unwrap() > 512);
            assert!(ws.model.projects.iter().all(|p| !p.exact));
            let result = ws.dispatch("navigation/definition", &request).unwrap();
            assert_ne!(result["completeness"], "exact", "{result}");
            assert!(
                result["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["uri"].as_str().unwrap().ends_with("/B.fs")),
                "{result}"
            );
            assert_eq!(ws.index.counters.helper_starts, 0);
            if restart {
                assert!(stats["background"]["filesReused"].as_u64().unwrap() >= 2);
            }
            drop(ws);
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn local_proof_survives_cancelled_discovery_and_new_overlay_generations() {
        let (root, original) = fixture();
        drop(original);
        let noise = root.join("noise");
        fs::create_dir(&noise).unwrap();
        for n in 0..700 {
            fs::write(noise.join(format!("{n}.txt")), "noise").unwrap();
        }
        let path = root.join("Main.fs");
        let text = "module Main\nlet mapped = List.map (fun local -> local + 1) [1]\n";
        fs::write(&path, text).unwrap();
        let mut ws = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        ws.dispatch(
            "navigation/definition",
            &json!({"path":path,"line":0,"character":8}),
        )
        .unwrap();
        assert_eq!(ws.background.phase, "scanning");
        for version in 1..=4 {
            let padding = " ".repeat(version as usize - 1);
            let dirty = text.replace("let mapped", &format!("{padding}let mapped"));
            let line = dirty.lines().nth(1).unwrap();
            let declaration = line.find("local ->").unwrap();
            ws.set_cancellation(cancellation::CancellationToken::new());
            ws.dispatch(
                if version == 1 {
                    "document/open"
                } else {
                    "document/change"
                },
                &json!({"path":path,"text":dirty,"version":version}),
            )
            .unwrap();
            let params = json!({"path":path,"line":1,"character":declaration+9,"version":version});
            let cancelled = cancellation::CancellationToken::new();
            cancelled.cancel();
            ws.set_cancellation(cancelled);
            assert!(ws
                .dispatch("navigation/definition", &params)
                .unwrap_err()
                .is::<cancellation::Cancelled>());
            ws.set_cancellation(cancellation::CancellationToken::new());
            let answer = ws.dispatch("navigation/definition", &params).unwrap();
            assert_eq!(answer["proofScope"], "document", "{answer}");
            assert_eq!(answer["documentVersion"], version);
            assert_eq!(
                answer["locations"][0]["range"]["start"]["character"],
                declaration
            );
            assert_eq!(
                answer["sourceHash"],
                blake3::hash(dirty.as_bytes()).to_hex().as_str()
            );
            let before_snapshot = ws.snapshot.clone();
            let before_loaded = ws.model_loaded;
            let preempted = cancellation::CancellationToken::new();
            preempted.cancel();
            ws.background_step(preempted).unwrap();
            for _ in 0..40 {
                ws.background_step(cancellation::CancellationToken::new())
                    .unwrap();
            }
            if version == 1 {
                assert!(!before_loaded);
                assert!(
                    ws.model_loaded,
                    "the test must exercise atomic publication after a local proof"
                );
                assert_ne!(ws.snapshot, before_snapshot);
            }
            ws.set_cancellation(cancellation::CancellationToken::new());
            let after = ws.dispatch("navigation/definition", &params).unwrap();
            assert_eq!(after["locations"], answer["locations"]);
            assert_eq!(after["sourceHash"], answer["sourceHash"]);
            assert_eq!(
                ws.stats().unwrap()["localProofLastAttempt"]["outcome"],
                "cache-hit"
            );
            if version > 1 {
                let mut stale = params.clone();
                stale["version"] = json!(version - 1);
                assert!(ws.dispatch("navigation/definition", &stale).is_err());
            }
        }
        assert_eq!(ws.background.phase, "ready");
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn background_mode_changes_stop_work_and_terminal_queries_do_not_restart_it() {
        let (root, mut ws) = fixture();
        ws.background.requested = true;
        ws.background.raw_model = true;
        ws.background.phase = "scanning";
        ws.dispatch("workspace/mode", &json!({"mode":"semantic"}))
            .unwrap();
        assert!(!ws.background_pending());
        ws.background_step(cancellation::CancellationToken::new())
            .unwrap();
        assert!(!ws.model_loaded);
        assert_eq!(ws.index.counters.helper_starts, 0);
        ws.dispatch("workspace/mode", &json!({"mode":"indexed"}))
            .unwrap();
        assert!(ws.background_pending());
        ws.background.phase = "limited";
        ws.background.reason = Some("depth limit".into());
        let response = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":root.join("Main.fs"),"line":3,"character":13}),
            )
            .unwrap();
        assert_eq!(response["completeness"], "unavailable");
        assert!(response["reason"]
            .as_str()
            .unwrap()
            .contains("stopped: depth limit"));
        assert_eq!(ws.background.phase, "limited");
        assert_eq!(ws.background.slices, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bounded_discovery_and_cold_reads_recover_after_explicit_index() {
        let (root, initial) = fixture();
        drop(initial);
        let mut project = String::from("<Project><ItemGroup>");
        for number in 0..foreground::MAX_READS + 16 {
            let name = format!("N{number}.fs");
            fs::write(
                root.join(&name),
                format!("module N{number}\nlet value = 3\n"),
            )
            .unwrap();
            project.push_str(&format!("<Compile Include=\"{name}\"/>"));
        }
        project.push_str("</ItemGroup></Project>");
        fs::write(root.join("Unrelated.fsproj"), project).unwrap();
        let noise = root.join("noise");
        fs::create_dir(&noise).unwrap();
        for number in 0..foreground::MAX_ENTRIES {
            fs::write(noise.join(number.to_string()), "").unwrap();
        }
        let mut ws = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        let cursor = json!({"path":root.join("Main.fs"),"line":3,"character":13});
        let query = |ws: &mut Workspace| ws.dispatch("navigation/definition", &cursor).unwrap();
        let cold = query(&mut ws);
        assert_eq!(cold["completeness"], "unavailable", "{cold}");
        assert_eq!(cold["indexing"]["phase"], "scanning");
        assert!(!ws.model_loaded);
        assert_eq!(ws.index.counters.files_read, 0);
        let pending_tree = ws.dispatch("workspace/tree", &json!({})).unwrap();
        assert_eq!(pending_tree["completeness"], "unavailable");
        assert_eq!(pending_tree["nodes"], json!([]));
        ws.dispatch("workspace/index", &json!({})).unwrap();
        let tree = ws.dispatch("workspace/tree", &json!({})).unwrap();
        assert_eq!(tree["completeness"], "approximate");
        assert_eq!(tree["nodes"].as_array().unwrap().len(), 2);
        let unrelated = tree["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["label"] == "Unrelated")
            .unwrap();
        assert_eq!(
            unrelated["children"].as_array().unwrap().len(),
            foreground::MAX_READS + 16
        );
        assert!(
            ws.model
                .projects
                .iter()
                .all(|project| project.items.is_empty()),
            "Plain compile items only need their ordered physical path"
        );
        assert!(ws.index_errors.is_empty());
        let reads = ws.index.counters.files_read;
        let result = query(&mut ws);
        assert_eq!(result["completeness"], "exact", "{result}");
        assert!(result["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/B.fs"));
        assert!(ws.index.counters.files_read - reads < 6);
        assert!(ws.index.counters.files_parsed < 6);
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cold_evaluated_models_obey_size_and_membership_limits_then_explicit_index_recovers() {
        use sha2::{Digest, Sha256};
        for large_manifest in [false, true] {
            let (root, original) = fixture();
            drop(original);
            let project = root.join("Test.fsproj");
            let mut files = vec![root.join("A.fs"), root.join("B.fs"), root.join("Main.fs")];
            if !large_manifest {
                for n in 0..foreground::MAX_ENTRIES {
                    let path = root.join(format!("Extra{n}.fs"));
                    fs::write(&path, format!("module Extra{n}\n")).unwrap();
                    files.push(path);
                }
            }
            let mut model = json!({"completeness":"exact","projects":[{
                "path":project,"name":"Test","files":files,"membershipReusable":true,
                "environmentCertificate":project::environment::empty_certificate()
            }],"inputs":[project],"inputHashes":[{"path":project,"sha256":format!("{:x}",Sha256::digest(fs::read(&project).unwrap()))}]});
            if large_manifest {
                model["padding"] = json!(" ".repeat(4 * 1024 * 1024));
            }
            let manifest = root.join("model.json");
            fs::write(&manifest, model.to_string()).unwrap();
            let mut ws =
                Workspace::open(&json!({"root":root,"evaluatedModelPath":manifest})).unwrap();
            let cursor = json!({"path":root.join("Main.fs"),"line":3,"character":13});
            let cold = ws.dispatch("navigation/definition", &cursor).unwrap();
            assert_eq!(cold["completeness"], "unavailable", "{cold}");
            assert!(!ws.model_loaded);
            assert_eq!(ws.index.counters.files_read, 0);
            let ready = ws.dispatch("workspace/index", &json!({})).unwrap();
            assert_eq!(ready["sourceFileCount"], files.len());
            let def = ws.dispatch("navigation/definition", &cursor).unwrap();
            assert_eq!(def["completeness"], "exact", "{def}");
            assert!(def["locations"][0]["uri"]
                .as_str()
                .unwrap()
                .ends_with("/B.fs"));
            assert_eq!(ws.index.counters.helper_starts, 0);
            drop(ws);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn oversized_configured_snapshot_is_imported_only_on_explicit_request() {
        for large in [false, true] {
            let (root, original) = fixture();
            drop(original);
            let file = root.join("Main.fs");
            let text = "module Probe\nlet v = Unproven\n";
            fs::write(&file, text).unwrap();
            let range = json!({"start":{"line":1,"character":8},"end":{"line":1,"character":16}});
            let declaration = json!({"path":file,"range":range});
            use sha2::{Digest, Sha256};
            let mut snapshot = json!({"kind":"semantic-snapshot","factsVersion":2,"projects":[{
                "projectPath":root.join("Test.fsproj"),"completeness":"exact","contextId":"test",
                "sourceMembership":[],"membershipReusable":true,"environmentCertificate":project::environment::empty_certificate(),
                "sourceHashes":[{"path":file,"sha256":format!("{:x}",Sha256::digest(text.as_bytes()))}],
                "occurrences":[{"symbolId":"probe","path":file,"range":range,"declaration":declaration,"isDefinition":false}]
            }]});
            if large {
                snapshot["padding"] = json!(" ".repeat(4 * 1024 * 1024));
            }
            let artifact = root.join("snapshot.json");
            fs::write(&artifact, snapshot.to_string()).unwrap();
            let mut ws =
                Workspace::open(&json!({"root":root,"semanticSnapshotPath":artifact})).unwrap();
            let cursor = json!({"path":file,"line":1,"character":9});
            let mut answer = ws.dispatch("navigation/definition", &cursor).unwrap();
            if large {
                assert_eq!(answer["completeness"], "unavailable", "{answer}");
                assert!(!ws.semantic_loaded);
                ws.dispatch("workspace/index", &json!({})).unwrap();
                answer = ws.dispatch("navigation/definition", &cursor).unwrap();
            }
            assert!(ws.semantic_loaded);
            assert_eq!(answer["completeness"], "exact", "{answer}");
            assert_eq!(answer["locations"][0]["range"], range);
            assert_eq!(ws.index.counters.helper_starts, 0);
            drop(ws);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn warm_indexed_definitions_skip_repeated_local_parsing_and_rebind_after_an_edit() {
        let (root, mut ws) = fixture();
        let main = root.join("Main.fs");
        ws.dispatch(
            "document/open",
            &json!({"path":main,"text":fs::read_to_string(&main).unwrap(),"version":1}),
        )
        .unwrap();
        let cursor = json!({"path":main,"line":3,"character":13,"version":1});
        let first = ws.dispatch("navigation/definition", &cursor).unwrap();
        assert!(first["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/B.fs"));
        let before = ws.stats().unwrap()["localProofParsedBytes"]
            .as_u64()
            .unwrap();
        assert!(before > 0);
        let repeated = ws.dispatch("navigation/definition", &cursor).unwrap();
        assert_eq!(repeated["locations"], first["locations"]);
        assert_eq!(ws.stats().unwrap()["localProofParsedBytes"], before);
        ws.dispatch("document/open", &json!({"path":root.join("B.fs"),"text":"module LibraryB\nlet other = 2\n","version":1})).unwrap();
        let edited = ws.dispatch("navigation/definition", &cursor).unwrap();
        assert_eq!(edited["completeness"], "exact");
        assert!(edited["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/A.fs"));
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lambda_documentation_candidates_refresh_dirty_ranges_and_versions() {
        let (root, mut ws) = fixture();
        let main = root.join("Main.fs");
        let source = "module Main\nlet f = fun n ->\n    match n with\n    | true, res -> res\n    | _ -> 0\n/// Next declaration\nlet next = 1\n";
        fs::write(&main, source).unwrap();
        ws.dispatch("workspace/index", &json!({})).unwrap();
        let exact = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":3,"character":19}),
            )
            .unwrap();
        assert_eq!(exact["completeness"], "exact", "{exact}");
        assert_eq!(
            exact["locations"],
            json!([{"uri":uri(main.to_str().unwrap()),"range":{"start":{"line":3,"character":12},"end":{"line":3,"character":15}}}])
        );
        // Exercise the explicitly incomplete raw-background candidate path.
        ws.background.raw_model = true;
        for (version, padding, line) in [(1, "", 3), (2, "// shifted\n", 4)] {
            let text = format!("{padding}{source}");
            ws.dispatch(
                "document/open",
                &json!({"path":main,"text":text,"version":version}),
            )
            .unwrap();
            let query = json!({"path":main,"line":line,"character":19,"documentVersion":version});
            let answer = ws.dispatch("navigation/definition", &query).unwrap();
            assert_ne!(answer["completeness"], "exact", "{answer}");
            assert_eq!(answer["locations"], json!([]), "{answer}");
            assert_eq!(answer["candidateCompleteness"], "incomplete", "{answer}");
            let candidates = answer["candidates"].as_array().unwrap();
            assert!(candidates.iter().any(|c| c["uri"] == uri(main.to_str().unwrap()) && c["range"] == json!({"start":{"line":line,"character":12},"end":{"line":line,"character":15}})), "{answer}");
            if version == 2 {
                assert!(!candidates
                    .iter()
                    .any(|c| c["uri"] == uri(main.to_str().unwrap())
                        && c["range"]["start"]["line"] == 3));
                let stale = json!({"path":main,"line":line,"character":19,"documentVersion":1});
                assert!(ws.dispatch("navigation/definition", &stale).is_err());
            }
        }
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ready_context_limit_preserves_current_source_candidates() {
        ready_context_limit_source_candidates(false);
    }
    #[test]
    fn ready_context_limit_preserves_physical_conditional_candidates() {
        ready_context_limit_source_candidates(true);
    }
    fn ready_context_limit_source_candidates(conditional: bool) {
        let (root, mut ws) = fixture();
        let main = root.join("Main.fs");
        let ordinary = "module Main\nlet f parts =\n    match parts with\n    | [| part |] when true -> part\n    | _ -> 0\nlet part = 2\n";
        // The ordinary arm is guarded to retain the candidate-only fallback.
        // The conditional source separately keeps declarations after a boundary.
        let physical = "#if UNKNOWN\n#endif\nlet f parts = match parts with\n    | [| part |] -> part\n    | _ -> 0\nlet part = 2\n";
        let source = if conditional { physical } else { ordinary };
        let use_column = if conditional { 20 } else { 30 };
        fs::write(&main, source).unwrap();
        let mut compile = String::from("<Compile Include=\"Main.fs\"/>");
        for i in 0..17 {
            let file = format!("Other{i}.fs");
            fs::write(
                root.join(&file),
                format!(
                    "module Other{i}\nlet part = {i}\n(*{}*)\n",
                    " ".repeat(256 * 1024)
                ),
            )
            .unwrap();
            compile.push_str(&format!("<Compile Include=\"{file}\"/>"));
        }
        fs::write(
            root.join("Test.fsproj"),
            format!("<Project><ItemGroup>{compile}</ItemGroup></Project>"),
        )
        .unwrap();
        for i in 0..520 {
            fs::write(root.join(format!("padding{i}.txt")), "").unwrap();
        }
        ws.dispatch(
            "document/open",
            &json!({"path":main,"version":1,"text":source}),
        )
        .unwrap();
        let query = json!({"path":main,"line":3,"character":use_column,"documentVersion":1});
        let _first = ws.dispatch("navigation/definition", &query).unwrap();
        let pending = ws.dispatch("navigation/definition", &query).unwrap();
        let expected = json!({"uri":uri(main.to_str().unwrap()),"range":{"start":{"line":3,"character":9},"end":{"line":3,"character":13}}});
        assert!(
            pending["candidates"]
                .as_array()
                .is_some_and(|xs| xs.contains(&expected)),
            "{pending}"
        );
        for _ in 0..1000 {
            if !ws.background_pending() {
                break;
            }
            ws.background_step(cancellation::CancellationToken::new())
                .unwrap();
        }
        assert_eq!(ws.background.phase, "ready");
        let ready = ws.dispatch("navigation/definition", &query).unwrap();
        assert_eq!(ready["locations"], json!([]));
        assert_eq!(ready["completeness"], "unavailable");
        assert!(
            ready["reason"]
                .as_str()
                .unwrap()
                .contains("foreground work limit"),
            "{ready}"
        );
        assert!(
            ready["candidates"]
                .as_array()
                .is_some_and(|xs| xs.contains(&expected)),
            "{ready}"
        );
        assert_eq!(ready["candidateCompleteness"], "incomplete");
        assert_eq!(ready["version"], 1);
        assert_eq!(ready["candidates"], pending["candidates"]);
        assert_eq!(ready["candidates"].as_array().unwrap().len(), 2);
        let moved = format!("// current overlay\n{source}");
        ws.dispatch(
            "document/change",
            &json!({"path":main,"version":2,"text":moved}),
        )
        .unwrap();
        let moved_query = json!({"path":main,"line":4,"character":use_column,"documentVersion":2});
        let shifted = ws.dispatch("navigation/definition", &moved_query).unwrap();
        assert_eq!(shifted["candidateCompleteness"], "incomplete");
        assert_eq!(shifted["locations"], json!([]));
        assert!(!shifted["candidates"]
            .as_array()
            .unwrap()
            .contains(&expected));
        let shifted_expected = json!({"uri":uri(main.to_str().unwrap()),"range":{"start":{"line":4,"character":9},"end":{"line":4,"character":13}}});
        assert!(
            shifted["candidates"]
                .as_array()
                .unwrap()
                .contains(&shifted_expected),
            "{shifted}"
        );
        assert!(ws.dispatch("navigation/definition", &query).is_err());
        let renamed = source.replace("part", "piece");
        ws.dispatch(
            "document/change",
            &json!({"path":main,"version":3,"text":renamed}),
        )
        .unwrap();
        let renamed_query =
            json!({"path":main,"line":3,"character":use_column+1,"documentVersion":3});
        let renamed_answer = ws
            .dispatch("navigation/definition", &renamed_query)
            .unwrap();
        assert_eq!(renamed_answer["candidates"].as_array().unwrap().len(), 2);
        assert_eq!(
            renamed_answer["candidates"][0]["range"]["end"]["character"],
            14
        );
        let cancel = cancellation::CancellationToken::new();
        cancel.cancel();
        ws.set_cancellation(cancel);
        assert!(ws
            .dispatch("navigation/definition", &renamed_query)
            .is_err());
        ws.set_cancellation(cancellation::CancellationToken::new());
        for method in ["navigation/references", "navigation/hover"] {
            let other = ws.dispatch(method, &renamed_query).unwrap();
            assert_ne!(
                other["candidateKind"], "same-source-declarations",
                "{other}"
            );
        }
        ws.dispatch("document/close", &json!({"path":main}))
            .unwrap();
        let closed = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":3,"character":use_column}),
            )
            .unwrap();
        let full = closed["candidates"].as_array().unwrap();
        for candidate in pending["candidates"].as_array().unwrap() {
            assert!(full.contains(candidate), "{closed}");
        }
        // Once contextual summaries are warm, the normal fallback retains all
        // other files too; the saved origin subset must not replace this union.
        assert_eq!(full.len(), 19);
        for i in 0..17 {
            let expected = json!({"uri":uri(root.join(format!("Other{i}.fs")).to_str().unwrap()),"range":{"start":{"line":1,"character":4},"end":{"line":1,"character":8}}});
            assert!(full.contains(&expected), "{closed}");
        }
        assert_eq!(closed["candidateCompleteness"], "incomplete");
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ordinary_local_let_document_references_track_overlay_lifecycle() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let source = "module Cases\nlet describe value =\n    let piece = value\n    piece\n";
        fs::write(&path, source).unwrap();
        let target = |line, col, len| json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":line,"character":col},"end":{"line":line,"character":col+len}}});
        for (version, text, decl_line, use_line, use_col, len) in [
            (1, source.to_string(), 2, 3, 4, 5),
            (2, format!("// shifted\n{source}"), 3, 4, 4, 5),
            (
                3,
                source.replace("    piece\n", "    (let piece = piece in piece)\n"),
                2,
                3,
                17,
                5,
            ),
            (4, source.replace("piece", "component"), 2, 3, 4, 9),
        ] {
            ws.dispatch(
                if version == 1 {
                    "document/open"
                } else {
                    "document/change"
                },
                &json!({"path":path,"text":text,"version":version}),
            )
            .unwrap();
            for include in [true, false] {
                let answer=ws.dispatch("navigation/references",&json!({"path":path,"line":use_line,"character":use_col,"version":version,"includeDeclaration":include})).unwrap();
                assert_eq!(answer["completeness"], "exact", "{answer}");
                assert_eq!(answer["proofScope"], "document");
                assert_eq!(answer["documentVersion"], version);
                assert_eq!(
                    answer["sourceHash"],
                    blake3::hash(text.as_bytes()).to_hex().to_string()
                );
                assert_eq!(
                    answer["locations"],
                    if include {
                        json!([target(decl_line, 8, len), target(use_line, use_col, len)])
                    } else {
                        json!([target(use_line, use_col, len)])
                    }
                );
            }
            if version > 1 {
                assert!(ws.dispatch("navigation/references",&json!({"path":path,"line":use_line,"character":use_col,"version":version-1})).is_err());
            }
            let definition = ws
                .dispatch(
                    "navigation/documentDefinition",
                    &json!({"path":path,"line":use_line,"character":use_col,"version":version}),
                )
                .unwrap();
            assert_eq!(
                definition["documentProofMiss"], true,
                "References must not seed Definition"
            );
        }
        let cancelled = cancellation::CancellationToken::new();
        cancelled.cancel();
        ws.set_cancellation(cancelled);
        assert!(ws
            .dispatch(
                "navigation/references",
                &json!({"path":path,"line":3,"character":4,"version":4})
            )
            .is_err());
        ws.set_cancellation(cancellation::CancellationToken::new());
        ws.dispatch("document/close", &json!({"path":path}))
            .unwrap();
        assert!(ws
            .dispatch(
                "navigation/references",
                &json!({"path":path,"line":3,"character":4,"version":4})
            )
            .is_err());
        ws.dispatch(
            "document/open",
            &json!({"path":path,"text":source,"version":5}),
        )
        .unwrap();
        let reopened = ws
            .dispatch(
                "navigation/references",
                &json!({"path":path,"line":3,"character":4,"version":5,"includeDeclaration":true}),
            )
            .unwrap();
        assert_eq!(
            reopened["locations"],
            json!([target(2, 8, 5), target(3, 4, 5)])
        );
        assert_eq!(
            reopened["sourceHash"],
            blake3::hash(source.as_bytes()).to_hex().to_string()
        );
        assert!(!ws.model_loaded);
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        let mut restarted = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        restarted
            .dispatch(
                "document/open",
                &json!({"path":path,"text":source,"version":1}),
            )
            .unwrap();
        let answer = restarted
            .dispatch(
                "navigation/references",
                &json!({"path":path,"line":3,"character":4,"version":1,"includeDeclaration":true}),
            )
            .unwrap();
        assert_eq!(answer["completeness"], "exact");
        assert_eq!(
            answer["locations"],
            json!([target(2, 8, 5), target(3, 4, 5)])
        );
        drop(restarted);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn singleton_array_document_definition_tracks_current_overlay() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let source = "module Cases\ntype RichText(parts: string[]) =\n    member _.Text =\n        match parts with\n        | [| part |] -> part.Length\n        | _ -> 0\n";
        fs::write(&path, source).unwrap();
        let at =
            |line, col, version| json!({"path":path,"line":line,"character":col,"version":version});
        let target = |line, col, len| json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":line,"character":col},"end":{"line":line,"character":col+len}}});
        for (version, text, line, col, len) in [
            (1, source.to_string(), 4, 24, 4),
            (2, format!("// shift\n{source}"), 5, 24, 4),
            (
                3,
                source.replace("part.Length", "(let part = part in part.Length)"),
                4,
                36,
                4,
            ),
            (4, source.replace("part", "entry"), 4, 25, 5),
        ] {
            ws.dispatch(
                if version == 1 {
                    "document/open"
                } else {
                    "document/change"
                },
                &json!({"path":path,"text":text,"version":version}),
            )
            .unwrap();
            let request = at(line, col, version);
            let result = ws
                .dispatch("navigation/documentDefinition", &request)
                .unwrap();
            assert_eq!(result["completeness"], "exact", "{result}");
            assert_eq!(result["locations"], json!([target(line, 13, len)]));
            assert_eq!(result["proofScope"], "document");
            assert_eq!(
                result["sourceHash"],
                blake3::hash(text.as_bytes()).to_hex().to_string()
            );
            assert_eq!(result["documentVersion"], version);
            if version > 1 {
                assert!(ws
                    .dispatch("navigation/documentDefinition", &at(line, col, version - 1))
                    .is_err());
            }
            for include in [true, false] {
                let mut references = request.clone();
                references["includeDeclaration"] = json!(include);
                let refs = ws.dispatch("navigation/references", &references).unwrap();
                assert_eq!(
                    refs["locations"],
                    if include {
                        json!([target(line, 13, len), target(line, col, len)])
                    } else {
                        json!([target(line, col, len)])
                    }
                );
                assert_eq!(refs["completeness"], "exact");
            }
        }
        let cancelled = cancellation::CancellationToken::new();
        cancelled.cancel();
        ws.set_cancellation(cancelled);
        assert!(ws
            .dispatch("navigation/documentDefinition", &at(4, 25, 4))
            .is_err());
        ws.set_cancellation(cancellation::CancellationToken::new());
        ws.dispatch("document/close", &json!({"path":path}))
            .unwrap();
        assert!(ws
            .dispatch("navigation/documentDefinition", &at(4, 25, 4))
            .is_err());
        ws.dispatch(
            "document/open",
            &json!({"path":path,"text":source,"version":5}),
        )
        .unwrap();
        let reopened = ws
            .dispatch("navigation/documentDefinition", &at(4, 24, 5))
            .unwrap();
        assert_eq!(reopened["locations"], json!([target(4, 13, 4)]));
        assert_eq!(
            reopened["sourceHash"],
            blake3::hash(source.as_bytes()).to_hex().to_string()
        );
        assert!(!ws.model_loaded);
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        let mut restarted = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        restarted
            .dispatch(
                "document/open",
                &json!({"path":path,"text":source,"version":1}),
            )
            .unwrap();
        let result = restarted
            .dispatch("navigation/documentDefinition", &at(4, 24, 1))
            .unwrap();
        assert_eq!(result["locations"], json!([target(4, 13, 4)]));
        assert_eq!(result["completeness"], "exact");
        assert_eq!(result["proofScope"], "document");
        drop(restarted);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cold_singleton_array_match_references_are_document_closed() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let source = "module Cases\nlet read (items:int[]) =\n    match items with\n    | [| part |] -> part + 1\n    | _ -> let part = 9 in part\n";
        fs::write(&path, source).unwrap();
        for i in 0..520 {
            fs::write(root.join(format!("padding{i}.txt")), "").unwrap();
        }
        ws.dispatch(
            "document/open",
            &json!({"path":path,"text":source,"version":1}),
        )
        .unwrap();
        let declaration = json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":3,"character":9},"end":{"line":3,"character":13}}});
        let usage = json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":3,"character":20},"end":{"line":3,"character":24}}});
        for include in [true, false] {
            let result = ws.dispatch("navigation/references", &json!({"path":path,"line":3,"character":20,"version":1,"includeDeclaration":include})).unwrap();
            assert_eq!(result["completeness"], "exact", "{result}");
            let expected = if include {
                json!([declaration, usage])
            } else {
                json!([usage])
            };
            assert_eq!(result["locations"], expected, "{result}");
            assert_eq!(result["proofScope"], "document");
            assert_eq!(result["documentVersion"], 1);
        }
        assert!(!ws.model_loaded);
        let query = |line, version| json!({"path":path,"line":line,"character":9,"version":version,"includeDeclaration":true});
        let shifted = format!("// shifted\n{source}");
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":shifted,"version":2}),
        )
        .unwrap();
        let current = ws.dispatch("navigation/references", &query(4, 2)).unwrap();
        let mut shifted_declaration = declaration.clone();
        let mut shifted_usage = usage.clone();
        for location in [&mut shifted_declaration, &mut shifted_usage] {
            location["range"]["start"]["line"] = json!(4);
            location["range"]["end"]["line"] = json!(4);
        }
        assert_eq!(
            current["locations"],
            json!([shifted_declaration, shifted_usage])
        );
        assert!(ws.dispatch("navigation/references", &query(3, 1)).is_err());
        let rebound = source.replace("part + 1", "(let part = part in part)");
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":rebound,"version":3}),
        )
        .unwrap();
        let rebound_result = ws.dispatch("navigation/references", &query(3, 3)).unwrap();
        let rebound_usage = json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":3,"character":32},"end":{"line":3,"character":36}}});
        assert_eq!(
            rebound_result["locations"],
            json!([declaration, rebound_usage])
        );
        assert_eq!(rebound_result["completeness"], "exact");
        assert_ne!(rebound_result["sourceHash"], current["sourceHash"]);
        let renamed = source.replace("part", "entry");
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":renamed,"version":4}),
        )
        .unwrap();
        let renamed_result = ws.dispatch("navigation/references", &query(3, 4)).unwrap();
        let renamed_declaration = json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":3,"character":9},"end":{"line":3,"character":14}}});
        let renamed_usage = json!({"uri":uri(path.to_str().unwrap()),"range":{"start":{"line":3,"character":21},"end":{"line":3,"character":26}}});
        assert_eq!(
            renamed_result["locations"],
            json!([renamed_declaration, renamed_usage])
        );
        assert_eq!(
            renamed_result["sourceHash"],
            blake3::hash(renamed.as_bytes()).to_hex().to_string()
        );
        let cancel = cancellation::CancellationToken::new();
        cancel.cancel();
        ws.set_cancellation(cancel);
        assert!(ws.dispatch("navigation/references", &query(3, 4)).is_err());
        ws.set_cancellation(cancellation::CancellationToken::new());
        ws.dispatch("document/close", &json!({"path":path}))
            .unwrap();
        assert!(ws.dispatch("navigation/references", &query(3, 4)).is_err());
        ws.dispatch(
            "document/open",
            &json!({"path":path,"text":source,"version":5}),
        )
        .unwrap();
        assert_eq!(
            ws.dispatch("navigation/references", &query(3, 5)).unwrap()["locations"],
            json!([declaration, usage])
        );
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        let mut reopened = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        reopened
            .dispatch(
                "document/open",
                &json!({"path":path,"text":source,"version":1}),
            )
            .unwrap();
        let restarted = reopened
            .dispatch("navigation/references", &query(3, 1))
            .unwrap();
        assert_eq!(restarted["locations"], json!([declaration, usage]));
        assert_eq!(restarted["completeness"], "exact");
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_ordered_opens_and_scoped_references_without_helper() {
        let (path, mut ws) = fixture();
        let main = path.join("Main.fs");
        let def = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":3,"character":13}),
            )
            .unwrap();
        assert_eq!(def["completeness"], "exact");
        assert!(def["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/B.fs"));
        let refs = ws
            .dispatch(
                "navigation/references",
                &json!({"path":main,"line":5,"character":8}),
            )
            .unwrap();
        assert_eq!(refs["completeness"], "exact");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 1);
        assert_eq!(refs["locations"][0]["range"]["start"]["line"], 6);
        let before = ws.index.counters.files_read;
        ws.dispatch(
            "navigation/definition",
            &json!({"path":main,"line":3,"character":13}),
        )
        .unwrap();
        assert_eq!(before, ws.index.counters.files_read);
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn a_later_open_with_an_attributed_child_cannot_certify_an_older_export() {
        let (root, mut ws) = fixture();
        let values = root.join("A.fs");
        let source = "namespace Domains\nmodule Earlier =\n    let probe x = x + 1\nnamespace Domains.Later\n[<AutoOpen>]\nmodule Nested =\n    let probe x = x + 2\n";
        fs::write(&values, source).unwrap();
        let main = root.join("Main.fs");
        fs::write(
            &main,
            "module Consumer\nopen Domains.Earlier\nopen Domains.Later\nlet caller v = probe v\n",
        )
        .unwrap();
        let request = json!({"path":main,"line":3,"character":15});
        for (version, effect) in [(1, true), (2, false), (3, true)] {
            let text = if effect {
                source.to_owned()
            } else {
                source.replace("[<AutoOpen>]", "")
            };
            ws.dispatch(
                "document/change",
                &json!({"path":values,"version":version,"text":text}),
            )
            .unwrap();
            let answer = ws.dispatch("navigation/definition", &request).unwrap();
            if effect {
                assert_eq!(answer["completeness"], "unavailable", "{answer}");
            } else {
                assert_eq!(answer["completeness"], "exact", "{answer}");
                assert_eq!(answer["locations"][0]["range"]["start"]["line"], 2);
            }
        }
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn opened_module_abbreviations_retain_the_environment_at_the_open() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let source = "module Consumer\nmodule A =\n    let probe v = v + 1\nmodule B =\n    let probe v = v + 2\nmodule Chosen = A\nopen Chosen\nmodule Chosen = B\nlet caller v = probe v\nlet direct v = Chosen.probe v\nopen Chosen\nlet reopened v = probe v\n";
        fs::write(&path, source).unwrap();
        for (line, name, expected) in [(8, "probe", 2), (11, "probe", 4)] {
            let character = source.lines().nth(line).unwrap().find(name).unwrap();
            let answer = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":path,"line":line,"character":character}),
                )
                .unwrap();
            assert_eq!(answer["completeness"], "exact", "{answer}");
            assert_eq!(answer["locations"][0]["range"]["start"]["line"], expected);
        }
        // A qualified call also needs a receiver-term certificate for Chosen.
        // This XML-only fixture has no selected external declaration inventory.
        let direct = ws.dispatch("navigation/definition", &json!({"path":path,"line":9,"character":source.lines().nth(9).unwrap().find("probe").unwrap()})).unwrap();
        assert_eq!(direct["completeness"], "unavailable", "{direct}");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn parameter_reference_domains_keep_local_sets_but_refuse_possible_signature_exports() {
        let source = "module Local\nlet ordinary ordinaryArg=ordinaryArg+ordinaryArg\nlet direct=fun directArg -> directArg+directArg\nlet returned ()=fun returnedArg -> returnedArg+returnedArg\nlet nested ()=let inner nestedArg=nestedArg+nestedArg in inner\nlet mapped=List.map (fun callbackArg -> callbackArg+callbackArg) [1]\nlet computed ()=let one=1 in fun computedArg -> computedArg+one\nlet pattern=function patternArg -> patternArg+patternArg\n";
        for signature_name in [
            None,
            Some("Signature.fsi"),
            Some("Signature.FSI"),
            Some("Signature.FsI"),
        ] {
            let (path, ws) = fixture();
            drop(ws);
            let signature = signature_name.is_some();
            let signature_item = signature_name
                .map(|name| format!("<Compile Include=\"{name}\"/>"))
                .unwrap_or_default();
            fs::write(path.join("Test.fsproj"), format!("<Project Sdk=\"Microsoft.NET.Sdk\"><ItemGroup>{signature_item}<Compile Include=\"Implementation.fs\"/></ItemGroup></Project>")).unwrap();
            // Different physical basenames can still be one compiler pair.
            if let Some(name) = signature_name {
                fs::write(path.join(name), "module Local\nval ordinary:ordinaryArg:int -> int\nval direct:directArg:int -> int\nval returned:unit -> returnedArg:int -> int\nval nested:unit -> (int -> int)\nval mapped:int list\nval computed:unit -> (int -> int)\nval pattern:patternArg:int -> int\n").unwrap();
            }
            let implementation = path.join("Implementation.fs");
            fs::write(&implementation, source).unwrap();
            let mut ws = Workspace::open(&json!({"root":path,"mode":"indexed"})).unwrap();
            for (name, independent, count) in [
                ("ordinaryArg", false, 3),
                ("directArg", false, 3),
                ("returnedArg", false, 3),
                ("nestedArg", true, 3),
                ("callbackArg", true, 3),
                ("computedArg", true, 2),
                ("patternArg", true, 3),
            ] {
                let (line, text) = source
                    .lines()
                    .enumerate()
                    .find(|(_, text)| text.contains(name))
                    .unwrap();
                let character = text.find(name).unwrap();
                for include in [false, true] {
                    let answer = ws.dispatch("navigation/references", &json!({"path":implementation,"line":line,"character":character,"includeDeclaration":include,"traceReferences":true})).unwrap();
                    let exact = independent || !signature;
                    assert_eq!(
                        answer["completeness"],
                        if exact { "exact" } else { "unavailable" },
                        "{name}, signature={signature}: {answer}"
                    );
                    assert_eq!(
                        answer["locations"].as_array().unwrap().len(),
                        count - usize::from(!include),
                        "Partial locations are preserved: {answer}"
                    );
                    if !exact {
                        assert_eq!(answer["complete"], false);
                        assert_eq!(
                            answer["proofGaps"]["examples"][0]["kind"],
                            "unclosed-parameter-domain"
                        );
                    }
                }
            }
            assert_eq!(ws.index.counters.helper_starts, 0);
            drop(ws);
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn signature_absence_requires_only_recognized_implementation_inputs() {
        let (path, mut ws) = fixture();
        ws.ensure_model().unwrap();
        let main = normalize(path.join("Main.fs"));
        assert!(!ws.possible_signature_context(&main, false));
        for (name, possible) in [
            ("Another.fs", false),
            ("Another.FS", false),
            ("Another.FsX", true),
            ("Another.fsi", true),
            ("Another.FSI", true),
            ("Another.FsI", true),
            ("Another.fsscript", true),
            ("Another.ml", true),
            ("Another.fş", true),
            ("Another.未知", true),
            ("dir.fsi/Another", true),
        ] {
            ws.model.projects[0].files.push(normalize(path.join(name)));
            assert_eq!(
                ws.possible_signature_context(&main, false),
                possible,
                "{name}"
            );
            ws.model.projects[0].files.pop();
        }
        ws.model.projects[0].exact = false;
        assert!(ws.possible_signature_context(&main, false));
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn only_unshared_ordered_formals_preceding_all_signatures_have_a_local_domain() {
        let (root, mut ws) = fixture();
        ws.ensure_model().unwrap();
        let target = normalize(root.join("Main.fs"));
        let sig = normalize(root.join("Elsewhere.FsI"));
        ws.model.projects[0].files.push(sig.clone());
        // A provisional XML list does not certify compiler order.
        assert!(ws.possible_signature_context(&target, true));
        let project = &mut ws.model.projects[0];
        project.evaluation = "design-time-fsc-arguments".into();
        project.membership_reusable = Some(true);
        project.compiler_args = project.files.clone();
        assert!(!ws.possible_signature_context(&target, true));
        assert!(ws.possible_signature_context(&target, false));
        ws.model.projects[0].files.push(target.clone());
        assert!(ws.possible_signature_context(&target, true));
        ws.model.projects[0].files.pop();
        let mut second = ws.model.projects[0].clone();
        second.path = normalize(root.join("Second.fsproj"));
        ws.model.projects.push(second);
        ws.active_project = Some(ws.model.projects[0].path.clone());
        assert!(ws.possible_signature_context(&target, true));
        ws.model.projects.pop();
        ws.model.projects[0].files.insert(0, sig);
        assert!(ws.possible_signature_context(&target, true));
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn callable_parameter_body_sets_are_not_complete_named_argument_domains() {
        let (path, ws) = fixture();
        drop(ws);
        let source = "module Library\ntype Counter(seed:int)=\n    member _.Add(amount:int)=seed+amount+amount\n    static member Twice(delta:int)=delta+delta\n";
        fs::write(path.join("A.fs"), source).unwrap();
        fs::write(path.join("B.fs"), "module Caller\nopen Library\nlet value=Counter(seed=1).Add(amount=2)+Counter.Twice(delta=3)\n").unwrap();
        let mut ws = Workspace::open(&json!({"root":path,"mode":"indexed"})).unwrap();
        for (name, count) in [("seed", 2), ("amount", 3), ("delta", 3)] {
            let (line, text) = source
                .lines()
                .enumerate()
                .find(|(_, text)| text.contains(name))
                .unwrap();
            for include in [false, true] {
                let answer = ws.dispatch("navigation/references", &json!({"path":path.join("A.fs"),"line":line,"character":text.find(name).unwrap(),"includeDeclaration":include,"traceReferences":true})).unwrap();
                assert_eq!(answer["completeness"], "unavailable", "{name}: {answer}");
                assert_eq!(
                    answer["locations"].as_array().unwrap().len(),
                    count - usize::from(!include),
                    "{name}: {answer}"
                );
                assert_eq!(
                    answer["proofGaps"]["examples"][0]["kind"],
                    "unclosed-parameter-domain"
                );
            }
        }
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn earlier_open_value_can_win_over_a_later_module_in_a_qualified_call() {
        let (root, mut ws) = fixture();
        fs::write(
            root.join("A.fs"),
            "module Imported\ntype Proxy() = member _.g(x:int)=x\nlet A=Proxy()\n",
        )
        .unwrap();
        let path = root.join("Main.fs");
        fs::write(
            &path,
            "module Consumer\nopen Imported\nmodule A =\n    let g a=a\nlet caller x = A.g(x=x)\n",
        )
        .unwrap();
        for column in [17, 19] {
            let def = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":path,"line":4,"character":column}),
                )
                .unwrap();
            assert_eq!(def["completeness"], "unavailable", "{def}");
        }
        let refs = ws
            .dispatch(
                "navigation/references",
                &json!({"path":path,"line":4,"character":21,"includeDeclaration":true}),
            )
            .unwrap();
        assert_eq!(refs["completeness"], "unavailable", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 2, "{refs}");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_opened_lowercase_constructor_competes_with_an_earlier_function() {
        let (root, mut ws) = fixture();
        let imported = root.join("A.fs");
        let main = root.join("Main.fs");
        fs::write(&imported, "module Imported\ntype g(x:int)=member _.P=x\n").unwrap();
        fs::write(
            &main,
            "module Consumer\nlet g a=a\nopen Imported\nlet caller x = g(x=x)\nlet other = g(1)\n",
        )
        .unwrap();
        let def = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":3,"character":15}),
            )
            .unwrap();
        assert_eq!(def["completeness"], "unavailable", "{def}");
        let ordinary = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":4,"character":12}),
            )
            .unwrap();
        assert_eq!(ordinary["completeness"], "unavailable", "{ordinary}");
        for (path, line, column) in [(&imported, 1, 5), (&main, 1, 4), (&main, 3, 15)] {
            for include in [true, false] {
                let refs = ws.dispatch("navigation/references", &json!({"path":path,"line":line,"character":column,"includeDeclaration":include})).unwrap();
                assert_eq!(refs["completeness"], "unavailable", "{refs}");
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn qualified_equality_operand_uses_its_full_module_value_range() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        fs::write(&path,"module Consumer\ntype C = static member M(x:bool)=x\nmodule A =\n    let x=0\nmodule Alias=A\nlet caller x = C.M(Alias.x=x)\n").unwrap();
        let reply = ws
            .dispatch(
                "navigation/references",
                &json!({"path":path,"line":3,"character":8}),
            )
            .unwrap();
        assert_eq!(reply["completeness"], "exact", "{reply}");
        assert_eq!(
            reply["locations"][0]["range"],
            json!({"start":{"line":5,"character":19},"end":{"line":5,"character":26}})
        );
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn named_argument_labels_keep_partial_caller_references_and_revalidate_edits() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let source = |call: &str| {
            format!("module Consumer\ntype C = static member M(x:bool)=x\nlet g b=b\nlet caller x = {call}\n")
        };
        let ordinary = source("g(x=x)");
        let method = source("C.M(x=x)");
        fs::write(&path, &ordinary).unwrap();
        let query = |ws: &mut Workspace, text: &str, equality: bool, version: Option<i32>| {
            let line = text.lines().last().unwrap();
            let declaration = line.find(" x ").unwrap() + 1;
            let left = line.rfind("(x=").unwrap() + 1;
            let right = left + 2;
            for column in [left, right] {
                let answer = ws
                    .dispatch(
                        "navigation/definition",
                        &json!({"path":path,"line":3,"character":column,"version":version}),
                    )
                    .unwrap();
                if column == left && !equality {
                    assert_eq!(answer["completeness"], "unavailable", "{answer}");
                } else {
                    assert_eq!(answer["completeness"], "exact", "{answer}");
                    assert_eq!(
                        answer["locations"][0]["range"]["start"],
                        json!({"line":3,"character":declaration})
                    );
                }
            }
            for include in [false, true] {
                let answer = ws.dispatch("navigation/references", &json!({"path":path,"line":3,"character":right,"includeDeclaration":include,"traceReferences":true,"version":version})).unwrap();
                assert_eq!(
                    answer["completeness"],
                    if equality { "exact" } else { "unavailable" },
                    "{answer}"
                );
                let mut actual: Vec<_> = answer["locations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|loc| loc["range"]["start"]["character"].as_u64().unwrap() as usize)
                    .collect();
                actual.sort();
                let mut expected = vec![right];
                if equality {
                    expected.push(left);
                }
                if include {
                    expected.push(declaration);
                }
                expected.sort();
                assert_eq!(actual, expected, "{answer}");
                if !equality {
                    assert_eq!(
                        answer["proofGaps"]["examples"][0]["kind"],
                        "unproven-call-argument-name"
                    );
                }
            }
        };
        query(&mut ws, &ordinary, true, None);
        ws.dispatch(
            "document/open",
            &json!({"path":path,"text":method,"version":1}),
        )
        .unwrap();
        query(&mut ws, &method, false, Some(1));
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":ordinary,"version":2}),
        )
        .unwrap();
        query(&mut ws, &ordinary, true, Some(2));
        assert!(ws
            .dispatch(
                "document/change",
                &json!({"path":path,"text":method,"version":1})
            )
            .is_err());
        query(&mut ws, &ordinary, true, Some(2));
        ws.dispatch("document/close", &json!({"path":path}))
            .unwrap();
        drop(ws);
        fs::write(&path, &method).unwrap();
        let mut ws = Workspace::open(&json!({"root":root,"mode":"indexed"})).unwrap();
        query(&mut ws, &method, false, None);
        assert_eq!(ws.index.counters.helper_starts, 0);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn activation_and_open_documents_do_not_read_or_parse_sources() {
        let (path, mut ws) = fixture();
        assert_eq!(ws.index.counters.files_read, 0);
        assert_eq!(ws.index.counters.files_parsed, 0);
        assert!(!ws.model_loaded);
        let main = path.join("Main.fs");
        let text = fs::read_to_string(&main).unwrap();
        ws.dispatch(
            "document/open",
            &json!({"path":main,"text":text,"version":1}),
        )
        .unwrap();
        assert_eq!(ws.index.counters.files_read, 0);
        assert_eq!(ws.index.counters.files_parsed, 0);
        let answer = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":6,"character":6,"version":1}),
            )
            .unwrap();
        assert_eq!(answer["completeness"], "exact");
        assert_eq!(ws.index.counters.files_parsed, 1);
        assert_eq!(ws.index.counters.files_read, 0);
        assert!(!ws.catalog_loaded);
        assert!(!ws.semantic_loaded);
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn overlays_rebind_and_stale_cursor_rejected() {
        let (path, mut ws) = fixture();
        let main = path.join("Main.fs");
        let b = path.join("B.fs");
        let before = ws.index.counters.files_parsed;
        ws.dispatch(
            "document/change",
            &json!({"path":b,"text":"module LibraryB\nlet changed = 2\n","version":1}),
        )
        .unwrap();
        ws.dispatch("document/change",&json!({"path":main,"text":"module Consumer\nopen LibraryB\nlet result = changed\n","version":1})).unwrap();
        let def = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":2,"character":14,"documentVersion":1}),
            )
            .unwrap();
        assert_eq!(def["completeness"], "exact");
        assert!(def["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/B.fs"));
        assert_eq!(ws.index.counters.files_parsed - before, 2);
        assert!(ws
            .dispatch(
                "navigation/definition",
                &json!({"path":main,"line":2,"character":14,"version":0})
            )
            .is_err());
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn private_open_does_not_shadow_an_accessible_binding() {
        let (path, mut ws) = fixture();
        fs::write(
            path.join("B.fs"),
            "module LibraryB\nlet private value = 2\n",
        )
        .unwrap();
        let def = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":path.join("Main.fs"),"line":3,"character":13}),
            )
            .unwrap();
        assert_eq!(def["completeness"], "exact");
        assert!(def["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/A.fs"));
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn compiled_signature_masks_implementation_only_exports() {
        let (path, mut ws) = fixture();
        fs::write(path.join("Test.fsproj"),r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><Compile Include="A.fs"/><Compile Include="B.fsi"/><Compile Include="B.fs"/><Compile Include="Main.fs"/></ItemGroup></Project>"#).unwrap();
        fs::write(path.join("B.fsi"), "module LibraryB\nval exported : int\n").unwrap();
        fs::write(
            path.join("B.fs"),
            "module LibraryB\nlet value = 2\nlet exported = 3\n",
        )
        .unwrap();
        let def = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":path.join("Main.fs"),"line":3,"character":13}),
            )
            .unwrap();
        assert_eq!(def["completeness"], "exact");
        assert!(def["locations"][0]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/A.fs"));
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn saved_mixed_case_sources_refresh_complete_reference_sets() {
        let (root, original) = fixture();
        drop(original);
        let declaration = root.join("Defs.fs");
        let caller = root.join("Caller.FS");
        fs::write(&declaration, "module Defs\nlet selected=2\n").unwrap();
        let source = "module Caller\nopen Defs\nlet first=selected\n";
        fs::write(&caller, source).unwrap();
        fs::write(root.join("Test.fsproj"), "<Project><ItemGroup><Compile Include=\"Defs.fs\"/><Compile Include=\"Caller.FS\"/></ItemGroup></Project>").unwrap();
        let mut ws = Workspace::open(&json!({"root":root})).unwrap();
        let refs = |ws: &mut Workspace| {
            let result = ws
                .dispatch(
                    "navigation/references",
                    &json!({"path":declaration,"line":1,"character":5,"includeDeclaration":true}),
                )
                .unwrap();
            assert_eq!(result["completeness"], "exact", "{result}");
            result["locations"].as_array().unwrap().len()
        };
        assert_eq!(refs(&mut ws), 2);
        // Inject the delivered OS event deterministically, without depending on
        // event-loop timing or coalescing on the CI filesystem.
        let (tx, rx) = mpsc::channel();
        ws.events = rx;
        fs::write(&caller, format!("{source}let second=selected\n")).unwrap();
        tx.send(Ok(notify::Event::new(notify::EventKind::Modify(
            notify::event::ModifyKind::Any,
        ))
        .add_path(caller.clone())))
            .unwrap();
        assert_eq!(refs(&mut ws), 3);
        fs::write(&caller, source).unwrap();
        ws.dispatch("workspace/filesChanged", &json!({"paths":[caller]}))
            .unwrap();
        assert_eq!(refs(&mut ws), 2);
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mixed_case_signature_target_uses_actual_ordered_paths() {
        let (root, original) = fixture();
        drop(original);
        let sig = root.join("Contracts.FsI");
        let imp = root.join("Contracts.fS");
        let main = root.join("Main.fs");
        fs::write(&sig, "module Contracts\nval answer:int\n").unwrap();
        fs::write(&imp, "module Contracts\nlet answer=1\n").unwrap();
        fs::write(&main, "module Main\nlet result=Contracts.answer\n").unwrap();
        fs::write(root.join("Test.fsproj"), "<Project><ItemGroup><Compile Include=\"Contracts.FsI\"/><Compile Include=\"Contracts.fS\"/><Compile Include=\"Main.fs\"/></ItemGroup></Project>").unwrap();
        let mut ws = Workspace::open(&json!({"root":root})).unwrap();
        for (from, col, to) in [(&sig, 5, &imp), (&imp, 5, &sig), (&main, 22, &sig)] {
            let def = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":from,"line":1,"character":col}),
                )
                .unwrap();
            assert_eq!(def["completeness"], "exact", "{def}");
            assert_eq!(def["locations"][0]["uri"], uri(&normalize(to)));
        }
        let refs = ws
            .dispatch(
                "navigation/references",
                &json!({"path":sig,"line":1,"character":5,"includeDeclaration":true}),
            )
            .unwrap();
        assert_eq!(refs["completeness"], "exact", "{refs}");
        assert_eq!(refs["locations"].as_array().unwrap().len(), 3, "{refs}");
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cached_external_hover_obeys_catalog_freshness() {
        use sha2::{Digest, Sha256};
        let (path, mut ws) = fixture();
        let assembly = path.with_extension("dll");
        fs::write(&assembly, b"certified fixture reference").unwrap();
        let source = "module Consumer\nlet result = ExternalLibrary.answer\n";
        fs::write(path.join("Main.fs"), source).unwrap();
        let catalog = path.join("libraries.json");
        let mut value = certified_metadata_fixture(
            json!({"schemaVersion":1,"factsVersion":2,"inputHashes":[{"path":assembly,"sha256":format!("{:x}",Sha256::digest(fs::read(&assembly).unwrap()))}],"completeness":"exact","symbols":[{"id":"library-answer","fullName":"ExternalLibrary.answer","name":"answer","signature":"val answer : int","documentation":"Old documentation","isExternal":true,"assemblyPath":assembly}]}),
        );
        fs::write(&catalog, value.to_string()).unwrap();
        ws.catalog_path = Some(catalog.to_string_lossy().into_owned());
        ws.ensure_model().unwrap();
        ws.model.projects[0].evaluation = "design-time-fsc-arguments".into();
        ws.model.projects[0].reference_bindings =
            vec![json!({"path":assembly,"identity":"Reference","isWorkspace":false})];
        let environment = path.join("environment.json");
        fs::write(&environment,crate::certified_metadata_fixture(json!({"schemaVersion":1,"factsVersion":2,"kind":"builtin-environment","completeness":"exact","typeNameInventory":"all-external-type-declarations","scopeNameInventory":"all-external-namespace-and-module-declarations","termNameInventory":"all-external-expression-bindings","scopeNames":{},"termNames":{},"assemblyAutoOpens":[],"certificateScope":"selected-reference-declaration-inventory","inputHashes":[{"path":assembly,"sha256":format!("{:x}",Sha256::digest(fs::read(&assembly).unwrap()))}],"referenceBindings":[{"path":assembly,"identity":"Reference"}],"workspaceAssemblies":[],"types":{},"patterns":{},"typeNames":{"ExternalLibrary":[{"identity":{"assemblyPath":assembly,"assemblyName":"Reference","fullName":"ExternalLibrary"},"sourceFullName":"ExternalLibrary","genericArity":0,"isAbbreviation":false,"isNominal":true}]}})).to_string()).unwrap();
        ws.builtin_path = Some(environment.to_string_lossy().into_owned());
        let col = source.lines().nth(1).unwrap().find("answer").unwrap();
        let params = json!({"path":path.join("Main.fs"),"line":1,"character":col});
        let first = ws.dispatch("navigation/hover", &params).unwrap();
        assert_eq!(first["completeness"], "exact");
        assert_eq!(first["ownership"], "external");
        value["symbols"][0]["documentation"] =
            json!("Changed documentation with a different length");
        fs::write(&catalog, value.to_string()).unwrap();
        let cached = ws.dispatch("navigation/hover", &params).unwrap();
        assert_eq!(cached["completeness"], "unavailable");
        assert!(cached["hover"].is_null());
        let fresh = ws
            .dispatch(
                "navigation/hover",
                &json!({"path":path.join("Main.fs"),"line":1,"character":col+1}),
            )
            .unwrap();
        assert_eq!(fresh["completeness"], "unavailable");
        assert!(fresh["hover"].is_null());
        drop(ws);
        // A second selected assembly contributes the same full nominal name.
        // The subset catalog still has one row, but cannot resolve the owner.
        let competitor = path.with_extension("competitor.dll");
        fs::write(&competitor, b"other certified reference").unwrap();
        let mut certificate: Value =
            serde_json::from_slice(&fs::read(&environment).unwrap()).unwrap();
        let mut entry = certificate["typeNames"]["ExternalLibrary"][0].clone();
        entry["identity"]["assemblyPath"] = json!(competitor);
        entry["identity"]["assemblyName"] = json!("Competitor");
        certificate["typeNames"]["ExternalLibrary"]
            .as_array_mut()
            .unwrap()
            .push(entry);
        certificate["referenceBindings"]
            .as_array_mut()
            .unwrap()
            .push(json!({"path":competitor,"identity":"Competitor"}));
        certificate["inputHashes"].as_array_mut().unwrap().push(json!({"path":competitor,"sha256":format!("{:x}",Sha256::digest(fs::read(&competitor).unwrap()))}));
        fs::write(&environment, certificate.to_string()).unwrap();
        let mut fresh = Workspace::open(
            &json!({"root":path,"libraryCatalogPath":catalog,"builtinEnvironmentPath":environment}),
        )
        .unwrap();
        fresh.ensure_model().unwrap();
        fresh.model.projects[0].evaluation = "design-time-fsc-arguments".into();
        fresh.model.projects[0].reference_bindings =
            certificate["referenceBindings"].as_array().unwrap().clone();
        let result = fresh.dispatch("navigation/hover", &params).unwrap();
        assert_eq!(
            result["completeness"], "unavailable",
            "A subset catalog cannot select a nominal owner: {result}"
        );
        assert!(
            !fresh.catalog_loaded,
            "Unproven owner should not load unrelated rendered metadata"
        );
        drop(fresh);
        fs::remove_file(competitor).unwrap();
        fs::remove_file(assembly).unwrap();
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn missing_input_directories_watch_existing_parent_and_invalidate_on_creation() {
        use sha2::{Digest, Sha256};
        let (path, old) = fixture();
        drop(old);
        let project = path.join("Test.fsproj");
        let missing = path.join("future/deep/Custom.props");
        let manifest = path.join("model.json");
        fs::write(&manifest,json!({"completeness":"exact","projects":[{"path":project,"name":"Test","files":[path.join("A.fs"),path.join("B.fs"),path.join("Main.fs")],"membershipReusable":true,"environmentCertificate":project::environment::empty_certificate()}],"inputs":[project,missing],"inputHashes":[{"path":project,"sha256":format!("{:x}",Sha256::digest(fs::read(&project).unwrap()))},{"path":missing,"sha256":"missing"}]}).to_string()).unwrap();
        let mut ws = Workspace::open(&json!({"root":path,"evaluatedModelPath":manifest})).unwrap();
        let tree = ws.dispatch("workspace/tree", &json!({})).unwrap();
        assert_eq!(
            tree["completeness"], "exact",
            "Missing input parents should not make watch setup fail: {tree}"
        );
        assert_eq!(ws.index.counters.files_parsed, 0);
        let (tx, rx) = std::sync::mpsc::channel();
        ws.events = rx;
        tx.send(Ok(notify::Event::new(notify::EventKind::Create(
            notify::event::CreateKind::Folder,
        ))
        .add_path(path.join("future"))))
            .unwrap();
        ws.drain_events().unwrap();
        assert!(
            !ws.model.exact,
            "An ancestor creation changes a negative input proof before deeper files appear"
        );
        drop(ws);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn imported_project_model_rejects_changed_inputs_on_lazy_load() {
        use sha2::{Digest, Sha256};
        let (path, ws) = fixture();
        drop(ws);
        let project = path.join("Test.fsproj");
        let missing = path.join("not-built-yet.dll");
        let manifest = path.join("projects.json");
        let value = json!({"completeness":"exact","projects":[{"membershipReusable":true,"environmentCertificate":project::environment::empty_certificate(),
            "path":project,"name":"Test","files":[path.join("A.fs"),path.join("B.fs"),path.join("Main.fs")]
        }],"inputs":[project,missing],"inputHashes":[
            {"path":project,"sha256":format!("{:x}",Sha256::digest(fs::read(&project).unwrap()))},
            {"path":missing,"sha256":"missing"}
        ]});
        fs::write(&manifest, value.to_string()).unwrap();
        let open = json!({"root":path,"mode":"indexed","evaluatedModelPath":manifest});
        let mut current = Workspace::open(&open).unwrap();
        assert!(!current.model_loaded);
        assert_eq!(
            current.dispatch("workspace/tree", &json!({})).unwrap()["completeness"],
            "exact"
        );
        drop(current);
        fs::write(&missing, b"now built").unwrap();
        let mut changed = Workspace::open(&open).unwrap();
        assert!(!changed.model_loaded);
        assert!(changed
            .dispatch("workspace/tree", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("stale"));
        drop(changed);
        fs::remove_file(&missing).unwrap();
        fs::write(&project, "<Project />").unwrap();
        let mut stale = Workspace::open(&open).unwrap();
        assert_eq!(stale.index.counters.files_read, 0);
        assert!(stale
            .dispatch("workspace/tree", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("stale"));
        drop(stale);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn custom_operator_definition_is_scoped_current_and_definition_only() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        fs::write(
            root.join("Test.fsproj"),
            "<Project><ItemGroup><Compile Include=\"Main.fs\"/></ItemGroup></Project>",
        )
        .unwrap();
        let source="module SymbolicScope\nmodule Other =\n    let (+++) left right = left + right\nmodule Selected =\n    let (+++) left right = left - right\n    let answer = (* 😀 *) 7 +++ 2\n";
        fs::write(&path, source).unwrap();
        let query = |text: &str| {
            let (line, text) = text
                .lines()
                .enumerate()
                .find(|(_, text)| text.contains("let answer"))
                .unwrap();
            let at = text.rfind("+++").unwrap();
            (line, text[..at].encode_utf16().count() + 1)
        };
        let (line, col) = query(source);
        let params = json!({"path":path,"line":line,"character":col});
        let answer = ws.dispatch("navigation/definition", &params).unwrap();
        assert_eq!(answer["completeness"], "exact", "{answer}");
        assert_eq!(
            answer["locations"][0]["range"],
            json!({"start":{"line":4,"character":9},"end":{"line":4,"character":12}})
        );
        assert_ne!(
            ws.dispatch("navigation/references", &params).unwrap()["completeness"],
            "exact"
        );
        for (version, text) in [
            source.replace(
                "    let answer",
                "    let ``+++`` a b = a*b\n    let answer",
            ),
            source.replace(
                "    let answer = (* 😀 *) 7 +++ 2",
                "    let answer ``+++`` = 7 +++ 2",
            ),
            source.to_owned() + "let op_Addition a b = a+b\n",
            source.to_owned() + "let ``op_Addition`` a b = a+b\n",
            source.replace(
                "let (+++) left right = left - right",
                "let (***) left right = left - right",
            ),
            source.replace("    let answer", "    open Other\n    let answer"),
            source.replace(
                "let (+++) left right = left - right",
                "let inline (+++) left right = left - right",
            ),
            source.replace(
                "    let answer = (* 😀 *) 7 +++ 2",
                "    let answer = let (+++) a b = a * b in 7 +++ 2",
            ),
            source.replace(
                "    let (+++) left right = left - right",
                "    [<CompiledName(\"Changed\")>]\n    let (+++) left right = left - right",
            ),
            source.replace("    let answer", "#if ACTIVE\n    let answer") + "#endif\n",
        ]
        .into_iter()
        .enumerate()
        {
            let version = version as i64 + 1;
            ws.dispatch(
                "document/change",
                &json!({"path":path,"text":text,"version":version}),
            )
            .unwrap();
            let (line, col) = query(&text);
            let answer = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":path,"line":line,"character":col,"version":version}),
                )
                .unwrap();
            assert_ne!(
                answer["completeness"], "exact",
                "version{version}: {answer}"
            );
        }
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":source,"version":20}),
        )
        .unwrap();
        let mut stale = params.clone();
        stale["version"] = json!(1);
        assert!(ws.dispatch("navigation/definition", &stale).is_err());
        let mut current = params.clone();
        current["version"] = json!(20);
        assert_eq!(
            ws.dispatch("navigation/definition", &current).unwrap()["locations"],
            answer["locations"]
        );
        let heldout = source.replace("+++", "++-");
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":heldout,"version":21}),
        )
        .unwrap();
        let mut heldout_params = params.clone();
        heldout_params["version"] = json!(21);
        assert_eq!(
            ws.dispatch("navigation/definition", &heldout_params)
                .unwrap()["locations"],
            answer["locations"]
        );
        let rhs =
            "module SymbolicScope\nmodule Selected =\n    let (+++) left right = left +++ right\n";
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":rhs,"version":22}),
        )
        .unwrap();
        let col = rhs.lines().nth(2).unwrap().rfind("+++").unwrap() + 1;
        assert_ne!(
            ws.dispatch(
                "navigation/definition",
                &json!({"path":path,"line":2,"character":col,"version":22})
            )
            .unwrap()["completeness"],
            "exact"
        );
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":source,"version":23}),
        )
        .unwrap();
        fs::write(
            root.join("Main.fsi"),
            "module SymbolicScope\nmodule Selected =\n    val (+++) : int -> int -> int\n",
        )
        .unwrap();
        fs::write(root.join("Test.fsproj"),"<Project><ItemGroup><Compile Include=\"Main.fsi\"/><Compile Include=\"Main.fs\"/></ItemGroup></Project>").unwrap();
        ws.dispatch(
            "workspace/filesChanged",
            &json!({"paths":[root.join("Test.fsproj"),root.join("Main.fsi")]}),
        )
        .unwrap();
        current["version"] = json!(23);
        assert_ne!(
            ws.dispatch("navigation/definition", &current).unwrap()["completeness"],
            "exact"
        );
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn single_partial_pattern_definition_is_current_scoped_and_not_a_reference_proof() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        fs::write(
            root.join("Test.fsproj"),
            "<Project><ItemGroup><Compile Include=\"Main.fs\"/></ItemGroup></Project>",
        )
        .unwrap();
        let source="module ActiveCase\n[<RequireQualifiedAccess>]\ntype Other =\n    | Hit of int\nlet (|Hit|_|) value = if value > 0 then Some value else None\nlet classify value =\n    match value with\n    | Hit captured -> captured\n    | _ -> 0\n";
        fs::write(&path, source).unwrap();
        let query = |text: &str, name: &str| {
            let (line, text) = text
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains("captured ->"))
                .unwrap();
            (
                line,
                text[..text.find(name).unwrap()].encode_utf16().count() + 1,
            )
        };
        let (line, col) = query(source, "Hit");
        let params = json!({"path":path,"line":line,"character":col});
        let positive = ws.dispatch("navigation/definition", &params).unwrap();
        assert_eq!(positive["completeness"], "exact", "{positive}");
        assert_eq!(
            positive["locations"][0]["range"],
            json!({"start":{"line":4,"character":6},"end":{"line":4,"character":9}})
        );
        assert_ne!(
            ws.dispatch("navigation/references", &params).unwrap()["completeness"],
            "exact"
        );
        for (index, text) in [
            source.replace("let classify", "open Imported\nlet classify"),
            source.replace("let classify", "type Later = Hit of int\nlet classify"),
            source.replace(
                "let classify",
                "let (|Hit|_|) value = Some value\nlet classify",
            ),
            source.replace("let classify", "let ``Hit`` = 42\nlet classify"),
            source.replace(
                "let (|Hit|_|)",
                "[<CompiledName(\"Changed\")>]\nlet (|Hit|_|)",
            ),
            source.replace("let (|Hit|_|)", "let ``|Hit|_|``"),
            source.replace("let (|Hit|_|) value", "let (|Hit|_|) cutoff value"),
            source.replace("let (|Hit|_|) value", "let (|Hit|Miss|) value"),
            source.replace("| Hit captured", "| Other.Hit captured"),
            source.replace("| Hit captured ->", "| Hit captured _ ->"),
            source.replace(
                "let classify value =",
                "let classify value =\n    let (|Hit|_|) value = Some value",
            ),
            format!("#if ACTIVE\n{source}#endif\n"),
        ]
        .into_iter()
        .enumerate()
        {
            let version = index as i64 + 1;
            ws.dispatch(
                "document/change",
                &json!({"path":path,"text":text,"version":version}),
            )
            .unwrap();
            // Extra-argument control has a different pattern but the same token.
            let (line, textline) = text
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains("captured") && line.contains("->"))
                .unwrap();
            let col = textline[..textline.find("Hit").unwrap()]
                .encode_utf16()
                .count()
                + 1;
            let answer = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":path,"line":line,"character":col,"version":version}),
                )
                .unwrap();
            assert_ne!(answer["completeness"], "exact", "case{index}: {answer}");
        }
        let heldout = source.replace("Hit", "Found");
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":heldout,"version":30}),
        )
        .unwrap();
        let (line, col) = query(&heldout, "Found");
        assert_eq!(
            ws.dispatch(
                "navigation/definition",
                &json!({"path":path,"line":line,"character":col,"version":30})
            )
            .unwrap()["locations"][0]["range"]["end"]["character"],
            11
        );
        assert!(ws
            .dispatch(
                "navigation/definition",
                &json!({"path":path,"line":line,"character":col,"version":1})
            )
            .is_err());
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":source,"version":31}),
        )
        .unwrap();
        let mut restored = params.clone();
        restored["version"] = json!(31);
        assert_eq!(
            ws.dispatch("navigation/definition", &restored).unwrap()["locations"],
            positive["locations"]
        );
        fs::write(
            root.join("Main.fsi"),
            "module ActiveCase\nval (|Hit|_|) : int -> int option\n",
        )
        .unwrap();
        fs::write(root.join("Test.fsproj"),"<Project><ItemGroup><Compile Include=\"Main.fsi\"/><Compile Include=\"Main.fs\"/></ItemGroup></Project>").unwrap();
        ws.dispatch(
            "workspace/filesChanged",
            &json!({"paths":[root.join("Test.fsproj"),root.join("Main.fsi")]}),
        )
        .unwrap();
        assert_ne!(
            ws.dispatch("navigation/definition", &restored).unwrap()["completeness"],
            "exact"
        );
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn target_scoped_compile_inventory_does_not_certify_definition_context() {
        // The item text/order is identical; only evaluation-time versus target-time
        // membership differs. No target or compiler is executed by this test.
        for target_scoped in [false, true] {
            let (root, mut ws) = fixture();
            let path = root.join("Main.fs");
            let source="module M\nlet (|Hit|_|) value = Some value\nlet classify value = match value with\n    | Hit captured -> captured\n    | _ -> 0\n";
            fs::write(&path, source).unwrap();
            let items = "<ItemGroup><Compile Include=\"Main.fs\"/></ItemGroup>";
            let project = if target_scoped {
                format!("<Project><Target Name=\"NeverInvoked\">{items}</Target></Project>")
            } else {
                format!("<Project>{items}</Project>")
            };
            fs::write(root.join("Test.fsproj"), project).unwrap();
            let tree = ws.dispatch("workspace/tree", &json!({})).unwrap();
            let definition = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":path,"line":3,"character":7}),
                )
                .unwrap();
            assert_eq!(tree["projects"][0]["files"], json!([normalize(&path)]));
            if target_scoped {
                assert_eq!(
                    tree["projects"][0]["exact"], false,
                    "target tree={tree}; definition={definition}"
                );
                assert_ne!(definition["completeness"], "exact", "{definition}");
                assert!(
                    definition["candidates"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|candidate| candidate["range"]["start"]
                            == json!({"line":1,"character":6})),
                    "{definition}"
                );
            } else {
                assert_eq!(tree["projects"][0]["exact"], true, "{tree}");
                assert_eq!(definition["completeness"], "exact", "{definition}");
                assert_eq!(
                    definition["locations"][0]["range"]["start"],
                    json!({"line":1,"character":6})
                );
            }
            drop(ws);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn target_scoped_signature_does_not_route_consumer_to_uninvoked_contract() {
        for context in ["ordinary", "target", "exclude", "extensions"] {
            let (root, mut ws) = fixture();
            fs::write(
                root.join("Wire.fsi"),
                "module Wire\nval render: text: string -> string\n",
            )
            .unwrap();
            fs::write(
                root.join("Wire.fs"),
                "module Wire\nlet render text = text\nlet concealed = 99\n",
            )
            .unwrap();
            fs::write(
                root.join("Consumer.fs"),
                "module Consumer\nopen Wire\nlet shown = render \"value\"\n",
            )
            .unwrap();
            let signature = "<ItemGroup><Compile Include=\"Wire.fsi\"/></ItemGroup>";
            let signature = match context {
                "target" => format!("<Target Name=\"NeverInvoked\">{signature}</Target>"),
                "exclude" => {
                    "<ItemGroup><Compile Include=\"Wire.fsi\" Exclude=\"Wire.fsi\"/></ItemGroup>"
                        .into()
                }
                "extensions" => format!("<ProjectExtensions>{signature}</ProjectExtensions>"),
                _ => signature.into(),
            };
            fs::write(root.join("Test.fsproj"),format!("<Project>{signature}<ItemGroup><Compile Include=\"Wire.fs\"/><Compile Include=\"Consumer.fs\"/></ItemGroup></Project>")).unwrap();
            let tree = ws.dispatch("workspace/tree", &json!({})).unwrap();
            let definition = ws
                .dispatch(
                    "navigation/definition",
                    &json!({"path":root.join("Consumer.fs"),"line":2,"character":13}),
                )
                .unwrap();
            let is_signature = |location: &Value| {
                location["uri"]
                    .as_str()
                    .is_some_and(|uri| uri.ends_with("Wire.fsi"))
            };
            if context != "ordinary" {
                // Target-only, excluded or metadata-only items cannot prove a contract.
                // Retain candidates, but never claim its contract is the exact target.
                assert!(
                    !(definition["completeness"] == "exact"
                        && definition["locations"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(is_signature)),
                    "tree={tree}; definition={definition}"
                );
                if context == "extensions" {
                    assert_eq!(definition["completeness"], "exact", "{definition}");
                    assert!(definition["locations"][0]["uri"]
                        .as_str()
                        .unwrap()
                        .ends_with("Wire.fs"));
                } else {
                    assert_ne!(definition["completeness"], "exact", "{definition}");
                    assert!(
                        definition["candidates"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|location| location["uri"].as_str().unwrap().ends_with("Wire.fs")),
                        "{definition}"
                    );
                }
                assert_eq!(
                    tree["projects"][0]["files"].as_array().unwrap().len(),
                    if context == "extensions" { 2 } else { 3 }
                );
                let wire = root.join("Wire.fs");
                ws.dispatch("document/change",&json!({"path":wire,"version":1,"text":"module Wire\nlet renamed text = text\n"})).unwrap();
                let changed = ws
                    .dispatch(
                        "navigation/definition",
                        &json!({"path":root.join("Consumer.fs"),"line":2,"character":13}),
                    )
                    .unwrap();
                assert_ne!(changed["completeness"], "exact", "{changed}");
                assert!(
                    !changed["candidates"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|location| location["uri"].as_str().unwrap().ends_with("Wire.fs")),
                    "{changed}"
                );
                let local = "module Consumer\nlet values = List.map (fun local -> local) [1]\n";
                let col = local.lines().nth(1).unwrap().rfind("local").unwrap();
                ws.dispatch(
                    "document/open",
                    &json!({"path":root.join("Consumer.fs"),"text":local,"version":1}),
                )
                .unwrap();
                let local_answer=ws.dispatch("navigation/definition",&json!({"path":root.join("Consumer.fs"),"line":1,"character":col,"version":1})).unwrap();
                assert_eq!(local_answer["completeness"], "exact", "{local_answer}");
                assert_eq!(
                    local_answer["locations"][0]["range"]["start"]["character"],
                    27
                );
            } else {
                assert_eq!(definition["completeness"], "exact", "{definition}");
                assert!(is_signature(&definition["locations"][0]), "{definition}");
            }
            drop(ws);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn document_definition_rpc_requires_current_acknowledgement_and_never_falls_back() {
        let (root, mut ws) = fixture();
        let path = root.join("Main.fs");
        let source="module Main\nlet mapped = List.map (fun value -> value) [1]\nlet exposed value = value\n";
        let line = source.lines().nth(1).unwrap();
        let use_col = line.rfind("value").unwrap();
        let declaration_col = line.find("value").unwrap();
        let request = json!({"path":path,"line":1,"character":use_col,"version":1});
        assert!(ws
            .dispatch("navigation/documentDefinition", &request)
            .is_err());
        ws.dispatch(
            "document/open",
            &json!({"path":path,"text":source,"version":1}),
        )
        .unwrap();
        let before_reads = ws.index.counters.files_read;
        for invalid in [
            json!({}),
            json!({"version":null}),
            json!({"version":"1"}),
            json!({"version":0}),
            json!({"version":1,"documentVersion":2}),
            json!({"version":1,"documentVersion":null}),
        ] {
            let mut p = json!({"path":path,"line":1,"character":use_col});
            p.as_object_mut()
                .unwrap()
                .extend(invalid.as_object().unwrap().clone());
            assert!(
                ws.dispatch("navigation/documentDefinition", &p).is_err(),
                "{p}"
            );
        }
        let exact = ws
            .dispatch("navigation/documentDefinition", &request)
            .unwrap();
        assert_eq!(exact["completeness"], "exact", "{exact}");
        assert_eq!(exact["proofScope"], "document");
        assert_eq!(
            exact["sourceHash"],
            ws.overlays[&normalize(&path)].source_hash
        );
        assert_eq!(exact["version"], 1);
        assert_eq!(
            exact["locations"],
            json!([{"uri":uri(&normalize(&path)),"range":{"start":{"line":1,"character":declaration_col},"end":{"line":1,"character":declaration_col+5}}}])
        );
        let mut aliases = request.clone();
        aliases["documentVersion"] = json!(1);
        assert_eq!(
            ws.dispatch("navigation/documentDefinition", &aliases)
                .unwrap()["documentProofId"],
            exact["documentProofId"]
        );
        // An ordinary public formal may require signature routing. Explicit miss,
        // with no attempt to prepare a model, find candidates or consult context.
        let miss = ws
            .dispatch(
                "navigation/documentDefinition",
                &json!({"path":path,"position":{"line":2,"character":22},"documentVersion":1}),
            )
            .unwrap();
        assert_eq!(miss["documentProofMiss"], true, "{miss}");
        assert_eq!(miss["completeness"], "unavailable");
        assert_eq!(miss["locations"], json!([]));
        assert!(miss.get("candidates").is_none());
        assert!(miss.get("indexing").is_none());
        assert!(!ws.model_loaded);
        assert!(!ws.background.requested);
        assert_eq!(ws.index.counters.query_count, 0);
        assert_eq!(ws.index.counters.files_read, before_reads);
        assert_eq!(ws.index.counters.files_parsed, 0);
        let cancelled = cancellation::CancellationToken::new();
        cancelled.cancel();
        ws.set_cancellation(cancelled);
        assert!(ws
            .dispatch("navigation/documentDefinition", &request)
            .unwrap_err()
            .is::<cancellation::Cancelled>());
        ws.set_cancellation(cancellation::CancellationToken::new());
        let changed = source.replace("value", "other");
        ws.dispatch(
            "document/change",
            &json!({"path":path,"text":changed,"version":2}),
        )
        .unwrap();
        assert!(ws
            .dispatch("navigation/documentDefinition", &request)
            .is_err());
        let mut current = request.clone();
        current["version"] = json!(2);
        let fresh = ws
            .dispatch("navigation/documentDefinition", &current)
            .unwrap();
        assert_eq!(fresh["completeness"], "exact");
        assert_eq!(fresh["version"], 2);
        assert_ne!(fresh["sourceHash"], exact["sourceHash"]);
        assert_ne!(fresh["documentProofId"], exact["documentProofId"]);
        assert_eq!(fresh["locations"], exact["locations"]);
        ws.mode = Mode::Lexical;
        assert!(ws
            .dispatch("navigation/documentDefinition", &current)
            .is_err());
        ws.mode = Mode::Indexed;
        ws.dispatch("document/close", &json!({"path":path}))
            .unwrap();
        assert!(ws
            .dispatch("navigation/documentDefinition", &current)
            .is_err());
        drop(ws);
        fs::remove_dir_all(root).unwrap();
    }
}
