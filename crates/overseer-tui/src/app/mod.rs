//! Application state and update logic.

mod input;
mod list;
mod modal;
mod operation;
mod pane;
mod sort;

pub(crate) use list::ListCursor;
pub(crate) use modal::{
    Confirm, ConfirmAction, DoctorReport, Info, LaunchRow, Modal, Prompt, PromptKind, Select,
    SelectKind,
};
pub(crate) use operation::{
    DeployJob, DoctorJob, InstallJob, OperationKind, OperationProgress, OperationState, PurgeJob,
    RefreshDownloadsJob, RefreshSavesJob, RemoveJob, ReplaceJob, ScanConflictsJob,
};
pub(crate) use pane::{ModPaneRow, ModsPane, PluginPaneRow, PluginsPane};
pub(crate) use sort::{downloads_sort_label, saves_sort_label};

use anyhow::Result;
use camino::Utf8Path;
use overseer_core::apply::{self, DeploymentStatus};
use overseer_core::deploy::{ConflictSnapshot, DestinationEntry, ProviderOrigin};
use overseer_core::install::DownloadEntry;
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, PluginSeparators, discover_plugins};
use overseer_core::saves::SaveInfo;
use overseer_core::settings::Settings;
use overseer_frontend::style::Role;
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};

/// Key bindings shown (and selectable) in the help modal: (keys, description)
pub(crate) const HELP_ENTRIES: &[(&str, &str)] = &[
    ("j / k   ↓ / ↑", "move selection"),
    ("Tab", "switch pane"),
    (
        "Space / Enter",
        "toggle enabled · collapse separator · install",
    ),
    ("X", "delete save"),
    ("L", "toggle local saves"),
    ("J / K", "reorder mod or plugin"),
    ("R", "rename mod / separator"),
    ("m / e", "remove / replace managed mod"),
    ("A", "add separator"),
    ("x / Del", "delete separator"),
    ("1 / 2 / 3 / 4", "switch workspace"),
    ("[ / ]", "cycle workspace"),
    ("r", "scan conflicts · refresh downloads"),
    ("f / g", "filter conflicts · jump to provider"),
    ("o / O", "cycle sort key · toggle direction"),
    ("D / P", "deploy / purge"),
    ("l", "launch a target"),
    ("p", "switch profile"),
    ("s", "switch instance"),
    ("d", "run diagnostics"),
    ("?", "toggle this help"),
    ("q / Esc", "quit"),
];

/// A transient footer message with a severity for coloring
#[derive(Debug)]
pub(crate) struct Notice {
    pub(crate) text: String,
    pub(crate) role: Role,
}

/// Which pane has keyboard focus
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
    /// The workspace `delta` steps away in `Workspace::iter()` order, wrapping at the ends
    pub(crate) fn cycle(self, delta: isize) -> Self {
        cycle_variant(self, delta)
    }

    /// The digit key that switches to this workspace (`1`..`4`)
    pub(crate) fn key(self) -> char {
        match self {
            Self::Plugins => '1',
            Self::Conflicts => '2',
            Self::Downloads => '3',
            Self::Saves => '4',
        }
    }

    /// The workspace a digit key selects, if any
    pub(crate) fn from_key(c: char) -> Option<Self> {
        Self::iter().find(|w| w.key() == c)
    }

    /// The switcher label for this workspace
    pub(crate) fn label(self) -> &'static str {
        self.into()
    }
}

/// The Conflicts workspace's scan state: not just `Vec`
#[derive(Debug, Default)]
pub(crate) enum ConflictsStatus {
    #[default]
    Stale,
    Ready(ConflictSnapshot),
}

/// The conflicts workspace's own state (grouped so `App` doesn't get loose fields)
#[derive(Debug, Default)]
pub(crate) struct ConflictsState {
    pub(crate) status: ConflictsStatus,
    pub(crate) list: ListCursor,
    pub(crate) filter: Option<String>,
}

impl ConflictsState {
    /// Snapshot-conflict indices that pass the active mod filter (empty unless ready)
    pub(crate) fn visible_indices(&self) -> Vec<usize> {
        let ConflictsStatus::Ready(snapshot) = &self.status else {
            return Vec::new();
        };
        (0..snapshot.conflicts().len())
            .filter(|&i| {
                match &self.filter {
            None => true,
            Some(name) => snapshot.conflicts()[i].providers.iter().any(|p| {
                matches!(&p.origin, ProviderOrigin::Mod { name: m } if m.eq_ignore_ascii_case(name))
            })
        }
            })
            .collect()
    }

    /// Return the selected visible conflict
    pub(crate) fn selected(&self) -> Option<&DestinationEntry> {
        let ConflictsStatus::Ready(snapshot) = &self.status else {
            return None;
        };
        let visible = self.visible_indices();
        if visible.is_empty() {
            return None;
        }
        let sel = self.list.index().unwrap_or(0).min(visible.len() - 1);
        snapshot.conflicts().get(visible[sel])
    }
}

/// The downloads workspace's own state
#[derive(Debug, Default)]
pub(crate) struct DownloadsState {
    pub(crate) entries: Vec<DownloadEntry>,
    pub(crate) list: ListCursor,
}

/// The saves workspace's own state: the current profile's listed `.fos` saves
#[derive(Debug, Default)]
pub(crate) struct SavesState {
    pub(crate) entries: Vec<SaveInfo>,
    pub(crate) list: ListCursor,
}

/// The loaded domain data for one instance — replaced wholesale on a switch
#[derive(Debug)]
pub(crate) struct Session {
    pub(crate) instance: Instance,
    pub(crate) profile: Profile,
    pub(crate) order: PluginLoadOrder,
    pub(crate) plugin_separators: PluginSeparators,
    pub(crate) discovered: Vec<PluginMeta>,
    pub(crate) status: Option<DeploymentStatus>,
}

impl Session {
    /// Load an instance's domain data. Reconciles in memory but never saves
    pub(crate) fn load(instance_dir: &Utf8Path, requested: Option<&str>) -> Result<Self> {
        let instance = Instance::load(instance_dir.to_owned())?;
        let profile_name = requested
            .map(str::to_owned)
            .unwrap_or_else(|| instance.config.default_profile.clone());

        let mut profile = Profile::load(&instance, &profile_name)?;
        profile.reconcile(&instance)?;

        let discovered = discover_plugins(&instance, &profile)?;
        let mut order = PluginLoadOrder::load(&instance, &profile_name)?;
        order.reconcile(&discovered);

        let plugin_separators = PluginSeparators::load(&instance.profile_dir(&profile_name))?;
        let status = apply::status(&instance)?;

        Ok(Self {
            instance,
            profile,
            order,
            plugin_separators,
            discovered,
            status,
        })
    }
}

/// Snapshot the UI renders: persistent UI state plus the current instance's [`Session`]
#[derive(Debug)]
pub(crate) struct App {
    pub(crate) should_quit: bool,
    pub(crate) modal: Option<Modal>,
    pub(crate) focus: Focus,
    pub(crate) workspace: Workspace,
    pub(crate) conflicts: ConflictsState,
    pub(crate) downloads: DownloadsState,
    pub(crate) saves: SavesState,
    pub(crate) operation: OperationState,
    pub(crate) message: Option<Notice>,
    pub(crate) settings: Settings,
    pub(crate) session: Session,
    pub(crate) mods: ModsPane,
    pub(crate) plugins: PluginsPane,
}

impl App {
    /// Load an instance and remember it in settings
    pub(crate) fn load(
        instance_dir: &Utf8Path,
        requested: Option<&str>,
        mut settings: Settings,
    ) -> Result<Self> {
        let session = Session::load(instance_dir, requested)?;

        // Only a successful load is worth remembering
        settings.record_opened(instance_dir);
        if let Err(e) = settings.save() {
            tracing::warn!(error = %e, "could not save settings");
        }
        let mods = ModsPane::new(session.profile.rows());
        let plugins = PluginsPane::new(&session.order.plugins, &session.plugin_separators);

        Ok(Self {
            should_quit: false,
            modal: None,
            focus: Focus::Mods,
            workspace: Workspace::default(),
            conflicts: ConflictsState::default(),
            downloads: DownloadsState::default(),
            saves: SavesState::default(),
            operation: OperationState::default(),
            message: None,
            mods,
            plugins,
            settings,
            session,
        })
    }

    /// Footer message: success (green)
    pub(crate) fn ok(&mut self, text: impl Into<String>) {
        self.message = Some(Notice {
            text: text.into(),
            role: Role::Success,
        });
    }

    /// Footer message: failure (red)
    pub(crate) fn fail(&mut self, text: impl Into<String>) {
        self.message = Some(Notice {
            text: text.into(),
            role: Role::Failure,
        });
    }

    /// Footer message: neutral notice (muted)
    pub(crate) fn note(&mut self, text: impl Into<String>) {
        self.message = Some(Notice {
            text: text.into(),
            role: Role::Muted,
        });
    }
}

/// The variant `delta` steps from `current` in `T::iter()` order, wrapping both ends
pub(crate) fn cycle_variant<T: IntoEnumIterator + PartialEq + Copy>(current: T, delta: isize) -> T {
    let n = T::iter().count();
    let i = T::iter().position(|v| v == current).unwrap_or(0);
    let target = (i as isize + delta).rem_euclid(n as isize) as usize;
    T::iter()
        .nth(target)
        .expect("target is within the variant count")
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;
