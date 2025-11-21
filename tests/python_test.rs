//! Integration tests for Python dependency analysis

use std::path::PathBuf;

// Re-export from main crate
#[path = "../src/python.rs"]
mod python;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample_python_project")
}

#[test]
fn test_sample_python_project_dot_output() {
    let root = fixture_path();
    let graph = python::analyze_project(&root).expect("Failed to analyze project");
    let dot_output = graph.to_dot();

    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_module_path_from_file_path() {
    let root = fixture_path();
    let file_path = root.join("pkg_a").join("module_a.py");

    let module = python::ModulePath::from_file_path(&file_path, &root);
    assert!(module.is_some());
    insta::assert_snapshot!(module.unwrap().to_dotted());
}

#[test]
fn test_init_file_represents_package() {
    let root = fixture_path();
    let init_path = root.join("pkg_a").join("__init__.py");

    let module = python::ModulePath::from_file_path(&init_path, &root);
    assert!(module.is_some());
    // __init__.py should represent the package itself, not "pkg_a.__init__"
    insta::assert_snapshot!(module.unwrap().to_dotted());
}

#[test]
fn test_skip_unparseable_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("unparseable_python_project");

    // Should succeed despite malformed.py containing invalid syntax
    let graph = python::analyze_project(&root).expect("Failed to analyze project with unparseable files");
    let dot_output = graph.to_dot();

    // Snapshot should only contain valid_module and another_valid, not malformed
    insta::assert_snapshot!(dot_output);
}
