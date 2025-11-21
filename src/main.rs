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
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.verbose {
        eprintln!("DEBUG {args:?}");
    }

    match args.command {
        Command::Python { path } => {
            let graph = python::analyze_project(&path)?;
            println!("{}", graph.to_dot());
        }
    }

    Ok(())
}
