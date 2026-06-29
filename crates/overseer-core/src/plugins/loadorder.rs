use super::error::PluginError;
use super::metadata::PluginMeta;
use crate::fs;
use crate::instance::Instance;

/// One line of a profiles's plugin load order: plugin name and whether it's active
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub name: String,
    pub active: bool,
}

/// A profile's plugin load order, persisted as `plugins.txt`
#[derive(Debug, Clone)]
pub struct PluginLoadOrder {
    pub profile: String,
    pub plugins: Vec<PluginEntry>,
}

impl PluginLoadOrder {
    /// Load a profile's `plugins.txt`
    pub fn load(instance: &Instance, profile: &str) -> Result<Self, PluginError> {
        let path = instance.profile_dir(profile).join("plugins.txt");
        let text = fs::read_to_string_opt(&path)?.unwrap_or_default();

        Ok(Self {
            profile: profile.to_owned(),
            plugins: parse_plugins(&text),
        })
    }

    /// Write the profile's `plugins.txt`, creating the profile dir if necessary
    pub fn save(&self, instance: &Instance) -> Result<(), PluginError> {
        let path = instance.profile_dir(&self.profile).join("plugins.txt");
        fs::write_atomic(&path, self.to_plugins_string().as_bytes())?;
        Ok(())
    }

    /// Serialize to `plugins.txt` text: `*name` for active, `name` for inactive
    pub(crate) fn to_plugins_string(&self) -> String {
        let mut out = String::new();
        for entry in &self.plugins {
            if entry.active {
                out.push('*');
            }
            out.push_str(&entry.name);
            out.push('\n');
        }
        out
    }

    pub fn position(&self, name: &str) -> Option<usize> {
        self.plugins
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }

    fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    pub fn is_active(&self, name: &str) -> bool {
        self.position(name).is_some_and(|i| self.plugins[i].active)
    }

    fn set_active(&mut self, name: &str, active: bool) -> Result<(), PluginError> {
        let idx = self
            .position(name)
            .ok_or_else(|| PluginError::NotInLoadOrder(name.to_owned()))?;
        self.plugins[idx].active = active;
        Ok(())
    }

    /// Mark a plugin active in the load order.
    pub fn activate(&mut self, name: &str) -> Result<(), PluginError> {
        self.set_active(name, true)
    }
    /// Mark a plugin inactive in the load order.
    pub fn deactivate(&mut self, name: &str) -> Result<(), PluginError> {
        self.set_active(name, false)
    }

    /// Reconcile the load order with the plugins actually discovered in the profile's enabled mods
    pub fn reconcile(&mut self, discovered: &[PluginMeta]) -> bool {
        let before = self.plugins.clone();

        // Drop entries that are no longer discovered
        self.plugins.retain(|e| {
            discovered
                .iter()
                .any(|m| m.name.eq_ignore_ascii_case(&e.name))
        });

        // Append newly discovered plugins
        for m in discovered {
            if !self.contains(&m.name) {
                self.plugins.push(PluginEntry {
                    name: m.name.clone(),
                    active: true,
                });
            }
        }

        // Stabke sort masters before normal plugins
        self.plugins
            .sort_by_key(|e| !is_master(&e.name, discovered));
        self.plugins != before
    }
}

fn is_master(name: &str, discovered: &[PluginMeta]) -> bool {
    discovered
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))
        .is_some_and(|m| m.is_master)
}

/// Parse `plugins.txt`
fn parse_plugins(text: &str) -> Vec<PluginEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (active, name) = match line.strip_prefix('*') {
                Some(rest) => (true, rest.trim()),
                None => (false, line),
            };
            (!name.is_empty()).then(|| PluginEntry {
                name: name.to_owned(),
                active,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::temp_instance;

    fn meta(name: &str, is_master: bool) -> PluginMeta {
        crate::test_support::plugin_meta(name, is_master, false, &[])
    }

    fn order_of(lo: &PluginLoadOrder) -> Vec<&str> {
        lo.plugins.iter().map(|e| e.name.as_str()).collect()
    }

    fn lo(profile: &str, plugins: Vec<PluginEntry>) -> PluginLoadOrder {
        PluginLoadOrder {
            profile: profile.to_owned(),
            plugins,
        }
    }

    fn active(name: &str) -> PluginEntry {
        PluginEntry {
            name: name.to_owned(),
            active: true,
        }
    }

    fn inactive(name: &str) -> PluginEntry {
        PluginEntry {
            name: name.to_owned(),
            active: false,
        }
    }

    // --- parse / serialize (asterisk format) ---

    #[test]
    fn parses_asterisk_active_and_bare_inactive() {
        let plugins = parse_plugins("*Active.esp\nInactive.esp\n");
        assert_eq!(
            plugins,
            vec![active("Active.esp"), inactive("Inactive.esp")]
        );
    }

    #[test]
    fn parse_skips_blank_and_comment_lines() {
        let plugins = parse_plugins("# header\n\n*A.esp\nB.esp\n");
        assert_eq!(plugins, vec![active("A.esp"), inactive("B.esp")]);
    }

    #[test]
    fn serialize_uses_asterisk_for_active_only() {
        let order = lo("P", vec![active("On.esp"), inactive("Off.esp")]);
        assert_eq!(order.to_plugins_string(), "*On.esp\nOff.esp\n");
    }

    #[test]
    fn serialize_parse_round_trips() {
        let order = lo(
            "P",
            vec![active("A.esp"), inactive("B.esp"), active("C.esp")],
        );
        assert_eq!(parse_plugins(&order.to_plugins_string()), order.plugins);
    }

    // --- load / save ---

    #[test]
    fn load_missing_file_is_empty() {
        let (_t, instance) = temp_instance();
        let order = PluginLoadOrder::load(&instance, "Default").expect("load");
        assert!(order.plugins.is_empty());
        assert_eq!(order.profile, "Default");
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_t, instance) = temp_instance();
        let order = lo("Default", vec![active("A.esp"), inactive("B.esp")]);
        order.save(&instance).expect("save");
        let loaded = PluginLoadOrder::load(&instance, "Default").expect("load");
        assert_eq!(loaded.plugins, order.plugins);
    }

    #[test]
    fn save_writes_plugins_txt_in_profile_dir() {
        let (_t, instance) = temp_instance();
        lo("Survival", vec![active("A.esp")])
            .save(&instance)
            .expect("save");
        assert!(
            instance
                .profile_dir("Survival")
                .join("plugins.txt")
                .exists()
        );
    }

    // --- activate / deactivate ---

    #[test]
    fn activate_and_deactivate_toggle_state() {
        let mut order = lo("P", vec![inactive("M.esp")]);
        order.activate("m.esp").expect("activate");
        assert!(order.is_active("M.esp"));
        order.deactivate("M.ESP").expect("deactivate");
        assert!(!order.is_active("M.esp"));
    }

    #[test]
    fn activate_missing_is_an_error() {
        let mut order = lo("P", vec![]);
        assert!(matches!(
            order.activate("ghost.esp").expect_err("err"),
            PluginError::NotInLoadOrder(_)
        ));
    }

    // --- reconcile ---

    #[test]
    fn reconcile_appends_new_plugins_active() {
        let mut order = lo("P", vec![active("Existing.esp")]);
        let discovered = [meta("Existing.esp", false), meta("New.esp", false)];
        let changed = order.reconcile(&discovered);
        assert!(changed);
        assert_eq!(order_of(&order), ["Existing.esp", "New.esp"]);
        assert!(order.is_active("New.esp"));
    }

    #[test]
    fn reconcile_drops_vanished_plugins() {
        let mut order = lo("P", vec![active("Keep.esp"), active("Gone.esp")]);
        let changed = order.reconcile(&[meta("Keep.esp", false)]);
        assert!(changed);
        assert_eq!(order_of(&order), ["Keep.esp"]);
    }

    #[test]
    fn reconcile_sorts_masters_before_normal_plugins() {
        // Stored in a load-order-invalid arrangement (a normal plugin before a master).
        let mut order = lo("P", vec![active("Patch.esp"), active("Core.esm")]);
        let discovered = [meta("Patch.esp", false), meta("Core.esm", true)];
        let changed = order.reconcile(&discovered);
        assert!(changed, "the master had to move up");
        assert_eq!(order_of(&order), ["Core.esm", "Patch.esp"]);
    }

    #[test]
    fn reconcile_is_stable_within_master_and_normal_groups() {
        let mut order = lo("P", vec![]);
        // Discovery order: m1(normal), A(master), m2(normal), B(master).
        let discovered = [
            meta("m1.esp", false),
            meta("A.esm", true),
            meta("m2.esp", false),
            meta("B.esm", true),
        ];
        order.reconcile(&discovered);
        // Masters first (A, B in their relative order), then normals (m1, m2).
        assert_eq!(order_of(&order), ["A.esm", "B.esm", "m1.esp", "m2.esp"]);
    }

    #[test]
    fn reconcile_preserves_active_state_of_existing() {
        let mut order = lo("P", vec![inactive("Keep.esp")]);
        order.reconcile(&[meta("Keep.esp", false)]);
        assert!(
            !order.is_active("Keep.esp"),
            "existing inactive stays inactive"
        );
    }

    #[test]
    fn reconcile_reports_no_change_when_in_sync_and_sorted() {
        let mut order = lo("P", vec![active("Core.esm"), active("Patch.esp")]);
        let discovered = [meta("Core.esm", true), meta("Patch.esp", false)];
        assert!(!order.reconcile(&discovered));
    }
}
