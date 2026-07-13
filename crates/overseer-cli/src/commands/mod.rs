//! Command handlers, one module per command group.

use camino::{Utf8Path, Utf8PathBuf};

use crate::ui::{Role, styled};

pub mod conflicts;
pub mod deploy;
pub mod doctor;
pub mod downloads;
pub mod exe;
pub mod install;
pub mod instance;
pub mod launch;
pub mod merge;
pub mod mods;
pub mod patch;
pub mod plugins;
pub mod profile;
pub mod saves;

pub(super) fn warn_lifecycle_residue(residue: Option<Utf8PathBuf>) {
    if let Some(path) = residue {
        println!(
            "{}",
            styled(Role::Warning, lifecycle_residue_warning(&path))
        );
    }
}

fn lifecycle_residue_warning(path: &Utf8Path) -> String {
    format!(
        "warning: pending lifecycle bundle remains at `{path}`; later lifecycle commands will refuse until it is manually resolved"
    )
}

#[cfg(test)]
#[path = "tests/lifecycle.rs"]
mod tests;
