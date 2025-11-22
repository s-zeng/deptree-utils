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

#### Basic Usage - Graph Output
Outputs a dependency graph showing all internal dependencies. By default, outputs in Graphviz DOT format:

```bash
deptree-utils python <path-to-python-project>
```

The analyzer:
- Parses Python files using `ruff_python_parser`
- Extracts `import` and `from ... import` statements
- Resolves relative imports based on module location
- Only includes internal dependencies (modules within the project)
- Outputs a deterministic graph (DOT or Mermaid format)
- **By default, filters out orphan nodes** (modules with no dependencies and no dependents)

#### Output Format Selection

You can choose between Graphviz DOT, Mermaid flowchart, and plain list formats using the `--format` flag:

```bash
# DOT format (default) - for use with Graphviz
deptree-utils python ./my-project --format dot

# Mermaid format - for use in Markdown, GitHub, documentation
deptree-utils python ./my-project --format mermaid

# List format - for downstream/upstream analysis only
deptree-utils python ./my-project --downstream pkg_a --format list
```

**DOT format:**
- Traditional graph visualization format
- Requires Graphviz for rendering
- Example: `deptree-utils python ./project | dot -Tpng > graph.png`

**Mermaid format:**
- Modern flowchart syntax (`flowchart TD`)
- Renders natively in GitHub, GitLab, and many documentation tools
- Scripts shown as rectangles `[script]`, modules as rounded rectangles `(module)`
- Can be embedded directly in Markdown files

**List format:**
- Sorted, newline-separated list of module names
- Only available with `--downstream` or `python-upstream` commands
- Useful for scripting and programmatic processing

All graph formats (DOT and Mermaid):
- Support the `--include-orphans` flag
- Work with upstream (`python-upstream`) and downstream analysis
- Provide deterministic, sorted output for version control
- Support `--max-rank` for distance filtering

**Orphan Node Filtering:**

By default, graph output (both DOT and Mermaid) excludes orphan nodes (modules that have no incoming or outgoing edges). This keeps the graph focused on modules that are part of the dependency structure.

To include orphan nodes in the output, use the `--include-orphans` flag:

```bash
# Include orphan nodes in the output (works with both formats)
deptree-utils python ./my-project --include-orphans
deptree-utils python ./my-project --format mermaid --include-orphans
```

Orphan nodes are typically:
- Standalone modules that don't import anything and aren't imported by anything
- Dead code that's not connected to the rest of the project
- New modules that haven't been integrated yet

This flag is available for all analysis modes (full graph, downstream, and upstream), and works with both DOT and Mermaid output formats.

#### Source Root Detection
The analyzer automatically detects the Python source root to correctly handle projects with different layouts.

**Supported Layouts:**

1. **Flat layout** (packages at project root):
```
project/
├── pkg_a/
└── pkg_b/
```

2. **src/ layout** (modern Python best practice):
```
project/
├── src/
│   ├── pkg_a/
│   └── pkg_b/
└── pyproject.toml
```

3. **lib/python/ layout** (common in monorepos):
```
project/
└── lib/
    └── python/
        ├── pkg_a/
        └── pkg_b/
```

**Auto-Detection Process:**
1. Parse `pyproject.toml` for `[tool.setuptools.packages.find] where = ["..."]` configuration
2. Check for `src/` directory with Python packages
3. Check for `lib/python/` directory with Python packages
4. Fall back to project root (flat layout)

**Explicit Source Root Override:**
You can explicitly specify the source root using the `--source-root` (or `-s`) flag:

```bash
# Explicitly specify source root
deptree-utils python ./my-project --source-root ./my-project/src

# Or use short form
deptree-utils python ./my-project -s ./my-project/src
```

This is useful when:
- Auto-detection fails or picks the wrong directory
- You want to analyze a specific subdirectory
- The project has an unusual structure

#### Downstream Dependency Analysis
Find all modules that depend on a given set of modules (downstream dependencies). **By default, outputs a dependency graph** (DOT or Mermaid format) showing only the specified modules and all modules that transitively depend on them.

**Basic usage via comma-separated list:**
```bash
# Default: outputs DOT graph format
deptree-utils python <path> --downstream pkg_a.module_a,pkg_b.module_b

# Output in Mermaid format
deptree-utils python <path> --downstream pkg_a.module_a,pkg_b.module_b --format mermaid

# Output as a sorted, newline-separated list
deptree-utils python <path> --downstream pkg_a.module_a,pkg_b.module_b --format list
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

**Output formats:**
- `--format dot` (default): Graphviz DOT format showing the downstream dependency graph
- `--format mermaid`: Mermaid flowchart format for the downstream dependency graph
- `--format list`: Sorted, newline-separated list of module names

**File path support:**
Instead of using dotted module names, you can directly specify file paths to Python files:

```bash
# Using a script file path
deptree-utils python ./my-project --downstream scripts/my_script.py

# Using an internal module file path
deptree-utils python ./my-project --downstream src/pkg_a/module_a.py

# Using relative paths (when running from project directory)
cd my-project
deptree-utils python . --downstream bin/my_script.py

# Mix file paths and dotted names
deptree-utils python ./my-project \
  --downstream-module scripts/runner.py \
  --downstream-module pkg_a.module_a
```

File paths can be:
- Absolute or relative paths
- Paths to scripts outside the source root (e.g., `scripts/`, `bin/`)
- Paths to internal modules inside the source root (e.g., `src/pkg_a/module_a.py`)
- Mixed with dotted module names in the same command

**Limit by distance (max-rank):**
You can limit the output to only include nodes within a specific distance from the specified modules using `--max-rank`:

```bash
# Include only direct dependents (distance 1)
deptree-utils python <path> --downstream pkg_a.module_a --max-rank 1

# Include modules up to 2 edges away
deptree-utils python <path> --downstream pkg_a.module_a --max-rank 2 --format mermaid

# Works with list format too
deptree-utils python <path> --downstream pkg_a.module_a --max-rank 1 --format list
```

Distance is measured as the minimum number of dependency edges from any of the specified modules. For example:
- Distance 0: The specified modules themselves
- Distance 1: Modules that directly depend on the specified modules
- Distance 2: Modules that depend on modules at distance 1
- And so on...

#### Upstream Dependency Analysis
Find all modules that a given set of modules depends on (upstream dependencies). **By default, outputs a dependency graph** (DOT or Mermaid format) showing only the specified modules and all modules they transitively depend on (the upstream dependency tree).

**Basic usage via comma-separated list:**
```bash
# Default: outputs DOT graph format
deptree-utils python <path> --upstream pkg_a.module_a,pkg_b.module_b

# Output in Mermaid format
deptree-utils python <path> --upstream pkg_a.module_a,pkg_b.module_b --format mermaid

# Output as a sorted, newline-separated list
deptree-utils python <path> --upstream pkg_a.module_a,pkg_b.module_b --format list
```

**Via repeated flags:**
```bash
deptree-utils python <path> --upstream-module pkg_a.module_a --upstream-module pkg_b.module_b
```

**Via file input:**
```bash
# Create a file with module names (one per line)
echo "pkg_a.module_a" > modules.txt
echo "pkg_b.module_b" >> modules.txt

deptree-utils python <path> --upstream-file modules.txt
```

**Combined usage:**
All three input methods can be combined in a single command. The module lists will be merged.

**File path support:**
Instead of using dotted module names, you can directly specify file paths to Python files:

```bash
# Using a script file path
deptree-utils python ./my-project --upstream scripts/my_script.py

# Using an internal module file path
deptree-utils python ./my-project --upstream src/pkg_a/module_a.py

# Using relative paths (when running from project directory)
cd my-project
deptree-utils python . --upstream bin/my_script.py

# Mix file paths and dotted names
deptree-utils python ./my-project \
  --upstream-module scripts/runner.py \
  --upstream-module pkg_a.module_a
```

File paths can be:
- Absolute or relative paths
- Paths to scripts outside the source root (e.g., `scripts/`, `bin/`)
- Paths to internal modules inside the source root (e.g., `src/pkg_a/module_a.py`)
- Mixed with dotted module names in the same command

**Output formats:**
- `--format dot` (default): Graphviz DOT format showing the upstream dependency graph
- `--format mermaid`: Mermaid flowchart format for the upstream dependency graph
- `--format list`: Sorted, newline-separated list of module names

The command outputs a dependency graph (or list) showing only the upstream dependency subgraph. This includes:
- The specified module(s)
- All modules they depend on (directly or transitively)
- Only edges between modules in this set (for graph formats)
- Visual distinction for scripts vs. internal modules (box/rectangle vs. ellipse/rounded rectangle in graphs)

**Limit by distance (max-rank):**
You can limit the output to only include nodes within a specific distance from the specified modules using `--max-rank`:

```bash
# Include only direct dependencies (distance 1)
deptree-utils python <path> --upstream main --max-rank 1

# Include modules up to 2 edges away
deptree-utils python <path> --upstream main --max-rank 2 --format mermaid

# Works with list format too
deptree-utils python <path> --upstream main --max-rank 1 --format list
```

Distance is measured as the minimum number of dependency edges from any of the specified modules. For example:
- Distance 0: The specified modules themselves
- Distance 1: Modules that the specified modules directly depend on
- Distance 2: Modules that distance-1 modules depend on
- And so on...

**Examples:**
```bash
# Find everything that main.py depends on (default DOT format)
deptree-utils python ./my-project --upstream main

# Output in Mermaid format
deptree-utils python ./my-project --upstream main --format mermaid

# Output as list format
deptree-utils python ./my-project --upstream main --format list

# Visualize DOT output with Graphviz
deptree-utils python ./my-project --upstream main --format dot | dot -Tpng > deps.png

# Embed Mermaid output in documentation
deptree-utils python ./my-project --upstream main --format mermaid > docs/dependencies.mmd
```

**Use cases:**
- Understanding what would need to be tested when modifying a dependency
- Identifying the minimal set of modules needed to run a specific module
- Analyzing the complexity of a module by counting its transitive dependencies
- Finding circular dependencies in a specific part of the codebase

**With explicit source root:**
```bash
deptree-utils python ./my-project --source-root ./my-project/src --upstream main
```

**Script exclusion:**
The `--exclude-scripts` flag works the same way for all analysis modes:

```bash
deptree-utils python ./my-project --upstream main --exclude-scripts "old_scripts"
```

**Combined downstream and upstream analysis:**
You can use both `--downstream` and `--upstream` flags together to find the intersection of modules:

```bash
# Find modules that are both downstream of module_a AND upstream of main
# (i.e., modules in the path from module_a to main)
deptree-utils python ./my-project \
  --downstream pkg_a.module_a \
  --upstream main

# This finds modules that:
# 1. Depend on pkg_a.module_a (downstream), AND
# 2. Are depended on by main (upstream)
```

This is useful for:
- Finding the dependency path between two modules
- Analyzing the impact zone of a change that affects a specific module
- Understanding which modules connect two different parts of the codebase

#### Script Discovery Outside Source Root
The analyzer automatically discovers and includes Python scripts outside the source root (e.g., `scripts/`, `tools/`) in dependency analysis. Scripts are treated as first-class citizens in the dependency graph and can import internal modules.

**How It Works:**

When analyzing a project, the tool:
1. Analyzes all modules within the source root (as normal)
2. Discovers Python files outside the source root
3. Applies default exclusions to skip common directories
4. Processes scripts separately with special import resolution rules

**Default Exclusions:**

The following directories are automatically excluded from script discovery:
- `venv/`, `.venv/`, and any `venv*` directories
- `__pycache__/`, `.pytest_cache/`, `.mypy_cache/`, `.tox/`
- `.git/`, `.egg-info/`, `*.egg/`, `eggs/`
- `build/`, `dist/`, `node_modules/`

**Example Project Structure:**
```
project/
├── src/
│   └── foo/
│       └── bar.py          # Internal module
├── scripts/
│   ├── blah.py             # Script importing foo.bar
│   └── utils/
│       └── helper.py       # Helper script
└── pyproject.toml
```

In `scripts/blah.py`:
```python
from foo.bar import something  # Imports internal module
```

**Script Naming Convention:**

Scripts are named using their path relative to the project root:
- `scripts/blah.py` → `scripts.blah`
- `tools/utils/helper.py` → `tools.utils.helper`

**Visual Distinction in Graph Output:**

Scripts are visually distinguished in the dependency graph:

DOT format:
- Internal modules: shown as ellipses (default DOT shape)
- Scripts: shown as boxes (`[shape=box]`)

Mermaid format:
- Internal modules: shown as rounded rectangles `(module.name)`
- Scripts: shown as rectangles `[script.name]`

**Custom Exclusion Patterns:**

You can exclude additional paths from script discovery using the `--exclude-scripts` flag:

```bash
# Exclude a specific directory
deptree-utils python ./my-project --exclude-scripts "old_scripts"

# Exclude multiple patterns
deptree-utils python ./my-project \
  --exclude-scripts "old_scripts" \
  --exclude-scripts "experimental"

# Use wildcards
deptree-utils python ./my-project --exclude-scripts "*backup*"
```

**Import Resolution for Scripts:**

Scripts use special import resolution rules:
- **Absolute imports** (e.g., `from foo.bar import x`) resolve against the source root
- **Relative imports** (e.g., `from .utils import helper`) resolve against the script's location
- Scripts can import both internal modules and other scripts

**Downstream Analysis with Scripts:**

Scripts are included in downstream dependency analysis. If a script imports an internal module, modifying that module will show the script as a downstream dependent:

```bash
# Find all code (modules and scripts) that depend on foo.bar
deptree-utils python ./my-project --downstream foo.bar
```

Output might include:
```
foo.bar
scripts.blah
scripts.runner
```

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
