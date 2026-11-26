//! Work admitted implicitly by one indexed provider request. Ready index rows do
//! not spend the cold-read budget. Explicit indexing and explorer requests run
//! outside this scope. Like cancellation, scopes are private to a worker thread.
use anyhow::{ensure, Result};
use std::{cell::Cell, fmt, fs::File, io::Read, path::Path};

pub(crate) const MAX_ENTRIES: usize = 512;
pub(crate) const MAX_READS: usize = 128;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PARSES: usize = 64;
// A pinned SDK bootstrap validates tens of MiB of shared compiler/runtime
// inputs. Account actual uncached reads separately from the source budget.
const MAX_CERTIFICATE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_CERTIFICATE_VISITS: usize = 32_768;

#[derive(Clone, Copy, Default)]
struct Budget {
    entries: usize,
    reads: usize,
    bytes: u64,
    parses: usize,
    certificate_bytes: u64,
    certificate_visits: usize,
    exhausted: bool,
}
thread_local! { static CURRENT: Cell<Option<Budget>> = const { Cell::new(None) }; }

#[derive(Debug)]
pub(crate) struct Limit;
impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Indexed navigation reached its foreground work limit; background indexing may prepare additional source postings, but this request has no complete result")
    }
}
impl std::error::Error for Limit {}

pub(crate) struct Scope(Option<Budget>, std::marker::PhantomData<std::rc::Rc<()>>);
impl Scope {
    pub(crate) fn enter() -> Self {
        Self(
            CURRENT.replace(Some(Budget::default())),
            std::marker::PhantomData,
        )
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}

fn charge(update: impl FnOnce(&mut Budget)) -> Result<()> {
    if let Some(mut budget) = CURRENT.get() {
        update(&mut budget);
        budget.exhausted |= budget.entries > MAX_ENTRIES
            || budget.reads > MAX_READS
            || budget.bytes > MAX_BYTES
            || budget.parses > MAX_PARSES
            || budget.certificate_bytes > MAX_CERTIFICATE_BYTES
            || budget.certificate_visits > MAX_CERTIFICATE_VISITS;
        CURRENT.set(Some(budget));
        if budget.exhausted {
            return Err(Limit.into());
        }
    }
    Ok(())
}
pub(crate) fn is_active() -> bool {
    CURRENT.get().is_some()
}
pub(crate) fn check() -> Result<()> {
    charge(|_| {})
}
pub(crate) fn entries(count: usize) -> Result<()> {
    charge(|budget| budget.entries = budget.entries.saturating_add(count))
}
pub(crate) fn parse() -> Result<()> {
    charge(|budget| budget.parses += 1)
}
pub(crate) fn file(size: u64) -> Result<()> {
    charge(|budget| {
        budget.reads += 1;
        budget.bytes = budget.bytes.saturating_add(size);
    })
}
pub(crate) fn certificate_visit() -> Result<()> {
    charge(|budget| budget.certificate_visits += 1)
}
pub(crate) fn certificate_read(path: &Path, size: u64) -> Result<()> {
    if crate::project::is_source(path) {
        file(size)?;
    }
    charge(|budget| budget.certificate_bytes = budget.certificate_bytes.saturating_add(size))
}

pub(crate) fn read_text(path: &Path, max: u64) -> Result<String> {
    let file = open_regular(path)?;
    let size = file.metadata()?.len();
    self::file(size)?;
    ensure!(
        size <= max,
        "Source exceeds configured maxFileBytes: {}",
        path.display()
    );
    let mut text = String::new();
    file.take(size.saturating_add(1))
        .read_to_string(&mut text)?;
    ensure!(
        text.len() as u64 == size,
        "File size changed during read: {}",
        path.display()
    );
    Ok(text)
}

// Check both the path and the opened descriptor. O_NONBLOCK closes the race
// where a regular file is replaced by a FIFO between metadata and open.
pub(crate) fn open_regular(path: &Path) -> Result<File> {
    ensure!(
        path.metadata()?.is_file(),
        "Expected a regular file: {}",
        path.display()
    );
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "Expected a regular file: {}",
        path.display()
    );
    Ok(file)
}
