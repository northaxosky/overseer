//! Tests for application state and update logic

use super::*;

#[test]
fn cycle_variant_wraps_in_both_directions() {
    assert_eq!(cycle_variant(Workspace::Plugins, 1), Workspace::Conflicts);
    assert_eq!(cycle_variant(Workspace::Saves, 1), Workspace::Plugins);
    assert_eq!(cycle_variant(Workspace::Plugins, -1), Workspace::Saves);
}

#[test]
fn session_load_without_a_profile_uses_the_configured_default() {
    use overseer_core::instance::Instance;
    use overseer_core::test_support::{save_profile, temp_instance};

    let (_tmp, mut scaffold) = temp_instance();
    scaffold.config.default_profile = "Survival".to_owned();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    save_profile(&instance, "Survival", &[]);

    // No requested profile resolves to config.default_profile, not a hardcoded "Default"
    let session = Session::load(&instance.root, None).expect("session");
    assert_eq!(session.profile.name, "Survival");
}
