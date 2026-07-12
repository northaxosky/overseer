//! Tests for operation status rendering

use super::*;

fn progress(
    completed: usize,
    total: usize,
    current: Option<&str>,
    finished: bool,
) -> OperationProgress {
    OperationProgress {
        completed,
        total,
        current: current.map(camino::Utf8PathBuf::from),
        finished,
    }
}

#[test]
fn determinate_line_includes_bar_count_phase_and_relative_path() {
    let progress = progress(3, 10, Some("Textures/Actors/a.dds"), false);

    let line = progress_line(&progress, "Deploying and backing up", 100);

    assert!(line.contains("[###"));
    assert!(line.contains("3/10"));
    assert!(line.contains("Deploying and backing up"));
    assert!(line.contains("Textures/Actors/a.dds"));
}

#[test]
fn zero_total_bar_is_empty_before_finish_and_full_after_finish() {
    let before = progress(0, 0, None, false);
    let after = progress(0, 0, None, true);

    assert_eq!(before.fraction(), 0.0);
    assert_eq!(after.fraction(), 1.0);
    assert_eq!(progress_bar(before.fraction(), 8), "--------");
    assert_eq!(progress_bar(after.fraction(), 8), "########");
    assert!(progress_line(&before, "Planning deployment", 40).contains("0/0"));
    assert!(progress_line(&after, "Finalizing", 40).contains("0/0"));
}

#[test]
fn narrow_progress_line_preserves_the_tail_of_long_paths() {
    let progress = progress(
        9,
        10,
        Some("Textures/Architecture/Institute/UniqueTail.dds"),
        false,
    );

    let line = progress_line(&progress, "Deploying and backing up", 36);

    assert!(line.chars().count() <= 36);
    assert!(line.contains("9/10"));
    assert!(line.ends_with("Tail.dds"), "{line}");
    assert!(line.contains("Deplo"), "{line}");
}

#[test]
fn ellipsize_preserves_unicode_width_at_both_positions() {
    let text = "αβγδε";

    assert_eq!(ellipsize(text, 4, EllipsisPosition::Leading), "…γδε");
    assert_eq!(ellipsize(text, 4, EllipsisPosition::Trailing), "αβγ…");
    assert_eq!(ellipsize(text, 1, EllipsisPosition::Leading), "…");
    assert_eq!(ellipsize(text, 0, EllipsisPosition::Trailing), "");
}
