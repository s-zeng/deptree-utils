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
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");
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
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project with unparseable files");
    let dot_output = graph.to_dot();

    // Snapshot should only contain valid_module and another_valid, not malformed
    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_downstream_single_module() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find all modules that depend on pkg_b.module_b
    let roots = vec![python::ModulePath(vec!["pkg_b".to_string(), "module_b".to_string()])];
    let downstream = graph.find_downstream(&roots);
    let output = graph.to_module_list(&downstream);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_multiple_modules() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

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
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // main has no modules depending on it
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let downstream = graph.find_downstream(&roots);
    let output = graph.to_module_list(&downstream);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_nonexistent_module() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Module that doesn't exist in the project
    let roots = vec![python::ModulePath(vec!["nonexistent".to_string()])];
    let downstream = graph.find_downstream(&roots);
    let output = graph.to_module_list(&downstream);

    // Should be empty
    insta::assert_snapshot!(output);
}

#[test]
fn test_src_layout_auto_detection() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("src_layout_project");

    // Auto-detection should find src/ directory from pyproject.toml
    let graph = python::analyze_project(&root, None, &[])
        .expect("Failed to analyze src layout project");
    let dot_output = graph.to_dot();

    // Should have same modules as flat layout (pkg_a, pkg_b, main)
    // Module names should be relative to src/ not project root
    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_lib_python_layout_auto_detection() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lib_python_layout");

    // Auto-detection should find lib/python/ directory via heuristics
    let graph = python::analyze_project(&root, None, &[])
        .expect("Failed to analyze lib/python layout");
    let dot_output = graph.to_dot();

    // Should have same modules as flat layout (pkg_a, pkg_b, main)
    // Module names should be relative to lib/python/ not project root
    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_explicit_source_root_override() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("src_layout_project");
    let source_root = project_root.join("src");

    // Explicitly specify source root instead of relying on auto-detection
    let graph = python::analyze_project(&project_root, Some(&source_root), &[])
        .expect("Failed to analyze with explicit source root");
    let dot_output = graph.to_dot();

    // Should produce same output as auto-detection
    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_project_with_scripts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    // Should discover scripts outside source root
    let graph = python::analyze_project(&root, None, &[])
        .expect("Failed to analyze project with scripts");
    let dot_output = graph.to_dot();

    // Should include:
    // - foo.bar (internal module)
    // - scripts.blah (script importing internal module)
    // - scripts.runner (script importing internal module and other script)
    // - scripts.utils.helper (script imported by runner)
    // Scripts should be shown with box shape
    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_script_imports_internal_module() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let graph = python::analyze_project(&root, None, &[])
        .expect("Failed to analyze project with scripts");

    // scripts.blah should depend on foo.bar
    let foo_bar = python::ModulePath(vec!["foo".to_string(), "bar".to_string()]);

    // Find downstream dependencies of foo.bar - should include scripts.blah
    let downstream = graph.find_downstream(&[foo_bar.clone()]);
    let output = graph.to_module_list(&downstream);

    // Should include both foo.bar and scripts.blah
    insta::assert_snapshot!(output);
}

#[test]
fn test_script_relative_imports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let graph = python::analyze_project(&root, None, &[])
        .expect("Failed to analyze project with scripts");

    // scripts.runner should depend on scripts.utils.helper (via relative import)
    let downstream = graph.find_downstream(&[
        python::ModulePath(vec!["scripts".to_string(), "utils".to_string(), "helper".to_string()])
    ]);
    let output = graph.to_module_list(&downstream);

    // Should include scripts.utils.helper and scripts.runner
    insta::assert_snapshot!(output);
}

#[test]
fn test_exclude_scripts_pattern() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    // Exclude the scripts directory entirely
    let graph = python::analyze_project(&root, None, &["scripts".to_string()])
        .expect("Failed to analyze project with exclusions");
    let dot_output = graph.to_dot();

    // Should only include foo.bar, no scripts
    insta::assert_snapshot!(dot_output);
}
