//! Filename-only provenance for evaluated dynamic Compile item domains.
//! Content edits never change these hashes; no source text or AST is read.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMembership {
    pub root: String,
    pub recursive: bool,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub excluded_directories: Vec<String>,
    pub sha256: String,
    pub entries: u64,
}

impl SourceMembership {
    fn exclusions(&self) -> Result<Vec<PathBuf>> {
        let mut result = Vec::new();
        for path in &self.excluded_directories {
            let path = Path::new(path);
            ensure!(
                !path.as_os_str().is_empty()
                    && !path.is_absolute()
                    && path.components().all(|p| matches!(p, Component::Normal(_))),
                "Membership exclusion must be a root-relative literal directory"
            );
            result.push(Path::new(&self.root).join(path));
        }
        Ok(result)
    }
    /// Sorted UTF-8 relative filenames, each followed by NUL. This certifies a
    /// conservative directory domain, not an invented MSBuild glob evaluator.
    pub fn current(&self) -> Result<(String, u64)> {
        crate::cancellation::check()?;
        let root = Path::new(&self.root);
        ensure!(root.is_absolute(), "Membership root must be absolute");
        let all_files = self.extensions.len() == 1 && self.extensions[0] == "*";
        ensure!(
            all_files
                || (!self.extensions.is_empty()
                    && self.extensions.iter().all(|e| matches!(
                        e.as_str(),
                        "fs" | "fsi" | "fsx" | "fsproj" | "props" | "targets"
                    ))),
            "Unsupported membership extension domain"
        );
        let exclusions = self.exclusions()?;
        match fs::symlink_metadata(root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((format!("{:x}", Sha256::digest([])), 0))
            }
            Err(error) => return Err(error).context("Read source membership root"),
            Ok(metadata) => {
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "Source membership directory links require fresh evaluated provenance: {}",
                    self.root
                );
                ensure!(
                    metadata.is_dir(),
                    "Source membership root is no longer a directory: {}",
                    self.root
                );
            }
        }
        let mut names = Vec::new();
        let mut name_bytes = 0usize;
        let mut visited = 0usize;
        let mut walker = WalkDir::new(root)
            .follow_links(false)
            .min_depth(1)
            .max_depth(if self.recursive { 256 } else { 1 })
            .into_iter();
        while let Some(entry) = walker.next() {
            crate::cancellation::check()?;
            let entry =
                entry.with_context(|| format!("Read source membership under {}", self.root))?;
            visited += 1;
            ensure!(
                visited <= 2_000_000,
                "Source membership enumeration exceeds structural budget"
            );
            if exclusions
                .iter()
                .any(|excluded| entry.path().starts_with(excluded))
            {
                if entry.file_type().is_dir() {
                    walker.skip_current_dir();
                }
                continue;
            }
            if entry.file_type().is_dir() {
                ensure!(
                    entry.depth() < 256,
                    "Source membership directory depth exceeds budget"
                );
                continue;
            }
            ensure!(
                !(entry.file_type().is_symlink() && entry.path().is_dir()),
                "Source membership directory links require fresh evaluated provenance: {}",
                entry.path().display()
            );
            if !entry.file_type().is_file() && !entry.file_type().is_symlink() {
                continue;
            }
            if !all_files
                && !entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        self.extensions
                            .iter()
                            .any(|e| e.eq_ignore_ascii_case(extension))
                    })
            {
                continue;
            }
            ensure!(
                !entry.file_type().is_symlink(),
                "Source membership file links require fresh evaluated provenance: {}",
                entry.path().display()
            );
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("Source membership contains a non-UTF-8 path")?
                .replace('\\', "/");
            name_bytes += relative.len();
            ensure!(
                names.len() < 250_000 && name_bytes <= 16 * 1024 * 1024,
                "Source membership filename inventory exceeds budget"
            );
            names.push(relative);
        }
        names.sort();
        let count = names.len() as u64;
        let mut hash = Sha256::new();
        for name in names {
            crate::cancellation::check()?;
            hash.update(name.as_bytes());
            hash.update([0]);
        }
        Ok((format!("{:x}", hash.finalize()), count))
    }
    pub fn validate(&self) -> Result<()> {
        let (hash, count) = self.current()?;
        ensure!(
            count == self.entries && hash.eq_ignore_ascii_case(&self.sha256),
            "Evaluated project source membership is stale for {}; prepare it again",
            self.root
        );
        Ok(())
    }
    /// Only call for create/remove/rename/rescan. Deleted directory shapes
    /// conservatively invalidate their domain; ordinary edits do not.
    pub fn affects(&self, path: &Path) -> bool {
        let root = Path::new(&self.root);
        if root.starts_with(path) {
            return true;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        if self
            .exclusions()
            .is_ok_and(|excluded| excluded.iter().any(|p| path.starts_with(p)))
        {
            return false;
        }
        if !self.recursive && relative.components().count() > 1 {
            return false;
        }
        if path.is_file() {
            if self.extensions.len() == 1 && self.extensions[0] == "*" {
                return true;
            }
            return path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|extension| {
                    self.extensions
                        .iter()
                        .any(|e| e.eq_ignore_ascii_case(extension))
                });
        }
        self.recursive || relative.components().count() <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "fsnav-membership-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    fn certificate(root: &Path, recursive: bool) -> SourceMembership {
        let mut value = SourceMembership {
            root: root.to_string_lossy().into_owned(),
            recursive,
            extensions: vec!["fs".into(), "fsi".into(), "fsx".into()],
            excluded_directories: vec![],
            sha256: String::new(),
            entries: 0,
        };
        let (hash, count) = value.current().unwrap();
        value.sha256 = hash;
        value.entries = count;
        value
    }
    #[test]
    fn names_invalidate_on_add_remove_but_not_content_edits() {
        let root = fixture();
        fs::write(root.join("A.fs"), "let value=1").unwrap();
        let cert = certificate(&root, false);
        fs::write(root.join("A.fs"), "let changed=2").unwrap();
        cert.validate().unwrap();
        fs::write(root.join("B.fs"), "let answer=A.value").unwrap();
        assert!(cert.validate().is_err());
        fs::remove_file(root.join("B.fs")).unwrap();
        cert.validate().unwrap();
        fs::remove_file(root.join("A.fs")).unwrap();
        assert!(cert.validate().is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recursive_external_roots_and_explicit_exclusions_have_correct_boundaries() {
        let root = fixture();
        fs::create_dir_all(root.join("linked/nested")).unwrap();
        fs::create_dir_all(root.join("linked/excluded")).unwrap();
        fs::write(root.join("linked/A.fs"), "").unwrap();
        let mut cert = certificate(&root.join("linked"), true);
        cert.excluded_directories.push("excluded".into());
        let (hash, count) = cert.current().unwrap();
        cert.sha256 = hash;
        cert.entries = count;
        fs::write(root.join("linked/excluded/Unused.fs"), "").unwrap();
        cert.validate().unwrap();
        assert!(!cert.affects(&root.join("linked/excluded/Unused.fs")));
        fs::write(root.join("linked/nested/B.fsi"), "").unwrap();
        assert!(cert.validate().is_err());
        let shallow = certificate(&root.join("linked"), false);
        fs::write(root.join("linked/nested/C.fs"), "").unwrap();
        shallow.validate().unwrap();
        assert!(!shallow.affects(&root.join("linked/nested/C.fs")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn utf8_sort_order_and_missing_root_match_the_contract() {
        let root = fixture();
        let missing = root.join("later");
        let empty = certificate(&missing, true);
        empty.validate().unwrap();
        fs::create_dir_all(&missing).unwrap();
        let mut names = ["\u{e000}.fs", "😀.fs", "α.fs", "a.FS"];
        for name in names {
            fs::write(missing.join(name), "").unwrap();
        }
        let cert = certificate(&missing, true);
        assert_eq!(cert.entries, 4);
        names.sort();
        let mut hash = Sha256::new();
        for name in names {
            hash.update(name.as_bytes());
            hash.update([0]);
        }
        assert_eq!(cert.sha256, format!("{:x}", hash.finalize()));
        assert!(empty.validate().is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn directory_links_cannot_silently_hide_sources() {
        let root = fixture();
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/A.fs"), "").unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();
        let cert = SourceMembership {
            root: root.to_string_lossy().into_owned(),
            recursive: true,
            extensions: vec!["fs".into()],
            excluded_directories: vec![],
            sha256: String::new(),
            entries: 0,
        };
        assert!(cert
            .current()
            .unwrap_err()
            .to_string()
            .contains("directory links"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn wildcard_import_domains_include_xml_and_extensionless_files() {
        let root = fixture();
        let mut cert = certificate(&root, false);
        cert.extensions = vec!["*".into()];
        (cert.sha256, cert.entries) = cert.current().unwrap();
        for name in ["custom.xml", "extensionless"] {
            fs::write(root.join(name), "<Project/>").unwrap();
            assert!(cert.validate().is_err());
            assert!(cert.affects(&root.join(name)));
            fs::remove_file(root.join(name)).unwrap();
            cert.validate().unwrap();
        }
        cert.extensions.push("fs".into());
        assert!(
            cert.current().is_err(),
            "Wildcard cannot be mixed with extension domains"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
