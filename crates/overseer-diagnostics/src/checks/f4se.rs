//! F4SE health: the loader must match the game's runtime, and deployed F4SE plugins need AL

use crate::context::{AddressLibraryStatus, GameContext};
use crate::finding::Finding;
use overseer_core::detect::Generation;

/// Reports F4SE setup problems: a loader for the wrong runtime, or a missing Address Library
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let mut findings = Vec::new();

    // A loader for the wrong runtime fails to launch the game; only flag when both the game and loader families are known and disagree
    if let (Some(game), Some(loader)) = (ctx.runtime_family, ctx.loader_family)
        && game != loader
    {
        findings.push(
            Finding::error(format!("F4SE is for {loader:?} but the game is {game:?}"))
                .detail("F4SE won't launch; install the F4SE build for your game version"),
        );
    }

    if let AddressLibraryStatus::Missing { expected } = &ctx.address_library {
        findings.push(
            Finding::warning(format!("Address Library `{expected}` is missing"))
                .detail("F4SE plugins need it; install Address Library (Nexus mod 47327)"),
        );
    }

    // A deployed F4SE plugin that doesn't advertise the installed runtime won't load
    if let (Some(packed), Some(game)) = (ctx.runtime_packed, ctx.runtime_family) {
        for p in &ctx.f4se_plugins {
            let advertises = if p.plugin.supports_ngae {
                p.plugin.supports(packed) || p.plugin.version_independent_for(game)
            } else {
                p.plugin.supports_og && game == Generation::OldGen // OG-only plugins (Query, no Version)
            };
            if !advertises {
                findings.push(
                    Finding::warning(format!(
                        "`{}` (from `{}`) may not support {game:?}",
                        p.name, p.mod_name
                    ))
                    .detail("Update the plugin for your F4SE/runtime version"),
                );
            }
        }
    }
    for unreadable in &ctx.unreadable_f4se {
        findings.push(
            Finding::warning(format!(
                "`{}` (from `{}`) could not be read",
                unreadable.name, unreadable.mod_name
            ))
            .detail(unreadable.reason.clone()),
        );
    }

    if findings.is_empty() {
        findings.push(Finding::info("No F4SE problems found"))
    }
    findings
}

#[cfg(test)]
#[path = "tests/f4se.rs"]
mod tests;
