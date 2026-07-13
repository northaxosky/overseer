//! Installing a mod from an archive into the instance's `mods/` directory

mod archive;
mod downloads;
mod error;
mod installer;
mod root;

pub(crate) use archive::ArchiveFormat;
pub use downloads::{DownloadEntry, list_downloads};
pub use error::InstallError;
pub use installer::install;
pub(crate) use installer::prepare_candidate;
