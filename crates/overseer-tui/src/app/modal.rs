//! Modal surfaces: views that block the main view and end in submit or cancel

use camino::Utf8PathBuf;
use overseer_diagnostics::Report;
use ratatui::widgets::ListState;

/// A blocking surface: select/prompt/confirm submit or cancel; `Info`/`Doctor` dismiss, with live Doctor details.
#[derive(Debug)]
pub(crate) enum Modal {
    Select(Select),
    Prompt(Prompt),
    Confirm(Confirm),
    Info(Info),
    Doctor(DoctorReport),
}

/// A single-line text input that ends in submit or cancel
#[derive(Debug)]
pub(crate) struct Prompt {
    pub(crate) kind: PromptKind,
    pub(crate) input: String,
    pub(crate) error: Option<String>,
}

/// Which prompt a [`Prompt`] drives; its title and what submitting it does
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromptKind {
    NewProfile,
    RenameMod { old: String },
}

impl PromptKind {
    /// Heading shown on the prompt's frame
    pub(crate) fn title(&self) -> String {
        match self {
            PromptKind::NewProfile => "New profile".to_owned(),
            PromptKind::RenameMod { old } => format!("Rename: {old}"),
        }
    }
}

/// Pick one item from a list and act on it
#[derive(Debug)]
pub(crate) struct Select {
    pub(crate) kind: SelectKind,
    pub(crate) items: Vec<String>,
    pub(crate) state: ListState,
}

/// Which selection a [`Select`] drives; its items and what submitting does
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectKind {
    Launch,
    Profile,
    Instance,
}

impl SelectKind {
    /// Accelerator that opens this selection from the main view
    pub(crate) fn toggle_key(self) -> char {
        match self {
            SelectKind::Launch => 'l',
            SelectKind::Profile => 'p',
            SelectKind::Instance => 's',
        }
    }

    /// Message shown when the list has no items to choose from
    pub(crate) fn empty_message(self) -> &'static str {
        match self {
            SelectKind::Launch => "No launch targets. Add with `overseer exe add`.",
            SelectKind::Profile => "No profiles.",
            SelectKind::Instance => "No other instances.",
        }
    }

    /// Verb naming what submitting does, shown in the hint line
    pub(crate) fn action_verb(self) -> &'static str {
        match self {
            SelectKind::Launch => "launch",
            SelectKind::Profile => "switch",
            SelectKind::Instance => "switch",
        }
    }

    /// Extra hint appended after the close hint, for kinds with a side-action
    pub(crate) fn extra_hint(self) -> &'static str {
        match self {
            SelectKind::Launch => "",
            SelectKind::Profile => " · n new",
            SelectKind::Instance => "",
        }
    }
}

/// A yes/no confirmation that runs its [`ConfirmAction`] when accpeted
#[derive(Debug)]
pub(crate) struct Confirm {
    pub(crate) message: String,
    pub(crate) action: ConfirmAction,
}

/// What a confirmed [`Confirm`] does
#[derive(Debug)]
pub(crate) enum ConfirmAction {
    /// Install the archive at this path into the instance's `mods/`
    InstallDownload(Utf8PathBuf),
    /// Delete the `.fos` save at this path (and its script-extender co-save)
    DeleteSave(Utf8PathBuf),
}

/// A dismiss-only reference modal with a title and key/description rows.
#[derive(Debug)]
pub(crate) struct Info {
    pub(crate) title: String,
    pub(crate) entries: Vec<(String, String)>,
    pub(crate) state: ListState,
}

/// A dismiss-only diagnostics modal with a selectable findings list and live detail pane.
#[derive(Debug)]
pub(crate) struct DoctorReport {
    pub(crate) report: Report,
    pub(crate) list: ListState,
}
