//! Python internal dependency tree analyzer
//!
//! Parses Python files to extract import statements and builds a dependency graph
//! of internal module dependencies.

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{Bfs, Reversed};
use petgraph::Direction;
use ruff_python_parser::parse_module;
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
        level: u32, // 0 = absolute, 1 = ., 2 = .., etc.
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
                names: _,
                level,
                ..
            }) => {
                let module_parts = module
                    .as_ref()
                    .map(|m| m.as_str().split('.').map(String::from).collect());
                imports.push(Import::From {
                    module: module_parts,
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

/// The dependency graph of Python modules
pub struct DependencyGraph {
    graph: DiGraph<ModulePath, ()>,
    node_indices: HashMap<ModulePath, NodeIndex>,
    scripts: HashSet<ModulePath>, // Track which modules are scripts (outside source root)
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
            scripts: HashSet::new(),
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

    /// Convert the graph to Graphviz DOT format
    pub fn to_dot(&self, include_orphans: bool) -> String {
        let mut output = String::from("digraph dependencies {\n");
        output.push_str("    rankdir=LR;\n");
        output.push_str(
            "    // Note: Scripts (files outside source root) are shown with box shape\n",
        );

        // Collect and sort nodes for deterministic output
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

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

        // Add nodes
        for idx in &nodes {
            let module = &self.graph[*idx];
            if self.is_script(module) {
                // Scripts get a different visual style (box shape)
                output.push_str(&format!("    \"{}\" [shape=box];\n", module.to_dotted()));
            } else {
                output.push_str(&format!("    \"{}\";\n", module.to_dotted()));
            }
        }

        // Collect and sort edges for deterministic output
        let mut edges: Vec<_> = self
            .graph
            .edge_indices()
            .filter_map(|e| self.graph.edge_endpoints(e))
            .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
            .collect();
        edges.sort();

        // Add edges
        for (from_name, to_name) in edges {
            output.push_str(&format!("    \"{}\" -> \"{}\";\n", from_name, to_name));
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

        // Add nodes with highlighting
        for idx in &nodes {
            let module = &self.graph[*idx];
            let is_highlighted = highlight_set.contains(module);

            if self.is_script(module) {
                // Scripts get a different visual style (box shape)
                if is_highlighted {
                    output.push_str(&format!(
                        "    \"{}\" [shape=box, fillcolor=lightblue, style=filled];\n",
                        module.to_dotted()
                    ));
                } else {
                    output.push_str(&format!("    \"{}\" [shape=box];\n", module.to_dotted()));
                }
            } else {
                // Regular modules
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

        // Collect and sort edges for deterministic output
        let mut edges: Vec<_> = self
            .graph
            .edge_indices()
            .filter_map(|e| self.graph.edge_endpoints(e))
            .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
            .collect();
        edges.sort();

        // Add edges
        for (from_name, to_name) in edges {
            output.push_str(&format!("    \"{}\" -> \"{}\";\n", from_name, to_name));
        }

        output.push_str("}\n");
        output
    }

    /// Convert the graph to Mermaid flowchart format
    pub fn to_mermaid(&self, include_orphans: bool) -> String {
        let mut output = String::from("flowchart TD\n");

        // Collect and sort nodes for deterministic output
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

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

        // Collect and sort edges for deterministic output
        let mut edges: Vec<_> = self
            .graph
            .edge_indices()
            .filter_map(|e| self.graph.edge_endpoints(e))
            .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
            .collect();
        edges.sort();

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

            // Determine shapes based on whether modules are scripts
            let from_module = self.node_indices.get(&ModulePath(
                from_name.split('.').map(String::from).collect(),
            ));
            let to_module = self.node_indices.get(&ModulePath(
                to_name.split('.').map(String::from).collect(),
            ));

            let from_shape = if from_module.map(|idx| {
                let m = &self.graph[*idx];
                self.is_script(m)
            }).unwrap_or(false) {
                format!("{}[\"{}\"", from_id, from_name)
            } else {
                format!("{}(\"{}\"", from_id, from_name)
            };

            let to_shape = if to_module.map(|idx| {
                let m = &self.graph[*idx];
                self.is_script(m)
            }).unwrap_or(false) {
                format!("{}[\"{}\"", to_id, to_name)
            } else {
                format!("{}(\"{}\"", to_id, to_name)
            };

            // Close the shapes
            let from_def = if from_shape.contains('[') {
                format!("{}]", from_shape)
            } else {
                format!("{})", from_shape)
            };

            let to_def = if to_shape.contains('[') {
                format!("{}]", to_shape)
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
    ) -> String {
        let mut output = String::from("flowchart TD\n");

        // Collect and sort nodes for deterministic output
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

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

        // Collect and sort edges for deterministic output
        let mut edges: Vec<_> = self
            .graph
            .edge_indices()
            .filter_map(|e| self.graph.edge_endpoints(e))
            .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
            .collect();
        edges.sort();

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
                    if is_highlighted {
                        output.push_str(&format!("    {}[\"{}\"]\n", node_id, module_name));
                    } else {
                        output.push_str(&format!("    {}[\"{}\"]\n", node_id, module_name));
                    }
                } else {
                    // Modules get rounded rectangle shape
                    if is_highlighted {
                        output.push_str(&format!("    {}(\"{}\")\n", node_id, module_name));
                    } else {
                        output.push_str(&format!("    {}(\"{}\")\n", node_id, module_name));
                    }
                }

                // Apply highlighting class if needed
                if is_highlighted {
                    output.push_str(&format!("    class {} highlighted\n", node_id));
                }
            }
        }

        // Track which nodes have been assigned the highlighted class to avoid duplicates
        let mut highlighted_nodes: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Add edges (which implicitly define nodes)
        for (from_name, to_name) in edges {
            let from_id = sanitize_mermaid_id(&from_name);
            let to_id = sanitize_mermaid_id(&to_name);

            // Determine shapes based on whether modules are scripts
            let from_module = self.node_indices.get(&ModulePath(
                from_name.split('.').map(String::from).collect(),
            ));
            let to_module = self.node_indices.get(&ModulePath(
                to_name.split('.').map(String::from).collect(),
            ));

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
            } else {
                format!("{})", from_shape)
            };

            let to_def = if to_shape.contains('[') {
                format!("{}]", to_shape)
            } else {
                format!("{})", to_shape)
            };

            output.push_str(&format!("    {} --> {}\n", from_def, to_def));

            // Apply highlighting class to nodes that appear in edges (avoid duplicates)
            let from_module_path = ModulePath(from_name.split('.').map(String::from).collect());
            let to_module_path = ModulePath(to_name.split('.').map(String::from).collect());

            if highlight_set.contains(&from_module_path) && highlighted_nodes.insert(from_id.clone()) {
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
    pub fn to_dot_filtered(&self, filter: &HashSet<ModulePath>, include_orphans: bool) -> String {
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

        // Add nodes
        for idx in &nodes {
            let module = &self.graph[*idx];
            if self.is_script(module) {
                // Scripts get a different visual style (box shape)
                output.push_str(&format!("    \"{}\" [shape=box];\n", module.to_dotted()));
            } else {
                output.push_str(&format!("    \"{}\";\n", module.to_dotted()));
            }
        }

        // Collect and sort edges where both endpoints are in the filter
        let mut edges: Vec<_> = self
            .graph
            .edge_indices()
            .filter_map(|e| self.graph.edge_endpoints(e))
            .filter(|(from, to)| {
                filter.contains(&self.graph[*from]) && filter.contains(&self.graph[*to])
            })
            .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
            .collect();
        edges.sort();

        // Add edges
        for (from_name, to_name) in edges {
            output.push_str(&format!("    \"{}\" -> \"{}\";\n", from_name, to_name));
        }

        output.push_str("}\n");
        output
    }

    /// Convert a filtered set of modules to Mermaid flowchart format (subgraph).
    /// Only includes nodes and edges where both endpoints are in the filtered set.
    pub fn to_mermaid_filtered(&self, filter: &HashSet<ModulePath>, include_orphans: bool) -> String {
        let mut output = String::from("flowchart TD\n");

        // Collect and sort nodes that are in the filter
        let mut nodes: Vec<_> = self
            .graph
            .node_indices()
            .filter(|idx| filter.contains(&self.graph[*idx]))
            .collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

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

        // Collect and sort edges where both endpoints are in the filter
        let mut edges: Vec<_> = self
            .graph
            .edge_indices()
            .filter_map(|e| self.graph.edge_endpoints(e))
            .filter(|(from, to)| {
                filter.contains(&self.graph[*from]) && filter.contains(&self.graph[*to])
            })
            .map(|(from, to)| (self.graph[from].to_dotted(), self.graph[to].to_dotted()))
            .collect();
        edges.sort();

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

            // Determine shapes based on whether modules are scripts
            let from_module = self.node_indices.get(&ModulePath(
                from_name.split('.').map(String::from).collect(),
            ));
            let to_module = self.node_indices.get(&ModulePath(
                to_name.split('.').map(String::from).collect(),
            ));

            let from_shape = if from_module.map(|idx| {
                let m = &self.graph[*idx];
                self.is_script(m)
            }).unwrap_or(false) {
                format!("{}[\"{}\"", from_id, from_name)
            } else {
                format!("{}(\"{}\"", from_id, from_name)
            };

            let to_shape = if to_module.map(|idx| {
                let m = &self.graph[*idx];
                self.is_script(m)
            }).unwrap_or(false) {
                format!("{}[\"{}\"", to_id, to_name)
            } else {
                format!("{}(\"{}\"", to_id, to_name)
            };

            // Close the shapes
            let from_def = if from_shape.contains('[') {
                format!("{}]", from_shape)
            } else {
                format!("{})", from_shape)
            };

            let to_def = if to_shape.contains('[') {
                format!("{}]", to_shape)
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
    pub fn find_downstream(&self, roots: &[ModulePath], max_rank: Option<usize>) -> HashMap<ModulePath, usize> {
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
                        for neighbor in self.graph.neighbors_directed(node_idx, Direction::Incoming) {
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
    pub fn find_upstream(&self, roots: &[ModulePath], max_rank: Option<usize>) -> HashMap<ModulePath, usize> {
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

    /// Convert a filtered set of modules to a sorted, newline-separated list of dotted module names
    pub fn to_list_filtered(&self, filter: &HashSet<ModulePath>) -> String {
        let mut sorted_modules: Vec<String> = filter.iter().map(|m| m.to_dotted()).collect();
        sorted_modules.sort();
        sorted_modules.join("\n")
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
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
            let resolved = match import {
                Import::Absolute { module } => Some(ModulePath(module)),
                Import::From { module, level } => {
                    let module_str = module.as_ref().map(|v| v.join("."));
                    module_path.resolve_relative(level, module_str.as_deref())
                }
            };

            // Only add if it's an internal dependency (exists in modules or all_files)
            if let Some(resolved) = resolved {
                if all_files.contains_key(&resolved) || is_package_import(&resolved, &all_files) {
                    graph.add_dependency(module_path.clone(), resolved);
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
            let resolved = match import {
                Import::Absolute { module } => Some(ModulePath(module)),
                Import::From { module, level } => {
                    let module_str = module.as_ref().map(|v| v.join("."));
                    // For scripts, relative imports resolve against the script's own location
                    script_path.resolve_relative(level, module_str.as_deref())
                }
            };

            // Only add if it's an internal dependency (module or script)
            if let Some(resolved) = resolved {
                if all_files.contains_key(&resolved) || is_package_import(&resolved, &all_files) {
                    graph.add_dependency(script_path.clone(), resolved);
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
