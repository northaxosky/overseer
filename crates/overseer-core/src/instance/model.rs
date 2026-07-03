use super::error::{InstanceError, io_err};
use super::profile::{ModKind, Profile};
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

/// An installed mod: a named staging folder under the instance's `mods/` directory.
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

    /// The directory holding the game's real `Plugins.txt`: configured `local_dir` or `%LOCALAPPDATA%\<game>`.
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

    /// Rename an installed mod folder and every profile entry that references it.
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
            for entry in &mut profile.mods {
                if entry.kind == ModKind::Managed && entry.name.eq_ignore_ascii_case(&old_name) {
                    entry.name = new.to_owned();
                }
            }
            // A rename only changes the mod name, which lives in modlist.txt;
            profile.save_modlist(self)?;
        }

        Ok(())
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

/// Validate a managed mod folder name.
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

/// Validate a profile directory name.
pub(crate) fn validate_profile_name(name: &str) -> Result<(), InstanceError> {
    check_fs_name(name).map_err(|m| InstanceError::InvalidProfileName(m.to_owned()))
}

fn profile_has_managed(profile: &Profile, name: &str) -> bool {
    profile
        .mods
        .iter()
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{ModKind, ModListEntry};
    use tempfile::TempDir;

    use crate::test_support::{install_mod, save_profile, temp_instance};

    #[test]
    fn path_helpers_compose_under_root() {
        let instance = Instance::new("C:/inst", "C:/game");
        assert_eq!(instance.mods_dir(), Utf8PathBuf::from("C:/inst/mods"));
        assert_eq!(
            instance.profiles_dir(),
            Utf8PathBuf::from("C:/inst/profiles")
        );
        assert_eq!(
            instance.profile_dir("Default"),
            Utf8PathBuf::from("C:/inst/profiles/Default")
        );
    }

    #[test]
    fn discovery_is_empty_on_a_fresh_instance() {
        // Nothing created yet: missing mods/ and profiles/ are a normal empty state.
        let (_tmp, instance) = temp_instance();
        assert!(instance.installed_mods().expect("mods").is_empty());
        assert!(instance.profiles().expect("profiles").is_empty());
    }

    #[test]
    fn installed_mods_lists_subdirs_sorted() {
        let (_tmp, instance) = temp_instance();
        for name in ["Zebra", "Alpha", "Mango"] {
            std::fs::create_dir_all(instance.mods_dir().join(name)).expect("mkdir");
        }
        // A stray file in mods/ must not be reported as a mod.
        std::fs::write(instance.mods_dir().join("loose.txt"), "x").expect("write");

        let names: Vec<String> = instance
            .installed_mods()
            .expect("mods")
            .into_iter()
            .map(|m| m.name)
            .collect();
        assert_eq!(names, ["Alpha", "Mango", "Zebra"]);
    }

    #[test]
    fn profiles_lists_profile_dirs_sorted() {
        let (_tmp, instance) = temp_instance();
        for name in ["Survival", "Default"] {
            std::fs::create_dir_all(instance.profile_dir(name)).expect("mkdir");
        }
        assert_eq!(
            instance.profiles().expect("profiles"),
            ["Default", "Survival"]
        );
    }

    // --- config persistence ---

    fn temp_root() -> (TempDir, Utf8PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
        (dir, root.join("inst"))
    }

    fn config(game_dir: &str) -> InstanceConfig {
        InstanceConfig {
            game_dir: Utf8PathBuf::from(game_dir),
            game: GameKind::default(),
            local_dir: None,
            ini_dir: None,
            default_profile: "Default".to_owned(),
            deployer: DeployerKind::default(),
            executables: Vec::new(),
        }
    }

    #[test]
    fn init_writes_config_and_creates_dirs() {
        let (_tmp, root) = temp_root();
        let instance = Instance::init(&root, config("C:/games/FO4")).expect("init");

        assert!(Instance::config_path(&root).exists());
        assert!(instance.mods_dir().is_dir());
        assert!(instance.profiles_dir().is_dir());
        assert_eq!(
            instance.config.game_dir.as_path(),
            Utf8Path::new("C:/games/FO4")
        );
    }

    #[test]
    fn init_then_load_round_trips_the_config() {
        let (_tmp, root) = temp_root();
        let cfg = InstanceConfig {
            game_dir: Utf8PathBuf::from("D:/FO4"),
            game: GameKind::SkyrimSE,
            local_dir: Some(Utf8PathBuf::from("C:/Users/Me/AppData/Local/Fallout4")),
            ini_dir: None,
            default_profile: "Survival".to_owned(),
            deployer: DeployerKind::Usvfs,
            executables: vec![Executable {
                name: "xEdit".to_owned(),
                path: Utf8PathBuf::from("C:/Tools/xEdit.exe"),
                args: vec!["-FO4".to_owned()],
            }],
        };
        Instance::init(&root, cfg).expect("init");

        let loaded = Instance::load(&root).expect("load");
        assert_eq!(loaded.config.game_dir, Utf8PathBuf::from("D:/FO4"));
        assert_eq!(
            loaded.config.local_dir,
            Some(Utf8PathBuf::from("C:/Users/Me/AppData/Local/Fallout4"))
        );
        assert_eq!(loaded.config.default_profile, "Survival");
        assert_eq!(loaded.config.deployer, DeployerKind::Usvfs);
        assert_eq!(loaded.config.game, GameKind::SkyrimSE);
        assert_eq!(loaded.config.executables.len(), 1);
        assert_eq!(loaded.config.executables[0].name, "xEdit");
        assert_eq!(loaded.config.executables[0].args, ["-FO4"]);
    }

    #[test]
    fn default_executables_seed_the_game_and_script_extender() {
        let exes =
            InstanceConfig::default_executables(GameKind::SkyrimSE, Utf8Path::new("D:/SkyrimSE"));

        assert_eq!(exes.len(), 2);
        assert_eq!(exes[0].name, "game");
        assert_eq!(exes[0].path, Utf8PathBuf::from("D:/SkyrimSE/SkyrimSE.exe"));
        assert!(exes[0].args.is_empty());
        assert_eq!(exes[1].name, "script-extender");
        assert_eq!(
            exes[1].path,
            Utf8PathBuf::from("D:/SkyrimSE/skse64_loader.exe")
        );
        assert!(exes[1].args.is_empty());
    }

    #[test]
    fn legacy_config_without_game_key_defaults_to_fallout4() {
        // A pre-multi-game overseer.toml only had `game_dir`; serde defaults fill
        // in the rest, and `game` must resolve to Fallout 4 so existing instances
        // keep working untouched.
        let cfg: InstanceConfig = toml::from_str("game_dir = \"D:/FO4\"\n").expect("legacy load");
        assert_eq!(cfg.game, GameKind::Fallout4);
        assert_eq!(cfg.default_profile, "Default");
        assert_eq!(cfg.deployer, DeployerKind::default());
        assert_eq!(cfg.local_dir, None);
        assert!(cfg.executables.is_empty());
    }

    #[test]
    fn load_missing_config_is_not_an_instance() {
        let (_tmp, root) = temp_root();
        let err = Instance::load(&root).expect_err("should fail");
        assert!(matches!(err, InstanceError::NotAnInstance { .. }));
    }

    #[test]
    fn init_refuses_to_clobber_existing_instance() {
        let (_tmp, root) = temp_root();
        Instance::init(&root, config("C:/a")).expect("first init");
        let err = Instance::init(&root, config("C:/b")).expect_err("should refuse");
        assert!(matches!(err, InstanceError::AlreadyAnInstance { .. }));
    }

    #[test]
    fn omitted_local_dir_is_absent_from_the_toml_and_loads_as_none() {
        let (_tmp, root) = temp_root();
        Instance::init(&root, config("C:/FO4")).expect("init");

        let text = std::fs::read_to_string(Instance::config_path(&root)).expect("read");
        assert!(!text.contains("local_dir"), "None local_dir is omitted");

        let loaded = Instance::load(&root).expect("load");
        assert_eq!(loaded.config.local_dir, None);
    }

    #[test]
    fn minimal_toml_uses_default_profile() {
        // A hand-written config with only game_dir must load with the default profile.
        let (_tmp, root) = temp_root();
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(Instance::config_path(&root), "game_dir = \"C:/FO4\"\n").expect("write");

        let loaded = Instance::load(&root).expect("load");
        assert_eq!(loaded.config.default_profile, "Default");
        assert_eq!(loaded.config.local_dir, None);
        assert_eq!(loaded.config.deployer, DeployerKind::HardLink);
    }

    #[test]
    fn create_profile_makes_an_empty_profile_on_disk() {
        let (_tmp, instance) = temp_instance();
        let profile = instance.create_profile("Survival").expect("create");

        assert_eq!(profile.name, "Survival");
        assert!(profile.mods.is_empty());
        // The directory and an (empty) modlist are persisted...
        assert!(instance.profile_dir("Survival").is_dir());
        assert!(
            instance
                .profile_dir("Survival")
                .join("modlist.txt")
                .exists()
        );
        // ...and the profile now shows up in the listing.
        assert_eq!(instance.profiles().expect("profiles"), ["Survival"]);
    }

    #[test]
    fn create_profile_refuses_to_overwrite_an_existing_one() {
        let (_tmp, instance) = temp_instance();
        instance.create_profile("Default").expect("first create");

        let err = instance
            .create_profile("Default")
            .expect_err("should refuse");
        assert!(matches!(err, InstanceError::ProfileExists(name) if name == "Default"));
    }

    #[test]
    fn create_profile_rejects_a_filesystem_unsafe_name() {
        let (_tmp, instance) = temp_instance();
        let err = instance
            .create_profile("bad/name")
            .expect_err("invalid name must be rejected");
        assert!(matches!(err, InstanceError::InvalidProfileName(_)));
    }

    #[test]
    fn rename_profile_moves_the_directory_and_its_contents() {
        let (_tmp, instance) = temp_instance();
        save_profile(&instance, "Old", &[]);
        // A file living inside the profile dir must travel with the rename.
        std::fs::write(instance.profile_dir("Old").join("plugins.txt"), "*A.esp\n").expect("seed");

        instance.rename_profile("Old", "New").expect("rename");

        assert!(!instance.profile_dir("Old").exists());
        assert!(instance.profile_dir("New").is_dir());
        assert_eq!(
            std::fs::read_to_string(instance.profile_dir("New").join("plugins.txt")).expect("read"),
            "*A.esp\n"
        );
        assert_eq!(instance.profiles().expect("profiles"), ["New"]);
    }

    #[test]
    fn rename_profile_moves_redirected_local_saves() {
        let (_tmp, instance) = temp_instance();
        let profile = Profile {
            name: "Old".to_owned(),
            mods: Vec::new(),
            local_saves: true,
        };
        profile.save(&instance).expect("save profile");
        let old_saves = instance.saves_dir("Old").expect("saves dir");
        std::fs::create_dir_all(&old_saves).expect("mk saves");
        std::fs::write(old_saves.join("Quicksave.fos"), "save").expect("seed save");

        instance.rename_profile("Old", "New").expect("rename");

        let new_saves = instance.saves_dir("New").expect("saves dir");
        assert!(!old_saves.exists(), "old saves dir moved");
        assert_eq!(
            std::fs::read_to_string(new_saves.join("Quicksave.fos")).expect("read save"),
            "save"
        );
    }

    #[test]
    fn rename_profile_rejects_a_colliding_target() {
        let (_tmp, instance) = temp_instance();
        save_profile(&instance, "Old", &[]);
        save_profile(&instance, "Taken", &[]);

        let err = instance
            .rename_profile("Old", "taken")
            .expect_err("collision must be rejected");
        assert!(matches!(err, InstanceError::ProfileExists(name) if name == "taken"));
        assert!(instance.profile_dir("Old").is_dir());
        assert!(instance.profile_dir("Taken").is_dir());
    }

    #[test]
    fn rename_profile_rejects_a_missing_source() {
        let (_tmp, instance) = temp_instance();
        let err = instance
            .rename_profile("Ghost", "New")
            .expect_err("missing source must be rejected");
        assert!(matches!(err, InstanceError::ProfileNotFound(name) if name == "Ghost"));
    }

    #[test]
    fn rename_profile_rejects_invalid_and_case_only_names() {
        let (_tmp, instance) = temp_instance();
        save_profile(&instance, "Old", &[]);

        let bad = instance
            .rename_profile("Old", "a/b")
            .expect_err("invalid name must be rejected");
        assert!(matches!(bad, InstanceError::InvalidProfileName(_)));

        let case_only = instance
            .rename_profile("Old", "old")
            .expect_err("case-only rename must be rejected");
        assert!(matches!(case_only, InstanceError::InvalidProfileName(_)));
        assert!(instance.profile_dir("Old").is_dir(), "nothing moved");
    }

    #[test]
    fn rename_mod_renames_folder_and_rewrites_referencing_profiles() {
        let (_tmp, instance) = temp_instance();
        install_mod(
            &instance,
            "CoolMod",
            &[("Cool.esp", "plugin bytes"), ("plugins.txt", "*Cool.esp\n")],
        );
        install_mod(&instance, "Other", &[("Data.txt", "other")]);
        save_profile(&instance, "Default", &[("CoolMod", true), ("Other", false)]);

        let mut survival = Profile {
            name: "Survival".to_owned(),
            mods: vec![
                ModListEntry {
                    name: "Other".to_owned(),
                    enabled: true,
                    kind: ModKind::Managed,
                },
                ModListEntry {
                    name: "CoolMod".to_owned(),
                    enabled: false,
                    kind: ModKind::Managed,
                },
            ],
            local_saves: false,
        };
        survival.save(&instance).expect("save survival");
        save_profile(&instance, "Clean", &[("Other", true)]);
        let clean_before =
            std::fs::read_to_string(instance.profile_dir("Clean").join("modlist.txt"))
                .expect("read clean before");

        instance
            .rename_mod("CoolMod", "BetterMod")
            .expect("rename mod");

        assert!(!instance.mods_dir().join("CoolMod").exists());
        assert!(instance.mods_dir().join("BetterMod").is_dir());
        assert_eq!(
            std::fs::read_to_string(instance.mods_dir().join("BetterMod").join("Cool.esp"))
                .expect("read plugin"),
            "plugin bytes"
        );
        assert_eq!(
            std::fs::read_to_string(instance.mods_dir().join("BetterMod").join("plugins.txt"))
                .expect("read plugins.txt"),
            "*Cool.esp\n"
        );

        let default = Profile::load(&instance, "Default").expect("load default");
        assert_eq!(default.mods[0].name, "BetterMod");
        assert!(default.mods[0].enabled);
        assert_eq!(default.mods[1].name, "Other");
        assert!(!default.mods[1].enabled);

        survival = Profile::load(&instance, "Survival").expect("load survival");
        assert_eq!(survival.mods[0].name, "Other");
        assert!(survival.mods[0].enabled);
        assert_eq!(survival.mods[1].name, "BetterMod");
        assert!(!survival.mods[1].enabled);

        let clean_after =
            std::fs::read_to_string(instance.profile_dir("Clean").join("modlist.txt"))
                .expect("read clean after");
        assert_eq!(clean_after, clean_before, "unreferencing profile untouched");
    }

    #[test]
    fn rename_mod_rejects_a_colliding_target_folder() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("a.txt", "a")]);
        install_mod(&instance, "Existing", &[("b.txt", "b")]);

        let err = instance
            .rename_mod("CoolMod", "existing")
            .expect_err("collision must be rejected");
        assert!(matches!(err, InstanceError::ModAlreadyInstalled(name) if name == "existing"));
        assert!(instance.mods_dir().join("CoolMod").is_dir());
        assert!(instance.mods_dir().join("Existing").is_dir());
    }

    #[test]
    fn rename_mod_rejects_reserved_and_separator_names() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("a.txt", "a")]);

        for name in ["Foo_separator", "overwrite"] {
            let err = instance
                .rename_mod("CoolMod", name)
                .expect_err("invalid target must be rejected");
            assert!(
                matches!(err, InstanceError::InvalidModName(_)),
                "{name} should be invalid, got {err:?}"
            );
        }
    }

    #[test]
    fn rename_mod_rejects_case_only_and_noop_renames() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("a.txt", "a")]);

        let err = instance
            .rename_mod("CoolMod", "coolmod")
            .expect_err("case-only rename must be rejected");
        assert!(matches!(err, InstanceError::InvalidModName(_)));

        let err = instance
            .rename_mod("CoolMod", "CoolMod")
            .expect_err("same-name rename must be rejected");
        assert!(matches!(err, InstanceError::InvalidModName(_)));
    }

    #[test]
    fn rename_mod_rejects_profiles_that_already_list_both_names() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("a.txt", "a")]);
        save_profile(
            &instance,
            "Default",
            &[("CoolMod", true), ("BetterMod", true)],
        );

        let err = instance
            .rename_mod("CoolMod", "BetterMod")
            .expect_err("both names in a profile must be rejected");
        assert!(matches!(err, InstanceError::ModAlreadyInList(name) if name == "BetterMod"));
        assert!(instance.mods_dir().join("CoolMod").is_dir());
        assert!(!instance.mods_dir().join("BetterMod").exists());
    }

    #[test]
    fn rename_mod_reports_missing_source_mod() {
        let (_tmp, instance) = temp_instance();

        let err = instance
            .rename_mod("Missing", "BetterMod")
            .expect_err("missing mod must be rejected");
        assert!(matches!(err, InstanceError::ModNotInstalled(name) if name == "Missing"));
    }
}
