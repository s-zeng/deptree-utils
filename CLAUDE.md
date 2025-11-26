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

## Build System

This project uses `just` (justfile) for build automation. The build pipeline has three main components:

1. **WASM Build** (`just wasm-build`):
   - Compiles Rust code in `crates/deptree-wasm/` to WebAssembly using `wasm-pack`
   - **Automatically copies** the built WASM files from `crates/deptree-wasm/pkg/` to `frontend/src/wasm/`
   - This copy step is critical - the frontend imports from `frontend/src/wasm/`, not from the pkg directory

2. **Frontend Build** (`just frontend-build`):
   - Runs `wasm-build` first to ensure WASM is up-to-date
   - Bundles the TypeScript frontend with Vite (includes the WASM from `frontend/src/wasm/`)
   - Copies the final `frontend/dist/index.html` to `crates/deptree-cli/templates/cytoscape.html`

3. **CLI Build** (`just cli-build` or `just cli-build-release`):
   - Compiles the Rust CLI binary
   - Embeds the template from `crates/deptree-cli/templates/cytoscape.html` at compile time

**Important**: Always use `just wasm-build` or `just frontend-build` instead of running `wasm-pack` directly. The justfile ensures WASM files are copied to the correct location for frontend consumption.

**Full build**: `just build` runs the complete pipeline (WASM → Frontend → CLI)

**Cleaning**:
- `just clean` - removes all build artifacts
- `just clean-wasm` - removes only `crates/deptree-wasm/pkg/`
- `just clean-frontend` - removes `frontend/dist/`, `frontend/src/wasm/`, and the CLI template

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

You can choose between Graphviz DOT, Mermaid flowchart, Cytoscape HTML, and plain list formats using the `--format` flag:

```bash
# DOT format (default) - for use with Graphviz
deptree-utils python ./my-project --format dot

# Mermaid format - for use in Markdown, GitHub, documentation
deptree-utils python ./my-project --format mermaid

# Cytoscape format - interactive HTML visualization
deptree-utils python ./my-project --format cytoscape > graph.html

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

**Cytoscape format:**
- Outputs a **self-contained HTML file** with interactive dependency graph visualization
- No external tools required to view (opens directly in any web browser)
- **Basic interactive features:**
  - Pan, zoom, node selection
  - Export to PNG
  - Automatic hierarchical layout using Dagre algorithm (left-to-right flow)
- **Interactive filtering panel** (collapsible sidebar):
  - **Display Options:**
    - Toggle orphan nodes visibility
    - Toggle namespace package visibility
    - Show only highlighted nodes (when using --show-all mode)
  - **Distance filtering:**
    - Slider to limit graph by distance from selected modules (0-10+ hops)
    - Real-time preview of distance limits
  - **Upstream/Downstream dependencies:**
    - Interactively select modules to show upstream dependencies
    - Select modules to show downstream dependencies
    - Add modules via button prompt or right-click context menu
    - Remove modules with chip-based UI
  - **Script exclusion:**
    - Text input with wildcard pattern support (*prefix, suffix*, *substring*)
    - Filter out scripts matching patterns
  - **Reset button:** Restore original CLI-specified view
  - **Apply button:** Execute filters with animated layout transition
- **Context menu** (right-click on nodes):
  - Add node to upstream dependencies
  - Add node to downstream dependencies
  - Remove node from upstream/downstream
- **Visual styling:**
  - **Modules**: Blue ellipses
  - **Scripts**: Green rectangles
  - **Namespace packages**: Orange hexagons (dashed border)
  - **Highlighted nodes**: Light blue with thick border (for --show-all mode)
- **Example:**
  ```bash
  deptree-utils python ./my-project --format cytoscape > graph.html
  # Open graph.html in browser to use interactive features
  ```
- **Use cases:**
  - Sharing visualizations with non-technical stakeholders
  - Interactive exploration of large codebases without regenerating
  - Presentations and documentation (no Graphviz or rendering tools needed)
  - Quick visual analysis and filtering without CLI commands
  - Experimenting with different filter combinations in real-time

**Configurable Layouts:**
- Interactive layout algorithm selection with 9 available layouts:
  - Built-in: dagre, cose, breadthfirst, circle, grid, concentric
  - Extensions: cose-bilkent, cola, elk
- Per-layout configurable settings with key settings and advanced options
- Manual "Apply Layout" button for user control
- Layout choice persists during session (resets on page reload)
- Example workflow:
  1. Select layout algorithm from dropdown
  2. Adjust key settings (always visible)
  3. Expand "Advanced" for fine-tuning
  4. Click "Apply Layout" to re-render with new settings

All graph formats (DOT, Mermaid, and Cytoscape):
- Support the `--include-orphans` flag
- Work with upstream (`python-upstream`) and downstream analysis
- Provide deterministic, sorted output for version control
- Support `--max-rank` for distance filtering

**Orphan Node Filtering:**

By default, graph output (DOT, Mermaid, and Cytoscape) excludes orphan nodes (modules that have no incoming or outgoing edges). This keeps the graph focused on modules that are part of the dependency structure.

To include orphan nodes in the output, use the `--include-orphans` flag:

```bash
# Include orphan nodes in the output (works with all graph formats)
deptree-utils python ./my-project --include-orphans
deptree-utils python ./my-project --format mermaid --include-orphans
deptree-utils python ./my-project --format cytoscape --include-orphans > graph.html
```

Orphan nodes are typically:
- Standalone modules that don't import anything and aren't imported by anything
- Dead code that's not connected to the rest of the project
- New modules that haven't been integrated yet

This flag is available for all analysis modes (full graph, downstream, and upstream), and works with all graph output formats (DOT, Mermaid, and Cytoscape).

#### Namespace Package Filtering

By default, namespace packages are **excluded** from the dependency graph output. This applies to both:
- **Native namespace packages (PEP 420)**: Directories without `__init__.py` that contain Python modules
- **Legacy namespace packages**: Packages with `__init__.py` containing `pkgutil.extend_path()` or `pkg_resources.declare_namespace()`

When namespace packages are excluded, **transitive edges are preserved**. For example, if module A depends on namespace package N, which contains module B, the output will show a direct edge from A to B.

**To include namespace packages in the output, use the `--include-namespace-packages` flag:**

```bash
# Include namespace packages in the output
deptree-utils python ./my-project --include-namespace-packages

# Works with all output formats
deptree-utils python ./my-project --format mermaid --include-namespace-packages
```

**Visual distinction:**

When namespace packages are included in the output, they are visually distinguished:
- **DOT format**: Hexagon shape with dashed style (`[shape=hexagon, style=dashed]`)
- **Mermaid format**: Hexagon shape (`{{{{ }}}}}`)

**Why exclude namespace packages by default?**

Namespace packages are typically structural/organizational constructs rather than functional modules. Excluding them:
- Simplifies the dependency graph by focusing on actual code modules
- Reduces noise in large projects with many namespace packages
- Makes it easier to understand the true dependencies between functional components
- Preserves transitive relationships so no dependency information is lost

**Use cases for including namespace packages:**
- Analyzing the complete package structure including organizational constructs
- Debugging namespace package issues
- Understanding how namespace packages are used in the project
- Creating comprehensive documentation that shows all package levels

This flag is available for all analysis modes (full graph, downstream, and upstream), and works with all graph output formats (DOT, Mermaid, and Cytoscape).

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

**Show full graph with highlighting:**
By default, `--downstream` and `--upstream` filter the output to show only the relevant subgraph. Use `--show-all` to show the **full dependency graph** while visually highlighting the filtered modules:

```bash
# Show full graph with downstream modules highlighted in light blue
deptree-utils python <path> --downstream pkg_a.module_a --show-all

# Show full graph with upstream modules highlighted in Mermaid format
deptree-utils python <path> --upstream main --show-all --format mermaid

# Works with max-rank too
deptree-utils python <path> --downstream pkg_a.module_a --show-all --max-rank 2
```

**Visual styling:**
- **DOT format**: Highlighted nodes have light blue background (`fillcolor=lightblue, style=filled`)
- **Mermaid format**: Highlighted nodes have blue styling (`fill:#bbdefb,stroke:#1976d2,stroke-width:2px`)
- Scripts maintain their distinct shape (box/rectangle) even when highlighted

**Restrictions:**
- `--show-all` requires either `--downstream` or `--upstream` to be specified
- `--show-all` cannot be used with `--format list` (list format only makes sense for filtered output)

**Use cases:**
- Understanding the context of a module within the entire codebase
- Visualizing the scope of impact while seeing the full architecture
- Identifying where filtered modules fit in the overall dependency structure
- Creating documentation that shows both the full graph and areas of interest

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
