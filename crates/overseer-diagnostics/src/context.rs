//! Facts about the setup, gathered once and shared by every check

use crate::binaries::{self, BinaryScan};
use crate::error::DiagnosticError;
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::archive::{Ba2Error, Ba2Header, Ba2Kind};
use overseer_core::deploy::{DATA_DIR, DeployPlan, F4SE_PLUGINS_DIR, strip_data_prefix};
use overseer_core::detect::{
    self, Edition, RuntimeFamily, address_library_name, file_version, loader_family,
};
use overseer_core::f4se::{F4seDll, F4sePlugin, parse_f4se_dll};
use overseer_core::game::GameKind;
use overseer_core::ini::{GameInis, read_game_inis};
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{
    PluginLoadOrder, PluginMeta, discover_plugins, implicit_active_plugins, read_metadata,
};
use std::collections::{BTreeMap, BTreeSet};

/// The base F4SE Papyrus scripts that ship loose in `Data/Scripts/` (lowercase `<name>.pex`)
const BASE_SCRIPT_NAMES: &[&str] = &[
    "actor.pex",
    "actorbase.pex",
    "armor.pex",
    "armoraddon.pex",
    "cell.pex",
    "component.pex",
    "constructibleobject.pex",
    "defaultobject.pex",
    "encounterzone.pex",
    "equipslot.pex",
    "f4se.pex",
    "favoritesmanager.pex",
    "form.pex",
    "game.pex",
    "headpart.pex",
    "input.pex",
    "instancedata.pex",
    "location.pex",
    "matswap.pex",
    "math.pex",
    "miscobject.pex",
    "objectmod.pex",
    "objectreference.pex",
    "perk.pex",
    "scriptobject.pex",
    "ui.pex",
    "utility.pex",
    "watertype.pex",
    "weapon.pex",
];
/// The state a diagnostic run inspects. Gathered once using [`GameContext::gather`]
#[derive(Default)]
pub struct GameContext {
    /// The active mod plugins to inspect (with their masters)
    pub active_plugins: Vec<PluginMeta>,
    /// The real load-order budget: active mod plugins plus force-loaded base/DLC/Creation Club plugins.
    pub loaded_plugins: Vec<PluginMeta>,
    /// The files this profile would deploy under the game's `Data/` folder
    pub data_files: Vec<DataFile>,
    /// The state of the game's Creation Club manifest
    pub ccc: CccStatus,
    /// The game's parsed INIs, if they could be read
    pub inis: Option<GameInis>,
    /// Race subgraph (`SADD`) record counts for active mod plugins
    pub sadd_records: Vec<SaddCount>,
    /// BA2 archives in the profile's deploy set, with their headers
    pub archives: Vec<ArchiveInfo>,
    /// Runtime family the game exe targets (OG/NG/AE), if recognised
    pub runtime_family: Option<RuntimeFamily>,
    /// Runtime family the installed F4SE loader targets, if present and recognised
    pub loader_family: Option<RuntimeFamily>,
    /// Whether the Address Library version file is present (only when F4SE plugins are deployed)
    pub address_library: AddressLibraryStatus,
    /// Deployed F4SE plugin DLLs and what runtime each advertises
    pub f4se_plugins: Vec<F4sePluginScan>,
    /// The game exe's runtime packed for matching plugin `compatibleVersions`, if known
    pub runtime_packed: Option<u32>,
    /// Loaded BA2 counts across base game archives and active mod archives
    pub loaded_archive_counts: LoadedArchiveCounts,
    /// Loose top-level `Data/Scripts/*.pex` files that shadow a base F4SE script
    pub script_overrides: Vec<ScriptOverrideScan>,
    /// The detected Fallout 4 edition, or `None` when the game isn't Fallout 4
    pub game_edition: Option<Edition>,
    /// The core game binaries (`Fallout4Launcher.exe`, `steam_api64.dll`) and their generation
    pub binaries: Vec<BinaryScan>,
}

/// Loaded BA2 counts split by content kind and Fallout 4 archive generation
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoadedArchiveCounts {
    /// Loaded `GNRL` archives
    pub gnrl: usize,
    /// Loaded `DX10` archives
    pub dx10: usize,
    /// Loaded version 1 archives
    pub v1: usize,
    /// Loaded version 7/8 archives
    pub vng: usize,
}

/// A deployed F4SE plugin DLL and the runtime support it advertises
pub struct F4sePluginScan {
    /// File name, e.g. `Buffout4.dll`
    pub name: String,
    /// The mod that owns it
    pub mod_name: String,
    /// What its PE exports / version data reveal
    pub plugin: F4sePlugin,
}

/// Whether the F4SE Address Library is in place. Only meaningful when F4SE plugins are deployed
#[derive(Default, PartialEq, Eq)]
pub enum AddressLibraryStatus {
    /// No F4SE plugins are deployed, so it isn't needed
    #[default]
    NotApplicable,
    /// The expected `version-*.bin` was found
    Present,
    /// F4SE plugins are deployed but the expected file is missing
    Missing { expected: String },
}

/// A file that will deploy under the game's `Data/` folder, and the mod it came from
pub struct DataFile {
    /// Path relative to `Data/` (e.g. `textures/foo.dds`)
    pub path: Utf8PathBuf,
    /// The mod that owns this file (the conflict winner)
    pub mod_name: String,
}

/// A BA2 archive in the profile's deploy set, with its scanned header
pub struct ArchiveInfo {
    /// File name, e.g. `Textures.ba2`
    pub name: String,
    /// The mod that owns it (conflict winner)
    pub mod_name: String,
    /// Path relative to the game root, including the `Data/`/`Root/` prefix
    pub relative: Utf8PathBuf,
    /// What reading its header found
    pub scan: ArchiveScan,
}

/// A loose top-level `Data/Scripts/<name>.pex` that shadows a base F4SE script
pub struct ScriptOverrideScan {
    /// File name, e.g. `Actor.pex`
    pub name: String,
    /// The mod that owns it (conflict winner) — not the F4SE package
    pub mod_name: String,
}

/// The outcome of reading a BA2 header during gather
pub enum ArchiveScan {
    /// Header parsed successfully
    Header(Ba2Header),
    /// Present but not a valid BA2 (bad magic or too short)
    Invalid,
    /// Could not be read (IO error); message kept for diagnosis
    Unreadable(String),
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

        // The files this profile would actually deploy, conflict-resolved
        let sources = profile.deploy_sources(instance);
        let plan = DeployPlan::from_rooted_mods(&instance.config.game_dir, &sources)?;
        let data_files: Vec<DataFile> = plan
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
        let archives = scan_archives(&plan);

        // What the engine force loads (base + dlc + cc)
        let data_dir = instance.config.game_dir.join(DATA_DIR);
        let plugin_id = instance.config.game.plugin_id();
        let mut loaded_plugins: Vec<PluginMeta> = Vec::new();

        if let Ok(local_dir) = instance.local_dir() {
            let game_id = instance.config.game.load_order_id();
            for name in implicit_active_plugins(game_id, &instance.config.game_dir, &local_dir)? {
                let path = data_dir.join(&name);
                if path.exists() {
                    loaded_plugins.push(read_metadata(plugin_id, &name, &path)?);
                }
            }
        }
        loaded_plugins.extend(active_plugins.iter().cloned());
        let loaded_archive_counts = scan_loaded_archive_counts(&data_dir, &plan, &loaded_plugins);

        // F4SE health: the game's runtime family, loader's family, Address Library
        let install = detect::detect(instance.config.game, &instance.config.game_dir);
        let runtime_family = install.version.and_then(detect::runtime_family);
        let loader_family = file_version(
            &instance
                .config
                .game_dir
                .join(instance.config.game.script_extender_loader()),
        )
        .and_then(loader_family);
        let address_library = address_library_status(&data_files, install.version);
        let f4se_plugins = scan_f4se_plugins(&plan);
        let runtime_packed = install.version.map(detect::packed_runtime);
        let script_overrides = scan_script_overrides(&plan);

        // Core binary consistency: only meaningful for Fallout 4 currently
        let game_edition = (instance.config.game == GameKind::Fallout4)
            .then(|| detect::edition(&install, &instance.config.game_dir));
        let binaries = game_edition
            .map(|_| binaries::scan(&instance.config.game_dir))
            .unwrap_or_default();

        Ok(Self {
            active_plugins,
            data_files,
            ccc: read_ccc(instance),
            inis: read_game_inis(instance).ok(),
            sadd_records,
            loaded_plugins,
            archives,
            runtime_family,
            loader_family,
            address_library,
            f4se_plugins,
            runtime_packed,
            loaded_archive_counts,
            script_overrides,
            game_edition,
            binaries,
        })
    }
}

/// Parse every `F4SE/Plugins/*.dll` the profile would deploy, attributed to its mod
fn scan_f4se_plugins(plan: &DeployPlan) -> Vec<F4sePluginScan> {
    plan.files()
        .iter()
        .filter(|f| {
            f.relative
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
                && strip_data_prefix(&f.relative).is_some_and(|p| p.starts_with(F4SE_PLUGINS_DIR))
        })
        .filter_map(|f| match parse_f4se_dll(&std::fs::read(&f.source).ok()?) {
            F4seDll::Plugin(plugin) => Some(F4sePluginScan {
                name: f.relative.file_name().unwrap_or_default().to_owned(),
                mod_name: f.winner.clone(),
                plugin,
            }),
            _ => None,
        })
        .collect()
}

/// Gate the Address Library on deployed F4SE plugins: any `F4SE/Plugins/*.dll` requires the matching `version-*.bin`.
fn address_library_status(
    data_files: &[DataFile],
    version: Option<overseer_core::detect::ExeVersion>,
) -> AddressLibraryStatus {
    let under_plugins = |f: &DataFile| f.path.starts_with(F4SE_PLUGINS_DIR);
    let has_plugin = data_files.iter().any(|f| {
        under_plugins(f)
            && f.path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
    });
    if !has_plugin {
        return AddressLibraryStatus::NotApplicable;
    }
    let Some(expected) = version.map(address_library_name) else {
        return AddressLibraryStatus::NotApplicable;
    };
    let present = data_files.iter().any(|f| {
        f.path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case(&expected))
    });
    if present {
        AddressLibraryStatus::Present
    } else {
        AddressLibraryStatus::Missing { expected }
    }
}

/// Read the game's Creation Club manifest, reporting any read error as [`CccStatus::Missing`] instead of failing.
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

/// Read the header of every `.ba2` the profile would deploy
fn scan_archives(plan: &DeployPlan) -> Vec<ArchiveInfo> {
    plan.files()
        .iter()
        .filter(|f| {
            f.relative
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ba2"))
        })
        .map(|f| ArchiveInfo {
            name: f.relative.file_name().unwrap_or_default().to_owned(),
            mod_name: f.winner.clone(),
            relative: f.relative.clone(),
            scan: match Ba2Header::read(&f.source) {
                Ok(header) => ArchiveScan::Header(header),
                Err(Ba2Error::BadMagic | Ba2Error::TooShort) => ArchiveScan::Invalid,
                Err(Ba2Error::Io(e)) => ArchiveScan::Unreadable(e.to_string()),
            },
        })
        .collect()
}

/// Top-level `Data/Scripts/*.pex` base scripts whose winner isn't the F4SE package (the mod that
/// provides the most base scripts). Robust to F4SE version changes — no CRCs to keep current
fn scan_script_overrides(plan: &DeployPlan) -> Vec<ScriptOverrideScan> {
    let candidates: Vec<(&str, &str)> = plan
        .files()
        .iter()
        .filter_map(|f| Some((base_script_pex_name(&f.relative)?, f.winner.as_str())))
        .collect();

    let Some(provider) = dominant_provider(&candidates) else {
        return Vec::new();
    };
    candidates
        .iter()
        .filter(|(_, winner)| *winner != provider)
        .map(|(name, winner)| ScriptOverrideScan {
            name: (*name).to_owned(),
            mod_name: (*winner).to_owned(),
        })
        .collect()
}

/// The mod that provides the most base scripts — the F4SE package. `None` on no candidates or a tie
/// (no single mod dominates), so an ambiguous set never flags an arbitrary "override"
fn dominant_provider<'a>(candidates: &[(&str, &'a str)]) -> Option<&'a str> {
    let mut counts: BTreeMap<&'a str, usize> = BTreeMap::new();
    for &(_, winner) in candidates {
        *counts.entry(winner).or_default() += 1;
    }
    let max = counts.values().copied().max()?;
    let mut leaders = counts.into_iter().filter(|&(_, n)| n == max);
    match (leaders.next(), leaders.next()) {
        (Some((provider, _)), None) => Some(provider),
        _ => None,
    }
}

///The filename if `relative` is a top-level `Data/Scripts/<name>.pex` naming a base script
fn base_script_pex_name(relative: &Utf8Path) -> Option<&str> {
    strip_data_prefix(relative)?;
    let mut components = relative.components();
    components.next();
    let scripts = components.next()?;
    let file = components.next()?.as_str();
    if components.next().is_some() || !scripts.as_str().eq_ignore_ascii_case("scripts") {
        return None;
    }
    let is_pex = Utf8Path::new(file)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pex"));
    (is_pex && BASE_SCRIPT_NAMES.contains(&file.to_lowercase().as_str())).then_some(file)
}

/// Count loaded BA2 archives from `Data/` plus the current deploy plan
fn scan_loaded_archive_counts(
    data_dir: &Utf8Path,
    plan: &DeployPlan,
    loaded_plugins: &[PluginMeta],
) -> LoadedArchiveCounts {
    let loaded_stems: BTreeSet<String> = loaded_plugins
        .iter()
        .filter_map(|p| plugin_stem(&p.name))
        .collect();
    if loaded_stems.is_empty() {
        return LoadedArchiveCounts::default();
    }

    // Candidates keyed by lowercased filename
    let mut candidates: BTreeMap<String, Utf8PathBuf> = BTreeMap::new();
    for path in data_dir_archive_paths(data_dir) {
        if let Some(name) = path.file_name() {
            candidates.insert(name.to_lowercase(), path);
        }
    }
    for file in plan.files() {
        let Some(rel) = strip_data_prefix(&file.relative) else {
            continue;
        };
        if rel.components().count() != 1
            || !rel
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ba2"))
        {
            continue;
        }
        if let Some(name) = rel.file_name() {
            candidates.insert(name.to_lowercase(), file.source.clone());
        }
    }

    let mut counts = LoadedArchiveCounts::default();
    for (name, path) in candidates {
        let Some(stem) = archive_plugin_stem(&name) else {
            continue;
        };
        if !loaded_stems.contains(&stem) {
            continue;
        }
        let Ok(header) = Ba2Header::read(&path) else {
            continue;
        };
        match header.kind {
            Ba2Kind::General => counts.gnrl += 1,
            Ba2Kind::Texture => counts.dx10 += 1,
            Ba2Kind::Other(_) => continue,
        }
        match header.version {
            1 => counts.v1 += 1,
            7 | 8 => counts.vng += 1,
            _ => {}
        }
    }
    counts
}

/// Enumerate top-level `.ba2` files in the game `Data/` directory
fn data_dir_archive_paths(data_dir: &Utf8Path) -> Vec<Utf8PathBuf> {
    let Ok(entries) = std::fs::read_dir(data_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.path()).ok())
        .filter(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ba2"))
        })
        .collect()
}

/// Lowercase plugin stem from a plugin filename
fn plugin_stem(name: &str) -> Option<String> {
    let path = Utf8Path::new(name);
    let is_plugin = path.extension().is_some_and(|e| {
        e.eq_ignore_ascii_case("esp")
            || e.eq_ignore_ascii_case("esm")
            || e.eq_ignore_ascii_case("esl")
    });
    is_plugin.then(|| path.file_stem().map(str::to_lowercase))?
}

/// Lowercase loaded-plugin stem implied by a BA2 filename
fn archive_plugin_stem(name: &str) -> Option<String> {
    let path = Utf8Path::new(name);
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ba2"))
    {
        return None;
    }
    let stem = path.file_stem()?;
    let base = stem.split_once(" - ").map_or(stem, |(base, _)| base);
    Some(base.to_lowercase())
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
    use overseer_core::game::GameKind;
    use overseer_core::test_support::{
        FLAG_MASTER, ba2_bytes, install_plugin, save_profile, temp as temp_base, write_plugin,
    };
    use tempfile::TempDir;

    fn active_set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| n.to_lowercase()).collect()
    }

    fn meta(name: &str) -> PluginMeta {
        overseer_core::test_support::plugin_meta(name, false, false, &[])
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

    // --- script overrides (provenance: the F4SE package vs. other mods) ---

    #[test]
    fn base_script_pex_name_accepts_only_top_level_base_scripts() {
        let ok = |p| base_script_pex_name(Utf8Path::new(p));
        assert_eq!(ok("Data/Scripts/Actor.pex"), Some("Actor.pex"));
        // Folder + extension match case-insensitively; the returned name keeps its casing.
        assert_eq!(ok("Data/scripts/ACTOR.PEX"), Some("ACTOR.PEX"));
        // Not one of the base script names.
        assert_eq!(ok("Data/Scripts/MyCustom.pex"), None);
        // Nested below Scripts/ — the engine path differs, out of scope.
        assert_eq!(ok("Data/Scripts/source/Actor.pex"), None);
        // A base name but not under Scripts/, or not under Data/ at all.
        assert_eq!(ok("Data/Actor.pex"), None);
        assert_eq!(ok("Root/Actor.pex"), None);
    }

    #[test]
    fn the_base_script_list_has_twenty_nine_unique_entries() {
        let names: BTreeSet<&str> = BASE_SCRIPT_NAMES.iter().copied().collect();
        assert_eq!(names.len(), 29);
    }

    #[test]
    fn dominant_provider_picks_the_biggest_supplier() {
        assert_eq!(dominant_provider(&[]), None);
        assert_eq!(
            dominant_provider(&[
                ("actor.pex", "F4SE"),
                ("game.pex", "F4SE"),
                ("form.pex", "Other"),
            ]),
            Some("F4SE")
        );
        // A tie has no clear F4SE package, so no mod is treated as the provider.
        assert_eq!(
            dominant_provider(&[("actor.pex", "A"), ("game.pex", "B")]),
            None
        );
    }

    fn write_file(path: &Utf8Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn the_f4se_package_alone_reports_no_overrides() {
        // The mod that ships the base scripts is the F4SE package — its own scripts are not
        // overrides, whatever their bytes (this is the AE / newer-F4SE case that must stay silent).
        let (_tmp, base) = temp_base();
        let f4se = base.join("mods/F4SE");
        write_file(&f4se.join("Scripts/Actor.pex"), b"ae bytes");
        write_file(&f4se.join("Scripts/Game.pex"), b"ae bytes");
        write_file(&f4se.join("Scripts/Form.pex"), b"ae bytes");

        let plan =
            DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("F4SE", &f4se)])
                .unwrap();
        assert!(scan_script_overrides(&plan).is_empty());
    }

    #[test]
    fn a_base_script_from_a_non_provider_mod_is_flagged() {
        let (_tmp, base) = temp_base();
        let f4se = base.join("mods/F4SE");
        write_file(&f4se.join("Scripts/Actor.pex"), b"f4se");
        write_file(&f4se.join("Scripts/Game.pex"), b"f4se");
        write_file(&f4se.join("Scripts/Form.pex"), b"f4se");
        // A different mod ships a base script — an override the F4SE package doesn't own.
        let other = base.join("mods/Other");
        write_file(&other.join("Scripts/Weapon.pex"), b"override");

        let plan = DeployPlan::from_rooted_mods(
            base.join("game"),
            &[
                ModSource::new("F4SE", &f4se),
                ModSource::new("Other", &other),
            ],
        )
        .unwrap();
        let scans = scan_script_overrides(&plan);

        assert_eq!(
            scans.len(),
            1,
            "only the non-provider's base script is flagged"
        );
        assert_eq!(scans[0].name, "Weapon.pex");
        assert_eq!(scans[0].mod_name, "Other");
    }

    // --- gather: installed implicit (base/DLC/CC) plugins (real temp-dir install) ---

    /// A fake Fallout 4 install with temp local/INI dirs away from real `%LOCALAPPDATA%`/Documents, plus empty `Data/`.
    fn fake_install() -> (TempDir, Instance) {
        let (tmp, base) = temp_base();
        let mut instance = Instance::new(base.join("instance"), base.join("game"));
        instance.config.game = GameKind::Fallout4;
        instance.config.local_dir = Some(base.join("local"));
        instance.config.ini_dir = Some(base.join("ini"));
        std::fs::create_dir_all(instance.config.game_dir.join("Data")).unwrap();
        std::fs::create_dir_all(instance.mods_dir()).unwrap();
        (tmp, instance)
    }

    fn install_game_plugin(instance: &Instance, name: &str, flags: u32) {
        write_plugin(&instance.config.game_dir.join("Data"), name, flags, &[]);
    }

    fn write_ba2(path: &Utf8Path, version: u32, tag: &[u8; 4]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, ba2_bytes(version, tag, b"")).unwrap();
    }

    #[test]
    fn gather_loads_only_installed_implicit_plugins() {
        let (_tmp, instance) = fake_install();
        // The base master, one owned DLC, and a Creation Club plugin are installed.
        install_game_plugin(&instance, "Fallout4.esm", FLAG_MASTER);
        install_game_plugin(&instance, "DLCCoast.esm", FLAG_MASTER);
        install_game_plugin(&instance, "ccBGSFO4001-PipBoy.esl", 0);
        std::fs::write(
            instance.config.game_dir.join("Fallout4.ccc"),
            "ccBGSFO4001-PipBoy.esl\n",
        )
        .unwrap();

        let ctx = GameContext::gather(&instance, "Default").expect("gather");
        let names: Vec<&str> = ctx.loaded_plugins.iter().map(|p| p.name.as_str()).collect();

        assert!(names.contains(&"Fallout4.esm"), "base master force-loads");
        assert!(names.contains(&"DLCCoast.esm"), "owned DLC force-loads");
        assert!(
            names.contains(&"ccBGSFO4001-PipBoy.esl"),
            "CC plugin from Fallout4.ccc force-loads"
        );
        // An implicit candidate that isn't installed must not be counted.
        assert!(
            !names.contains(&"DLCNukaWorld.esm"),
            "an uninstalled DLC does not load"
        );

        // The budget the engine actually sees: 2 full ESMs + 1 light ESL.
        let full = ctx.loaded_plugins.iter().filter(|p| !p.is_light).count();
        let light = ctx.loaded_plugins.iter().filter(|p| p.is_light).count();
        assert_eq!(full, 2, "Fallout4.esm + DLCCoast.esm");
        assert_eq!(light, 1, "the CC .esl");
    }

    #[test]
    fn gather_counts_loaded_archives_and_excludes_inactive_plugin_archives() {
        let (_tmp, instance) = fake_install();
        install_game_plugin(&instance, "Fallout4.esm", FLAG_MASTER);
        write_ba2(
            &instance
                .config
                .game_dir
                .join("Data/Fallout4 - Textures1.ba2"),
            1,
            b"DX10",
        );

        install_plugin(&instance, "ActiveMod", "Active.esp");
        write_ba2(
            &instance.mods_dir().join("ActiveMod/Active - Main.ba2"),
            7,
            b"GNRL",
        );
        // A nested archive is not top-level in Data/, so the engine won't auto-load it —
        // even though its basename matches the active plugin. Must not be counted.
        write_ba2(
            &instance
                .mods_dir()
                .join("ActiveMod/textures/Active - Extra.ba2"),
            7,
            b"GNRL",
        );
        install_plugin(&instance, "InactiveMod", "Inactive.esp");
        write_ba2(
            &instance.mods_dir().join("InactiveMod/Inactive - Main.ba2"),
            8,
            b"GNRL",
        );
        save_profile(
            &instance,
            "Default",
            &[("ActiveMod", true), ("InactiveMod", true)],
        );
        std::fs::write(
            instance.profile_dir("Default").join("plugins.txt"),
            "*Active.esp\nInactive.esp\n",
        )
        .unwrap();

        let ctx = GameContext::gather(&instance, "Default").expect("gather");

        assert_eq!(
            ctx.loaded_archive_counts,
            LoadedArchiveCounts {
                gnrl: 1,
                dx10: 1,
                v1: 1,
                vng: 1,
            },
            "base + active-mod archives count; the inactive-plugin and nested archives do not"
        );
    }

    #[test]
    fn archive_plugin_stem_strips_only_the_basename_prefix() {
        assert_eq!(
            archive_plugin_stem("Fallout4 - Textures1.ba2").as_deref(),
            Some("fallout4")
        );
        assert_eq!(
            archive_plugin_stem("MyMod - Main.ba2").as_deref(),
            Some("mymod")
        );
        assert_eq!(archive_plugin_stem("MyMod.ba2").as_deref(), Some("mymod"));
        assert_eq!(archive_plugin_stem("MyMod.txt"), None);
    }
}
