//! Tests for the persisted deployment state

use super::*;

/// A deployment.json from an older Overseer (before per-profile saves and Plugins.txt intent) still deserializes
#[test]
fn a_journal_without_the_newer_optional_fields_still_loads() {
    let json = r#"{
        "status": "Committed",
        "profile": "Default",
        "record": {
            "deployer": "HardLink",
            "target_root": "C:/Game",
            "backup_root": "C:/Game/.overseer-backup",
            "entries": [],
            "created_dirs": []
        }
    }"#;
    let deployment: Deployment =
        serde_json::from_str(json).expect("an older journal still deserializes");
    assert_eq!(deployment.status, Status::Committed);
    assert_eq!(deployment.profile, "Default");
    assert!(deployment.plugins_txt_backup.is_none());
    assert!(deployment.plugins_txt_intended.is_none());
    assert!(deployment.save_redirect.is_none());
}
