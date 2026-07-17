//! Shared declared-master dependency graph, built once and read by both reconcile and validation.

use super::{PluginEntry, PluginMeta};
use std::collections::{HashMap, HashSet};

/// A directed graph of declared-master dependencies between the plugins in an order.
pub(super) struct DependencyGraph<'a> {
    /// For each node, the plugins that declare it as a master (edge targets).
    pub successors: Vec<Vec<usize>>,
    /// For each node, whether it is a master file (by header flag).
    pub master_flags: Vec<bool>,
    /// For each node, whether it declares itself as a master (a trivial cycle).
    pub self_loops: Vec<bool>,
    /// Strongly connected component id for each node.
    scc_ids: Vec<usize>,
    /// The lowercased name of each node, indexed by node position.
    keys: Vec<String>,
    /// Discovered metadata by lowercased name (first occurrence wins).
    metadata: HashMap<String, &'a PluginMeta>,
    /// Node position by lowercased name (first occurrence wins).
    node_indices: HashMap<String, usize>,
}

impl<'a> DependencyGraph<'a> {
    /// Build the graph from an order's entries and the discovered plugin metadata.
    pub fn build(entries: &[PluginEntry], discovered: &'a [PluginMeta]) -> Self {
        let keys: Vec<_> = entries
            .iter()
            .map(|entry| entry.name.to_ascii_lowercase())
            .collect();
        let metadata = discovered
            .iter()
            .fold(HashMap::new(), |mut metadata, meta| {
                metadata
                    .entry(meta.name.to_ascii_lowercase())
                    .or_insert(meta);
                metadata
            });
        let node_indices =
            keys.iter()
                .enumerate()
                .fold(HashMap::new(), |mut indices, (index, key)| {
                    indices.entry(key.clone()).or_insert(index);
                    indices
                });
        let master_flags = keys
            .iter()
            .map(|key| metadata.get(key).is_some_and(|meta| meta.is_master))
            .collect();
        let mut successors = vec![Vec::new(); entries.len()];
        let mut self_loops = vec![false; entries.len()];
        let mut edges = HashSet::new();

        for (dependant, key) in keys.iter().enumerate() {
            let Some(meta) = metadata.get(key) else {
                continue;
            };
            for dependency in &meta.masters {
                let dependency_key = dependency.to_ascii_lowercase();
                let Some(&dependency) = node_indices.get(&dependency_key) else {
                    continue;
                };
                if edges.insert((dependency, dependant)) {
                    successors[dependency].push(dependant);
                    self_loops[dependant] |= dependency == dependant;
                }
            }
        }
        let scc_ids = compute_scc_ids(&successors);

        Self {
            successors,
            master_flags,
            self_loops,
            scc_ids,
            keys,
            metadata,
            node_indices,
        }
    }

    /// The lowercased name of the node at `index`.
    pub fn key(&self, index: usize) -> &str {
        &self.keys[index]
    }

    /// The discovered metadata for the node at `index`, if the plugin was discovered.
    pub fn metadata(&self, index: usize) -> Option<&'a PluginMeta> {
        self.metadata.get(self.key(index)).copied()
    }

    /// The node position for a lowercased `key`, if present in the order.
    pub fn node_index(&self, key: &str) -> Option<usize> {
        self.node_indices.get(key).copied()
    }

    /// Whether two nodes belong to the same strongly connected component.
    pub fn same_scc(&self, a: usize, b: usize) -> bool {
        self.scc_ids[a] == self.scc_ids[b]
    }

    /// Declared-dependency cycles with members and groups in node order.
    pub fn cyclic_components(&self) -> Vec<Vec<usize>> {
        let component_count = self.scc_ids.iter().max().map_or(0, |id| id + 1);
        let mut components = vec![Vec::new(); component_count];
        for (node, &scc_id) in self.scc_ids.iter().enumerate() {
            components[scc_id].push(node);
        }
        components.retain(|component| {
            component.len() > 1 || component.first().is_some_and(|&node| self.self_loops[node])
        });
        for component in &mut components {
            component.sort_unstable();
        }
        components.sort_by_key(|component| component[0]);
        components
    }
}

/// Compute a deterministic strongly connected component id for each node.
fn compute_scc_ids(successors: &[Vec<usize>]) -> Vec<usize> {
    struct Tarjan<'a> {
        successors: &'a [Vec<usize>],
        next_index: usize,
        indices: Vec<Option<usize>>,
        lowlinks: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        scc_ids: Vec<usize>,
        next_scc_id: usize,
    }

    impl Tarjan<'_> {
        fn visit(&mut self, node: usize) {
            let node_index = self.next_index;
            self.next_index += 1;
            self.indices[node] = Some(node_index);
            self.lowlinks[node] = node_index;
            self.stack.push(node);
            self.on_stack[node] = true;

            for &successor in &self.successors[node] {
                if self.indices[successor].is_none() {
                    self.visit(successor);
                    self.lowlinks[node] = self.lowlinks[node].min(self.lowlinks[successor]);
                } else if self.on_stack[successor] {
                    self.lowlinks[node] = self.lowlinks[node]
                        .min(self.indices[successor].expect("visited node has an index"));
                }
            }

            if self.lowlinks[node] != node_index {
                return;
            }

            loop {
                let member = self.stack.pop().expect("component root is on stack");
                self.on_stack[member] = false;
                self.scc_ids[member] = self.next_scc_id;
                if member == node {
                    break;
                }
            }
            self.next_scc_id += 1;
        }
    }

    let node_count = successors.len();
    let mut tarjan = Tarjan {
        successors,
        next_index: 0,
        indices: vec![None; node_count],
        lowlinks: vec![0; node_count],
        stack: Vec::new(),
        on_stack: vec![false; node_count],
        scc_ids: vec![usize::MAX; node_count],
        next_scc_id: 0,
    };
    for node in 0..node_count {
        if tarjan.indices[node].is_none() {
            tarjan.visit(node);
        }
    }
    tarjan.scc_ids
}
