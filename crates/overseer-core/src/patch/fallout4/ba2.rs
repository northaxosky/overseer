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
mod tests {
    use super::*;
    use crate::archive::{Ba2Header, Ba2Kind};
    use crate::test_support::{ba2_bytes, temp};
    use camino::{Utf8Path, Utf8PathBuf};

    /// Generation maps to a BA2 edition, with Anniversary deliberately mapping to None (AE reuses Next-Gen archives)
    #[test]
    fn generation_maps_to_edition_with_anniversary_excluded() {
        assert_eq!(
            Ba2Edition::from_generation(Generation::OldGen),
            Some(Ba2Edition::OldGen)
        );
        assert_eq!(
            Ba2Edition::from_generation(Generation::NextGen),
            Some(Ba2Edition::NextGen)
        );
        assert_eq!(Ba2Edition::from_generation(Generation::Anniversary), None);
    }

    fn header(version: u32, kind: Ba2Kind) -> Ba2Header {
        Ba2Header {
            version,
            kind,
            file_count: 0,
        }
    }

    fn write_ba2(root: &Utf8Path, version: u32, tag: &[u8; 4], body: &[u8]) -> Utf8PathBuf {
        let path = root.join("test.ba2");
        std::fs::write(&path, ba2_bytes(version, tag, body)).expect("write ba2");
        path
    }

    #[test]
    fn edition_mapping() {
        assert_eq!(Ba2Edition::from_version(1), Some(Ba2Edition::OldGen));
        assert_eq!(Ba2Edition::from_version(7), Some(Ba2Edition::NextGen));
        assert_eq!(Ba2Edition::from_version(8), Some(Ba2Edition::NextGen));
        for non_fo4 in [0u32, 2, 3, 9, 999] {
            assert_eq!(Ba2Edition::from_version(non_fo4), None);
        }
        assert_eq!(Ba2Edition::OldGen.target_version(), 1);
        assert_eq!(Ba2Edition::NextGen.target_version(), 8);
    }

    #[test]
    fn plan_downgrades_next_gen_of_either_kind() {
        assert_eq!(
            plan(&header(8, Ba2Kind::General), Ba2Edition::OldGen),
            PatchOutcome::Patched { from: 8, to: 1 }
        );
        assert_eq!(
            plan(&header(7, Ba2Kind::Texture), Ba2Edition::OldGen),
            PatchOutcome::Patched { from: 7, to: 1 }
        );
    }

    #[test]
    fn plan_upgrades_old_gen_to_v8() {
        assert_eq!(
            plan(&header(1, Ba2Kind::General), Ba2Edition::NextGen),
            PatchOutcome::Patched { from: 1, to: 8 }
        );
    }

    #[test]
    fn plan_leaves_v7_alone_when_targeting_next_gen() {
        // v7 is already Next-Gen — we never silently canonicalise it to v8
        assert_eq!(
            plan(&header(7, Ba2Kind::General), Ba2Edition::NextGen),
            PatchOutcome::AlreadyTarget { version: 7 }
        );
    }

    #[test]
    fn plan_reports_already_target() {
        assert_eq!(
            plan(&header(1, Ba2Kind::General), Ba2Edition::OldGen),
            PatchOutcome::AlreadyTarget { version: 1 }
        );
        assert_eq!(
            plan(&header(8, Ba2Kind::Texture), Ba2Edition::NextGen),
            PatchOutcome::AlreadyTarget { version: 8 }
        );
    }

    #[test]
    fn plan_skips_non_fo4_version_or_kind() {
        assert_eq!(
            plan(&header(2, Ba2Kind::General), Ba2Edition::OldGen),
            PatchOutcome::Unsupported {
                version: 2,
                kind: Ba2Kind::General
            }
        );
        let gnmf = Ba2Kind::Other(*b"GNMF");
        assert_eq!(
            plan(&header(1, gnmf), Ba2Edition::OldGen),
            PatchOutcome::Unsupported {
                version: 1,
                kind: gnmf
            }
        );
    }

    #[test]
    fn set_edition_downgrades_and_preserves_the_body() {
        let (_tmp, root) = temp();
        let path = write_ba2(&root, 8, b"GNRL", b"body that must be preserved");
        let original = std::fs::read(&path).unwrap();
        assert_eq!(
            set_edition(&path, Ba2Edition::OldGen).unwrap(),
            PatchOutcome::Patched { from: 8, to: 1 }
        );
        let patched = std::fs::read(&path).unwrap();
        assert_eq!(&patched[4..8], 1u32.to_le_bytes().as_slice());
        let mut restored = patched.clone();
        restored[4..8].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(restored, original);
    }

    #[test]
    fn set_edition_round_trip_is_byte_exact() {
        let (_tmp, root) = temp();
        let path = write_ba2(&root, 8, b"DX10", b"texture body");
        let original = std::fs::read(&path).unwrap();
        assert_eq!(
            set_edition(&path, Ba2Edition::OldGen).unwrap(),
            PatchOutcome::Patched { from: 8, to: 1 }
        );
        assert_eq!(
            set_edition(&path, Ba2Edition::NextGen).unwrap(),
            PatchOutcome::Patched { from: 1, to: 8 }
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn set_edition_no_ops_a_v7_targeting_next_gen() {
        let (_tmp, root) = temp();
        let path = write_ba2(&root, 7, b"GNRL", b"body");
        let original = std::fs::read(&path).unwrap();
        assert_eq!(
            set_edition(&path, Ba2Edition::NextGen).unwrap(),
            PatchOutcome::AlreadyTarget { version: 7 }
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn set_edition_skips_a_non_fo4_archive() {
        let (_tmp, root) = temp();
        let path = write_ba2(&root, 2, b"GNRL", b"starfield-ish");
        let original = std::fs::read(&path).unwrap();
        assert_eq!(
            set_edition(&path, Ba2Edition::OldGen).unwrap(),
            PatchOutcome::Unsupported {
                version: 2,
                kind: Ba2Kind::General
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
