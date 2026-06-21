//! Core logic for Overseer => Fallout 4 Mod Manager
//!
//! This crate is intentionally UI agnostic: It pulls in no GUI or CLI
//! dependencies so the command line tool and the app can both drive it

pub mod apply;
pub mod deploy;
pub mod install;
pub mod instance;
pub mod plugins;
pub mod settings;
