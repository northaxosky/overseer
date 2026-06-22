//! Facts about the setup, gathered once and shared by every check

use crate::error::DiagnosticError;
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::deploy::DeployPlan;
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins, find_plugin_files};
use std::collections::BTreeSet;

/// The state a diagnostic run inspects. Gathered once using [`GameContext::gather`]
pub struct GameContext {
    /// The active mod plugins to inspect (with their masters)
    pub active_plugins: Vec<PluginMeta>,
    /// Lowercased names of every plugin present when the game loads
    pub present_plugins: BTreeSet<String>,
    /// The files this profile would deploy under the game's `Data/` folder
    pub data_files: Vec<DataFile>,
    /// The state of the game's Creation Club manifest
    pub ccc: CccStatus,
}

/// A file that will deploy under the game's `Data/` folder, and the mod it came from
pub struct DataFile {
    /// Path relative to `Data/` (e.g. `textures/foo.dds`)
    pub path: Utf8PathBuf,
    /// The mod that owns this file (the conflict winner)
    pub mod_name: String,
}

/// The state of the game's Creation Club manifest (e.g. `Fallout4.ccc`)
pub enum CccStatus {
    /// This game has no Creation Club manifest
    NotApplicable,
    /// The named manifest should exist in the game folder but doesn't
    Missing { file: &'static str },
    /// The manifest lists these Creation Club plugin filenames, in load order
    Present {
        file: &'static str,
        entries: Vec<String>,
    },
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

        Ok(Self {
            active_plugins,
            present_plugins,
            data_files,
            ccc: read_ccc(instance),
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

/// Keep only deploy paths under `Data/`, returning the path relative to `Data/`
fn strip_data_prefix(relative: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut components = relative.components();
    match components.next() {
        Some(first) if first.as_str().eq_ignore_ascii_case("Data") => {
            Some(components.as_path().to_owned())
        }
        _ => None,
    }
}
