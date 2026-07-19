//! Modal surfaces: views that block the main view and end in submit or cancel

use camino::Utf8PathBuf;
use overseer_core::launch::ToolKind;
use overseer_diagnostics::Report;

use super::ListCursor;

/// A blocking surface: select/prompt/confirm submit or cancel; `Info`/`Doctor` dismiss, with live Doctor details
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
    NewSeparator,
    NewPluginSeparator,
    RenameSeparator { index: usize, name: String },
    RenamePluginSeparator { index: usize, name: String },
    RenameMod { old: String },
    RenameProfile { old: String },
    AddExe,
    EditExeName { key: String },
    EditExeArgs { key: String },
    InstallName { archive: Utf8PathBuf },
}

impl PromptKind {
    /// Heading shown on the prompt's frame
    pub(crate) fn title(&self) -> String {
        match self {
            PromptKind::NewProfile => "New profile".to_owned(),
            PromptKind::RenameMod { old } => format!("Rename: {old}"),
            PromptKind::RenameProfile { old } => format!("Rename profile: {old}"),
            PromptKind::AddExe => "Add launch target — full path".to_owned(),
            PromptKind::NewSeparator => "New separator".to_owned(),
            PromptKind::NewPluginSeparator => "New plugin separator".to_owned(),
            PromptKind::RenameSeparator { name, .. } => format!("Rename separator: {name}"),
            PromptKind::RenamePluginSeparator { name, .. } => {
                format!("Rename plugin separator: {name}")
            }
            PromptKind::EditExeName { .. } => "Edit target — name".to_owned(),
            PromptKind::EditExeArgs { .. } => "Edit target — launch args".to_owned(),
            PromptKind::InstallName { .. } => "Install — new mod name".to_owned(),
        }
    }

    /// Cap on the prompt's input length: a path needs more room than a name
    pub(crate) fn max_len(&self) -> usize {
        match self {
            PromptKind::AddExe | PromptKind::EditExeArgs { .. } => 260, // Windows MAX_PATH
            _ => 64,
        }
    }

    /// Selection modal to reopen when this prompt is cancelled
    pub(crate) fn cancel_to(&self) -> Option<SelectKind> {
        match self {
            PromptKind::NewProfile | PromptKind::RenameProfile { .. } => Some(SelectKind::Profile),
            PromptKind::AddExe
            | PromptKind::EditExeArgs { .. }
            | PromptKind::EditExeName { .. } => Some(SelectKind::Launch),
            PromptKind::RenameMod { .. }
            | PromptKind::NewSeparator
            | PromptKind::RenameSeparator { .. }
            | PromptKind::NewPluginSeparator
            | PromptKind::InstallName { .. }
            | PromptKind::RenamePluginSeparator { .. } => None,
        }
    }
}

/// Pick one item from a list and act on it
#[derive(Debug)]
pub(crate) struct Select {
    pub(crate) kind: SelectKind,
    pub(crate) items: Vec<String>,
    pub(crate) launch_rows: Vec<LaunchRow>,
    pub(crate) state: ListCursor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchRow {
    pub(crate) key: String,
    pub(crate) kind: ToolKind,
    pub(crate) display_name: String,
}

/// Which selection a [`Select`] drives; its items and what submitting does
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelectKind {
    Launch,
    Profile,
    Instance,
    ReplaceArchive { target: String },
    JumpProvider { providers: Vec<String> },
}

impl SelectKind {
    /// Selection kind opened by a main-view toggle key
    pub(crate) fn from_toggle_key(c: char) -> Option<Self> {
        match c {
            'l' => Some(SelectKind::Launch),
            'p' => Some(SelectKind::Profile),
            's' => Some(SelectKind::Instance),
            _ => None,
        }
    }

    /// Accelerator that opens this selection from the main view
    pub(crate) fn toggle_key(&self) -> char {
        match self {
            SelectKind::Launch => 'l',
            SelectKind::Profile => 'p',
            SelectKind::Instance => 's',
            SelectKind::ReplaceArchive { .. } | SelectKind::JumpProvider { .. } => '\0',
        }
    }

    /// Message shown when the list has no items to choose from
    pub(crate) fn empty_message(&self) -> &'static str {
        match self {
            SelectKind::Launch => "No launch targets. Add with `overseer exe add`.",
            SelectKind::Profile => "No profiles.",
            SelectKind::Instance => "No other instances.",
            SelectKind::ReplaceArchive { .. } => "No archives in Downloads to replace with.",
            SelectKind::JumpProvider { .. } => "No mod providers.",
        }
    }

    /// Verb naming what submitting does, shown in the hint line
    pub(crate) fn action_verb(&self) -> &'static str {
        match self {
            SelectKind::Launch => "launch",
            SelectKind::Profile => "switch",
            SelectKind::Instance => "switch",
            SelectKind::ReplaceArchive { .. } => "replace",
            SelectKind::JumpProvider { .. } => "jump",
        }
    }

    /// Extra hint appended after the close hint, for kinds with a side-action
    pub(crate) fn extra_hint(&self) -> &'static str {
        match self {
            SelectKind::Launch => " · a add · e edit · x remove",
            SelectKind::Profile => " · n new · r rename",
            SelectKind::Instance
            | SelectKind::ReplaceArchive { .. }
            | SelectKind::JumpProvider { .. } => "",
        }
    }
}

impl Modal {
    /// Selection and row count for modal variants that own a list
    pub(super) fn list_parts_mut(&mut self) -> Option<(&mut ListCursor, usize)> {
        match self {
            Modal::Select(select) => {
                let len = select.items.len();
                Some((&mut select.state, len))
            }
            Modal::Info(info) => {
                let len = info.entries.len();
                Some((&mut info.state, len))
            }
            Modal::Doctor(doctor) => {
                let len = doctor.report.findings.len();
                Some((&mut doctor.list, len))
            }
            Modal::Prompt(_) | Modal::Confirm(_) => None,
        }
    }
}

/// A yes/no confirmation that runs its [`ConfirmAction`] when accepted
#[derive(Debug)]
pub(crate) struct Confirm {
    pub(crate) message: String,
    pub(crate) action: ConfirmAction,
}

/// What a confirmed [`Confirm`] does
#[derive(Debug)]
pub(crate) enum ConfirmAction {
    /// Delete the `.fos` save at this path (and its script-extender co-save)
    DeleteSave(Utf8PathBuf),
    /// Remove the launch target with this key from the instance config
    RemoveExe(String),
    /// Remove the managed mod with this name
    RemoveMod(String),
    /// Replace the managed mod with this archive
    ReplaceMod { name: String, archive: String },
    /// Delete the mod-list separator at this index
    DeleteModSeparator { index: usize },
    /// Delete the plugin separator at this index
    DeletePluginSeparator { index: usize },
    /// Deploy the active profile
    Deploy,
    /// Purge the live deployment
    Purge,
    /// Clear a stale tracked-launch marker
    ClearLaunchMarker,
}

/// A dismiss-only reference modal with a title and key/description rows
#[derive(Debug)]
pub(crate) struct Info {
    pub(crate) title: String,
    pub(crate) entries: Vec<(String, String)>,
    pub(crate) state: ListCursor,
}

/// A dismiss-only diagnostics modal with a selectable findings list and live detail pane
#[derive(Debug)]
pub(crate) struct DoctorReport {
    pub(crate) report: Report,
    pub(crate) list: ListCursor,
}

#[cfg(test)]
#[path = "tests/modal.rs"]
mod tests;
