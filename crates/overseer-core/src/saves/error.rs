//! Errors from parsing a Fallout 4 `.fos` save header

use thiserror::Error;

/// Why a `.fos` save header could not be parsed
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum SaveParseError {
    /// The file did not begin with the `FO4_SAVEGAME` magic
    #[error("not a Fallout 4 save: bad magic")]
    BadMagic,
    /// The header ended before a field we needed was fully read
    #[error("save header ended unexpectedly")]
    UnexpectedEof,
    /// A header string was not valid UTF-8
    #[error("save header string was not valid UTF-8")]
    BadString,
    /// The declared header size was implausibly large
    #[error("save header size {0} is implausibly large")]
    HeaderTooLarge(usize),
}
