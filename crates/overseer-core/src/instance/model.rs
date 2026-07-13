//! The instance model: on-disk `overseer.toml` config, installed mods, and executables

use super::error::{InstanceError, io_err};
use super::profile::Profile;
use crate::deploy::DeployerKind;
use crate::fs;
use crate::game::GameKind;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executables: Vec<Executable>,
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

/// A user configured launch target: An external tool or other way to run game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Executable {
    /// Display name and lookup key (eg "FO4Edit")
    pub name: String,
    /// Path to the executable
    pub path: Utf8PathBuf,
    /// Arguments passed on the command line
    #[serde(default)]
    pub args: Vec<String>,
}

impl InstanceConfig {
    /// The launch targets seeded into a fresh instance: game and script extender
    pub fn default_executables(game: GameKind, game_dir: &Utf8Path) -> Vec<Executable> {
        vec![
            Executable {
                name: "game".to_owned(),
                path: game_dir.join(game.executable()),
                args: Vec::new(),
            },
            Executable {
                name: "script-extender".to_owned(),
                path: game_dir.join(game.script_extender_loader()),
                args: Vec::new(),
            },
        ]
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
                executables: Vec::new(),
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
        std::fs::write(&path, text).map_err(|e| io_err(&path, e).into())
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

    pub fn overwrite_dir(&self) -> Utf8PathBuf {
        self.root.join("overwrite")
    }

    pub fn downloads_dir(&self) -> Utf8PathBuf {
        self.root.join("downloads")
    }

    /// Installed mods: the immediate subdirectories of `mods/`, sorted by name
    pub fn installed_mods(&self) -> Result<Vec<InstalledMod>, InstanceError> {
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
        let profile = Profile {
            name: name.to_owned(),
            mods: Vec::new(),
            local_saves: false,
        };
        profile.save(self)?;
        Ok(profile)
    }

    /// rename a profile directory and its redirected saves
    pub(crate) fn rename_profile(&self, old: &str, new: &str) -> Result<(), InstanceError> {
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
    if name.ends_with("_separator") {
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
