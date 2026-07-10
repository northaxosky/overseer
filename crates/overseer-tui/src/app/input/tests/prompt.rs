//! Tests for the Prompt modal

use super::*;
use crate::app::Select;
use crate::app::input::test_helpers::*;
use overseer_core::instance::{ModListEntry, Profile};
use overseer_core::test_support::{install_mod, save_profile};
use ratatui::widgets::ListState;

#[test]
fn n_in_the_profile_picker_opens_the_new_profile_prompt() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('p')));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Profile,
                ..
            }))
        ),
        "p opens the profile picker"
    );
    app.handle_key(key(KeyCode::Char('n')));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Prompt(Prompt {
                kind: PromptKind::NewProfile,
                ..
            }))
        ),
        "n opens the new-profile prompt"
    );
}

#[test]
fn r_in_the_profile_picker_opens_a_rename_prompt_for_the_highlighted_profile() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    instance.create_profile("Default").expect("create Default");
    let mut app = App::sample();
    app.session.instance = instance;

    app.handle_key(key(KeyCode::Char('p'))); // profile picker
    app.handle_key(key(KeyCode::Char('r'))); // rename side-action

    match &app.modal {
        Some(Modal::Prompt(Prompt {
            kind: PromptKind::RenameProfile { old },
            input,
            error,
        })) => {
            assert_eq!(old, "Default");
            assert_eq!(input, "");
            assert!(error.is_none());
        }
        other => panic!("expected a rename-profile prompt, got {other:?}"),
    }
}

#[test]
fn submitting_a_valid_profile_rename_moves_it_on_disk_and_reopens_the_picker() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    instance.create_profile("Default").expect("create Default");
    instance.create_profile("Extra").expect("create Extra");
    let mut app = App::sample();
    app.session.instance = instance;

    app.handle_key(key(KeyCode::Char('p'))); // picker: [Default, Extra]
    app.handle_key(key(KeyCode::Down)); // highlight the non-active "Extra"
    app.handle_key(key(KeyCode::Char('r')));
    for c in "Extra2".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(app.session.instance.profile_dir("Extra2").is_dir());
    assert!(!app.session.instance.profile_dir("Extra").exists());
    match &app.modal {
        Some(Modal::Select(s)) => {
            assert!(
                s.items.iter().any(|p| p == "Extra2"),
                "picker reopened with the new name"
            );
            assert!(!s.items.iter().any(|p| p == "Extra"));
        }
        other => panic!("expected the reopened profile picker, got {other:?}"),
    }
    assert!(app.message.is_some(), "an ok notice is shown");
}

#[test]
fn renaming_the_active_profile_reloads_the_session_under_the_new_name() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    instance.create_profile("Default").expect("create Default");
    let mut app = App::sample();
    app.session.instance = instance;

    app.handle_key(key(KeyCode::Char('p'))); // picker: [Default] (the active profile)
    app.handle_key(key(KeyCode::Char('r')));
    for c in "Main".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        app.session.profile.name, "Main",
        "the active session reloaded under the new name"
    );
    assert!(app.session.instance.profile_dir("Main").is_dir());
}

#[test]
fn submitting_an_invalid_profile_rename_keeps_the_prompt_with_error() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    instance.create_profile("Default").expect("create Default");
    let mut app = App::sample();
    app.session.instance = instance;

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('r')));
    for c in "a/b".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(matches!(prompt_state(&app), Some(("a/b", Some(_)))));
}

#[test]
fn submitting_a_duplicate_profile_rename_keeps_the_prompt_with_error() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    instance.create_profile("Default").expect("create Default");
    instance.create_profile("Extra").expect("create Extra");
    let mut app = App::sample();
    app.session.instance = instance;

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Down)); // Extra
    app.handle_key(key(KeyCode::Char('r')));
    for c in "Default".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(matches!(prompt_state(&app), Some(("Default", Some(_)))));
    assert!(
        app.session.instance.profile_dir("Extra").is_dir(),
        "nothing moved on collision"
    );
}

#[test]
fn typing_and_backspace_edit_the_prompt_input() {
    let mut app = App::sample();
    open_prompt_and_type(&mut app, "Surv");
    assert_eq!(prompt_state(&app), Some(("Surv", None)));
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(
        prompt_state(&app),
        Some(("Sur", None)),
        "backspace pops a char"
    );
}

#[test]
fn esc_from_the_prompt_returns_to_the_profile_picker() {
    let mut app = App::sample();
    open_prompt_and_type(&mut app, "Whatever");
    app.handle_key(key(KeyCode::Esc));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Profile,
                ..
            }))
        ),
        "Esc cancels back to the picker it came from"
    );
}

#[test]
fn submitting_an_empty_name_sets_an_inline_error_and_keeps_the_prompt() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('n')));
    app.handle_key(key(KeyCode::Enter)); // input is empty
    match prompt_state(&app) {
        Some((input, error)) => {
            assert_eq!(input, "", "input is preserved");
            assert!(error.is_some(), "an inline error is shown");
        }
        None => panic!("the prompt must stay open on a validation error"),
    }
}

#[test]
fn submitting_a_name_with_a_path_separator_is_rejected_inline() {
    let mut app = App::sample();
    open_prompt_and_type(&mut app, "a/b");
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(prompt_state(&app), Some(("a/b", Some(_)))),
        "a path-dangerous name keeps the prompt open with an error"
    );
}

#[test]
fn submitting_a_valid_name_creates_the_profile_and_returns_to_the_picker() {
    // create_profile writes to disk, so back the session with a temp instance
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;

    open_prompt_and_type(&mut app, "Survival");
    app.handle_key(key(KeyCode::Enter));

    match &app.modal {
        Some(Modal::Select(s)) => {
            let i = s
                .items
                .iter()
                .position(|p| p == "Survival")
                .expect("new profile is listed");
            assert_eq!(s.state.index(), Some(i), "the new profile is selected");
        }
        _ => panic!("a successful create returns to the profile picker"),
    }
    assert!(
        app.session.instance.profile_dir("Survival").is_dir(),
        "the profile exists on disk"
    );
    assert!(app.message.is_some(), "an ok notice is shown");
}

#[test]
fn submitting_a_duplicate_name_keeps_the_prompt_with_an_error() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    instance.create_profile("Default").expect("seed a profile");
    let mut app = App::sample();
    app.session.instance = instance;

    open_prompt_and_type(&mut app, "Default");
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(prompt_state(&app), Some(("Default", Some(_)))),
        "a duplicate keeps the prompt open with an inline error"
    );
}

#[test]
fn validate_name_rejects_windows_unsafe_names() {
    // Windows strips a trailing dot/space, so these would create a different directory than requested and silently desync from Profile.name
    assert!(validate_name("Foo.").is_err(), "trailing dot");
    assert!(validate_name("Foo ").is_err(), "trailing space");
    // Reserved device names are rejected as a whole, case-insensitively
    assert!(validate_name("nul").is_err(), "reserved, lowercase");
    assert!(validate_name("COM1").is_err(), "reserved, uppercase");
}

#[test]
fn validate_name_allows_an_interior_space() {
    assert!(validate_name("Survival Build").is_ok());
}

#[test]
fn r_on_a_managed_mod_opens_an_empty_rename_prompt() {
    let mut app = App::sample();
    app.mods_state.select(Some(1)); // display 1 = CoolMod (model 0) under reversal

    app.handle_key(key(KeyCode::Char('R')));

    match &app.modal {
        Some(Modal::Prompt(Prompt {
            kind: PromptKind::RenameMod { old },
            input,
            error,
        })) => {
            assert_eq!(old, "CoolMod");
            assert_eq!(input, "");
            assert!(error.is_none());
        }
        other => panic!("expected rename prompt, got {other:?}"),
    }
}

#[test]
fn r_on_a_foreign_row_or_the_plugins_pane_is_a_note() {
    let mut app = App::sample();
    app.session
        .profile
        .mods
        .push(overseer_core::instance::ModListEntry {
            name: "DLC".to_owned(),
            enabled: true,
            kind: ModKind::Foreign,
        });
    app.mods_state.select(Some(0)); // display 0 = the pushed row (model 2) under reversal

    app.handle_key(key(KeyCode::Char('R')));

    assert!(app.modal.is_none());
    assert!(app.message.is_some());

    app.focus = Focus::Workspace;
    app.message = None;
    app.handle_key(key(KeyCode::Char('R')));
    assert!(app.modal.is_none());
    assert!(app.message.is_some());
}

#[test]
fn r_on_a_separator_opens_a_rename_separator_prompt_prefilled_with_its_display_name() {
    let mut app = App::sample();
    app.session
        .profile
        .mods
        .push(overseer_core::instance::ModListEntry {
            name: "Gameplay_separator".to_owned(),
            enabled: false,
            kind: ModKind::Separator,
        });
    app.mods_state.select(Some(0)); // display 0 = the pushed separator (model 2) under reversal

    app.handle_key(key(KeyCode::Char('R')));

    match &app.modal {
        Some(Modal::Prompt(Prompt {
            kind: PromptKind::RenameSeparator { index, name },
            input,
            error,
        })) => {
            assert_eq!(*index, 2);
            assert_eq!(name, "Gameplay");
            assert_eq!(input, "");
            assert!(error.is_none());
        }
        other => panic!("expected a rename-separator prompt, got {other:?}"),
    }
}

#[test]
fn submitting_a_valid_separator_rename_persists_and_reselects() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    app.session.profile.mods = vec![
        ModListEntry {
            name: "A".to_owned(),
            enabled: true,
            kind: ModKind::Managed,
        },
        ModListEntry {
            name: "Zone_separator".to_owned(),
            enabled: false,
            kind: ModKind::Separator,
        },
    ];
    app.session
        .profile
        .save(&app.session.instance)
        .expect("seed the profile");
    app.mods_state.select(Some(0)); // display 0 = the separator (model 1) under reversal

    app.handle_key(key(KeyCode::Char('R')));
    for c in "Areas".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(app.modal.is_none(), "a successful rename closes the prompt");
    assert_eq!(app.session.profile.mods[1].name, "Areas_separator");
    assert_eq!(
        app.selected_mod(),
        Some(1),
        "the renamed separator stays selected"
    );
    let reloaded = Profile::load(&app.session.instance, "Default").expect("reload");
    assert_eq!(
        reloaded.mods[1].name, "Areas_separator",
        "persisted to disk"
    );
}

#[test]
fn submitting_a_colliding_separator_rename_keeps_the_prompt_with_error() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    app.session.profile.mods = vec![
        ModListEntry {
            name: "Alpha_separator".to_owned(),
            enabled: false,
            kind: ModKind::Separator,
        },
        ModListEntry {
            name: "Beta_separator".to_owned(),
            enabled: false,
            kind: ModKind::Separator,
        },
    ];
    app.session
        .profile
        .save(&app.session.instance)
        .expect("seed the profile");
    app.mods_state.select(Some(1)); // display 1 = Alpha (model 0) under reversal

    app.handle_key(key(KeyCode::Char('R')));
    for c in "Beta".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(
        matches!(prompt_state(&app), Some(("Beta", Some(_)))),
        "a colliding name keeps the prompt open with an inline error"
    );
    assert_eq!(app.session.profile.mods[0].name, "Alpha_separator");
}

#[test]
fn submitting_a_valid_mod_rename_updates_memory_and_keeps_selection() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    let mut app = App::sample();
    app.session.instance = instance;
    app.mods_state.select(Some(1)); // display 1 = CoolMod (model 0), the installed mod

    app.handle_key(key(KeyCode::Char('R')));
    for c in "BetterMod".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(app.modal.is_none(), "successful rename closes prompt");
    assert_eq!(app.session.profile.mods[0].name, "BetterMod");
    assert_eq!(app.mods_state.selected(), Some(1));
    assert!(app.message.is_some(), "an ok notice is shown");
}

#[test]
fn submitting_an_invalid_mod_rename_keeps_the_prompt_with_error() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('R')));
    for c in "a/b".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(
        matches!(prompt_state(&app), Some(("a/b", Some(_)))),
        "invalid name stays inline"
    );
}

#[test]
fn submitting_a_duplicate_mod_rename_keeps_the_prompt_with_error() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    install_mod(&instance, "Existing", &[("Textures/b.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    let mut app = App::sample();
    app.session.instance = instance;
    app.mods_state = ListState::default();
    app.mods_state.select(Some(1)); // display 1 = CoolMod (model 0), the installed mod

    app.handle_key(key(KeyCode::Char('R')));
    for c in "Existing".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(
        matches!(prompt_state(&app), Some(("Existing", Some(_)))),
        "duplicate name stays inline"
    );
    assert_eq!(app.session.profile.mods[0].name, "CoolMod");
}

#[test]
fn a_adds_a_separator_heading_the_selection_and_saves() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    let mut app = App::sample();
    app.session.instance = instance;
    app.mods_state.select(Some(0));

    app.handle_key(key(KeyCode::Char('A')));
    for c in "Gameplay".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(app.modal.is_none(), "success closes the prompt");
    // The new separator becomes the selection and heads the previously-selected row
    let sel = app.selected_mod().expect("a row is selected");
    assert_eq!(app.session.profile.mods[sel].kind, ModKind::Separator);
    assert_eq!(app.session.profile.mods[sel].name, "Gameplay_separator");
    assert_eq!(
        app.mods_state.selected(),
        Some(0),
        "the new separator heads the selection at the top of its group"
    );
}

#[test]
fn a_with_an_invalid_separator_name_keeps_the_prompt_with_error() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('A')));
    for c in "a/b".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(prompt_state(&app), Some(("a/b", Some(_)))),
        "an invalid name stays inline with an error"
    );
}

#[test]
fn submitting_a_path_adds_a_derived_launch_target() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    std::fs::create_dir_all(&app.session.instance.root).unwrap();

    app.handle_key(key(KeyCode::Char('l'))); // launch picker (empty)
    app.handle_key(key(KeyCode::Char('a'))); // add-exe prompt
    for c in "C:/Tools/FO4Edit.exe".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    // The name is derived from the file stem and selected in the reopened picker
    match &app.modal {
        Some(Modal::Select(s)) => {
            let i = s
                .items
                .iter()
                .position(|p| p == "FO4Edit")
                .expect("the derived target is listed");
            assert_eq!(s.state.index(), Some(i), "the new target is selected");
        }
        _ => panic!("a successful add returns to the launch picker"),
    }
    let reloaded =
        overseer_core::instance::Instance::load(app.session.instance.root.clone()).unwrap();
    let names: Vec<_> = reloaded
        .config
        .executables
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, vec!["FO4Edit"], "the derived target is persisted");
}

#[test]
fn esc_from_the_add_exe_prompt_returns_to_the_launch_picker() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('a')));
    app.handle_key(key(KeyCode::Esc));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Launch,
                ..
            }))
        ),
        "Esc cancels back to the launch picker"
    );
}

#[test]
fn submitting_a_duplicate_derived_name_keeps_the_prompt_with_an_error() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    app.session.instance.config.executables = vec![Executable {
        name: "FO4Edit".to_owned(),
        path: camino::Utf8PathBuf::from("C:/old/FO4Edit.exe"),
        args: Vec::new(),
    }];

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('a')));
    for c in "D:/new/FO4Edit.exe".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    match prompt_state(&app) {
        Some((input, error)) => {
            assert_eq!(input, "D:/new/FO4Edit.exe", "the input is preserved");
            assert!(error.is_some(), "a duplicate derived name errors inline");
        }
        None => panic!("the prompt stays open on a duplicate"),
    }
}

#[test]
fn a_relative_path_is_absolutized_before_it_is_stored() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    std::fs::create_dir_all(&app.session.instance.root).unwrap();

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('a')));
    for c in "tools/MyTool.exe".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    // A relative path is resolved against cwd, so config never stores a cwd-dependent path
    let exe = app
        .session
        .instance
        .config
        .executables
        .iter()
        .find(|e| e.name == "MyTool")
        .expect("the target was added under its derived name");
    assert!(exe.path.is_absolute(), "the stored path is absolutized");
}

#[test]
fn typing_q_edits_the_prompt_instead_of_quitting() {
    let mut app = App::sample();
    open_prompt_and_type(&mut app, "q"); // a profile name may start with q
    assert_eq!(
        prompt_state(&app),
        Some(("q", None)),
        "q is typed into the input, not treated as a quit"
    );
    assert!(
        !app.should_quit,
        "quitting is suppressed while a prompt is open"
    );
}

#[test]
fn esc_on_a_rename_mod_prompt_returns_to_the_main_view() {
    let mut app = App::sample();
    app.mods_state.select(Some(1)); // display 1 = CoolMod, a managed mod
    app.handle_key(key(KeyCode::Char('R'))); // rename-mod prompt, opened from the main view
    assert!(
        matches!(
            app.modal,
            Some(Modal::Prompt(Prompt {
                kind: PromptKind::RenameMod { .. },
                ..
            }))
        ),
        "R opens a rename-mod prompt"
    );

    app.handle_key(key(KeyCode::Esc));
    assert!(
        app.modal.is_none(),
        "Esc on a main-view prompt closes to the main view, not back to a picker"
    );
}
