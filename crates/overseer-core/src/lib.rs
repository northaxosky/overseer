//! Core domain logic for Overseer, a Fallout 4 mod manager.
//!
//! This crate is intentionally UI-agnostic: it pulls in no GUI or CLI dependencies,
//! so the command-line tool and the (future) desktop app can both drive it, and the
//! logic can be unit-tested on any platform.

pub mod deploy;
