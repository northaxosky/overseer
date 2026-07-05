//! Per-profile save games: redirecting the game's `SLocalSavePath` to a profile's folder.

mod error;
mod info;
mod redirect;

pub(crate) use error::SaveParseError;
pub use info::{SaveInfo, SaveMeta, delete_save, list_saves};
pub use redirect::{apply_save_redirect, restore_save_redirect, save_redirect_value};
