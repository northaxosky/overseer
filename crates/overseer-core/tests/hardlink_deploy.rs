use std::fs;

use camino::Utf8PathBuf;
use overseer_core::deploy::{
    DeployPlan, DeployRecord, Deployer, DeployerKind, HardlinkDeployer, ModSource, NullSink,
};
use overseer_core::test_support::write;
use tempfile::tempdir;

#[test]
fn higher_priority_wins_files_are_hardlinks_and_purge_is_clean() {
    let tmp = tempdir().unwrap();
    let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();

    let mod_a = base.join("mods/AlphaTextures");
    let mod_b = base.join("mods/BetterTextures");
    let data = base.join("game/Data");

    // Both mods provide the same file (B should win); A also has a unique file.
    write(&mod_a.join("Textures/shared.dds"), "A-shared");
    write(&mod_a.join("Textures/only_a.dds"), "A-only");
    write(&mod_b.join("Textures/shared.dds"), "B-shared");

    let mods = [
        ModSource::new("AlphaTextures", mod_a),
        ModSource::new("BetterTextures", mod_b.clone()),
    ];

    let plan = DeployPlan::from_mods(&data, &mods).unwrap();
    assert_eq!(plan.len(), 2, "two distinct destination paths");

    let deployer = HardlinkDeployer::new();
    let record =
        DeployRecord::from_plan(&plan, base.join(".overseer-backup"), DeployerKind::HardLink)
            .unwrap();
    deployer.deploy(&record, &NullSink).unwrap();

    // Conflict resolution: the higher-priority mod B won the shared path.
    let shared = data.join("Textures/shared.dds");
    assert_eq!(fs::read_to_string(&shared).unwrap(), "B-shared");
    assert_eq!(
        fs::read_to_string(data.join("Textures/only_a.dds")).unwrap(),
        "A-only"
    );

    // Proof it's a hard link, not a copy: editing the staged source is reflected at
    // the deployed path, because they share the same underlying file data.
    fs::write(mod_b.join("Textures/shared.dds"), "B-edited").unwrap();
    assert_eq!(fs::read_to_string(&shared).unwrap(), "B-edited");

    assert!(deployer.verify(&record).is_ok());

    // Purge removes every deployed file and the directories we created.
    let report = deployer.undeploy(&record, &NullSink);
    assert!(report.is_fully_resolved());
    assert!(!shared.exists());
    assert!(!data.join("Textures/only_a.dds").exists());
    assert!(!data.join("Textures").exists(), "created dir was removed");

    // Staging is left untouched.
    assert!(mod_b.join("Textures/shared.dds").exists());
}
