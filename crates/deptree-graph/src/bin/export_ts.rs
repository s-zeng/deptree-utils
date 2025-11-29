use std::error::Error;
use std::fs;
use std::path::PathBuf;

use deptree_graph::GraphData;
use ts_rs::TS;

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("frontend")
        .join("src")
        .join("bindings");

    fs::create_dir_all(&out_dir)?;

    GraphData::export_all_to(&out_dir)
        .map_err(|err| format!("failed to export TypeScript bindings: {err}"))?;

    println!("Generated TypeScript bindings in {}", out_dir.display());
    Ok(())
}
