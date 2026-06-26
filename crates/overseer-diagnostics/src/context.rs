//! Facts about the setup, gathered once and shared by every check

use crate::error::DiagnosticError;
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::deploy::{DATA_DIR, DeployPlan, strip_data_prefix};
use overseer_core::ini::{GameInis, read_game_inis};
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins, find_plugin_files};
use std::collections::BTreeSet;

/// The state a diagnostic run inspects. Gathered once using [`GameContext::gather`]
#[derive(Default)]
pub struct GameContext {
    /// The active mod plugins to inspect (with their masters)
    pub active_plugins: Vec<PluginMeta>,
    /// Lowercased names of every plugin present when the game loads
    pub present_plugins: BTreeSet<String>,
    /// The files this profile would deploy under the game's `Data/` folder
    pub data_files: Vec<DataFile>,
    /// The state of the game's Creation Club manifest
    pub ccc: CccStatus,
    /// The game's parsed INIs, if they could be read
    pub inis: Option<GameInis>,
    /// Race subgraph (`SADD`) record counts for active mod plugins
    pub sadd_records: Vec<SaddCount>,
}

/// A file that will deploy under the game's `Data/` folder, and the mod it came from
pub struct DataFile {
    /// Path relative to `Data/` (e.g. `textures/foo.dds`)
    pub path: Utf8PathBuf,
    /// The mod that owns this file (the conflict winner)
    pub mod_name: String,
}

/// The state of the game's Creation Club manifest (e.g. `Fallout4.ccc`)
#[derive(Default)]
pub enum CccStatus {
    /// This game has no Creation Club manifest
    #[default]
    NotApplicable,
    /// The named manifest should exist in the game folder but doesn't
    Missing { file: &'static str },
    /// The manifest lists these Creation Club plugin filenames, in load order
    Present {
        file: &'static str,
        entries: Vec<String>,
    },
}

/// How many race-subgraph (`SADD`) records a plugin contains
pub struct SaddCount {
    /// The plugin's filename
    pub plugin: String,
    /// Number of `SADD` markers found in its bytes
    pub count: usize,
}

impl GameContext {
    /// Gather the context for one profile
    pub fn gather(instance: &Instance, profile: &str) -> Result<Self, DiagnosticError> {
        let mut profile = Profile::load(instance, profile)?;
        profile.reconcile(instance)?;

        let discovered = discover_plugins(instance, &profile)?;
        let mut order = PluginLoadOrder::load(instance, &profile.name)?;
        order.reconcile(&discovered);

        let active_plugins: Vec<PluginMeta> = discovered
            .into_iter()
            .filter(|p| order.is_active(&p.name))
            .collect();

        // Everything a master can resolve against at load time: the real Data/ folder
        // (vanilla + owned DLC + CC) plus the active mod plugins.
        let mut present_plugins: BTreeSet<String> =
            find_plugin_files(&instance.config.game_dir.join("Data"))?
                .iter()
                .filter_map(|p| p.file_name())
                .map(str::to_lowercase)
                .collect();
        present_plugins.extend(active_plugins.iter().map(|p| p.name.to_lowercase()));

        // The files this profile would actually deploy, conflict-resolved. Root/ content
        // deploys to the game root rather than Data/, so it's dropped here.
        let sources = profile.deploy_sources(instance);
        let plan = DeployPlan::from_rooted_mods(&instance.config.game_dir, &sources)?;
        let data_files = plan
            .files()
            .iter()
            .filter_map(|f| {
                strip_data_prefix(&f.relative).map(|path| DataFile {
                    path,
                    mod_name: f.winner.clone(),
                })
            })
            .collect();
        let sadd_records = scan_sadd(&plan, &active_plugins);

        Ok(Self {
            active_plugins,
            present_plugins,
            data_files,
            ccc: read_ccc(instance),
            inis: read_game_inis(instance).ok(),
            sadd_records,
        })
    }
}

/// Read the game's Creation Club manifest, if the game has one. A read error (including
/// a missing file) is reported as [`CccStatus::Missing`] rather than failing the run.
fn read_ccc(instance: &Instance) -> CccStatus {
    let Some(file) = instance.config.game.ccc_file() else {
        return CccStatus::NotApplicable;
    };
    let path = instance.config.game_dir.join(file);
    match std::fs::read_to_string(&path) {
        Ok(text) => CccStatus::Present {
            file,
            entries: text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect(),
        },
        Err(_) => CccStatus::Missing { file },
    }
}

/// Race subgraph (`SADD`) record counts for each active mod plugin that has any
fn scan_sadd(plan: &DeployPlan, active_plugins: &[PluginMeta]) -> Vec<SaddCount> {
    const SADD_MARKER: &[u8] = b"\x00SADD";

    let active: BTreeSet<String> = active_plugins
        .iter()
        .map(|p| p.name.to_lowercase())
        .collect();

    plan.files()
        .iter()
        .filter_map(|file| {
            let name = active_plugins_name(&file.relative, &active)?;
            let bytes = std::fs::read(&file.source).ok()?;
            let count = bytes
                .windows(SADD_MARKER.len())
                .filter(|window| *window == SADD_MARKER)
                .count();
            (count > 0).then(|| SaddCount {
                plugin: name.to_owned(),
                count,
            })
        })
        .collect()
}

/// The filename if `relative` is a top level `Data/<plugin>` path naming an active plugin
fn active_plugins_name<'a>(relative: &'a Utf8Path, active: &BTreeSet<String>) -> Option<&'a str> {
    let mut components = relative.components();
    let data = components.next()?;
    let name = components.next()?.as_str();

    // Must be 2 components: `Data/<plugin>`
    if components.next().is_some() || !data.as_str().eq_ignore_ascii_case(DATA_DIR) {
        return None;
    }
    active.contains(&name.to_lowercase()).then_some(name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use overseer_core::deploy::ModSource;
    use tempfile::TempDir;

    fn active_set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| n.to_lowercase()).collect()
    }

    fn meta(name: &str) -> PluginMeta {
        PluginMeta {
            name: name.to_owned(),
            is_master: false,
            is_light: false,
            masters: Vec::new(),
        }
    }

    // --- active_plugins_name (pure) ---

    #[test]
    fn names_a_top_level_active_plugin() {
        let active = active_set(&["foo.esp"]);
        assert_eq!(
            active_plugins_name(Utf8Path::new("Data/Foo.esp"), &active),
            Some("Foo.esp")
        );
    }

    #[test]
    fn rejects_inactive_nested_and_non_data_paths() {
        let active = active_set(&["foo.esp"]);
        // Not in the active set.
        assert_eq!(
            active_plugins_name(Utf8Path::new("Data/Bar.esp"), &active),
            None
        );
        // Deeper than Data/<plugin>.
        assert_eq!(
            active_plugins_name(Utf8Path::new("Data/meshes/Foo.esp"), &active),
            None
        );
        // Not under Data/.
        assert_eq!(active_plugins_name(Utf8Path::new("Foo.esp"), &active), None);
    }

    #[test]
    fn folder_and_name_match_case_insensitively() {
        let active = active_set(&["foo.esp"]);
        assert_eq!(
            active_plugins_name(Utf8Path::new("data/FOO.ESP"), &active),
            Some("FOO.ESP")
        );
    }

    // --- scan_sadd (real temp-dir plan) ---

    fn temp_base() -> (TempDir, Utf8PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8");
        (tmp, base)
    }

    #[test]
    fn counts_markers_only_in_active_top_level_plugins() {
        let (_tmp, base) = temp_base();
        let mod_dir = base.join("mods/A");
        std::fs::create_dir_all(mod_dir.join("meshes")).unwrap();
        // Two markers in the active plugin; markers elsewhere must be ignored.
        std::fs::write(mod_dir.join("Active.esp"), b"--\x00SADD--\x00SADD--").unwrap();
        std::fs::write(mod_dir.join("Inactive.esp"), b"\x00SADD").unwrap();
        std::fs::write(mod_dir.join("meshes/anim.nif"), b"\x00SADD").unwrap();

        let plan =
            DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("A", &mod_dir)])
                .unwrap();

        let records = scan_sadd(&plan, &[meta("Active.esp")]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].plugin, "Active.esp");
        assert_eq!(records[0].count, 2);
    }

    #[test]
    fn a_plugin_without_markers_is_omitted() {
        let (_tmp, base) = temp_base();
        let mod_dir = base.join("mods/A");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(mod_dir.join("Clean.esp"), b"no markers here").unwrap();

        let plan =
            DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("A", &mod_dir)])
                .unwrap();

        assert!(scan_sadd(&plan, &[meta("Clean.esp")]).is_empty());
    }
}
