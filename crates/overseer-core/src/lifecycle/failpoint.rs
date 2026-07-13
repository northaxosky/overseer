//! Thread-local failure hooks for lifecycle unit tests

use std::cell::RefCell;

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::io_err;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Point {
    Rename,
    Save,
    Restore,
    Cleanup,
}

thread_local! {
    static FAILURES: RefCell<Vec<(Point, Utf8PathBuf)>> = const { RefCell::new(Vec::new()) };
}

pub(super) struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        FAILURES.with(|failures| failures.borrow_mut().clear());
    }
}

/// Fail each named operation and path once on the current test thread
pub(super) fn scoped(failures: impl IntoIterator<Item = (Point, Utf8PathBuf)>) -> Guard {
    FAILURES.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_empty(), "nested lifecycle failpoint scope");
        slot.extend(failures);
    });
    Guard
}

/// Return a synthetic I/O error when this operation and path is armed
pub(super) fn check(point: Point, path: &Utf8Path) -> Result<(), crate::IoError> {
    let failed = FAILURES.with(|failures| {
        let mut failures = failures.borrow_mut();
        failures
            .iter()
            .position(|failure| failure.0 == point && failure.1 == path)
            .map(|index| failures.remove(index))
            .is_some()
    });
    if failed {
        return Err(io_err(
            path,
            std::io::Error::other(format!("{point:?} lifecycle test failpoint")),
        ));
    }
    Ok(())
}
