//! Python internal dependency tree analyzer
//!
//! Parses Python files to extract import statements and builds a dependency graph
//! of internal module dependencies.

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{Bfs, Reversed};
use ruff_python_parser::parse_module;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

/// Errors that can occur during Python dependency analysis
#[derive(Error, Debug)]
pub enum PythonAnalysisError {
    #[error("Invalid project root: {0}")]
    InvalidRoot(PathBuf),

    #[error("Failed to read config file {0}: {1}")]
    ConfigReadError(PathBuf, std::io::Error),

    #[error("Failed to parse config file {0}: {1}")]
    ConfigParseError(PathBuf, toml::de::Error),

    #[error("No Python source root found in {0}")]
    NoSourceRootFound(PathBuf),
}

/// Output format for dependency graphs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Dot,
    Mermaid,
    List,
    Cytoscape,
}

/// Represents a Python module within the project
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePath(pub Vec<String>);

impl ModulePath {
    /// Create a module path from a file path relative to the project root
    pub fn from_file_path(path: &Path, root: &Path) -> Option<Self> {
        let relative = path.strip_prefix(root).ok()?;
        let mut parts: Vec<String> = relative
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(String::from))
            .collect();

        // Remove .py extension from last component
        if let Some(last) = parts.last_mut() {
            if last.ends_with(".py") {
                *last = last.strip_suffix(".py")?.to_string();
            }
        }

        // Handle __init__.py - remove it as it represents the package itself
        if parts.last().map(|s| s.as_str()) == Some("__init__") {
            parts.pop();
        }

        if parts.is_empty() {
            None
        } else {
            Some(ModulePath(parts))
        }
    }

    /// Create a module path from a script file path outside the source root.
    /// Uses path-based naming: scripts/blah.py -> ModulePath(["scripts", "blah"])
    pub fn from_script_path(path: &Path, project_root: &Path) -> Option<Self> {
        let relative = path.strip_prefix(project_root).ok()?;
        let mut parts: Vec<String> = relative
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(String::from))
            .collect();

        // Remove .py extension from last component
        if let Some(last) = parts.last_mut() {
            if last.ends_with(".py") {
                *last = last.strip_suffix(".py")?.to_string();
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(ModulePath(parts))
        }
    }

    /// Convert to dotted module name (e.g., "pkg_a.module_a")
    pub fn to_dotted(&self) -> String {
        self.0.join(".")
    }

    /// Resolve a relative import from this module's location
    pub fn resolve_relative(&self, level: u32, module: Option<&str>) -> Option<ModulePath> {
        if level == 0 {
            return module.map(|m| ModulePath(m.split('.').map(String::from).collect()));
        }

        // level indicates how many directories to go up
        let up_count = level as usize;
        if up_count > self.0.len() {
            return None;
        }

        let mut base: Vec<String> = self.0[..self.0.len() - up_count + 1].to_vec();
        // Remove the last element (current module name) for level 1
        if level >= 1 && !base.is_empty() {
            base.pop();
        }

        if let Some(m) = module {
            base.extend(m.split('.').map(String::from));
        }

        if base.is_empty() {
            None
        } else {
            Some(ModulePath(base))
        }
    }
}

/// Represents an import extracted from a Python file
#[derive(Debug, Clone)]
pub enum Import {
    /// `import foo` or `import foo.bar`
    Absolute { module: Vec<String> },
    /// `from foo import bar` or `from . import bar`
    From {
        module: Option<Vec<String>>,
        names: Vec<String>, // Names being imported (e.g., ["bar", "baz"] for "from foo import bar, baz")
        level: u32,         // 0 = absolute, 1 = ., 2 = .., etc.
    },
}

/// Extract imports from a Python source file
fn extract_imports(source: &str) -> Result<Vec<Import>, String> {
    let parsed = parse_module(source).map_err(|e| e.to_string())?;

    let mut imports = Vec::new();

    // Recursively visit all statements to capture imports at all nesting levels
    visit_stmts(parsed.suite(), &mut imports);

    Ok(imports)
}

/// Recursively visit all statements in the AST to extract imports
fn visit_stmts(stmts: &[ruff_python_ast::Stmt], imports: &mut Vec<Import>) {
    use ruff_python_ast::{Stmt, StmtImport, StmtImportFrom};

    for stmt in stmts {
        // Extract imports from current statement
        match stmt {
            Stmt::Import(StmtImport { names, .. }) => {
                for alias in names {
                    let module: Vec<String> =
                        alias.name.as_str().split('.').map(String::from).collect();
                    imports.push(Import::Absolute { module });
                }
            }
            Stmt::ImportFrom(StmtImportFrom {
                module,
                names,
                level,
                ..
            }) => {
                let module_parts = module
                    .as_ref()
                    .map(|m| m.as_str().split('.').map(String::from).collect());

                // Extract the imported names
                let imported_names: Vec<String> = names
                    .iter()
                    .filter_map(|alias| {
                        // Skip star imports (from foo import *)
                        if alias.name.as_str() == "*" {
                            None
                        } else {
                            Some(alias.name.to_string())
                        }
                    })
                    .collect();

                imports.push(Import::From {
                    module: module_parts,
                    names: imported_names,
                    level: *level,
                });
            }
            _ => {}
        }

        // Recursively visit nested statement bodies
        match stmt {
            Stmt::FunctionDef(func) => {
                visit_stmts(&func.body, imports);
            }
            Stmt::ClassDef(class) => {
                visit_stmts(&class.body, imports);
            }
            Stmt::If(if_stmt) => {
                visit_stmts(&if_stmt.body, imports);
                // Visit elif and else clauses
                for clause in &if_stmt.elif_else_clauses {
                    visit_stmts(&clause.body, imports);
                }
            }
            Stmt::While(while_stmt) => {
                visit_stmts(&while_stmt.body, imports);
                visit_stmts(&while_stmt.orelse, imports);
            }
            Stmt::For(for_stmt) => {
                visit_stmts(&for_stmt.body, imports);
                visit_stmts(&for_stmt.orelse, imports);
            }
            Stmt::With(with_stmt) => {
                visit_stmts(&with_stmt.body, imports);
            }
            Stmt::Try(try_stmt) => {
                use ruff_python_ast::ExceptHandler;

                visit_stmts(&try_stmt.body, imports);
                // Visit exception handler bodies
                for handler in &try_stmt.handlers {
                    match handler {
                        ExceptHandler::ExceptHandler(except) => {
                            visit_stmts(&except.body, imports);
                        }
                    }
                }
                visit_stmts(&try_stmt.orelse, imports);
                visit_stmts(&try_stmt.finalbody, imports);
            }
            Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    visit_stmts(&case.body, imports);
                }
            }
            _ => {}
        }
    }
}

/// Convert a dotted module name to a valid Mermaid node ID
/// Replaces dots with underscores since dots are not valid in Mermaid IDs
fn sanitize_mermaid_id(name: &str) -> String {
    name.replace('.', "_")
}

/// Graph node for frontend data model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    is_orphan: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    highlighted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
}

/// Graph edge for frontend data model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    source: String,
    target: String,
}

/// Graph configuration for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    pub include_orphans: bool,
    pub include_namespaces: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlighted_modules: Option<Vec<String>>,
}

/// Complete graph data for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    config: GraphConfig,
}

/// Rendering mode for Cytoscape data generation
enum CytoscapeMode<'a> {
    Full,
    Filtered(&'a HashSet<ModulePath>),
    Highlighted(&'a HashSet<ModulePath>),
}

/// Represents a node in the namespace hierarchy tree
#[derive(Debug, Clone)]
struct NamespaceNode {
    /// The full module path for this namespace (e.g., ["foo", "bar"])
    path: Vec<String>,

    /// Direct children of this namespace
    children: HashMap<String, NamespaceNode>,

    /// Whether this namespace has a corresponding ModulePath in the graph
    is_concrete_module: bool,

    /// Whether this namespace should be rendered as a group (2+ children)
    should_group: bool,
}

impl NamespaceNode {
    /// Create a new namespace node with the given path
    fn new(path: Vec<String>) -> Self {
        Self {
            path,
            children: HashMap::new(),
            is_concrete_module: false,
            should_group: false,
        }
    }
}

/// Root of the namespace hierarchy
#[derive(Debug)]
struct NamespaceHierarchy {
    /// Root for internal modules
    internal_root: NamespaceNode,

    /// Root for scripts (files outside source root)
    script_root: NamespaceNode,
}

/// Helper function to insert a module into the namespace tree
/// All modules inserted are concrete (exist in the graph), so we mark the leaf as concrete
fn insert_into_tree(node: &mut NamespaceNode, path: &[String]) {
    if path.is_empty() {
        // Reached the module itself - mark it as concrete
        node.is_concrete_module = true;
        return;
    }

    let mut current_path = node.path.clone();
    current_path.push(path[0].clone());

    let child = node
        .children
        .entry(path[0].clone())
        .or_insert_with(|| NamespaceNode::new(current_path));

    // Continue down the path
    insert_into_tree(child, &path[1..]);
}

/// Helper function to mark which namespace nodes should become groups
fn mark_grouping_nodes(node: &mut NamespaceNode) {
    // Recursively process children first
    for child in node.children.values_mut() {
        mark_grouping_nodes(child);
    }

    // Group if 2+ children
    node.should_group = node.children.len() >= 2;
}

/// The dependency graph of Python modules
pub struct DependencyGraph {
    graph: DiGraph<ModulePath, ()>,
    node_indices: HashMap<ModulePath, NodeIndex>,
    scripts: HashSet<ModulePath>, // Track which modules are scripts (outside source root)
    namespace_packages: HashSet<ModulePath>, // Track namespace packages (PEP 420 and legacy)
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
            scripts: HashSet::new(),
            namespace_packages: HashSet::new(),
        }
    }

    /// Mark a module as a script (file outside the source root)
    pub fn mark_as_script(&mut self, module: &ModulePath) {
        self.scripts.insert(module.clone());
    }

    /// Check if a module is a script
    pub fn is_script(&self, module: &ModulePath) -> bool {
        self.scripts.contains(module)
    }

    /// Mark a module as a namespace package
    pub fn mark_as_namespace_package(&mut self, module: &ModulePath) {
        self.namespace_packages.insert(module.clone());
    }

    /// Check if a module is a namespace package
    pub fn is_namespace_package(&self, module: &ModulePath) -> bool {
        self.namespace_packages.contains(module)
    }

    /// Get or create a node for the given module
    fn get_or_create_node(&mut self, module: ModulePath) -> NodeIndex {
        if let Some(&idx) = self.node_indices.get(&module) {
            idx
        } else {
            let idx = self.graph.add_node(module.clone());
            self.node_indices.insert(module, idx);
            idx
        }
    }

    /// Add a dependency edge from `from` to `to`
    pub fn add_dependency(&mut self, from: ModulePath, to: ModulePath) {
        let from_idx = self.get_or_create_node(from);
        let to_idx = self.get_or_create_node(to);
        self.graph.add_edge(from_idx, to_idx, ());
    }

    /// Build the complete namespace hierarchy from graph nodes
    fn build_namespace_hierarchy(&self, visible_nodes: &[NodeIndex]) -> NamespaceHierarchy {
        let mut internal_root = NamespaceNode::new(vec![]);
        let mut script_root = NamespaceNode::new(vec![]);

        // Insert only visible nodes into appropriate tree
        for idx in visible_nodes {
            let module_path = &self.graph[*idx];
            let root = if self.is_script(module_path) {
                &mut script_root
            } else {
                &mut internal_root
            };
            insert_into_tree(root, &module_path.0);
        }

        // Mark which nodes should be groups (2+ children)
        mark_grouping_nodes(&mut internal_root);
        mark_grouping_nodes(&mut script_root);

        NamespaceHierarchy {
            internal_root,
            script_root,
        }
    }

    /// Find a namespace node in the hierarchy by module path
    fn find_namespace_node<'a>(
        node: &'a NamespaceNode,
        path: &[String],
    ) -> Option<&'a NamespaceNode> {
        if path.is_empty() {
            return Some(node);
        }

        if let Some(child) = node.children.get(&path[0]) {
            Self::find_namespace_node(child, &path[1..])
        } else {
            None
        }
    }

    /// Check if a module is a group-only namespace (has children and should only appear as group)
    fn is_group_only_namespace(&self, hierarchy: &NamespaceHierarchy, module: &ModulePath) -> bool {
        // Determine which root to use
        let root = if self.is_script(module) {
            &hierarchy.script_root
        } else {
            &hierarchy.internal_root
        };

        // Find the namespace node for this module
        if let Some(node) = Self::find_namespace_node(root, &module.0) {
            // It's group-only if it should be grouped and is a concrete module
            node.should_group && node.is_concrete_module
        } else {
            false
        }
    }

    /// Generate compound nodes from namespace hierarchy for Cytoscape output
    /// Returns (leaf_parent_map, pure_parent_nodes)
    /// - leaf_parent_map: Maps module IDs to their parent IDs
    /// - pure_parent_nodes: Pure parent nodes (namespace groups that are not concrete modules)
    fn generate_compound_nodes(
        &self,
        hierarchy: &NamespaceHierarchy,
        visible_indices: &HashSet<NodeIndex>,
        include_namespace_packages: bool,
    ) -> (HashMap<String, String>, Vec<GraphNode>) {
        let mut leaf_parent_map = HashMap::new();
        let mut parent_nodes = Vec::new();

        // Process internal modules
        self.collect_compound_nodes_recursive(
            &hierarchy.internal_root,
            None,
            visible_indices,
            include_namespace_packages,
            &mut leaf_parent_map,
            &mut parent_nodes,
        );

        // Process scripts
        self.collect_compound_nodes_recursive(
            &hierarchy.script_root,
            None,
            visible_indices,
            include_namespace_packages,
            &mut leaf_parent_map,
            &mut parent_nodes,
        );

        (leaf_parent_map, parent_nodes)
    }

    /// Recursively collect compound node relationships from namespace hierarchy
    fn collect_compound_nodes_recursive(
        &self,
        node: &NamespaceNode,
        parent_id: Option<String>,
        visible_indices: &HashSet<NodeIndex>,
        include_namespace_packages: bool,
        leaf_parent_map: &mut HashMap<String, String>,
        parent_nodes: &mut Vec<GraphNode>,
    ) {
        // Root node - process children without creating a parent
        if node.path.is_empty() {
            for child in node.children.values() {
                self.collect_compound_nodes_recursive(
                    child,
                    None,
                    visible_indices,
                    include_namespace_packages,
                    leaf_parent_map,
                    parent_nodes,
                );
            }
            return;
        }

        let current_id = node.path.join(".");

        // If this node should group (2+ children), create a parent node
        if node.should_group {
            // Create parent node only if it's NOT a concrete module
            // (if it's concrete, it will be a leaf node too - hybrid node)
            if !node.is_concrete_module {
                // Pure parent node (namespace group only)
                parent_nodes.push(GraphNode {
                    id: current_id.clone(),
                    node_type: "namespace_group".to_string(),
                    is_orphan: false,
                    highlighted: None,
                    parent: parent_id.clone(),
                });
            } else {
                // Hybrid node - concrete module that also has children
                // Will be created as leaf node later, but still acts as parent
                // Just record its parent relationship
                if let Some(pid) = &parent_id {
                    leaf_parent_map.insert(current_id.clone(), pid.clone());
                }
            }

            // Recursively process children with current node as parent
            for child in node.children.values() {
                self.collect_compound_nodes_recursive(
                    child,
                    Some(current_id.clone()),
                    visible_indices,
                    include_namespace_packages,
                    leaf_parent_map,
                    parent_nodes,
                );
            }
        } else {
            // Not a group - just a leaf or intermediate node
            // If concrete, it will be added as leaf later
            if node.is_concrete_module {
                // Record parent relationship
                if let Some(pid) = parent_id.clone() {
                    leaf_parent_map.insert(current_id, pid);
                }
            }

            // Continue recursively for children (propagate parent)
            for child in node.children.values() {
                self.collect_compound_nodes_recursive(
                    child,
                    parent_id.clone(),
                    visible_indices,
                    include_namespace_packages,
                    leaf_parent_map,
                    parent_nodes,
                );
            }
        }
    }

    /// Get all visible leaf descendants of a namespace (for edge redirection)
    fn get_visible_leaf_descendants(
        &self,
        hierarchy: &NamespaceHierarchy,
        module: &ModulePath,
        visible_indices: &HashSet<NodeIndex>,
    ) -> Vec<ModulePath> {
        let root = if self.is_script(module) {
            &hierarchy.script_root
        } else {
            &hierarchy.internal_root
        };

        if let Some(node) = Self::find_namespace_node(root, &module.0) {
            let mut descendants = Vec::new();
            self.collect_leaf_descendants(node, visible_indices, &mut descendants);
            descendants
        } else {
            vec![]
        }
    }

    /// Helper to collect all leaf descendants from a namespace node
    fn collect_leaf_descendants(
        &self,
        node: &NamespaceNode,
        visible_indices: &HashSet<NodeIndex>,
        descendants: &mut Vec<ModulePath>,
    ) {
        // If this node has no children, check if it's in visible set
        if node.children.is_empty() {
            let module_path = ModulePath(node.path.clone());
            if let Some(&idx) = self.node_indices.get(&module_path) {
                if visible_indices.contains(&idx) {
                    descendants.push(module_path);
                }
            }
            return;
        }

        // Recursively collect from children
        for child in node.children.values() {
            self.collect_leaf_descendants(child, visible_indices, descendants);
        }
    }

    /// Helper function to find all non-namespace package targets reachable through namespace packages
    /// Uses DFS to traverse through namespace packages and calls the callback for each non-namespace target found
    fn find_transitive_non_namespace_targets<F>(
        &self,
        start_idx: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
        visible_nodes: &HashSet<NodeIndex>,
        callback: &mut F,
    ) where
        F: FnMut(NodeIndex),
    {
        // Mark as visited to avoid infinite loops
        if !visited.insert(start_idx) {
            return;
        }

        let start_module = &self.graph[start_idx];

        // If this is not a namespace package and it's visible, call the callback
        if !self.is_namespace_package(start_module) && visible_nodes.contains(&start_idx) {
            callback(start_idx);
            return;
        }

        // Otherwise, if this is a namespace package, continue traversing
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

    /// Helper to recursively render DOT subgraphs for namespace groups
    fn sorted_direct_children(
        &self,
        node: &NamespaceNode,
        visible_indices: &HashSet<NodeIndex>,
    ) -> Vec<NodeIndex> {
        let mut children: Vec<NodeIndex> = visible_indices
            .iter()
            .copied()
            .filter(|idx| {
                let module = &self.graph[*idx];
                let module_path = &module.0;
                module_path.len() == node.path.len() + 1 && module_path.starts_with(&node.path)
            })
            .collect();

        children.sort_by_key(|idx| self.graph[*idx].to_dotted());
        children
    }

    /// Helper to recursively render DOT subgraphs for namespace groups
    fn render_dot_subgraph(
        &self,
        node: &NamespaceNode,
        hierarchy: &NamespaceHierarchy,
        visible_indices: &HashSet<NodeIndex>,
        indent_level: usize,
        output: &mut String,
    ) {
        let indent = "    ".repeat(indent_level);

        // Root node should never create a cluster, just process children
        if node.path.is_empty() {
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                if let Some(child) = node.children.get(child_name) {
                    self.render_dot_subgraph(
                        child,
                        hierarchy,
                        visible_indices,
                        indent_level,
                        output,
                    );
                }
            }
            return;
        }

        if node.should_group {
            let cluster_name = node.path.join("_");
            let label = node.path.join(".");

            output.push_str(&format!("{}subgraph cluster_{} {{\n", indent, cluster_name));
            output.push_str(&format!("{}    label = \"{}\";\n", indent, label));

            // Find all direct children modules (leaf nodes at this level)
            for idx in self.sorted_direct_children(node, visible_indices) {
                let module = &self.graph[idx];

                // Skip group-only namespaces (they appear as group labels only)
                if self.is_group_only_namespace(hierarchy, module) {
                    continue;
                }

                // This is a direct child - render it
                if self.is_script(module) {
                    output.push_str(&format!(
                        "{}    \"{}\" [shape=box];\n",
                        indent,
                        module.to_dotted()
                    ));
                } else if self.is_namespace_package(module) {
                    output.push_str(&format!(
                        "{}    \"{}\" [shape=hexagon, style=dashed];\n",
                        indent,
                        module.to_dotted()
                    ));
                } else {
                    output.push_str(&format!("{}    \"{}\";\n", indent, module.to_dotted()));
                }
            }

            // Recursively render child groups
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                if let Some(child) = node.children.get(child_name) {
                    self.render_dot_subgraph(
                        child,
                        hierarchy,
                        visible_indices,
                        indent_level + 1,
                        output,
                    );
                }
            }

            output.push_str(&format!("{}}}\n", indent));
        } else {
            // Not a group - render direct children recursively
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                if let Some(child) = node.children.get(child_name) {
                    self.render_dot_subgraph(
                        child,
                        hierarchy,
                        visible_indices,
                        indent_level,
                        output,
                    );
                }
            }
        }
    }

    /// Helper to collect modules that should not be grouped (don't belong to any group)
    fn collect_ungrouped_modules(
        &self,
        node: &NamespaceNode,
        visible_indices: &HashSet<NodeIndex>,
        ungrouped: &mut Vec<NodeIndex>,
    ) {
        if node.should_group {
            // This node forms a group - don't collect its children as ungrouped
            // Recursively check grandchildren
            for child in node.children.values() {
                self.collect_ungrouped_modules(child, visible_indices, ungrouped);
            }
        } else {
            // Not a group - collect direct leaf children
            for idx in self.sorted_direct_children(node, visible_indices) {
                ungrouped.push(idx);
            }

            // Recursively process children
            for child in node.children.values() {
                self.collect_ungrouped_modules(child, visible_indices, ungrouped);
            }
        }
    }

    /// Convert the graph to Graphviz DOT format
    pub fn to_dot(&self, include_orphans: bool, include_namespace_packages: bool) -> String {
        let mut output = String::from("digraph dependencies {\n");
        output.push_str("    rankdir=LR;\n");
        output.push_str(
            "    // Note: Scripts (files outside source root) are shown with box shape\n",
        );

        // Collect and sort nodes for deterministic output
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        // Filter out namespace packages unless explicitly requested
        if !include_namespace_packages {
            nodes.retain(|idx| {
                let module = &self.graph[*idx];
                !self.is_namespace_package(module)
            });
        }

        // Filter out orphan nodes unless explicitly requested
        if !include_orphans {
            nodes.retain(|idx| {
                let has_incoming = self
                    .graph
                    .neighbors_directed(*idx, Direction::Incoming)
                    .count()
                    > 0;
                let has_outgoing = self
                    .graph
                    .neighbors_directed(*idx, Direction::Outgoing)
                    .count()
                    > 0;
                has_incoming || has_outgoing
            });
        }

        // Build namespace hierarchy from visible nodes only
        let hierarchy = self.build_namespace_hierarchy(&nodes);
        let visible_indices: HashSet<NodeIndex> = nodes.iter().copied().collect();

        // Render groups recursively (internal modules)
        self.render_dot_subgraph(
            &hierarchy.internal_root,
            &hierarchy,
            &visible_indices,
            1,
            &mut output,
        );

        // Render groups recursively (scripts)
        self.render_dot_subgraph(
            &hierarchy.script_root,
            &hierarchy,
            &visible_indices,
            1,
            &mut output,
        );

        // Collect and render ungrouped nodes (nodes not in any group)
        let mut ungrouped = Vec::new();
        self.collect_ungrouped_modules(&hierarchy.internal_root, &visible_indices, &mut ungrouped);
        self.collect_ungrouped_modules(&hierarchy.script_root, &visible_indices, &mut ungrouped);

        // Sort ungrouped nodes for deterministic output
        ungrouped.sort_by_key(|idx| self.graph[*idx].to_dotted());

        for idx in &ungrouped {
            let module = &self.graph[*idx];

            // Skip group-only namespaces
            if self.is_group_only_namespace(&hierarchy, module) {
                continue;
            }

            if self.is_script(module) {
                output.push_str(&format!("    \"{}\" [shape=box];\n", module.to_dotted()));
            } else if self.is_namespace_package(module) && include_namespace_packages {
                output.push_str(&format!(
                    "    \"{}\" [shape=hexagon, style=dashed];\n",
                    module.to_dotted()
                ));
            } else {
                output.push_str(&format!("    \"{}\";\n", module.to_dotted()));
            }
        }

        // Collect edges, with transitive edge preservation for namespace packages
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut edges: Vec<(String, String)> = Vec::new();

        if !include_namespace_packages {
            // When excluding namespace packages, we need to create transitive edges
            // For each edge in the original graph, if either endpoint is a namespace package,
            // we need to find the transitive path through namespace packages
            for from_idx in self.graph.node_indices() {
                let from_module = &self.graph[from_idx];

                // Skip if this node is filtered out
                if !node_set.contains(&from_idx) {
                    continue;
                }

                // Find all reachable non-namespace nodes through namespace packages
                for to_idx in self.graph.neighbors(from_idx) {
                    let to_module = &self.graph[to_idx];

                    if self.is_namespace_package(to_module) {
                        // This is a namespace package, traverse through it
                        let mut visited = HashSet::new();
                        self.find_transitive_non_namespace_targets(
                            to_idx,
                            &mut visited,
                            &node_set,
                            &mut |target_idx| {
                                let target_module = &self.graph[target_idx];
                                edges.push((from_module.to_dotted(), target_module.to_dotted()));
                            },
                        );
                    } else if node_set.contains(&to_idx) {
                        // Direct edge to a non-namespace package
                        edges.push((from_module.to_dotted(), to_module.to_dotted()));
                    }
                }
            }
        } else {
            // Include all edges between visible nodes
            edges = self
                .graph
                .edge_indices()
                .filter_map(|e| self.graph.edge_endpoints(e))
                .filter(|(from, to)| node_set.contains(from) && node_set.contains(to))
                .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
                .collect();
        }

        // Transform edges to redirect group-only namespaces to their children
        let mut transformed_edges = Vec::new();
        for (from_name, to_name) in edges {
            let from_module = ModulePath(from_name.split('.').map(String::from).collect());
            let to_module = ModulePath(to_name.split('.').map(String::from).collect());

            let from_is_group_only = self.is_group_only_namespace(&hierarchy, &from_module);
            let to_is_group_only = self.is_group_only_namespace(&hierarchy, &to_module);

            match (from_is_group_only, to_is_group_only) {
                (false, false) => {
                    // Normal edge
                    transformed_edges.push((from_name, to_name));
                }
                (true, false) => {
                    // From is group-only: create edges from all leaf descendants
                    let descendants = self.get_visible_leaf_descendants(
                        &hierarchy,
                        &from_module,
                        &visible_indices,
                    );
                    for descendant in descendants {
                        transformed_edges.push((descendant.to_dotted(), to_name.clone()));
                    }
                }
                (false, true) => {
                    // To is group-only: create edges to all leaf descendants
                    let descendants =
                        self.get_visible_leaf_descendants(&hierarchy, &to_module, &visible_indices);
                    for descendant in descendants {
                        transformed_edges.push((from_name.clone(), descendant.to_dotted()));
                    }
                }
                (true, true) => {
                    // Both are group-only: cartesian product of descendants
                    let from_descendants = self.get_visible_leaf_descendants(
                        &hierarchy,
                        &from_module,
                        &visible_indices,
                    );
                    let to_descendants =
                        self.get_visible_leaf_descendants(&hierarchy, &to_module, &visible_indices);
                    for from_desc in &from_descendants {
                        for to_desc in &to_descendants {
                            transformed_edges.push((from_desc.to_dotted(), to_desc.to_dotted()));
                        }
                    }
                }
            }
        }

        edges = transformed_edges;

        // Remove duplicates and sort edges for deterministic output
        edges.sort();
        edges.dedup();

        // Add edges
        for (from_name, to_name) in edges {
            output.push_str(&format!("    \"{}\" -> \"{}\";\n", from_name, to_name));
        }

        output.push_str("}\n");
        output
    }

    /// Helper to recursively render DOT subgraphs with highlighting
    fn render_dot_subgraph_highlighted(
        &self,
        node: &NamespaceNode,
        visible_indices: &HashSet<NodeIndex>,
        highlight_set: &HashSet<ModulePath>,
        include_namespace_packages: bool,
        indent_level: usize,
        output: &mut String,
    ) {
        let indent = "    ".repeat(indent_level);

        if node.should_group {
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

            output.push_str(&format!("{}subgraph cluster_{} {{\n", indent, cluster_name));
            output.push_str(&format!("{}    label = \"{}\";\n", indent, label));

            // Find and render direct children modules
            for idx in self.sorted_direct_children(node, visible_indices) {
                let module = &self.graph[idx];
                let is_highlighted = highlight_set.contains(module);
                let is_ns_pkg = self.is_namespace_package(module);

                if self.is_script(module) {
                    if is_highlighted {
                        output.push_str(&format!(
                            "{}    \"{}\" [shape=box, fillcolor=lightblue, style=filled];\n",
                            indent,
                            module.to_dotted()
                        ));
                    } else {
                        output.push_str(&format!(
                            "{}    \"{}\" [shape=box];\n",
                            indent,
                            module.to_dotted()
                        ));
                    }
                } else if is_ns_pkg && include_namespace_packages {
                    if is_highlighted {
                        output.push_str(&format!(
                            "{}    \"{}\" [shape=hexagon, fillcolor=lightblue, style=filled];\n",
                            indent,
                            module.to_dotted()
                        ));
                    } else {
                        output.push_str(&format!(
                            "{}    \"{}\" [shape=hexagon, style=dashed];\n",
                            indent,
                            module.to_dotted()
                        ));
                    }
                } else if is_highlighted {
                    output.push_str(&format!(
                        "{}    \"{}\" [fillcolor=lightblue, style=filled];\n",
                        indent,
                        module.to_dotted()
                    ));
                } else {
                    output.push_str(&format!("{}    \"{}\";\n", indent, module.to_dotted()));
                }
            }

            // Recursively render child groups
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                if let Some(child) = node.children.get(child_name) {
                    self.render_dot_subgraph_highlighted(
                        child,
                        visible_indices,
                        highlight_set,
                        include_namespace_packages,
                        indent_level + 1,
                        output,
                    );
                }
            }

            output.push_str(&format!("{}}}\n", indent));
        } else {
            // Not a group - render children recursively
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                if let Some(child) = node.children.get(child_name) {
                    self.render_dot_subgraph_highlighted(
                        child,
                        visible_indices,
                        highlight_set,
                        include_namespace_packages,
                        indent_level,
                        output,
                    );
                }
            }
        }
    }

    /// Convert the full graph to DOT format with highlighted nodes
    /// Nodes in the highlight_set are visually distinguished with a light blue background
    pub fn to_dot_highlighted(
        &self,
        highlight_set: &HashSet<ModulePath>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let mut output = String::from("digraph dependencies {\n");
        output.push_str("    rankdir=LR;\n");
        output.push_str(
            "    // Note: Scripts (files outside source root) are shown with box shape\n",
        );
        output.push_str("    // Note: Highlighted nodes are shown with light blue background\n");

        // Collect and sort nodes for deterministic output
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        // Filter out namespace packages unless explicitly requested
        if !include_namespace_packages {
            nodes.retain(|idx| {
                let module = &self.graph[*idx];
                !self.is_namespace_package(module)
            });
        }

        // Filter out orphan nodes unless explicitly requested
        if !include_orphans {
            nodes.retain(|idx| {
                let has_incoming = self
                    .graph
                    .neighbors_directed(*idx, Direction::Incoming)
                    .count()
                    > 0;
                let has_outgoing = self
                    .graph
                    .neighbors_directed(*idx, Direction::Outgoing)
                    .count()
                    > 0;
                has_incoming || has_outgoing
            });
        }

        // Build namespace hierarchy from visible nodes only
        let hierarchy = self.build_namespace_hierarchy(&nodes);
        let visible_indices: HashSet<NodeIndex> = nodes.iter().copied().collect();

        // Render groups recursively (internal modules)
        self.render_dot_subgraph_highlighted(
            &hierarchy.internal_root,
            &visible_indices,
            highlight_set,
            include_namespace_packages,
            1,
            &mut output,
        );

        // Render groups recursively (scripts)
        self.render_dot_subgraph_highlighted(
            &hierarchy.script_root,
            &visible_indices,
            highlight_set,
            include_namespace_packages,
            1,
            &mut output,
        );

        // Collect and render ungrouped nodes
        let mut ungrouped = Vec::new();
        self.collect_ungrouped_modules(&hierarchy.internal_root, &visible_indices, &mut ungrouped);
        self.collect_ungrouped_modules(&hierarchy.script_root, &visible_indices, &mut ungrouped);

        ungrouped.sort_by_key(|idx| self.graph[*idx].to_dotted());

        for idx in &ungrouped {
            let module = &self.graph[*idx];
            let is_highlighted = highlight_set.contains(module);
            let is_ns_pkg = self.is_namespace_package(module);

            if self.is_script(module) {
                if is_highlighted {
                    output.push_str(&format!(
                        "    \"{}\" [shape=box, fillcolor=lightblue, style=filled];\n",
                        module.to_dotted()
                    ));
                } else {
                    output.push_str(&format!("    \"{}\" [shape=box];\n", module.to_dotted()));
                }
            } else if is_ns_pkg && include_namespace_packages {
                if is_highlighted {
                    output.push_str(&format!(
                        "    \"{}\" [shape=hexagon, fillcolor=lightblue, style=filled];\n",
                        module.to_dotted()
                    ));
                } else {
                    output.push_str(&format!(
                        "    \"{}\" [shape=hexagon, style=dashed];\n",
                        module.to_dotted()
                    ));
                }
            } else {
                if is_highlighted {
                    output.push_str(&format!(
                        "    \"{}\" [fillcolor=lightblue, style=filled];\n",
                        module.to_dotted()
                    ));
                } else {
                    output.push_str(&format!("    \"{}\";\n", module.to_dotted()));
                }
            }
        }

        // Collect edges with transitive edge preservation for namespace packages
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut edges: Vec<(String, String)> = Vec::new();

        if !include_namespace_packages {
            // When excluding namespace packages, create transitive edges
            for from_idx in self.graph.node_indices() {
                let from_module = &self.graph[from_idx];

                if !node_set.contains(&from_idx) {
                    continue;
                }

                for to_idx in self.graph.neighbors(from_idx) {
                    let to_module = &self.graph[to_idx];

                    if self.is_namespace_package(to_module) {
                        let mut visited = HashSet::new();
                        self.find_transitive_non_namespace_targets(
                            to_idx,
                            &mut visited,
                            &node_set,
                            &mut |target_idx| {
                                let target_module = &self.graph[target_idx];
                                edges.push((from_module.to_dotted(), target_module.to_dotted()));
                            },
                        );
                    } else if node_set.contains(&to_idx) {
                        edges.push((from_module.to_dotted(), to_module.to_dotted()));
                    }
                }
            }
        } else {
            edges = self
                .graph
                .edge_indices()
                .filter_map(|e| self.graph.edge_endpoints(e))
                .filter(|(from, to)| node_set.contains(from) && node_set.contains(to))
                .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
                .collect();
        }

        // Remove duplicates and sort edges
        edges.sort();
        edges.dedup();

        // Add edges
        for (from_name, to_name) in edges {
            output.push_str(&format!("    \"{}\" -> \"{}\";\n", from_name, to_name));
        }

        output.push_str("}\n");
        output
    }

    /// Helper to recursively render Mermaid subgraphs for namespace groups
    fn render_mermaid_subgraph(
        &self,
        node: &NamespaceNode,
        visible_indices: &HashSet<NodeIndex>,
        indent_level: usize,
        output: &mut String,
    ) {
        let indent = "    ".repeat(indent_level);

        // Root node should never create a subgraph, just process children
        if node.path.is_empty() {
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                if let Some(child) = node.children.get(child_name) {
                    self.render_mermaid_subgraph(child, visible_indices, indent_level, output);
                }
            }
            return;
        }

        if node.should_group {
            let subgraph_id = sanitize_mermaid_id(&node.path.join("."));
            let label = node.path.join(".");

            output.push_str(&format!(
                "{}subgraph {}[\"{}\"]\n",
                indent, subgraph_id, label
            ));

            // Find and render direct children modules
            for idx in self.sorted_direct_children(node, visible_indices) {
                let module = &self.graph[idx];
                let id = sanitize_mermaid_id(&module.to_dotted());
                if self.is_script(module) {
                    output.push_str(&format!(
                        "{}    {}[\"{}\"]\n",
                        indent,
                        id,
                        module.to_dotted()
                    ));
                } else if self.is_namespace_package(module) {
                    output.push_str(&format!(
                        "{}    {}{{{{\"{}\"}}}} \n",
                        indent,
                        id,
                        module.to_dotted()
                    ));
                } else {
                    output.push_str(&format!(
                        "{}    {}(\"{}\")\n",
                        indent,
                        id,
                        module.to_dotted()
                    ));
                }
            }

            // Recursively render child groups
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                if let Some(child) = node.children.get(child_name) {
                    self.render_mermaid_subgraph(child, visible_indices, indent_level + 1, output);
                }
            }

            output.push_str(&format!("{}end\n", indent));
        } else {
            // Not a group - render children recursively
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                if let Some(child) = node.children.get(child_name) {
                    self.render_mermaid_subgraph(child, visible_indices, indent_level, output);
                }
            }
        }
    }

    /// Convert the graph to Mermaid flowchart format
    pub fn to_mermaid(&self, include_orphans: bool, include_namespace_packages: bool) -> String {
        let mut output = String::from("flowchart TD\n");

        // Collect and sort nodes for deterministic output
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        // Filter out namespace packages unless explicitly requested
        if !include_namespace_packages {
            nodes.retain(|idx| {
                let module = &self.graph[*idx];
                !self.is_namespace_package(module)
            });
        }

        // Filter out orphan nodes unless explicitly requested
        if !include_orphans {
            nodes.retain(|idx| {
                let has_incoming = self
                    .graph
                    .neighbors_directed(*idx, Direction::Incoming)
                    .count()
                    > 0;
                let has_outgoing = self
                    .graph
                    .neighbors_directed(*idx, Direction::Outgoing)
                    .count()
                    > 0;
                has_incoming || has_outgoing
            });
        }

        // Build namespace hierarchy from visible nodes only
        let hierarchy = self.build_namespace_hierarchy(&nodes);
        let visible_indices: HashSet<NodeIndex> = nodes.iter().copied().collect();

        // Render groups recursively (for all nodes)
        self.render_mermaid_subgraph(&hierarchy.internal_root, &visible_indices, 1, &mut output);
        self.render_mermaid_subgraph(&hierarchy.script_root, &visible_indices, 1, &mut output);

        // Collect and render ungrouped nodes
        let mut ungrouped = Vec::new();
        self.collect_ungrouped_modules(&hierarchy.internal_root, &visible_indices, &mut ungrouped);
        self.collect_ungrouped_modules(&hierarchy.script_root, &visible_indices, &mut ungrouped);

        ungrouped.sort_by_key(|idx| self.graph[*idx].to_dotted());

        for idx in &ungrouped {
            let module = &self.graph[*idx];
            let module_name = module.to_dotted();
            let node_id = sanitize_mermaid_id(&module_name);

            if self.is_script(module) {
                output.push_str(&format!("    {}[\"{}\"]\n", node_id, module_name));
            } else if self.is_namespace_package(module) && include_namespace_packages {
                output.push_str(&format!("    {}{{{{\"{}\"}}}} \n", node_id, module_name));
            } else {
                output.push_str(&format!("    {}(\"{}\")\n", node_id, module_name));
            }
        }

        // Collect edges with transitive edge preservation for namespace packages
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut edges: Vec<(String, String)> = Vec::new();

        if !include_namespace_packages {
            // When excluding namespace packages, create transitive edges
            for from_idx in self.graph.node_indices() {
                let from_module = &self.graph[from_idx];

                if !node_set.contains(&from_idx) {
                    continue;
                }

                for to_idx in self.graph.neighbors(from_idx) {
                    let to_module = &self.graph[to_idx];

                    if self.is_namespace_package(to_module) {
                        let mut visited = HashSet::new();
                        self.find_transitive_non_namespace_targets(
                            to_idx,
                            &mut visited,
                            &node_set,
                            &mut |target_idx| {
                                let target_module = &self.graph[target_idx];
                                edges.push((from_module.to_dotted(), target_module.to_dotted()));
                            },
                        );
                    } else if node_set.contains(&to_idx) {
                        edges.push((from_module.to_dotted(), to_module.to_dotted()));
                    }
                }
            }
        } else {
            edges = self
                .graph
                .edge_indices()
                .filter_map(|e| self.graph.edge_endpoints(e))
                .filter(|(from, to)| node_set.contains(from) && node_set.contains(to))
                .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
                .collect();
        }

        // Remove duplicates and sort edges
        edges.sort();
        edges.dedup();

        // Add edges (which implicitly define nodes)
        for (from_name, to_name) in edges {
            let from_id = sanitize_mermaid_id(&from_name);
            let to_id = sanitize_mermaid_id(&to_name);

            // Determine shapes based on whether modules are scripts or namespace packages
            let from_module_path = ModulePath(from_name.split('.').map(String::from).collect());
            let to_module_path = ModulePath(to_name.split('.').map(String::from).collect());

            let from_module = self.node_indices.get(&from_module_path);
            let to_module = self.node_indices.get(&to_module_path);

            let from_shape = if let Some(idx) = from_module {
                let m = &self.graph[*idx];
                if self.is_script(m) {
                    format!("{}[\"{}\"", from_id, from_name)
                } else if self.is_namespace_package(m) && include_namespace_packages {
                    format!("{}{{{{\"{}\"", from_id, from_name)
                } else {
                    format!("{}(\"{}\"", from_id, from_name)
                }
            } else {
                format!("{}(\"{}\"", from_id, from_name)
            };

            let to_shape = if let Some(idx) = to_module {
                let m = &self.graph[*idx];
                if self.is_script(m) {
                    format!("{}[\"{}\"", to_id, to_name)
                } else if self.is_namespace_package(m) && include_namespace_packages {
                    format!("{}{{{{\"{}\"", to_id, to_name)
                } else {
                    format!("{}(\"{}\"", to_id, to_name)
                }
            } else {
                format!("{}(\"{}\"", to_id, to_name)
            };

            // Close the shapes
            let from_def = if from_shape.contains('[') {
                format!("{}]", from_shape)
            } else if from_shape.contains("{{{{") {
                format!("{}}}}}", from_shape)
            } else {
                format!("{})", from_shape)
            };

            let to_def = if to_shape.contains('[') {
                format!("{}]", to_shape)
            } else if to_shape.contains("{{{{") {
                format!("{}}}}}", to_shape)
            } else {
                format!("{})", to_shape)
            };

            output.push_str(&format!("    {} --> {}\n", from_def, to_def));
        }

        output
    }

    /// Convert the full graph to Mermaid flowchart format with highlighted nodes
    /// Nodes in the highlight_set are visually distinguished with blue styling
    pub fn to_mermaid_highlighted(
        &self,
        highlight_set: &HashSet<ModulePath>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let mut output = String::from("flowchart TD\n");

        // Collect and sort nodes for deterministic output
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        // Filter out namespace packages unless explicitly requested
        if !include_namespace_packages {
            nodes.retain(|idx| {
                let module = &self.graph[*idx];
                !self.is_namespace_package(module)
            });
        }

        // Filter out orphan nodes unless explicitly requested
        if !include_orphans {
            nodes.retain(|idx| {
                let has_incoming = self
                    .graph
                    .neighbors_directed(*idx, Direction::Incoming)
                    .count()
                    > 0;
                let has_outgoing = self
                    .graph
                    .neighbors_directed(*idx, Direction::Outgoing)
                    .count()
                    > 0;
                has_incoming || has_outgoing
            });
        }

        // Collect edges with transitive edge preservation for namespace packages
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut edges: Vec<(String, String)> = Vec::new();

        if !include_namespace_packages {
            // When excluding namespace packages, create transitive edges
            for from_idx in self.graph.node_indices() {
                let from_module = &self.graph[from_idx];

                if !node_set.contains(&from_idx) {
                    continue;
                }

                for to_idx in self.graph.neighbors(from_idx) {
                    let to_module = &self.graph[to_idx];

                    if self.is_namespace_package(to_module) {
                        let mut visited = HashSet::new();
                        self.find_transitive_non_namespace_targets(
                            to_idx,
                            &mut visited,
                            &node_set,
                            &mut |target_idx| {
                                let target_module = &self.graph[target_idx];
                                edges.push((from_module.to_dotted(), target_module.to_dotted()));
                            },
                        );
                    } else if node_set.contains(&to_idx) {
                        edges.push((from_module.to_dotted(), to_module.to_dotted()));
                    }
                }
            }
        } else {
            edges = self
                .graph
                .edge_indices()
                .filter_map(|e| self.graph.edge_endpoints(e))
                .filter(|(from, to)| node_set.contains(from) && node_set.contains(to))
                .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
                .collect();
        }

        // Remove duplicates and sort edges
        edges.sort();
        edges.dedup();

        // Create a set of nodes that appear in edges for efficient lookup
        let nodes_in_edges: std::collections::HashSet<String> = edges
            .iter()
            .flat_map(|(from, to)| vec![from.clone(), to.clone()])
            .collect();

        // Add nodes that don't appear in edges (orphans if include_orphans is true)
        for idx in &nodes {
            let module = &self.graph[*idx];
            let module_name = module.to_dotted();

            // Only output standalone node definitions for nodes without edges
            if !nodes_in_edges.contains(&module_name) {
                let node_id = sanitize_mermaid_id(&module_name);
                let is_highlighted = highlight_set.contains(module);

                if self.is_script(module) {
                    // Scripts get rectangle shape
                    output.push_str(&format!("    {}[\"{}\"]\n", node_id, module_name));
                } else if self.is_namespace_package(module) && include_namespace_packages {
                    // Namespace packages get hexagon shape
                    output.push_str(&format!("    {}{{{{\"{}\"}}}} \n", node_id, module_name));
                } else {
                    // Modules get rounded rectangle shape
                    output.push_str(&format!("    {}(\"{}\")\n", node_id, module_name));
                }

                // Apply highlighting class if needed
                if is_highlighted {
                    output.push_str(&format!("    class {} highlighted\n", node_id));
                }
            }
        }

        // Track which nodes have been assigned the highlighted class to avoid duplicates
        let mut highlighted_nodes: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Add edges (which implicitly define nodes)
        for (from_name, to_name) in edges {
            let from_id = sanitize_mermaid_id(&from_name);
            let to_id = sanitize_mermaid_id(&to_name);

            // Determine shapes based on whether modules are scripts
            let from_module = self.node_indices.get(&ModulePath(
                from_name.split('.').map(String::from).collect(),
            ));
            let to_module = self
                .node_indices
                .get(&ModulePath(to_name.split('.').map(String::from).collect()));

            let from_is_script = from_module
                .map(|idx| {
                    let m = &self.graph[*idx];
                    self.is_script(m)
                })
                .unwrap_or(false);

            let to_is_script = to_module
                .map(|idx| {
                    let m = &self.graph[*idx];
                    self.is_script(m)
                })
                .unwrap_or(false);

            let from_shape = if from_is_script {
                format!("{}[\"{}\"", from_id, from_name)
            } else {
                format!("{}(\"{}\"", from_id, from_name)
            };

            let to_shape = if to_is_script {
                format!("{}[\"{}\"", to_id, to_name)
            } else {
                format!("{}(\"{}\"", to_id, to_name)
            };

            // Close the shapes
            let from_def = if from_shape.contains('[') {
                format!("{}]", from_shape)
            } else if from_shape.contains("{{{{") {
                format!("{}}}}}", from_shape)
            } else {
                format!("{})", from_shape)
            };

            let to_def = if to_shape.contains('[') {
                format!("{}]", to_shape)
            } else if to_shape.contains("{{{{") {
                format!("{}}}}}", to_shape)
            } else {
                format!("{})", to_shape)
            };

            output.push_str(&format!("    {} --> {}\n", from_def, to_def));

            // Apply highlighting class to nodes that appear in edges (avoid duplicates)
            let from_module_path = ModulePath(from_name.split('.').map(String::from).collect());
            let to_module_path = ModulePath(to_name.split('.').map(String::from).collect());

            if highlight_set.contains(&from_module_path)
                && highlighted_nodes.insert(from_id.clone())
            {
                output.push_str(&format!("    class {} highlighted\n", from_id));
            }
            if highlight_set.contains(&to_module_path) && highlighted_nodes.insert(to_id.clone()) {
                output.push_str(&format!("    class {} highlighted\n", to_id));
            }
        }

        // Add the highlighted class definition at the end
        output.push_str("    classDef highlighted fill:#bbdefb,stroke:#1976d2,stroke-width:2px\n");

        output
    }

    /// Convert a filtered set of modules to Graphviz DOT format (subgraph).
    /// Only includes nodes and edges where both endpoints are in the filtered set.
    pub fn to_dot_filtered(
        &self,
        filter: &HashSet<ModulePath>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let mut output = String::from("digraph dependencies {\n");
        output.push_str("    rankdir=LR;\n");
        output.push_str(
            "    // Note: Scripts (files outside source root) are shown with box shape\n",
        );

        // Collect and sort nodes that are in the filter
        let mut nodes: Vec<_> = self
            .graph
            .node_indices()
            .filter(|idx| filter.contains(&self.graph[*idx]))
            .collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        // Filter out namespace packages unless explicitly requested
        if !include_namespace_packages {
            nodes.retain(|idx| {
                let module = &self.graph[*idx];
                !self.is_namespace_package(module)
            });
        }

        // Filter out orphan nodes unless explicitly requested
        if !include_orphans {
            nodes.retain(|idx| {
                let has_incoming = self
                    .graph
                    .neighbors_directed(*idx, Direction::Incoming)
                    .count()
                    > 0;
                let has_outgoing = self
                    .graph
                    .neighbors_directed(*idx, Direction::Outgoing)
                    .count()
                    > 0;
                has_incoming || has_outgoing
            });
        }

        // Build namespace hierarchy from visible nodes only
        let hierarchy = self.build_namespace_hierarchy(&nodes);
        let visible_indices: HashSet<NodeIndex> = nodes.iter().copied().collect();

        // Render groups recursively (internal modules)
        self.render_dot_subgraph(
            &hierarchy.internal_root,
            &hierarchy,
            &visible_indices,
            1,
            &mut output,
        );

        // Render groups recursively (scripts)
        self.render_dot_subgraph(
            &hierarchy.script_root,
            &hierarchy,
            &visible_indices,
            1,
            &mut output,
        );

        // Collect and render ungrouped nodes
        let mut ungrouped = Vec::new();
        self.collect_ungrouped_modules(&hierarchy.internal_root, &visible_indices, &mut ungrouped);
        self.collect_ungrouped_modules(&hierarchy.script_root, &visible_indices, &mut ungrouped);

        ungrouped.sort_by_key(|idx| self.graph[*idx].to_dotted());

        for idx in &ungrouped {
            let module = &self.graph[*idx];

            // Skip group-only namespaces
            if self.is_group_only_namespace(&hierarchy, module) {
                continue;
            }

            if self.is_script(module) {
                output.push_str(&format!("    \"{}\" [shape=box];\n", module.to_dotted()));
            } else if self.is_namespace_package(module) && include_namespace_packages {
                output.push_str(&format!(
                    "    \"{}\" [shape=hexagon, style=dashed];\n",
                    module.to_dotted()
                ));
            } else {
                output.push_str(&format!("    \"{}\";\n", module.to_dotted()));
            }
        }

        // Collect edges with transitive edge preservation for namespace packages
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut edges: Vec<(String, String)> = Vec::new();

        if !include_namespace_packages {
            // When excluding namespace packages, create transitive edges
            for from_idx in self.graph.node_indices() {
                let from_module = &self.graph[from_idx];

                if !filter.contains(from_module) || !node_set.contains(&from_idx) {
                    continue;
                }

                for to_idx in self.graph.neighbors(from_idx) {
                    let to_module = &self.graph[to_idx];

                    if self.is_namespace_package(to_module) {
                        let mut visited = HashSet::new();
                        self.find_transitive_non_namespace_targets(
                            to_idx,
                            &mut visited,
                            &node_set,
                            &mut |target_idx| {
                                let target_module = &self.graph[target_idx];
                                if filter.contains(target_module) {
                                    edges
                                        .push((from_module.to_dotted(), target_module.to_dotted()));
                                }
                            },
                        );
                    } else if filter.contains(to_module) && node_set.contains(&to_idx) {
                        edges.push((from_module.to_dotted(), to_module.to_dotted()));
                    }
                }
            }
        } else {
            edges = self
                .graph
                .edge_indices()
                .filter_map(|e| self.graph.edge_endpoints(e))
                .filter(|(from, to)| {
                    filter.contains(&self.graph[*from]) && filter.contains(&self.graph[*to])
                })
                .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
                .collect();
        }

        // Remove duplicates and sort edges
        edges.sort();
        edges.dedup();

        // Add edges
        for (from_name, to_name) in edges {
            output.push_str(&format!("    \"{}\" -> \"{}\";\n", from_name, to_name));
        }

        output.push_str("}\n");
        output
    }

    /// Convert a filtered set of modules to Mermaid flowchart format (subgraph).
    /// Only includes nodes and edges where both endpoints are in the filtered set.
    pub fn to_mermaid_filtered(
        &self,
        filter: &HashSet<ModulePath>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let mut output = String::from("flowchart TD\n");

        // Collect and sort nodes that are in the filter
        let mut nodes: Vec<_> = self
            .graph
            .node_indices()
            .filter(|idx| filter.contains(&self.graph[*idx]))
            .collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        // Filter out namespace packages unless explicitly requested
        if !include_namespace_packages {
            nodes.retain(|idx| {
                let module = &self.graph[*idx];
                !self.is_namespace_package(module)
            });
        }

        // Filter out orphan nodes unless explicitly requested
        if !include_orphans {
            nodes.retain(|idx| {
                let has_incoming = self
                    .graph
                    .neighbors_directed(*idx, Direction::Incoming)
                    .count()
                    > 0;
                let has_outgoing = self
                    .graph
                    .neighbors_directed(*idx, Direction::Outgoing)
                    .count()
                    > 0;
                has_incoming || has_outgoing
            });
        }

        // Collect edges with transitive edge preservation for namespace packages
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut edges: Vec<(String, String)> = Vec::new();

        if !include_namespace_packages {
            // When excluding namespace packages, create transitive edges
            for from_idx in self.graph.node_indices() {
                let from_module = &self.graph[from_idx];

                if !filter.contains(from_module) || !node_set.contains(&from_idx) {
                    continue;
                }

                for to_idx in self.graph.neighbors(from_idx) {
                    let to_module = &self.graph[to_idx];

                    if self.is_namespace_package(to_module) {
                        let mut visited = HashSet::new();
                        self.find_transitive_non_namespace_targets(
                            to_idx,
                            &mut visited,
                            &node_set,
                            &mut |target_idx| {
                                let target_module = &self.graph[target_idx];
                                if filter.contains(target_module) {
                                    edges
                                        .push((from_module.to_dotted(), target_module.to_dotted()));
                                }
                            },
                        );
                    } else if filter.contains(to_module) && node_set.contains(&to_idx) {
                        edges.push((from_module.to_dotted(), to_module.to_dotted()));
                    }
                }
            }
        } else {
            edges = self
                .graph
                .edge_indices()
                .filter_map(|e| self.graph.edge_endpoints(e))
                .filter(|(from, to)| {
                    filter.contains(&self.graph[*from]) && filter.contains(&self.graph[*to])
                })
                .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
                .collect();
        }

        // Remove duplicates and sort edges
        edges.sort();
        edges.dedup();

        // Create a set of nodes that appear in edges for efficient lookup
        let nodes_in_edges: std::collections::HashSet<String> = edges
            .iter()
            .flat_map(|(from, to)| vec![from.clone(), to.clone()])
            .collect();

        // Add nodes that don't appear in edges (orphans if include_orphans is true)
        for idx in &nodes {
            let module = &self.graph[*idx];
            let module_name = module.to_dotted();

            // Only output standalone node definitions for nodes without edges
            if !nodes_in_edges.contains(&module_name) {
                let node_id = sanitize_mermaid_id(&module_name);
                if self.is_script(module) {
                    // Scripts get rectangle shape
                    output.push_str(&format!("    {}[\"{}\"]\n", node_id, module_name));
                } else if self.is_namespace_package(module) && include_namespace_packages {
                    // Namespace packages get hexagon shape
                    output.push_str(&format!("    {}{{{{\"{}\"}}}} \n", node_id, module_name));
                } else {
                    // Modules get rounded rectangle shape
                    output.push_str(&format!("    {}(\"{}\")\n", node_id, module_name));
                }
            }
        }

        // Add edges (which implicitly define nodes)
        for (from_name, to_name) in edges {
            let from_id = sanitize_mermaid_id(&from_name);
            let to_id = sanitize_mermaid_id(&to_name);

            // Determine shapes based on whether modules are scripts or namespace packages
            let from_module_path = ModulePath(from_name.split('.').map(String::from).collect());
            let to_module_path = ModulePath(to_name.split('.').map(String::from).collect());

            let from_module = self.node_indices.get(&from_module_path);
            let to_module = self.node_indices.get(&to_module_path);

            let from_shape = if let Some(idx) = from_module {
                let m = &self.graph[*idx];
                if self.is_script(m) {
                    format!("{}[\"{}\"", from_id, from_name)
                } else if self.is_namespace_package(m) && include_namespace_packages {
                    format!("{}{{{{\"{}\"", from_id, from_name)
                } else {
                    format!("{}(\"{}\"", from_id, from_name)
                }
            } else {
                format!("{}(\"{}\"", from_id, from_name)
            };

            let to_shape = if let Some(idx) = to_module {
                let m = &self.graph[*idx];
                if self.is_script(m) {
                    format!("{}[\"{}\"", to_id, to_name)
                } else if self.is_namespace_package(m) && include_namespace_packages {
                    format!("{}{{{{\"{}\"", to_id, to_name)
                } else {
                    format!("{}(\"{}\"", to_id, to_name)
                }
            } else {
                format!("{}(\"{}\"", to_id, to_name)
            };

            // Close the shapes
            let from_def = if from_shape.contains('[') {
                format!("{}]", from_shape)
            } else if from_shape.contains("{{{{") {
                format!("{}}}}}", from_shape)
            } else {
                format!("{})", from_shape)
            };

            let to_def = if to_shape.contains('[') {
                format!("{}]", to_shape)
            } else if to_shape.contains("{{{{") {
                format!("{}}}}}", to_shape)
            } else {
                format!("{})", to_shape)
            };

            output.push_str(&format!("    {} --> {}\n", from_def, to_def));
        }

        output
    }

    /// Find all modules that depend on the given root modules (downstream dependencies).
    /// Returns a map containing the roots and all modules that transitively depend on them,
    /// along with their distance from the nearest root.
    /// If max_rank is specified, only includes nodes within that distance.
    pub fn find_downstream(
        &self,
        roots: &[ModulePath],
        max_rank: Option<usize>,
    ) -> HashMap<ModulePath, usize> {
        let mut downstream: HashMap<ModulePath, usize> = HashMap::new();

        // Convert ModulePaths to NodeIndices
        let root_indices: Vec<NodeIndex> = roots
            .iter()
            .filter_map(|module| self.node_indices.get(module).copied())
            .collect();

        // Add the root modules themselves with distance 0
        for module in roots {
            if self.node_indices.contains_key(module) {
                downstream.insert(module.clone(), 0);
            }
        }

        // Use BFS on the reversed graph to find all modules that depend on the roots
        // In the reversed graph, edges point from dependents to dependencies
        let reversed = Reversed(&self.graph);

        for root_idx in root_indices {
            if let Some(max) = max_rank {
                // When max_rank is specified, use custom BFS with distance tracking
                let mut queue = std::collections::VecDeque::new();
                let mut visited = HashSet::new();

                queue.push_back((root_idx, 0));
                visited.insert(root_idx);

                while let Some((node_idx, distance)) = queue.pop_front() {
                    let module = &self.graph[node_idx];

                    // Update downstream map with minimum distance
                    downstream
                        .entry(module.clone())
                        .and_modify(|d| *d = (*d).min(distance))
                        .or_insert(distance);

                    // Only explore neighbors if we haven't reached max_rank
                    if distance < max {
                        for neighbor in self.graph.neighbors_directed(node_idx, Direction::Incoming)
                        {
                            if visited.insert(neighbor) {
                                queue.push_back((neighbor, distance + 1));
                            }
                        }
                    }
                }
            } else {
                // When max_rank is None, use petgraph's BFS (faster)
                let mut bfs = Bfs::new(&reversed, root_idx);
                while let Some(node_idx) = bfs.next(&reversed) {
                    let module = &self.graph[node_idx];
                    downstream.entry(module.clone()).or_insert(0);
                }
            }
        }

        downstream
    }

    /// Find all modules that the given root modules depend on (upstream dependencies).
    /// Returns a map containing the roots and all modules that they transitively depend on,
    /// along with their distance from the nearest root.
    /// If max_rank is specified, only includes nodes within that distance.
    pub fn find_upstream(
        &self,
        roots: &[ModulePath],
        max_rank: Option<usize>,
    ) -> HashMap<ModulePath, usize> {
        let mut upstream: HashMap<ModulePath, usize> = HashMap::new();

        // Convert ModulePaths to NodeIndices
        let root_indices: Vec<NodeIndex> = roots
            .iter()
            .filter_map(|module| self.node_indices.get(module).copied())
            .collect();

        // Add the root modules themselves with distance 0
        for module in roots {
            if self.node_indices.contains_key(module) {
                upstream.insert(module.clone(), 0);
            }
        }

        // Use BFS on the original graph to find all modules that the roots depend on
        // Edges point from modules to their dependencies
        for root_idx in root_indices {
            if let Some(max) = max_rank {
                // When max_rank is specified, use custom BFS with distance tracking
                let mut queue = std::collections::VecDeque::new();
                let mut visited = HashSet::new();

                queue.push_back((root_idx, 0));
                visited.insert(root_idx);

                while let Some((node_idx, distance)) = queue.pop_front() {
                    let module = &self.graph[node_idx];

                    // Update upstream map with minimum distance
                    upstream
                        .entry(module.clone())
                        .and_modify(|d| *d = (*d).min(distance))
                        .or_insert(distance);

                    // Only explore neighbors if we haven't reached max_rank
                    if distance < max {
                        for neighbor in self.graph.neighbors(node_idx) {
                            if visited.insert(neighbor) {
                                queue.push_back((neighbor, distance + 1));
                            }
                        }
                    }
                }
            } else {
                // When max_rank is None, use petgraph's BFS (faster)
                let mut bfs = Bfs::new(&self.graph, root_idx);
                while let Some(node_idx) = bfs.next(&self.graph) {
                    let module = &self.graph[node_idx];
                    upstream.entry(module.clone()).or_insert(0);
                }
            }
        }

        upstream
    }

    /// Check if a node is an orphan (has no incoming or outgoing edges)
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

    /// Convert a filtered set of modules to a sorted, newline-separated list of dotted module names
    pub fn to_list_filtered(
        &self,
        filter: &HashSet<ModulePath>,
        include_namespace_packages: bool,
    ) -> String {
        let mut sorted_modules: Vec<String> = filter
            .iter()
            .filter(|m| include_namespace_packages || !self.is_namespace_package(m))
            .map(|m| m.to_dotted())
            .collect();
        sorted_modules.sort();
        sorted_modules.join("\n")
    }

    /// Convert the graph to self-contained Cytoscape.js HTML
    pub fn to_cytoscape(&self, include_orphans: bool, include_namespace_packages: bool) -> String {
        let graph_data = self.to_cytoscape_graph_data(include_orphans, include_namespace_packages);
        Self::render_cytoscape_html(&graph_data)
    }

    /// Convert a filtered set of modules to Cytoscape.js HTML (subgraph)
    pub fn to_cytoscape_filtered(
        &self,
        filter: &HashSet<ModulePath>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let graph_data = self.to_cytoscape_graph_data_filtered(
            filter,
            include_orphans,
            include_namespace_packages,
        );
        Self::render_cytoscape_html(&graph_data)
    }

    /// Convert the full graph to Cytoscape.js HTML with highlighted nodes
    pub fn to_cytoscape_highlighted(
        &self,
        highlight_set: &HashSet<ModulePath>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> String {
        let graph_data = self.to_cytoscape_graph_data_highlighted(
            highlight_set,
            include_orphans,
            include_namespace_packages,
        );
        Self::render_cytoscape_html(&graph_data)
    }

    /// Build Cytoscape graph data without rendering it into HTML
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

    /// Build Cytoscape graph data for a filtered subgraph
    pub fn to_cytoscape_graph_data_filtered(
        &self,
        filter: &HashSet<ModulePath>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> GraphData {
        self.cytoscape_graph_data_internal(
            CytoscapeMode::Filtered(filter),
            include_orphans,
            include_namespace_packages,
        )
    }

    /// Build Cytoscape graph data with highlighted nodes
    pub fn to_cytoscape_graph_data_highlighted(
        &self,
        highlight_set: &HashSet<ModulePath>,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> GraphData {
        self.cytoscape_graph_data_internal(
            CytoscapeMode::Highlighted(highlight_set),
            include_orphans,
            include_namespace_packages,
        )
    }

    /// Internal method that builds Cytoscape.js graph data with optional filtering/highlighting
    ///
    /// Parameters:
    /// - filter_mode: None for full graph, Some((set, false)) for filtered, Some((set, true)) for highlighted
    /// - include_orphans: Whether to include orphan nodes
    /// - include_namespace_packages: Whether to include namespace packages
    fn cytoscape_graph_data_internal(
        &self,
        mode: CytoscapeMode,
        include_orphans: bool,
        include_namespace_packages: bool,
    ) -> GraphData {
        let filter_set = match mode {
            CytoscapeMode::Full => None,
            CytoscapeMode::Filtered(set) | CytoscapeMode::Highlighted(set) => Some(set),
        };
        let is_highlighting_mode = matches!(mode, CytoscapeMode::Highlighted(_));

        // Collect and sort nodes
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        // Apply filtering based on mode
        if let Some(filter) = filter_set {
            if !is_highlighting_mode {
                // Filtered mode: only include nodes in filter
                nodes.retain(|idx| filter.contains(&self.graph[*idx]));
            }
            // In highlighting mode, we keep all nodes
        }

        // Filter out namespace packages unless explicitly requested
        if !include_namespace_packages {
            nodes.retain(|idx| {
                let module = &self.graph[*idx];
                !self.is_namespace_package(module)
            });
        }

        // Filter out orphan nodes unless explicitly requested
        if !include_orphans {
            nodes.retain(|idx| {
                let has_incoming = self
                    .graph
                    .neighbors_directed(*idx, Direction::Incoming)
                    .count()
                    > 0;
                let has_outgoing = self
                    .graph
                    .neighbors_directed(*idx, Direction::Outgoing)
                    .count()
                    > 0;
                has_incoming || has_outgoing
            });
        }

        // Build namespace hierarchy from visible nodes
        let hierarchy = self.build_namespace_hierarchy(&nodes);
        let visible_indices: HashSet<NodeIndex> = nodes.iter().copied().collect();

        // Generate compound node relationships
        let (leaf_parent_map, parent_nodes) =
            self.generate_compound_nodes(&hierarchy, &visible_indices, include_namespace_packages);

        // Build graph nodes
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut graph_nodes = Vec::new();

        // First, add all pure parent nodes
        graph_nodes.extend(parent_nodes);

        // Then, add all leaf nodes (concrete modules/scripts)
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

            // Get parent from map
            let parent = leaf_parent_map.get(&module_name).cloned();

            graph_nodes.push(GraphNode {
                id: module_name,
                node_type: node_type.to_string(),
                is_orphan,
                highlighted: if is_highlighted { Some(true) } else { None },
                parent,
            });
        }

        // Build edge elements JSON with transitive edge preservation for namespace packages
        let mut edges: Vec<(String, String)> = Vec::new();

        if !include_namespace_packages {
            // When excluding namespace packages, create transitive edges
            for from_idx in self.graph.node_indices() {
                let from_module = &self.graph[from_idx];

                if !node_set.contains(&from_idx) {
                    continue;
                }

                // Apply filtering for filtered mode
                if let Some(filter) = filter_set {
                    if !is_highlighting_mode && !filter.contains(from_module) {
                        continue;
                    }
                }

                for to_idx in self.graph.neighbors(from_idx) {
                    let to_module = &self.graph[to_idx];

                    if self.is_namespace_package(to_module) {
                        let mut visited = HashSet::new();
                        self.find_transitive_non_namespace_targets(
                            to_idx,
                            &mut visited,
                            &node_set,
                            &mut |target_idx| {
                                let target_module = &self.graph[target_idx];

                                // Apply filtering for filtered mode
                                if let Some(filter) = filter_set {
                                    if !is_highlighting_mode && !filter.contains(target_module) {
                                        return;
                                    }
                                }

                                edges.push((from_module.to_dotted(), target_module.to_dotted()));
                            },
                        );
                    } else if node_set.contains(&to_idx) {
                        // Apply filtering for filtered mode
                        if let Some(filter) = filter_set {
                            if !is_highlighting_mode && !filter.contains(to_module) {
                                continue;
                            }
                        }

                        edges.push((from_module.to_dotted(), to_module.to_dotted()));
                    }
                }
            }
        } else {
            // Include all edges between visible nodes
            edges = self
                .graph
                .edge_indices()
                .filter_map(|e| self.graph.edge_endpoints(e))
                .filter(|(from, to)| node_set.contains(from) && node_set.contains(to))
                .filter(|(from, to)| {
                    if let Some(filter) = filter_set {
                        if !is_highlighting_mode {
                            return filter.contains(&self.graph[*from])
                                && filter.contains(&self.graph[*to]);
                        }
                    }
                    true
                })
                .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
                .collect();
        }

        // Remove duplicates and sort edges
        edges.sort();
        edges.dedup();

        // Build graph edges
        let graph_edges: Vec<GraphEdge> = edges
            .iter()
            .map(|(from, to)| GraphEdge {
                source: from.clone(),
                target: to.clone(),
            })
            .collect();

        // Determine highlighted modules if in highlighting mode
        let highlighted_modules = if is_highlighting_mode {
            filter_set.map(|set| {
                let mut modules: Vec<String> = set.iter().map(|m| m.to_dotted()).collect();
                modules.sort();
                modules
            })
        } else {
            None
        };

        // Create graph data
        let graph_data = GraphData {
            nodes: graph_nodes,
            edges: graph_edges,
            config: GraphConfig {
                include_orphans,
                include_namespaces: include_namespace_packages,
                highlighted_modules,
            },
        };

        graph_data
    }

    fn render_cytoscape_html(graph_data: &GraphData) -> String {
        generate_cytoscape_html(graph_data).expect(
            "Failed to generate HTML from template. Did you run ./scripts/build-frontend.sh?",
        )
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a self-contained HTML file with embedded Cytoscape.js visualization
fn generate_cytoscape_html(graph_data: &GraphData) -> Result<String, Box<dyn std::error::Error>> {
    // Load the template from the embedded file
    const TEMPLATE: &str = include_str!("../templates/cytoscape.html");

    // Serialize graph data to JSON
    let graph_json = serde_json::to_string(graph_data)?;

    // Replace the placeholder with the actual data
    let html = TEMPLATE.replace("<!--GRAPH_DATA_PLACEHOLDER-->", &graph_json);

    Ok(html)
}

/// Check if a given Python package directory is a namespace package
///
/// Detects two types:
/// 1. Native namespace packages (PEP 420): directories without __init__.py
/// 2. Legacy namespace packages: __init__.py containing pkgutil.extend_path() or pkg_resources.declare_namespace()
///
/// # Arguments
/// * `package_path` - Path to the package directory (should be a directory, not a file)
///
/// # Returns
/// `true` if the directory is a namespace package, `false` otherwise
fn is_namespace_package(package_path: &Path) -> bool {
    if !package_path.is_dir() {
        return false;
    }

    let init_path = package_path.join("__init__.py");

    // Native namespace package (PEP 420): directory with Python files but no __init__.py
    if !init_path.exists() {
        // Check if there are any .py files in the directory
        if let Ok(entries) = std::fs::read_dir(package_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Some(ext) = entry.path().extension() {
                    if ext == "py" {
                        return true; // Found a .py file without __init__.py -> namespace package
                    }
                }
            }
        }
        return false;
    }

    // Legacy namespace package: check __init__.py content for namespace declarations
    if let Ok(content) = std::fs::read_to_string(&init_path) {
        // Look for common namespace package patterns
        let has_pkgutil = content.contains("pkgutil.extend_path");
        let has_pkg_resources = content.contains("pkg_resources.declare_namespace");

        if has_pkgutil || has_pkg_resources {
            return true;
        }
    }

    false
}

/// Analyze a Python project and return its internal dependency graph
///
/// # Arguments
/// * `project_root` - The root directory of the Python project
/// * `source_root` - Optional explicit source root. If None, auto-detection will be used
/// * `exclude_patterns` - Glob patterns to exclude from script discovery
pub fn analyze_project(
    project_root: &Path,
    source_root: Option<&Path>,
    exclude_patterns: &[String],
) -> Result<DependencyGraph, PythonAnalysisError> {
    if !project_root.is_dir() {
        return Err(PythonAnalysisError::InvalidRoot(project_root.to_path_buf()));
    }

    // Determine the actual source root to use
    let actual_source_root = if let Some(explicit_root) = source_root {
        explicit_root.to_path_buf()
    } else {
        detect_source_root(project_root)?
    };

    let mut graph = DependencyGraph::new();

    // Collect all Python modules in the source root
    let mut modules: HashMap<ModulePath, PathBuf> = HashMap::new();

    for entry in WalkDir::new(&actual_source_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "py").unwrap_or(false))
    {
        let path = entry.path();
        if let Some(module_path) = ModulePath::from_file_path(path, &actual_source_root) {
            modules.insert(module_path, path.to_path_buf());
        }
    }

    // Detect namespace packages in the source root
    // We need to check all directories that could be packages
    for entry in WalkDir::new(&actual_source_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && e.path() != actual_source_root)
    {
        let dir_path = entry.path();
        if is_namespace_package(dir_path) {
            // Convert directory path to ModulePath
            if let Some(module_path) = ModulePath::from_file_path(
                &dir_path.join("__dummy__.py"), // Temporary file to get package path
                &actual_source_root,
            ) {
                // Remove the dummy file part to get the package path
                let mut package_parts = module_path.0;
                if !package_parts.is_empty()
                    && package_parts.last() == Some(&"__dummy__".to_string())
                {
                    package_parts.pop();
                    if !package_parts.is_empty() {
                        let package_module_path = ModulePath(package_parts);
                        graph.mark_as_namespace_package(&package_module_path);
                    }
                }
            }
        }
    }

    // Discover scripts outside the source root
    let mut scripts: HashMap<ModulePath, PathBuf> = HashMap::new();

    for entry in WalkDir::new(project_root)
        .into_iter()
        .filter_entry(|e| {
            // Skip the source root directory (already processed)
            if e.path() == actual_source_root {
                return false;
            }
            // Skip excluded directories
            !should_exclude_path(e.path(), project_root, exclude_patterns)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "py").unwrap_or(false))
    {
        let path = entry.path();
        // Only include files outside the source root
        if !path.starts_with(&actual_source_root) {
            if let Some(script_path) = ModulePath::from_script_path(path, project_root) {
                scripts.insert(script_path.clone(), path.to_path_buf());
                graph.mark_as_script(&script_path);
            }
        }
    }

    // Combine modules and scripts for import resolution
    let all_files: HashMap<ModulePath, PathBuf> = modules
        .iter()
        .chain(scripts.iter())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Analyze each module's imports (from source root)
    for (module_path, file_path) in &modules {
        let source = match std::fs::read_to_string(file_path) {
            Ok(source) => source,
            Err(e) => {
                eprintln!("Warning: Skipping file {}: {}", file_path.display(), e);
                continue;
            }
        };

        let imports = match extract_imports(&source) {
            Ok(imports) => imports,
            Err(message) => {
                eprintln!(
                    "Warning: Skipping unparseable file {}: {}",
                    file_path.display(),
                    message
                );
                continue;
            }
        };

        // Ensure the module exists in the graph even if it has no deps
        graph.get_or_create_node(module_path.clone());

        for import in imports {
            match import {
                Import::Absolute { module } => {
                    let resolved = ModulePath(module);
                    // Only add if it's an internal dependency
                    if all_files.contains_key(&resolved) || is_package_import(&resolved, &all_files)
                    {
                        graph.add_dependency(module_path.clone(), resolved);
                    }
                }
                Import::From {
                    module,
                    names,
                    level,
                } => {
                    // First resolve the base module path (e.g., "foo.bar" in "from foo.bar import a")
                    let module_str = module.as_ref().map(|v| v.join("."));
                    if let Some(base_path) =
                        module_path.resolve_relative(level, module_str.as_deref())
                    {
                        // For each imported name, try to resolve it as a submodule
                        for name in &names {
                            // Try resolving as a submodule (e.g., foo.bar.a)
                            let mut submodule_path = base_path.0.clone();
                            submodule_path.push(name.clone());
                            let submodule = ModulePath(submodule_path);

                            // Check if it's a submodule or fall back to the base package
                            if all_files.contains_key(&submodule) {
                                // It's a submodule (e.g., foo.bar.a exists as a file)
                                graph.add_dependency(module_path.clone(), submodule);
                            } else if all_files.contains_key(&base_path)
                                || is_package_import(&base_path, &all_files)
                            {
                                // It's importing a name from the package's __init__.py
                                // Create edge to the package itself
                                graph.add_dependency(module_path.clone(), base_path.clone());
                            }
                        }

                        // If no names were imported (star import), add edge to the base package
                        if names.is_empty() {
                            if all_files.contains_key(&base_path)
                                || is_package_import(&base_path, &all_files)
                            {
                                graph.add_dependency(module_path.clone(), base_path);
                            }
                        }
                    }
                }
            }
        }
    }

    // Analyze each script's imports (from outside source root)
    for (script_path, file_path) in &scripts {
        let source = match std::fs::read_to_string(file_path) {
            Ok(source) => source,
            Err(e) => {
                eprintln!("Warning: Skipping file {}: {}", file_path.display(), e);
                continue;
            }
        };

        let imports = match extract_imports(&source) {
            Ok(imports) => imports,
            Err(message) => {
                eprintln!(
                    "Warning: Skipping unparseable file {}: {}",
                    file_path.display(),
                    message
                );
                continue;
            }
        };

        // Ensure the script exists in the graph even if it has no deps
        graph.get_or_create_node(script_path.clone());

        for import in imports {
            match import {
                Import::Absolute { module } => {
                    let resolved = ModulePath(module);
                    // Only add if it's an internal dependency
                    if all_files.contains_key(&resolved) || is_package_import(&resolved, &all_files)
                    {
                        graph.add_dependency(script_path.clone(), resolved);
                    }
                }
                Import::From {
                    module,
                    names,
                    level,
                } => {
                    // First resolve the base module path
                    let module_str = module.as_ref().map(|v| v.join("."));
                    // For scripts, relative imports resolve against the script's own location
                    if let Some(base_path) =
                        script_path.resolve_relative(level, module_str.as_deref())
                    {
                        // For each imported name, try to resolve it as a submodule
                        for name in &names {
                            // Try resolving as a submodule (e.g., foo.bar.a)
                            let mut submodule_path = base_path.0.clone();
                            submodule_path.push(name.clone());
                            let submodule = ModulePath(submodule_path);

                            // Check if it's a submodule or fall back to the base package
                            if all_files.contains_key(&submodule) {
                                // It's a submodule (e.g., foo.bar.a exists as a file)
                                graph.add_dependency(script_path.clone(), submodule);
                            } else if all_files.contains_key(&base_path)
                                || is_package_import(&base_path, &all_files)
                            {
                                // It's importing a name from the package's __init__.py
                                // Create edge to the package itself
                                graph.add_dependency(script_path.clone(), base_path.clone());
                            }
                        }

                        // If no names were imported (star import), add edge to the base package
                        if names.is_empty() {
                            if all_files.contains_key(&base_path)
                                || is_package_import(&base_path, &all_files)
                            {
                                graph.add_dependency(script_path.clone(), base_path);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(graph)
}

/// Check if a module path refers to a package (directory with modules)
fn is_package_import(module: &ModulePath, modules: &HashMap<ModulePath, PathBuf>) -> bool {
    // Check if any known module starts with this path (it's a parent package)
    modules
        .keys()
        .any(|m| m.0.len() > module.0.len() && m.0.starts_with(&module.0))
}

/// Check if a path should be excluded based on exclusion patterns
fn should_exclude_path(path: &Path, project_root: &Path, exclude_patterns: &[String]) -> bool {
    // Get path relative to project root for pattern matching
    let relative_path = match path.strip_prefix(project_root) {
        Ok(rel) => rel,
        Err(_) => return true, // Exclude paths outside project root
    };

    let path_str = relative_path.to_string_lossy();

    // Default exclusion patterns
    let default_excludes = [
        "venv",
        ".venv",
        "__pycache__",
        ".git",
        ".pytest_cache",
        ".egg-info",
        "build",
        "dist",
        ".tox",
        ".mypy_cache",
        "node_modules",
        ".egg",
        "eggs",
    ];

    // Check if path contains any default excluded directories
    for component in relative_path.components() {
        if let Some(component_str) = component.as_os_str().to_str() {
            // Check exact match or prefix match for patterns like "venv*"
            for pattern in &default_excludes {
                if component_str == *pattern ||
                   (pattern.ends_with('*') && component_str.starts_with(pattern.trim_end_matches('*'))) ||
                   component_str.starts_with("venv") || // venv, venv1, venv_old, etc.
                   component_str.ends_with(".egg-info")
                {
                    return true;
                }
            }
        }
    }

    // Check custom exclusion patterns
    for pattern in exclude_patterns {
        // Simple glob pattern matching (supports * wildcard)
        if pattern.contains('*') {
            // Convert glob pattern to simple prefix/suffix/contains check
            if pattern.starts_with('*') && pattern.ends_with('*') {
                let substr = &pattern[1..pattern.len() - 1];
                if path_str.contains(substr) {
                    return true;
                }
            } else if pattern.starts_with('*') {
                let suffix = &pattern[1..];
                if path_str.ends_with(suffix) {
                    return true;
                }
            } else if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1];
                if path_str.starts_with(prefix) {
                    return true;
                }
            }
        } else if path_str.contains(pattern.as_str()) {
            return true;
        }
    }

    false
}

/// Parse pyproject.toml to find the configured source root
fn parse_pyproject_toml(project_root: &Path) -> Result<Option<PathBuf>, PythonAnalysisError> {
    let toml_path = project_root.join("pyproject.toml");

    if !toml_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&toml_path)
        .map_err(|e| PythonAnalysisError::ConfigReadError(toml_path.clone(), e))?;

    let config: toml::Value = content
        .parse()
        .map_err(|e| PythonAnalysisError::ConfigParseError(toml_path.clone(), e))?;

    // Try to extract source root from [tool.setuptools.packages.find] where
    let source_root = config
        .get("tool")
        .and_then(|t| t.get("setuptools"))
        .and_then(|s| s.get("packages"))
        .and_then(|p| p.get("find"))
        .and_then(|f| f.get("where"))
        .and_then(|w| w.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .map(|s| project_root.join(s));

    Ok(source_root)
}

/// Check if a directory contains Python packages (has .py files or __init__.py)
fn has_python_packages(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }

    WalkDir::new(path)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .any(|e| {
            let path = e.path();
            path.extension().map(|ext| ext == "py").unwrap_or(false)
                || path.join("__init__.py").exists()
        })
}

/// Detect the Python source root using heuristics
pub fn detect_source_root(project_root: &Path) -> Result<PathBuf, PythonAnalysisError> {
    // 1. Try parsing pyproject.toml
    if let Some(root) = parse_pyproject_toml(project_root)? {
        if root.is_dir() && has_python_packages(&root) {
            return Ok(root);
        }
    }

    // 2. Check for common source directory patterns
    for candidate in ["src", "lib/python"] {
        let path = project_root.join(candidate);
        if path.is_dir() && has_python_packages(&path) {
            return Ok(path);
        }
    }

    // 3. Use project root as fallback (flat layout)
    if has_python_packages(project_root) {
        return Ok(project_root.to_path_buf());
    }

    Err(PythonAnalysisError::NoSourceRootFound(
        project_root.to_path_buf(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_path_to_dotted() {
        let mp = ModulePath(vec!["pkg_a".to_string(), "module_a".to_string()]);
        assert_eq!(mp.to_dotted(), "pkg_a.module_a");
    }

    #[test]
    fn test_resolve_relative_level_1() {
        let mp = ModulePath(vec!["pkg_a".to_string(), "module_a".to_string()]);
        let resolved = mp.resolve_relative(1, Some("sibling"));
        assert_eq!(
            resolved.map(|m| m.to_dotted()),
            Some("pkg_a.sibling".to_string())
        );
    }
}
