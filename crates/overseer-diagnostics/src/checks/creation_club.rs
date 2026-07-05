//! The game's Creation Club load-order manifest (`Fallout4.ccc`)

use crate::context::{CccStatus, GameContext};
use crate::finding::Finding;

/// Reports on the game's CC manifest
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let finding = match &ctx.ccc {
        CccStatus::NotApplicable => return Vec::new(),

        CccStatus::Missing { file } => {
            Finding::warning(format!("`{file}` is missing from the game folder"))
                .detail("The install may be incomplete; Creation Club content won't load in order")
        }

        CccStatus::Unreadable { file, error } => {
            Finding::warning(format!("`{file}` could not be read")).detail(error.clone())
        }

        CccStatus::Present { file, entries } => Finding::info(format!(
            "`{file}` lists {} Creation Club plugin{}",
            entries.len(),
            if entries.len() == 1 { "" } else { "s" }
        )),
    };
    vec![finding]
}

#[cfg(test)]
#[path = "tests/creation_club.rs"]
mod tests;
