//! Python internal dependency tree analyzer
//!
//! Parses Python files to extract import statements and builds a dependency graph
//! of internal module dependencies.

use petgraph::Direction;
use petgraph::algo::dijkstra;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::Reversed;
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

struct MermaidRenderArgs<'a> {
    highlight_set: Option<&'a HashSet<ModulePath>>,
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

/// Selection mode for rendering/filtering
enum NodeSelection<'a> {
    /// All nodes
    Full,
    /// Only nodes in the provided set
    Filtered(&'a HashSet<ModulePath>),
    /// All nodes, but keep track of which are highlighted
    Highlighted,
}

/// Represents a node in the namespace hierarchy tree
#[derive(Debug, Clone)]
struct NamespaceTree {
    /// Full module path for this namespace (e.g., ["foo", "bar"])
    path: Vec<String>,
    /// Whether this namespace corresponds to a concrete module
    is_concrete: bool,
    /// Child namespaces/modules
    children: Vec<NamespaceTree>,
    /// Whether this namespace should render as a group (2+ children)
    grouped: bool,
}

impl NamespaceTree {
    fn new(path: Vec<String>) -> Self {
        Self {
            path,
            is_concrete: false,
            children: Vec::new(),
            grouped: false,
        }
    }

    fn insert(&mut self, parts: &[String]) {
        if parts.is_empty() {
            self.is_concrete = true;
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
            existing.insert(&parts[1..]);
        } else {
            let mut new_child = NamespaceTree::new(child_path);
            new_child.insert(&parts[1..]);
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

    fn find(&self, path: &[String]) -> Option<&NamespaceTree> {
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
            .map(|node| node.grouped && node.is_concrete)
            .unwrap_or(false)
    }

    fn direct_concrete_children(&self) -> Vec<ModulePath> {
        self.children
            .iter()
            .filter(|c| c.is_concrete)
            .map(|c| ModulePath(c.path.clone()))
            .collect()
    }

    fn child_groups(&self) -> impl Iterator<Item = &NamespaceTree> {
        self.children.iter()
    }

    fn collect_leaf_descendants(&self, acc: &mut Vec<ModulePath>) {
        if self.children.is_empty() {
            if self.is_concrete {
                acc.push(ModulePath(self.path.clone()));
            }
            return;
        }

        for child in &self.children {
            child.collect_leaf_descendants(acc);
        }
    }

    fn collect_ungrouped_modules(&self, acc: &mut Vec<ModulePath>) {
        if self.grouped {
            for child in &self.children {
                child.collect_ungrouped_modules(acc);
            }
            return;
        }

        for child in &self.children {
            if child.is_concrete {
                acc.push(ModulePath(child.path.clone()));
            }
            child.collect_ungrouped_modules(acc);
        }
    }
}

/// Root namespace trees for internal modules and scripts
#[derive(Debug)]
struct NamespaceForest {
    internal: NamespaceTree,
    scripts: NamespaceTree,
}

enum EitherGraph<'a> {
    Forward(&'a DiGraph<ModulePath, ()>),
    Reversed(Reversed<&'a DiGraph<ModulePath, ()>>),
}

impl<'a> EitherGraph<'a> {
    fn run_dijkstra(&self, start: NodeIndex) -> HashMap<NodeIndex, usize> {
        match self {
            EitherGraph::Forward(graph) => dijkstra(*graph, start, None, |_| 1usize),
            EitherGraph::Reversed(graph) => dijkstra(*graph, start, None, |_| 1usize),
        }
    }
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

    fn select_visible_nodes(
        &self,
        selection: NodeSelection,
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
    ) -> Vec<(String, String)> {
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

        edges.sort();
        edges.dedup();
        edges
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

    /// Build the complete namespace forest from graph nodes
    fn build_namespace_forest(&self, visible_nodes: &[NodeIndex]) -> NamespaceForest {
        let mut internal = NamespaceTree::new(vec![]);
        let mut scripts = NamespaceTree::new(vec![]);

        for idx in visible_nodes {
            let module_path = &self.graph[*idx];
            let target = if self.is_script(module_path) {
                &mut scripts
            } else {
                &mut internal
            };
            target.insert(&module_path.0);
        }

        internal.finalize();
        scripts.finalize();

        NamespaceForest { internal, scripts }
    }

    fn tree_for<'a>(&self, forest: &'a NamespaceForest, module: &ModulePath) -> &'a NamespaceTree {
        if self.is_script(module) {
            &forest.scripts
        } else {
            &forest.internal
        }
    }

    /// Check if a module is a group-only namespace (has children and should only appear as group)
    fn is_group_only_namespace(&self, forest: &NamespaceForest, module: &ModulePath) -> bool {
        self.tree_for(forest, module).is_group_only(&module.0)
    }

    /// Generate compound nodes from namespace hierarchy for Cytoscape output
    /// Returns (leaf_parent_map, pure_parent_nodes)
    /// - leaf_parent_map: Maps module IDs to their parent IDs
    /// - pure_parent_nodes: Pure parent nodes (namespace groups that are not concrete modules)
    fn generate_compound_nodes(
        &self,
        forest: &NamespaceForest,
        include_namespace_packages: bool,
    ) -> (HashMap<String, String>, Vec<GraphNode>) {
        let mut leaf_parent_map = HashMap::new();
        let mut parent_nodes = Vec::new();

        // Process internal modules
        self.collect_compound_nodes_recursive(
            &forest.internal,
            None,
            include_namespace_packages,
            &mut leaf_parent_map,
            &mut parent_nodes,
        );

        // Process scripts
        self.collect_compound_nodes_recursive(
            &forest.scripts,
            None,
            include_namespace_packages,
            &mut leaf_parent_map,
            &mut parent_nodes,
        );

        (leaf_parent_map, parent_nodes)
    }

    /// Recursively collect compound node relationships from namespace hierarchy
    #[allow(clippy::only_used_in_recursion)]
    fn collect_compound_nodes_recursive(
        &self,
        node: &NamespaceTree,
        parent_id: Option<String>,
        include_namespace_packages: bool,
        leaf_parent_map: &mut HashMap<String, String>,
        parent_nodes: &mut Vec<GraphNode>,
    ) {
        // Root node - process children without creating a parent
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

        // If this node should group (2+ children), create a parent node
        if node.grouped {
            // Create parent node only if it's NOT a concrete module
            // (if it's concrete, it will be a leaf node too - hybrid node)
            if !node.is_concrete {
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
            // Not a group - just a leaf or intermediate node
            // If concrete, it will be added as leaf later
            if node.is_concrete {
                // Record parent relationship
                if let Some(pid) = parent_id.clone() {
                    leaf_parent_map.insert(current_id, pid);
                }
            }

            // Continue recursively for children (propagate parent)
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

    /// Get all visible leaf descendants of a namespace (for edge redirection)
    fn get_visible_leaf_descendants(
        &self,
        forest: &NamespaceForest,
        module: &ModulePath,
    ) -> Vec<ModulePath> {
        self.tree_for(forest, module)
            .find(&module.0)
            .map(|node| {
                let mut descendants = Vec::new();
                node.collect_leaf_descendants(&mut descendants);
                descendants
            })
            .unwrap_or_default()
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

    fn dot_spec_for_module(
        &self,
        module: &ModulePath,
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
        highlight_set: Option<&HashSet<ModulePath>>,
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

    /// Helper to recursively render DOT subgraphs for namespace groups (with optional highlighting)
    #[allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]
    fn render_dot_subgraph_generic(
        &self,
        node: &NamespaceTree,
        forest: &NamespaceForest,
        highlight_set: Option<&HashSet<ModulePath>>,
        include_namespace_packages: bool,
        specs: &HashMap<String, DotNodeSpec>,
        cluster_root: bool,
        indent_level: usize,
        is_script_root: bool,
        output: &mut String,
    ) {
        let indent = "    ".repeat(indent_level);

        if node.children.is_empty() && !node.is_concrete {
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

            // Find all direct children modules (leaf nodes at this level)
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

    /// Helper to collect modules that should not be grouped (don't belong to any group)
    fn collect_ungrouped_modules(&self, node: &NamespaceTree, ungrouped: &mut Vec<ModulePath>) {
        node.collect_ungrouped_modules(ungrouped);
    }

    /// Convert the graph to Graphviz DOT format
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

        // Collect and render ungrouped nodes (nodes not in any group)
        let mut ungrouped: Vec<ModulePath> = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);

        // Sort ungrouped nodes for deterministic output
        ungrouped.sort_by_key(|module| module.to_dotted());

        for module in &ungrouped {
            if !self.is_group_only_namespace(&forest, module) {
                if let Some(spec) = specs.get(&module.to_dotted()) {
                    output.push_str(&spec.render(""));
                }
            }
        }

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut edges = self.collect_edges(&node_set, include_namespace_packages);

        // Transform edges to redirect group-only namespaces to their children
        let mut transformed_edges = Vec::new();
        for (from_name, to_name) in edges {
            let from_module = ModulePath(from_name.split('.').map(String::from).collect());
            let to_module = ModulePath(to_name.split('.').map(String::from).collect());

            let from_is_group_only = self.is_group_only_namespace(&forest, &from_module);
            let to_is_group_only = self.is_group_only_namespace(&forest, &to_module);

            match (from_is_group_only, to_is_group_only) {
                (false, false) => {
                    // Normal edge
                    transformed_edges.push((from_name, to_name));
                }
                (true, false) => {
                    // From is group-only: create edges from all leaf descendants
                    let descendants = self.get_visible_leaf_descendants(&forest, &from_module);
                    for descendant in descendants {
                        transformed_edges.push((descendant.to_dotted(), to_name.clone()));
                    }
                }
                (false, true) => {
                    // To is group-only: create edges to all leaf descendants
                    let descendants = self.get_visible_leaf_descendants(&forest, &to_module);
                    for descendant in descendants {
                        transformed_edges.push((from_name.clone(), descendant.to_dotted()));
                    }
                }
                (true, true) => {
                    // Both are group-only: cartesian product of descendants
                    let from_descendants = self.get_visible_leaf_descendants(&forest, &from_module);
                    let to_descendants = self.get_visible_leaf_descendants(&forest, &to_module);
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
            output.push_str(&format!("    \"{from_name}\" -> \"{to_name}\";\n"));
        }

        output.push_str("}\n");
        output
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

        // Collect and render ungrouped nodes
        let mut ungrouped: Vec<ModulePath> = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);

        ungrouped.sort_by_key(|module| module.to_dotted());

        for module in &ungrouped {
            if !self.is_group_only_namespace(&forest, module) {
                if let Some(spec) = specs.get(&module.to_dotted()) {
                    output.push_str(&spec.render(""));
                }
            }
        }

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);

        // Add edges
        for (from_name, to_name) in edges {
            output.push_str(&format!("    \"{from_name}\" -> \"{to_name}\";\n"));
        }

        output.push_str("}\n");
        output
    }

    fn mermaid_spec_for_module(
        &self,
        module: &ModulePath,
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

    /// Helper to recursively render Mermaid subgraphs for namespace groups
    #[allow(clippy::only_used_in_recursion)]
    fn render_mermaid_subgraph(
        &self,
        node: &NamespaceTree,
        indent_level: usize,
        args: &MermaidRenderArgs<'_>,
        highlighted_nodes: &mut HashSet<String>,
        output: &mut String,
    ) {
        let indent = "    ".repeat(indent_level);

        // Root node should never create a subgraph, just process children
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

            // Find and render direct children modules
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

            // Recursively render child groups
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

    /// Convert the graph to Mermaid flowchart format
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

        // Collect and render ungrouped nodes
        let mut ungrouped = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);

        ungrouped.sort_by_key(|module: &ModulePath| module.to_dotted());

        for module in &ungrouped {
            if let Some(spec) = specs.get(&module.to_dotted()) {
                output.push_str(&spec.render_definition("", false));
            }
        }

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);

        // Add edges (which implicitly define nodes)
        for (from_name, to_name) in edges {
            if let Some(line) = self.render_mermaid_edge(&from_name, &to_name, &specs) {
                output.push_str(&line);
            }
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
        let nodes = self.select_visible_nodes(
            NodeSelection::Highlighted,
            include_orphans,
            include_namespace_packages,
        );
        let specs = self.mermaid_spec_map(&nodes, include_namespace_packages);
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);
        let forest = self.build_namespace_forest(&nodes);
        let mut highlighted_nodes: std::collections::HashSet<String> =
            std::collections::HashSet::new();
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
        ungrouped.sort_by_key(|module| module.to_dotted());

        for module in &ungrouped {
            let is_highlighted = highlight_set.contains(module);
            if let Some(spec) = specs.get(&module.to_dotted()) {
                if is_highlighted {
                    highlighted_nodes.insert(spec.id.clone());
                }
                output.push_str(&spec.render_definition("", is_highlighted));
            }
        }

        let highlighted_names: std::collections::HashSet<String> =
            highlight_set.iter().map(ModulePath::to_dotted).collect();

        // Add edges (which implicitly define nodes)
        for (from_name, to_name) in edges {
            if let Some(line) = self.render_mermaid_edge(&from_name, &to_name, &specs) {
                output.push_str(&line);
            }

            if highlighted_names.contains(&from_name) {
                if let Some(spec) = specs.get(&from_name) {
                    if highlighted_nodes.insert(spec.id.clone()) {
                        output.push_str(&format!("    class {} highlighted\n", spec.id));
                    }
                }
            }
            if highlighted_names.contains(&to_name) {
                if let Some(spec) = specs.get(&to_name) {
                    if highlighted_nodes.insert(spec.id.clone()) {
                        output.push_str(&format!("    class {} highlighted\n", spec.id));
                    }
                }
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

        // Collect and render ungrouped nodes
        let mut ungrouped: Vec<ModulePath> = Vec::new();
        self.collect_ungrouped_modules(&forest.internal, &mut ungrouped);
        self.collect_ungrouped_modules(&forest.scripts, &mut ungrouped);

        ungrouped.sort_by_key(|module| module.to_dotted());

        for module in &ungrouped {
            if !self.is_group_only_namespace(&forest, module) {
                if let Some(spec) = specs.get(&module.to_dotted()) {
                    output.push_str(&spec.render(""));
                }
            }
        }

        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let edges = self.collect_edges(&node_set, include_namespace_packages);

        // Add edges
        for (from_name, to_name) in edges {
            output.push_str(&format!("    \"{from_name}\" -> \"{to_name}\";\n"));
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
                if let Some(spec) = specs.get(&module_name) {
                    output.push_str(&spec.render_definition("", false));
                }
            }
        }

        // Add edges (which implicitly define nodes)
        for (from_name, to_name) in edges {
            if let Some(line) = self.render_mermaid_edge(&from_name, &to_name, &specs) {
                output.push_str(&line);
            }
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
        self.collect_with_distances(roots, max_rank, Direction::Incoming)
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
        self.collect_with_distances(roots, max_rank, Direction::Outgoing)
    }

    fn collect_with_distances(
        &self,
        roots: &[ModulePath],
        max_rank: Option<usize>,
        direction: Direction,
    ) -> HashMap<ModulePath, usize> {
        let distance_maps = roots
            .iter()
            .filter_map(|module| self.node_indices.get(module).map(|&idx| (module, idx)));

        let mut aggregated: HashMap<NodeIndex, usize> = HashMap::new();

        for (_, start_idx) in distance_maps {
            let view = match direction {
                Direction::Outgoing => EitherGraph::Forward(&self.graph),
                Direction::Incoming => EitherGraph::Reversed(Reversed(&self.graph)),
            };

            let distances = view.run_dijkstra(start_idx);

            for (idx, distance) in distances {
                if max_rank.map(|limit| distance > limit).unwrap_or(false) {
                    continue;
                }
                match aggregated.get(&idx) {
                    Some(&existing) if existing <= distance => {}
                    _ => {
                        aggregated.insert(idx, distance);
                    }
                }
            }
        }

        aggregated
            .into_iter()
            .map(|(idx, distance)| (self.graph[idx].clone(), distance))
            .collect()
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
        let selection = match mode {
            CytoscapeMode::Full => NodeSelection::Full,
            CytoscapeMode::Filtered(set) => NodeSelection::Filtered(set),
            CytoscapeMode::Highlighted(_) => NodeSelection::Highlighted,
        };

        let nodes =
            self.select_visible_nodes(selection, include_orphans, include_namespace_packages);

        // Build namespace hierarchy from visible nodes
        let forest = self.build_namespace_forest(&nodes);

        // Generate compound node relationships
        let (leaf_parent_map, parent_nodes) =
            self.generate_compound_nodes(&forest, include_namespace_packages);

        // Build graph nodes
        let node_set: HashSet<NodeIndex> = nodes.iter().copied().collect();
        let mut graph_nodes = Vec::new();

        // First, add all pure parent nodes
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
        let edges = self.collect_edges(&node_set, include_namespace_packages);

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

        GraphData {
            nodes: graph_nodes,
            edges: graph_edges,
            config: GraphConfig {
                include_orphans,
                include_namespaces: include_namespace_packages,
                highlighted_modules,
            },
        }
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
    #[derive(Clone, Copy)]
    enum SourceKind {
        Internal,
        Script,
    }

    struct SourceFile {
        module: ModulePath,
        path: PathBuf,
        kind: SourceKind,
    }

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
    let mut sources: Vec<SourceFile> = Vec::new();

    for entry in WalkDir::new(&actual_source_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "py").unwrap_or(false))
    {
        let path = entry.path();
        if let Some(module_path) = ModulePath::from_file_path(path, &actual_source_root) {
            sources.push(SourceFile {
                module: module_path,
                path: path.to_path_buf(),
                kind: SourceKind::Internal,
            });
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
                graph.mark_as_script(&script_path);
                sources.push(SourceFile {
                    module: script_path,
                    path: path.to_path_buf(),
                    kind: SourceKind::Script,
                });
            }
        }
    }

    // Combine modules and scripts for import resolution
    let all_files: HashMap<ModulePath, PathBuf> = sources
        .iter()
        .map(|source| (source.module.clone(), source.path.clone()))
        .collect();

    // Analyze each source file's imports using a single pipeline
    for source_file in &sources {
        let SourceFile {
            module: module_path,
            path: file_path,
            kind,
        } = source_file;

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
        if matches!(kind, SourceKind::Script) {
            graph.mark_as_script(module_path);
        }

        for import in imports {
            match import {
                Import::Absolute { module } => {
                    let resolved = ModulePath(module);
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
                    let module_str = module.as_ref().map(|v| v.join("."));
                    if let Some(base_path) =
                        module_path.resolve_relative(level, module_str.as_deref())
                    {
                        for name in &names {
                            let mut submodule_path = base_path.0.clone();
                            submodule_path.push(name.clone());
                            let submodule = ModulePath(submodule_path);

                            if all_files.contains_key(&submodule) {
                                graph.add_dependency(module_path.clone(), submodule);
                            } else if all_files.contains_key(&base_path)
                                || is_package_import(&base_path, &all_files)
                            {
                                graph.add_dependency(module_path.clone(), base_path.clone());
                            }
                        }

                        if names.is_empty()
                            && (all_files.contains_key(&base_path)
                                || is_package_import(&base_path, &all_files))
                        {
                            graph.add_dependency(module_path.clone(), base_path);
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
            if let Some(substr) = pattern.strip_prefix('*').and_then(|p| p.strip_suffix('*')) {
                if path_str.contains(substr) {
                    return true;
                }
            } else if let Some(suffix) = pattern.strip_prefix('*') {
                if path_str.ends_with(suffix) {
                    return true;
                }
            } else if let Some(prefix) = pattern.strip_suffix('*') {
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
