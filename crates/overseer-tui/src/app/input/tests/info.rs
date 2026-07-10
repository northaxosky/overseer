//! Tests for the Info modal

use super::*;
use crate::app::input::test_helpers::key;

#[test]
fn help_modal_opens_navigates_and_closes() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('?')));
    let selected = match &app.modal {
        Some(Modal::Info(info)) => {
            assert_eq!(info.title, "Help", "? opens the Help info modal");
            info.state.index()
        }
        _ => panic!("? opens an Info modal"),
    };
    assert_eq!(selected, Some(0), "opens on the first entry");

    app.handle_key(key(KeyCode::Char('j')));
    match &app.modal {
        Some(Modal::Info(info)) => {
            assert_eq!(info.state.index(), Some(1), "j scrolls within help");
        }
        _ => panic!("navigation does not close the modal"),
    }

    app.handle_key(key(KeyCode::Esc));
    assert!(app.modal.is_none(), "Esc closes the help modal");
}

#[test]
fn enter_does_not_submit_the_info_modal() {
    let mut app = App::sample();
    app.handle_key(key(KeyCode::Char('?')));
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(app.modal, Some(Modal::Info(_))),
        "Enter is inert: the Info modal is dismiss-only"
    );
}
