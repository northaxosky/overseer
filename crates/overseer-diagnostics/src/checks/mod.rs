//! The setup health checks: the `CheckSpec` registry and every check that implements it

mod archive_names;
mod archives;
mod binaries;
mod creation_club;
mod dlc_consistency;
mod f4se;
mod header_versions;
mod ini_config;
mod loose_files;
mod loose_folders;
mod missing_masters;
mod plugin_count;
mod plugins;
mod race_subgraphs;
mod script_overrides;

use crate::context::GameContext;
use crate::finding::Finding;
use camino::Utf8Path;

/// Check: a pure function of the gathered context
type CheckFn = fn(&GameContext) -> Vec<Finding>;

/// One registered check: its stable id and its function
pub struct CheckSpec {
    /// A stable identifier, used to group output and select checks
    pub id: &'static str,
    /// The check function itself
    pub run: CheckFn,
}

/// Every check that runs, in display order
pub const CHECKS: &[CheckSpec] = &[
    CheckSpec {
        id: "plugins",
        run: plugins::run,
    },
    CheckSpec {
        id: "plugin-count",
        run: plugin_count::run,
    },
    CheckSpec {
        id: "missing-masters",
        run: missing_masters::run,
    },
    CheckSpec {
        id: "race-subgraphs",
        run: race_subgraphs::run,
    },
    CheckSpec {
        id: "loose-files",
        run: loose_files::run,
    },
    CheckSpec {
        id: "loose-folders",
        run: loose_folders::run,
    },
    CheckSpec {
        id: "creation-club",
        run: creation_club::run,
    },
    CheckSpec {
        id: "ini-config",
        run: ini_config::run,
    },
    CheckSpec {
        id: "archives",
        run: archives::run,
    },
    CheckSpec {
        id: "f4se",
        run: f4se::run,
    },
    CheckSpec {
        id: "header-versions",
        run: header_versions::run,
    },
    CheckSpec {
        id: "archive-names",
        run: archive_names::run,
    },
    CheckSpec {
        id: "script-overrides",
        run: script_overrides::run,
    },
    CheckSpec {
        id: "binaries",
        run: binaries::run,
    },
    CheckSpec {
        id: "dlc-consistency",
        run: dlc_consistency::run,
    },
];

/// True if `path`'s leading components match `prefix` case-insensitively, shared by loose-file/folder checks
fn under(path: &Utf8Path, prefix: &[&str]) -> bool {
    let mut components = path.components();
    prefix.iter().all(|d| {
        components
            .next()
            .is_some_and(|c| c.as_str().eq_ignore_ascii_case(d))
    })
}

/// How a count sits against an engine limit: over it, within ~10%, or comfortably under
enum LimitTier {
    Over,
    Near,
    Under,
}

/// Classify `count` against `limit` for count-limit checks
fn limit_tier(count: usize, limit: usize) -> LimitTier {
    if count > limit {
        LimitTier::Over
    } else if count >= limit * 9 / 10 {
        LimitTier::Near
    } else {
        LimitTier::Under
    }
}

#[cfg(test)]
#[path = "tests/checks.rs"]
mod tests;
