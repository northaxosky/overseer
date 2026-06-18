use super::error::{InstanceError, io_err};
use camino::{Utf8Path, Utf8PathBuf};

/// A managed Overseer instance: a `mods/` folder and `profiles/`, plus target game
#[derive(Debug, Clone)]
pub struct Instance {
    pub root: Utf8PathBuf,
    pub game_dir: Utf8PathBuf,
}

/// An installed mod: a named staging folder under the instance's `mods/` directory.
#[derive(Debug, Clone)]
pub struct InstalledMod {
    pub name: String,
}

impl Instance {
    pub fn new(root: impl Into<Utf8PathBuf>, game_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            root: root.into(),
            game_dir: game_dir.into(),
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
        Err(e) => return Err(io_err(dir, e)),
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
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_instance() -> (TempDir, Instance) {
        let dir = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
        let instance = Instance::new(root.join("instance"), root.join("game"));
        (dir, instance)
    }

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
}
