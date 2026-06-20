//! Front-end support for Overseer's binaries (CLI, TUI, and later GUI)
//!
//! Backend-neutral concerns every front end needs but that `overseer-core` must
//! not own (it stays UI-agnostic and print-free): file logging now, the
//! role/style descriptor later.

pub mod logging;
