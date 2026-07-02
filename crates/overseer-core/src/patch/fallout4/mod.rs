//! Fallout 4 patching policy: BA2 archive editions ([`ba2`]) and, later, whole-install
//! OG↔NG conversion. Each submodule layers a Fallout 4 policy over `patch`'s generic mechanisms.

pub mod ba2;

pub use ba2::{Ba2Edition, PatchOutcome, plan, set_edition};
