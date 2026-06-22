//! Errors from gathering context for diagnostics

use overseer_core::deploy::DeployError;
use overseer_core::instance::InstanceError;
use overseer_core::plugins::PluginError;
use thiserror::Error;

/// Something went wrong while gathering the game context
#[derive(Debug, Error)]
pub enum DiagnosticError {
    #[error(transparent)]
    Instance(#[from] InstanceError),

    #[error(transparent)]
    Plugin(#[from] PluginError),

    #[error(transparent)]
    Deploy(#[from] DeployError),
}
