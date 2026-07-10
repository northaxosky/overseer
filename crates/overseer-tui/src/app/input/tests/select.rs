//! Tests for the Select modal

use super::*;
use crate::app::input::test_helpers::*;
use ratatui::crossterm::event::KeyModifiers;

#[test]
fn l_opens_the_launcher_and_l_again_closes_it() {
    let mut app = App::sample();
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Launch,
                ..
            }))
        ),
        "l opens the launch select modal"
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert!(app.modal.is_none(), "l again closes it");
}

#[test]
fn launching_with_no_targets_notes_and_closes() {
    let mut app = App::sample(); // sample instance configures no exes
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.modal.is_none(), "picker closes");
    assert!(app.message.is_some(), "user is told there are none");
}

#[test]
fn esc_closes_the_launch_modal() {
    let mut app = App::sample();
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert!(app.modal.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.modal.is_none(), "Esc cancels the modal");
}

#[test]
fn launch_modal_navigates_and_clamps() {
    use camino::Utf8PathBuf;
    use overseer_core::instance::Executable;
    let mut app = App::sample();
    app.session.instance.config.executables = vec![
        Executable {
            name: "game".to_owned(),
            path: Utf8PathBuf::from("game.exe"),
            args: Vec::new(),
        },
        Executable {
            name: "script-extender".to_owned(),
            path: Utf8PathBuf::from("f4se.exe"),
            args: Vec::new(),
        },
    ];
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(modal_selection(&app), Some(0), "opens on the first target");
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(modal_selection(&app), Some(1), "j moves down");
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(modal_selection(&app), Some(1), "clamps at the end");
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(modal_selection(&app), Some(0), "k moves up");
}

#[test]
fn n_does_nothing_in_the_launch_picker() {
    // `n` is a profile-picker side-action only; in the launcher it's inert
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('n')));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Launch,
                ..
            }))
        ),
        "the launch picker stays open and unchanged"
    );
}

#[test]
fn a_in_the_launch_picker_opens_the_add_exe_prompt() {
    use crate::app::{Prompt, PromptKind};
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('a')));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Prompt(Prompt {
                kind: PromptKind::AddExe,
                ..
            }))
        ),
        "a opens the add-exe prompt"
    );
}

#[test]
fn x_in_the_launch_picker_confirms_removal_of_the_highlighted_target() {
    let mut app = App::sample();
    app.session.instance.config.executables = vec![overseer_core::instance::Executable {
        name: "FO4Edit".to_owned(),
        path: Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
        args: Vec::new(),
    }];
    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('x')));
    match &app.modal {
        Some(Modal::Confirm(c)) => {
            assert!(
                c.message.contains("FO4Edit"),
                "the confirm names the target"
            );
            assert!(
                matches!(&c.action, ConfirmAction::RemoveExe(n) if n == "FO4Edit"),
                "x stages a RemoveExe confirm"
            );
        }
        _ => panic!("x opens a remove confirm"),
    }
}

#[test]
fn x_on_an_empty_launch_picker_notes_and_stays_open() {
    let mut app = App::sample(); // the sample instance configures no exes
    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('x')));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Launch,
                ..
            }))
        ),
        "the picker stays open"
    );
    assert!(
        app.message.is_some(),
        "the user is told there is nothing to remove"
    );
}

#[test]
fn confirming_removal_deletes_the_target_and_reopens_the_picker() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    std::fs::create_dir_all(&app.session.instance.root).unwrap();
    app.session.instance.config.executables = vec![
        overseer_core::instance::Executable {
            name: "game".to_owned(),
            path: Utf8PathBuf::from("game.exe"),
            args: Vec::new(),
        },
        overseer_core::instance::Executable {
            name: "FO4Edit".to_owned(),
            path: Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
            args: Vec::new(),
        },
    ];
    app.session.instance.save().unwrap();

    app.handle_key(key(KeyCode::Char('l'))); // picker opens on "game"
    app.handle_key(key(KeyCode::Char('x'))); // confirm remove "game"
    app.handle_key(key(KeyCode::Char('y'))); // accept

    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Launch,
                ..
            }))
        ),
        "removal reopens the launch picker"
    );
    let names: Vec<_> = app
        .session
        .instance
        .config
        .executables
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, vec!["FO4Edit"], "the target is gone from memory");

    let reloaded =
        overseer_core::instance::Instance::load(app.session.instance.root.clone()).unwrap();
    assert_eq!(
        reloaded.config.executables.len(),
        1,
        "the removal is persisted to disk"
    );
}

#[test]
fn a_failed_save_on_removal_rolls_the_target_back_in_memory() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    std::fs::create_dir_all(&app.session.instance.root).unwrap();
    app.session.instance.config.executables = vec![overseer_core::instance::Executable {
        name: "game".to_owned(),
        path: Utf8PathBuf::from("game.exe"),
        args: Vec::new(),
    }];
    app.session.instance.save().unwrap();
    // Delete the instance dir so the next save() fails mid-removal
    std::fs::remove_dir_all(&app.session.instance.root).unwrap();

    app.handle_key(key(KeyCode::Char('l'))); // picker opens on "game"
    app.handle_key(key(KeyCode::Char('x'))); // confirm remove "game"
    app.handle_key(key(KeyCode::Char('y'))); // accept → save fails

    assert_eq!(
        app.session
            .instance
            .config
            .executables
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        vec!["game"],
        "a failed save leaves the target in memory so it still matches disk"
    );
    assert!(
        app.message
            .as_ref()
            .is_some_and(|n| n.text.contains("Could not save")),
        "the failure is reported"
    );
}

#[test]
fn s_opens_the_instance_picker_and_navigation_clamps() {
    let mut app = App::sample();
    let current = app.session.instance.root.to_string();
    app.handle_key(key(KeyCode::Char('s')));
    match &app.modal {
        Some(Modal::Select(s)) => {
            assert_eq!(s.kind, SelectKind::Instance, "s opens the instance picker");
            assert!(
                !s.items.contains(&current),
                "the current instance is excluded"
            );
        }
        _ => panic!("s opens a Select modal"),
    }
    assert_eq!(
        modal_selection(&app),
        Some(0),
        "opens on the first instance"
    );
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(modal_selection(&app), Some(1), "j moves down");
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(modal_selection(&app), Some(1), "clamps at the end");
    app.handle_key(key(KeyCode::Char('k')));
    assert_eq!(modal_selection(&app), Some(0), "k moves up");
}

#[test]
fn s_again_closes_the_instance_picker() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('s')));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Instance,
                ..
            }))
        ),
        "s opens the instance select modal"
    );
    app.handle_key(key(KeyCode::Char('s')));
    assert!(app.modal.is_none(), "s again closes it");
}

#[test]
fn switching_to_a_missing_instance_keeps_the_picker_open() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('s')));
    // The sample's recents point at directories with no instance, so the load fails
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Instance,
                ..
            }))
        ),
        "a failed switch leaves the instance picker open"
    );
    assert!(app.message.is_some(), "the failure is reported");
}

fn instance_with_one_target() -> (tempfile::TempDir, crate::app::App) {
    let (tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    std::fs::create_dir_all(&app.session.instance.root).unwrap();
    app.session.instance.config.executables = vec![overseer_core::instance::Executable {
        name: "FO4Edit".to_owned(),
        path: Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
        args: Vec::new(),
    }];
    app.session.instance.save().unwrap();
    (tmp, app)
}

#[test]
fn e_edits_a_targets_name_then_args_and_persists_both() {
    let (_tmp, mut app) = instance_with_one_target();

    app.handle_key(key(KeyCode::Char('l'))); // picker on FO4Edit
    app.handle_key(key(KeyCode::Char('e'))); // edit -> name step (prefilled)
    for _ in 0.."FO4Edit".len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "xEdit".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter)); // apply name -> args step
    for c in "-IKnowWhatImDoing -foo".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter)); // apply args -> reopen picker

    let reloaded =
        overseer_core::instance::Instance::load(app.session.instance.root.clone()).unwrap();
    let exe = &reloaded.config.executables[0];
    assert_eq!(exe.name, "xEdit", "the rename persisted");
    assert_eq!(
        exe.args,
        vec!["-IKnowWhatImDoing".to_owned(), "-foo".to_owned()],
        "the whitespace-split args persisted"
    );
    match &app.modal {
        Some(Modal::Select(s)) => {
            let i = s.items.iter().position(|p| p == "xEdit").expect("listed");
            assert_eq!(s.state.index(), Some(i), "the renamed target is selected");
        }
        _ => panic!("editing reopens the launch picker"),
    }
}

#[test]
fn editing_the_name_to_an_existing_target_keeps_the_prompt_with_an_error() {
    let (_tmp, mut app) = instance_with_one_target();
    app.session
        .instance
        .config
        .executables
        .push(overseer_core::instance::Executable {
            name: "game".to_owned(),
            path: Utf8PathBuf::from("game.exe"),
            args: Vec::new(),
        });
    app.session.instance.save().unwrap();

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('j'))); // move to "game"
    app.handle_key(key(KeyCode::Char('e'))); // edit "game"
    for _ in 0.."game".len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "FO4Edit".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter)); // collides with the other target

    match &app.modal {
        Some(Modal::Prompt(p)) => assert!(
            p.error
                .as_deref()
                .is_some_and(|e| e.contains("already exists")),
            "a colliding rename keeps the prompt with an error"
        ),
        _ => panic!("a rejected rename stays on the name prompt"),
    }
}

#[test]
fn editing_args_to_empty_clears_them() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    std::fs::create_dir_all(&app.session.instance.root).unwrap();
    app.session.instance.config.executables = vec![overseer_core::instance::Executable {
        name: "FO4Edit".to_owned(),
        path: Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
        args: vec!["-old".to_owned()],
    }];
    app.session.instance.save().unwrap();

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('e'))); // name step
    app.handle_key(key(KeyCode::Enter)); // keep the name -> args step (prefilled "-old")
    for _ in 0.."-old".len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    app.handle_key(key(KeyCode::Enter)); // empty args

    let reloaded =
        overseer_core::instance::Instance::load(app.session.instance.root.clone()).unwrap();
    assert!(
        reloaded.config.executables[0].args.is_empty(),
        "clearing the args prompt removes all launch args"
    );
}

#[test]
fn e_on_an_empty_launch_picker_just_notes_it() {
    let mut app = App::sample();
    app.session.instance.config.executables = Vec::new();

    app.handle_key(key(KeyCode::Char('l'))); // empty picker
    app.handle_key(key(KeyCode::Char('e')));

    assert!(
        matches!(
            app.modal,
            Some(Modal::Select(Select {
                kind: SelectKind::Launch,
                ..
            }))
        ),
        "e on an empty picker opens no prompt"
    );
    assert!(
        app.message
            .as_ref()
            .is_some_and(|n| n.text.contains("No launch target")),
        "the user is told there is nothing to edit"
    );
}
