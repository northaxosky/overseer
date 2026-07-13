//! Tests for installed-mod lifecycle output

use super::*;

#[test]
fn residue_warning_names_bundle_and_future_refusal() {
    let path = Utf8Path::new(r"state\pending-mod-operation");

    assert_eq!(
        lifecycle_residue_warning(path),
        "warning: pending lifecycle bundle remains at `state\\pending-mod-operation`; later lifecycle commands will refuse until it is manually resolved"
    );
}
