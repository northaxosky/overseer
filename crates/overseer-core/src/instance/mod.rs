//! A managed Overseer instance: installed mods, profiles, and the game it targets

mod error;
mod model;
mod profile;

pub use error::InstanceError;
pub use model::{Executable, InstalledMod, Instance, InstanceConfig};
pub use profile::{ModListEntry, Profile};
