//! Application state and update logic.

mod input;
mod modal;
mod sort;

pub(crate) use modal::{
    Confirm, ConfirmAction, DoctorReport, Info, Modal, Prompt, PromptKind, Select, SelectKind,
};
pub(crate) use sort::{downloads_sort_label, saves_sort_label};

use anyhow::Result;
use camino::Utf8Path;
use overseer_core::apply::{self, DeploymentStatus};
use overseer_core::deploy::FileConflict;
use overseer_core::install::DownloadEntry;
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins};
use overseer_core::saves::SaveInfo;
use overseer_core::settings::Settings;
use overseer_frontend::style::Role;
use ratatui::widgets::ListState;
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};

/// Key bindings shown (and selectable) in the help modal: (keys, description).
pub(crate) const HELP_ENTRIES: &[(&str, &str)] = &[
    ("j / k   ↓ / ↑", "move selection"),
    ("Tab", "switch pane"),
    ("Space / Enter", "toggle enabled · install download"),
    ("x", "delete save"),
    ("L", "toggle local saves"),
    ("J / K", "reorder mod (priority)"),
    ("R", "rename selected mod"),
    ("1 / 2 / 3 / 4", "switch workspace"),
    ("[ / ]", "cycle workspace"),
    ("r", "scan conflicts · refresh downloads"),
    ("o / O", "cycle sort key · toggle direction"),
    ("D / P", "deploy / purge"),
    ("l", "launch a target"),
    ("p", "switch profile"),
    ("n", "new profile"),
    ("s", "switch instance"),
    ("d", "run diagnostics"),
    ("?", "toggle this help"),
    ("q / Esc", "quit"),
];

/// A transient footer message with a severity for coloring.
#[derive(Debug)]
pub(crate) struct Notice {
    pub(crate) text: String,
    pub(crate) role: Role,
}

/// Which pane has keyboard focus.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Mods,
    Workspace,
}

/// Which view fills the right (workspace) pane
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, EnumIter, IntoStaticStr)]
pub(crate) enum Workspace {
    #[default]
    Plugins,
    Conflicts,
    Downloads,
    Saves,
}

impl Workspace {
    /// The workspace `delta` steps away in `Workspace::iter()` order, wrapping at the ends.
    pub(crate) fn cycle(self, delta: isize) -> Workspace {
        let all: Vec<Workspace> = Workspace::iter().collect();
        let i = all.iter().position(|&w| w == self).unwrap_or(0) as isize;
        let n = all.len() as isize;
        all[(i + delta).rem_euclid(n) as usize]
    }

    /// The digit key that switches to this workspace (`1`..`4`).
    pub(crate) fn key(self) -> char {
        match self {
            Workspace::Plugins => '1',
            Workspace::Conflicts => '2',
            Workspace::Downloads => '3',
            Workspace::Saves => '4',
        }
    }

    /// The workspace a digit key selects, if any.
    pub(crate) fn from_key(c: char) -> Option<Workspace> {
        Workspace::iter().find(|w| w.key() == c)
    }

    /// The switcher label for this workspace.
    pub(crate) fn label(self) -> &'static str {
        self.into()
    }
}

/// The Conflicts workspace's scan state: not just `Vec`
#[derive(Debug, Default)]
pub(crate) enum ConflictsStatus {
    #[default]
    Stale,
    Ready(Vec<FileConflict>),
    Error(String),
}

/// The conflicts workspace's own state (grouped so `App` doesn't get loose fields)
#[derive(Debug, Default)]
pub(crate) struct ConflictsState {
    pub(crate) status: ConflictsStatus,
    pub(crate) list: ListState,
}

/// The downloads workspace's own state
#[derive(Debug, Default)]
pub(crate) struct DownloadsState {
    pub(crate) entries: Vec<DownloadEntry>,
    pub(crate) list: ListState,
}

/// The saves workspace's own state: the current profile's listed `.fos` saves
#[derive(Debug, Default)]
pub(crate) struct SavesState {
    pub(crate) entries: Vec<SaveInfo>,
    pub(crate) list: ListState,
}

/// The loaded domain data for one instance — replaced wholesale on a switch.
#[derive(Debug)]
pub(crate) struct Session {
    pub(crate) instance: Instance,
    pub(crate) profile: Profile,
    pub(crate) order: PluginLoadOrder,
    pub(crate) discovered: Vec<PluginMeta>,
    pub(crate) status: Option<DeploymentStatus>,
}

impl Session {
    /// Load an instance's domain data. Reconciles in memory but never saves.
    pub(crate) fn load(instance_dir: &Utf8Path, profile_name: &str) -> Result<Self> {
        let instance = Instance::load(instance_dir.to_owned())?;

        let mut profile = Profile::load(&instance, profile_name)?;
        profile.reconcile(&instance)?;

        let discovered = discover_plugins(&instance, &profile)?;
        let mut order = PluginLoadOrder::load(&instance, profile_name)?;
        order.reconcile(&discovered);

        let status = apply::status(&instance)?;

        Ok(Self {
            instance,
            profile,
            order,
            discovered,
            status,
        })
    }
}

/// Snapshot the UI renders: persistent UI state plus the current instance's [`Session`].
#[derive(Debug)]
pub(crate) struct App {
    pub(crate) should_quit: bool,
    pub(crate) modal: Option<Modal>,
    pub(crate) focus: Focus,
    pub(crate) workspace: Workspace,
    pub(crate) conflicts: ConflictsState,
    pub(crate) downloads: DownloadsState,
    pub(crate) saves: SavesState,
    pub(crate) message: Option<Notice>,
    pub(crate) settings: Settings,
    pub(crate) session: Session,
    pub(crate) mods_state: ListState,
    pub(crate) plugins_state: ListState,
}

impl App {
    /// Load an instance and remember it in settings.
    pub(crate) fn load(
        instance_dir: &Utf8Path,
        profile_name: &str,
        mut settings: Settings,
    ) -> Result<Self> {
        let session = Session::load(instance_dir, profile_name)?;

        // Only a successful load is worth remembering.
        settings.record_opened(instance_dir);
        if let Err(e) = settings.save() {
            tracing::warn!(error = %e, "could not save settings");
        }

        Ok(Self {
            should_quit: false,
            modal: None,
            focus: Focus::Mods,
            workspace: Workspace::default(),
            conflicts: ConflictsState::default(),
            downloads: DownloadsState::default(),
            saves: SavesState::default(),
            message: None,
            mods_state: initial_selection(session.profile.mods.len()),
            plugins_state: initial_selection(session.order.plugins.len()),
            settings,
            session,
        })
    }

    /// Footer message: success (green).
    pub(crate) fn ok(&mut self, text: impl Into<String>) {
        self.message = Some(Notice {
            text: text.into(),
            role: Role::Success,
        });
    }

    /// Footer message: failure (red).
    pub(crate) fn fail(&mut self, text: impl Into<String>) {
        self.message = Some(Notice {
            text: text.into(),
            role: Role::Failure,
        });
    }

    /// Footer message: neutral notice (muted).
    pub(crate) fn note(&mut self, text: impl Into<String>) {
        self.message = Some(Notice {
            text: text.into(),
            role: Role::Muted,
        });
    }
}

/// A `ListState` selecting the first row when the list is non-empty.
pub(crate) fn initial_selection(len: usize) -> ListState {
    let mut state = ListState::default();
    if len > 0 {
        state.select(Some(0));
    }
    state
}
