//! Orchestration: turn a profile into a live on disk deployment, and reverse it

mod error;
mod lock;
mod ops;
mod state;

pub use error::ApplyError;
pub use ops::{DeploymentStatus, deploy_profile, purge, status};
pub use state::{Deployment, Status};
