//! Installing a mod from an archive into the instance's `mods/` directory

mod error;
mod root;

pub use error::InstallError;
pub use root::find_content_root;
