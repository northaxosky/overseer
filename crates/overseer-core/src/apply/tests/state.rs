//! Tests for the persisted deployment state

use super::*;
use crate::test_support::temp_instance;

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
    let (_tmp, instance) = temp_instance();
    crate::fs::ensure_dir(&instance.state_dir()).expect("state directory");
    std::fs::write(Deployment::path(&instance), json).expect("legacy journal");
    let deployment = Deployment::load(&instance).expect("an older journal still loads");
    assert_eq!(deployment.status, Status::Committed);
    assert!(deployment.committed.is_none());
    assert!(
        deployment.was_committed(),
        "legacy Committed status implies committed origin"
    );
    assert_eq!(deployment.profile, "Default");
    assert!(deployment.plugins_txt_backup.is_none());
    assert!(deployment.plugins_txt_intended.is_none());
    assert!(deployment.save_redirect.is_none());
    assert!(
        Deployment::load_baseline(&instance)
            .expect("optional baseline")
            .is_none(),
        "legacy journals select conservative capture"
    );
}
