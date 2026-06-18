//! Installing a mod from an archive into the instance's `mods/` directory

mod archive;
mod error;
mod root;

pub use archive::extract;
pub use error::InstallError;
pub use root::find_content_root;
