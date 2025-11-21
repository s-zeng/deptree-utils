use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod python;

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
        } => {
            let graph = python::analyze_project(&path, source_root.as_deref())?;

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
                let content = std::fs::read_to_string(&file_path)
                    .map_err(|e| format!("Failed to read downstream file {}: {}", file_path.display(), e))?;
                module_names.extend(
                    content
                        .lines()
                        .map(|line| line.trim())
                        .filter(|line| !line.is_empty())
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
    }

    Ok(())
}
