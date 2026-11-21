//! Bounded digest reuse within one certificate-validation generation.
//! Every path is opened and checked; only stable regular-file contents are reused.
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::Read,
    path::Path,
};

const MAX_DIGESTS: usize = 128;

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub file_states_checked: u64,
    pub regular_file_reads: u64,
    pub requested_bytes: u64,
    pub bytes_hashed: u64,
    pub digest_cache_hits: u64,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::symlink,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "navigator-validation-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn hash(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    #[test]
    fn foreground_certificate_refuses_oversized_input_before_hashing() {
        let tmp = Temp::new();
        let input = tmp.file("selection.bin", b"");
        fs::OpenOptions::new()
            .write(true)
            .open(&input)
            .unwrap()
            .set_len(129 * 1024 * 1024)
            .unwrap();
        let scope = crate::foreground::Scope::enter();
        let mut generation = Generation::default();
        let error = generation.verify_file(&input, "unused", false).unwrap_err();
        assert!(error.is::<crate::foreground::Limit>());
        assert_eq!(generation.metrics().bytes_hashed, 0);
        drop(scope);
        fs::write(&input, b"admitted").unwrap();
        Generation::default()
            .verify_file(&input, &hash(b"admitted"), false)
            .unwrap();
    }

    #[test]
    fn aliases_reuse_actual_digest_but_compare_every_expectation() {
        let tmp = Temp::new();
        let a = tmp.file("a", b"input");
        let b = tmp.0.join("b");
        fs::hard_link(&a, &b).unwrap();
        let c = tmp.0.join("c");
        symlink(&a, &c).unwrap();
        let mut generation = Generation::default();
        for path in [&a, &b, &c] {
            generation
                .verify_file(path, &hash(b"input"), false)
                .unwrap();
        }
        let m = generation.metrics();
        assert_eq!(
            (m.bytes_hashed, m.requested_bytes, m.digest_cache_hits),
            (5, 15, 2)
        );
        assert!(generation.verify_file(&b, &hash(b"wrong"), false).is_err());
        assert!(generation.digests.is_empty() && generation.recent.is_empty());
        assert!(generation.verify_file(&a, &hash(b"input"), false).is_err());
        let mut retry = Generation::default();
        retry.verify_file(&a, &hash(b"input"), false).unwrap();
        assert_eq!(retry.metrics().bytes_hashed, 5);
    }

    #[test]
    fn same_inode_restored_mtime_and_atomic_replacement_do_not_reuse_old_contents() {
        let tmp = Temp::new();
        let a = tmp.file("a", b"first");
        let alias = tmp.0.join("alias");
        fs::hard_link(&a, &alias).unwrap();
        let time = fs::metadata(&a).unwrap().modified().unwrap();
        let mut generation = Generation::default();
        generation.verify_file(&a, &hash(b"first"), false).unwrap();
        fs::write(&a, b"later").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&a)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
        assert!(generation
            .verify_file(&alias, &hash(b"later"), false)
            .is_err());
        let mut generation = Generation::default();
        generation
            .verify_file(&alias, &hash(b"later"), false)
            .unwrap();
        assert_eq!(generation.metrics().bytes_hashed, 5);
        let replacement = tmp.file("replacement", b"third");
        fs::rename(replacement, &a).unwrap();
        generation.verify_file(&a, &hash(b"third"), false).unwrap();
        assert_eq!(generation.metrics().bytes_hashed, 10);
        // Unlinking the old name can change the retained inode's ctime. Do not
        // silently bless that changed identity even though its bytes match.
        assert!(generation
            .verify_file(&alias, &hash(b"later"), false)
            .is_err());
        Generation::default()
            .verify_file(&alias, &hash(b"later"), false)
            .unwrap();
    }

    #[test]
    fn retarget_cancellation_and_special_files_fail_without_trusting_partial_state() {
        let tmp = Temp::new();
        let a = tmp.file("a", b"first");
        let b = tmp.file("b", b"other");
        let link = tmp.0.join("link");
        symlink(&a, &link).unwrap();
        let mut generation = Generation::default();
        generation
            .verify_file(&link, &hash(b"first"), false)
            .unwrap();
        fs::remove_file(&link).unwrap();
        symlink(&b, &link).unwrap();
        assert!(generation
            .verify_file(&link, &hash(b"first"), false)
            .is_err());
        let mut generation = Generation::default();
        generation.verify_file(&a, &hash(b"first"), false).unwrap();
        let token = crate::cancellation::CancellationToken::new();
        token.cancel();
        {
            let _scope = token.enter();
            assert!(generation.verify_file(&a, &hash(b"first"), false).is_err());
        }
        assert!(generation.digests.is_empty());
        assert!(generation.verify_file(&a, &hash(b"first"), false).is_err());
        let fifo = tmp.0.join("fifo");
        let c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(Generation::default()
            .verify_file(&fifo, &hash(b""), false)
            .is_err());
    }

    #[test]
    fn bounded_eviction_rehashes_without_persisted_trust() {
        let tmp = Temp::new();
        let mut generation = Generation::default();
        for i in 0..=MAX_DIGESTS {
            let path = tmp.file(&i.to_string(), b"x");
            generation.verify_file(&path, &hash(b"x"), false).unwrap();
        }
        assert_eq!(generation.digests.len(), MAX_DIGESTS);
        assert_eq!(generation.recent.len(), MAX_DIGESTS);
        generation
            .verify_file(&tmp.0.join("0"), &hash(b"x"), false)
            .unwrap();
        assert_eq!(
            generation.metrics().regular_file_reads,
            MAX_DIGESTS as u64 + 2
        );
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Identity {
    device: u64,
    inode: u64,
    mode: u32,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

fn identity(metadata: &fs::Metadata) -> Option<Identity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(Identity {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn unchanged(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    match (identity(a), identity(b)) {
        (Some(a), Some(b)) => a == b,
        // No physical identity: never reuse a digest; still retain the existing
        // metadata stability checks around each complete read.
        _ => {
            a.is_file() == b.is_file()
                && a.len() == b.len()
                && a.modified()
                    .ok()
                    .zip(b.modified().ok())
                    .is_some_and(|(a, b)| a == b)
        }
    }
}

#[derive(Default)]
pub struct Generation {
    digests: HashMap<Identity, Entry>,
    recent: VecDeque<Identity>,
    failed: bool,
    metrics: Metrics,
}

struct Entry {
    digest: String,
    // Retain the inode for this generation, including after an unlink. A later
    // file cannot recycle an evicted inode while its digest remains reusable.
    file: fs::File,
}

impl Generation {
    pub fn metrics(&self) -> Metrics {
        self.metrics
    }

    pub fn ensure_active(&self) -> Result<()> {
        ensure!(!self.failed, "Certificate validation generation has failed");
        crate::cancellation::check()
    }

    /// Any validator failure ends this generation, including errors outside the
    /// file loop. Callers cannot reuse a partially successful validation pass.
    pub fn finish<T>(&mut self, result: Result<T>) -> Result<T> {
        let result = result.and_then(|value| {
            self.ensure_active()?;
            Ok(value)
        });
        if result.is_err() {
            self.digests.clear();
            self.recent.clear();
            self.failed = true;
        }
        result
    }

    pub fn verify_file(
        &mut self,
        path: &Path,
        expected: &str,
        digest_ignore_case: bool,
    ) -> Result<()> {
        let result = (|| {
            self.ensure_active()?;
            let actual = self.file_state(path)?;
            let matches = if digest_ignore_case
                && expected.len() == 64
                && expected.bytes().all(|b| b.is_ascii_hexdigit())
            {
                actual.eq_ignore_ascii_case(expected)
            } else {
                actual == expected
            };
            ensure!(
                matches,
                "Certificate file state changed: {}",
                path.display()
            );
            Ok(())
        })();
        self.finish(result)
    }

    fn file_state(&mut self, path: &Path) -> Result<String> {
        crate::foreground::certificate_visit()?;
        self.metrics.file_states_checked += 1;
        let path_before = match fs::metadata(path) {
            Ok(m) => m,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok("missing".into())
            }
            Err(e) => return Err(e).context("Inspect certificate input"),
        };
        if path_before.is_dir() {
            return Ok("directory".into());
        }
        ensure!(
            path_before.is_file(),
            "Unsupported certificate file kind: {}",
            path.display()
        );
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC);
        }
        let mut file = options
            .open(path)
            .with_context(|| format!("Open certificate input {}", path.display()))?;
        let before = file.metadata()?;
        ensure!(
            before.is_file() && unchanged(&path_before, &before),
            "Certificate input changed during open"
        );
        // The cache key comes from the opened descriptor, never a path stat.
        let key = identity(&before);
        if let Some(current) = &key {
            ensure!(
                self.digests.keys().all(|old| old.device != current.device
                    || old.inode != current.inode
                    || old == current),
                "Certificate input changed within validation generation: {}",
                path.display()
            );
        }
        self.metrics.requested_bytes += before.len();
        let cached = if let Some(entry) = key.as_ref().and_then(|key| self.digests.get(key)) {
            ensure!(
                unchanged(&before, &entry.file.metadata()?),
                "Cached certificate input changed"
            );
            Some(entry.digest.clone())
        } else {
            None
        };
        let digest = if let Some(digest) = cached {
            self.metrics.digest_cache_hits += 1;
            digest
        } else {
            crate::foreground::certificate_read(path, before.len())?;
            self.metrics.regular_file_reads += 1;
            let mut hash = Sha256::new();
            let mut input = (&mut file).take(before.len().saturating_add(1));
            let mut buffer = [0u8; 65536];
            loop {
                crate::cancellation::check()?;
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                self.metrics.bytes_hashed += n as u64;
                hash.update(&buffer[..n]);
            }
            format!("{:x}", hash.finalize())
        };
        crate::cancellation::check()?;
        let after = file.metadata()?;
        let path_after = fs::metadata(path)?;
        ensure!(
            after.is_file()
                && path_after.is_file()
                && unchanged(&before, &after)
                && unchanged(&after, &path_after),
            "Certificate input changed during validation: {}",
            path.display()
        );
        if let Some(key) = key {
            if let Some(position) = self.recent.iter().position(|item| item == &key) {
                self.recent.remove(position);
            } else if self.digests.len() >= MAX_DIGESTS {
                if let Some(oldest) = self.recent.pop_front() {
                    self.digests.remove(&oldest);
                }
            }
            self.recent.push_back(key.clone());
            self.digests.insert(
                key,
                Entry {
                    digest: digest.clone(),
                    file,
                },
            );
        }
        Ok(digest)
    }
}
