//! Shared in-memory fixtures for the TUI tests.

use crate::app::{
    App, ConflictsState, DownloadsState, Focus, ModsPane, PluginsPane, SavesState, Session,
    Workspace,
};
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::install::DownloadEntry;
use overseer_core::instance::{Instance, ModKind, ModListEntry, Profile};
use overseer_core::plugins::{PluginEntry, PluginLoadOrder, PluginMeta, PluginSeparators};
use overseer_core::saves::{SaveInfo, SaveMeta};
use overseer_core::settings::Settings;
use std::time::{Duration, SystemTime};

impl App {
    /// A small in-memory fixture for tests (no disk access)
    pub(crate) fn sample() -> Self {
        let session = Session {
            instance: Instance::new("test-instance", "test-game"),
            profile: Profile {
                name: "Default".to_owned(),
                mods: vec![
                    ModListEntry {
                        name: "CoolMod".to_owned(),
                        enabled: true,
                        kind: ModKind::Managed,
                    },
                    ModListEntry {
                        name: "OffMod".to_owned(),
                        enabled: false,
                        kind: ModKind::Managed,
                    },
                ],
                local_saves: false,
            },
            order: PluginLoadOrder {
                profile: "Default".to_owned(),
                plugins: vec![
                    PluginEntry {
                        name: "Cool.esm".to_owned(),
                        active: true,
                    },
                    PluginEntry {
                        name: "Cool.esp".to_owned(),
                        active: false,
                    },
                ],
            },
            discovered: vec![PluginMeta {
                name: "Cool.esm".to_owned(),
                is_master: true,
                is_light: false,
                masters: Vec::new(),
                header_version: None,
            }],
            plugin_separators: PluginSeparators::default(),
            status: None,
        };
        let mods = ModsPane::new(&session.profile.mods);
        let plugins = PluginsPane::new(&session.order.plugins, &session.plugin_separators);
        Self {
            should_quit: false,
            modal: None,
            focus: Focus::Mods,
            workspace: Workspace::default(),
            message: None,
            settings: Settings {
                recent_instances: vec![
                    Utf8Path::new("/alpha").to_owned(),
                    Utf8Path::new("/beta").to_owned(),
                ],
                ..Settings::default()
            },
            session,
            mods,
            plugins,
            conflicts: ConflictsState::default(),
            downloads: DownloadsState::default(),
            saves: SavesState::default(),
        }
    }
}

/// A `SaveInfo` at `Saves/{name}` whose mtime sits `modified_secs` past the epoch
pub(crate) fn save_info(name: &str, modified_secs: u64, meta: Option<SaveMeta>) -> SaveInfo {
    SaveInfo {
        path: Utf8PathBuf::from(format!("Saves/{name}")),
        file_name: name.to_owned(),
        modified: SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs),
        meta,
    }
}

/// A `DownloadEntry` at `downloads/{name}` whose mtime sits `modified_secs` past the epoch
pub(crate) fn download_entry(
    name: &str,
    size: u64,
    modified_secs: u64,
    installed: bool,
) -> DownloadEntry {
    DownloadEntry {
        name: name.to_owned(),
        path: Utf8PathBuf::from(format!("downloads/{name}")),
        installed,
        size,
        modified: SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs),
    }
}
