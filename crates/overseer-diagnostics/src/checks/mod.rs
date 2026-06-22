//! The setup health checks: the `Check` trait and every check that implements it

mod creation_club;
mod loose_files;
mod missing_masters;
mod plugin_count;

use crate::context::GameContext;
use crate::finding::Finding;

pub use creation_club::CreationClub;
pub use loose_files::LooseFiles;
pub use missing_masters::MissingMasters;
pub use plugin_count::PluginCount;

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
        Box::new(LooseFiles),
        Box::new(CreationClub),
    ]
}
