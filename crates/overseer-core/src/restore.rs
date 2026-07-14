//! Shared content-aware restore helpers.

/// Whether a content-aware restore put the original back, or left a diverged file alone
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// The original was restored, or there was nothing to undo
    Restored,
    /// The live file no longer matched what we wrote, so it was left untouched
    Conflict,
}

pub(crate) fn restore_if_ours<T, C, E>(
    intended: Option<T>,
    original: T,
    read_current: impl FnOnce() -> Result<(T, Option<C>), E>,
    restore_original: impl FnOnce(Option<C>) -> Result<(), E>,
) -> Result<Restore, E>
where
    T: PartialEq,
{
    let (current, context) = read_current()?;
    if current == original {
        return Ok(Restore::Restored);
    }

    if intended.is_some_and(|intended| current == intended) {
        restore_original(context)?;
        Ok(Restore::Restored)
    } else {
        Ok(Restore::Conflict)
    }
}
