//! Independent tests compiled against a byte-identical snapshot of validation.rs.
use std::{fs, path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};
use sha2::{Digest, Sha256};

mod cancellation {
    use super::*;
    pub static CHECKS: AtomicUsize = AtomicUsize::new(0);
    pub static CANCEL_AT: AtomicUsize = AtomicUsize::new(usize::MAX);
    pub fn check() -> anyhow::Result<()> {
        let count = CHECKS.fetch_add(1, Ordering::SeqCst) + 1;
        anyhow::ensure!(count < CANCEL_AT.load(Ordering::SeqCst), "review cancellation");
        Ok(())
    }
    pub fn reset() {
        CHECKS.store(0, Ordering::SeqCst);
        CANCEL_AT.store(usize::MAX, Ordering::SeqCst);
    }
}
#[path = "validation-under-review.rs"]
mod validation;

static SERIAL: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        cancellation::reset();
        let path = PathBuf::from(std::env::var("REVIEW_FIXTURE_ROOT").unwrap())
            .join(format!("case-{}", SERIAL.fetch_add(1, Ordering::SeqCst)));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture { fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); } }
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

#[test]
fn three_aliases_share_bytes_but_validate_each_expected_digest() {
    let f = Fixture::new();
    let first = f.file("direct", b"original bytes");
    let hard = f.0.join("hard"); fs::hard_link(&first, &hard).unwrap();
    let sym = f.0.join("sym"); std::os::unix::fs::symlink("direct", &sym).unwrap();
    let mut g = validation::Generation::default();
    for path in [&first, &hard, &sym] { g.verify_file(path, &digest(b"original bytes"), false).unwrap(); }
    assert_eq!(g.metrics().regular_file_reads, 1);
    assert_eq!(g.metrics().digest_cache_hits, 2);
    assert_eq!(g.metrics().bytes_hashed, 14);
    assert_eq!(g.metrics().requested_bytes, 42);
    assert!(g.verify_file(&hard, &digest(b"incorrect"), false).is_err());
    assert!(g.verify_file(&first, &digest(b"original bytes"), false).is_err(), "failed generation must not be reused");
    let mut fresh = validation::Generation::default();
    fresh.verify_file(&sym, &digest(b"original bytes"), false).unwrap();
    assert_eq!(fresh.metrics().regular_file_reads, 1, "no cross-generation trust");
}

#[test]
fn same_inode_size_and_restored_mtime_mutation_cannot_reuse_old_digest() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    let first = f.file("direct", b"first");
    let hard = f.0.join("hard"); fs::hard_link(&first, &hard).unwrap();
    let before = fs::metadata(&first).unwrap();
    let mut g = validation::Generation::default();
    g.verify_file(&first, &digest(b"first"), false).unwrap();
    fs::write(&hard, b"other").unwrap();
    fs::OpenOptions::new().write(true).open(&hard).unwrap()
        .set_times(fs::FileTimes::new().set_modified(before.modified().unwrap())).unwrap();
    let after = fs::metadata(&first).unwrap();
    assert_eq!(before.ino(), after.ino()); assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    assert_ne!((before.ctime(), before.ctime_nsec()), (after.ctime(), after.ctime_nsec()));
    // A changed already-observed inode invalidates this validation generation.
    assert!(g.verify_file(&hard, &digest(b"other"), false).is_err());
    assert_eq!(g.metrics().digest_cache_hits, 0);
    assert!(g.verify_file(&first, &digest(b"other"), false).is_err());
    let mut fresh = validation::Generation::default();
    fresh.verify_file(&hard, &digest(b"other"), false).unwrap();
    assert_eq!(fresh.metrics().regular_file_reads, 1);
    assert_eq!(fresh.metrics().bytes_hashed, 5);
}

#[test]
fn symlink_retarget_and_same_length_atomic_replacement_use_opened_identity() {
    let f = Fixture::new();
    let first = f.file("first", b"first");
    let other = f.file("other", b"other");
    let sym = f.0.join("sym"); std::os::unix::fs::symlink("first", &sym).unwrap();
    let hard = f.0.join("hard"); fs::hard_link(&first, &hard).unwrap();
    let mut g = validation::Generation::default();
    g.verify_file(&sym, &digest(b"first"), false).unwrap();
    fs::remove_file(&sym).unwrap(); std::os::unix::fs::symlink("other", &sym).unwrap();
    g.verify_file(&sym, &digest(b"other"), false).unwrap();
    let replacement = f.file("replacement", b"third"); fs::rename(replacement, &first).unwrap();
    g.verify_file(&first, &digest(b"third"), false).unwrap();
    g.verify_file(&other, &digest(b"other"), false).unwrap();
    // Unlinking the original path changes the old inode's ctime. This generation
    // must refuse that changed prior identity, even though the bytes match.
    assert!(g.verify_file(&hard, &digest(b"first"), false).is_err());
    assert_eq!(g.metrics().regular_file_reads, 3);
    assert_eq!(g.metrics().digest_cache_hits, 1);
    let mut fresh = validation::Generation::default();
    fresh.verify_file(&hard, &digest(b"first"), false).unwrap();
    assert_eq!(fresh.metrics().regular_file_reads, 1);
}

#[test]
fn cancellation_during_read_poisoned_generation_cannot_reuse_prior_success() {
    let f = Fixture::new();
    let first = f.file("first", b"first");
    let big_bytes = vec![b'x'; 131072];
    let big = f.file("big", &big_bytes);
    let mut g = validation::Generation::default();
    g.verify_file(&first, &digest(b"first"), false).unwrap();
    cancellation::CHECKS.store(0, Ordering::SeqCst);
    cancellation::CANCEL_AT.store(3, Ordering::SeqCst);
    assert!(g.verify_file(&big, &digest(&big_bytes), false).is_err());
    assert!(g.metrics().bytes_hashed >= 65536 && g.metrics().bytes_hashed < 131077);
    cancellation::reset();
    assert!(g.verify_file(&first, &digest(b"first"), false).is_err());
    let mut fresh = validation::Generation::default();
    fresh.verify_file(&big, &digest(&big_bytes), false).unwrap();
    assert_eq!(fresh.metrics().bytes_hashed, 131072);
}

#[test]
fn filesystem_kind_is_checked_even_when_other_file_digest_is_cached() {
    use std::ffi::CString;
    let f = Fixture::new();
    let first = f.file("first", b"first");
    let fifo = f.0.join("fifo");
    let path = CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let mut g = validation::Generation::default();
    g.verify_file(&first, &digest(b"first"), false).unwrap();
    assert!(g.verify_file(&fifo, &digest(b"first"), false).is_err());
    let mut fresh = validation::Generation::default();
    fresh.verify_file(&f.0.join("missing"), "missing", false).unwrap();
    fresh.verify_file(&f.0, "directory", false).unwrap();
    assert_eq!(fresh.metrics().bytes_hashed, 0);
}

#[test]
fn retained_descriptors_are_bounded_and_eviction_rehashes() {
    let f = Fixture::new();
    let count_fds = || fs::read_dir("/proc/self/fd").unwrap().count();
    let baseline = count_fds();
    let mut g = validation::Generation::default();
    let paths: Vec<_> = (0..150).map(|n| f.file(&format!("file-{n}"), b"bytes")).collect();
    for path in &paths { g.verify_file(path, &digest(b"bytes"), false).unwrap(); }
    assert!(count_fds() <= baseline + 128, "descriptor retention must remain bounded");
    assert_eq!(g.metrics().regular_file_reads, 150);
    g.verify_file(&paths[0], &digest(b"bytes"), false).unwrap();
    assert_eq!(g.metrics().regular_file_reads, 151, "evicted content must be rehashed");
    g.verify_file(&paths[149], &digest(b"bytes"), false).unwrap();
    assert_eq!(g.metrics().digest_cache_hits, 1);
    drop(g);
    assert_eq!(count_fds(), baseline, "generation end must release retained handles");
}
