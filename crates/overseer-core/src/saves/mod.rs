//! Per-profile save games: redirecting the game's `SLocalSavePath` to a
//! profile's folder.

mod redirect;

pub use redirect::{SaveRestore, apply_save_redirect, restore_save_redirect, save_redirect_value};
