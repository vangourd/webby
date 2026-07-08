{
  description = "Webby - Leptos app with SQLite";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        commonEnv = ''
          export PATH="${rustToolchain}/bin:${pkgs.wasm-bindgen-cli}/bin:${pkgs.dart-sass}/bin:${pkgs.pkg-config}/bin:${pkgs.stdenv.cc}/bin:${pkgs.openssl}/bin:$PATH"
          export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"
          export OPENSSL_DIR="${pkgs.openssl.dev}"
          export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
          export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
          export CC="${pkgs.stdenv.cc}/bin/cc"
        '';

        devServerScript = pkgs.writeShellScriptBin "webby-dev" ''
          set -e

          if [ -f .env ]; then
            set -a
            source .env
            set +a
            echo "Loaded .env"
          fi

          ${commonEnv}

          echo "Running migrations..."
          ${pkgs.sqlx-cli}/bin/sqlx migrate run

          echo "Starting dev server..."
          ${pkgs.cargo-leptos}/bin/cargo-leptos watch
        '';

        buildPushScript = pkgs.writeShellScriptBin "webby-build-push" ''
          set -e

          if [ -f .env ]; then
            set -a
            source .env
            set +a
          fi

          ${commonEnv}

          IMAGE="''${WEBBY_IMAGE:-registry.logan.systems/webby:latest}"

          echo "Building webby..."
          LEPTOS_HASH_FILES=true ${pkgs.cargo-leptos}/bin/cargo-leptos build --release

          echo "Patching binary for standard Linux..."
          ${pkgs.patchelf}/bin/patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 target/release/webby
          ${pkgs.patchelf}/bin/patchelf --set-rpath /lib/x86_64-linux-gnu target/release/webby

          echo "Preparing build context..."
          rm -rf build-context
          mkdir -p build-context
          cp target/release/webby build-context/
          cp target/release/hash.txt build-context/ 2>/dev/null || true
          cp -r target/site build-context/site
          cp -r migrations build-context/
          cp Cargo.toml build-context/

          echo "Building container image: $IMAGE"
          ${pkgs.podman}/bin/podman build -t "$IMAGE" -f Dockerfile.runtime build-context

          echo "Pushing to registry.logan.systems..."
          ${pkgs.podman}/bin/podman push "$IMAGE"

          echo "Done! Image pushed: $IMAGE"
        '';
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust
            rustToolchain
            cargo-leptos
            wasm-bindgen-cli

            # SQLx
            sqlx-cli

            # Container runtime
            podman

            # Build tools
            binaryen
            patchelf

            # Styles
            dart-sass

            # Utils
            openssl
            pkg-config
          ];

          shellHook = ''
            if [ -f .env ]; then
              set -a
              source .env
              set +a
              echo "Loaded .env"
            fi

            export OPENSSL_DIR="${pkgs.openssl.dev}"
            export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
            export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
            export CC="${pkgs.stdenv.cc}/bin/cc"
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"

            echo "Webby dev environment loaded"
            echo ""
            echo "Commands:"
            echo "  cargo leptos watch    - Dev server with hot reload"
            echo "  nix run .#dev         - Same, via flake app"
            echo "  nix run .#build-push  - Build release image and push to registry.logan.systems"
            echo ""
          '';

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };

        apps = {
          dev = {
            type = "app";
            program = "${devServerScript}/bin/webby-dev";
          };

          build-push = {
            type = "app";
            program = "${buildPushScript}/bin/webby-build-push";
          };
        };
      });
}
