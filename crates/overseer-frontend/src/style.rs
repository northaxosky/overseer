//! Styling roles shared by all of Overseer's front ends

/// A semantic styling role
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Section heading
    Heading,
    /// Completed action, good result, or enabled item
    Success,
    /// Error or failed check
    Failure,
    /// Caution: something missing or removed the user should notice
    Warning,
    /// Secondary info: counts, hints, disables...
    Muted,
    /// Deployed / added entry
    Added,
    /// Removed Entry
    Removed,
}
