//! Main view key commands: the key->command table, execution, and busy-state policy

use super::{App, Focus, OperationKind, RefreshCause, SelectKind, Workspace};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

/// A main-view action resolved from a key press, independent of its binding
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Command {
    Move(isize),
    Reorder(isize),
    ToggleFocus,
    ToggleSelected,
    SwitchWorkspace(Workspace),
    CycleWorkspace(isize),
    OpenSelect(SelectKind),
    OpenHelp,
    OpenDoctor,
    OpenRenameMod,
    OpenNewSeparator,
    RefreshWorkspace,
    CycleSort,
    ToggleSortDir,
    DeleteSave,
    DeleteSeparator,
    ToggleLocalSaves,
    FilterConflicts,
    RemoveMod,
    ReplaceMod,
    Deploy,
    Purge,
}

/// Whether a command runs or is blocked while a background operation is active
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BusyPolicy {
    Allowed,
    Blocked(OperationKind),
    BlockedGeneric,
}

/// The slice of view state a busy-state decision depends on
#[derive(Clone, Copy)]
pub(super) struct Context {
    pub(super) focus: Focus,
    pub(super) workspace: Workspace,
}

impl App {
    /// Map a key press to its command; a pure table with no view-state effects
    pub(super) fn command_for(&self, key: KeyEvent) -> Option<Command> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => Some(Command::Move(1)),
            KeyCode::Up | KeyCode::Char('k') => Some(Command::Move(-1)),
            KeyCode::Char('J') => Some(Command::Reorder(1)),
            KeyCode::Char('K') => Some(Command::Reorder(-1)),
            KeyCode::Tab => Some(Command::ToggleFocus),
            KeyCode::Char(' ') | KeyCode::Enter => Some(Command::ToggleSelected),
            KeyCode::Char(']') => Some(Command::CycleWorkspace(1)),
            KeyCode::Char('[') => Some(Command::CycleWorkspace(-1)),
            KeyCode::Char(c) if Workspace::from_key(c).is_some() => {
                Workspace::from_key(c).map(Command::SwitchWorkspace)
            }
            KeyCode::Char(c) if SelectKind::from_toggle_key(c).is_some() => {
                SelectKind::from_toggle_key(c).map(Command::OpenSelect)
            }
            KeyCode::Char('?') => Some(Command::OpenHelp),
            KeyCode::Char('d') => Some(Command::OpenDoctor),
            KeyCode::Char('R') => Some(Command::OpenRenameMod),
            KeyCode::Char('A') => Some(Command::OpenNewSeparator),
            KeyCode::Char('r') => Some(Command::RefreshWorkspace),
            KeyCode::Char('o') => Some(Command::CycleSort),
            KeyCode::Char('O') => Some(Command::ToggleSortDir),
            KeyCode::Char('X') => Some(Command::DeleteSave),
            KeyCode::Char('x') | KeyCode::Delete => Some(Command::DeleteSeparator),
            KeyCode::Char('L') => Some(Command::ToggleLocalSaves),
            KeyCode::Char('f') => Some(Command::FilterConflicts),
            KeyCode::Char('m') => Some(Command::RemoveMod),
            KeyCode::Char('e') => Some(Command::ReplaceMod),
            KeyCode::Char('D') => Some(Command::Deploy),
            KeyCode::Char('P') => Some(Command::Purge),
            _ => None,
        }
    }

    /// Perform a resolved command by dispatching to the existing action methods
    pub(super) fn execute(&mut self, command: Command) {
        match command {
            Command::Move(delta) => self.move_main_selection(delta),
            Command::Reorder(delta) => self.reorder_selected(delta),
            Command::ToggleFocus => self.toggle_focus(),
            Command::ToggleSelected => self.toggle_selected(),
            Command::SwitchWorkspace(workspace) => self.switch_workspace(workspace),
            Command::CycleWorkspace(delta) => self.switch_workspace(self.workspace.cycle(delta)),
            Command::OpenSelect(kind) => self.open_select(kind),
            Command::OpenHelp => self.open_help(),
            Command::OpenDoctor => self.open_doctor(),
            Command::OpenRenameMod => self.open_rename_mod(),
            Command::OpenNewSeparator => self.open_new_separator(),
            Command::RefreshWorkspace => {
                let workspace = self.workspace;
                workspace.refresh(self, RefreshCause::Explicit);
            }
            Command::CycleSort => {
                let workspace = self.workspace;
                workspace.cycle_sort(self);
            }
            Command::ToggleSortDir => {
                let workspace = self.workspace;
                workspace.toggle_sort_dir(self);
            }
            Command::DeleteSave => self.begin_delete_save(),
            Command::DeleteSeparator => self.begin_delete_separator(),
            Command::ToggleLocalSaves => self.toggle_local_saves(),
            Command::FilterConflicts => self.filter_conflicts_to_selection(),
            Command::RemoveMod if self.focus == Focus::Mods => self.begin_remove_mod(),
            Command::ReplaceMod if self.focus == Focus::Mods => self.begin_replace_mod(),
            // no-op outside the Mods pane, but recognized focus-free so busy-state still notes
            Command::RemoveMod | Command::ReplaceMod => {}
            Command::Deploy => self.begin_deploy(),
            Command::Purge => self.begin_purge(),
        }
    }
}

impl Command {
    /// Classify how this command behaves while a background operation is running
    pub(super) fn busy_policy(&self, ctx: Context) -> BusyPolicy {
        match self {
            Self::Move(_)
            | Self::ToggleFocus
            | Self::SwitchWorkspace(_)
            | Self::CycleWorkspace(_)
            | Self::OpenHelp
            | Self::CycleSort
            | Self::ToggleSortDir
            | Self::FilterConflicts => BusyPolicy::Allowed,
            Self::Deploy => BusyPolicy::Blocked(OperationKind::Deploy),
            Self::Purge => BusyPolicy::Blocked(OperationKind::Purge),
            Self::OpenDoctor => BusyPolicy::Blocked(OperationKind::Doctor),
            Self::RemoveMod => BusyPolicy::Blocked(OperationKind::Remove),
            Self::ReplaceMod => BusyPolicy::Blocked(OperationKind::Replace),
            Self::RefreshWorkspace => match ctx.workspace {
                Workspace::Conflicts => BusyPolicy::Blocked(OperationKind::ScanConflicts),
                Workspace::Downloads => BusyPolicy::Blocked(OperationKind::RefreshDownloads),
                Workspace::Saves => BusyPolicy::Blocked(OperationKind::RefreshSaves),
                Workspace::Plugins => BusyPolicy::BlockedGeneric,
            },
            Self::ToggleSelected
                if ctx.focus == Focus::Workspace && ctx.workspace == Workspace::Downloads =>
            {
                BusyPolicy::Blocked(OperationKind::Install)
            }
            Self::ToggleSelected
            | Self::Reorder(_)
            | Self::OpenSelect(_)
            | Self::OpenRenameMod
            | Self::OpenNewSeparator
            | Self::DeleteSave
            | Self::DeleteSeparator
            | Self::ToggleLocalSaves => BusyPolicy::BlockedGeneric,
        }
    }
}

#[cfg(test)]
#[path = "tests/command.rs"]
mod tests;
