//! Facts about the setup, gathered once and shared by every check

use crate::binaries::{self, BinaryScan};
use crate::error::DiagnosticError;
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::archive::{Ba2Error, Ba2Header, Ba2Kind};
use overseer_core::deploy::{DATA_DIR, DeployPlan, F4SE_PLUGINS_DIR, strip_data_prefix};
use overseer_core::detect::{
    self, Edition, Generation, address_library_name, file_version, loader_family,
};
use overseer_core::f4se::{F4seDll, F4sePlugin, parse_f4se_dll};
use overseer_core::game::GameKind;
use overseer_core::ini::{GameInis, IniError, read_game_inis};
use overseer_core::instance::{Instance, Profile};
use overseer_core::patch::fallout4::dlc;
use overseer_core::patch::fingerprint::fingerprint_file;
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
pub(crate) struct GameContext {
    /// The active mod plugins to inspect (with their masters)
    pub active_plugins: Vec<PluginMeta>,
    /// The real load-order budget: active mod plugins plus force-loaded base/DLC/Creation Club plugins
    pub loaded_plugins: Vec<PluginMeta>,
    /// The files this profile would deploy under the game's `Data/` folder
    pub data_files: Vec<DataFile>,
    /// The state of the game's Creation Club manifest
    pub ccc: CccStatus,
    /// The game's parsed INIs, if they could be read
    pub inis: Option<GameInis>,
    /// Whether the game INIs were found, missing, or unreadable
    pub ini_status: IniStatus,
    /// Race subgraph (`SADD`) record counts for active mod plugins
    pub sadd_records: Vec<SaddCount>,
    /// BA2 archives in the profile's deploy set, with their headers
    pub archives: Vec<ArchiveInfo>,
    /// Runtime family the game exe targets (OG/NG/AE), if recognised
    pub runtime_family: Option<Generation>,
    /// Runtime family the installed F4SE loader targets, if present and recognised
    pub loader_family: Option<Generation>,
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
    /// Installed DLC groups, each with any files not at the cross-storefront consistency revision
    pub dlc_consistency: Vec<DlcGroupState>,
}

/// Loaded BA2 counts split by content kind and Fallout 4 archive generation
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct LoadedArchiveCounts {
    /// Loaded `GNRL` archives
    pub gnrl: usize,
    /// Loaded `DX10` archives
    pub dx10: usize,
    /// Loaded version 1 archives
    pub v1: usize,
    /// Loaded version 7/8 archives
    pub vng: usize,
}

/// An installed DLC group and any of its files not at the consistency revision
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DlcGroupState {
    /// The DLC group name (e.g. `DLCCoast`)
    pub group: &'static str,
    /// Present files whose on-disk identity differs from the consistency revision
    pub off_revision: Vec<&'static str>,
    /// Required files absent or unreadable in an otherwise-installed group
    pub missing: Vec<&'static str>,
}

/// A deployed F4SE plugin DLL and the runtime support it advertises
pub(crate) struct F4sePluginScan {
    /// File name, e.g. `Buffout4.dll`
    pub name: String,
    /// The mod that owns it
    pub mod_name: String,
    /// What its PE exports / version data reveal
    pub plugin: F4sePlugin,
}

/// Whether the F4SE Address Library is in place. Only meaningful when F4SE plugins are deployed
#[derive(Default, PartialEq, Eq)]
pub(crate) enum AddressLibraryStatus {
    /// No F4SE plugins are deployed, so it isn't needed
    #[default]
    NotApplicable,
    /// The expected `version-*.bin` was found
    Present,
    /// F4SE plugins are deployed but the expected file is missing
    Missing { expected: String },
}

/// A file that will deploy under the game's `Data/` folder, and the mod it came from
pub(crate) struct DataFile {
    /// Path relative to `Data/` (e.g. `textures/foo.dds`)
    pub path: Utf8PathBuf,
    /// The mod that owns this file (the conflict winner)
    pub mod_name: String,
}

/// A BA2 archive in the profile's deploy set, with its scanned header
pub(crate) struct ArchiveInfo {
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
pub(crate) struct ScriptOverrideScan {
    /// File name, e.g. `Actor.pex`
    pub name: String,
    /// The mod that owns it (conflict winner) — not the F4SE package
    pub mod_name: String,
}

/// The outcome of reading a BA2 header during gather
pub(crate) enum ArchiveScan {
    /// Header parsed successfully
    Header(Ba2Header),
    /// Present but not a valid BA2 (bad magic or too short)
    Invalid,
    /// Could not be read (IO error); message kept for diagnosis
    Unreadable(String),
}

/// The state of the game's INI files
#[derive(Default)]
pub(crate) enum IniStatus {
    /// INIs were read successfully
    Present,
    /// INIs are not configured for this instance
    #[default]
    Missing,
    /// INIs exist conceptually but could not be read
    Unreadable(String),
}

/// The state of the game's Creation Club manifest (e.g. `Fallout4.ccc`)
#[derive(Default)]
pub(crate) enum CccStatus {
    /// This game has no Creation Club manifest
    #[default]
    NotApplicable,
    /// The named manifest should exist in the game folder but doesn't
    Missing { file: &'static str },
    /// The manifest exists conceptually but could not be read
    Unreadable { file: &'static str, error: String },
    /// The manifest lists these Creation Club plugin filenames, in load order
    Present {
        file: &'static str,
        entries: Vec<String>,
    },
}

/// How many race-subgraph (`SADD`) records a plugin contains
pub(crate) struct SaddCount {
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
        let (inis, ini_status) = match read_game_inis(instance) {
            Ok(inis) => (Some(inis), IniStatus::Present),
            Err(IniError::Instance(_)) => (None, IniStatus::Missing),
            Err(IniError::Io(error)) => (None, IniStatus::Unreadable(error.to_string())),
        };
        let dlc_consistency = if instance.config.game == GameKind::Fallout4 {
            scan_dlc_consistency(&instance.config.game_dir)
        } else {
            Vec::new()
        };

        Ok(Self {
            active_plugins,
            data_files,
            ccc: read_ccc(instance),
            inis,
            ini_status,
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
            dlc_consistency,
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
        .filter_map(|f| {
            let bytes = match std::fs::read(&f.source) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(path = %f.source, error = %e, "skipping unreadable F4SE plugin");
                    return None;
                }
            };
            match parse_f4se_dll(&bytes) {
                F4seDll::Plugin(plugin) => Some(F4sePluginScan {
                    name: f.relative.file_name().unwrap_or_default().to_owned(),
                    mod_name: f.winner.clone(),
                    plugin,
                }),
                _ => None,
            }
        })
        .collect()
}

/// Gate the Address Library on deployed F4SE plugins: any `F4SE/Plugins/*.dll` requires the matching `version-*.bin`
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
        under_plugins(f)
            && f.path
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(&expected))
    });
    if present {
        AddressLibraryStatus::Present
    } else {
        AddressLibraryStatus::Missing { expected }
    }
}

/// Read the game's Creation Club manifest without failing the whole diagnostic run
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
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => CccStatus::Missing { file },
        Err(error) => CccStatus::Unreadable {
            file,
            error: error.to_string(),
        },
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

/// Base `Data/Scripts/*.pex` whose winner isn't the F4SE package (the mod providing the most base scripts); provenance-based, so robust to F4SE version changes (no CRCs)
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

/// The mod providing the most base scripts (the F4SE package); `None` on no candidates or a tie, so an ambiguous set never flags an arbitrary override
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
    let mut components = relative.components();
    let data = components.next()?;
    let scripts = components.next()?;
    let file = components.next()?.as_str();
    if components.next().is_some()
        || !data.as_str().eq_ignore_ascii_case(DATA_DIR)
        || !scripts.as_str().eq_ignore_ascii_case("scripts")
    {
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

/// Survey installed DLC groups for the consistency revision: `.ba2` by size, others by SHA
fn scan_dlc_consistency(game_dir: &Utf8Path) -> Vec<DlcGroupState> {
    dlc::DLC_GROUPS
        .iter()
        .filter(|group| match group.is_owned(game_dir) {
            Ok(owned) => owned,
            Err(e) => {
                tracing::warn!(group = %group.name, error = %e, "skipping DLC group with unreadable ownership");
                false
            }
        })
        .map(|group| {
            let mut off_revision = Vec::new();
            let mut missing = Vec::new();
            for rel in group.files.iter().copied() {
                match file_revision_state(game_dir, rel) {
                    Some(true) => {}
                    Some(false) => off_revision.push(rel),
                    None => missing.push(rel),
                }
            }
            DlcGroupState {
                group: group.name,
                off_revision,
                missing,
            }
        })
        .collect()
}

/// Whether a DLC file is present and at its consistency-revision identity: archives by size, others by SHA
fn file_revision_state(game_dir: &Utf8Path, rel: &str) -> Option<bool> {
    let target = dlc::dlc_target(rel)?;
    let path = game_dir.join(rel);
    if rel.to_ascii_lowercase().ends_with(".ba2") {
        // Textures/archives: size is 2K vs 4K; dont hash GB
        let size = std::fs::metadata(&path).ok()?.len();
        Some(size == target.expected.size)
    } else {
        // Masters/idx/cdx: small and cheap(er) SHA
        let fp = fingerprint_file(&path).ok()??;
        Some(target.expected.matches(&fp))
    }
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

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

    // --- scan_dlc_consistency (disk) ---

    #[test]
    fn dlc_survey_flags_off_revision_files_in_an_owned_group() {
        let (_tmp, root) = temp_base();
        let data = root.join("Data");
        std::fs::create_dir_all(&data).unwrap();
        // The sentinel makes DLCworkshop02 owned; neither file is the revision identity
        std::fs::write(data.join("DLCworkshop02.esm"), b"not the corrected master").unwrap();
        std::fs::write(data.join("DLCworkshop02 - Textures.ba2"), b"tiny").unwrap();

        let survey = scan_dlc_consistency(&root);
        let group = survey
            .iter()
            .find(|g| g.group == "DLCworkshop02")
            .expect("owned group surveyed");
        // The master fails the fingerprint check; the archive fails the size check
        assert!(group.off_revision.contains(&"Data/DLCworkshop02.esm"));
        assert!(
            group
                .off_revision
                .contains(&"Data/DLCworkshop02 - Textures.ba2")
        );
    }

    #[test]
    fn dlc_survey_skips_a_group_whose_sentinel_is_absent() {
        let (_tmp, root) = temp_base();
        std::fs::create_dir_all(root.join("Data")).unwrap();
        // No DLCworkshop02.esm → the group isn't owned → not surveyed
        assert!(
            scan_dlc_consistency(&root)
                .iter()
                .all(|g| g.group != "DLCworkshop02")
        );
    }

    #[test]
    fn dlc_survey_reports_a_missing_companion_file() {
        let (_tmp, root) = temp_base();
        let data = root.join("Data");
        std::fs::create_dir_all(&data).unwrap();
        // Sentinel present (group owned), but the textures archive is absent
        std::fs::write(data.join("DLCworkshop02.esm"), b"present but off-revision").unwrap();
        let survey = scan_dlc_consistency(&root);
        let group = survey
            .iter()
            .find(|g| g.group == "DLCworkshop02")
            .expect("owned group surveyed");
        assert!(group.missing.contains(&"Data/DLCworkshop02 - Textures.ba2"));
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
        // Not in the active set
        assert_eq!(
            active_plugins_name(Utf8Path::new("Data/Bar.esp"), &active),
            None
        );
        // Deeper than Data/<plugin>
        assert_eq!(
            active_plugins_name(Utf8Path::new("Data/meshes/Foo.esp"), &active),
            None
        );
        // Not under Data/
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
        // Two markers in the active plugin; markers elsewhere must be ignored
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
        // Folder + extension match case-insensitively; the returned name keeps its casing
        assert_eq!(ok("Data/scripts/ACTOR.PEX"), Some("ACTOR.PEX"));
        // Not one of the base script names
        assert_eq!(ok("Data/Scripts/MyCustom.pex"), None);
        // Nested below Scripts/ — the engine path differs, out of scope
        assert_eq!(ok("Data/Scripts/source/Actor.pex"), None);
        // A base name but not under Scripts/, or not under Data/ at all
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
        // A tie has no clear F4SE package, so no mod is treated as the provider
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
        // The mod that ships the base scripts is the F4SE package — its own scripts are not; overrides, whatever their bytes (this is the AE / newer-F4SE case that must stay silent)
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

    /// Without a deployed F4SE/Plugins/*.dll, the Address Library requirement does not apply
    #[test]
    fn address_library_is_not_applicable_without_a_deployed_f4se_plugin() {
        let version = overseer_core::detect::ExeVersion {
            major: 1,
            minor: 10,
            patch: 163,
            build: 0,
        };
        let files = vec![DataFile {
            path: Utf8PathBuf::from("textures/armor.dds"),
            mod_name: "ArmorMod".to_owned(),
        }];
        assert!(matches!(
            address_library_status(&files, Some(version)),
            AddressLibraryStatus::NotApplicable
        ));
    }

    /// A deployed plugin needs the Address Library, but an undetectable runtime can't name the expected bin
    #[test]
    fn address_library_is_not_applicable_when_the_game_version_is_unknown() {
        let files = vec![DataFile {
            path: Utf8PathBuf::from("F4SE/Plugins/Buffout4.dll"),
            mod_name: "Buffout".to_owned(),
        }];
        assert!(matches!(
            address_library_status(&files, None),
            AddressLibraryStatus::NotApplicable
        ));
    }

    /// read_ccc parses the manifest's plugin lines, trimming whitespace and dropping blank lines
    #[test]
    fn read_ccc_parses_entries_and_drops_blank_lines() {
        let (_tmp, base) = temp_base();
        let mut instance = Instance::new(base.join("instance"), base.join("game"));
        instance.config.game = GameKind::Fallout4;
        std::fs::create_dir_all(&instance.config.game_dir).unwrap();
        std::fs::write(
            instance.config.game_dir.join("Fallout4.ccc"),
            "ccOne.esl\n\n  ccTwo.esl  \n\n",
        )
        .unwrap();

        match read_ccc(&instance) {
            CccStatus::Present { file, entries } => {
                assert_eq!(file, "Fallout4.ccc");
                assert_eq!(
                    entries,
                    vec!["ccOne.esl".to_owned(), "ccTwo.esl".to_owned()]
                );
            }
            _ => panic!("a readable manifest reads as Present"),
        }
    }

    /// A game folder with no Fallout4.ccc reads as a missing manifest
    #[test]
    fn read_ccc_reports_a_missing_manifest() {
        let (_tmp, base) = temp_base();
        let mut instance = Instance::new(base.join("instance"), base.join("game"));
        instance.config.game = GameKind::Fallout4;
        std::fs::create_dir_all(&instance.config.game_dir).unwrap();
        assert!(matches!(
            read_ccc(&instance),
            CccStatus::Missing {
                file: "Fallout4.ccc"
            }
        ));
    }

    /// A corrupt .ba2 (right extension, wrong bytes) surfaces through scan_archives as Invalid
    #[test]
    fn scan_archives_flags_a_corrupt_ba2_as_invalid() {
        let (_tmp, base) = temp_base();
        let mod_dir = base.join("mods/A");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            mod_dir.join("Broken - Main.ba2"),
            b"this is not a valid BA2 header",
        )
        .unwrap();

        let plan =
            DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("A", &mod_dir)])
                .unwrap();

        let archives = scan_archives(&plan);
        assert_eq!(archives.len(), 1);
        assert_eq!(archives[0].name, "Broken - Main.ba2");
        assert_eq!(archives[0].mod_name, "A");
        assert!(matches!(archives[0].scan, ArchiveScan::Invalid));
    }

    /// An F4SE plugin DLL that can't be read is skipped, not scanned
    #[test]
    fn scan_f4se_plugins_skips_an_unreadable_dll() {
        let (_tmp, base) = temp_base();
        let f4se = base.join("mods/F4SE");
        let dll = f4se.join("F4SE/Plugins/Buffout4.dll");
        write_file(&dll, b"stub");
        let plan =
            DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("F4SE", &f4se)])
                .unwrap();
        // Swap the planned DLL for a directory so reading its bytes fails
        std::fs::remove_file(&dll).unwrap();
        std::fs::create_dir(&dll).unwrap();
        assert!(
            scan_f4se_plugins(&plan).is_empty(),
            "an unreadable F4SE DLL is skipped, not scanned"
        );
    }

    #[test]
    fn a_base_script_from_a_non_provider_mod_is_flagged() {
        let (_tmp, base) = temp_base();
        let f4se = base.join("mods/F4SE");
        write_file(&f4se.join("Scripts/Actor.pex"), b"f4se");
        write_file(&f4se.join("Scripts/Game.pex"), b"f4se");
        write_file(&f4se.join("Scripts/Form.pex"), b"f4se");
        // A different mod ships a base script — an override the F4SE package doesn't own
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

    /// A fake Fallout 4 install with temp local/INI dirs away from real `%LOCALAPPDATA%`/Documents, plus empty `Data/`
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
        // The base master, one owned DLC, and a Creation Club plugin are installed
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
        // An implicit candidate that isn't installed must not be counted
        assert!(
            !names.contains(&"DLCNukaWorld.esm"),
            "an uninstalled DLC does not load"
        );

        // The budget the engine actually sees: 2 full ESMs + 1 light ESL
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
        // A nested archive is not top-level in Data/, so the engine won't auto-load it — even though its basename matches the active plugin. Must not be counted
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

    #[test]
    fn address_library_outside_f4se_plugins_does_not_count_as_present() {
        // A stray version-*.bin loose in Data/ must not satisfy the check; it belongs under F4SE/Plugins/
        let version = overseer_core::detect::ExeVersion {
            major: 1,
            minor: 10,
            patch: 163,
            build: 0,
        };
        let files = vec![
            DataFile {
                path: Utf8PathBuf::from("F4SE/Plugins/Buffout4.dll"),
                mod_name: "Buffout".to_owned(),
            },
            DataFile {
                path: Utf8PathBuf::from("version-1-10-163-0.bin"),
                mod_name: "Stray".to_owned(),
            },
        ];
        match address_library_status(&files, Some(version)) {
            AddressLibraryStatus::Missing { expected } => {
                assert_eq!(expected, "version-1-10-163-0.bin")
            }
            _ => panic!("a loose version-*.bin outside F4SE/Plugins must read as Missing"),
        }
    }

    #[test]
    fn address_library_under_f4se_plugins_is_present() {
        let version = overseer_core::detect::ExeVersion {
            major: 1,
            minor: 10,
            patch: 163,
            build: 0,
        };
        let files = vec![
            DataFile {
                path: Utf8PathBuf::from("F4SE/Plugins/Buffout4.dll"),
                mod_name: "Buffout".to_owned(),
            },
            DataFile {
                path: Utf8PathBuf::from("F4SE/Plugins/version-1-10-163-0.bin"),
                mod_name: "AddrLib".to_owned(),
            },
        ];
        assert!(matches!(
            address_library_status(&files, Some(version)),
            AddressLibraryStatus::Present
        ));
    }
}
