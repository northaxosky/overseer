//! Shared in-memory fixtures for the TUI tests.

use crate::app::{
    App, ConflictsState, DownloadsState, Focus, SavesState, Session, Workspace, initial_selection,
};
use camino::Utf8Path;
use overseer_core::instance::{Instance, ModKind, ModListEntry, Profile};
use overseer_core::plugins::{PluginEntry, PluginLoadOrder, PluginMeta};
use overseer_core::settings::Settings;

impl App {
    /// A small in-memory fixture for tests (no disk access).
    pub(crate) fn sample() -> Self {
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
            session: Session {
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
                status: None,
            },
            mods_state: initial_selection(2),
            plugins_state: initial_selection(2),
            conflicts: ConflictsState::default(),
            downloads: DownloadsState::default(),
            saves: SavesState::default(),
        }
    }
}
