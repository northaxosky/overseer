//! Mod Deployment: Turning an ordered set of mods into files visible in the game directory

mod deployer;
mod error;
mod hardlink;
mod layout;
mod plan;
mod progress;
mod record;

pub use deployer::{Deployer, DeployerKind, LaunchTarget, deployer_for};
pub use error::DeployError;
pub use hardlink::HardlinkDeployer;
pub use layout::{DATA_DIR, ROOT_DIR, strip_data_prefix};
pub use plan::{DeployPlan, ModSource, PlannedFile};
pub use progress::{NullSink, ProgressEvent, ProgressSink};
pub use record::{DeployEntry, DeployRecord, ReversalReport, VerifyReport};
