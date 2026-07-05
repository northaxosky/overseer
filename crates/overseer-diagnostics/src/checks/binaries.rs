//! Verify the core game binaries all belong to the same generation as the installed game

use crate::binaries::{BinaryEdition, BinaryScan};
use crate::context::GameContext;
use crate::finding::Finding;
use overseer_core::detect::{Edition, Generation};

/// Flags launcher/Steam-API binaries left over from a partial or failed up/downgrade
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let Some(edition) = ctx.game_edition else {
        return Vec::new();
    };

    // No readable game exe on disk
    if edition == Edition::Undetermined {
        if ctx.binaries.iter().any(|b| b.present) {
            return vec![
                Finding::warning(
                    "Fallout4.exe is missing or unreadable; cannot verify the other game binaries",
                )
                .detail(
                    "Verify the game files through your storefront (or reinstall) so Overseer \
                     can read the game version.",
                ),
            ];
        }
        return Vec::new();
    }

    // A real exe we can't pin to a generation
    let Some(expected) = edition.generation() else {
        return vec![Finding::warning(
            "Game edition could not be determined; skipping binary consistency checks",
        )];
    };

    let mut findings: Vec<Finding> = ctx
        .binaries
        .iter()
        .filter_map(|b| inspect(b, expected))
        .collect();
    if findings.is_empty() {
        findings.push(Finding::info(format!(
            "Core game binaries are consistent with the {} install",
            expected.label()
        )));
    }
    findings
}

/// compare on binary against the expected generation, returning a warning if it doesn't fit
fn inspect(binary: &BinaryScan, expected: Generation) -> Option<Finding> {
    if !binary.present {
        return Some(
            Finding::warning(format!("`{}` is missing from the game folder", binary.name)).detail(
                "Verify the game files through your storefront (or reinstall) so every core binary is \
            present and matches your game version.",
            ),
        );
    }

    match binary.edition {
        Some(e) if generation_matches(expected, e) => None,
        Some(e) => Some(
            Finding::warning(format!(
                "`{}` looks {} but your game is {}",
                binary.name,
                e.label(),
                expected.label()
            ))
            .detail(
                "This usually means a partial or failed up/downgrade — a storefront update \
                 replaced one file out of step with the rest. Reinstall the matching version so \
                 the game and its binaries agree.",
            ),
        ),
        None => Some(
            Finding::warning(format!(
                "could not verify `{}` (unrecognized version)",
                binary.name
            ))
            .detail(
                "Its version isn't one Overseer recognises; a mismatched or tampered core binary \
                 can cause crashes and odd behaviour.",
            ),
        ),
    }
}

/// Whether a binary's generation fits the expected one (`NgAe` satisfies both NG & AE)
fn generation_matches(expected: Generation, edition: BinaryEdition) -> bool {
    match expected {
        Generation::OldGen => edition == BinaryEdition::OldGen,
        Generation::NextGen => matches!(edition, BinaryEdition::NextGen | BinaryEdition::NgAe),
        Generation::Anniversary => {
            matches!(edition, BinaryEdition::Anniversary | BinaryEdition::NgAe)
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::Severity;

    fn scan(name: &'static str, edition: Option<BinaryEdition>, present: bool) -> BinaryScan {
        BinaryScan {
            name,
            edition,
            present,
        }
    }

    fn run(edition: Option<Edition>, binaries: Vec<BinaryScan>) -> Vec<Finding> {
        super::run(&GameContext {
            game_edition: edition,
            binaries,
            ..GameContext::default()
        })
    }

    fn warnings(findings: &[Finding]) -> usize {
        findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count()
    }

    #[test]
    fn a_matching_install_reports_a_single_clean_info() {
        let findings = run(
            Some(Edition::OldGen),
            vec![
                scan("Fallout4Launcher.exe", Some(BinaryEdition::OldGen), true),
                scan("steam_api64.dll", Some(BinaryEdition::OldGen), true),
            ],
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].title.contains("Old-Gen"));
    }

    #[test]
    fn a_downgraded_game_expects_old_gen_binaries() {
        let findings = run(
            Some(Edition::Downgraded),
            vec![scan("steam_api64.dll", Some(BinaryEdition::OldGen), true)],
        );
        assert_eq!(warnings(&findings), 0);
        assert_eq!(findings[0].severity, Severity::Info);
    }

    #[test]
    fn a_mismatched_binary_warns_with_both_generations() {
        let findings = run(
            Some(Edition::OldGen),
            vec![
                scan("Fallout4Launcher.exe", Some(BinaryEdition::OldGen), true),
                scan("steam_api64.dll", Some(BinaryEdition::NextGen), true),
            ],
        );
        assert_eq!(warnings(&findings), 1);
        // No clean-bill Info once a warning is present
        assert!(findings.iter().all(|f| f.severity == Severity::Warning));
        let warn = &findings[0];
        assert!(warn.title.contains("steam_api64.dll"));
        assert!(warn.title.contains("Next-Gen"));
        assert!(warn.title.contains("Old-Gen"));
    }

    #[test]
    fn a_missing_binary_warns() {
        let findings = run(
            Some(Edition::OldGen),
            vec![
                scan("Fallout4Launcher.exe", Some(BinaryEdition::OldGen), true),
                scan("steam_api64.dll", None, false),
            ],
        );
        assert_eq!(warnings(&findings), 1);
        assert!(findings[0].title.contains("missing"));
        assert!(findings[0].title.contains("steam_api64.dll"));
    }

    #[test]
    fn a_present_but_unrecognised_binary_warns() {
        let findings = run(
            Some(Edition::NextGen),
            vec![scan("steam_api64.dll", None, true)],
        );
        assert_eq!(warnings(&findings), 1);
        assert!(findings[0].title.contains("could not verify"));
    }

    #[test]
    fn a_shared_ng_ae_binary_satisfies_next_gen() {
        let findings = run(
            Some(Edition::NextGen),
            vec![scan("steam_api64.dll", Some(BinaryEdition::NgAe), true)],
        );
        assert_eq!(warnings(&findings), 0);
    }

    #[test]
    fn a_shared_ng_ae_binary_satisfies_anniversary() {
        let findings = run(
            Some(Edition::Anniversary),
            vec![scan("steam_api64.dll", Some(BinaryEdition::NgAe), true)],
        );
        assert_eq!(warnings(&findings), 0);
    }

    #[test]
    fn an_unclassifiable_real_exe_emits_a_single_skip_warning() {
        for edition in [Edition::Obsolete, Edition::Unknown] {
            let findings = run(
                Some(edition),
                vec![scan("steam_api64.dll", Some(BinaryEdition::OldGen), true)],
            );
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Warning);
            assert!(findings[0].title.contains("skipping"));
        }
    }

    #[test]
    fn a_fresh_instance_with_no_binaries_present_is_silent() {
        // `Undetermined` + nothing on disk (a fresh/empty game folder) is not a problem
        let findings = run(
            Some(Edition::Undetermined),
            vec![
                scan("Fallout4Launcher.exe", None, false),
                scan("steam_api64.dll", None, false),
            ],
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn a_broken_install_with_a_present_binary_warns_once() {
        // `Undetermined` but the folder is populated: `Fallout4.exe` is missing/unreadable
        let findings = run(
            Some(Edition::Undetermined),
            vec![
                scan("Fallout4Launcher.exe", Some(BinaryEdition::OldGen), true),
                scan("steam_api64.dll", None, false),
            ],
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("Fallout4.exe"));
        assert!(findings[0].title.contains("cannot verify"));
    }
}
