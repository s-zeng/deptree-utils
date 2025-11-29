use crate::{GraphConfig, GraphData, GraphEdge, GraphNode};
use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet, VecDeque};

/// Identifier trait for nodes stored in the dependency graph.
/// Implementations should provide a dotted string representation and path segments
/// for namespace grouping.
pub trait GraphId: Eq + std::hash::Hash + Clone {
    fn to_dotted(&self) -> String;
    fn segments(&self) -> Vec<String>;
}

fn sanitize_mermaid_id(name: &str) -> String {
    name.replace('.', "_")
}

struct DotNodeSpec {
    name: String,
    attrs: String,
}

impl DotNodeSpec {
    fn render(&self, indent: &str) -> String {
        let attrs = if self.attrs.is_empty() {
            String::new()
        } else {
            format!(" {}", self.attrs)
        };
        format!("{indent}    \"{}\"{attrs};\n", self.name)
    }
}

#[derive(Clone)]
enum MermaidShape {
    Module,
    Script,
    Namespace,
}

#[derive(Clone)]
struct MermaidNodeSpec {
    id: String,
    label: String,
    shape: MermaidShape,
}

struct MermaidRenderArgs<'a, T> {
    highlight_set: Option<&'a HashSet<T>>,
    specs: &'a HashMap<String, MermaidNodeSpec>,
}

impl MermaidNodeSpec {
    fn render_definition(&self, indent: &str, highlighted: bool) -> String {
        let base = match self.shape {
            MermaidShape::Script => format!("{indent}    {}[\"{}\"]\n", self.id, self.label),
            MermaidShape::Namespace => {
                format!("{indent}    {}{{{{\"{}\"}}}} \n", self.id, self.label)
            }
            MermaidShape::Module => format!("{indent}    {}(\"{}\")\n", self.id, self.label),
        };

        if highlighted {
            format!("{base}{indent}    class {} highlighted\n", self.id)
        } else {
            base
        }
    }

    fn render_inline(&self) -> String {
        match self.shape {
            MermaidShape::Script => format!("{}[\"{}\"]", self.id, self.label),
            MermaidShape::Namespace => format!("{}{{{{\"{}\"}}}}", self.id, self.label),
            MermaidShape::Module => format!("{}(\"{}\")", self.id, self.label),
        }
    }
}

/// Rendering mode for Cytoscape data generation
enum CytoscapeMode<'a, T> {
    Full,
    Filtered(&'a HashSet<T>),
    Highlighted(&'a HashSet<T>),
}

/// Selection mode for rendering/filtering
enum NodeSelection<'a, T> {
    Full,
    Filtered(&'a HashSet<T>),
    Highlighted,
}

#[derive(Debug, Clone)]
struct NamespaceTree<T> {
    path: Vec<String>,
    id: Option<T>,
    children: Vec<NamespaceTree<T>>,
    grouped: bool,
}

impl<T: GraphId> NamespaceTree<T> {
    fn new(path: Vec<String>) -> Self {
        Self {
            path,
            id: None,
            children: Vec::new(),
            grouped: false,
        }
    }

    fn insert(&mut self, module: &T) {
        self.insert_parts(&module.segments(), module);
    }

    fn insert_parts(&mut self, parts: &[String], module: &T) {
        if parts.is_empty() {
            self.id = Some(module.clone());
            return;
        }

        let child_name = &parts[0];
        let mut child_path = self.path.clone();
        child_path.push(child_name.clone());

        let child = self
            .children
            .iter_mut()
            .find(|c| c.path.last() == Some(child_name));

        if let Some(existing) = child {
            existing.insert_parts(&parts[1..], module);
        } else {
            let mut new_child = NamespaceTree::new(child_path);
            new_child.insert_parts(&parts[1..], module);
            self.children.push(new_child);
        }
    }

    fn finalize(&mut self) {
        for child in &mut self.children {
            child.finalize();
        }
        self.children.sort_by(|a, b| a.path.cmp(&b.path));
        self.grouped = !self.path.is_empty() && self.children.len() >= 2;
    }

    fn find(&self, path: &[String]) -> Option<&NamespaceTree<T>> {
        if path.is_empty() {
            return Some(self);
        }

        self.children
            .iter()
            .find(|c| c.path.last() == path.first())
            .and_then(|child| child.find(&path[1..]))
    }

    fn is_group_only(&self, path: &[String]) -> bool {
        self.find(path)
            .map(|node| node.grouped && node.id.is_some())
            .unwrap_or(false)
    }

    fn direct_concrete_children(&self) -> Vec<T> {
        self.children.iter().filter_map(|c| c.id.clone()).collect()
    }

    fn child_groups(&self) -> impl Iterator<Item = &NamespaceTree<T>> {
        self.children.iter()
    }

    fn collect_leaf_descendants(&self, acc: &mut Vec<T>) {
        if self.children.is_empty() {
            if let Some(id) = &self.id {
                acc.push(id.clone());
            }
            return;
        }

        for child in &self.children {
            child.collect_leaf_descendants(acc);
        }
    }

    fn collect_ungrouped_modules(&self, acc: &mut Vec<T>) {
        if self.grouped {
            for child in &self.children {
                child.collect_ungrouped_modules(acc);
            }
            return;
        }

        for child in &self.children {
            if let Some(id) = &child.id {
                acc.push(id.clone());
            }
            child.collect_ungrouped_modules(acc);
        }
    }
}

#[derive(Debug)]
struct NamespaceForest<T> {
    internal: NamespaceTree<T>,
    scripts: NamespaceTree<T>,
}

pub struct DependencyGraph<T: GraphId> {
    graph: DiGraph<T, ()>,
    node_indices: HashMap<T, NodeIndex>,
    scripts: HashSet<T>,
    namespace_packages: HashSet<T>,
}

impl<T: GraphId> DependencyGraph<T> {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
            scripts: HashSet::new(),
            namespace_packages: HashSet::new(),
        }
    }

    pub fn mark_as_script(&mut self, module: &T) {
        self.scripts.insert(module.clone());
    }

    pub fn is_script(&self, module: &T) -> bool {
        self.scripts.contains(module)
    }

    pub fn mark_as_namespace_package(&mut self, module: &T) {
        self.namespace_packages.insert(module.clone());
    }

    pub fn is_namespace_package(&self, module: &T) -> bool {
        self.namespace_packages.contains(module)
    }

    pub fn ensure_node(&mut self, module: T) {
        let _ = self.get_or_create_node(module);
    }

    fn get_or_create_node(&mut self, module: T) -> NodeIndex {
        if let Some(&idx) = self.node_indices.get(&module) {
            idx
        } else {
            let idx = self.graph.add_node(module.clone());
            self.node_indices.insert(module, idx);
            idx
        }
    }

    pub fn add_dependency(&mut self, from: T, to: T) {
        let from_idx = self.get_or_create_node(from);
        let to_idx = self.get_or_create_node(to);
        self.graph.add_edge(from_idx, to_idx, ());
    }

    fn select_visible_nodes(
        &self,
        selection: NodeSelection<'_, T>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> Vec<NodeIndex> {
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        nodes
            .into_iter()
            .filter(|idx| match selection {
                NodeSelection::Full | NodeSelection::Highlighted => true,
                NodeSelection::Filtered(set) => set.contains(&self.graph[*idx]),
            })
            .filter(|idx| {
                include_namespace_packages || !self.is_namespace_package(&self.graph[*idx])
            })
            .filter(|idx| {
                include_orphans
                    || self
                        .graph
                        .neighbors_directed(*idx, Direction::Incoming)
                        .count()
                        > 0
                    || self
                        .graph
                        .neighbors_directed(*idx, Direction::Outgoing)
                        .count()
                        > 0
            })
            .collect()
    }

    fn collect_edges(
        &self,
        node_set: &HashSet<NodeIndex>,
        include_namespace_packages: bool,
    ) -> Vec<(T, T)> {
        let mut edges = Vec::new();

        if !include_namespace_packages {
            for from_idx in self.graph.node_indices() {
                if !node_set.contains(&from_idx) {
                    continue;
                }
                let from_module = &self.graph[from_idx];

                for to_idx in self.graph.neighbors(from_idx) {
                    let to_module = &self.graph[to_idx];

                    if self.is_namespace_package(to_module) {
                        let mut visited = HashSet::new();
                        self.find_transitive_non_namespace_targets(
                            to_idx,
                            &mut visited,
                            node_set,
                            &mut |target_idx| {
                                let target_module = &self.graph[target_idx];
                                edges.push((from_module.clone(), target_module.clone()));
                            },
                        );
                    } else if node_set.contains(&to_idx) {
                        edges.push((from_module.clone(), to_module.clone()));
                    }
                }
            }
        } else {
            edges = self
                .graph
                .edge_indices()
                .filter_map(|e| self.graph.edge_endpoints(e))
                .filter(|(from, to)| node_set.contains(from) && node_set.contains(to))
                .map(|(from, to)| (self.graph[from].clone(), self.graph[to].clone()))
                .collect();
        }

        edges.sort_by(|a, b| {
            a.0.to_dotted()
                .cmp(&b.0.to_dotted())
                .then_with(|| a.1.to_dotted().cmp(&b.1.to_dotted()))
        });
        edges.dedup();
        edges
    }

    fn build_namespace_forest(&self, visible_nodes: &[NodeIndex]) -> NamespaceForest<T> {
        let mut internal = NamespaceTree::new(vec![]);
        let mut scripts = NamespaceTree::new(vec![]);

        for idx in visible_nodes {
            let module_path = &self.graph[*idx];
            let target = if self.is_script(module_path) {
                &mut scripts
            } else {
                &mut internal
            };
            target.insert(module_path);
        }

        internal.finalize();
        scripts.finalize();

        NamespaceForest { internal, scripts }
    }

    fn tree_for<'a>(&self, forest: &'a NamespaceForest<T>, module: &T) -> &'a NamespaceTree<T> {
        if self.is_script(module) {
            &forest.scripts
        } else {
            &forest.internal
        }
    }

    fn is_group_only_namespace(&self, forest: &NamespaceForest<T>, module: &T) -> bool {
        self.tree_for(forest, module)
            .is_group_only(&module.segments())
    }

    fn generate_compound_nodes(
        &self,
        forest: &NamespaceForest<T>,
        include_namespace_packages: bool,
    ) -> (HashMap<String, String>, Vec<GraphNode>) {
        let mut leaf_parent_map = HashMap::new();
        let mut parent_nodes = Vec::new();

        self.collect_compound_nodes_recursive(
            &forest.internal,
            None,
            include_namespace_packages,
            &mut leaf_parent_map,
            &mut parent_nodes,
        );

        self.collect_compound_nodes_recursive(
            &forest.scripts,
            None,
            include_namespace_packages,
            &mut leaf_parent_map,
            &mut parent_nodes,
        );

        (leaf_parent_map, parent_nodes)
    }

    #[allow(clippy::only_used_in_recursion)]
    fn collect_compound_nodes_recursive(
        &self,
        node: &NamespaceTree<T>,
        parent_id: Option<String>,
        include_namespace_packages: bool,
        leaf_parent_map: &mut HashMap<String, String>,
        parent_nodes: &mut Vec<GraphNode>,
    ) {
        if node.path.is_empty() {
            for child in node.child_groups() {
                self.collect_compound_nodes_recursive(
                    child,
                    None,
                    include_namespace_packages,
                    leaf_parent_map,
                    parent_nodes,
                );
            }
            return;
        }

        let current_id = node.path.join(".");

        if node.grouped {
            if node.id.is_none() {
                parent_nodes.push(GraphNode {
                    id: current_id.clone(),
                    node_type: "namespace_group".to_string(),
                    is_orphan: false,
                    highlighted: None,
                    parent: parent_id.clone(),
                });
            } else if let Some(pid) = &parent_id {
                leaf_parent_map.insert(current_id.clone(), pid.clone());
            }

            for child in node.child_groups() {
                self.collect_compound_nodes_recursive(
                    child,
                    Some(current_id.clone()),
                    include_namespace_packages,
                    leaf_parent_map,
                    parent_nodes,
                );
            }
        } else {
            if let Some(id) = &node.id {
                if let Some(pid) = parent_id.clone() {
                    leaf_parent_map.insert(id.to_dotted(), pid);
                }
            }

            for child in node.child_groups() {
                self.collect_compound_nodes_recursive(
                    child,
                    parent_id.clone(),
                    include_namespace_packages,
                    leaf_parent_map,
                    parent_nodes,
                );
            }
        }
    }

    fn get_visible_leaf_descendants(&self, forest: &NamespaceForest<T>, module: &T) -> Vec<T> {
        self.tree_for(forest, module)
            .find(&module.segments())
            .map(|node| {
                let mut descendants = Vec::new();
                node.collect_leaf_descendants(&mut descendants);
                descendants
            })
            .unwrap_or_default()
    }

    fn find_transitive_non_namespace_targets<F>(
        &self,
        start_idx: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
        visible_nodes: &HashSet<NodeIndex>,
        callback: &mut F,
    ) where
        F: FnMut(NodeIndex),
    {
        if !visited.insert(start_idx) {
            return;
        }

        let start_module = &self.graph[start_idx];

        if !self.is_namespace_package(start_module) && visible_nodes.contains(&start_idx) {
            callback(start_idx);
            return;
        }

        if self.is_namespace_package(start_module) {
            for neighbor_idx in self.graph.neighbors(start_idx) {
                self.find_transitive_non_namespace_targets(
                    neighbor_idx,
                    visited,
                    visible_nodes,
                    callback,
                );
            }
        }
    }

    fn dot_spec_for_module(
        &self,
        module: &T,
        include_namespace_packages: bool,
        is_highlighted: bool,
    ) -> Option<DotNodeSpec> {
        if self.is_namespace_package(module) && !include_namespace_packages {
            return None;
        }

        let attrs = if self.is_script(module) {
            if is_highlighted {
                "[shape=box, fillcolor=lightblue, style=filled]"
            } else {
                "[shape=box]"
            }
        } else if self.is_namespace_package(module) {
            if is_highlighted {
                "[shape=hexagon, fillcolor=lightblue, style=filled]"
            } else {
                "[shape=hexagon, style=dashed]"
            }
        } else if is_highlighted {
            "[fillcolor=lightblue, style=filled]"
        } else {
            ""
        };

        Some(DotNodeSpec {
            name: module.to_dotted(),
            attrs: attrs.to_string(),
        })
    }

    fn dot_spec_map(
        &self,
        nodes: &[NodeIndex],
        include_namespace_packages: bool,
        highlight_set: Option<&HashSet<T>>,
    ) -> HashMap<String, DotNodeSpec> {
        nodes
            .iter()
            .filter_map(|idx| {
                let module = &self.graph[*idx];
                let is_highlighted = highlight_set
                    .map(|set| set.contains(module))
                    .unwrap_or(false);

                self.dot_spec_for_module(module, include_namespace_packages, is_highlighted)
                    .map(|spec| (spec.name.clone(), spec))
            })
            .collect()
    }

    #[allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]
    fn render_dot_subgraph_generic(
        &self,
        node: &NamespaceTree<T>,
        forest: &NamespaceForest<T>,
        highlight_set: Option<&HashSet<T>>,
        include_namespace_packages: bool,
        specs: &HashMap<String, DotNodeSpec>,
        cluster_root: bool,
        indent_level: usize,
        is_script_root: bool,
        output: &mut String,
    ) {
        let indent = "    ".repeat(indent_level);

        if node.children.is_empty() && node.id.is_none() {
            return;
        }

        let has_root_content = !node.path.is_empty()
            || !node.direct_concrete_children().is_empty()
            || (!is_script_root && node.children.iter().any(|c| c.grouped));

        if (node.grouped || (cluster_root && node.path.is_empty() && has_root_content))
            && (cluster_root || !node.path.is_empty())
        {
            let cluster_name = if node.path.is_empty() {
                "root".to_string()
            } else {
                node.path.join("_")
            };
            let label = if node.path.is_empty() {
                "root".to_string()
            } else {
                node.path.join(".")
            };

            output.push_str(&format!("{indent}subgraph cluster_{cluster_name} {{\n"));
            output.push_str(&format!("{indent}    label = \"{label}\";\n"));

            for module in node.direct_concrete_children() {
                if self.is_group_only_namespace(forest, &module) {
                    continue;
                }
                if let Some(spec) = specs.get(&module.to_dotted()) {
                    output.push_str(&spec.render(&indent));
                }
            }

            for child in node.child_groups() {
                self.render_dot_subgraph_generic(
                    child,
                    forest,
                    highlight_set,
                    include_namespace_packages,
                    specs,
                    cluster_root,
                    indent_level + 1,
                    is_script_root,
                    output,
                );
            }

            output.push_str(&format!("{indent}}}\n"));
        } else {
            for child in node.child_groups() {
                self.render_dot_subgraph_generic(
                    child,
                    forest,
                    highlight_set,
                    include_namespace_packages,
                    specs,
                    cluster_root,
                    indent_level,
                    is_script_root,
                    output,
                );
            }
        }
    }

    fn collect_ungrouped_modules(&self, node: &NamespaceTree<T>, ungrouped: &mut Vec<T>) {
        node.collect_ungrouped_modules(ungrouped);
    }

    pub fn to_dot(&self, include_orphans: bool, include_namespace_packages: bool) -> String {
        let mut output = String::from("digraph dependencies {\n");
        output.push_str("    rankdir=LR;\n");
        output.push_str(
            "    // Note: Scripts (files outside source root) are shown with box shape\n",
        );
        let nodes = self.select_visible_nodes(
            NodeSelection::Full,
            include_orphans,
            include_namespace_packages,
        );
        let forest = self.build_namespace_forest(&nodes);
        let specs = self.dot_spec_map(&nodes, include_namespace_packages, None);

        self.render_dot_subgraph_generic(
            &forest.internal,
            &forest,
            None,
            include_namespace_packages,
            &specs,
            false,
            1,
            false,
            &mut output,
        );

        self.render_dot_subgraph_generic(
            &forest.scripts,
            &forest,
            None,
            include_namespace_packages,
            &specs,
            false,
            1,
            true,
            &mut output,
        );

        let mut ungrouped: Vec<T> = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);

        ungrouped.sort_by_key(GraphId::to_dotted);

        for module in &ungrouped {
            if !self.is_group_only_namespace(&forest, module) {
                if let Some(spec) = specs.get(&module.to_dotted()) {
                    output.push_str(&spec.render(""));
                }
            }
        }

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut edges = self.collect_edges(&node_set, include_namespace_packages);

        let mut transformed_edges = Vec::new();
        for (from_name, to_name) in edges.drain(..) {
            let from_is_group_only = self.is_group_only_namespace(&forest, &from_name);
            let to_is_group_only = self.is_group_only_namespace(&forest, &to_name);

            match (from_is_group_only, to_is_group_only) {
                (false, false) => {
                    transformed_edges.push((from_name, to_name));
                }
                (true, false) => {
                    let descendants = self.get_visible_leaf_descendants(&forest, &from_name);
                    for descendant in descendants {
                        transformed_edges.push((descendant, to_name.clone()));
                    }
                }
                (false, true) => {
                    let descendants = self.get_visible_leaf_descendants(&forest, &to_name);
                    for descendant in descendants {
                        transformed_edges.push((from_name.clone(), descendant));
                    }
                }
                (true, true) => {
                    let from_descendants = self.get_visible_leaf_descendants(&forest, &from_name);
                    let to_descendants = self.get_visible_leaf_descendants(&forest, &to_name);
                    for from_desc in &from_descendants {
                        for to_desc in &to_descendants {
                            transformed_edges.push((from_desc.clone(), to_desc.clone()));
                        }
                    }
                }
            }
        }

        edges = transformed_edges;

        edges.sort_by(|a, b| {
            a.0.to_dotted()
                .cmp(&b.0.to_dotted())
                .then_with(|| a.1.to_dotted().cmp(&b.1.to_dotted()))
        });
        edges.dedup();

        for (from_name, to_name) in edges {
            output.push_str(&format!(
                "    \"{}\" -> \"{}\";\n",
                from_name.to_dotted(),
                to_name.to_dotted()
            ));
        }

        output.push_str("}\n");
        output
    }

    pub fn to_dot_highlighted(
        &self,
        highlight_set: &HashSet<T>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let mut output = String::from("digraph dependencies {\n");
        output.push_str("    rankdir=LR;\n");
        output.push_str(
            "    // Note: Scripts (files outside source root) are shown with box shape\n",
        );
        output.push_str("    // Note: Highlighted nodes are shown with light blue background\n");
        let nodes = self.select_visible_nodes(
            NodeSelection::Highlighted,
            include_orphans,
            include_namespace_packages,
        );
        let forest = self.build_namespace_forest(&nodes);
        let specs = self.dot_spec_map(&nodes, include_namespace_packages, Some(highlight_set));

        self.render_dot_subgraph_generic(
            &forest.internal,
            &forest,
            Some(highlight_set),
            include_namespace_packages,
            &specs,
            true,
            1,
            false,
            &mut output,
        );

        self.render_dot_subgraph_generic(
            &forest.scripts,
            &forest,
            Some(highlight_set),
            include_namespace_packages,
            &specs,
            true,
            1,
            true,
            &mut output,
        );

        let mut ungrouped: Vec<T> = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);

        ungrouped.sort_by_key(GraphId::to_dotted);

        for module in &ungrouped {
            if !self.is_group_only_namespace(&forest, module) {
                if let Some(spec) = specs.get(&module.to_dotted()) {
                    output.push_str(&spec.render(""));
                }
            }
        }

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);

        for (from_name, to_name) in edges {
            output.push_str(&format!(
                "    \"{}\" -> \"{}\";\n",
                from_name.to_dotted(),
                to_name.to_dotted()
            ));
        }

        output.push_str("}\n");
        output
    }

    fn mermaid_spec_for_module(
        &self,
        module: &T,
        include_namespace_packages: bool,
    ) -> Option<MermaidNodeSpec> {
        if self.is_namespace_package(module) && !include_namespace_packages {
            return None;
        }

        let shape = if self.is_script(module) {
            MermaidShape::Script
        } else if self.is_namespace_package(module) {
            MermaidShape::Namespace
        } else {
            MermaidShape::Module
        };

        let label = module.to_dotted();
        Some(MermaidNodeSpec {
            id: sanitize_mermaid_id(&label),
            label,
            shape,
        })
    }

    fn mermaid_spec_map(
        &self,
        nodes: &[NodeIndex],
        include_namespace_packages: bool,
    ) -> HashMap<String, MermaidNodeSpec> {
        nodes
            .iter()
            .filter_map(|idx| {
                let module = &self.graph[*idx];
                self.mermaid_spec_for_module(module, include_namespace_packages)
                    .map(|spec| (spec.label.clone(), spec))
            })
            .collect()
    }

    fn render_mermaid_edge(
        &self,
        from_name: &str,
        to_name: &str,
        specs: &HashMap<String, MermaidNodeSpec>,
    ) -> Option<String> {
        let from_spec = specs.get(from_name)?;
        let to_spec = specs.get(to_name)?;
        Some(format!(
            "    {} --> {}\n",
            from_spec.render_inline(),
            to_spec.render_inline()
        ))
    }

    #[allow(clippy::only_used_in_recursion)]
    fn render_mermaid_subgraph(
        &self,
        node: &NamespaceTree<T>,
        indent_level: usize,
        args: &MermaidRenderArgs<'_, T>,
        highlighted_nodes: &mut HashSet<String>,
        output: &mut String,
    ) {
        let indent = "    ".repeat(indent_level);

        if node.path.is_empty() {
            for child in node.child_groups() {
                self.render_mermaid_subgraph(child, indent_level, args, highlighted_nodes, output);
            }
            return;
        }

        if node.grouped {
            let subgraph_id = sanitize_mermaid_id(&node.path.join("."));
            let label = node.path.join(".");

            output.push_str(&format!("{indent}subgraph {subgraph_id}[\"{label}\"]\n"));

            for module in node.direct_concrete_children() {
                if let Some(spec) = args.specs.get(&module.to_dotted()) {
                    let is_highlighted = args
                        .highlight_set
                        .map(|set| set.contains(&module))
                        .unwrap_or(false);
                    if is_highlighted {
                        highlighted_nodes.insert(spec.id.clone());
                    }
                    output.push_str(&spec.render_definition(&indent, is_highlighted));
                }
            }

            for child in node.child_groups() {
                self.render_mermaid_subgraph(
                    child,
                    indent_level + 1,
                    args,
                    highlighted_nodes,
                    output,
                );
            }

            output.push_str(&format!("{indent}end\n"));
        } else {
            for child in node.child_groups() {
                self.render_mermaid_subgraph(child, indent_level, args, highlighted_nodes, output);
            }
        }
    }

    pub fn to_mermaid(&self, include_orphans: bool, include_namespace_packages: bool) -> String {
        let mut output = String::from("flowchart TD\n");
        let nodes = self.select_visible_nodes(
            NodeSelection::Full,
            include_orphans,
            include_namespace_packages,
        );
        let forest = self.build_namespace_forest(&nodes);
        let specs = self.mermaid_spec_map(&nodes, include_namespace_packages);
        let mut highlighted_nodes = HashSet::new();
        let args = MermaidRenderArgs {
            highlight_set: None,
            specs: &specs,
        };

        self.render_mermaid_subgraph(
            &forest.internal,
            1,
            &args,
            &mut highlighted_nodes,
            &mut output,
        );
        self.render_mermaid_subgraph(
            &forest.scripts,
            1,
            &args,
            &mut highlighted_nodes,
            &mut output,
        );

        let mut ungrouped = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);

        ungrouped.sort_by_key(GraphId::to_dotted);

        for module in &ungrouped {
            if let Some(spec) = specs.get(&module.to_dotted()) {
                output.push_str(&spec.render_definition("", false));
            }
        }

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);

        for (from_name, to_name) in edges {
            if let Some(line) =
                self.render_mermaid_edge(&from_name.to_dotted(), &to_name.to_dotted(), &specs)
            {
                output.push_str(&line);
            }
        }

        output
    }

    pub fn to_mermaid_highlighted(
        &self,
        highlight_set: &HashSet<T>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let mut output = String::from("flowchart TD\n");
        let nodes = self.select_visible_nodes(
            NodeSelection::Highlighted,
            include_orphans,
            include_namespace_packages,
        );
        let specs = self.mermaid_spec_map(&nodes, include_namespace_packages);
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);
        let forest = self.build_namespace_forest(&nodes);
        let mut highlighted_nodes: HashSet<String> = HashSet::new();
        let args = MermaidRenderArgs {
            highlight_set: Some(highlight_set),
            specs: &specs,
        };

        self.render_mermaid_subgraph(
            &forest.internal,
            1,
            &args,
            &mut highlighted_nodes,
            &mut output,
        );
        self.render_mermaid_subgraph(
            &forest.scripts,
            1,
            &args,
            &mut highlighted_nodes,
            &mut output,
        );

        let mut ungrouped = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);
        ungrouped.sort_by_key(GraphId::to_dotted);

        for module in &ungrouped {
            let is_highlighted = highlight_set.contains(module);
            if let Some(spec) = specs.get(&module.to_dotted()) {
                if is_highlighted {
                    highlighted_nodes.insert(spec.id.clone());
                }
                output.push_str(&spec.render_definition("", is_highlighted));
            }
        }

        let highlighted_names: HashSet<String> =
            highlight_set.iter().map(GraphId::to_dotted).collect();

        for (from_name, to_name) in edges {
            if let Some(line) =
                self.render_mermaid_edge(&from_name.to_dotted(), &to_name.to_dotted(), &specs)
            {
                output.push_str(&line);
            }

            if highlighted_names.contains(&from_name.to_dotted()) {
                if let Some(spec) = specs.get(&from_name.to_dotted()) {
                    if highlighted_nodes.insert(spec.id.clone()) {
                        output.push_str(&format!("    class {} highlighted\n", spec.id));
                    }
                }
            }
            if highlighted_names.contains(&to_name.to_dotted()) {
                if let Some(spec) = specs.get(&to_name.to_dotted()) {
                    if highlighted_nodes.insert(spec.id.clone()) {
                        output.push_str(&format!("    class {} highlighted\n", spec.id));
                    }
                }
            }
        }

        output.push_str("    classDef highlighted fill:#bbdefb,stroke:#1976d2,stroke-width:2px\n");

        output
    }

    pub fn to_dot_filtered(
        &self,
        filter: &HashSet<T>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let mut output = String::from("digraph dependencies {\n");
        output.push_str("    rankdir=LR;\n");
        output.push_str(
            "    // Note: Scripts (files outside source root) are shown with box shape\n",
        );
        let nodes = self.select_visible_nodes(
            NodeSelection::Filtered(filter),
            include_orphans,
            include_namespace_packages,
        );
        let forest = self.build_namespace_forest(&nodes);
        let specs = self.dot_spec_map(&nodes, include_namespace_packages, None);

        self.render_dot_subgraph_generic(
            &forest.internal,
            &forest,
            None,
            include_namespace_packages,
            &specs,
            false,
            1,
            false,
            &mut output,
        );

        self.render_dot_subgraph_generic(
            &forest.scripts,
            &forest,
            None,
            include_namespace_packages,
            &specs,
            false,
            1,
            true,
            &mut output,
        );

        let mut ungrouped: Vec<T> = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);

        ungrouped.sort_by_key(GraphId::to_dotted);

        for module in &ungrouped {
            if !self.is_group_only_namespace(&forest, module) {
                if let Some(spec) = specs.get(&module.to_dotted()) {
                    output.push_str(&spec.render(""));
                }
            }
        }

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);

        for (from_name, to_name) in edges {
            output.push_str(&format!(
                "    \"{}\" -> \"{}\";\n",
                from_name.to_dotted(),
                to_name.to_dotted()
            ));
        }

        output.push_str("}\n");
        output
    }

    pub fn to_mermaid_filtered(
        &self,
        filter: &HashSet<T>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let mut output = String::from("flowchart TD\n");
        let nodes = self.select_visible_nodes(
            NodeSelection::Filtered(filter),
            include_orphans,
            include_namespace_packages,
        );
        let forest = self.build_namespace_forest(&nodes);
        let specs = self.mermaid_spec_map(&nodes, include_namespace_packages);
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);
        let mut highlighted_nodes = HashSet::new();
        let args = MermaidRenderArgs {
            highlight_set: None,
            specs: &specs,
        };

        self.render_mermaid_subgraph(
            &forest.internal,
            1,
            &args,
            &mut highlighted_nodes,
            &mut output,
        );
        self.render_mermaid_subgraph(
            &forest.scripts,
            1,
            &args,
            &mut highlighted_nodes,
            &mut output,
        );

        let nodes_in_edges: HashSet<String> = edges
            .iter()
            .flat_map(|(from, to)| vec![from.to_dotted(), to.to_dotted()])
            .collect();

        for idx in &nodes {
            let module = &self.graph[*idx];
            let module_name = module.to_dotted();

            if !nodes_in_edges.contains(&module_name) {
                if let Some(spec) = specs.get(&module_name) {
                    output.push_str(&spec.render_definition("", false));
                }
            }
        }

        for (from_name, to_name) in edges {
            if let Some(line) =
                self.render_mermaid_edge(&from_name.to_dotted(), &to_name.to_dotted(), &specs)
            {
                output.push_str(&line);
            }
        }

        output
    }

    pub fn find_downstream(&self, roots: &[T], max_rank: Option<usize>) -> HashMap<T, usize> {
        self.collect_reachable(roots, Direction::Incoming, max_rank)
    }

    pub fn find_upstream(&self, roots: &[T], max_rank: Option<usize>) -> HashMap<T, usize> {
        self.collect_reachable(roots, Direction::Outgoing, max_rank)
    }

    fn collect_reachable(
        &self,
        roots: &[T],
        direction: Direction,
        max_rank: Option<usize>,
    ) -> HashMap<T, usize> {
        let mut result = HashMap::new();
        let mut queue = VecDeque::new();
        let mut visited: HashMap<NodeIndex, usize> = HashMap::new();

        for root in roots {
            if let Some(&idx) = self.node_indices.get(root) {
                result.insert(root.clone(), 0);
                queue.push_back((idx, 0usize));
                visited.insert(idx, 0);
            }
        }

        while let Some((idx, dist)) = queue.pop_front() {
            let next_dist = dist + 1;
            if max_rank.map(|limit| next_dist > limit).unwrap_or(false) {
                continue;
            }

            for neighbor in self.graph.neighbors_directed(idx, direction) {
                let should_visit = match visited.get(&neighbor) {
                    Some(&existing) => next_dist < existing,
                    None => true,
                };

                if !should_visit {
                    continue;
                }

                visited.insert(neighbor, next_dist);

                if let Some(node) = self.graph.node_weight(neighbor) {
                    let entry = result.entry(node.clone()).or_insert(next_dist);
                    if next_dist < *entry {
                        *entry = next_dist;
                    }
                }

                queue.push_back((neighbor, next_dist));
            }
        }

        result
    }

    fn is_orphan(&self, idx: NodeIndex) -> bool {
        let has_incoming = self
            .graph
            .neighbors_directed(idx, Direction::Incoming)
            .count()
            > 0;
        let has_outgoing = self
            .graph
            .neighbors_directed(idx, Direction::Outgoing)
            .count()
            > 0;
        !has_incoming && !has_outgoing
    }

    pub fn to_list_filtered(
        &self,
        filter: &HashSet<T>,
        include_namespace_packages: bool,
    ) -> String {
        let mut sorted_modules: Vec<String> = filter
            .iter()
            .filter(|m| include_namespace_packages || !self.is_namespace_package(m))
            .map(GraphId::to_dotted)
            .collect();
        sorted_modules.sort();
        sorted_modules.join("\n")
    }

    pub fn to_cytoscape_graph_data(
        &self,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> GraphData {
        self.cytoscape_graph_data_internal(
            CytoscapeMode::Full,
            include_orphans,
            include_namespace_packages,
        )
    }

    pub fn to_cytoscape_graph_data_filtered(
        &self,
        filter: &HashSet<T>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> GraphData {
        self.cytoscape_graph_data_internal(
            CytoscapeMode::Filtered(filter),
            include_orphans,
            include_namespace_packages,
        )
    }

    pub fn to_cytoscape_graph_data_highlighted(
        &self,
        highlight_set: &HashSet<T>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> GraphData {
        self.cytoscape_graph_data_internal(
            CytoscapeMode::Highlighted(highlight_set),
            include_orphans,
            include_namespace_packages,
        )
    }

    fn cytoscape_graph_data_internal(
        &self,
        mode: CytoscapeMode<T>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> GraphData {
        let filter_set = match mode {
            CytoscapeMode::Full => None,
            CytoscapeMode::Filtered(set) | CytoscapeMode::Highlighted(set) => Some(set),
        };
        let is_highlighting_mode = matches!(mode, CytoscapeMode::Highlighted(_));
        let selection = match mode {
            CytoscapeMode::Full => NodeSelection::Full,
            CytoscapeMode::Filtered(set) => NodeSelection::Filtered(set),
            CytoscapeMode::Highlighted(_) => NodeSelection::Highlighted,
        };

        let nodes =
            self.select_visible_nodes(selection, include_orphans, include_namespace_packages);

        let forest = self.build_namespace_forest(&nodes);

        let (leaf_parent_map, parent_nodes) =
            self.generate_compound_nodes(&forest, include_namespace_packages);

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut graph_nodes = Vec::new();

        graph_nodes.extend(parent_nodes);

        for idx in &nodes {
            let module = &self.graph[*idx];
            let module_name = module.to_dotted();
            let is_script = self.is_script(module);
            let is_namespace = self.is_namespace_package(module);
            let is_highlighted = filter_set
                .map(|f| is_highlighting_mode && f.contains(module))
                .unwrap_or(false);
            let is_orphan = self.is_orphan(*idx);

            let node_type = if is_script {
                "script"
            } else if is_namespace {
                "namespace"
            } else {
                "module"
            };

            let parent = leaf_parent_map.get(&module_name).cloned();

            graph_nodes.push(GraphNode {
                id: module_name,
                node_type: node_type.to_string(),
                is_orphan,
                highlighted: if is_highlighted { Some(true) } else { None },
                parent,
            });
        }

        let edges = self.collect_edges(&node_set, include_namespace_packages);

        let graph_edges: Vec<GraphEdge> = edges
            .iter()
            .map(|(from, to)| GraphEdge {
                source: from.to_dotted(),
                target: to.to_dotted(),
            })
            .collect();

        let highlighted_modules = if is_highlighting_mode {
            filter_set.map(|set| {
                let mut modules: Vec<String> = set.iter().map(GraphId::to_dotted).collect();
                modules.sort();
                modules
            })
        } else {
            None
        };

        GraphData {
            nodes: graph_nodes,
            edges: graph_edges,
            config: Some(GraphConfig {
                include_orphans,
                include_namespaces: include_namespace_packages,
                highlighted_modules,
            }),
        }
    }
}

impl<T: GraphId> Default for DependencyGraph<T> {
    fn default() -> Self {
        Self::new()
    }
}
