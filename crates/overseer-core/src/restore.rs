//! Shared content-aware restore helpers.

/// Whether a content-aware restore put the original back, or left a diverged file alone
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// The original was restored, or there was nothing to undo
    Restored,
    /// The live file no longer matched what we wrote, so it was left untouched
    Conflict,
}

pub(crate) enum MissingCurrent {
    Restored,
    Conflict,
}

pub(crate) fn restore_if_ours<T, C, E>(
    intended: Option<T>,
    read_current: impl FnOnce() -> Result<Option<(T, C)>, E>,
    restore_original: impl FnOnce(Option<C>) -> Result<(), E>,
    missing_current: MissingCurrent,
) -> Result<Restore, E>
where
    T: PartialEq,
{
    let Some(intended) = intended else {
        restore_original(None)?;
        return Ok(Restore::Restored);
    };

    match read_current()? {
        Some((current, context)) if current == intended => {
            restore_original(Some(context))?;
            Ok(Restore::Restored)
        }
        Some(_) => Ok(Restore::Conflict),
        None => match missing_current {
            MissingCurrent::Restored => Ok(Restore::Restored),
            MissingCurrent::Conflict => Ok(Restore::Conflict),
        },
    }
}
