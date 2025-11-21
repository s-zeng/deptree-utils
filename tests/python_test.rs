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

#[test]
fn test_downstream_single_module() {
    let root = fixture_path();
    let graph = python::analyze_project(&root).expect("Failed to analyze project");

    // Find all modules that depend on pkg_b.module_b
    let roots = vec![python::ModulePath(vec!["pkg_b".to_string(), "module_b".to_string()])];
    let downstream = graph.find_downstream(&roots);
    let output = graph.to_module_list(&downstream);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_multiple_modules() {
    let root = fixture_path();
    let graph = python::analyze_project(&root).expect("Failed to analyze project");

    // Find all modules that depend on both pkg_a.module_a and pkg_b.module_b
    let roots = vec![
        python::ModulePath(vec!["pkg_a".to_string(), "module_a".to_string()]),
        python::ModulePath(vec!["pkg_b".to_string(), "module_b".to_string()]),
    ];
    let downstream = graph.find_downstream(&roots);
    let output = graph.to_module_list(&downstream);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_no_dependents() {
    let root = fixture_path();
    let graph = python::analyze_project(&root).expect("Failed to analyze project");

    // main has no modules depending on it
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let downstream = graph.find_downstream(&roots);
    let output = graph.to_module_list(&downstream);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_nonexistent_module() {
    let root = fixture_path();
    let graph = python::analyze_project(&root).expect("Failed to analyze project");

    // Module that doesn't exist in the project
    let roots = vec![python::ModulePath(vec!["nonexistent".to_string()])];
    let downstream = graph.find_downstream(&roots);
    let output = graph.to_module_list(&downstream);

    // Should be empty
    insta::assert_snapshot!(output);
}
