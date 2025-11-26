use std::path::Path;

fn main() {
    let template_path = "templates/cytoscape.html";

    // Check if the frontend template exists
    if !Path::new(template_path).exists() {
        eprintln!("WARNING: Frontend template not found at {}", template_path);
        eprintln!("Run `./scripts/build-frontend.sh` to build the frontend bundle");
        eprintln!("Continuing with build, but the frontend template will be missing");
    }

    // Rerun build script if template changes
    println!("cargo:rerun-if-changed={}", template_path);

    // Also rerun if the build script itself changes
    println!("cargo:rerun-if-changed=build.rs");
}
