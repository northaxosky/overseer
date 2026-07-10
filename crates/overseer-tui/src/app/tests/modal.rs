//! Tests for modal list routing

use super::*;

#[test]
fn list_parts_routes_select_info_and_doctor() {
    let mut select = Modal::Select(Select {
        kind: SelectKind::Launch,
        items: vec!["one".to_owned(), "two".to_owned()],
        state: ListCursor::default(),
    });
    let mut info = Modal::Info(Info {
        title: "Help".to_owned(),
        entries: vec![("?".to_owned(), "help".to_owned())],
        state: ListCursor::default(),
    });
    let mut doctor = Modal::Doctor(DoctorReport {
        report: Report::new(Vec::new()),
        list: ListCursor::default(),
    });

    assert_eq!(select.list_parts_mut().map(|(_, len)| len), Some(2));
    assert_eq!(info.list_parts_mut().map(|(_, len)| len), Some(1));
    assert_eq!(doctor.list_parts_mut().map(|(_, len)| len), Some(0));
}

#[test]
fn list_parts_excludes_non_list_modals() {
    let mut prompt = Modal::Prompt(Prompt {
        kind: PromptKind::NewProfile,
        input: String::new(),
        error: None,
    });
    let mut confirm = Modal::Confirm(Confirm {
        message: "Continue?".to_owned(),
        action: ConfirmAction::RemoveExe("tool".to_owned()),
    });

    assert!(prompt.list_parts_mut().is_none());
    assert!(confirm.list_parts_mut().is_none());
}
