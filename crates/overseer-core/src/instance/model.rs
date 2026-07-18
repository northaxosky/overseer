//! The instance model: on-disk `overseer.toml` config, installed mods, and executables

use super::error::{InstanceError, io_err};
use super::profile::{ModKind, Profile};
use crate::deploy::DeployerKind;
use crate::fs;
use crate::game::GameKind;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use thiserror::Error;

const PENDING_MOD_OPERATION_DIR: &str = "pending-mod-operation";
const RESERVED_TOOL_KEYS: [&str; 2] = ["game", "script-extender"];
const DERIVED_TOOL_NAMES: [&str; 2] = ["Game", "Script Extender (F4SE)"];

/// Persisted configuration for an instance, stored as `overseer.toml` at the instance root
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    /// The game install directory (contains the game exe & `Data/`)
    pub game_dir: Utf8PathBuf,

    /// Which game this instance manages
    #[serde(default)]
    pub game: GameKind,

    /// Where the game's real `Plugins.txt` lives
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub local_dir: Option<Utf8PathBuf>,

    /// Where the game reads its INIs (`Documents\My Games\<game>`)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ini_dir: Option<Utf8PathBuf>,

    /// The profile used when a command doesn't specify one
    #[serde(default = "default_profile")]
    pub default_profile: String,

    /// Which deployment backend this instance uses
    #[serde(default)]
    pub deployer: DeployerKind,

    /// User configured launch targets
    #[serde(default)]
    pub tools: Vec<UserTool>,
}

fn default_profile() -> String {
    "Default".to_owned()
}

/// A managed Overseer instance: a `mods/` folder and `profiles/`, plus target game
#[derive(Debug, Clone)]
pub struct Instance {
    pub root: Utf8PathBuf,
    pub config: InstanceConfig,
}

/// An installed mod: a named staging folder under the instance's `mods/` directory
#[derive(Debug, Clone)]
pub struct InstalledMod {
    pub name: String,
}

/// A stable key for a user configured tool, immutable across display-name renames
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct UserToolId(String);

impl UserToolId {
    /// Validate and wrap a tool key: nonempty, `[a-z0-9-]` only, not a reserved key
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidUserToolId> {
        let value = value.into();
        if value.is_empty()
            || RESERVED_TOOL_KEYS.contains(&value.as_str())
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(InvalidUserToolId(value));
        }
        Ok(Self(value))
    }

    /// The key as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'de> Deserialize<'de> for UserToolId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// The reason a string was not a valid [`UserToolId`]
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("invalid user tool id `{0}`")]
pub struct InvalidUserToolId(String);

/// A user configured launch target; derived game/F4SE targets are not persisted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTool {
    /// Stable lookup key, minted once and immutable across renames
    pub id: UserToolId,
    /// Display name, renameable
    pub name: String,
    /// Path to the executable
    pub path: Utf8PathBuf,
    /// Arguments passed on the command line
    #[serde(default)]
    pub args: Vec<String>,
    /// The mod generated output routes into (inert until output routing lands)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_mod: Option<String>,
}

impl UserTool {
    /// Create a user tool, minting a fresh id from the display name
    pub fn new(name: impl Into<String>, path: impl Into<Utf8PathBuf>, args: Vec<String>) -> Self {
        let name = name.into();
        Self {
            id: mint_tool_id(&name, &[]),
            name,
            path: path.into(),
            args,
            output_mod: None,
        }
    }
}

/// Turn a display name into a lowercase `[a-z0-9-]` slug, or "tool" when it has no usable characters
fn slug_of(name: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(byte.to_ascii_lowercase() as char);
            separator = false;
        } else {
            separator = true;
        }
    }
    if slug.is_empty() {
        slug.push_str("tool");
    }
    slug
}

/// Derive a stable, unique tool key from a display name, avoiding reserved and existing keys
pub fn mint_tool_id(name: &str, existing: &[UserToolId]) -> UserToolId {
    let base = slug_of(name);
    let is_free = |candidate: &str| {
        !RESERVED_TOOL_KEYS.contains(&candidate)
            && existing.iter().all(|id| id.as_str() != candidate)
    };
    if is_free(&base) {
        return UserToolId(base);
    }
    // The base is taken, so use one past the highest `base-N` in use (always free)
    let prefix = format!("{base}-");
    let highest = existing
        .iter()
        .filter_map(|id| id.as_str().strip_prefix(&prefix))
        .filter_map(|suffix| suffix.parse::<u32>().ok())
        .max()
        .unwrap_or(1);
    UserToolId(format!("{base}-{}", highest + 1))
}

/// Why a tool mutation was rejected
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ToolMutationError {
    #[error("the derived tool `{0}` cannot be changed")]
    Derived(String),
    #[error("no user tool with id `{0}`")]
    NotFound(String),
    #[error("a launch target named `{0}` already exists")]
    DuplicateName(String),
}

impl InstanceConfig {
    /// Add a user tool, minting a unique id and rejecting a name clash with a derived or existing tool
    pub fn add_tool(
        &mut self,
        name: String,
        path: Utf8PathBuf,
        args: Vec<String>,
    ) -> Result<UserToolId, ToolMutationError> {
        if DERIVED_TOOL_NAMES
            .iter()
            .any(|derived| derived.eq_ignore_ascii_case(&name))
            || self
                .tools
                .iter()
                .any(|tool| tool.name.eq_ignore_ascii_case(&name))
        {
            return Err(ToolMutationError::DuplicateName(name));
        }
        let ids: Vec<UserToolId> = self.tools.iter().map(|tool| tool.id.clone()).collect();
        let id = mint_tool_id(&name, &ids);
        self.tools.push(UserTool {
            id: id.clone(),
            name,
            path,
            args,
            output_mod: None,
        });
        Ok(id)
    }

    /// Remove a user tool by key, refusing a derived tool
    pub fn remove_tool(&mut self, key: &str) -> Result<UserTool, ToolMutationError> {
        if RESERVED_TOOL_KEYS.contains(&key) {
            return Err(ToolMutationError::Derived(key.to_owned()));
        }
        let index = self
            .tools
            .iter()
            .position(|tool| tool.id.as_str() == key)
            .ok_or_else(|| ToolMutationError::NotFound(key.to_owned()))?;
        Ok(self.tools.remove(index))
    }

    /// Rename a user tool by key (id unchanged), refusing a derived tool or a name clash
    pub fn rename_tool(&mut self, key: &str, name: String) -> Result<(), ToolMutationError> {
        if RESERVED_TOOL_KEYS.contains(&key) {
            return Err(ToolMutationError::Derived(key.to_owned()));
        }
        if DERIVED_TOOL_NAMES
            .iter()
            .any(|derived| derived.eq_ignore_ascii_case(&name))
            || self.tools.iter().any(|tool| {
                tool.id.as_str() != key && tool.name.eq_ignore_ascii_case(name.as_str())
            })
        {
            return Err(ToolMutationError::DuplicateName(name));
        }
        let tool = self
            .tools
            .iter_mut()
            .find(|tool| tool.id.as_str() == key)
            .ok_or_else(|| ToolMutationError::NotFound(key.to_owned()))?;
        tool.name = name;
        Ok(())
    }

    /// Replace a user tool's launch args by key, refusing a derived tool
    pub fn set_tool_args(&mut self, key: &str, args: Vec<String>) -> Result<(), ToolMutationError> {
        if RESERVED_TOOL_KEYS.contains(&key) {
            return Err(ToolMutationError::Derived(key.to_owned()));
        }
        let tool = self
            .tools
            .iter_mut()
            .find(|tool| tool.id.as_str() == key)
            .ok_or_else(|| ToolMutationError::NotFound(key.to_owned()))?;
        tool.args = args;
        Ok(())
    }
}

impl Instance {
    /// Construct an in-memory instance with a default config for the given game directory
    pub fn new(root: impl Into<Utf8PathBuf>, game_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            root: root.into(),
            config: InstanceConfig {
                game_dir: game_dir.into(),
                game: GameKind::default(),
                local_dir: None,
                ini_dir: None,
                default_profile: default_profile(),
                deployer: DeployerKind::default(),
                tools: Vec::new(),
            },
        }
    }

    /// Path to the instance's config file
    pub fn config_path(root: &Utf8Path) -> Utf8PathBuf {
        root.join("overseer.toml")
    }

    /// Load an existing instance from disk by reading its `overseer.toml`
    pub fn load(root: impl Into<Utf8PathBuf>) -> Result<Self, InstanceError> {
        let root = root.into();
        let path = Self::config_path(&root);
        let text = std::fs::read_to_string(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                InstanceError::NotAnInstance { path: path.clone() }
            } else {
                io_err(&path, source).into()
            }
        })?;

        let config = toml::from_str(&text).map_err(|source| InstanceError::Config {
            path: path.clone(),
            source: Box::new(source),
        })?;
        Ok(Self { root, config })
    }

    /// Creates a new instance on disk: write `overseer.toml` and the `mods/`/`profiles/` dirs
    pub fn init(
        root: impl Into<Utf8PathBuf>,
        config: InstanceConfig,
    ) -> Result<Self, InstanceError> {
        let root = root.into();
        let path = Self::config_path(&root);
        if path.exists() {
            return Err(InstanceError::AlreadyAnInstance { path });
        }
        fs::ensure_dir(&root)?;
        let instance = Self { root, config };
        instance.save()?;
        let mods_dir = instance.mods_dir();
        fs::ensure_dir(&mods_dir)?;
        let profiles_dir = instance.profiles_dir();
        fs::ensure_dir(&profiles_dir)?;
        let overwrite_dir = instance.overwrite_dir();
        fs::ensure_dir(&overwrite_dir)?;
        let downloads_dir = instance.downloads_dir();
        fs::ensure_dir(&downloads_dir)?;
        Ok(instance)
    }

    /// Write the current config to `overseer.toml`
    pub fn save(&self) -> Result<(), InstanceError> {
        let path = Self::config_path(&self.root);
        let text =
            toml::to_string_pretty(&self.config).map_err(|source| InstanceError::ConfigWrite {
                path: path.clone(),
                source: Box::new(source),
            })?;
        fs::write_atomic(&path, text.as_bytes()).map_err(Into::into)
    }

    /// The directory holding the game's real `Plugins.txt`: configured `local_dir` or `%LOCALAPPDATA%\<game>`
    pub fn local_dir(&self) -> Result<Utf8PathBuf, InstanceError> {
        if let Some(dir) = &self.config.local_dir {
            return Ok(dir.clone());
        }
        let base = std::env::var("LOCALAPPDATA").map_err(|_| InstanceError::NoLocalAppData)?;
        Ok(Utf8PathBuf::from(base).join(self.config.game.local_appdata_dir()))
    }

    /// The directory the game reads its INIs from: the configured `ini_dir`, else `Documents\My Games\<game>`
    pub fn ini_dir(&self) -> Result<Utf8PathBuf, InstanceError> {
        if let Some(dir) = &self.config.ini_dir {
            return Ok(dir.clone());
        }
        #[cfg(windows)]
        {
            let docs = dirs::document_dir().ok_or(InstanceError::NoDocumentsDir)?;
            let docs =
                Utf8PathBuf::from_path_buf(docs).map_err(InstanceError::NonUtf8DocumentsPath)?;
            Ok(docs.join("My Games").join(self.config.game.my_games_dir()))
        }
        #[cfg(not(windows))]
        {
            Err(InstanceError::NoDocumentsDir)
        }
    }

    /// This profile's redirected saves folder: `<ini_dir>/Saves/<profile>`
    pub fn saves_dir(&self, profile: &str) -> Result<Utf8PathBuf, InstanceError> {
        Ok(self.ini_dir()?.join("Saves").join(profile))
    }

    pub fn mods_dir(&self) -> Utf8PathBuf {
        self.root.join("mods")
    }

    pub fn profiles_dir(&self) -> Utf8PathBuf {
        self.root.join("profiles")
    }

    pub fn profile_dir(&self, name: &str) -> Utf8PathBuf {
        self.profiles_dir().join(name)
    }

    pub fn state_dir(&self) -> Utf8PathBuf {
        self.root.join("state")
    }

    /// The fixed path reserved for incomplete installed-mod work
    pub(crate) fn pending_mod_operation_dir(&self) -> Utf8PathBuf {
        self.state_dir().join(PENDING_MOD_OPERATION_DIR)
    }

    /// Refuse installed-mod reads and profile writes while lifecycle residue exists
    pub(crate) fn ensure_mod_state_available(&self) -> Result<(), InstanceError> {
        let path = self.pending_mod_operation_dir();
        match std::fs::symlink_metadata(&path) {
            Ok(_) => Err(InstanceError::PendingModOperation { path }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_err(&path, error).into()),
        }
    }

    pub fn overwrite_dir(&self) -> Utf8PathBuf {
        self.root.join("overwrite")
    }

    pub fn downloads_dir(&self) -> Utf8PathBuf {
        self.root.join("downloads")
    }

    /// Installed mods: the immediate subdirectories of `mods/`, sorted by name
    pub fn installed_mods(&self) -> Result<Vec<InstalledMod>, InstanceError> {
        self.ensure_mod_state_available()?;
        let names = read_subdirs(&self.mods_dir())?;
        Ok(names
            .into_iter()
            .map(|name| InstalledMod { name })
            .collect())
    }

    /// Profile names: the immediate subdirectories of `profiles/`, sorted
    pub fn profiles(&self) -> Result<Vec<String>, InstanceError> {
        read_subdirs(&self.profiles_dir())
    }

    /// Create a new & empty profile
    pub fn create_profile(&self, name: &str) -> Result<Profile, InstanceError> {
        self.ensure_mod_state_available()?;
        validate_profile_name(name)?;
        let dir = self.profile_dir(name);
        crate::fs::ensure_dir(&self.profiles_dir())?;
        std::fs::create_dir(&dir).map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                InstanceError::ProfileExists(name.to_owned())
            } else {
                io_err(&dir, source).into()
            }
        })?;
        let profile = Profile::new(name, Vec::new(), false);
        profile.save(self)?;
        Ok(profile)
    }

    /// Rename an installed mod folder and every profile entry that references it
    pub(crate) fn rename_mod(&self, old: &str, new: &str) -> Result<(), InstanceError> {
        validate_mod_name(new)?;
        if new == old {
            return Err(InstanceError::InvalidModName(
                "new name matches the old name".to_owned(),
            ));
        }
        if new.eq_ignore_ascii_case(old) {
            return Err(InstanceError::InvalidModName(
                "case-only rename isn't supported yet".to_owned(),
            ));
        }

        let installed = self.installed_mods()?;
        let old_name = installed
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(old))
            .map(|m| m.name.clone())
            .ok_or_else(|| InstanceError::ModNotInstalled(old.to_owned()))?;

        if installed
            .iter()
            .any(|m| !m.name.eq_ignore_ascii_case(&old_name) && m.name.eq_ignore_ascii_case(new))
        {
            return Err(InstanceError::ModAlreadyInstalled(new.to_owned()));
        }

        let mut profiles = Vec::new();
        for profile_name in self.profiles()? {
            let profile = Profile::load(self, &profile_name)?;
            if profile.contains(&old_name) && profile.contains(new) {
                return Err(InstanceError::ModAlreadyInList(new.to_owned()));
            }
            if profile_has_managed(&profile, &old_name) {
                profiles.push(profile);
            }
        }

        let old_dir = self.mods_dir().join(&old_name);
        let new_dir = self.mods_dir().join(new);
        std::fs::rename(&old_dir, &new_dir)
            .map_err(|e| InstanceError::from(io_err(&old_dir, e)))?;

        for mut profile in profiles {
            for entry in profile.items_mut() {
                if entry.kind == ModKind::Managed && entry.name.eq_ignore_ascii_case(&old_name) {
                    entry.name = new.to_owned();
                }
            }
            // A rename only changes the mod name, which lives in modlist.txt
            profile.save_modlist(self)?;
        }

        Ok(())
    }

    /// rename a profile directory and its redirected saves
    pub(crate) fn rename_profile(&self, old: &str, new: &str) -> Result<(), InstanceError> {
        self.ensure_mod_state_available()?;
        validate_profile_name(new)?;
        if new.eq_ignore_ascii_case(old) {
            return Err(InstanceError::InvalidProfileName(
                "new name (case-insensitive) matches old name".to_owned(),
            ));
        }

        let profiles = self.profiles()?;
        let old_name = profiles
            .iter()
            .find(|p| p.eq_ignore_ascii_case(old))
            .cloned()
            .ok_or_else(|| InstanceError::ProfileNotFound(old.to_owned()))?;
        if profiles
            .iter()
            .any(|p| !p.eq_ignore_ascii_case(&old_name) && p.eq_ignore_ascii_case(new))
        {
            return Err(InstanceError::ProfileExists(new.to_owned()));
        }

        // Redirected saves are name-keyed and live outside the profile dir
        let saves_move = if Profile::load(self, &old_name)?.local_saves {
            let from = self.saves_dir(&old_name)?;
            let to = self.saves_dir(new)?;
            match (from.exists(), to.exists()) {
                (true, true) => return Err(InstanceError::ProfileExists(new.to_owned())),
                (true, false) => Some((from, to)),
                (false, _) => None,
            }
        } else {
            None
        };

        let old_dir = self.profile_dir(&old_name);
        let new_dir = self.profile_dir(new);
        std::fs::rename(&old_dir, &new_dir)
            .map_err(|e| InstanceError::from(io_err(&old_dir, e)))?;

        // If the saves move fails, undo the dir rename
        if let Some((from, to)) = saves_move
            && let Err(e) = std::fs::rename(&from, &to)
        {
            let _ = std::fs::rename(&new_dir, &old_dir);
            return Err(InstanceError::from(io_err(&from, e)));
        }
        Ok(())
    }
}

/// Filesystem safety checks shared by mod and profile names; returns the failure reason
fn check_fs_name(name: &str) -> Result<(), &'static str> {
    const BAD: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if name.is_empty() {
        Err("name cannot be empty")
    } else if name.chars().count() > 64 {
        Err("name cannot be longer than 64 characters")
    } else if name.contains("..") || name.contains(BAD) || name.contains(char::is_control) {
        Err("name cannot contain .. or any of / \\ : * ? \" < > |")
    } else if name.ends_with('.') || name.ends_with(' ') {
        Err("name cannot end with a space or '.'")
    } else if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        Err("that name is reserved by Windows")
    } else {
        Ok(())
    }
}

/// Validate a managed mod folder name
pub(crate) fn validate_mod_name(name: &str) -> Result<(), InstanceError> {
    check_fs_name(name).map_err(|m| InstanceError::InvalidModName(m.to_owned()))?;
    if name.to_ascii_lowercase().ends_with("_separator") {
        return Err(InstanceError::InvalidModName(
            "mod names cannot end with _separator".to_owned(),
        ));
    }
    if ["overwrite", "downloads"]
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
    {
        return Err(InstanceError::InvalidModName(
            "that name is reserved by the instance layout".to_owned(),
        ));
    }
    Ok(())
}

/// Validate a profile directory name
pub(crate) fn validate_profile_name(name: &str) -> Result<(), InstanceError> {
    check_fs_name(name).map_err(|m| InstanceError::InvalidProfileName(m.to_owned()))
}

fn profile_has_managed(profile: &Profile, name: &str) -> bool {
    profile
        .items()
        .any(|entry| entry.kind == ModKind::Managed && entry.name.eq_ignore_ascii_case(name))
}

/// Names of the immediate subdirectories of `dir`, sorted; a missing dir is an empty list
fn read_subdirs(dir: &Utf8Path) -> Result<Vec<String>, InstanceError> {
    let Some(entries) = crate::fs::read_dir_opt(dir)? else {
        return Ok(Vec::new());
    };

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        if entry.file_type().map_err(|e| io_err(dir, e))?.is_dir() {
            let os_name = entry.file_name();
            match os_name.to_str() {
                Some(name) => names.push(name.to_owned()),
                None => {
                    return Err(InstanceError::NonUtf8Path(
                        os_name.to_string_lossy().into_owned(),
                    ));
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
#[path = "tests/model.rs"]
mod tests;
