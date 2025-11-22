//! Integration tests for Python dependency analysis

use std::path::PathBuf;
use std::process::Command;

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
    let dot_output = graph.to_dot(false);

    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_sample_python_project_mermaid_output() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");
    let mermaid_output = graph.to_mermaid(false);

    insta::assert_snapshot!(mermaid_output);
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
    let graph = python::analyze_project(&root, None, &[])
        .expect("Failed to analyze project with unparseable files");
    let dot_output = graph.to_dot(false);

    // Snapshot should only contain valid_module and another_valid, not malformed
    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_downstream_single_module() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find all modules that depend on pkg_b.module_b
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

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
    let downstream = graph.find_downstream(&roots, None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_no_dependents() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // main has no modules depending on it
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let downstream = graph.find_downstream(&roots, None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_nonexistent_module() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Module that doesn't exist in the project
    let roots = vec![python::ModulePath(vec!["nonexistent".to_string()])];
    let downstream = graph.find_downstream(&roots, None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

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
    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze src layout project");
    let dot_output = graph.to_dot(false);

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
    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze lib/python layout");
    let dot_output = graph.to_dot(false);

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
    let dot_output = graph.to_dot(false);

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
    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze project with scripts");
    let dot_output = graph.to_dot(false);

    // Should include:
    // - foo.bar (internal module)
    // - scripts.blah (script importing internal module)
    // - scripts.runner (script importing internal module and other script)
    // - scripts.utils.helper (script imported by runner)
    // Scripts should be shown with box shape
    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_project_with_scripts_mermaid() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    // Should discover scripts outside source root
    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze project with scripts");
    let mermaid_output = graph.to_mermaid(false);

    // Should include:
    // - foo.bar (internal module) - rounded rectangle shape
    // - scripts.blah (script) - rectangle shape
    // - scripts.runner (script) - rectangle shape
    // - scripts.utils.helper (script) - rectangle shape
    insta::assert_snapshot!(mermaid_output);
}

#[test]
fn test_script_imports_internal_module() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze project with scripts");

    // scripts.blah should depend on foo.bar
    let foo_bar = python::ModulePath(vec!["foo".to_string(), "bar".to_string()]);

    // Find downstream dependencies of foo.bar - should include scripts.blah
    let downstream = graph.find_downstream(&[foo_bar.clone()], None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    // Should include both foo.bar and scripts.blah
    insta::assert_snapshot!(output);
}

#[test]
fn test_script_relative_imports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze project with scripts");

    // scripts.runner should depend on scripts.utils.helper (via relative import)
    let downstream = graph.find_downstream(&[python::ModulePath(vec![
        "scripts".to_string(),
        "utils".to_string(),
        "helper".to_string(),
    ])], None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

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
    let dot_output = graph.to_dot(false);

    // Should only include foo.bar, no scripts
    insta::assert_snapshot!(dot_output);
}

#[test]
fn test_upstream_single_module_no_deps() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find all modules that pkg_b.module_b depends on (it has no internal dependencies)
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_single_module_with_deps() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find all modules that pkg_a.module_a depends on
    // Should include pkg_a.module_a and pkg_b.module_b
    let roots = vec![python::ModulePath(vec![
        "pkg_a".to_string(),
        "module_a".to_string(),
    ])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_transitive_deps() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find all modules that main depends on (should include transitive dependencies)
    // Should include: main, pkg_a.module_a, pkg_b.module_b
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_transitive_deps_mermaid() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find all modules that main depends on (should include transitive dependencies)
    // Should include: main, pkg_a.module_a, pkg_b.module_b
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_mermaid_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_multiple_modules() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find all modules that both pkg_a.module_a and pkg_b.module_b depend on
    // Union of their upstream dependencies
    let roots = vec![
        python::ModulePath(vec!["pkg_a".to_string(), "module_a".to_string()]),
        python::ModulePath(vec!["pkg_b".to_string(), "module_b".to_string()]),
    ];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_nonexistent_module() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Module that doesn't exist in the project
    let roots = vec![python::ModulePath(vec!["nonexistent".to_string()])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    // Should be empty graph
    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_with_scripts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze project with scripts");

    // Find what scripts.blah depends on
    // Should include scripts.blah and foo.bar
    let roots = vec![python::ModulePath(vec![
        "scripts".to_string(),
        "blah".to_string(),
    ])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    // Should show box shape for scripts.blah
    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_with_scripts_mermaid() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze project with scripts");

    // Find what scripts.blah depends on
    // Should include scripts.blah (rectangle) and foo.bar (rounded rectangle)
    let roots = vec![python::ModulePath(vec![
        "scripts".to_string(),
        "blah".to_string(),
    ])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_mermaid_filtered(&filter, false);

    // Should show rectangle shape for scripts.blah, rounded for foo.bar
    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_script_with_relative_imports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze project with scripts");

    // Find what scripts.runner depends on
    // Should include scripts.runner, scripts.utils.helper, and foo.bar
    let roots = vec![python::ModulePath(vec![
        "scripts".to_string(),
        "runner".to_string(),
    ])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    // Should show dependencies between scripts and to internal modules
    insta::assert_snapshot!(output);
}

// Tests for nested imports (imports inside functions, classes, conditionals, etc.)

#[test]
fn test_function_level_imports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("nested_imports_project");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze nested imports project");

    // function_imports should depend on both base_module (top-level) and another_module (function-level)
    let roots = vec![python::ModulePath(vec!["function_imports".to_string()])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    // Should include function_imports, base_module, and another_module
    insta::assert_snapshot!(output);
}

#[test]
fn test_class_method_imports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("nested_imports_project");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze nested imports project");

    // class_imports should depend on base_module (imported in method)
    let roots = vec![python::ModulePath(vec!["class_imports".to_string()])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    // Should include class_imports and base_module
    insta::assert_snapshot!(output);
}

#[test]
fn test_conditional_imports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("nested_imports_project");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze nested imports project");

    // conditional_imports should depend on both base_module (if block) and another_module (try block)
    let roots = vec![python::ModulePath(vec!["conditional_imports".to_string()])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    // Should include conditional_imports, base_module, and another_module
    insta::assert_snapshot!(output);
}

#[test]
fn test_full_graph_with_nested_imports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("nested_imports_project");

    let graph =
        python::analyze_project(&root, None, &[]).expect("Failed to analyze nested imports project");
    let dot_output = graph.to_dot(false);

    // Should show all dependencies including those from nested imports
    insta::assert_snapshot!(dot_output);
}

// CLI integration tests for file path support

fn get_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("deptree-utils");
    path
}

#[test]
fn test_upstream_cli_with_script_file_path() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let script_path = project_root.join("scripts").join("blah.py");

    let output = Command::new(get_binary_path())
        .arg("python-upstream")
        .arg(&project_root)
        .arg("--upstream")
        .arg(script_path.to_str().expect("Invalid path"))
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!(stdout);
}

#[test]
fn test_upstream_cli_with_internal_module_file_path() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let module_path = project_root.join("src").join("foo").join("bar.py");

    let output = Command::new(get_binary_path())
        .arg("python-upstream")
        .arg(&project_root)
        .arg("--upstream")
        .arg(module_path.to_str().expect("Invalid path"))
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!(stdout);
}

#[test]
fn test_upstream_cli_with_relative_file_path() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    // Change to project directory and use relative path
    let output = Command::new(get_binary_path())
        .current_dir(&project_root)
        .arg("python-upstream")
        .arg(".")
        .arg("--upstream")
        .arg("scripts/blah.py")
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!(stdout);
}

#[test]
fn test_upstream_cli_with_mixed_inputs() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let script_path = project_root.join("scripts").join("blah.py");

    // Mix file path and dotted name
    let output = Command::new(get_binary_path())
        .arg("python-upstream")
        .arg(&project_root)
        .arg("--upstream-module")
        .arg(script_path.to_str().expect("Invalid path"))
        .arg("--upstream-module")
        .arg("foo.bar")
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!(stdout);
}

#[test]
fn test_upstream_cli_with_nonexistent_file() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project_with_scripts");

    let nonexistent_path = project_root.join("scripts").join("nonexistent.py");

    let output = Command::new(get_binary_path())
        .arg("python-upstream")
        .arg(&project_root)
        .arg("--upstream")
        .arg(nonexistent_path.to_str().expect("Invalid path"))
        .output()
        .expect("Failed to execute command");

    // Should fail with error message
    assert!(!output.status.success(), "Command should have failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist"),
        "Error message should mention file doesn't exist"
    );
}

// Tests for new features: graph output for downstream and max-rank filtering

#[test]
fn test_downstream_graph_dot_format() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find downstream dependencies of pkg_b.module_b in DOT format
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_graph_mermaid_format() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find downstream dependencies of pkg_b.module_b in Mermaid format
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_mermaid_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_max_rank_0() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Only the root module itself (distance 0)
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, Some(0));
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_max_rank_1() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Root + direct dependents (pkg_b.module_b + pkg_a.module_a + main)
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, Some(1));
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_max_rank_2() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // All transitive dependents (should be same as unlimited for this fixture)
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, Some(2));
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_max_rank_unlimited() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // No limit - all transitive dependents
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, None);
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_max_rank_with_mermaid_format() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Verify max-rank works with Mermaid format
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, Some(1));
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_mermaid_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_list_format() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Find upstream dependencies of main in list format
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let upstream = graph.find_upstream(&roots, None);
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_max_rank_0() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Only the root module itself
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let upstream = graph.find_upstream(&roots, Some(0));
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_max_rank_1() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Root + direct dependencies (main + pkg_a + pkg_b.module_b)
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let upstream = graph.find_upstream(&roots, Some(1));
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_max_rank_2() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // All transitive dependencies (should be same as max_rank_1 for main since max depth is 1)
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let upstream = graph.find_upstream(&roots, Some(2));
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_upstream_max_rank_with_dot_format() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Verify max-rank works with DOT format
    let roots = vec![python::ModulePath(vec!["main".to_string()])];
    let upstream = graph.find_upstream(&roots, Some(1));
    let filter: std::collections::HashSet<_> = upstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    insta::assert_snapshot!(output);
}

#[test]
fn test_downstream_multiple_roots_max_rank() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // Test with multiple roots and max-rank - distances should merge correctly
    // pkg_a has distance 0, pkg_b.module_b has distance 0
    // main has distance 1 from both
    let roots = vec![
        python::ModulePath(vec!["pkg_a".to_string()]),
        python::ModulePath(vec!["pkg_b".to_string(), "module_b".to_string()]),
    ];
    let downstream = graph.find_downstream(&roots, Some(1));
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_list_filtered(&filter);

    insta::assert_snapshot!(output);
}

#[test]
fn test_orphan_filtering_with_max_rank() {
    let root = fixture_path();
    let graph = python::analyze_project(&root, None, &[]).expect("Failed to analyze project");

    // With max_rank=0, we only get pkg_b.module_b
    // With include_orphans=false, orphans should still be filtered in graph output
    let roots = vec![python::ModulePath(vec![
        "pkg_b".to_string(),
        "module_b".to_string(),
    ])];
    let downstream = graph.find_downstream(&roots, Some(0));
    let filter: std::collections::HashSet<_> = downstream.keys().cloned().collect();
    let output = graph.to_dot_filtered(&filter, false);

    // Should have nodes but pkg_b.module_b itself might be considered an orphan
    // in the filtered graph (no edges within the filter)
    insta::assert_snapshot!(output);
}
