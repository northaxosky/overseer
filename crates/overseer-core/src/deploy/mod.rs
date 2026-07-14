//! Mod Deployment: Turning an ordered set of mods into files visible in the game directory

mod conflict;
mod deployer;
mod error;
mod hardlink;
mod layout;
mod plan;
mod progress;
mod record;

pub use conflict::{FileConflict, detect_conflicts};
pub use deployer::{Deployer, DeployerKind, LaunchTarget, TargetOwnership, deployer_for};
pub use error::DeployError;
pub use hardlink::HardlinkDeployer;
pub use layout::{BACKUP_DIR, DATA_DIR, ROOT_DIR, strip_data_prefix};
pub(crate) use plan::logical_path_key;
pub use plan::{DeployPlan, ModSource, PlannedFile};
pub use progress::{NullSink, ProgressEvent, ProgressSink};
pub use record::{
    DeployEntry, DeployRecord, PreservedConflict, ReversalIssue, ReversalReport, VerifyReport,
};
