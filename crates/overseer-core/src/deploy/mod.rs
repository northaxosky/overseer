//! Mod Deployment: Turning an ordered set of mods into files visible in the game directory

mod deployer;
mod error;
mod hardlink;
mod plan;
mod progress;
mod record;
mod usvfs;

pub use deployer::Deployer;
pub use error::DeployError;
pub use hardlink::HardlinkDeployer;
pub use plan::{DeployPlan, ModSource, PlannedFile};
pub use progress::{NullSink, ProgressEvent, ProgressSink};
pub use record::{
    DeployEntry, DeployRecord, DeployerKind, ReversalReport, VerifyReport, deployer_for,
};
pub use usvfs::UsvfsDeployer;
