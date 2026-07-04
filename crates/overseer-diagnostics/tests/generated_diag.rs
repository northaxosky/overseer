//! Diagnostics over a generated instance: staged condition that produces the expected finding

use overseer_core::test_support::{self, FLAG_MASTER, TestbedSpec};
use overseer_diagnostics::{Severity, diagnose};

#[test]
fn missing_master_is_flagged_over_a_generated_instance() {
    let (_tmp, root) = test_support::temp();
    // Base.esm is provided and Good.esp masters it; Orphan.esp masters an .esm nothing ships.
    let spec = TestbedSpec::new()
        .managed("Base", true, |m| m.plugin("Base.esm", FLAG_MASTER, &[]))
        .managed("Good", true, |m| m.plugin("Good.esp", 0, &["Base.esm"]))
        .managed("Orphan", true, |m| {
            m.plugin("Orphan.esp", 0, &["AbsentMaster.esm"])
        });
    let instance = test_support::build_testbed(&root, &spec);

    let report = diagnose(&instance, "Default").expect("diagnose");

    // The orphan's absent master is flagged as an error; the satisfied master is not.
    assert!(
        report.findings.iter().any(|f| f.check == "missing-masters"
            && f.severity == Severity::Error
            && f.title.contains("Orphan.esp")
            && f.title.contains("AbsentMaster.esm")),
        "missing-masters should flag Orphan.esp: {:?}",
        report.findings
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.check == "missing-masters" && f.title.contains("Good.esp")),
        "a satisfied master must not be flagged"
    );
}

#[test]
fn header_version_and_archive_name_are_flagged_over_a_generated_instance() {
    let (_tmp, root) = test_support::temp();
    let spec = TestbedSpec::new()
        // A plugin whose HEDR is neither 0.95 nor 1.00.
        .managed("OldEsp", true, |m| {
            m.plugin_versioned("Old.esp", 0, &[], 0.94)
        })
        // A conventionally-named archive (valid) beside a misnamed one (flagged).
        .managed("GoodArchive", true, |m| {
            m.archive("GoodArchive - Main.ba2", 1, b"GNRL")
        })
        .managed("BadArchive", true, |m| {
            m.archive("RandomStuff.ba2", 1, b"GNRL")
        });
    let instance = test_support::build_testbed(&root, &spec);

    let report = diagnose(&instance, "Default").expect("diagnose");

    // header-versions flags the odd HEDR value.
    assert!(
        report.findings.iter().any(|f| f.check == "header-versions"
            && f.severity == Severity::Warning
            && f.title.contains("Old.esp")
            && f.title.contains("0.94")),
        "header-versions should flag Old.esp: {:?}",
        report.findings
    );
    // archive-names flags the misnamed archive but not the conventionally-named one.
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.check == "archive-names" && f.title.contains("RandomStuff.ba2")),
        "archive-names should flag RandomStuff.ba2: {:?}",
        report.findings
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.check == "archive-names" && f.title.contains("GoodArchive - Main.ba2")),
        "a conventionally-named archive must not be flagged"
    );
}

#[test]
fn a_source_format_loose_file_is_flagged_over_a_generated_instance() {
    let (_tmp, root) = test_support::temp();
    let spec = TestbedSpec::new().managed("Retex", true, |m| {
        m.loose("Textures/armor.dds", b"real asset")
            .loose("Textures/preview.png", b"source format")
    });
    let instance = test_support::build_testbed(&root, &spec);

    let report = diagnose(&instance, "Default").expect("diagnose");

    // A `.png` the game can't load is flagged; the real `.dds` asset beside it is not.
    assert!(
        report.findings.iter().any(|f| f.check == "loose-files"
            && f.severity == Severity::Warning
            && f.title.contains("preview.png")),
        "loose-files should flag preview.png: {:?}",
        report.findings
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.check == "loose-files" && f.title.contains("armor.dds")),
        "a real .dds asset must not be flagged"
    );
}

#[test]
fn a_base_script_override_is_flagged_over_a_generated_instance() {
    let (_tmp, root) = test_support::temp();
    // The F4SE package ships the most base scripts (dominant provider); BadMod, at higher
    // priority, wins one of those paths — so its Actor.pex reads as an override.
    let spec = TestbedSpec::new()
        .managed("BadMod", true, |m| {
            m.loose("Scripts/Actor.pex", b"override")
        })
        .managed("F4SE", true, |m| {
            m.loose("Scripts/Actor.pex", b"base")
                .loose("Scripts/Game.pex", b"base")
                .loose("Scripts/Form.pex", b"base")
        });
    let instance = test_support::build_testbed(&root, &spec);

    let report = diagnose(&instance, "Default").expect("diagnose");

    assert!(
        report.findings.iter().any(|f| f.check == "script-overrides"
            && f.severity == Severity::Warning
            && f.title.contains("Actor.pex")
            && f.title.contains("BadMod")),
        "script-overrides should flag BadMod's Actor.pex: {:?}",
        report.findings
    );
}
