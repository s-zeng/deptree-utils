#!/usr/bin/env bash
set -euo pipefail

# Build script for the frontend bundle
# This script:
# 1. Builds the WASM module using wasm-pack
# 2. Copies WASM files to frontend/src/wasm/
# 3. Builds the frontend using Vite into a single HTML file
# 4. Places the output where build.rs can find it

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "==> Building WASM module..."
cd "$PROJECT_ROOT/crates/deptree-wasm"
wasm-pack build --target web --out-dir ../../frontend/src/wasm

echo "==> Building frontend with Vite..."
cd "$PROJECT_ROOT/frontend"
bun run build

echo "==> Moving built HTML to crates/deptree-cli/..."
mkdir -p "$PROJECT_ROOT/crates/deptree-cli/templates"
cp "$PROJECT_ROOT/frontend/dist/index.html" "$PROJECT_ROOT/crates/deptree-cli/templates/cytoscape.html"

echo "==> Frontend build complete!"
echo "    Template: crates/deptree-cli/templates/cytoscape.html"
