//! Tests for the Select modal

use super::*;
use crate::app::OperationKind;
use crate::app::input::test_helpers::*;
use ratatui::crossterm::event::KeyModifiers;

fn app_with_temp_instance() -> (tempfile::TempDir, App) {
    let (temp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    (temp, app)
}

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
    if let Some(Modal::Select(select)) = &app.modal {
        assert_eq!(select.launch_rows[0].key, "game");
        assert_eq!(select.launch_rows[1].key, "script-extender");
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert!(app.modal.is_none(), "l again closes it");
}

#[test]
fn launch_selection_starts_the_uniform_prepare_operation() {
    let mut app = App::sample();
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.modal.is_none(), "picker closes");
    assert!(
        app.operation_running(),
        "launch preparation runs in the worker"
    );
    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::PrepareLaunch)
    );
    app.finish_operation_after_terminal();
}

struct PendingLaunch;

impl overseer_core::deploy::LaunchHandle for PendingLaunch {
    fn try_wait(
        &mut self,
    ) -> Result<Option<std::process::ExitStatus>, overseer_core::deploy::DeployError> {
        Ok(None)
    }

    fn detach(self: Box<Self>) {}
}

#[test]
fn a_second_launch_is_rejected_before_spawning() {
    let mut app = App::sample();
    app.track_launch(crate::app::LaunchSession::new(
        Box::new(PendingLaunch),
        "Fallout 4".to_owned(),
        "Default".to_owned(),
        app.session.instance.clone(),
        overseer_core::launch::LaunchMarker {
            launch_id: 1,
            tool: "Fallout 4".to_owned(),
            profile: "Default".to_owned(),
            timestamp: 1,
            launcher_pid: std::process::id(),
        },
    ));

    app.launch(Some(LaunchRow {
        key: "script-extender".to_owned(),
        kind: ToolKind::ScriptExtender,
        display_name: "Script Extender".to_owned(),
    }));

    assert!(app.game_running());
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("A launch session is already running")
    );
}

#[test]
fn empty_replace_picker_explains_that_downloads_have_no_archives() {
    let (_temp, mut app) = app_with_temp_instance();

    app.begin_replace_mod();

    match &app.modal {
        Some(Modal::Select(select)) => {
            assert!(select.items.is_empty(), "there is no actionable archive");
            assert_eq!(
                select.kind.empty_message(),
                "No archives in Downloads to replace with."
            );
        }
        _ => panic!("replace opens its archive picker"),
    }
}

#[test]
fn replace_picker_confirms_the_selected_archive() {
    let (_temp, mut app) = app_with_temp_instance();
    overseer_core::test_support::write_zip(
        &app.session.instance.downloads_dir().join("New.zip"),
        &[("Textures/a.dds", b"replacement")],
    );

    app.begin_replace_mod();
    assert!(matches!(
        app.modal,
        Some(Modal::Select(Select {
            ref items,
            ..
        })) if items == &["New.zip"]
    ));
    app.handle_key(key(KeyCode::Enter));

    assert!(matches!(
        app.modal,
        Some(Modal::Confirm(Confirm {
            action: ConfirmAction::ReplaceMod {
                ref name,
                ref archive,
            },
            ..
        })) if name == "OffMod" && archive == "New.zip"
    ));
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
    let mut app = App::sample();
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
    app.session.instance.config.tools = vec![overseer_core::instance::UserTool::new(
        "FO4Edit".to_owned(),
        Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
        Vec::new(),
    )];
    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('x')));
    match &app.modal {
        Some(Modal::Confirm(c)) => {
            assert!(
                c.message.contains("FO4Edit"),
                "the confirm names the target"
            );
            assert!(
                matches!(&c.action, ConfirmAction::RemoveExe(n) if n == "fo4edit"),
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
    app.session.instance.config.tools = vec![overseer_core::instance::UserTool::new(
        "FO4Edit",
        "C:/Tools/FO4Edit.exe",
        Vec::new(),
    )];
    app.session.instance.save().unwrap();

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('j'))); // user tool after derived rows
    app.handle_key(key(KeyCode::Char('x')));
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
        .tools
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, Vec::<&str>::new(), "the target is gone from memory");

    let reloaded =
        overseer_core::instance::Instance::load(app.session.instance.root.clone()).unwrap();
    assert_eq!(
        reloaded.config.tools.len(),
        0,
        "the removal is persisted to disk"
    );
}

#[test]
fn a_failed_save_on_removal_rolls_the_target_back_in_memory() {
    let (_tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    std::fs::create_dir_all(&app.session.instance.root).unwrap();
    app.session.instance.config.tools = vec![overseer_core::instance::UserTool::new(
        "game".to_owned(),
        Utf8PathBuf::from("game.exe"),
        Vec::new(),
    )];
    app.session.instance.save().unwrap();
    // Replace overseer.toml with a directory so the next atomic save fails
    let config = app.session.instance.root.join("overseer.toml");
    std::fs::remove_file(&config).unwrap();
    std::fs::create_dir(&config).unwrap();

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('j'))); // user tool after derived rows
    app.handle_key(key(KeyCode::Char('x')));
    app.handle_key(key(KeyCode::Char('y'))); // accept → save fails

    assert_eq!(
        app.session
            .instance
            .config
            .tools
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
    app.session.instance.config.tools = vec![overseer_core::instance::UserTool::new(
        "FO4Edit".to_owned(),
        Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
        Vec::new(),
    )];
    app.session.instance.save().unwrap();
    (tmp, app)
}

#[test]
fn e_edits_a_targets_name_then_args_and_persists_both() {
    let (_tmp, mut app) = instance_with_one_target();

    app.handle_key(key(KeyCode::Char('l'))); // picker on FO4Edit
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('j')));
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
    let exe = &reloaded.config.tools[0];
    assert_eq!(exe.name, "xEdit", "the rename persisted");
    assert_eq!(
        exe.args,
        vec!["-IKnowWhatImDoing".to_owned(), "-foo".to_owned()],
        "the whitespace-split args persisted"
    );
    match &app.modal {
        Some(Modal::Select(s)) => {
            let i = s
                .launch_rows
                .iter()
                .position(|row| row.key == "fo4edit")
                .expect("listed");
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
        .tools
        .push(overseer_core::instance::UserTool::new(
            "game".to_owned(),
            Utf8PathBuf::from("game.exe"),
            Vec::new(),
        ));
    app.session.instance.save().unwrap();

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('j'))); // move to user tool "game"
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
    app.session.instance.config.tools = vec![overseer_core::instance::UserTool::new(
        "FO4Edit".to_owned(),
        Utf8PathBuf::from("C:/Tools/FO4Edit.exe"),
        vec!["-old".to_owned()],
    )];
    app.session.instance.save().unwrap();

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('e'))); // name step
    app.handle_key(key(KeyCode::Enter)); // keep the name -> args step (prefilled "-old")
    for _ in 0.."-old".len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    app.handle_key(key(KeyCode::Enter)); // empty args

    let reloaded =
        overseer_core::instance::Instance::load(app.session.instance.root.clone()).unwrap();
    assert!(
        reloaded.config.tools[0].args.is_empty(),
        "clearing the args prompt removes all launch args"
    );
}

#[test]
fn e_on_a_derived_launch_target_refuses_to_edit_it() {
    let mut app = App::sample();
    app.session.instance.config.tools = Vec::new();

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
            .is_some_and(|n| n.text.contains("cannot be edited")),
        "the user is told derived targets cannot be edited"
    );
}
