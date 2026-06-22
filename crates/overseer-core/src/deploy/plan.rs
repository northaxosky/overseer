//! Deployment plan generation and file resolution for mod deployment.

use std::collections::BTreeMap;

use super::error::DeployError;
use camino::{Utf8Path, Utf8PathBuf};
use walkdir::WalkDir;

/// A mod as it exists on disk: name + staging directory
#[derive(Debug, Clone)]
pub struct ModSource {
    pub name: String,
    pub staging_dir: Utf8PathBuf,
}

impl ModSource {
    pub fn new(name: impl Into<String>, staging_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            name: name.into(),
            staging_dir: staging_dir.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlannedFile {
    /// Path relative to the target root
    pub relative: Utf8PathBuf,
    /// Absolute path to the source file in the winning mod's staging dir
    pub source: Utf8PathBuf,
    /// Name of the mod that won this path
    pub winner: String,
}

#[derive(Debug, Clone)]
pub struct DeployPlan {
    pub target_root: Utf8PathBuf,
    files: Vec<PlannedFile>,
}

impl DeployPlan {
    /// Build a plan from an ordered list of mods. When two mods provide the same
    /// relative path, the higher-priority (later) one wins. Path comparison is
    /// case-insensitive, like the game filesystem.
    pub fn from_mods(
        target_root: impl Into<Utf8PathBuf>,
        mods: &[ModSource],
    ) -> Result<Self, DeployError> {
        let target_root = target_root.into();
        let mut winners: BTreeMap<String, PlannedFile> = BTreeMap::new();

        for m in mods {
            if !m.staging_dir.is_dir() {
                return Err(DeployError::MissingStaging {
                    mod_name: m.name.clone(),
                    path: m.staging_dir.clone(),
                });
            }
            for entry in WalkDir::new(&m.staging_dir) {
                let entry = entry.map_err(|source| DeployError::Walk {
                    path: m.staging_dir.clone(),
                    source,
                })?;
                if !entry.file_type().is_file() {
                    continue;
                }

                let abs = Utf8Path::from_path(entry.path())
                    .ok_or_else(|| DeployError::NonUtf8Path(entry.path().display().to_string()))?;
                let relative = abs
                    .strip_prefix(&m.staging_dir)
                    .expect("walked entry is always under staging dir")
                    .to_owned();
                let key = relative.as_str().to_lowercase();

                winners.insert(
                    key,
                    PlannedFile {
                        relative,
                        source: abs.to_owned(),
                        winner: m.name.clone(),
                    },
                );
            }
        }

        Ok(Self {
            target_root,
            files: winners.into_values().collect(),
        })
    }

    /// Build a plan rooted at the game directory, honouring the `Root/` convention:
    /// a mod's top-level `Root/` folder deploys to the game root, everything else
    /// under `Data/`.
    pub fn from_rooted_mods(
        game_dir: impl Into<Utf8PathBuf>,
        mods: &[ModSource],
    ) -> Result<Self, DeployError> {
        // Plan as usual (target = the game dir), then rewrite each file's relative
        // path to its real destination: strip a leading `Root/`, or prefix `Data/`.
        let mut plan = Self::from_mods(game_dir, mods)?;
        for file in &mut plan.files {
            let dest = map_root_relative(&file.winner, &file.relative)?;
            file.relative = dest;
        }
        Ok(plan)
    }

    pub fn files(&self) -> &[PlannedFile] {
        &self.files
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Map a staged file's path to its deploy destination, relative to the game root
fn map_root_relative(mod_name: &str, relative: &Utf8Path) -> Result<Utf8PathBuf, DeployError> {
    let mut components = relative.components();
    let under_root = match components.next() {
        Some(first) if first.as_str().eq_ignore_ascii_case("Root") => components.as_path(),
        _ => return Ok(Utf8Path::new("Data").join(relative)),
    };

    // A top level file literally named "Root"
    if under_root.as_str().is_empty() {
        return Ok(Utf8Path::new("Data").join(relative));
    }

    if under_root
        .components()
        .next()
        .is_some_and(|c| c.as_str().eq_ignore_ascii_case("Data"))
    {
        return Err(DeployError::RootDataConflict {
            name: mod_name.to_owned(),
            path: relative.to_owned(),
        });
    }

    Ok(under_root.to_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a temp dir and return the guard (keeps it alive) plus its UTF-8 base path.
    fn temp() -> (TempDir, Utf8PathBuf) {
        let dir = TempDir::new().expect("create temp dir");
        let base = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 temp path");
        (dir, base)
    }

    fn write(path: &Utf8Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parents");
        }
        std::fs::write(path, contents).expect("write file");
    }

    #[test]
    fn empty_mod_list_yields_empty_plan() {
        let (_tmp, base) = temp();
        let plan = DeployPlan::from_mods(&base, &[]).expect("plan builds");
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
        assert_eq!(plan.files().len(), 0);
    }

    #[test]
    fn single_mod_plans_all_its_files() {
        let (_tmp, base) = temp();
        let m = base.join("mods/A");
        write(&m.join("Textures/a.dds"), "a");
        write(&m.join("Meshes/b.nif"), "b");
        let data = base.join("Data");

        let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
        assert_eq!(plan.len(), 2);
        for f in plan.files() {
            assert_eq!(f.winner, "A");
            assert!(
                f.source.starts_with(&m),
                "source lives under the staging dir"
            );
        }
    }

    #[test]
    fn higher_priority_mod_wins_conflict() {
        let (_tmp, base) = temp();
        let a = base.join("mods/A");
        let b = base.join("mods/B");
        write(&a.join("Textures/shared.dds"), "from-a");
        write(&b.join("Textures/shared.dds"), "from-b");
        let data = base.join("Data");

        let plan =
            DeployPlan::from_mods(&data, &[ModSource::new("A", &a), ModSource::new("B", &b)])
                .expect("plan");

        assert_eq!(plan.len(), 1, "the shared path collapses to one winner");
        let winner = &plan.files()[0];
        assert_eq!(winner.winner, "B");
        assert!(winner.source.starts_with(&b));
    }

    #[test]
    fn conflict_resolution_is_case_insensitive() {
        let (_tmp, base) = temp();
        let a = base.join("mods/A");
        let b = base.join("mods/B");
        write(&a.join("Textures/Armor.dds"), "a");
        write(&b.join("textures/armor.dds"), "b");
        let data = base.join("Data");

        let plan =
            DeployPlan::from_mods(&data, &[ModSource::new("A", &a), ModSource::new("B", &b)])
                .expect("plan");

        // Different casing, same logical path on a case-insensitive filesystem.
        assert_eq!(plan.len(), 1);
        let winner = &plan.files()[0];
        assert_eq!(winner.winner, "B");
        // The winner keeps its own casing.
        assert_eq!(winner.relative.file_name(), Some("armor.dds"));
    }

    #[test]
    fn non_conflicting_files_from_multiple_mods_are_unioned() {
        let (_tmp, base) = temp();
        let a = base.join("mods/A");
        let b = base.join("mods/B");
        write(&a.join("x.txt"), "x");
        write(&b.join("y.txt"), "y");
        let data = base.join("Data");

        let plan =
            DeployPlan::from_mods(&data, &[ModSource::new("A", &a), ModSource::new("B", &b)])
                .expect("plan");
        assert_eq!(plan.len(), 2);
    }

    #[test]
    fn missing_staging_directory_is_an_error() {
        let (_tmp, base) = temp();
        let missing = base.join("does/not/exist");
        let data = base.join("Data");

        let err = DeployPlan::from_mods(&data, &[ModSource::new("Ghost", &missing)])
            .expect_err("should fail");
        match err {
            DeployError::MissingStaging { mod_name, path } => {
                assert_eq!(mod_name, "Ghost");
                assert_eq!(path, missing);
            }
            other => panic!("expected MissingStaging, got {other:?}"),
        }
    }

    #[test]
    fn empty_staging_directory_contributes_nothing() {
        let (_tmp, base) = temp();
        let m = base.join("mods/Empty");
        std::fs::create_dir_all(&m).expect("create empty staging");
        let data = base.join("Data");

        let plan = DeployPlan::from_mods(&data, &[ModSource::new("Empty", &m)]).expect("plan");
        assert!(plan.is_empty());
    }

    #[test]
    fn files_are_ordered_deterministically() {
        let (_tmp, base) = temp();
        let m = base.join("mods/A");
        write(&m.join("zeta.txt"), "z");
        write(&m.join("alpha.txt"), "a");
        write(&m.join("mid/beta.txt"), "b");
        let data = base.join("Data");

        let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
        let keys: Vec<String> = plan
            .files()
            .iter()
            .map(|f| f.relative.as_str().to_lowercase())
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(
            keys, sorted,
            "files() is sorted by lowercased path (BTreeMap order)"
        );
    }

    #[test]
    fn nested_paths_are_relative_to_staging_root() {
        let (_tmp, base) = temp();
        let m = base.join("mods/A");
        write(&m.join("a/b/c.txt"), "c");
        let data = base.join("Data");

        let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
        assert_eq!(plan.len(), 1);
        // Build the expectation with join() so the path separator matches the platform.
        let expected = Utf8Path::new("a").join("b").join("c.txt");
        assert_eq!(plan.files()[0].relative, expected);
    }

    #[test]
    fn last_mod_in_a_three_way_conflict_wins() {
        let (_tmp, base) = temp();
        let a = base.join("mods/A");
        let b = base.join("mods/B");
        let c = base.join("mods/C");
        write(&a.join("f.txt"), "a");
        write(&b.join("f.txt"), "b");
        write(&c.join("f.txt"), "c");
        let data = base.join("Data");

        let plan = DeployPlan::from_mods(
            &data,
            &[
                ModSource::new("A", &a),
                ModSource::new("B", &b),
                ModSource::new("C", &c),
            ],
        )
        .expect("plan");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan.files()[0].winner, "C");
    }

    #[test]
    fn target_root_is_recorded_on_the_plan() {
        let (_tmp, base) = temp();
        let m = base.join("mods/A");
        write(&m.join("f.txt"), "f");
        let data = base.join("Game/Data");

        let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
        assert_eq!(plan.target_root, data);
    }

    // --- root deployment (from_rooted_mods) ---

    #[test]
    fn rooted_plan_targets_the_game_dir_and_prefixes_data_content() {
        let (_tmp, base) = temp();
        let m = base.join("mods/M");
        write(&m.join("Textures/x.dds"), "x");
        let game = base.join("Game");

        let plan = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)]).expect("plan");
        assert_eq!(plan.target_root, game);
        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan.files()[0].relative,
            Utf8Path::new("Data").join("Textures").join("x.dds")
        );
    }

    #[test]
    fn rooted_plan_sends_root_content_to_the_game_root() {
        let (_tmp, base) = temp();
        let m = base.join("mods/M");
        write(&m.join("Root/f4se_loader.exe"), "exe");
        write(&m.join("Root/enbseries/enb.ini"), "ini");
        let game = base.join("Game");

        let plan = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)]).expect("plan");
        let relatives: Vec<&Utf8Path> = plan.files().iter().map(|f| f.relative.as_path()).collect();
        // The loose loader lands directly in the game root...
        assert!(relatives.contains(&Utf8Path::new("f4se_loader.exe")));
        // ...and subfolders under Root/ are preserved verbatim.
        assert!(relatives.contains(&Utf8Path::new("enbseries").join("enb.ini").as_path()));
    }

    #[test]
    fn rooted_plan_root_marker_is_case_insensitive() {
        let (_tmp, base) = temp();
        let m = base.join("mods/M");
        write(&m.join("root/dxgi.dll"), "dll");
        let game = base.join("Game");

        let plan = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)]).expect("plan");
        assert_eq!(plan.files()[0].relative, Utf8Path::new("dxgi.dll"));
    }

    #[test]
    fn rooted_plan_rejects_a_data_folder_nested_in_root() {
        let (_tmp, base) = temp();
        let m = base.join("mods/M");
        write(&m.join("Root/Data/Sneaky.esp"), "esp");
        let game = base.join("Game");

        let err = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)])
            .expect_err("Root/Data must be rejected");
        match err {
            DeployError::RootDataConflict { name, .. } => assert_eq!(name, "M"),
            other => panic!("expected RootDataConflict, got {other:?}"),
        }
    }

    #[test]
    fn rooted_plan_keeps_same_named_root_and_data_files_separate() {
        let (_tmp, base) = temp();
        let m = base.join("mods/M");
        write(&m.join("Root/x.dll"), "root-side");
        write(&m.join("x.dll"), "data-side");
        let game = base.join("Game");

        let plan = DeployPlan::from_rooted_mods(&game, &[ModSource::new("M", &m)]).expect("plan");
        assert_eq!(plan.len(), 2, "the two x.dll target different roots");
        let relatives: Vec<&Utf8Path> = plan.files().iter().map(|f| f.relative.as_path()).collect();
        assert!(relatives.contains(&Utf8Path::new("x.dll")));
        assert!(relatives.contains(&Utf8Path::new("Data").join("x.dll").as_path()));
    }
}
