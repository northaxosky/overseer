use super::error::{InstanceError, io_err};
use crate::deploy::DeployerKind;
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
        std::fs::create_dir_all(&root).map_err(|e| io_err(&root, e))?;
        let instance = Self { root, config };
        instance.save()?;
        std::fs::create_dir_all(instance.mods_dir())
            .map_err(|e| io_err(&instance.mods_dir(), e))?;
        std::fs::create_dir_all(instance.profiles_dir())
            .map_err(|e| io_err(&instance.profiles_dir(), e))?;
        std::fs::create_dir_all(instance.overwrite_dir())
            .map_err(|e| io_err(&instance.overwrite_dir(), e))?;
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

    /// The directory holding the game's real `Plugins.txt`: the configured
    /// `local_dir`, else the standard `%LOCALAPPDATA%\<game>` location.
    pub fn local_dir(&self) -> Result<Utf8PathBuf, InstanceError> {
        if let Some(dir) = &self.config.local_dir {
            return Ok(dir.clone());
        }
        let base = std::env::var("LOCALAPPDATA").map_err(|_| InstanceError::NoLocalAppData)?;
        Ok(Utf8PathBuf::from(base).join(self.config.game.local_appdata_dir()))
    }

    /// The directory the game reads its INIs from: the configured `ini_dir`,
    /// else the standard `Documents\My Games\<game>` location.
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
}

/// Names of the immediate subdirectories of `dir`, sorted; a missing dir is an empty list
fn read_subdirs(dir: &Utf8Path) -> Result<Vec<String>, InstanceError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(dir, e).into()),
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
    use tempfile::TempDir;

    use crate::test_support::temp_instance;

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
}
