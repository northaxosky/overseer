//! Tests for transactional installed-mod rename and recovery

use super::journal::{self, Journal, Operation, Phase, ProfileSnapshot, journal_path};
use super::rename::rename_no_replace;
use super::*;
use crate::apply::{self, ApplyError};
use crate::deploy::NullSink;
use crate::instance::{InstanceError, ModKind, ModListEntry, Profile};
use crate::test_support::{install_mod, save_profile, temp_instance};
use camino::Utf8Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Failpoint {
    Active,
    MarkerCreated,
    OldMoved,
    NewPublished,
    ProfileWritten,
    OldRestored,
    Cleanup,
    CommittedWriteVisibleError,
    RenameRace,
}

thread_local! {
    static FAILPOINT: std::cell::Cell<Option<Failpoint>> = const { std::cell::Cell::new(None) };
}

fn setup() -> (tempfile::TempDir, Instance) {
    let (temp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("file.txt", "payload")]);
    save_profile(&instance, "Default", &[("CoolMod", true), ("Other", false)]);
    save_profile(
        &instance,
        "Survival",
        &[("Other", true), ("CoolMod", false)],
    );
    (temp, instance)
}

fn crash(instance: &Instance, point: Failpoint) {
    set_failpoint(point);
    assert!(matches!(
        rename_mod(instance, "CoolMod", "BetterMod"),
        Err(LifecycleError::TestCrash)
    ));
    assert!(journal_path(instance).exists());
}

fn recover(instance: &Instance) -> Result<(), LifecycleError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_locked(instance)
}

fn assert_rolled_back(instance: &Instance) {
    assert!(instance.mods_dir().join("CoolMod").is_dir());
    assert!(!instance.mods_dir().join("BetterMod").exists());
    assert!(!journal_path(instance).exists());
    for name in ["Default", "Survival"] {
        let profile = Profile::load(instance, name).expect("load profile");
        assert!(profile.mods.iter().any(|entry| entry.name == "CoolMod"));
        assert!(!profile.mods.iter().any(|entry| entry.name == "BetterMod"));
    }
}

pub(super) fn committed_write_visible_error(path: &Utf8Path) -> Result<(), LifecycleError> {
    if hit(Failpoint::CommittedWriteVisibleError) {
        Err(crate::error::io_err(path, std::io::Error::other("visible journal write")).into())
    } else {
        Ok(())
    }
}

pub(super) fn inject_rename_race(to: &Utf8Path) -> Result<(), LifecycleError> {
    if hit(Failpoint::RenameRace) {
        crate::fs::ensure_dir(to)?;
        crate::fs::write(&to.join("foreign.txt"), b"foreign")?;
    }
    Ok(())
}

pub(super) fn hit(point: Failpoint) -> bool {
    FAILPOINT.with(|slot| {
        let hit = slot.get() == Some(point);
        if hit {
            slot.set(None);
        }
        hit
    })
}

pub(super) fn trip(point: Failpoint) -> Result<(), LifecycleError> {
    if hit(point) {
        return Err(LifecycleError::TestCrash);
    }
    Ok(())
}

fn set_failpoint(point: Failpoint) {
    FAILPOINT.with(|slot| slot.set(Some(point)));
}

#[test]
fn rename_preserves_profile_order_state_and_nonmanaged_rows() {
    let (_temp, instance) = setup();
    let mixed = Profile {
        name: "Mixed".to_owned(),
        mods: vec![
            ModListEntry {
                name: "CoolMod".to_owned(),
                enabled: true,
                kind: ModKind::Foreign,
            },
            ModListEntry {
                name: "Group_separator".to_owned(),
                enabled: false,
                kind: ModKind::Separator,
            },
            ModListEntry {
                name: "CoolMod".to_owned(),
                enabled: false,
                kind: ModKind::Managed,
            },
        ],
        local_saves: false,
    };
    mixed.save(&instance).expect("save mixed");

    let outcome = rename_mod(&instance, "coolmod", "BetterMod").expect("rename");

    assert_eq!(outcome.report.old, "CoolMod");
    assert_eq!(outcome.report.new, "BetterMod");
    assert!(outcome.cleanup_warning.is_none());
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert!(
        instance
            .mods_dir()
            .join("BetterMod")
            .join("file.txt")
            .is_file()
    );
    let default = Profile::load(&instance, "Default").expect("load default");
    assert_eq!(
        default
            .mods
            .iter()
            .map(|entry| (&entry.name, entry.enabled))
            .collect::<Vec<_>>(),
        [
            (&"BetterMod".to_owned(), true),
            (&"Other".to_owned(), false)
        ]
    );
    let survival = Profile::load(&instance, "Survival").expect("load survival");
    assert_eq!(survival.mods[0].name, "Other");
    assert!(survival.mods[0].enabled);
    assert_eq!(survival.mods[1].name, "BetterMod");
    assert!(!survival.mods[1].enabled);
    let mixed = Profile::load(&instance, "Mixed").expect("load mixed");
    assert_eq!(mixed.mods[0].name, "CoolMod");
    assert_eq!(mixed.mods[0].kind, ModKind::Foreign);
    assert_eq!(mixed.mods[1].kind, ModKind::Separator);
    assert_eq!(mixed.mods[2].name, "BetterMod");
    assert!(!mixed.mods[2].enabled);
}

#[test]
fn true_case_only_rename_uses_the_private_intermediate() {
    let (_temp, instance) = setup();

    rename_mod(&instance, "CoolMod", "coolmod").expect("case-only rename");

    assert!(
        instance
            .installed_mods()
            .expect("installed")
            .iter()
            .any(|item| item.name == "coolmod")
    );
    assert_eq!(
        Profile::load(&instance, "Default").expect("load").mods[0].name,
        "coolmod"
    );
}

#[test]
fn name_source_and_install_collisions_are_premutation() {
    let (_temp, instance) = setup();
    install_mod(&instance, "Existing", &[("other.txt", "other")]);

    assert!(matches!(
        rename_mod(&instance, "CoolMod", "CoolMod"),
        Err(LifecycleError::Instance(InstanceError::InvalidModName(_)))
    ));
    assert!(matches!(
        rename_mod(&instance, "Missing", "BetterMod"),
        Err(LifecycleError::Instance(InstanceError::ModNotInstalled(name))) if name == "Missing"
    ));
    for name in ["Foo_separator", "Overwrite"] {
        assert!(
            matches!(
                rename_mod(&instance, "CoolMod", name),
                Err(LifecycleError::Instance(InstanceError::InvalidModName(_)))
            ),
            "{name} should be rejected"
        );
    }
    assert!(matches!(
        rename_mod(&instance, "CoolMod", "existing"),
        Err(LifecycleError::Instance(
            InstanceError::ModAlreadyInstalled(_)
        ))
    ));
    assert!(instance.mods_dir().join("CoolMod").is_dir());
    assert!(!journal_path(&instance).exists());
}

#[test]
fn lock_and_deployment_rejections_are_premutation() {
    let (_temp, instance) = setup();
    let _held = InstanceLock::acquire(&instance).expect("hold lock");
    assert!(matches!(
        rename_mod(&instance, "CoolMod", "BetterMod"),
        Err(LifecycleError::Busy)
    ));
    drop(_held);

    apply::deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    assert!(matches!(
        rename_mod(&instance, "CoolMod", "BetterMod"),
        Err(LifecycleError::LiveDeployment { .. })
    ));
    assert!(instance.mods_dir().join("CoolMod").is_dir());
}

#[test]
fn managed_and_foreign_destination_rows_are_collisions() {
    for contents in ["+CoolMod\n-BetterMod\n", "*BetterMod\n+CoolMod\n"] {
        let (_temp, instance) = setup();
        std::fs::write(
            instance.profile_dir("Default").join("modlist.txt"),
            contents,
        )
        .expect("write destination row");

        assert!(matches!(
            rename_mod(&instance, "CoolMod", "BetterMod"),
            Err(LifecycleError::Instance(InstanceError::ModAlreadyInList(_)))
        ));
        assert!(instance.mods_dir().join("CoolMod").is_dir());
    }
}

#[test]
fn recovery_handles_each_mutating_crash_window() {
    for point in [
        Failpoint::OldMoved,
        Failpoint::NewPublished,
        Failpoint::ProfileWritten,
    ] {
        let (_temp, instance) = setup();
        crash(&instance, point);
        recover(&instance).expect("recover");
        assert_rolled_back(&instance);
    }
}

#[test]
fn pre_move_crash_windows_recover_with_or_without_the_marker() {
    for (point, marked) in [(Failpoint::Active, false), (Failpoint::MarkerCreated, true)] {
        let (_temp, instance) = setup();
        crash(&instance, point);
        let journal = journal::load(&instance)
            .expect("load journal")
            .expect("active journal");
        let marker = marker_path(&instance.mods_dir().join("CoolMod"), &journal.transaction);
        assert_eq!(marker.exists(), marked);
        if marked {
            assert_eq!(std::fs::metadata(&marker).expect("marker").len(), 0);
        }

        recover(&instance).expect("recover before move");

        assert_rolled_back(&instance);
        assert!(!marker.exists());
    }
}

#[test]
fn third_state_profile_is_untouched_and_retains_the_journal() {
    let (_temp, instance) = setup();
    crash(&instance, Failpoint::NewPublished);
    let path = modlist(&instance, "Default");
    std::fs::write(&path, b"+ExternalEdit\n").expect("external edit");

    let error = recover(&instance).unwrap_err();

    assert!(matches!(error, LifecycleError::RecoveryConflict { .. }));
    assert_eq!(std::fs::read(&path).expect("read edit"), b"+ExternalEdit\n");
    assert!(journal_path(&instance).exists());
}

#[test]
fn unmarked_and_wrong_marker_occupants_are_untouched() {
    for wrong_marker in [false, true] {
        let (_temp, instance) = setup();
        crash(&instance, Failpoint::OldMoved);
        let occupant = instance.mods_dir().join("BetterMod");
        install_mod(&instance, "BetterMod", &[("foreign.txt", "foreign")]);
        if wrong_marker {
            std::fs::File::create(occupant.join(format!("{MARKER_PREFIX}wrong")))
                .expect("wrong marker");
        }

        let error = recover(&instance).unwrap_err();

        assert!(matches!(error, LifecycleError::RecoveryConflict { .. }));
        assert_eq!(
            std::fs::read_to_string(occupant.join("foreign.txt")).expect("occupant"),
            "foreign"
        );
        assert!(journal_path(&instance).exists());
    }
}

#[test]
fn wrong_marker_in_private_tree_is_untouched() {
    let (_temp, instance) = setup();
    crash(&instance, Failpoint::OldMoved);
    let journal = journal::load(&instance)
        .expect("load journal")
        .expect("active journal");
    let private = work_path(&instance).join("old");
    std::fs::remove_file(marker_path(&private, &journal.transaction)).expect("remove marker");
    std::fs::File::create(private.join(format!("{MARKER_PREFIX}wrong"))).expect("wrong marker");

    let error = recover(&instance).unwrap_err();

    assert!(matches!(error, LifecycleError::RecoveryConflict { .. }));
    assert_eq!(
        std::fs::read_to_string(private.join("file.txt")).expect("private tree"),
        "payload"
    );
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert!(!instance.mods_dir().join("BetterMod").exists());
    assert!(journal_path(&instance).exists());
}

#[test]
fn rollback_can_recrash_after_tree_restore_and_retry() {
    let (_temp, instance) = setup();
    crash(&instance, Failpoint::NewPublished);
    set_failpoint(Failpoint::OldRestored);
    assert!(matches!(recover(&instance), Err(LifecycleError::TestCrash)));

    recover(&instance).expect("retry recovery");

    assert_rolled_back(&instance);
}

#[test]
fn committed_cleanup_warns_and_is_retried() {
    let (_temp, instance) = setup();
    set_failpoint(Failpoint::Cleanup);

    let outcome = rename_mod(&instance, "CoolMod", "BetterMod").expect("committed rename");

    let warning = outcome.cleanup_warning.expect("cleanup warning");
    assert_eq!(warning.blocked_path, work_path(&instance));
    assert!(journal_path(&instance).exists());
    assert!(instance.mods_dir().join("BetterMod").is_dir());
    let blocker = work_path(&instance).join("external.txt");
    std::fs::write(&blocker, b"external").expect("write blocker");

    let apply_error = apply::status(&instance).unwrap_err();
    assert!(matches!(
        apply_error,
        ApplyError::Lifecycle(error)
            if matches!(*error, LifecycleError::CleanupPending(_))
    ));
    assert_eq!(
        std::fs::read_to_string(&blocker).expect("blocked content"),
        "external"
    );

    std::fs::remove_file(blocker).expect("remove blocker");
    recover(&instance).expect("cleanup retry");
    assert!(!journal_path(&instance).exists());
}

#[test]
fn apply_status_recovers_or_blocks_lifecycle_before_deployment() {
    let (_temp, instance) = setup();
    crash(&instance, Failpoint::OldMoved);
    assert!(
        apply::status(&instance)
            .expect("status after recovery")
            .is_none()
    );
    assert_rolled_back(&instance);

    crash(&instance, Failpoint::NewPublished);
    let path = modlist(&instance, "Default");
    std::fs::write(&path, b"+ExternalEdit\n").expect("external edit");
    let error = apply::status(&instance).unwrap_err();
    assert!(matches!(error, ApplyError::Lifecycle(_)));
    assert!(journal_path(&instance).exists());
}

#[test]
fn no_replace_rename_never_overwrites_or_copies() {
    let (_temp, instance) = setup();
    let source = instance.mods_dir().join("CoolMod");
    let target = instance.mods_dir().join("Occupied");
    install_mod(&instance, "Occupied", &[("other.txt", "other")]);

    assert!(rename_no_replace(&source, &target).is_err());

    assert_eq!(
        std::fs::read_to_string(source.join("file.txt")).expect("source"),
        "payload"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("other.txt")).expect("target"),
        "other"
    );

    crate::fs::remove_dir_all_opt(&target).expect("remove target");
    set_failpoint(Failpoint::RenameRace);
    assert!(matches!(
        rename_no_replace(&source, &target),
        Err(LifecycleError::RecoveryConflict { .. })
    ));
    assert!(source.join("file.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(target.join("foreign.txt")).expect("racing target"),
        "foreign"
    );
}

#[test]
fn committed_journal_write_visible_error_still_completes() {
    let (_temp, instance) = setup();
    set_failpoint(Failpoint::CommittedWriteVisibleError);

    let outcome = rename_mod(&instance, "CoolMod", "BetterMod").expect("visible commit");

    assert!(outcome.cleanup_warning.is_none());
    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert!(instance.mods_dir().join("BetterMod").is_dir());
    assert!(!journal_path(&instance).exists());
}

#[test]
fn inline_snapshot_restores_exact_utf8_profile_text() {
    let (_temp, instance) = setup();
    let path = modlist(&instance, "Default");
    let original = "+CoolMod\r\n-Other\r\n";
    std::fs::write(&path, original).expect("write CRLF profile");
    crash(&instance, Failpoint::ProfileWritten);

    recover(&instance).expect("recover profile");

    assert_eq!(
        std::fs::read_to_string(path).expect("read profile"),
        original
    );
    assert_rolled_back(&instance);
}

#[test]
fn journal_validation_rejects_unsafe_persisted_values() {
    for case in 0..4 {
        let (_temp, instance) = setup();
        let mut journal = Journal {
            version: 1,
            transaction: "test-transaction".to_owned(),
            phase: Phase::Active,
            operation: Operation::Rename {
                old: "CoolMod".to_owned(),
                new: "BetterMod".to_owned(),
            },
            profiles: Vec::new(),
        };
        match case {
            0 => journal.version = 2,
            1 => journal.transaction = "../escape".to_owned(),
            2 => {
                journal.operation = Operation::Rename {
                    old: "../CoolMod".to_owned(),
                    new: "BetterMod".to_owned(),
                };
            }
            3 => journal.profiles.push(ProfileSnapshot {
                profile: "../Default".to_owned(),
                original: Some("+CoolMod\n".to_owned()),
                intended: "+BetterMod\n".to_owned(),
            }),
            _ => unreachable!(),
        }
        journal::save(&instance, &journal).expect("save invalid journal");

        let error = recover(&instance).unwrap_err();

        assert!(matches!(error, LifecycleError::CorruptJournal { .. }));
        assert!(instance.mods_dir().join("CoolMod").is_dir());
        assert!(journal_path(&instance).exists());
    }
}
