//! The setup health checks: the `Check` trait and every check that implements it

mod creation_club;
mod loose_files;
mod loose_folders;
mod missing_masters;
mod plugin_count;
mod race_subgraphs;

use crate::context::GameContext;
use crate::finding::Finding;
use camino::Utf8Path;

pub use creation_club::CreationClub;
pub use loose_files::LooseFiles;
pub use loose_folders::LooseFolders;
pub use missing_masters::MissingMasters;
pub use plugin_count::PluginCount;
pub use race_subgraphs::RaceSubgraphs;

/// A single setup/health check: pure function of the gathered context
pub trait Check {
    /// A stable identifier, used to group output and select checks
    fn id(&self) -> &'static str;

    /// Inspect the context and report any findings
    fn run(&self, ctx: &GameContext) -> Vec<Finding>;
}

/// Every check that runs, in display order
pub fn all() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(PluginCount),
        Box::new(MissingMasters),
        Box::new(RaceSubgraphs),
        Box::new(LooseFiles),
        Box::new(LooseFolders),
        Box::new(CreationClub),
    ]
}

/// True if `path`'s leading components match `prefix`, compared case-insensitively.
/// Shared by the loose-file and loose-folder checks.
fn under(path: &Utf8Path, prefix: &[&str]) -> bool {
    let mut components = path.components();
    prefix.iter().all(|d| {
        components
            .next()
            .is_some_and(|c| c.as_str().eq_ignore_ascii_case(d))
    })
}
