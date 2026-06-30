//! Application state and update logic.

mod input;
mod modal;
mod popup;

pub(crate) use modal::{Modal, Prompt, PromptKind, Select, SelectKind};
pub(crate) use popup::{HELP_ENTRIES, Popup};

use anyhow::Result;
use camino::Utf8Path;
use overseer_core::apply::{self, DeploymentStatus};
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins};
use overseer_core::settings::Settings;
use overseer_diagnostics::Report;
use overseer_frontend::style::Role;
use ratatui::widgets::ListState;

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
    Plugins,
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
    pub(crate) popup: Option<Popup>,
    pub(crate) modal: Option<Modal>,
    pub(crate) focus: Focus,
    pub(crate) message: Option<Notice>,
    pub(crate) settings: Settings,
    pub(crate) session: Session,
    pub(crate) mods_state: ListState,
    pub(crate) plugins_state: ListState,
    pub(crate) settings_state: ListState,
    pub(crate) help_state: ListState,
    pub(crate) report: Option<Report>,
    pub(crate) doctor_state: ListState,
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
            popup: None,
            modal: None,
            focus: Focus::Mods,
            message: None,
            mods_state: initial_selection(session.profile.mods.len()),
            plugins_state: initial_selection(session.order.plugins.len()),
            settings_state: ListState::default(),
            help_state: ListState::default(),
            report: None,
            doctor_state: ListState::default(),
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
