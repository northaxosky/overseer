//! Installing a mod from an archive into the instance's `mods/` directory

mod archive;
mod downloads;
mod error;
mod installer;
mod root;

pub use downloads::{DownloadEntry, list_downloads};
pub use error::InstallError;
pub use installer::install;
