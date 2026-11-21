//! Work admitted implicitly by one indexed provider request. Ready index rows do
//! not spend the cold-read budget. Explicit indexing and explorer requests run
//! outside this scope. Like cancellation, scopes are private to a worker thread.
use anyhow::{ensure, Result};
use std::{cell::Cell, fmt, fs::File, io::Read, path::Path};

pub(crate) const MAX_ENTRIES: usize = 512;
pub(crate) const MAX_READS: usize = 128;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PARSES: usize = 64;

#[derive(Clone, Copy)]
struct Budget {
    entries: usize,
    reads: usize,
    bytes: u64,
    parses: usize,
    exhausted: bool,
}
thread_local! { static CURRENT: Cell<Option<Budget>> = const { Cell::new(None) }; }

#[derive(Debug)]
pub(crate) struct Limit;
impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Indexed navigation reached its foreground work limit; run F# Navigator: Index Workspace (workspace/index) to prepare repository postings and the configured semantic snapshot")
    }
}
impl std::error::Error for Limit {}

pub(crate) struct Scope(Option<Budget>, std::marker::PhantomData<std::rc::Rc<()>>);
impl Scope {
    pub(crate) fn enter() -> Self {
        Self(
            CURRENT.replace(Some(Budget {
                entries: 0,
                reads: 0,
                bytes: 0,
                parses: 0,
                exhausted: false,
            })),
            std::marker::PhantomData,
        )
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}

fn charge(entries: usize, reads: usize, bytes: u64, parses: usize) -> Result<()> {
    if let Some(mut budget) = CURRENT.get() {
        budget.entries = budget.entries.saturating_add(entries);
        budget.reads = budget.reads.saturating_add(reads);
        budget.bytes = budget.bytes.saturating_add(bytes);
        budget.parses = budget.parses.saturating_add(parses);
        budget.exhausted |= budget.entries > MAX_ENTRIES
            || budget.reads > MAX_READS
            || budget.bytes > MAX_BYTES
            || budget.parses > MAX_PARSES;
        CURRENT.set(Some(budget));
        if budget.exhausted {
            return Err(Limit.into());
        }
    }
    Ok(())
}
pub(crate) fn check() -> Result<()> {
    charge(0, 0, 0, 0)
}
pub(crate) fn entry() -> Result<()> {
    charge(1, 0, 0, 0)
}
pub(crate) fn parse() -> Result<()> {
    charge(0, 0, 0, 1)
}
pub(crate) fn file(size: u64) -> Result<()> {
    charge(0, 1, size, 0)
}

pub(crate) fn read_text(path: &Path, max: u64) -> Result<String> {
    let file = File::open(path)?;
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
