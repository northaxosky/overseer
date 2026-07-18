//! Tests for Mods pane projection and separator collapse state

use super::*;
use overseer_core::instance::{ModEntry, ModKind};

/// Build a managed mod fixture
fn managed(name: &str) -> ModRow {
    ModRow::Item(ModEntry {
        name: name.to_owned(),
        enabled: true,
        kind: ModKind::Managed,
    })
}

/// Build a separator fixture
fn separator(name: &str) -> ModRow {
    ModRow::Separator(name.to_owned())
}

/// Collect model indices from projected rows
fn model_indices(rows: &[ModPaneRow]) -> Vec<usize> {
    rows.iter().map(|row| row.model_index()).collect()
}

/// Projection reverses persistence order for MO2 display
#[test]
fn projection_reverses_persistence_order() {
    let mods = vec![managed("High"), separator("Group"), managed("Low")];
    let pane = ModsPane::new(&mods);
    assert_eq!(model_indices(&pane.project(&mods)), [2, 1, 0]);
}

/// Collapse hides only members and retains the total count
#[test]
fn collapse_hides_only_members_and_keeps_total_count() {
    let mods = vec![
        managed("A"),
        managed("B"),
        separator("Gameplay"),
        managed("Texture"),
        separator("Visual"),
    ];
    let mut pane = ModsPane::new(&mods);
    pane.toggle_separator(0);

    assert_eq!(model_indices(&pane.project(&mods)), [4, 3, 2]);
    assert!(matches!(
        pane.project(&mods)[2],
        ModPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            member_count: 2,
            ..
        }
    ));
    assert!(matches!(
        ModsPane::new(&mods).project(&mods)[2],
        ModPaneRow::Separator {
            collapsed: false,
            member_count: 2,
            ..
        }
    ));
}

/// Consecutive separators assign members to the latest separator
#[test]
fn consecutive_separators_give_members_to_the_latest_separator() {
    let mods = vec![managed("Member"), separator("Latest"), separator("Earlier")];
    let pane = ModsPane::new(&mods);
    assert_eq!(
        pane.project(&mods),
        [
            ModPaneRow::Separator {
                name: "Earlier",
                model_index: 2,
                separator_index: 1,
                collapsed: false,
                member_count: 0,
            },
            ModPaneRow::Separator {
                name: "Latest",
                model_index: 1,
                separator_index: 0,
                collapsed: false,
                member_count: 1,
            },
            ModPaneRow::Mod { model_index: 0 },
        ]
    );
}

/// Duplicate labels retain independent collapse state
#[test]
fn duplicate_labels_collapse_independently() {
    let mods = vec![
        managed("A"),
        separator("Duplicate"),
        managed("B"),
        separator("Duplicate"),
    ];
    let mut pane = ModsPane::new(&mods);
    pane.toggle_separator(0);

    assert_eq!(model_indices(&pane.project(&mods)), [3, 2, 1]);
    assert!(matches!(
        pane.project(&mods)[0],
        ModPaneRow::Separator {
            separator_index: 1,
            collapsed: false,
            ..
        }
    ));
    assert!(matches!(
        pane.project(&mods)[2],
        ModPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            ..
        }
    ));
}

/// Rename preserves collapse state by separator index
#[test]
fn rename_retains_collapse_by_separator_index() {
    let mut mods = vec![managed("A"), separator("Old")];
    let mut pane = ModsPane::new(&mods);
    pane.toggle_separator(0);
    mods[1] = separator("New");

    assert!(matches!(
        pane.project(&mods)[0],
        ModPaneRow::Separator {
            collapsed: true,
            ..
        }
    ));
}

/// Recreation inserts expanded collapse state
#[test]
fn delete_then_recreate_inserts_an_expanded_entry() {
    let mut mods = vec![managed("A"), separator("Group")];
    let mut pane = ModsPane::new(&mods);
    pane.toggle_separator(0);

    mods.remove(1);
    pane.remove_separator(0);
    mods.push(separator("Group"));
    pane.insert_separator(0);

    assert!(matches!(
        pane.project(&mods)[0],
        ModPaneRow::Separator {
            collapsed: false,
            ..
        }
    ));
}

#[test]
fn reconcile_preserves_collapse_and_clamps_when_separator_counts_match() {
    let initial = vec![
        managed("A"),
        separator("Group"),
        managed("B"),
        separator("Other"),
    ];
    let replacement = vec![managed("A"), separator("Renamed"), separator("Other")];
    let mut pane = ModsPane::new(&initial);
    pane.toggle_separator(0);
    pane.select(Some(8));

    pane.reconcile_model(&replacement);

    let rows = pane.project(&replacement);
    assert!(rows.iter().any(|row| matches!(
        row,
        ModPaneRow::Separator {
            separator_index: 0,
            collapsed: true,
            ..
        }
    )));
    assert_eq!(pane.index(), Some(rows.len() - 1));
}

#[test]
fn reconcile_resets_mismatched_collapse_before_projection_and_clamps() {
    let initial = vec![managed("A"), separator("Group")];
    let replacement = vec![
        managed("A"),
        separator("Group"),
        managed("B"),
        separator("Other"),
    ];
    let mut pane = ModsPane::new(&initial);
    pane.toggle_separator(0);
    pane.select(Some(8));

    pane.reconcile_model(&replacement);

    let rows = pane.project(&replacement);
    assert!(rows.iter().all(|row| !matches!(
        row,
        ModPaneRow::Separator {
            collapsed: true,
            ..
        }
    )));
    assert_eq!(pane.index(), Some(rows.len() - 1));
    assert_ne!(pane.index(), Some(0));
}

/// Projection rejects mod separator collapse misalignment
#[test]
#[should_panic(expected = "mod separator collapse state must align with profile order")]
fn projection_rejects_collapse_misalignment() {
    let pane = ModsPane::new(&[]);
    pane.project(&[separator("Unexpected")]);
}
