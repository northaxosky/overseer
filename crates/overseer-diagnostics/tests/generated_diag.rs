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
