//! The setup health checks: the `Check` trait and every check that implements it

mod archives;
mod creation_club;
mod f4se;
mod ini_config;
mod loose_files;
mod loose_folders;
mod missing_masters;
mod plugin_count;
mod race_subgraphs;

use crate::context::GameContext;
use crate::finding::Finding;
use camino::Utf8Path;

use archives::Archives;
use creation_club::CreationClub;
use f4se::F4se;
use ini_config::IniConfig;
use loose_files::LooseFiles;
use loose_folders::LooseFolders;
use missing_masters::MissingMasters;
use plugin_count::PluginCount;
use race_subgraphs::RaceSubgraphs;

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
        Box::new(IniConfig),
        Box::new(Archives),
        Box::new(F4se),
    ]
}

/// True if `path`'s leading components match `prefix` case-insensitively, shared by loose-file/folder checks.
fn under(path: &Utf8Path, prefix: &[&str]) -> bool {
    let mut components = path.components();
    prefix.iter().all(|d| {
        components
            .next()
            .is_some_and(|c| c.as_str().eq_ignore_ascii_case(d))
    })
}
