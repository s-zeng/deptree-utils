//! Python internal dependency tree analyzer
//!
//! Parses Python files to extract import statements and builds a dependency graph
//! of internal module dependencies.

use deptree_graph::{DependencyGraph, GraphId, filters};
use ruff_python_parser::parse_module;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

/// Concrete dependency graph for Python modules.
pub type PythonGraph = DependencyGraph<ModulePath>;

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
    /// Create a module path from a dotted module string (e.g., "pkg_a.module_a")
    pub fn from_dotted(input: &str) -> Option<Self> {
        if input.trim().is_empty() {
            return None;
        }
        Some(ModulePath(input.split('.').map(str::to_string).collect()))
    }

    /// Create a module path from a file path relative to the project root
    pub fn from_file_path(path: &Path, root: &Path) -> Option<Self> {
        let relative = path.strip_prefix(root).ok()?;
        let mut parts: Vec<String> = relative
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(String::from))
            .collect();

        if let Some(last) = parts.last_mut() {
            if last.ends_with(".py") {
                *last = last.strip_suffix(".py")?.to_string();
            }
        }

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

        let up_count = level as usize;
        if up_count > self.0.len() {
            return None;
        }

        let mut base: Vec<String> = self.0[..self.0.len() - up_count + 1].to_vec();
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

impl GraphId for ModulePath {
    fn to_dotted(&self) -> String {
        self.0.join(".")
    }

    fn segments(&self) -> Vec<String> {
        self.0.clone()
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
        names: Vec<String>,
        level: u32,
    },
}

/// Extract imports from a Python source file
fn extract_imports(source: &str) -> Result<Vec<Import>, String> {
    let parsed = parse_module(source).map_err(|e| e.to_string())?;

    let mut imports = Vec::new();
    visit_stmts(parsed.suite(), &mut imports);

    Ok(imports)
}

/// Recursively visit all statements in the AST to extract imports
fn visit_stmts(stmts: &[ruff_python_ast::Stmt], imports: &mut Vec<Import>) {
    use ruff_python_ast::{Stmt, StmtImport, StmtImportFrom};

    for stmt in stmts {
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

                let imported_names: Vec<String> = names
                    .iter()
                    .filter_map(|alias| {
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

        match stmt {
            Stmt::FunctionDef(func) => {
                visit_stmts(&func.body, imports);
            }
            Stmt::ClassDef(class) => {
                visit_stmts(&class.body, imports);
            }
            Stmt::If(if_stmt) => {
                visit_stmts(&if_stmt.body, imports);
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

/// Check if a given Python package directory is a namespace package
///
/// Detects two types:
/// 1. Native namespace packages (PEP 420): directories without __init__.py
/// 2. Legacy namespace packages: __init__.py containing pkgutil.extend_path() or pkg_resources.declare_namespace()
fn is_namespace_package(package_path: &Path) -> bool {
    if !package_path.is_dir() {
        return false;
    }

    let init_path = package_path.join("__init__.py");

    if !init_path.exists() {
        if let Ok(entries) = std::fs::read_dir(package_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Some(ext) = entry.path().extension() {
                    if ext == "py" {
                        return true;
                    }
                }
            }
        }
        return false;
    }

    if let Ok(content) = std::fs::read_to_string(&init_path) {
        let has_pkgutil = content.contains("pkgutil.extend_path");
        let has_pkg_resources = content.contains("pkg_resources.declare_namespace");

        if has_pkgutil || has_pkg_resources {
            return true;
        }
    }

    false
}

/// Analyze a Python project and return its internal dependency graph
pub fn analyze_project(
    project_root: &Path,
    source_root: Option<&Path>,
    exclude_patterns: &[String],
) -> Result<PythonGraph, PythonAnalysisError> {
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

    let actual_source_root = if let Some(explicit_root) = source_root {
        explicit_root.to_path_buf()
    } else {
        detect_source_root(project_root)?
    };

    let mut graph = PythonGraph::new();

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

    for entry in WalkDir::new(&actual_source_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && e.path() != actual_source_root)
    {
        let dir_path = entry.path();
        if is_namespace_package(dir_path) {
            if let Some(module_path) =
                ModulePath::from_file_path(&dir_path.join("__dummy__.py"), &actual_source_root)
            {
                let mut package_parts = module_path.0;
                if !package_parts.is_empty()
                    && package_parts.last() == Some(&"__dummy__".to_string())
                {
                    package_parts.pop();
                    if !package_parts.is_empty() {
                        let package_module_path = ModulePath(package_parts);
                        graph.mark_as_namespace_package(&package_module_path);
                        graph.ensure_node(package_module_path);
                    }
                }
            }
        }
    }

    for entry in WalkDir::new(project_root)
        .into_iter()
        .filter_entry(|e| {
            if e.path() == actual_source_root {
                return false;
            }
            !should_exclude_path(e.path(), project_root, exclude_patterns)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "py").unwrap_or(false))
    {
        let path = entry.path();
        if !path.starts_with(&actual_source_root) {
            if let Some(script_path) = ModulePath::from_script_path(path, project_root) {
                graph.mark_as_script(&script_path);
                graph.ensure_node(script_path.clone());
                sources.push(SourceFile {
                    module: script_path,
                    path: path.to_path_buf(),
                    kind: SourceKind::Script,
                });
            }
        }
    }

    let all_files: HashMap<ModulePath, PathBuf> = sources
        .iter()
        .map(|source| (source.module.clone(), source.path.clone()))
        .collect();

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

        graph.ensure_node(module_path.clone());
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

fn is_package_import(module: &ModulePath, modules: &HashMap<ModulePath, PathBuf>) -> bool {
    modules
        .keys()
        .any(|m| m.0.len() > module.0.len() && m.0.starts_with(&module.0))
}

fn should_exclude_path(path: &Path, project_root: &Path, exclude_patterns: &[String]) -> bool {
    let relative_path = match path.strip_prefix(project_root) {
        Ok(rel) => rel,
        Err(_) => return true,
    };

    let path_str = relative_path.to_string_lossy();

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

    for component in relative_path.components() {
        if let Some(component_str) = component.as_os_str().to_str() {
            for pattern in &default_excludes {
                if component_str == *pattern
                    || (pattern.ends_with('*')
                        && component_str.starts_with(pattern.trim_end_matches('*')))
                    || component_str.starts_with("venv")
                    || component_str.ends_with(".egg-info")
                {
                    return true;
                }
            }
        }
    }

    exclude_patterns
        .iter()
        .any(|pattern| filters::matches_pattern(&path_str, pattern))
}

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

pub fn detect_source_root(project_root: &Path) -> Result<PathBuf, PythonAnalysisError> {
    if let Some(root) = parse_pyproject_toml(project_root)? {
        if root.is_dir() && has_python_packages(&root) {
            return Ok(root);
        }
    }

    for candidate in ["src", "lib/python"] {
        let path = project_root.join(candidate);
        if path.is_dir() && has_python_packages(&path) {
            return Ok(path);
        }
    }

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
