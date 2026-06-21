//! Installing a mod from an archive into the instance's `mods/` directory

mod archive;
mod error;
mod installer;
mod root;

pub use error::InstallError;
pub use installer::install;
