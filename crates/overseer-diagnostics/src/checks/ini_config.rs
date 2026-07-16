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

    // `sTestFile*` entries make FO4 drive load order from plugin timestamps instead of `Plugins.txt`, which Overseer's deploy/purge doesn't manage
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

#[cfg(test)]
#[path = "tests/ini_config.rs"]
mod tests;
