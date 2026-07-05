//! The game's INI configuration: archive invalidation and related setup

use crate::context::{GameContext, IniStatus};
use crate::finding::Finding;

/// Reads the game INIs for archive invalidation and related problems
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let Some(inis) = &ctx.inis else {
        return match &ctx.ini_status {
            IniStatus::Unreadable(error) => {
                vec![Finding::warning("The game INIs could not be read").detail(error.clone())]
            }
            IniStatus::Missing | IniStatus::Present => Vec::new(),
        };
    };
    let settings = &inis.settings;

    // If the game reads INIs from its install folder, then the ones we parsed aren't real
    if settings.get("General", "bUseMyGamesDirectory") == Some("0") {
        return vec![
            Finding::warning("The game is set to ignore the My Games INIs").detail(
                "`bUseMyGamesDirectory=0` makes the game read INIs from its install folder",
            ),
        ];
    }

    let mut findings = Vec::new();
    let invalidate_on = settings.get("Archive", "bInvalidateOlderFiles") == Some("1");
    let dirs_final_empty = settings.get("Archive", "sResourceDataDirsFinal") == Some("");

    if invalidate_on && dirs_final_empty {
        findings.push(Finding::info("Archive invalidation is enabled"));
    } else {
        if !invalidate_on {
            findings.push(
                Finding::error(
                    "Archive invalidation is off; loose files won't override archived content",
                )
                .detail("Set `[Archive] bInvalidateOlderFiles=1` in `Fallout4Custom.ini`"),
            );
        }
        if !dirs_final_empty {
            findings.push(
                Finding::warning("`sResourceDataDirsFinal` is not empty").detail(
                    "Clear it with `[Archive] sResourceDataDirsFinal=` in `Fallout4Custom.ini`",
                ),
            );
        }
    }

    // non-english language changes which `<plugin> - Voices_<lang>.ba2` archives load
    if let Some(lang) = settings.get("General", "sLanguage") {
        let lang = lang.to_lowercase();
        if lang != "en" {
            findings.push(Finding::info(format!(
                "Game language is `{lang}` (expects `Voices_{lang}` archives)"
            )));
        }
    }

    // `sTestFile*` entries make FO4 drive load order from plugin timestamps instead; of `Plugins.txt`, which Overseer's deploy/purge doesn't manage
    if (1..=10).any(|n| {
        settings
            .get("General", &format!("sTestFile{n}"))
            .is_some_and(|v| !v.trim().is_empty())
    }) {
        findings.push(
            Finding::warning("`sTestFile` entries are set in the game INI").detail(
                "These force a plugin list by file timestamp, bypassing `Plugins.txt`; \
                 Overseer's deploy and purge won't manage that load order. Remove the \
                 `[General] sTestFile*` lines unless you set them deliberately.",
            ),
        );
    }

    findings
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::Severity;
    use overseer_core::ini::{GameInis, Ini};

    fn ctx(settings: &str) -> GameContext {
        GameContext {
            inis: Some(GameInis {
                settings: Ini::parse(settings),
                prefs: Ini::default(),
            }),
            ..GameContext::default()
        }
    }

    fn severities(findings: &[Finding]) -> Vec<Severity> {
        findings.iter().map(|f| f.severity).collect()
    }

    #[test]
    fn no_inis_is_silent() {
        // The default context has `inis: None`
        assert!(super::run(&GameContext::default()).is_empty());
    }

    #[test]
    fn unreadable_inis_warn() {
        let findings = super::run(&GameContext {
            ini_status: IniStatus::Unreadable("access denied".to_owned()),
            ..GameContext::default()
        });
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
    fn correct_invalidation_is_a_single_info() {
        let findings = super::run(&ctx(
            "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        ));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].title.contains("enabled"));
    }

    #[test]
    fn invalidation_off_is_an_error() {
        let findings = super::run(&ctx(
            "[Archive]\nbInvalidateOlderFiles=0\nsResourceDataDirsFinal=\n",
        ));
        assert!(findings.iter().any(|f| f.severity == Severity::Error));
    }

    #[test]
    fn absent_invalidation_keys_flag_both() {
        // Empty INIs: invalidation absent (Error) and DataDirsFinal not explicitly empty (Warning)
        let sev = severities(&super::run(&ctx("")));
        assert!(sev.contains(&Severity::Error));
        assert!(sev.contains(&Severity::Warning));
    }

    #[test]
    fn nonempty_resource_dirs_final_warns() {
        let findings = super::run(&ctx(
            "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=STRINGS\\\n",
        ));
        // Invalidation on, but DataDirsFinal not empty: one Warning, no Info/Error
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("sResourceDataDirsFinal"));
    }

    #[test]
    fn use_my_games_directory_zero_gates_everything() {
        // Even with broken invalidation present, the gate is the only finding
        let findings = super::run(&ctx(
            "[General]\nbUseMyGamesDirectory=0\n[Archive]\nbInvalidateOlderFiles=0\n",
        ));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("My Games"));
    }

    #[test]
    fn a_non_english_language_adds_an_info() {
        let findings = super::run(&ctx(
            "[General]\nsLanguage=DE\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        ));
        // Case-insensitive: `DE` is non-English
        assert!(
            findings
                .iter()
                .any(|f| f.severity == Severity::Info && f.title.contains("`de`"))
        );
    }

    #[test]
    fn english_language_adds_no_language_info() {
        let findings = super::run(&ctx(
            "[General]\nsLanguage=en\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        ));
        // Only the invalidation Info; English needs no note
        assert_eq!(findings.len(), 1);
        assert!(findings[0].title.contains("enabled"));
    }

    #[test]
    fn stestfile_entries_warn() {
        let findings = super::run(&ctx(
            "[General]\nsTestFile1=WIP.esp\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        ));
        assert!(
            findings
                .iter()
                .any(|f| f.severity == Severity::Warning && f.title.contains("sTestFile"))
        );
    }

    #[test]
    fn an_empty_stestfile_does_not_warn() {
        // A blank value isn't a valid test file, so it shouldn't trip the warning
        let findings = super::run(&ctx(
            "[General]\nsTestFile1=\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        ));
        assert!(!findings.iter().any(|f| f.title.contains("sTestFile")));
    }
}
