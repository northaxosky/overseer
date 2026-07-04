//! Installed DLC that isn't at the cross-storefront consistency revision

use super::Check;
use crate::context::{DlcGroupState, GameContext};
use crate::finding::{Finding, Severity};

/// Flags installed DLC whose files aren't at the consistency revision
pub struct DlcConsistency;

impl Check for DlcConsistency {
    fn id(&self) -> &'static str {
        "dlc-consistency"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        if ctx.dlc_consistency.is_empty() {
            return Vec::new();
        }
        let mut findings = Vec::new();
        for group in &ctx.dlc_consistency {
            if !group.off_revision.is_empty() {
                findings.push(off_revision_warn(group));
            }
            if !group.missing.is_empty() {
                findings.push(missing_warn(group));
            }
        }
        if findings.is_empty() {
            findings.push(Finding::new(
                Severity::Info,
                format!(
                    "DLC is at the cross-storefront consistency revision ({} group(s))",
                    ctx.dlc_consistency.len()
                ),
                None,
            ));
        }
        findings
    }
}

/// A warning that a DLC group has files off the consistency revision
fn off_revision_warn(group: &DlcGroupState) -> Finding {
    Finding::new(
        Severity::Warning,
        format!(
            "`{}` DLC isn't at the cross-storefront consistency revision ({} file(s) differ)",
            group.group,
            group.off_revision.len()
        ),
        Some(
            "Run `overseer patch dlc-consistency` to bring the DLC to the consistency revision."
                .to_owned(),
        ),
    )
}

/// A warning that an installed DLC group is missing required files
fn missing_warn(group: &DlcGroupState) -> Finding {
    Finding::new(
        Severity::Warning,
        format!("`{}` DLC is missing {} file(s) from a complete install", group.group, group.missing.len()),
        Some("Verify the game files through your storefront (or reinstall); the DLC install is incomplete".to_owned()),
    )
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(groups: Vec<DlcGroupState>) -> GameContext {
        GameContext {
            dlc_consistency: groups,
            ..GameContext::default()
        }
    }

    fn group(name: &'static str, off: &[&'static str], missing: &[&'static str]) -> DlcGroupState {
        DlcGroupState {
            group: name,
            off_revision: off.to_vec(),
            missing: missing.to_vec(),
        }
    }

    #[test]
    fn no_dlc_installed_is_silent() {
        assert!(DlcConsistency.run(&ctx(Vec::new())).is_empty());
    }

    #[test]
    fn all_consistent_reports_a_single_info() {
        let findings = DlcConsistency.run(&ctx(vec![
            group("DLCCoast", &[], &[]),
            group("DLCNukaWorld", &[], &[]),
        ]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].title.contains("consistency revision"));
    }

    #[test]
    fn an_off_revision_group_warns_and_names_it() {
        let findings = DlcConsistency.run(&ctx(vec![
            group("DLCCoast", &["Data/DLCCoast - Textures.ba2"], &[]),
            group("DLCNukaWorld", &[], &[]),
        ]));
        // Only the off-revision group warns; no clean-bill Info alongside a warning
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("DLCCoast"));
        assert!(
            findings[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("patch dlc-consistency")
        );
    }

    #[test]
    fn a_missing_file_group_warns_and_blocks_the_clean_info() {
        let findings = DlcConsistency.run(&ctx(vec![group(
            "DLCCoast",
            &[],
            &["Data/DLCCoast - Textures.ba2"],
        )]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("missing"));
        assert!(
            findings[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("Verify the game files")
        );
    }
}
