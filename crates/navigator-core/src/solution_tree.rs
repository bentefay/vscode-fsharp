//! Read solution layout only for an explicit tree request. The project model
//! remains the authority for the order and physical identity of compile items.
use super::{normalize, uri, Model, Project};
use anyhow::{bail, ensure, Context, Result};
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

const MAX_LAYOUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

pub(super) fn is_solution(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sln") || e.eq_ignore_ascii_case("slnx"))
}

fn basename(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

fn project_node(project: &Project, id: &str) -> Value {
    let directory = Path::new(&project.path).parent().unwrap_or(Path::new("."));
    let mut children = Vec::new();
    let mut seen = HashSet::new();
    let references = project
        .references
        .iter()
        .filter(|p| seen.insert(p.as_str()))
        .enumerate()
        .map(|(i, path)| {
            json!({"id":format!("{id}:ref:{i}"),"kind":"projectReference",
            "label":basename(path),"path":path,"uri":uri(path)})
        })
        .collect::<Vec<_>>();
    if !references.is_empty() {
        children.push(
            json!({"id":format!("{id}:references"),"kind":"projectReferences",
            "label":"Project References","children":references}),
        );
    }
    for (index, file) in project.files.iter().enumerate() {
        let default = Path::new(file)
            .strip_prefix(directory)
            .ok()
            .and_then(Path::to_str)
            .unwrap_or_else(|| basename(file));
        let path = project
            .items
            .get(index)
            .and_then(|item| item["virtualPath"].as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(default)
            .replace('\\', "/");
        let mut parts = path
            .splitn(MAX_DEPTH, '/')
            .filter(|s| !s.is_empty() && *s != ".")
            .enumerate()
            .peekable();
        let mut level = &mut children;
        while let Some((depth, part)) = parts.next() {
            let label = percent_encoding::percent_decode_str(part)
                .decode_utf8()
                .unwrap_or_else(|_| part.into());
            if parts.peek().is_none() {
                level.push(
                    json!({"id":format!("{id}:{index}"),"kind":"file","label":label,
                    "path":file,"uri":uri(file),"compileIndex":index}),
                );
                break;
            }
            // Ionide builds directory runs: an intervening file or folder
            // closes the preceding run even when a later virtual path repeats it.
            let same_run = level
                .last()
                .is_some_and(|last| last["kind"] == "folder" && last["label"] == label.as_ref());
            if !same_run {
                level.push(json!({"id":format!("{id}:dir:{index}:{depth}"),
                    "kind":"folder","label":label,"children":[]}));
            }
            level = level.last_mut().unwrap()["children"]
                .as_array_mut()
                .unwrap();
        }
    }
    json!({"id":id,"label":project.name,"kind":"project","path":project.path,
        "uri":uri(&project.path),"children":children})
}

// A prepared graph can contain multiple instances of the same project. Keep
// all their compile lists and preserve the engine's existing ambiguity guard.
fn project_instances(projects: &[&Project], id: &str) -> Value {
    if let [project] = projects {
        return project_node(project, id);
    }
    let children = projects
        .iter()
        .enumerate()
        .map(|(index, project)| {
            let mut node = project_node(project, &format!("{id}:instance:{index}"));
            let description = [&project.target_framework, &project.configuration]
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            node["label"] = if description.is_empty() {
                json!(format!("{} ({})", project.name, index + 1))
            } else {
                json!(format!("{} ({description})", project.name))
            };
            node
        })
        .collect::<Vec<_>>();
    json!({"id":id, "label":projects[0].name, "kind":"projectGroup", "children":children})
}

enum Item {
    Folder,
    Project(String),
    File(String),
}

struct Row {
    key: String,
    parent: Option<String>,
    label: String,
    item: Item,
}

fn child_path(base: &Path, name: &str) -> String {
    normalize(base.join(name.replace('\\', "/")))
}

fn xml_attr(element: &BytesStart<'_>, name: &[u8]) -> Result<String> {
    for attribute in element.attributes() {
        let attribute = attribute?;
        if attribute.key.as_ref() == name {
            let s = attribute.unescape_value()?.into_owned();
            ensure!(!s.is_empty(), "Empty solution item attribute");
            return Ok(s);
        }
    }
    bail!("Missing {} attribute", String::from_utf8_lossy(name))
}

fn folder_rows(
    rows: &mut Vec<Row>,
    folders: &mut HashSet<String>,
    name: &str,
    parent: Option<&str>,
) -> Result<String> {
    let mut path = if name.starts_with('/') {
        String::new()
    } else {
        parent.unwrap_or("").to_owned()
    };
    for label in name.split('/').filter(|s| !s.is_empty()) {
        ensure!(
            path.matches('/').count() < MAX_DEPTH,
            "Solution folder nesting limit exceeded"
        );
        let parent = (!path.is_empty()).then(|| path.clone());
        path.push('/');
        path.push_str(label);
        if folders.insert(path.clone()) {
            rows.push(Row {
                key: path.clone(),
                parent,
                label: label.to_owned(),
                item: Item::Folder,
            });
        }
    }
    ensure!(!path.is_empty(), "Empty solution folder path");
    Ok(path)
}

fn slnx(source: &str, base: &Path) -> Result<Vec<Row>> {
    let mut reader = Reader::from_str(source);
    let mut rows = Vec::new();
    let mut folders = HashSet::new();
    // Some(parent key) for a solution/folder, None for unmodeled XML such
    // as solution configurations and per-project build/dependency metadata.
    let mut parents: Vec<Option<String>> = Vec::new();
    let mut saw_solution = false;
    loop {
        crate::cancellation::check()?;
        let event = reader.read_event()?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let mut container = None;
                if parents.is_empty() {
                    ensure!(
                        !saw_solution && element.local_name().as_ref() == b"Solution",
                        "Expected Solution root element"
                    );
                    saw_solution = true;
                    container = Some(String::new());
                } else if let Some(Some(key)) = parents.last() {
                    let parent = (!key.is_empty()).then(|| key.clone());
                    match element.local_name().as_ref() {
                        b"Folder" => {
                            container = Some(folder_rows(
                                &mut rows,
                                &mut folders,
                                &xml_attr(&element, b"Name")?,
                                parent.as_deref(),
                            )?);
                        }
                        kind @ (b"Project" | b"File") => {
                            let path = child_path(base, &xml_attr(&element, b"Path")?);
                            let label = basename(&path).to_owned();
                            let item = if kind == b"Project" {
                                Item::Project(path)
                            } else {
                                Item::File(path)
                            };
                            rows.push(Row {
                                key: format!("item:{}", rows.len()),
                                parent,
                                label,
                                item,
                            });
                        }
                        _ => {}
                    }
                }
                if !empty {
                    ensure!(
                        parents.len() < MAX_DEPTH,
                        "Solution XML nesting limit exceeded"
                    );
                    parents.push(container);
                }
            }
            Event::End(_) => {
                parents.pop().context("Unexpected closing element")?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(
        saw_solution && parents.is_empty(),
        "Incomplete solution XML"
    );
    Ok(rows)
}

fn quoted(source: &str) -> Result<Vec<String>> {
    let mut input = source.trim().chars().peekable();
    let mut values = Vec::new();
    loop {
        ensure!(input.next() == Some('"'), "Expected quoted solution field");
        let mut value = String::new();
        loop {
            let ch = input
                .next()
                .context("Unterminated solution project field")?;
            if ch == '"' {
                if input.next_if_eq(&'"').is_some() {
                    value.push('"');
                } else {
                    break;
                }
            } else {
                value.push(ch);
            }
        }
        values.push(value);
        while input.next_if(|c| c.is_whitespace()).is_some() {}
        if input.peek().is_none() {
            break;
        }
        ensure!(
            input.next() == Some(','),
            "Expected comma in solution project"
        );
        while input.next_if(|c| c.is_whitespace()).is_some() {}
    }
    ensure!(
        values.len() == 3 && input.peek().is_none(),
        "Expected three quoted solution project fields"
    );
    Ok(values)
}

fn sln(source: &str, base: &Path) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    let mut nested = HashMap::new();
    let mut project = None;
    let mut items = false;
    let mut relationships = false;
    ensure!(
        source
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with("Microsoft Visual Studio Solution File"),
        "Invalid solution file header"
    );
    for line in source.lines().map(str::trim) {
        crate::cancellation::check()?;
        if let Some(declaration) = line.strip_prefix("Project(") {
            let (kind, rest) = declaration
                .split_once(')')
                .context("Invalid solution project type")?;
            let values = quoted(
                rest.trim()
                    .strip_prefix('=')
                    .context("Missing solution project fields")?,
            )?;
            let key = values[2].to_ascii_uppercase();
            let item = match kind.trim_matches('"').to_ascii_uppercase().as_str() {
                "{2150E333-8FDC-42A3-9474-1A3956D46DE8}"
                | "{66A26720-8FB5-11D2-AA7E-00C04F688DDE}" => Item::Folder,
                _ => Item::Project(child_path(base, &values[1])),
            };
            project = Some(key.clone());
            rows.push(Row {
                key,
                parent: None,
                label: values[0].clone(),
                item,
            });
        } else if line == "EndProject" {
            project = None;
        } else if line.starts_with("ProjectSection(SolutionItems)") {
            items = true;
        } else if line == "EndProjectSection" {
            items = false;
        } else if line.starts_with("GlobalSection(NestedProjects)") {
            relationships = true;
        } else if line == "EndGlobalSection" {
            relationships = false;
        } else if items {
            let (name, file) = line
                .split_once(" = ")
                .context("Invalid solution file item")?;
            let path = child_path(base, file.trim());
            rows.push(Row {
                key: format!("file:{}", rows.len()),
                parent: Some(project.clone().context("Solution item outside a folder")?),
                label: basename(&name.replace('\\', "/")).to_owned(),
                item: Item::File(path),
            });
        } else if relationships {
            let (child, parent) = line.split_once('=').context("Invalid solution nesting")?;
            ensure!(
                nested
                    .insert(
                        child.trim().to_ascii_uppercase(),
                        parent.trim().to_ascii_uppercase()
                    )
                    .is_none(),
                "Duplicate solution parent"
            );
        }
    }
    for row in &mut rows {
        if let Some(parent) = nested.remove(&row.key) {
            row.parent = Some(parent);
        }
    }
    ensure!(nested.is_empty(), "Unknown nested solution item");
    Ok(rows)
}

struct Layout<'a> {
    rows: &'a [Row],
    children: HashMap<Option<&'a str>, Vec<usize>>,
    projects: &'a HashMap<&'a str, Vec<&'a Project>>,
    solution: &'a str,
    used: HashSet<String>,
    diagnostics: Vec<String>,
}

impl<'a> Layout<'a> {
    fn render(&mut self, parent: Option<&'a str>, depth: usize) -> Result<Vec<Value>> {
        ensure!(depth <= MAX_DEPTH, "Solution folder nesting limit exceeded");
        let mut result = Vec::new();
        // Solution documents precede child solution folders/projects in Ionide.
        let mut indices = self.children.remove(&parent).unwrap_or_default();
        indices.sort_by_key(|&i| !matches!(self.rows[i].item, Item::File(_)));
        for i in indices {
            let row = &self.rows[i];
            let id = format!("{}:{}", self.solution, row.key);
            let node = match &row.item {
                Item::Folder => {
                    json!({"id":id,"label":row.label,"kind":"folder","children":self.render(Some(&row.key),depth+1)?})
                }
                Item::File(path) => {
                    json!({"id":id,"label":row.label,"kind":"file","path":path,"uri":uri(path)})
                }
                Item::Project(path) => {
                    if let Some(p) = self.projects.get(path.as_str()) {
                        self.used.insert(path.clone());
                        project_instances(p, &id)
                    } else {
                        self.diagnostics.push(format!(
                            "Solution project outside the loaded F# model: {path}"
                        ));
                        json!({"id":id,"label":format!("{} (not loaded)",basename(path)),"kind":"unloadedProject","path":path,"uri":uri(path)})
                    }
                }
            };
            result.push(node);
        }
        Ok(result)
    }
}

fn solution(
    path: &str,
    projects: &HashMap<&str, Vec<&Project>>,
) -> Result<(Value, HashSet<String>, Vec<String>)> {
    let metadata = fs::metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_LAYOUT_BYTES,
        "Expected a solution file of at most 4 MiB"
    );
    let source = crate::foreground::read_text(Path::new(path), MAX_LAYOUT_BYTES)?;
    let base = Path::new(path)
        .parent()
        .context("Missing solution directory")?;
    let rows = if path.to_ascii_lowercase().ends_with(".slnx") {
        slnx(&source, base)?
    } else {
        sln(&source, base)?
    };
    let mut keys = HashSet::new();
    let mut project_paths = HashSet::new();
    let mut children = HashMap::<Option<&str>, Vec<usize>>::new();
    for (i, row) in rows.iter().enumerate() {
        ensure!(keys.insert(&row.key), "Duplicate solution item identity");
        if let Item::Project(path) = &row.item {
            ensure!(
                project_paths.insert(path),
                "Duplicate solution project {path}"
            );
        }
        children.entry(row.parent.as_deref()).or_default().push(i);
    }
    let mut layout = Layout {
        rows: &rows,
        children,
        projects,
        solution: path,
        used: HashSet::new(),
        diagnostics: Vec::new(),
    };
    let children = layout.render(None, 0)?;
    ensure!(
        layout.children.is_empty(),
        "Solution contains unreachable, cyclic or unsupported folder relationships"
    );
    let node = json!({"id":path,"label":basename(path),"kind":"solution","path":path,"uri":uri(path),"children":children});
    Ok((node, layout.used, layout.diagnostics))
}

pub(super) fn tree(model: &Model) -> Value {
    let mut diagnostics = model.diagnostics.clone();
    if model.exact
        && model
            .projects
            .iter()
            .any(|p| p.environment_certificate.is_none())
    {
        // The native reader shows explicit XML items, while SDK targets and
        // imports can introduce additional compiler inputs at evaluation time.
        diagnostics.push(
            "Project XML preview: prepare an evaluated model for authoritative compile order"
                .into(),
        );
    }
    let mut solutions = model
        .inputs
        .iter()
        .filter(|p| is_solution(Path::new(p)))
        .cloned()
        .collect::<Vec<_>>();
    if solutions.is_empty() {
        // Prepared models may have been exported by project/directory instead
        // of solution. Look for root-level solution layout only on tree demand.
        match fs::read_dir(&model.root) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) if is_solution(&entry.path()) => {
                            solutions.push(normalize(entry.path()))
                        }
                        Ok(_) => {}
                        Err(e) => diagnostics.push(e.to_string()),
                    }
                }
            }
            Err(e) => diagnostics.push(e.to_string()),
        }
    }
    solutions.sort();
    solutions.dedup();
    let mut projects: HashMap<&str, Vec<&Project>> = HashMap::new();
    for project in &model.projects {
        projects.entry(&project.path).or_default().push(project);
    }
    let mut nodes = Vec::new();
    let mut represented = HashSet::new();
    for path in solutions {
        match solution(&path, &projects) {
            Ok((node, used, notes)) => {
                nodes.push(node);
                represented.extend(used);
                diagnostics.extend(notes);
            }
            Err(e) => diagnostics.push(format!("Could not read solution layout {path}: {e}")),
        }
    }
    for project in &model.projects {
        if represented.insert(project.path.clone()) {
            nodes.push(project_instances(
                &projects[project.path.as_str()],
                &project.path,
            ));
        }
    }
    json!({"nodes":nodes,"projects":model.projects,"completeness":if model.exact && diagnostics.len()==model.diagnostics.len(){"exact"}else{"approximate"},"source":"project-model","diagnostics":diagnostics})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "fsnav-tree-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(path.join("App")).unwrap();
            fs::write(
                path.join("App/P.fsproj"),
                r#"<Project><ItemGroup>
<Compile Include="../Shared.fs" Link="A/X/First.fs"/>
<Compile Include="Second.fs"><Link>A/Y/Duplicate.fs</Link></Compile>
<Compile Include="Third.fs" Link="A/X/Duplicate.fs" />
<Compile Include="Fourth.fs" Link="B/Fourth%20Name.fs" />
<Compile Include="Fifth.fs"><Link>A/Fifth.fs</Link></Compile>
<Compile Include="../Outside.fs"/>
</ItemGroup></Project>"#,
            )
            .unwrap();
            Self(path)
        }
        fn model(&self) -> Model {
            super::super::discover(self.0.to_str().unwrap()).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn labels(nodes: &Value) -> Vec<&str> {
        nodes
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["label"].as_str().unwrap())
            .collect()
    }
    fn leaves<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
        if node["kind"] == "file" {
            out.push(node);
        }
        for child in node["children"].as_array().into_iter().flatten() {
            leaves(child, out);
        }
    }

    #[test]
    fn ordered_virtual_folder_runs_keep_compiler_indices_and_physical_targets() {
        let fixture = Fixture::new();
        let model = fixture.model();
        let value = tree(&model);
        assert_eq!(value["completeness"], "approximate");
        let p = &value["nodes"][0];
        assert_eq!(p["kind"], "project");
        let nodes = &p["children"];
        assert_eq!(labels(nodes), ["A", "B", "A", "Outside.fs"]);
        assert_ne!(nodes[0]["id"], nodes[2]["id"]);
        assert_eq!(nodes[1]["children"][0]["label"], "Fourth Name.fs");
        assert_eq!(labels(&nodes[0]["children"]), ["X", "Y", "X"]);
        assert_ne!(nodes[0]["children"][0]["id"], nodes[0]["children"][2]["id"]);
        let mut files = Vec::new();
        leaves(p, &mut files);
        assert_eq!(files.len(), 6);
        for (i, file) in files.into_iter().enumerate() {
            assert_eq!(file["compileIndex"], i);
            assert_eq!(file["path"], model.projects[0].files[i]);
            assert_eq!(file["uri"], uri(&model.projects[0].files[i]));
        }
        assert_eq!(tree(&model)["nodes"], value["nodes"]);
    }

    #[test]
    fn duplicate_project_instances_remain_visible_without_reusing_tree_item_ids() {
        let fixture = Fixture::new();
        let mut model = fixture.model();
        let mut second = model.projects[0].clone();
        second.target_framework = "net10.0".into();
        model.projects[0].target_framework = "net9.0".into();
        model.projects.push(second);
        fs::write(
            fixture.0.join("Targets.slnx"),
            "<Solution><Project Path=\"App/P.fsproj\"/></Solution>",
        )
        .unwrap();
        let result = tree(&model);
        let groups = &result["nodes"][0]["children"][0];
        assert_eq!(groups["kind"], "projectGroup");
        let instances = &groups["children"];
        assert_eq!(labels(instances), ["P (net9.0)", "P (net10.0)"]);
        assert_ne!(
            instances[0]["children"][0]["id"],
            instances[1]["children"][0]["id"]
        );
        assert_eq!(instances[0]["path"], instances[1]["path"]);
    }

    #[test]
    fn both_solution_formats_keep_hierarchy_and_distinct_project_context_ids() {
        let fixture = Fixture::new();
        let xml = fixture.0.join("Xml.slnx");
        fs::write(
            &xml,
            r#"<Solution><Folder Name="/Src/Nested/"><Project Path="App/P.fsproj"/></Folder>
<Folder Name="/Src/"><File Path="notes.md"/></Folder><Project Path="Other.csproj"/></Solution>"#,
        )
        .unwrap();
        let classic = fixture.0.join("Legacy.sln");
        fs::write(
            &classic,
            r#"Microsoft Visual Studio Solution File, Format Version 12.00
Project("{F2A71F9B-5D33-465A-A702-920D77279786}") = "Application", "App\P.fsproj", "{00000000-0000-0000-0000-00000000000A}"
EndProject
Project("{2150E333-8FDC-42A3-9474-1A3956D46DE8}") = "Src", "Src", "{00000000-0000-0000-0000-00000000000B}"
  ProjectSection(SolutionItems) = preProject
    notes.md = notes.md
  EndProjectSection
EndProject
Project("{2150E333-8FDC-42A3-9474-1A3956D46DE8}") = "Nested", "Nested", "{00000000-0000-0000-0000-00000000000C}"
EndProject
Global
  GlobalSection(NestedProjects) = preSolution
    {00000000-0000-0000-0000-00000000000a} = {00000000-0000-0000-0000-00000000000c}
    {00000000-0000-0000-0000-00000000000C} = {00000000-0000-0000-0000-00000000000B}
  EndGlobalSection
EndGlobal"#,
        )
        .unwrap();
        let model = fixture.model();
        let value = tree(&model);
        assert_eq!(labels(&value["nodes"]), ["Legacy.sln", "Xml.slnx"]);
        let roots = value["nodes"].as_array().unwrap();
        let mut ids = HashSet::new();
        for root in roots {
            assert_eq!(root["kind"], "solution");
            let src = &root["children"][0];
            assert_eq!(src["label"], "Src");
            assert_eq!(labels(&src["children"]), ["notes.md", "Nested"]);
            let p = &src["children"][1]["children"][0];
            assert_eq!(p["label"], "P");
            assert!(ids.insert(p["id"].as_str().unwrap()));
            assert_eq!(p["path"], fixture.0.join("App/P.fsproj").to_str().unwrap());
            assert_eq!(labels(&p["children"]), ["A", "B", "A", "Outside.fs"]);
        }
        assert_eq!(roots[1]["children"][1]["kind"], "unloadedProject");
        assert_eq!(value["completeness"], "approximate");
        assert!(value["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|note| note.as_str().unwrap().contains("Other.csproj")));

        // An explicitly selected solution is the only solution view.
        let mut selected = model;
        selected.inputs = vec![normalize(&xml)];
        assert_eq!(labels(&tree(&selected)["nodes"]), ["Xml.slnx"]);
        fs::write(&xml, "<Solution><Folder").unwrap();
        let invalid = tree(&selected);
        assert_eq!(invalid["completeness"], "approximate");
        assert_eq!(labels(&invalid["nodes"]), ["P"]);
        assert!(invalid["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|note| note.as_str().unwrap().contains("Could not read solution")));
    }
}
