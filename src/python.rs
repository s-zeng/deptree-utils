//! Python internal dependency tree analyzer
//!
//! Parses Python files to extract import statements and builds a dependency graph
//! of internal module dependencies.

use petgraph::graph::{DiGraph, NodeIndex};
use ruff_python_parser::parse_module;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

/// Errors that can occur during Python dependency analysis
#[derive(Error, Debug)]
pub enum PythonAnalysisError {
    #[error("Invalid project root: {0}")]
    InvalidRoot(PathBuf),
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
    use ruff_python_ast::{Stmt, StmtImport, StmtImportFrom};

    let parsed = parse_module(source).map_err(|e| e.to_string())?;

    let mut imports = Vec::new();

    for stmt in parsed.suite() {
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
    }

    Ok(imports)
}

/// The dependency graph of Python modules
pub struct DependencyGraph {
    graph: DiGraph<ModulePath, ()>,
    node_indices: HashMap<ModulePath, NodeIndex>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
        }
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
    pub fn to_dot(&self) -> String {
        let mut output = String::from("digraph dependencies {\n");
        output.push_str("    rankdir=LR;\n");

        // Collect and sort nodes for deterministic output
        let mut nodes: Vec<_> = self.graph.node_indices().collect();
        nodes.sort_by_key(|idx| self.graph[*idx].to_dotted());

        // Add nodes
        for idx in &nodes {
            let module = &self.graph[*idx];
            output.push_str(&format!("    \"{}\";\n", module.to_dotted()));
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
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Analyze a Python project and return its internal dependency graph
pub fn analyze_project(root: &Path) -> Result<DependencyGraph, PythonAnalysisError> {
    if !root.is_dir() {
        return Err(PythonAnalysisError::InvalidRoot(root.to_path_buf()));
    }

    let mut graph = DependencyGraph::new();

    // Collect all Python modules in the project
    let mut modules: HashMap<ModulePath, PathBuf> = HashMap::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "py").unwrap_or(false))
    {
        let path = entry.path();
        if let Some(module_path) = ModulePath::from_file_path(path, root) {
            modules.insert(module_path, path.to_path_buf());
        }
    }

    // Analyze each module's imports
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
                eprintln!("Warning: Skipping unparseable file {}: {}", file_path.display(), message);
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

            // Only add if it's an internal dependency
            if let Some(resolved) = resolved {
                if modules.contains_key(&resolved) || is_package_import(&resolved, &modules) {
                    graph.add_dependency(module_path.clone(), resolved);
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
