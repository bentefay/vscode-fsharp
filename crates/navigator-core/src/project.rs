use anyhow::{Context, Result};
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
#[path = "environment.rs"]
pub mod environment;
#[path = "membership.rs"]
pub mod membership;
pub use membership::SourceMembership;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub path: String,
    pub name: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default)]
    pub items: Vec<Value>,
    #[serde(default)]
    pub assembly_references: Vec<String>,
    #[serde(default)]
    pub workspace_assembly_references: Vec<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub target_framework: String,
    #[serde(default)]
    pub configuration: String,
    #[serde(default)]
    pub compiler_args: Vec<String>,
    #[serde(default)]
    pub evaluation: String,
    #[serde(default)]
    pub source_membership: Vec<SourceMembership>,
    #[serde(default)]
    pub membership_reusable: Option<bool>,
    #[serde(default)]
    pub membership_diagnostics: Vec<String>,
    #[serde(default)]
    pub environment_certificate: Option<Value>,
    #[serde(default)]
    pub reference_bindings: Vec<Value>,
    #[serde(default)]
    pub exact: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub root: String,
    pub projects: Vec<Project>,
    pub files: Vec<String>,
    pub exact: bool,
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(skip)]
    pub validation_metrics: crate::validation::Metrics,
}

pub fn normalize(path: impl AsRef<Path>) -> String {
    let p = path.as_ref();
    let p = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    let mut out = PathBuf::new();
    for part in p.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            _ => out.push(part.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}
pub fn uri(path: &str) -> String {
    if path.contains("://") {
        return path.to_string();
    }
    url::Url::from_file_path(path)
        .map(|x| x.to_string())
        .unwrap_or_else(|_| path.into())
}
pub fn from_uri(path: &str) -> String {
    if path.starts_with("file:") {
        if let Ok(u) = url::Url::parse(path) {
            if let Ok(p) = u.to_file_path() {
                return normalize(p);
            }
        }
    }
    normalize(path)
}
pub(crate) fn has_extension(path: &Path, suffix: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(suffix))
}
pub fn is_source(path: &Path) -> bool {
    ["fs", "fsi", "fsx"]
        .iter()
        .any(|suffix| has_extension(path, suffix))
}
pub(crate) fn signature(path: &str) -> bool {
    has_extension(Path::new(path), "fsi")
}
pub(crate) fn implementation(path: &str) -> bool {
    has_extension(Path::new(path), "fs")
}

// Fold only the compiler-recognized extension. Directory and basename identity
// still use the exact paths supplied by the ordered compilation instance.
pub(crate) fn same_stem(left: &str, right: &str) -> bool {
    left.rsplit_once('.')
        .zip(right.rsplit_once('.'))
        .is_some_and(|((a, _), (b, _))| a == b)
}
pub(crate) fn has_signature<'a>(path: &str, visible: impl IntoIterator<Item = &'a String>) -> bool {
    implementation(path)
        && visible
            .into_iter()
            .any(|candidate| signature(candidate) && same_stem(path, candidate))
}
// A compiled script can add sources through #load. The evaluated model
// records its path, but has no dependency certificate for that source closure.
pub(crate) fn explicit_source(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("fs") || ext.eq_ignore_ascii_case("fsi"))
}

// Fsc expands @response files before parsing options and filenames. Their
// contents and inner dependencies are absent from the evaluated certificate.
pub(crate) fn modeled_compiler_argument(arg: &str) -> bool {
    let arg = arg.trim().trim_matches('"');
    !arg.starts_with('@')
        && arg != "--"
        && (arg.starts_with('-') || explicit_source(Path::new(arg)))
}

pub fn excluded(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".tools"
            | "node_modules"
            | "target"
            | "bin"
            | "obj"
            | ".fsharp-navigator"
            | "artifacts"
    )
}

pub fn discover(root: &str) -> Result<Model> {
    let mut projects = Vec::new();
    let mut sources = Vec::new();
    let mut exact = true;
    let mut diagnostics = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !excluded(&e.file_name().to_string_lossy()))
    {
        crate::cancellation::check()?;
        crate::foreground::entries(1)?;
        let e = entry?;
        if !e.file_type().is_file() {
            continue;
        }
        let p = e.path();
        if is_source(p) {
            sources.push(normalize(p))
        }
        if p.extension().and_then(|x| x.to_str()) == Some("fsproj") {
            let (project, complete, notes) = parse_project(p)?;
            exact &= complete;
            diagnostics.extend(notes);
            projects.push(project);
        }
        if matches!(
            e.file_name().to_str(),
            Some("Directory.Build.props" | "Directory.Build.targets")
        ) {
            exact = false;
            diagnostics.push("Directory.Build inputs require evaluated project manifest".into());
        }
    }
    // Ambient MSBuild imports may change order, defines, references and source
    // membership. A provisional parser cannot certify their absence by looking
    // only inside a project XML document.
    let ambient = Path::new(root).ancestors().any(|p| {
        p.join("Directory.Build.props").exists() || p.join("Directory.Build.targets").exists()
    }) || diagnostics.iter().any(|d| d.contains("Directory.Build"));
    if ambient {
        exact = false;
        for p in &mut projects {
            p.exact = false;
        }
        diagnostics
            .push("Ambient Directory.Build imports require evaluated project context".into());
    }
    projects.sort_by(|a, b| a.path.cmp(&b.path));
    sources.sort();
    // Explicit compile includes take priority over discovery, including linked
    // or generated files under otherwise skipped directories.
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for p in &projects {
        for f in &p.files {
            if seen.insert(f.clone()) {
                files.push(f.clone())
            }
        }
    }
    if projects.is_empty() {
        exact = false;
        diagnostics.push("No evaluated project: loose file scope only".into());
        files = sources;
    }
    Ok(Model {
        validation_metrics: Default::default(),
        root: root.into(),
        projects,
        files,
        exact,
        diagnostics,
        inputs: Vec::new(),
    })
}

fn parse_project(path: &Path) -> Result<(Project, bool, Vec<String>)> {
    let source = crate::foreground::read_text(path, u64::MAX)
        .with_context(|| format!("read {}", path.display()))?;
    let mut reader = Reader::from_str(&source);
    reader.config_mut().trim_text(true);
    let mut files = Vec::new();
    let mut refs = Vec::new();
    // MSBuild can inherit OtherFlags even when the project does not define it.
    let mut exact = !std::env::vars_os().any(|(key, value)| {
        key.to_str()
            .is_some_and(|key| key.eq_ignore_ascii_case("OtherFlags"))
            && !value.is_empty()
    });
    let mut notes = Vec::new();
    let mut defines = Vec::new();
    let mut target_framework = String::new();
    let mut configuration = String::new();
    let mut property = String::new();
    let parent = path.parent().unwrap_or(Path::new("."));
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                property = name.clone();
                for a in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(a.key.as_ref());
                    let value = a.unescape_value()?.into_owned();
                    if key == "Condition" || name == "Import" {
                        exact = false;
                    }
                    if name == "Compile" && key == "Include" {
                        if value.contains(['*', '?', '$', ';']) {
                            exact = false;
                            continue;
                        }
                        crate::foreground::entries(1)?;
                        let source = parent.join(value.replace('\\', "/"));
                        exact &= explicit_source(&source);
                        files.push(normalize(source));
                    }
                    if name == "Compile" && matches!(key.as_ref(), "Remove" | "Update") {
                        exact = false;
                    }
                    if name == "ProjectReference" && key == "Include" {
                        crate::foreground::entries(1)?;
                        refs.push(normalize(parent.join(value.replace('\\', "/"))));
                    }
                }
                if name.eq_ignore_ascii_case("OtherFlags")
                    || matches!(
                        name.as_str(),
                        "TargetFrameworks" | "EnableDefaultCompileItems"
                    )
                {
                    exact = false;
                }
            }
            Ok(Event::Text(text)) => {
                if matches!(
                    property.as_str(),
                    "DefineConstants" | "TargetFramework" | "Configuration"
                ) {
                    let value = text.unescape()?.into_owned();
                    if value.contains(['$', '@', '%']) {
                        exact = false;
                    } else {
                        match property.as_str() {
                            "DefineConstants" => defines.extend(
                                value
                                    .split([';', ','])
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_owned),
                            ),
                            "TargetFramework" => target_framework = value,
                            "Configuration" => configuration = value,
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(_)) => property.clear(),
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    if files.is_empty() {
        exact = false;
    }
    if !exact {
        notes.push(format!(
            "{} needs MSBuild evaluation for authoritative compile inputs",
            path.display()
        ));
    }
    Ok((
        Project {
            path: normalize(path),
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            files,
            references: refs,
            items: Vec::new(),
            assembly_references: Vec::new(),
            workspace_assembly_references: Vec::new(),
            defines,
            target_framework,
            configuration,
            compiler_args: Vec::new(),
            evaluation: String::new(),
            source_membership: Vec::new(),
            membership_reusable: None,
            environment_certificate: None,
            membership_diagnostics: Vec::new(),
            reference_bindings: Vec::new(),
            exact,
        },
        exact,
        notes,
    ))
}

impl Model {
    pub fn from_evaluated(root: &str, value: &Value) -> Result<Self> {
        let mut generation = crate::validation::Generation::default();
        let result = Self::from_evaluated_with(root, value, &mut generation);
        generation.finish(result).map(|mut model| {
            model.validation_metrics = generation.metrics();
            model
        })
    }
    fn from_evaluated_with(
        root: &str,
        value: &Value,
        generation: &mut crate::validation::Generation,
    ) -> Result<Self> {
        let mut projects: Vec<Project> =
            serde_json::from_value(value.get("projects").cloned().unwrap_or(json!([])))?;
        for project in &projects {
            crate::foreground::entries(1 + project.files.len() + project.references.len())?;
        }
        let mut files = Vec::new();
        let mut seen = HashSet::new();
        let exact = value.get("completeness").and_then(Value::as_str) == Some("exact");
        // An exported evaluation is a snapshot, not an authority independent of
        // its build inputs. Validate once when the lazy model is loaded; live
        // changes are then tracked by the workspace's input watchers.
        if exact {
            for project in &projects {
                anyhow::ensure!(
                    project.compiler_args.iter().all(|arg| modeled_compiler_argument(arg))
                        && project.files.iter().all(|file| explicit_source(Path::new(file))),
                    "Evaluated project includes an unmodeled compiler input: {}; prepare a project with explicit source arguments", project.path
                );
                for binding in &project.reference_bindings {
                    crate::foreground::certificate_visit()?;
                    // Source-project identities do not depend on whether a
                    // workspace DLL has been built yet. External resolution
                    // existence is part of the imported reference selection.
                    if binding.get("isWorkspace").and_then(Value::as_bool) == Some(true) {
                        continue;
                    }
                    if let (Some(path), Some(expected)) = (
                        binding.get("path").and_then(Value::as_str),
                        binding.get("exists").and_then(Value::as_bool),
                    ) {
                        anyhow::ensure!(
                            Path::new(path).try_exists()? == expected,
                            "Evaluated reference binding is stale: {path}; prepare it again"
                        );
                    }
                }
            }
            let inputs = value
                .get("inputHashes")
                .and_then(Value::as_array)
                .context("Evaluated project model has no input fingerprints; prepare it again")?;
            let mut fingerprints = HashMap::new();
            for input in inputs {
                crate::cancellation::check()?;
                let path = input
                    .get("path")
                    .and_then(Value::as_str)
                    .context("Evaluated project input is missing its path")?;
                let expected = input
                    .get("sha256")
                    .and_then(Value::as_str)
                    .context("Evaluated project input is missing its SHA256")?;
                let path = normalize(path);
                anyhow::ensure!(
                    fingerprints.insert(path.clone(), expected).is_none(),
                    "Duplicate evaluated project input: {path}"
                );
                generation
                    .verify_file(Path::new(&path), expected, true)
                    .with_context(|| {
                        format!("Evaluated project model is stale: {path}; prepare it again")
                    })?;
            }
            for path in value
                .get("inputs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .chain(projects.iter().map(|p| p.path.as_str()))
            {
                anyhow::ensure!(
                    fingerprints.contains_key(&normalize(path)),
                    "Evaluated project model has no fingerprint for {path}; prepare it again"
                );
            }
        }
        if exact {
            let mut validated = HashSet::new();
            for project in &projects {
                anyhow::ensure!(project.membership_reusable == Some(true),
                    "Evaluated project lacks reusable source-membership provenance: {}; prepare it again. {}",
                    project.path, project.membership_diagnostics.join("; "));
                environment::validate_with(
                    project.environment_certificate.as_ref().context(
                        "Evaluated project lacks environment provenance; prepare it again",
                    )?,
                    generation,
                )?;
                for membership in &project.source_membership {
                    crate::cancellation::check()?;
                    if validated.insert(serde_json::to_string(membership)?) {
                        membership.validate()?;
                    }
                }
            }
        }
        for p in &mut projects {
            p.exact = exact;
            p.path = normalize(&p.path);
            for f in &mut p.files {
                *f = normalize(&*f);
                if seen.insert(f.clone()) {
                    files.push(f.clone())
                }
            }
        }
        let inputs = value
            .get("inputs")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            root: root.into(),
            projects,
            files,
            exact: value.get("completeness").and_then(Value::as_str) == Some("exact"),
            diagnostics: Vec::new(),
            validation_metrics: Default::default(),
            inputs,
        })
    }
    pub fn membership_roots(&self) -> Vec<(String, bool)> {
        let mut roots: HashMap<String, bool> = HashMap::new();
        for membership in self.projects.iter().flat_map(|p| &p.source_membership) {
            roots
                .entry(membership.root.clone())
                .and_modify(|recursive| *recursive |= membership.recursive)
                .or_insert(membership.recursive);
        }
        for certificate in self
            .projects
            .iter()
            .filter_map(|p| p.environment_certificate.as_ref())
        {
            for (path, recursive) in environment::globalization::roots(certificate) {
                roots
                    .entry(path)
                    .and_modify(|old| *old |= recursive)
                    .or_insert(recursive);
            }
        }
        let mut roots = roots.into_iter().collect::<Vec<_>>();
        roots.sort();
        roots
    }
    /// Caller restricts this to create/remove/rename/rescan, not ordinary edits.
    pub fn membership_affects(&self, path: &Path) -> bool {
        self.projects
            .iter()
            .flat_map(|p| &p.source_membership)
            .any(|membership| membership.affects(path))
            || self
                .projects
                .iter()
                .filter_map(|p| p.environment_certificate.as_ref())
                .any(|certificate| environment::globalization::affects(certificate, path))
    }
    pub fn validate_membership(&self) -> Result<()> {
        let mut checked = HashSet::new();
        for membership in self.projects.iter().flat_map(|p| &p.source_membership) {
            if checked.insert(serde_json::to_string(membership)?) {
                membership.validate()?;
            }
        }
        for certificate in self
            .projects
            .iter()
            .filter_map(|p| p.environment_certificate.as_ref())
        {
            environment::validate(certificate)?;
        }
        Ok(())
    }
    pub fn project_for(&self, path: &str) -> Option<&Project> {
        self.projects
            .iter()
            .find(|p| p.files.iter().any(|f| f == path))
    }
    pub fn visible_files(&self, path: &str) -> Vec<String> {
        let Some(project) = self.project_for(path) else {
            return vec![path.into()];
        };
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        self.add_references(project, &mut seen, &mut result);
        for f in &project.files {
            result.push(f.clone());
            if f == path {
                break;
            }
        }
        result
    }
    fn add_references(&self, p: &Project, seen: &mut HashSet<String>, out: &mut Vec<String>) {
        for r in &p.references {
            if seen.insert(r.clone()) {
                if let Some(other) = self.projects.iter().find(|x| &x.path == r) {
                    self.add_references(other, seen, out);
                    out.extend(other.files.clone())
                }
            }
        }
    }
    pub fn ambiguous_instance(&self, path: &str) -> bool {
        self.projects
            .iter()
            .filter(|p| p.files.iter().any(|f| f == path))
            .count()
            > 1
    }
    pub fn context_exact(&self, path: &str) -> bool {
        fn check(model: &Model, p: &Project, seen: &mut HashSet<String>) -> bool {
            if !p.exact {
                return false;
            }
            if !seen.insert(p.path.clone()) {
                return true;
            }
            p.references.iter().all(|r| {
                model
                    .projects
                    .iter()
                    .find(|x| &x.path == r)
                    .is_some_and(|x| check(model, x, seen))
            })
        }
        self.project_for(path)
            .is_some_and(|p| check(self, p, &mut HashSet::new()))
    }
    pub fn tree(&self) -> Value {
        let nodes:Vec<Value>=self.projects.iter().map(|p|{
            let children:Vec<Value>=p.files.iter().enumerate().map(|(i,f)|{
                let virtual_path=p.items.get(i).and_then(|x|x.get("virtualPath")).and_then(Value::as_str).map(str::to_string)
                    .unwrap_or_else(||Path::new(f).strip_prefix(Path::new(&p.path).parent().unwrap()).unwrap_or(Path::new(f)).to_string_lossy().into_owned());
                json!({"id":format!("{}:{}",p.path,i),"label":virtual_path,"kind":"file","path":f,"uri":uri(f),"compileIndex":i})
            }).collect();
            json!({"id":p.path,"label":p.name,"kind":"project","path":p.path,"uri":uri(&p.path),"children":children})
        }).collect();
        json!({"nodes":nodes,"projects":self.projects,"completeness":if self.exact{"exact"}else{"approximate"},"source":"project-model","diagnostics":self.diagnostics})
    }
}

#[cfg(test)]
mod membership_model_tests {
    use super::*;
    use std::fs;
    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "fsnav-model-membership-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    use std::time::UNIX_EPOCH;
    fn exported(root: &Path, dynamic: bool) -> Value {
        let project = root.join("P.fsproj");
        fs::write(&project, "<Project/>").unwrap();
        fs::write(root.join("A.fs"), "module A\nlet value=1\n").unwrap();
        let memberships = if dynamic {
            let mut certificate = SourceMembership {
                root: normalize(root),
                recursive: false,
                extensions: vec!["fs".into(), "fsi".into()],
                excluded_directories: vec![],
                sha256: String::new(),
                entries: 0,
            };
            (certificate.sha256, certificate.entries) = certificate.current().unwrap();
            vec![certificate]
        } else {
            vec![]
        };
        json!({"projects":[{"path":normalize(&project),"name":"P","files":[normalize(root.join("A.fs"))],"membershipReusable":true,"environmentCertificate":environment::empty_certificate(),"sourceMembership":memberships}],"completeness":"exact","inputs":[normalize(&project)],"inputHashes":[{"path":normalize(&project),"sha256":format!("{:x}",Sha256::digest(fs::read(&project).unwrap()))}]})
    }
    #[cfg(unix)]
    #[test]
    fn input_and_globalization_aliases_share_one_generation_without_hiding_conflicts() {
        let root = fixture();
        let mut value = exported(&root, false);
        let file = root.join("runtime");
        let alias = root.join("runtime-hardlink");
        let link = root.join("runtime-link");
        fs::write(&file, b"runtime bytes").unwrap();
        fs::hard_link(&file, &alias).unwrap();
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let directory = root.join("selection");
        fs::create_dir(&directory).unwrap();
        let sha = format!("{:x}", Sha256::digest(b"runtime bytes"));
        value["inputHashes"].as_array_mut().unwrap().extend([
            json!({"path":file,"sha256":sha}),
            json!({"path":alias,"sha256":sha}),
        ]);
        let profile = &mut value["projects"][0]["environmentCertificate"]["globalization"];
        profile["files"] = json!([{"path":file,"sha256":sha},{"path":alias,"sha256":sha},{"path":link,"sha256":sha}]);
        profile["selectionDirectories"] = json!([{"path":directory,"exists":true,"sha256":format!("{:x}",Sha256::digest([])),"entries":0}]);
        profile["pathLinks"] = json!([{"path":root,"target":null},{"path":link,"target":file}]);
        let model = Model::from_evaluated(&normalize(&root), &value).unwrap();
        let metrics = model.validation_metrics;
        assert_eq!(metrics.regular_file_reads, 2);
        assert_eq!(metrics.bytes_hashed, 10 + 13); // project XML + one runtime inode
        assert_eq!(metrics.requested_bytes, 10 + 5 * 13);
        assert_eq!(metrics.digest_cache_hits, 4);
        value["projects"][0]["environmentCertificate"]["globalization"]["files"][2]["sha256"] =
            json!("0".repeat(64));
        assert!(Model::from_evaluated(&normalize(&root), &value).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn explicit_inputs_do_not_gain_an_unrelated_directory_dependency() {
        let root = fixture();
        let value = exported(&root, false);
        fs::write(root.join("Unrelated.fs"), "module Unrelated").unwrap();
        let model = Model::from_evaluated(&normalize(&root), &value).unwrap();
        assert!(model.exact);
        assert_eq!(model.files.len(), 1);
        assert!(model
            .membership_roots()
            .iter()
            .all(|(path, _)| !Path::new(path).starts_with(&root)));
        assert!(!model.membership_affects(&root.join("Unrelated.fs")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unchanged_project_with_new_signature_invalidates_evaluated_membership() {
        let root = fixture();
        let value = exported(&root, true);
        let model = Model::from_evaluated(&normalize(&root), &value).unwrap();
        fs::write(root.join("A.fsi"), "module A\nval value:int").unwrap();
        assert!(model.validate_membership().is_err());
        assert!(Model::from_evaluated(&normalize(&root), &value).is_err());
        assert!(model.membership_affects(&root.join("A.fsi")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn indirect_compiler_inputs_cannot_certify_source_membership() {
        let root = fixture();
        let mut value = exported(&root, false);
        for argument in [
            "@extra.rsp",
            "\"@parameters.fs\"",
            "Included.fsscript",
            "--",
        ] {
            value["projects"][0]["compilerArgs"] = json!([argument]);
            assert!(Model::from_evaluated(&normalize(&root), &value)
                .unwrap_err()
                .to_string()
                .contains("unmodeled compiler input"));
        }
        value["projects"][0]["compilerArgs"] = json!(["-r:Reference.dll", "A.fs"]);
        assert!(Model::from_evaluated(&normalize(&root), &value).is_ok());
        let proj = root.join("P.fsproj");
        fs::write(&proj, "<Project><PropertyGroup><otherFLAGS>@extra.rsp</otherFLAGS></PropertyGroup><ItemGroup><Compile Include=\"A.fs\"/></ItemGroup></Project>").unwrap();
        assert!(!parse_project(&proj).unwrap().0.exact);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn old_or_explicitly_nonreusable_models_do_not_claim_exact_membership() {
        let root = fixture();
        let mut value = exported(&root, false);
        value["projects"][0]
            .as_object_mut()
            .unwrap()
            .remove("membershipReusable");
        assert!(Model::from_evaluated(&normalize(&root), &value)
            .unwrap_err()
            .to_string()
            .contains("source-membership"));
        value["projects"][0]["membershipReusable"] = json!(false);
        assert!(Model::from_evaluated(&normalize(&root), &value).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
