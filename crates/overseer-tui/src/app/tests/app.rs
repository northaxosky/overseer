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

#[test]
fn startup_marker_offers_a_dismissable_clear_confirmation() {
    let mut app = App::sample();
    let marker = overseer_core::launch::launch_marker_path(&app.session.instance);
    std::fs::create_dir_all(marker.parent().expect("marker parent")).expect("marker parent");
    std::fs::write(&marker, b"active").expect("marker");

    app.modal = stale_launch_modal(&app.session.instance).expect("startup marker query");

    assert!(matches!(
        app.modal,
        Some(Modal::Confirm(Confirm {
            action: ConfirmAction::ClearLaunchMarker,
            ..
        }))
    ));
    app.handle_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char('n'),
        ratatui::crossterm::event::KeyModifiers::NONE,
    ));
    assert!(app.modal.is_none());
    assert!(marker.exists(), "dismissal keeps the marker");
}
