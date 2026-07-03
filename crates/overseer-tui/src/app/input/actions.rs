//! Main-view mutations: toggling, reordering, deploying, and purging.

use anyhow::Result;
use overseer_core::apply;
use overseer_core::deploy::NullSink;
use overseer_core::instance::ModKind;
use overseer_core::plugins::discover_plugins;

use super::clamp_selection;
use crate::app::{App, Focus, Workspace};

impl App {
    /// Toggle the selected item in the focused pane & report the outcome
    pub(super) fn toggle_selected(&mut self) {
        if !self.flip_selected() {
            return;
        }
        match self.persist() {
            Ok(()) => self.ok("Saved"),
            Err(e) => self.fail(format!("Error: {e}")),
        }
    }

    /// Move the selected mod up or down in priority
    pub(super) fn reorder_selected(&mut self, delta: isize) {
        if !self.shift_selected_mod(delta) {
            return;
        }
        match self.session.profile.save(&self.session.instance) {
            Ok(()) => self.ok("Saved"),
            Err(e) => self.fail(format!("Error: {e}")),
        }
    }

    /// Move the selected mod one step in priority
    fn shift_selected_mod(&mut self, delta: isize) -> bool {
        if self.focus != Focus::Mods {
            return false;
        }
        let Some(i) = self.mods_state.selected() else {
            return false;
        };
        let target = i as isize + delta;
        if target < 0 || target >= self.session.profile.mods.len() as isize {
            return false;
        }
        let name = self.session.profile.mods[i].name.clone();
        let moved = if delta < 0 {
            self.session.profile.move_up(&name).is_ok()
        } else {
            self.session.profile.move_down(&name).is_ok()
        };
        if moved {
            self.mods_state.select(Some(target as usize));
            self.mark_conflicts_stale();
        }
        moved
    }

    /// Flip the mod's `enabled` / plugin's `active`
    fn flip_selected(&mut self) -> bool {
        match self.focus {
            Focus::Mods => {
                if let Some(i) = self.mods_state.selected() {
                    let m = &mut self.session.profile.mods[i];
                    // Only Managed mods serialize an enabled flag; flipping a DLC/CC; (Foreign) or Separator would be a silent no-op on save.
                    if m.kind != ModKind::Managed {
                        self.note("Only managed mods can be toggled");
                        return false;
                    }
                    m.enabled = !m.enabled;
                    // The enabled set drives conflict detection; invalidate the scan.
                    self.mark_conflicts_stale();
                    return true;
                }
            }
            Focus::Workspace => {
                let ws = self.workspace;
                return ws.primary(self);
            }
        }
        false
    }

    /// Save the profile and load order, re-deriving plugins
    fn persist(&mut self) -> Result<()> {
        self.session.profile.save(&self.session.instance)?;
        self.session.discovered = discover_plugins(&self.session.instance, &self.session.profile)?;
        self.session.order.reconcile(&self.session.discovered);
        self.session.order.save(&self.session.instance)?;
        clamp_selection(&mut self.plugins_state, self.session.order.plugins.len());
        Ok(())
    }

    pub(super) fn deploy(&mut self) {
        match apply::deploy_profile(
            &self.session.instance,
            &self.session.profile.name,
            &NullSink,
        ) {
            Ok(d) => self.ok(format!("Deployed {} files", d.record.entries.len())),
            Err(e) => self.fail(format!("Deploy failed: {e}")),
        }
        self.refresh_status();
    }

    pub(super) fn purge(&mut self) {
        match apply::purge(&self.session.instance, &NullSink) {
            Ok(()) => self.ok("Purged the live deployment"),
            Err(e) => self.fail(format!("Purge failed: {e}")),
        }
        self.refresh_status();
    }

    /// Refresh cached deployment status after deploy/purge without surfacing probe failures.
    fn refresh_status(&mut self) {
        self.session.status = apply::status(&self.session.instance).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "could not read deployment status");
            None
        });
    }
}

impl Workspace {
    /// The Enter/Space primary action; returns `true` when persistent state changed and should be saved.
    fn primary(self, app: &mut App) -> bool {
        match self {
            Workspace::Plugins => {
                if let Some(i) = app.plugins_state.selected() {
                    let p = &mut app.session.order.plugins[i];
                    p.active = !p.active;
                    return true;
                }
                false
            }
            Workspace::Conflicts => {
                app.note("Conflicts are read-only");
                false
            }
            Workspace::Downloads => {
                app.begin_install_selected();
                false
            }
            Workspace::Saves => {
                app.note("Press x to delete a save");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_a_non_managed_mod_is_refused() {
        use overseer_core::instance::ModKind;
        let mut app = App::sample();
        app.session
            .profile
            .mods
            .push(overseer_core::instance::ModListEntry {
                name: "DLCRobot".to_owned(),
                enabled: true,
                kind: ModKind::Foreign,
            });
        let foreign = app.session.profile.mods.len() - 1;
        app.mods_state.select(Some(foreign));
        assert!(!app.flip_selected(), "foreign entries can't be flipped");
        assert!(app.session.profile.mods[foreign].enabled, "left unchanged");
        assert!(app.message.is_some(), "user is told why");
    }

    #[test]
    fn flip_toggles_the_selected_mod() {
        let mut app = App::sample();
        assert!(app.session.profile.mods[0].enabled);
        assert!(app.flip_selected());
        assert!(!app.session.profile.mods[0].enabled);
    }

    #[test]
    fn flip_toggles_the_selected_plugin() {
        let mut app = App::sample();
        app.focus = Focus::Workspace;
        assert!(app.session.order.plugins[0].active);
        assert!(app.flip_selected());
        assert!(!app.session.order.plugins[0].active);
    }

    #[test]
    fn flip_in_the_conflicts_workspace_is_read_only() {
        use crate::app::Workspace;
        let mut app = App::sample();
        app.focus = Focus::Workspace;
        app.workspace = Workspace::Conflicts;
        let before = app.session.order.plugins[0].active;
        assert!(!app.flip_selected(), "conflicts mutate nothing");
        assert_eq!(
            app.session.order.plugins[0].active, before,
            "plugin active flags are untouched"
        );
        assert!(app.message.is_some(), "the user is told it is read-only");
    }

    #[test]
    fn flipping_a_mod_marks_the_conflicts_scan_stale() {
        use crate::app::ConflictsStatus;
        let mut app = App::sample();
        app.conflicts.status = ConflictsStatus::Ready(Vec::new());
        assert!(app.flip_selected(), "a managed mod flips");
        assert!(
            matches!(app.conflicts.status, ConflictsStatus::Stale),
            "changing the enabled set invalidates the scan"
        );
    }

    #[test]
    fn shift_moves_the_selected_mod_and_keeps_selection() {
        let mut app = App::sample();
        assert!(app.shift_selected_mod(1));
        assert_eq!(app.session.profile.mods[1].name, "CoolMod");
        assert_eq!(app.mods_state.selected(), Some(1));
        assert!(app.shift_selected_mod(-1));
        assert_eq!(app.session.profile.mods[0].name, "CoolMod");
        assert_eq!(app.mods_state.selected(), Some(0));
    }

    #[test]
    fn shift_is_a_noop_at_edges_and_in_the_plugins_pane() {
        let mut app = App::sample();
        assert!(!app.shift_selected_mod(-1)); // at the top
        assert_eq!(app.mods_state.selected(), Some(0));
        app.focus = Focus::Workspace;
        assert!(!app.shift_selected_mod(1)); // unsupported pane
    }
}
