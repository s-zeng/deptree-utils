use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod python;

/// Parse a module input, which can be either:
/// - A dotted module name like "pkg_a.module_a"
/// - A file path like "scripts/blah.py" or "src/pkg_a/module_a.py"
fn parse_module_input(
    input: &str,
    project_root: &Path,
    source_root: &Path,
) -> Result<python::ModulePath, String> {
    // Check if input looks like a file path
    let is_file_path = input.contains('/') || input.contains('\\') || input.ends_with(".py");

    if is_file_path {
        // Treat as file path
        let input_path = PathBuf::from(input);

        // Try to canonicalize the path, but if it fails (e.g., file doesn't exist),
        // still try to work with it as a relative path
        let abs_path = if input_path.is_absolute() {
            input_path.clone()
        } else {
            project_root.join(&input_path)
        };

        // Check if file exists
        if !abs_path.exists() {
            return Err(format!("File does not exist: {}", abs_path.display()));
        }

        // Canonicalize to resolve symlinks and relative components
        let canonical_path = abs_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize path {}: {}", abs_path.display(), e))?;

        // Also canonicalize project_root and source_root for comparison
        let canonical_project_root = project_root.canonicalize().map_err(|e| {
            format!(
                "Failed to canonicalize project root {}: {}",
                project_root.display(),
                e
            )
        })?;
        let canonical_source_root = source_root.canonicalize().map_err(|e| {
            format!(
                "Failed to canonicalize source root {}: {}",
                source_root.display(),
                e
            )
        })?;

        // Check if the path is under the project root
        if !canonical_path.starts_with(&canonical_project_root) {
            return Err(format!(
                "File {} is outside the project root {}",
                canonical_path.display(),
                canonical_project_root.display()
            ));
        }

        // Determine if it's inside the source root or outside (script)
        if canonical_path.starts_with(&canonical_source_root) {
            // Inside source root - use from_file_path
            python::ModulePath::from_file_path(&canonical_path, &canonical_source_root).ok_or_else(
                || {
                    format!(
                        "Failed to convert file path to module path: {}",
                        canonical_path.display()
                    )
                },
            )
        } else {
            // Outside source root - use from_script_path
            python::ModulePath::from_script_path(&canonical_path, &canonical_project_root)
                .ok_or_else(|| {
                    format!(
                        "Failed to convert script path to module path: {}",
                        canonical_path.display()
                    )
                })
        }
    } else {
        // Treat as dotted module name
        Ok(python::ModulePath(
            input.split('.').map(String::from).collect(),
        ))
    }
}

#[derive(Parser, Debug)]
#[clap(author = "Simon Zeng", version, about = "Dependency tree utilities")]
struct Args {
    /// Enable verbose output
    #[arg(short = 'v', global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Analyze Python project dependencies
    Python {
        /// Path to the Python project root
        #[arg()]
        path: PathBuf,

        /// Python source root directory (defaults to auto-detection)
        #[arg(long, short = 's')]
        source_root: Option<PathBuf>,

        /// Comma-separated list of modules to find downstream dependencies for
        #[arg(long)]
        downstream: Option<String>,

        /// Individual module to find downstream dependencies for (can be repeated)
        #[arg(long = "downstream-module")]
        downstream_module: Vec<String>,

        /// File containing newline-separated list of modules to find downstream dependencies for
        #[arg(long)]
        downstream_file: Option<PathBuf>,

        /// Glob patterns to exclude from script discovery (can be repeated)
        #[arg(long = "exclude-scripts")]
        exclude_scripts: Vec<String>,
    },

    /// Analyze upstream dependencies (what a module depends on)
    PythonUpstream {
        /// Path to the Python project root
        #[arg()]
        path: PathBuf,

        /// Python source root directory (defaults to auto-detection)
        #[arg(long, short = 's')]
        source_root: Option<PathBuf>,

        /// Comma-separated list of modules to find upstream dependencies for
        #[arg(long)]
        upstream: Option<String>,

        /// Individual module to find upstream dependencies for (can be repeated)
        #[arg(long = "upstream-module")]
        upstream_module: Vec<String>,

        /// File containing newline-separated list of modules to find upstream dependencies for
        #[arg(long)]
        upstream_file: Option<PathBuf>,

        /// Glob patterns to exclude from script discovery (can be repeated)
        #[arg(long = "exclude-scripts")]
        exclude_scripts: Vec<String>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.verbose {
        eprintln!("DEBUG {args:?}");
    }

    match args.command {
        Command::Python {
            path,
            source_root,
            downstream,
            downstream_module,
            downstream_file,
            exclude_scripts,
        } => {
            let graph = python::analyze_project(&path, source_root.as_deref(), &exclude_scripts)?;

            // Collect module names from all three sources
            let mut module_names: Vec<String> = Vec::new();

            // From comma-separated list
            if let Some(csv) = downstream {
                module_names.extend(csv.split(',').map(|s| s.trim().to_string()));
            }

            // From repeated --downstream-module flags
            module_names.extend(downstream_module);

            // From file
            if let Some(file_path) = downstream_file {
                // Check if user accidentally passed a Python source file instead of a list file
                if file_path.extension().and_then(|s| s.to_str()) == Some("py") {
                    return Err(format!(
                        "Error: --downstream-file expects a text file with module names (one per line), but got a Python file: {}\n\
                         Hint: If you want to analyze this module, use --downstream {} instead",
                        file_path.display(),
                        file_path.to_str().unwrap_or("").trim_end_matches(".py")
                    ).into());
                }

                let content = std::fs::read_to_string(&file_path).map_err(|e| {
                    format!(
                        "Failed to read downstream file {}: {}",
                        file_path.display(),
                        e
                    )
                })?;
                module_names.extend(
                    content
                        .lines()
                        .map(|line| line.trim())
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(String::from),
                );
            }

            // If downstream modules are specified, perform downstream analysis
            if !module_names.is_empty() {
                let module_paths: Vec<python::ModulePath> = module_names
                    .iter()
                    .map(|name| python::ModulePath(name.split('.').map(String::from).collect()))
                    .collect();

                let downstream_modules = graph.find_downstream(&module_paths);
                println!("{}", graph.to_module_list(&downstream_modules));
            } else {
                // Default behavior: output DOT graph
                println!("{}", graph.to_dot());
            }
        }

        Command::PythonUpstream {
            path,
            source_root,
            upstream,
            upstream_module,
            upstream_file,
            exclude_scripts,
        } => {
            // Determine the source root first (needed for parsing module inputs)
            let actual_source_root = if let Some(explicit_root) = source_root.as_ref() {
                explicit_root.clone()
            } else {
                python::detect_source_root(&path)?
            };

            let graph =
                python::analyze_project(&path, Some(&actual_source_root), &exclude_scripts)?;

            // Collect module names/paths from all three sources
            let mut module_inputs: Vec<String> = Vec::new();

            // From comma-separated list
            if let Some(csv) = upstream {
                module_inputs.extend(csv.split(',').map(|s| s.trim().to_string()));
            }

            // From repeated --upstream-module flags
            module_inputs.extend(upstream_module);

            // From file
            if let Some(file_path) = upstream_file {
                // Check if user accidentally passed a Python source file instead of a list file
                if file_path.extension().and_then(|s| s.to_str()) == Some("py") {
                    return Err(format!(
                        "Error: --upstream-file expects a text file with module names (one per line), but got a Python file: {}\n\
                         Hint: Use --upstream {} instead",
                        file_path.display(),
                        file_path.display()
                    ).into());
                }

                let content = std::fs::read_to_string(&file_path).map_err(|e| {
                    format!(
                        "Failed to read upstream file {}: {}",
                        file_path.display(),
                        e
                    )
                })?;
                module_inputs.extend(
                    content
                        .lines()
                        .map(|line| line.trim())
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(String::from),
                );
            }

            // Upstream modules must be specified
            if module_inputs.is_empty() {
                return Err("No modules specified for upstream analysis. Use --upstream, --upstream-module, or --upstream-file.".into());
            }

            // Parse module inputs (can be dotted names or file paths)
            let module_paths: Result<Vec<python::ModulePath>, String> = module_inputs
                .iter()
                .map(|input| parse_module_input(input, &path, &actual_source_root))
                .collect();
            let module_paths = module_paths?;

            let upstream_modules = graph.find_upstream(&module_paths);
            println!("{}", graph.to_dot_filtered(&upstream_modules));
        }
    }

    Ok(())
}
