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
        None if !binary.readable => Some(
            Finding::warning(format!(
                "`{}` is present but could not be read",
                binary.name
            ))
            .detail(
                "Overseer couldn't read the file to verify its version; close the game or its \
                 launcher if it's running, or check the file's permissions.",
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

#[cfg(test)]
#[path = "tests/binaries.rs"]
mod tests;
