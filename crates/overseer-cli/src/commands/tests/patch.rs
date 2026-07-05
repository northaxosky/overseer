//! Tests for the patch CLI plan and summary rendering

use super::*;

const COAST: &[&str] = &[
    "Data/DLCCoast.esm",
    "Data/DLCCoast.cdx",
    "Data/DLCCoast - Geometry.csg",
    "Data/DLCCoast - Main.ba2",
    "Data/DLCCoast - Textures.ba2",
];

fn core_plan_for(rel: &str, state: ItemState) -> ItemPlan {
    ItemPlan {
        item: convert::explicit_item(Generation::OldGen, rel).expect("known core rel_path"),
        state,
        current: None,
        known_source: None,
    }
}

fn dlc_plan_for(rel: &str, state: ItemState) -> ItemPlan {
    ItemPlan {
        item: dlc::explicit_item(rel).expect("known dlc rel_path"),
        state,
        current: None,
        known_source: None,
    }
}

fn deltas(rels: &[&str]) -> HashMap<String, Utf8PathBuf> {
    rels.iter()
        .map(|r| ((*r).to_owned(), Utf8PathBuf::from("d.vcdiff")))
        .collect()
}

#[test]
fn dlc_only_deltas_convert_dlc_and_leave_core_untouched() {
    let mut plans = vec![core_plan_for("Fallout4.exe", ItemState::NeedsConversion)];
    plans.extend(
        COAST
            .iter()
            .map(|r| dlc_plan_for(r, ItemState::NeedsConversion)),
    );
    let (jobs, noop) = build_jobs(
        "the DLC consistency revision",
        &plans,
        &deltas(COAST),
        false,
    )
    .unwrap();
    assert!(!noop);
    assert_eq!(jobs.len(), COAST.len());
    assert!(jobs.iter().all(|j| j.item.group == "DLCCoast"));
}

#[test]
fn a_partial_dlc_group_is_refused() {
    let plans: Vec<_> = COAST
        .iter()
        .map(|r| dlc_plan_for(r, ItemState::NeedsConversion))
        .collect();
    let err = build_jobs(
        "the DLC consistency revision",
        &plans,
        &deltas(&["Data/DLCCoast.esm"]),
        false,
    )
    .unwrap_err();
    assert!(err.to_string().contains("refusing partial group"));
}

#[test]
fn no_deltas_means_nothing_to_convert() {
    let plans = vec![core_plan_for("Fallout4.exe", ItemState::NeedsConversion)];
    assert!(build_jobs("Old-Gen", &plans, &HashMap::new(), false).is_err());
}

#[test]
fn a_fully_converted_selected_group_is_a_noop() {
    let plans: Vec<_> = COAST
        .iter()
        .map(|r| dlc_plan_for(r, ItemState::AlreadyTarget))
        .collect();
    let (jobs, noop) = build_jobs(
        "the DLC consistency revision",
        &plans,
        &deltas(COAST),
        false,
    )
    .unwrap();
    assert!(jobs.is_empty());
    assert!(noop);
}

#[test]
fn a_bare_tool_name_is_searched_on_path_not_absolutized() {
    // A bare name (empty parent) must go through PATH lookup, not be absolutized against the CWD
    let result = resolve_executable(Utf8Path::new("overseer-nonexistent-tool-xyz"));
    assert!(
        result.is_err(),
        "a bare name not on PATH must error, not silently absolutize to the CWD"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("not found on PATH"),
        "the error must come from the PATH lookup branch"
    );
}

#[test]
fn repair_mode_allows_a_partial_group() {
    // --allow-incomplete-repair converts only the delta-supplied file, not refusing the group
    let plans: Vec<_> = COAST
        .iter()
        .map(|r| dlc_plan_for(r, ItemState::NeedsConversion))
        .collect();
    let (jobs, noop) = build_jobs(
        "the DLC consistency revision",
        &plans,
        &deltas(&["Data/DLCCoast.esm"]),
        true,
    )
    .unwrap();
    assert!(!noop);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].item.rel_path, "Data/DLCCoast.esm");
}

#[test]
fn repair_mode_skips_a_missing_file() {
    // A Missing file no longer refuses the group under repair; the rest still convert
    let mut plans = vec![dlc_plan_for("Data/DLCCoast.esm", ItemState::Missing)];
    plans.extend(
        COAST[1..]
            .iter()
            .map(|r| dlc_plan_for(r, ItemState::NeedsConversion)),
    );
    let (jobs, noop) = build_jobs(
        "the DLC consistency revision",
        &plans,
        &deltas(&COAST[1..]),
        true,
    )
    .unwrap();
    assert!(!noop);
    assert_eq!(jobs.len(), COAST.len() - 1);
    assert!(jobs.iter().all(|j| j.item.rel_path != "Data/DLCCoast.esm"));
}

#[test]
fn a_name_with_a_directory_is_absolutized() {
    // A path with a directory component resolves against the CWD, never PATH-searched
    let path = resolve_executable(Utf8Path::new("tools/overseer-fake-xdelta3.exe"))
        .expect("a relative path with a directory must absolutize, not error");
    assert!(
        path.is_absolute(),
        "the resolved tool path must be absolute"
    );
    assert!(
        path.ends_with("tools/overseer-fake-xdelta3.exe"),
        "absolutizing must preserve the supplied relative path"
    );
}
