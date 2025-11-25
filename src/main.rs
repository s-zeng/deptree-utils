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

        /// Output format: 'dot', 'mermaid', 'list', or 'cytoscape' (default: dot)
        #[arg(long, default_value = "dot", value_parser = ["dot", "mermaid", "list", "cytoscape"])]
        format: String,

        /// Comma-separated list of modules to find downstream dependencies for
        #[arg(long)]
        downstream: Option<String>,

        /// Individual module to find downstream dependencies for (can be repeated)
        #[arg(long = "downstream-module")]
        downstream_module: Vec<String>,

        /// File containing newline-separated list of modules to find downstream dependencies for
        #[arg(long)]
        downstream_file: Option<PathBuf>,

        /// Comma-separated list of modules to find upstream dependencies for
        #[arg(long)]
        upstream: Option<String>,

        /// Individual module to find upstream dependencies for (can be repeated)
        #[arg(long = "upstream-module")]
        upstream_module: Vec<String>,

        /// File containing newline-separated list of modules to find upstream dependencies for
        #[arg(long)]
        upstream_file: Option<PathBuf>,

        /// Include only nodes within distance N from specified modules
        #[arg(long)]
        max_rank: Option<usize>,

        /// Glob patterns to exclude from script discovery (can be repeated)
        #[arg(long = "exclude-scripts")]
        exclude_scripts: Vec<String>,

        /// Include orphan nodes (nodes with no dependencies) in DOT output
        #[arg(long)]
        include_orphans: bool,

        /// Show full graph with highlighted nodes instead of filtering (requires --downstream or --upstream)
        #[arg(long)]
        show_all: bool,

        /// Include namespace packages in the output (by default they are excluded)
        #[arg(long)]
        include_namespace_packages: bool,
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
            format,
            downstream,
            downstream_module,
            downstream_file,
            upstream,
            upstream_module,
            upstream_file,
            max_rank,
            exclude_scripts,
            include_orphans,
            show_all,
            include_namespace_packages,
        } => {
            // Determine the source root first (needed for parsing module inputs with file paths)
            let actual_source_root = if let Some(explicit_root) = source_root.as_ref() {
                explicit_root.clone()
            } else {
                python::detect_source_root(&path)?
            };

            let graph =
                python::analyze_project(&path, Some(&actual_source_root), &exclude_scripts)?;

            // Collect downstream module inputs from all three sources
            let mut downstream_inputs: Vec<String> = Vec::new();

            // From comma-separated list
            if let Some(csv) = downstream {
                downstream_inputs.extend(csv.split(',').map(|s| s.trim().to_string()));
            }

            // From repeated --downstream-module flags
            downstream_inputs.extend(downstream_module);

            // From file
            if let Some(file_path) = downstream_file {
                // Check if user accidentally passed a Python source file instead of a list file
                if file_path.extension().and_then(|s| s.to_str()) == Some("py") {
                    return Err(format!(
                        "Error: --downstream-file expects a text file with module names (one per line), but got a Python file: {}\n\
                         Hint: If you want to analyze this module, use --downstream {} instead",
                        file_path.display(),
                        file_path.display()
                    ).into());
                }

                let content = std::fs::read_to_string(&file_path).map_err(|e| {
                    format!(
                        "Failed to read downstream file {}: {}",
                        file_path.display(),
                        e
                    )
                })?;
                downstream_inputs.extend(
                    content
                        .lines()
                        .map(|line| line.trim())
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(String::from),
                );
            }

            // Collect upstream module inputs from all three sources
            let mut upstream_inputs: Vec<String> = Vec::new();

            // From comma-separated list
            if let Some(csv) = upstream {
                upstream_inputs.extend(csv.split(',').map(|s| s.trim().to_string()));
            }

            // From repeated --upstream-module flags
            upstream_inputs.extend(upstream_module);

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
                upstream_inputs.extend(
                    content
                        .lines()
                        .map(|line| line.trim())
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(String::from),
                );
            }

            // Parse output format
            let output_format = match format.as_str() {
                "dot" => python::OutputFormat::Dot,
                "mermaid" => python::OutputFormat::Mermaid,
                "list" => python::OutputFormat::List,
                "cytoscape" => python::OutputFormat::Cytoscape,
                _ => unreachable!("Invalid format validated by clap"),
            };

            // Determine what kind of analysis to perform
            let has_downstream = !downstream_inputs.is_empty();
            let has_upstream = !upstream_inputs.is_empty();

            // Validate show_all flag usage
            if show_all && !has_downstream && !has_upstream {
                return Err(
                    "--show-all requires --downstream or --upstream to be specified".into()
                );
            }

            if has_downstream || has_upstream {
                // Parse downstream module inputs (can be dotted names or file paths)
                let downstream_paths: Option<Vec<python::ModulePath>> = if has_downstream {
                    let paths: Result<Vec<python::ModulePath>, String> = downstream_inputs
                        .iter()
                        .map(|input| parse_module_input(input, &path, &actual_source_root))
                        .collect();
                    Some(paths?)
                } else {
                    None
                };

                // Parse upstream module inputs (can be dotted names or file paths)
                let upstream_paths: Option<Vec<python::ModulePath>> = if has_upstream {
                    let paths: Result<Vec<python::ModulePath>, String> = upstream_inputs
                        .iter()
                        .map(|input| parse_module_input(input, &path, &actual_source_root))
                        .collect();
                    Some(paths?)
                } else {
                    None
                };

                // Compute the filter set based on which flags are provided
                let filter: std::collections::HashSet<python::ModulePath> = match (
                    downstream_paths,
                    upstream_paths,
                ) {
                    (Some(down_paths), Some(up_paths)) => {
                        // Both downstream and upstream specified: compute intersection
                        let downstream_modules = graph.find_downstream(&down_paths, max_rank);
                        let upstream_modules = graph.find_upstream(&up_paths, max_rank);

                        let downstream_set: std::collections::HashSet<_> =
                            downstream_modules.keys().cloned().collect();
                        let upstream_set: std::collections::HashSet<_> =
                            upstream_modules.keys().cloned().collect();

                        downstream_set
                            .intersection(&upstream_set)
                            .cloned()
                            .collect()
                    }
                    (Some(down_paths), None) => {
                        // Only downstream specified
                        let downstream_modules = graph.find_downstream(&down_paths, max_rank);
                        downstream_modules.keys().cloned().collect()
                    }
                    (None, Some(up_paths)) => {
                        // Only upstream specified
                        let upstream_modules = graph.find_upstream(&up_paths, max_rank);
                        upstream_modules.keys().cloned().collect()
                    }
                    (None, None) => unreachable!("Already checked has_downstream || has_upstream"),
                };

                match output_format {
                    python::OutputFormat::Dot => {
                        if show_all {
                            println!("{}", graph.to_dot_highlighted(&filter, include_orphans, include_namespace_packages));
                        } else {
                            println!("{}", graph.to_dot_filtered(&filter, include_orphans, include_namespace_packages));
                        }
                    }
                    python::OutputFormat::Mermaid => {
                        if show_all {
                            println!("{}", graph.to_mermaid_highlighted(&filter, include_orphans, include_namespace_packages));
                        } else {
                            println!("{}", graph.to_mermaid_filtered(&filter, include_orphans, include_namespace_packages));
                        }
                    }
                    python::OutputFormat::Cytoscape => {
                        if show_all {
                            println!("{}", graph.to_cytoscape_highlighted(&filter, include_orphans, include_namespace_packages));
                        } else {
                            println!("{}", graph.to_cytoscape_filtered(&filter, include_orphans, include_namespace_packages));
                        }
                    }
                    python::OutputFormat::List => {
                        if show_all {
                            return Err(
                                "--show-all cannot be used with --format list".into()
                            );
                        }
                        println!("{}", graph.to_list_filtered(&filter, include_namespace_packages));
                    }
                }
            } else {
                // Default behavior: output full graph in the specified format
                match output_format {
                    python::OutputFormat::Dot => {
                        println!("{}", graph.to_dot(include_orphans, include_namespace_packages));
                    }
                    python::OutputFormat::Mermaid => {
                        println!("{}", graph.to_mermaid(include_orphans, include_namespace_packages));
                    }
                    python::OutputFormat::Cytoscape => {
                        println!("{}", graph.to_cytoscape(include_orphans, include_namespace_packages));
                    }
                    python::OutputFormat::List => {
                        return Err(
                            "List format requires --downstream or --upstream to be specified".into()
                        );
                    }
                }
            }
        }
    }

    Ok(())
}
