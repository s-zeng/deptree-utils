# Default: list all available commands
default:
    @just --list

# === Frontend Commands ===

# Install frontend dependencies
frontend-install:
    cd frontend && bun install

# Build WASM module
wasm-build:
    cd crates/deptree-wasm && wasm-pack build --target web

# Build frontend (Vite bundle)
frontend-build-only:
    cd frontend && bun run build

# Full frontend build pipeline (WASM → Frontend → Copy template)
frontend-build: wasm-build frontend-build-only
    mkdir -p crates/deptree-cli/templates
    cp frontend/dist/index.html crates/deptree-cli/templates/cytoscape.html
    @echo "✓ Frontend built and template copied"

# Run frontend dev server
frontend-dev:
    cd frontend && bun run dev

# Run frontend tests
frontend-test:
    cd frontend && bun test

# Run frontend tests in watch mode
frontend-test-watch:
    cd frontend && bun test:watch

# Run frontend tests with UI
frontend-test-ui:
    cd frontend && bun test:ui

# === CLI Commands ===

# Build CLI (requires frontend to be built first)
cli-build:
    cargo build

# Build CLI in release mode
cli-build-release:
    cargo build --release

# Run CLI tests (Rust snapshot tests)
cli-test:
    cargo test

# Run CLI tests and review snapshot changes
cli-test-review:
    cargo insta test --review

# Run CLI tests and auto-accept snapshot changes
cli-test-accept:
    cargo insta test --accept

# Run CLI with arguments
cli-run *ARGS:
    cargo run {{ARGS}}

# === Unified Commands ===

# Build everything (frontend + CLI)
build: frontend-build cli-build
    @echo "✓ Full build complete"

# Build everything in release mode
build-release: frontend-build cli-build-release
    @echo "✓ Release build complete"

# Run all tests (frontend + CLI)
test: frontend-test cli-test
    @echo "✓ All tests passed"

# Clean all build artifacts
clean: clean-frontend clean-cli
    @echo "✓ All build artifacts removed"

# Clean frontend build artifacts
clean-frontend:
    rm -rf frontend/dist
    rm -rf frontend/node_modules
    rm -f crates/deptree-cli/templates/cytoscape.html
    @echo "✓ Frontend cleaned"

# Clean CLI build artifacts
clean-cli:
    cargo clean
    @echo "✓ CLI cleaned"

# Clean WASM artifacts
clean-wasm:
    rm -rf crates/deptree-wasm/pkg
    rm -rf frontend/src/wasm
    @echo "✓ WASM artifacts cleaned"

# === Development Workflow Commands ===

# Full development setup (install deps, build everything)
dev-setup: frontend-install build
    @echo "✓ Development environment ready"

# Watch CLI for changes and auto-rebuild
watch *ARGS:
    bacon --job run -- -- {{ARGS}}

# Format all code (Rust + Nix)
format:
    treefmt

# Quick check before committing
check: format test
    @echo "✓ Pre-commit checks passed"

# === Convenience Aliases ===

# Alias for cli-run
run *ARGS: cli-run
