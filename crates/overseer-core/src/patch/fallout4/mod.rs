//! Fallout 4 patching policy layered over `patch`'s game-agnostic mechanisms: BA2 archive editions
//! ([`ba2`]), the Creation Club merge allow-list ([`cc`]), the core-binary edition flip
//! ([`convert`]), the DLC consistency revision ([`dlc`]), and the FO4 core-binary fingerprint table
//! ([`fingerprint`]).

pub mod ba2;
pub mod cc;
pub mod convert;
pub mod dlc;
pub mod fingerprint;

pub use ba2::{Ba2Edition, PatchOutcome, plan, set_edition};
