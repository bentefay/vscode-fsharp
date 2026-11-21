//! Cooperative request cancellation. A thread-local scope bridges the native
//! parser callback without making cancellation global across workspaces/tests.
use std::{
    cell::RefCell,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

#[derive(Debug)]
pub struct Cancelled;
impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Request cancelled; no complete result is available")
    }
}
impl std::error::Error for Cancelled {}

thread_local! {
    static CURRENT: RefCell<Option<CancellationToken>> = const { RefCell::new(None) };
}

pub struct CancellationScope(
    Option<CancellationToken>,
    std::marker::PhantomData<std::rc::Rc<()>>,
);
impl Drop for CancellationScope {
    fn drop(&mut self) {
        CURRENT.with(|current| {
            current.replace(self.0.take());
        });
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> anyhow::Result<()> {
        if self.is_cancelled() {
            Err(Cancelled.into())
        } else {
            Ok(())
        }
    }
    pub fn enter(&self) -> CancellationScope {
        CancellationScope(
            CURRENT.with(|current| current.replace(Some(self.clone()))),
            std::marker::PhantomData,
        )
    }
}

pub fn is_cancelled() -> bool {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    })
}

pub fn check() -> anyhow::Result<()> {
    if is_cancelled() {
        Err(Cancelled.into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_scope_restores_thread_local_token_and_preserves_typed_error() {
        let outer = CancellationToken::new();
        let inner = CancellationToken::new();
        outer.cancel();
        assert!(!is_cancelled());
        {
            let _scope = outer.enter();
            assert!(is_cancelled());
            assert!(check().unwrap_err().is::<Cancelled>());
            {
                let _scope = inner.enter();
                assert!(!is_cancelled());
                inner.cancel();
                assert!(is_cancelled());
            }
            assert!(is_cancelled());
        }
        assert!(!is_cancelled());
    }
}
