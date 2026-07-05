//! Fallout 4 BA2 edition policy: which header version means which "generation", and patching
//! an archive between them via the generic [`crate::patch::set_version`].

use crate::archive::{Ba2Error, Ba2Header, Ba2Kind};
use crate::detect::Generation;
use crate::patch::{VersionChange, set_version};
use camino::Utf8Path;

/// The two Fallout 4 archive "generations", keyed on the header version field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ba2Edition {
    /// Old-Gen: version 1 (loads in every FO4 exe)
    OldGen,
    /// Next-Gen: version 7 or 8 (needs the NG/AE exe, or a backport)
    NextGen,
}

impl Ba2Edition {
    /// The FO4 edition a raw header version denotes
    pub fn from_version(version: u32) -> Option<Self> {
        match version {
            1 => Some(Self::OldGen),
            7 | 8 => Some(Self::NextGen),
            _ => None,
        }
    }

    /// The BA2 edition for a [`Generation`]; `Anniversary` uses Next-Gen archives, so it maps to `None`
    pub fn from_generation(generation: Generation) -> Option<Self> {
        match generation {
            Generation::OldGen => Some(Self::OldGen),
            Generation::NextGen => Some(Self::NextGen),
            Generation::Anniversary => None,
        }
    }

    /// The version number written when targeting this edition: OldGen ⇒ 1, NextGen ⇒ 8
    pub fn target_version(self) -> u32 {
        match self {
            Self::OldGen => 1,
            Self::NextGen => 8,
        }
    }
}

/// What [`set_edition`] did — or, via [`plan`], *would* do — to one archive
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOutcome {
    /// The version field was rewritten from `from` to `to`
    Patched { from: u32, to: u32 },
    /// Already the requested edition; nothing written
    AlreadyTarget { version: u32 },
    /// A readable BA2 we won't touch
    Unsupported { version: u32, kind: Ba2Kind },
}

/// Classify what patching `header` to `target` would do, touching nothing
pub fn plan(header: &Ba2Header, target: Ba2Edition) -> PatchOutcome {
    let kind_ok = matches!(header.kind, Ba2Kind::General | Ba2Kind::Texture);
    match Ba2Edition::from_version(header.version) {
        Some(current) if kind_ok => {
            if current == target {
                PatchOutcome::AlreadyTarget {
                    version: header.version,
                }
            } else {
                PatchOutcome::Patched {
                    from: header.version,
                    to: target.target_version(),
                }
            }
        }
        _ => PatchOutcome::Unsupported {
            version: header.version,
            kind: header.kind,
        },
    }
}

/// Patch the BA2 at `path` so it reads as `target`, in place
pub fn set_edition(path: &Utf8Path, target: Ba2Edition) -> Result<PatchOutcome, Ba2Error> {
    let header = Ba2Header::read(path)?;
    let to = match plan(&header, target) {
        PatchOutcome::Patched { to, .. } => to,
        other => return Ok(other), // AlreadyTarget or Unsupported: nothing to write
    };

    Ok(match set_version(path, to)? {
        VersionChange::Changed { from, to } => PatchOutcome::Patched { from, to },
        VersionChange::Unchanged { version } => PatchOutcome::AlreadyTarget { version },
    })
}

#[cfg(test)]
#[path = "tests/ba2.rs"]
mod tests;
