//! Orchestration: turn a profile into a live on disk deployment, and reverse it

mod error;
mod lock;
mod ops;
mod outcome;
mod state;

pub use error::ApplyError;
pub use ops::{
    DeploymentStatus, deploy_profile, deploy_sources, purge, purge_forced, rename_mod,
    rename_profile, status,
};
pub use outcome::{CapturedPath, ReversalOutcome};
pub use state::{Deployment, Status};

pub(crate) use lock::InstanceLock;
