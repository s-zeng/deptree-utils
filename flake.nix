{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";

    # Dev tools
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        inputs.treefmt-nix.flakeModule
      ];
      perSystem = { config, self', pkgs, lib, system, ... }:
        let
          cargoToml = builtins.fromTOML (builtins.readFile ./crates/deptree-cli/Cargo.toml);
          nonRustDeps = [
            pkgs.libiconv
            pkgs.pkg-config
            pkgs.openssl
            pkgs.pandoc
            pkgs.texlive.combined.scheme-small
          ];
          frontendDeps = [
            pkgs.bun
            pkgs.wasm-pack
            pkgs.binaryen
            pkgs.llvmPackages.lld
          ];

          # Build wasm-bindgen-cli at the exact version needed (0.2.105)
          wasm-bindgen-cli = pkgs.rustPlatform.buildRustPackage rec {
            pname = "wasm-bindgen-cli";
            version = "0.2.105";

            src = pkgs.fetchCrate {
              inherit pname version;
              sha256 = "sha256-zLPFFgnqAWq5R2KkaTGAYqVQswfBEYm9x3OPjx8DJRY=";
            };

            cargoHash = "sha256-a2X9bzwnMWNt0fTf30qAiJ4noal/ET1jEtf5fBFj5OU=";

            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ] ++ lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
            ];

            doCheck = false;
          };

          # Stage 1: Build WASM module
          wasmBuild = pkgs.rustPlatform.buildRustPackage {
            pname = "deptree-wasm";
            version = cargoToml.package.version;
            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
              outputHashes = {
                "ruff_python_ast-0.0.0" = "sha256-Nws1CDywLEp6ffK4gQQMfcMl3TPIYHVYe1HI1TWCU1Q=";
              };
            };

            buildAndTestSubdir = "crates/deptree-wasm";

            nativeBuildInputs = [
              pkgs.binaryen
              wasm-bindgen-cli
              pkgs.llvmPackages.lld
            ];

            buildPhase = ''
              cd crates/deptree-wasm

              # Build for wasm32-unknown-unknown target
              cargo build --lib --release --target wasm32-unknown-unknown

              # Run wasm-bindgen to generate JS bindings
              mkdir -p $out/pkg
              wasm-bindgen \
                --target web \
                --out-dir $out/pkg \
                --out-name deptree_wasm \
                ../../target/wasm32-unknown-unknown/release/deptree_wasm.wasm

              # Optimize with wasm-opt
              wasm-opt -O3 $out/pkg/deptree_wasm_bg.wasm -o $out/pkg/deptree_wasm_bg.wasm

              # Copy package.json and other metadata
              cp package.json $out/pkg/ || echo "No package.json to copy"
            '';

            installPhase = ''
              echo "WASM build complete"
            '';

            doCheck = false;
          };

          # Stage 2: Build frontend with the WASM module
          frontendBuild = pkgs.stdenv.mkDerivation {
            pname = "deptree-frontend";
            version = cargoToml.package.version;
            src = ./.;

            nativeBuildInputs = [ pkgs.bun ];

            buildPhase = ''
              # Copy WASM build output to frontend
              mkdir -p frontend/src/wasm
              cp -r ${wasmBuild}/pkg/* frontend/src/wasm/

              # Install frontend dependencies and build
              cd frontend
              export HOME=$TMPDIR
              bun install --frozen-lockfile
              bun run build
            '';

            installPhase = ''
              mkdir -p $out
              cp dist/index.html $out/cytoscape.html
            '';
          };

        in
        {
          # Expose intermediate build stages as packages
          packages.wasm = wasmBuild;
          packages.frontend = frontendBuild;

          # Stage 3: Build CLI with embedded frontend template
          packages.default = pkgs.rustPlatform.buildRustPackage {
            inherit (cargoToml.package) name version;
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
              outputHashes = {
                "ruff_python_ast-0.0.0" = "sha256-Nws1CDywLEp6ffK4gQQMfcMl3TPIYHVYe1HI1TWCU1Q=";
              };
            };
            nativeBuildInputs = nonRustDeps;
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            LIBRARY_PATH = "${pkgs.libiconv}/lib";

            # Inject the frontend template before building
            preBuild = ''
              mkdir -p crates/deptree-cli/templates
              cp ${frontendBuild}/cytoscape.html crates/deptree-cli/templates/cytoscape.html
            '';

            # Skip tests in nix build
            doCheck = false;
          };

          # Rust dev environment
          devShells.default = pkgs.mkShell {
            inputsFrom = [
              config.treefmt.build.devShell
            ];
            shellHook = ''
              # For rust-analyzer 'hover' tooltips to work.
              export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
              # openssl
              export OPENSSL_DIR="${pkgs.openssl.dev}"
              export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
              export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"
              export OPENSSL_DEV="${pkgs.openssl.dev}"
              # libiconv
              export LIBRARY_PATH="${pkgs.libiconv}/lib:$LIBRARY_PATH"
            '';
            buildInputs = nonRustDeps;
            nativeBuildInputs = with pkgs; [
              just
              rustc
              cargo
              clippy
              cargo-watch
              cargo-insta
              rust-analyzer
            ] ++ frontendDeps;
          };

          # Add your auto-formatters here.
          # cf. https://numtide.github.io/treefmt/
          treefmt.config = {
            projectRootFile = "flake.nix";
            programs = {
              nixpkgs-fmt.enable = true;
              rustfmt.enable = true;
            };
          };
        };
    };
}
