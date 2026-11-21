mod builtins;
pub mod cancellation;
pub mod context;
pub mod decompiler;
mod document_navigation;
mod foreground;
mod helper;
mod index;
mod library_types;
mod local_lexical;
mod metadata;
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
    root: String,
    mode: Mode,
    model: Model,
    index: Index,
    overlays: HashMap<String, Overlay>,
    document_navigation: document_navigation::DocumentNavigation,
    epoch: u64,
    snapshot: String,
    session_id: String,
    context_hash: String,
    model_hash: String,
    events: Receiver<notify::Result<notify::Event>>,
    pending_refresh: HashSet<String>,
    _watcher: Option<RecommendedWatcher>,
    watcher_reliable: bool,
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
    builtin_path: Option<String>,
    builtin_environment: Option<builtins::Environment>,
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
            watcher_reliable: false,
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
            epoch: 0,
            snapshot: String::new(),
            context_hash: String::new(),
            model_hash: String::new(),
            events,
            pending_refresh: HashSet::new(),
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
                    Ok(parsed) => Ok(parsed),
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
        let started = Instant::now();
        self.model = if let Some(path) = &self.evaluated_model_path {
            Model::from_evaluated(
                &self.root,
                &serde_json::from_str(&foreground::read_text(Path::new(path), u64::MAX)?)?,
            )?
        } else {
            project::discover(&self.root)?
        };
        self.index.retain_paths(&self.model.files)?;
        // Register only known source/configuration directories, not a recursive
        // crawl through .git, toolchains, node_modules or build outputs.
        let (tx, rx) = mpsc::channel();
        self.events = rx;
        if let Ok(mut watcher) = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        }) {
            self.watcher_reliable = true;
            let mut dirs = HashSet::from([PathBuf::from(&self.root)]);
            for path in self
                .model
                .files
                .iter()
                .chain(self.model.projects.iter().map(|p| &p.path))
                .chain(self.model.inputs.iter())
            {
                if let Some(parent) = Path::new(path).parent() {
                    dirs.insert(parent.to_path_buf());
                }
            }
            let mut watches = HashMap::new();
            for mut directory in dirs {
                while !directory.is_dir() && directory.pop() {}
                watches.insert(directory, false);
            }
            for (root, recursive) in self.model.membership_roots() {
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
        self.model_load_micros += started.elapsed().as_micros() as u64;
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
        let mut context_changed = false;
        let mut semantic_changed = false;
        for event in self.events.try_iter() {
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
        let _scope = self.cancellation.enter();
        if self.cancellation.is_cancelled() {
            self.document_navigation.clear();
            self.cancellation.check()?;
        }
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
                self.ensure_model()?;
                self.drain_events()?;
                Ok(self.model.tree())
            }
            "workspace/stats" => self.stats(),
            "workspace/mode" => {
                let mode = Mode::parse(string(p, "mode")?)?;
                if self.mode != mode {
                    self.helper = None;
                    self.document_navigation.clear();
                    self.mode = mode;
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
                self.ensure_model()?;
                self.drain_events()?;
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
            "navigation/definition" | "navigation/references" | "navigation/hover" => {
                if self.mode != Mode::Indexed {
                    return self.navigation(method, p);
                }
                let _scope = foreground::Scope::enter();
                let answer = self.navigation(method, p);
                self.cancellation.check()?;
                if let Err(limit) = foreground::check() {
                    // Some speculative proof edges intentionally suppress errors.
                    // Never publish a positive answer or a name-absence certificate
                    // if part of its required work was declined.
                    self.covered_names.clear();
                    self.refresh_snapshot()?;
                    let mut unavailable =
                        self.empty(method == "navigation/hover", &limit.to_string());
                    let version = self.overlays.get(&path_param(p)?).map(|o| o.version);
                    unavailable["documentVersion"] = json!(version);
                    unavailable["version"] = json!(version);
                    unavailable["snapshotId"] = json!(self.snapshot);
                    unavailable["snapshot"] = json!(self.epoch);
                    Ok(unavailable)
                } else {
                    answer
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
        let (changed, source_hash) = self
            .index
            .overlay(&path, text, self.mode != Mode::Lexical)?;
        self.document_navigation.invalidate(&path);
        self.overlays.insert(
            path.clone(),
            Overlay {
                path,
                text: text.into(),
                version,
                source_hash: source_hash.to_hex().to_string(),
            },
        );
        if changed {
            self.semantic_revision += 1;
            self.covered_names.clear();
            self.refresh_snapshot()?;
        }
        Ok(
            json!({"accepted":true,"version":version,"documentVersion":version,"snapshot":self.epoch,"snapshotId":self.snapshot}),
        )
    }
    fn resolve(&mut self, path: &str, p: &ParsedFile, token: usize) -> Result<Option<Symbol>> {
        if p.possible_named_argument(token) {
            return self.call_argument_binding(path, p, token);
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
                    let choices = self.opened_modules(path, p, token, &open.name)?;
                    let mut known = false;
                    for module in choices {
                        known = true;
                        let matches =
                            self.visible_exports(&module, &t.text, path, p, token, &visible)?;
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
        if let Some(q) = qualifier {
            let first = q.split('.').next().unwrap_or("");
            // F# resolves an expression value before a same-spelled module alias.
            if p.declarations
                .iter()
                .any(|d| d.name == first && d.visible_start <= t.start && t.start < d.scope_end)
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
                let opened = self.opened_modules(path, p, token, &open.name)?;
                if opened.is_empty() {
                    // This ordered import has an unknown name environment. Do
                    // not skip it then certify an older same-spelled binding.
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
                return Ok(None);
            }
            let matches = self.visible_exports(&module, &t.text, path, p, token, &visible)?;
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
    ) -> Result<Vec<(String, usize)>> {
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
            self.index.exported_in(module, name, &instances)?
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
        Ok(matches)
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
        self.type_proofs = Default::default();
        self.ensure_model()?;
        self.drain_events()?;
        let requested_project = p
            .get("projectPath")
            .and_then(Value::as_str)
            .map(project::from_uri);
        let selected = self
            .model
            .source_instance_project(&path, requested_project.as_deref())
            .map(|p| p.path.clone());
        if requested_project.is_some() && selected.is_none() {
            return Ok(self.empty(
                method == "navigation/hover",
                "The selected project does not identify this source instance",
            ));
        }
        self.select_project(selected);
        if self.ambiguous_instance(&path) {
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
                if needs_name_coverage && !parsed.possible_named_argument(token) {
                    self.ensure_name_coverage(&parsed.tokens[token].text)?;
                }
            }
        }
        let key = answer_key(self.active_project.as_deref())?;
        let mut result = if self.mode == Mode::Semantic {
            self.semantic(method, p)?
        } else if let Some(cached) = semantic_cached {
            cached
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
            let mut value = if let Some(token) = parsed.token_at(line, col) {
                if hover {
                    self.hover_native(&path, &parsed, token)?
                } else if let Some(symbol) = if method == "navigation/references" {
                    self.reference_binding(&path, &parsed, token)?
                } else {
                    self.definition_binding(&path, &parsed, token)?
                } {
                    if method == "navigation/definition" {
                        let Some(target) =
                            self.definition_target(&path, &parsed, token, symbol.clone())?
                        else {
                            return Ok(
                                self.empty(false, "Declaration-to-contract routing is not proven")
                            );
                        };
                        let mut answer = json!({"locations":[self.location(&target)?],"completeness":"exact","source":self.mode.name()});
                        if symbol.member {
                            answer["proof"] = json!({"kind":"nominal-receiver-facts","declarationPath":symbol.path,"invalidation":"source-and-project-snapshot"});
                        }
                        answer
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
        for arg in origin.call_argument_name_candidates.iter().filter(|arg| {
            arg.callee
                .is_some_and(|head| origin.tokens[head].text == method.name)
        }) {
            self.cancellation.check()?;
            if self
                .call_argument_binding(&symbol.path, origin, arg.token)?
                .as_ref()
                != Some(symbol)
            {
                return Ok(None);
            }
            labels.insert(arg.token);
            heads.insert(arg.callee.unwrap());
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
        let is_member = origin
            .record_types
            .iter()
            .any(|r| r.fields.iter().any(|f| f.token == symbol.token))
            || origin
                .method_declarations
                .iter()
                .any(|m| m.token == symbol.token);
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
                    let argument_binding = if parsed.possible_named_argument(token) {
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
                            // FCS reports the written module qualification as part of a
                            // value use's range, including an abbreviation such as A.x.
                            let range = if !is_declaration
                                && declaration.is_some_and(|d| d.top_level && d.kind == "value")
                            {
                                parsed.module_value_use_range(token)
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
        for path in self.model.files.clone() {
            self.refresh_file(&path, false)?;
        }
        self.refresh_context()?;
        self.refresh_snapshot()?;
        Ok(self.model.tree())
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
        assert!(!ws.model_loaded);
        assert_eq!(ws.index.counters.files_read, 0);
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
        let partial = query(&mut ws);
        assert_eq!(partial["completeness"], "unavailable", "{partial}");
        assert!(partial["reason"].as_str().unwrap().contains("foreground"));
        assert!(!ws.covered_names.contains("value"));
        assert!(ws.index.counters.files_read <= foreground::MAX_READS as u64);
        assert!(ws.index_errors.is_empty());
        ws.dispatch("workspace/index", &json!({})).unwrap();
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
        let def = ws
            .dispatch(
                "navigation/definition",
                &json!({"path":path,"line":4,"character":19}),
            )
            .unwrap();
        assert_eq!(def["completeness"], "unavailable", "{def}");
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
}
