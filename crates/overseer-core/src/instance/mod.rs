//! A managed Overseer instance: installed mods, profiles, and the game it targets

mod error;
mod model;
mod profile;

pub use error::InstanceError;
pub(crate) use model::validate_mod_name;
pub use model::{
    InstalledMod, Instance, InstanceConfig, InvalidUserToolId, ToolMutationError, UserTool,
    UserToolId, mint_tool_id,
};
pub use profile::{CommitOutcome, ModKind, ModListEntry, Profile};
