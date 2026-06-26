//! Mod Deployment: Turning an ordered set of mods into files visible in the game directory

mod deployer;
mod error;
mod hardlink;
mod layout;
mod plan;
mod progress;
mod projfs;
mod record;
mod usvfs;

pub use deployer::{Deployer, LaunchTarget};
pub use error::DeployError;
pub use hardlink::HardlinkDeployer;
pub use layout::{DATA_DIR, ROOT_DIR, strip_data_prefix};
pub use plan::{DeployPlan, ModSource, PlannedFile};
pub use progress::{NullSink, ProgressEvent, ProgressSink};
pub use projfs::ProjFsDeployer;
pub use record::{
    DeployEntry, DeployRecord, DeployerKind, ReversalReport, VerifyReport, deployer_for,
};
pub use usvfs::UsvfsDeployer;
