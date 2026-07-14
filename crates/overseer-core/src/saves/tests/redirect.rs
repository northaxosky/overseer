//! Tests for save-redirect handling

use super::*;
use crate::test_support::temp;
use camino::Utf8PathBuf;

/// A temp My Games dir plus the paths the deploy flow would compute from it
fn setup() -> (tempfile::TempDir, Utf8PathBuf, Utf8PathBuf) {
    let (tmp, my_games) = temp();
    let custom_ini = my_games.join("Fallout4Custom.ini");
    let saves_dir = my_games.join("Saves").join("Hardcore");
    (tmp, custom_ini, saves_dir)
}

#[test]
fn value_is_a_relative_per_profile_saves_path() {
    assert_eq!(save_redirect_value("Hardcore"), "Saves\\Hardcore\\");
}

#[test]
fn apply_into_a_fresh_install_writes_the_redirect_and_creates_the_folder() {
    let (_tmp, custom_ini, saves_dir) = setup();
    // No INI yet: the user never launched the game
    let original = apply_save_redirect(&custom_ini, &saves_dir, "Hardcore").expect("apply");

    assert_eq!(original, None, "nothing to back up");
    assert!(saves_dir.is_dir(), "saves folder pre-created");
    let written = std::fs::read_to_string(&custom_ini).unwrap();
    assert_eq!(
        Ini::parse(&written).get("General", "SLocalSavePath"),
        Some("Saves\\Hardcore\\")
    );
}

#[test]
fn apply_preserves_existing_settings_and_returns_the_prior_value() {
    let (_tmp, custom_ini, saves_dir) = setup();
    std::fs::write(
        &custom_ini,
        "[General]\r\nSLocalSavePath=Saves\\Old\\\r\n[Archive]\r\nbInvalidateOlderFiles=1\r\n",
    )
    .unwrap();

    let original = apply_save_redirect(&custom_ini, &saves_dir, "Hardcore").expect("apply");

    assert_eq!(
        original.as_deref(),
        Some("Saves\\Old\\"),
        "prior value captured"
    );
    let ini = Ini::parse(&std::fs::read_to_string(&custom_ini).unwrap());
    assert_eq!(
        ini.get("General", "SLocalSavePath"),
        Some("Saves\\Hardcore\\")
    );
    // The user's archive-invalidation block is untouched
    assert_eq!(ini.get("Archive", "bInvalidateOlderFiles"), Some("1"));
}

#[test]
fn restore_removes_the_key_when_the_user_had_none() {
    let (_tmp, custom_ini, saves_dir) = setup();
    std::fs::write(&custom_ini, "[General]\r\nuGridsToLoad=5\r\n").unwrap();

    let original = apply_save_redirect(&custom_ini, &saves_dir, "Hardcore").expect("apply");
    assert_eq!(original, None);

    let outcome =
        restore_save_redirect(&custom_ini, "Hardcore", original.as_deref()).expect("restore");
    assert_eq!(outcome, Restore::Restored);

    let ini = Ini::parse(&std::fs::read_to_string(&custom_ini).unwrap());
    assert_eq!(
        ini.get("General", "SLocalSavePath"),
        None,
        "our key removed"
    );
    assert_eq!(
        ini.get("General", "uGridsToLoad"),
        Some("5"),
        "other key kept"
    );
}

#[test]
fn restore_puts_back_the_users_original_value() {
    let (_tmp, custom_ini, saves_dir) = setup();
    std::fs::write(&custom_ini, "[General]\r\nSLocalSavePath=Saves\\Old\\\r\n").unwrap();

    let original = apply_save_redirect(&custom_ini, &saves_dir, "Hardcore").expect("apply");
    let outcome =
        restore_save_redirect(&custom_ini, "Hardcore", original.as_deref()).expect("restore");

    assert_eq!(outcome, Restore::Restored);
    assert_eq!(
        Ini::parse(&std::fs::read_to_string(&custom_ini).unwrap()).get("General", "SLocalSavePath"),
        Some("Saves\\Old\\"),
        "user's original value restored"
    );
}

#[test]
fn restore_leaves_a_value_the_user_changed_afterward() {
    let (_tmp, custom_ini, saves_dir) = setup();
    let original = apply_save_redirect(&custom_ini, &saves_dir, "Hardcore").expect("apply");

    // The user (or a tool) re-pointed the save path while we were deployed
    std::fs::write(
        &custom_ini,
        "[General]\r\nSLocalSavePath=Saves\\Manual\\\r\n",
    )
    .unwrap();

    let outcome =
        restore_save_redirect(&custom_ini, "Hardcore", original.as_deref()).expect("restore");
    assert_eq!(outcome, Restore::Conflict, "diverged value is left alone");
    assert_eq!(
        Ini::parse(&std::fs::read_to_string(&custom_ini).unwrap()).get("General", "SLocalSavePath"),
        Some("Saves\\Manual\\")
    );
}

#[test]
fn restore_is_resolved_when_current_already_equals_original() {
    let (_tmp, custom_ini, _saves_dir) = setup();
    std::fs::write(&custom_ini, "[General]\r\nSLocalSavePath=Saves\\Old\\\r\n").unwrap();

    let outcome =
        restore_save_redirect(&custom_ini, "Hardcore", Some("Saves\\Old\\")).expect("restore");

    assert_eq!(outcome, Restore::Restored);
    assert_eq!(
        Ini::parse(&std::fs::read_to_string(&custom_ini).unwrap()).get("General", "SLocalSavePath"),
        Some("Saves\\Old\\")
    );
}

#[test]
fn restore_is_a_noop_when_the_ini_is_gone() {
    let (_tmp, custom_ini, _saves_dir) = setup();
    // Never written; a clean restore should simply succeed
    let outcome = restore_save_redirect(&custom_ini, "Hardcore", None).expect("restore");
    assert_eq!(outcome, Restore::Restored);
}
