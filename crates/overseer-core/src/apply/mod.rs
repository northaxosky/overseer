//! Orchestration: turn a profile into a live on disk deployment, and reverse it

mod error;
mod ops;
mod state;

pub use error::ApplyError;
pub use ops::{DeploymentStatus, deploy_profile, purge, rename_mod, rename_profile, status};
pub use state::{Deployment, Status};

pub(crate) use ops::recover_if_needed_locked;
