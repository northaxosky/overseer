//! The real game load order: writing the game's `Plugins.txt` via libloadorder

use super::error::PluginError;
use super::loadorder::PluginEntry;
use camino::{Utf8Path, Utf8PathBuf};
use loadorder::{GameId, GameSettings};

/// Write the game's real `Plugins.txt` to match `plugins` (load order + active flags)
pub fn write_active_plugins(
    game_id: GameId,
    game_dir: &Utf8Path,
    local_dir: &Utf8Path,
    plugins: &[PluginEntry],
) -> Result<(), PluginError> {
    let settings =
        GameSettings::with_local_path(game_id, game_dir.as_std_path(), local_dir.as_std_path())?;
    let mut load_order = settings.into_load_order();
    load_order.load()?;

    let order: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
    load_order.set_load_order(&order)?;

    let active: Vec<&str> = plugins
        .iter()
        .filter(|p| p.active)
        .map(|p| p.name.as_str())
        .collect();
    load_order.set_active_plugins(&active)?;
    load_order.save()?;
    Ok(())
}

/// The game's real `Plugins.txt` lives directly in the local data dir
fn plugins_txt_path(local_dir: &Utf8Path) -> Utf8PathBuf {
    local_dir.join("Plugins.txt")
}

/// Read the current real `Plugins.txt` so it can be restored later
pub fn read_plugins_txt(local_dir: &Utf8Path) -> Result<Option<Vec<u8>>, PluginError> {
    let path = plugins_txt_path(local_dir);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(PluginError::Io { path, source }),
    }
}

/// Restore `Plugins.txt`: `Some(bytes)` rewrites original; `None` removes the file we created
pub fn restore_plugins_txt(local_dir: &Utf8Path, backup: Option<&[u8]>) -> Result<(), PluginError> {
    let path = plugins_txt_path(local_dir);
    match backup {
        Some(bytes) => {
            std::fs::write(&path, bytes).map_err(|source| PluginError::Io { path, source })
        }
        None => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(PluginError::Io { path, source }),
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::test_support::{FLAG_MASTER, write_plugin};
    use tempfile::TempDir;

    /// A temp game dir (with `Data/`) + a temp local dir for `Plugins.txt`.
    fn setup() -> (TempDir, Utf8PathBuf, Utf8PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8 path");
        let game = root.join("game");
        let local = root.join("local");
        std::fs::create_dir_all(game.join("Data")).expect("Data dir");
        std::fs::create_dir_all(&local).expect("local dir");
        (tmp, game, local)
    }

    fn entry(name: &str, active: bool) -> PluginEntry {
        PluginEntry {
            name: name.to_owned(),
            active,
        }
    }

    #[test]
    fn writes_ordered_asterisk_plugins_txt() {
        let (_tmp, game, local) = setup();
        let data = game.join("Data");
        write_plugin(&data, "Aaa.esp", 0, &[]);
        write_plugin(&data, "Bbb.esp", 0, &[]);
        write_plugin(&data, "Ccc.esp", 0, &[]);

        write_active_plugins(
            GameId::Fallout4,
            &game,
            &local,
            &[
                entry("Aaa.esp", true),
                entry("Bbb.esp", false),
                entry("Ccc.esp", true),
            ],
        )
        .expect("write");

        // Order preserved; `*` marks active; inactive listed without a prefix.
        let txt = std::fs::read_to_string(local.join("Plugins.txt")).expect("read");
        assert_eq!(txt, "*Aaa.esp\nBbb.esp\n*Ccc.esp\n");
    }

    #[test]
    fn masters_serialize_before_normal_plugins() {
        let (_tmp, game, local) = setup();
        let data = game.join("Data");
        write_plugin(&data, "Base.esm", FLAG_MASTER, &[]);
        write_plugin(&data, "Mod.esp", 0, &["Base.esm"]);

        write_active_plugins(
            GameId::Fallout4,
            &game,
            &local,
            &[entry("Base.esm", true), entry("Mod.esp", true)],
        )
        .expect("write");

        let txt = std::fs::read_to_string(local.join("Plugins.txt")).expect("read");
        assert_eq!(txt, "*Base.esm\n*Mod.esp\n");
    }

    #[test]
    fn backup_round_trips_raw_bytes() {
        let (_tmp, _game, local) = setup();
        // Nothing written yet.
        assert_eq!(read_plugins_txt(&local).expect("read"), None);

        // A Windows-1252 byte (0xE9 = 'é') makes this invalid UTF-8 on purpose.
        let original = b"*Caf\xE9.esp\n".to_vec();
        std::fs::write(local.join("Plugins.txt"), &original).expect("seed");

        let backup = read_plugins_txt(&local).expect("read").expect("present");
        assert_eq!(backup, original);

        // Restoring rewrites the exact bytes...
        std::fs::write(local.join("Plugins.txt"), b"clobbered").expect("clobber");
        restore_plugins_txt(&local, Some(&backup)).expect("restore");
        assert_eq!(
            std::fs::read(local.join("Plugins.txt")).expect("read"),
            original
        );

        // ...and restoring `None` removes the file (there was none originally).
        restore_plugins_txt(&local, None).expect("restore none");
        assert!(!local.join("Plugins.txt").exists());
    }

    #[test]
    fn exceeding_the_active_plugin_limit_is_reported() {
        let (_tmp, game, local) = setup();
        let data = game.join("Data");
        let names: Vec<String> = (0..260).map(|i| format!("Mod{i:03}.esp")).collect();
        for name in &names {
            write_plugin(&data, name, 0, &[]);
        }
        let plugins: Vec<PluginEntry> = names.iter().map(|n| entry(n, true)).collect();

        let err = write_active_plugins(GameId::Fallout4, &game, &local, &plugins)
            .expect_err("over the limit");
        assert!(matches!(
            err,
            PluginError::GameState(loadorder::Error::TooManyActivePlugins { .. })
        ));
    }
}
