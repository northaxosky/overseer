//! Main-view mutations: toggling, reordering, deploying, and purging.

use anyhow::Result;
use overseer_core::apply;
use overseer_core::deploy::NullSink;
use overseer_core::instance::ModKind;
use overseer_core::plugins::discover_plugins;

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

    /// Move the selected mod up or down in priority, in display (MO2) order
    fn shift_selected_mod(&mut self, display_delta: isize) -> bool {
        if self.focus != Focus::Mods {
            return false;
        }
        let rows = self.visible_rows();
        let Some(p) = self.mods_state.selected() else {
            return false;
        };
        let q = p as isize + display_delta;
        if q < 0 || q >= rows.len() as isize {
            return false;
        }
        let Some(&a) = rows.get(p) else {
            return false;
        };
        let b = rows[q as usize];
        let mods = &self.session.profile.mods;
        if mods[a].kind != ModKind::Managed {
            self.note("Only mods can be reordered");
            return false;
        }
        if mods[b].kind == ModKind::Foreign {
            self.note("Can't reorder past a base-game entry");
            return false;
        }
        if mods[b].kind == ModKind::Separator && self.is_collapsed(b) {
            self.note("Expand the group to move past it");
            return false;
        }

        // Both endpoints visible, they are model-adjacent: plain swap is clean
        self.session.profile.mods.swap(a, b);
        self.mods_state.select(Some(q as usize));
        self.mark_conflicts_stale();
        true
    }

    /// Flip the mod's `enabled`, or act on the focused workspace pane
    fn flip_selected(&mut self) -> bool {
        match self.focus {
            Focus::Mods => {
                let Some(m) = self.selected_mod() else {
                    return false;
                };
                if self.session.profile.mods[m].kind == ModKind::Separator {
                    self.toggle_collapsed(m);
                    return false;
                }
                let entry = &mut self.session.profile.mods[m];
                if entry.kind != ModKind::Managed {
                    self.note("Only managed mods can be toggled");
                    return false;
                }
                entry.enabled = !entry.enabled;
                self.mark_conflicts_stale();
                true
            }
            Focus::Workspace => {
                let ws = self.workspace;
                ws.primary(self)
            }
        }
    }

    /// Save the profile and load order, re-deriving plugins
    fn persist(&mut self) -> Result<()> {
        self.session.profile.save(&self.session.instance)?;
        self.session.discovered = discover_plugins(&self.session.instance, &self.session.profile)?;
        self.session.order.reconcile(&self.session.discovered);
        self.session.order.save(&self.session.instance)?;
        self.clamp_plugins_selection();
        Ok(())
    }

    /// Deploy the active profile & report the outcome
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

    /// Purge the live deployment & report the outcome
    pub(super) fn purge(&mut self) {
        match apply::purge(&self.session.instance, &NullSink) {
            Ok(()) => self.ok("Purged the live deployment"),
            Err(e) => self.fail(format!("Purge failed: {e}")),
        }
        self.refresh_status();
    }

    /// Refresh cached deployment status after deploy/purge without surfacing probe failures
    fn refresh_status(&mut self) {
        self.session.status = apply::status(&self.session.instance).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "could not read deployment status");
            None
        });
    }
}

impl Workspace {
    /// The Enter/Space primary action; returns `true` when persistent state changed and should be saved
    fn primary(self, app: &mut App) -> bool {
        match self {
            Workspace::Plugins => app.toggle_selected_plugin_row(),
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
#[path = "tests/actions.rs"]
mod tests;
