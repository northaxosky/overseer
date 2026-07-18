//! Core logic for Overseer => Fallout 4 Mod Manager
//!
//! This crate is intentionally UI agnostic: It pulls in no GUI or CLI
//! dependencies so the command line tool and the app can both drive it

pub mod apply;
pub mod archive;
pub mod ba2;
pub mod deploy;
pub mod detect;
mod error;
pub mod f4se;
mod fs;
pub mod game;
pub mod ini;
pub mod install;
pub mod instance;
pub mod launch;
pub mod lifecycle;
pub mod merge;
pub mod patch;
pub mod plugins;
pub mod restore;
pub mod saves;
mod separator;
pub mod settings;

pub use error::IoError;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
