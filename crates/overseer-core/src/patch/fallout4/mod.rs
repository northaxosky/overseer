//! Fallout 4 patching policy: BA2 archive editions ([`ba2`]), the shared crash-safe conversion
//! [`engine`], and the two policies layered over it — core edition flips ([`convert`]) and the DLC
//! consistency revision ([`dlc`]). Each submodule layers a Fallout 4 policy over `patch`'s
//! generic mechanisms.

pub mod ba2;
pub mod convert;
pub mod dlc;
pub mod engine;
pub mod fingerprint;
pub mod vcdiff;

pub use ba2::{Ba2Edition, PatchOutcome, plan, set_edition};
