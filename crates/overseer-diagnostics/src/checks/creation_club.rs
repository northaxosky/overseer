//! The game's Creation Club load-order manifest (`Fallout4.ccc`)

use super::Check;
use crate::context::{CccStatus, GameContext};
use crate::finding::{Finding, Severity};

/// Reports on the game's CC manifest
pub struct CreationClub;

impl Check for CreationClub {
    fn id(&self) -> &'static str {
        "creation-club"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let finding = match &ctx.ccc {
            CccStatus::NotApplicable => return Vec::new(),

            CccStatus::Missing { file } => Finding::new(
                Severity::Warning,
                format!("`{file}` is missing from the game folder"),
                Some(
                    "The install may be incomplete; Creation Club content won't load in order"
                        .to_owned(),
                ),
            ),

            CccStatus::Unreadable { file, error } => Finding::new(
                Severity::Warning,
                format!("`{file}` could not be read"),
                Some(error.clone()),
            ),

            CccStatus::Present { file, entries } => Finding::new(
                Severity::Info,
                format!(
                    "`{file}` lists {} Creation Club plugin{}",
                    entries.len(),
                    if entries.len() == 1 { "" } else { "s" }
                ),
                None,
            ),
        };
        vec![finding]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(ccc: CccStatus) -> GameContext {
        GameContext {
            ccc,
            ..GameContext::default()
        }
    }

    fn present(entries: &[&str]) -> CccStatus {
        CccStatus::Present {
            file: "Fallout4.ccc",
            entries: entries.iter().map(|e| (*e).to_owned()).collect(),
        }
    }

    #[test]
    fn a_game_without_a_manifest_is_silent() {
        assert!(CreationClub.run(&ctx(CccStatus::NotApplicable)).is_empty());
    }

    #[test]
    fn a_missing_manifest_warns() {
        let findings = CreationClub.run(&ctx(CccStatus::Missing {
            file: "Fallout4.ccc",
        }));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("Fallout4.ccc"));
        assert!(findings[0].title.contains("missing"));
    }

    #[test]
    fn an_unreadable_manifest_warns() {
        let findings = CreationClub.run(&ctx(CccStatus::Unreadable {
            file: "Fallout4.ccc",
            error: "access denied".to_owned(),
        }));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("could not be read"));
        assert!(
            findings[0]
                .detail
                .as_deref()
                .is_some_and(|d| d.contains("access denied"))
        );
    }

    #[test]
    fn a_present_manifest_reports_its_count() {
        let findings = CreationClub.run(&ctx(present(&["ccA.esl", "ccB.esl"])));
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].title.contains("2 Creation Club plugins"));
    }

    #[test]
    fn a_single_entry_is_singular() {
        let findings = CreationClub.run(&ctx(present(&["ccA.esl"])));
        assert!(
            findings[0].title.ends_with("Creation Club plugin"),
            "got: {}",
            findings[0].title
        );
    }

    #[test]
    fn an_empty_manifest_reports_zero() {
        let findings = CreationClub.run(&ctx(present(&[])));
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].title.contains("0 Creation Club plugins"));
    }
}
