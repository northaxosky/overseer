//! The game's INI configuration: archive invalidation and related setup

use super::Check;
use crate::context::GameContext;
use crate::finding::{Finding, Severity};

/// Reads the game INIs for archive invalidation and related problems
pub struct IniConfig;

impl Check for IniConfig {
    fn id(&self) -> &'static str {
        "ini-config"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        // No INIs could be read, nothing to say
        let Some(inis) = &ctx.inis else {
            return Vec::new();
        };
        let settings = &inis.settings;

        // If the game reads INIs from its install folder, then the ones we parsed aren't real
        if settings.get("General", "bUseMyGamesDirectory") == Some("0") {
            return vec![Finding {
                check: self.id(),
                severity: Severity::Warning,
                title: "The game is set to ignore the My Games INIs".to_owned(),
                detail: Some(
                    "`bUseMyGamesDirectory=0` makes the game read INIs from its install folder"
                        .to_owned(),
                ),
            }];
        }

        let mut findings = Vec::new();
        let invalidate_on = settings.get("Archive", "bInvalidateOlderFiles") == Some("1");
        let dirs_final_empty = settings.get("Archive", "sResourceDataDirsFinal") == Some("");

        if invalidate_on && dirs_final_empty {
            findings.push(Finding {
                check: self.id(),
                severity: Severity::Info,
                title: "Archive invalidation is enabled".to_owned(),
                detail: None,
            });
        } else {
            if !invalidate_on {
                findings.push(Finding {
                    check: self.id(),
                    severity: Severity::Error,
                    title:
                        "Archive invalidation is off; loose files won't override archived content"
                            .to_owned(),
                    detail: Some(
                        "Set `[Archive] bInvalidateOlderFiles=1` in `Fallout4Custom.ini`"
                            .to_owned(),
                    ),
                });
            }
            if !dirs_final_empty {
                findings.push(Finding {
                    check: self.id(),
                    severity: Severity::Warning,
                    title: "`sResourceDataDirsFinal` is not empty".to_owned(),
                    detail: Some(
                        "Clear it with `[Archive] sResourceDataDirsFinal=` in `Fallout4Custom.ini`"
                            .to_owned(),
                    ),
                });
            }
        }

        // non-english language changes which `<plugin> - Voices_<lang>.ba2` archives load
        if let Some(lang) = settings.get("General", "sLanguage") {
            let lang = lang.to_lowercase();
            if lang != "en" {
                findings.push(Finding {
                    check: self.id(),
                    severity: Severity::Info,
                    title: format!("Game language is `{lang}` (expects `Voices_{lang}` archives)"),
                    detail: None,
                });
            }
        }

        findings
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
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
        // The default context has `inis: None`.
        assert!(IniConfig.run(&GameContext::default()).is_empty());
    }

    #[test]
    fn correct_invalidation_is_a_single_info() {
        let findings = IniConfig.run(&ctx(
            "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        ));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].title.contains("enabled"));
    }

    #[test]
    fn invalidation_off_is_an_error() {
        let findings = IniConfig.run(&ctx(
            "[Archive]\nbInvalidateOlderFiles=0\nsResourceDataDirsFinal=\n",
        ));
        assert!(findings.iter().any(|f| f.severity == Severity::Error));
    }

    #[test]
    fn absent_invalidation_keys_flag_both() {
        // Empty INIs: invalidation absent (Error) and DataDirsFinal not explicitly empty (Warning).
        let sev = severities(&IniConfig.run(&ctx("")));
        assert!(sev.contains(&Severity::Error));
        assert!(sev.contains(&Severity::Warning));
    }

    #[test]
    fn nonempty_resource_dirs_final_warns() {
        let findings = IniConfig.run(&ctx(
            "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=STRINGS\\\n",
        ));
        // Invalidation on, but DataDirsFinal not empty: one Warning, no Info/Error.
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("sResourceDataDirsFinal"));
    }

    #[test]
    fn use_my_games_directory_zero_gates_everything() {
        // Even with broken invalidation present, the gate is the only finding.
        let findings = IniConfig.run(&ctx(
            "[General]\nbUseMyGamesDirectory=0\n[Archive]\nbInvalidateOlderFiles=0\n",
        ));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("My Games"));
    }

    #[test]
    fn a_non_english_language_adds_an_info() {
        let findings = IniConfig.run(&ctx(
            "[General]\nsLanguage=DE\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        ));
        // Case-insensitive: `DE` is non-English.
        assert!(
            findings
                .iter()
                .any(|f| f.severity == Severity::Info && f.title.contains("`de`"))
        );
    }

    #[test]
    fn english_language_adds_no_language_info() {
        let findings = IniConfig.run(&ctx(
            "[General]\nsLanguage=en\n[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
        ));
        // Only the invalidation Info; English needs no note.
        assert_eq!(findings.len(), 1);
        assert!(findings[0].title.contains("enabled"));
    }
}
