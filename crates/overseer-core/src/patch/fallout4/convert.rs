//! The core-binary edition policy: flip Fallout 4's three binaries between OG, NG and AE.
//!
//! This is a thin policy over the shared [`super::engine`]: it maps each core binary to its target
//! fingerprint for a requested [`Generation`] and exposes the pieces the CLI wires into a `Policy`.

use super::engine::{ConvertItem, GroupSpec, Ownership, TargetSpec};
use super::fingerprint::{self, CORE_BINARIES, target_fingerprint, target_table_complete};
use crate::detect::Generation;
use crate::patch::fingerprint::FileFingerprint;

/// The single mandatory group: the three core binaries that must convert together
pub const CORE_GROUP: GroupSpec = GroupSpec {
    name: "core",
    ownership: Ownership::Mandatory,
    files: CORE_BINARIES,
};

/// The core policy's only group
pub static CORE_GROUPS: &[GroupSpec] = &[CORE_GROUP];

/// The target identity for `rel` at `target`, if this binary is known
pub fn core_target_spec(target: Generation, rel: &str) -> Option<TargetSpec> {
    target_fingerprint(target, rel).map(|fp| TargetSpec {
        rel_path: fp.rel_path,
        expected: fp.expected,
    })
}

/// Whether any known core fingerprint for `rel` has exactly `size` bytes
pub fn core_any_known_size(rel: &str, size: u64) -> bool {
    fingerprint::any_known_size(rel, size)
}

/// The edition label of the known core file matching `file`, if any
pub fn core_known_source(rel: &str, file: &FileFingerprint) -> Option<String> {
    fingerprint::known_source(rel, file).map(|fp| fp.label())
}

/// Build a single core convert item for `rel` at `target`, if the binary is known
pub fn explicit_item(target: Generation, rel: &str) -> Option<ConvertItem> {
    let spec = core_target_spec(target, rel)?;
    Some(ConvertItem {
        rel_path: spec.rel_path,
        target: spec,
        group: CORE_GROUP.name,
    })
}

/// Whether every core binary has a known target fingerprint for `target`
pub fn target_is_complete(target: Generation) -> bool {
    target_table_complete(target)
}

/// The three core binaries an edition swap converts together
pub fn core_binary_names() -> &'static [&'static str] {
    CORE_BINARIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_item_resolves_a_core_binary() {
        let item = explicit_item(Generation::OldGen, "Fallout4.exe").unwrap();
        assert_eq!(item.rel_path, "Fallout4.exe");
        assert_eq!(item.group, "core");
    }

    #[test]
    fn explicit_item_rejects_a_non_core_file() {
        assert!(explicit_item(Generation::OldGen, "Data/DLCCoast.esm").is_none());
    }

    #[test]
    fn core_group_holds_exactly_the_three_binaries() {
        assert_eq!(CORE_GROUP.files, CORE_BINARIES);
        assert_eq!(CORE_BINARIES.len(), 3);
    }

    #[test]
    fn target_completeness_tracks_known_editions() {
        assert!(target_is_complete(Generation::OldGen));
        assert!(target_is_complete(Generation::Anniversary));
        assert!(!target_is_complete(Generation::NextGen));
    }
}
