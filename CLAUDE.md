# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with 
code in this repository.

## Style

Try to keep the style as functional as possible ("Ocaml with manual garbage 
collection", as opposed to "C++ with borrow checker"). Use features like 
Algebraic Data Types and Traits liberally, with an algebra-oriented design 
mindset

When writing new documentation files, ensure to clarify that "Documentation written 
by Claude Code" somewhere in the file.

ALL tests should be in the `tests/` directory, and should follow the snapshot 
testing instructions in the `## Testing` section.

This project is in heavy development. Whenever you make a change, make sure to 
check `CLAUDE.md` and update it if necessary to reflect any newly added/changed 
features or structures

## Error Handling & Safety Guidelines

### Never Use `unwrap()` in Production Code
- **NEVER** use `.unwrap()` on `Option` or `Result` types in production paths
- Use proper error handling with `?`, `.ok_or()`, `.map_err()`, or pattern matching
- Example: Replace `tag_name.chars().nth(1).unwrap()` with proper error handling
- Exception: Only use `unwrap()` in tests or when preceded by explicit checks that guarantee safety

### Error Message Quality
- Include contextual information in error messages
- Use structured error types instead of plain strings where possible
- Provide actionable information for debugging

## Features

### Python Dependency Analysis
Analyzes Python projects to extract internal module dependencies.

#### Basic Usage - DOT Graph Output
Outputs a Graphviz DOT graph showing all internal dependencies:

```bash
deptree-utils python <path-to-python-project>
```

The analyzer:
- Parses Python files using `ruff_python_parser`
- Extracts `import` and `from ... import` statements
- Resolves relative imports based on module location
- Only includes internal dependencies (modules within the project)
- Outputs a deterministic DOT format graph

#### Downstream Dependency Analysis
Find all modules that depend on a given set of modules (downstream dependencies). The output includes the specified modules and all modules that transitively depend on them, as a sorted, newline-separated list.

**Via comma-separated list:**
```bash
deptree-utils python <path> --downstream pkg_a.module_a,pkg_b.module_b
```

**Via repeated flags:**
```bash
deptree-utils python <path> --downstream-module pkg_a.module_a --downstream-module pkg_b.module_b
```

**Via file input:**
```bash
# Create a file with module names (one per line)
echo "pkg_a.module_a" > modules.txt
echo "pkg_b.module_b" >> modules.txt

deptree-utils python <path> --downstream-file modules.txt
```

**Combined usage:**
All three input methods can be combined in a single command. The module lists will be merged.

## Development Environment

This project uses Nix for reproducible builds and development environments. The
`flake.nix` provides all necessary dependencies. You are always running in the relevant nix environment.

## Testing

The project uses **snapshot testing** via the `insta` crate for all test assertions. This testing paradigm provides deterministic, maintainable tests that capture expected behavior through literal value snapshots.

### Snapshot Testing Approach

All tests follow these principles:
- **Single assertion per test**: Each test has exactly one `insta::assert_snapshot!()` or `insta::assert_json_snapshot!()` call
- **Deterministic snapshots**: Dynamic data (timestamps, file sizes, temp paths) is normalized to ensure reproducible results
- **Literal value snapshots**: Snapshots contain only concrete, expected values without variables
- **Offline resilience**: All tests must pass in offline environments (CI, restricted networks) by using dual-snapshot patterns or graceful degradation

 in `tests/golden_output/`

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test <test_name>

# Review and accept snapshot changes
cargo insta review

# Auto-accept all snapshot changes (use carefully)
cargo insta accept
```

### Snapshot Management

- Snapshots are stored in `src/snapshots/` (unit tests) and `tests/snapshots/` (integration tests)
- When test behavior changes, run `cargo insta review` to inspect differences
- Accept valid changes with `cargo insta accept` or reject with `cargo insta reject`
- Never commit `.snap.new` files - these are pending snapshot updates

## Version control

This project uses jujutsu `jj` for version control
