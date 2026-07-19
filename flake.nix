{
  description = "Webby — Leptos SSR app with SQLite, PTY-forwarding runner, and NixOS modules";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, crane, ... }:
    let
      # ── Per-system outputs (packages, apps, devShell, overlays) ────────────
      perSystem = flake-utils.lib.eachDefaultSystem (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" ];
            targets = [ "wasm32-unknown-unknown" ];
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          # Keep everything except build artifacts / VCS / DB files. Cargo-leptos
          # needs Rust source, SCSS, static assets under public/, migrations, etc.
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            name = "webby-source";
            filter = path: type:
              let baseName = baseNameOf (toString path);
              in !(
                (type == "directory" && (
                  baseName == "target"
                  || baseName == "target-dev"
                  || baseName == "result"
                  || baseName == ".git"
                  || baseName == ".claude"
                  || baseName == "node_modules"
                  || baseName == "build-context"
                ))
                || (type == "regular" && (
                  pkgs.lib.hasPrefix "webby.db" baseName
                  || pkgs.lib.hasSuffix ".swp" baseName
                ))
              );
          };

          commonArgs = {
            inherit src;
            strictDeps = true;
            pname = "webby";
            version = "0.1.0";

            nativeBuildInputs = with pkgs; [
              rustToolchain
              cargo-leptos
              wasm-bindgen-cli
              dart-sass
              binaryen
              pkg-config
            ];
            buildInputs = with pkgs; [ openssl ];

            OPENSSL_DIR = "${pkgs.openssl.dev}";
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

            LEPTOS_HASH_FILES = "true";
          };

          # Warm the cargo cache separately (with both host + wasm32 targets).
          cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
            # cargo-leptos needs both targets to build ssr + hydrate.
            cargoExtraArgs = "--features ssr,runner,hydrate --no-default-features";
            doCheck = false;
          });

          webby = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
            doCheck = false;

            # Skip crane's cargo build; cargo-leptos runs its own two-target flow.
            buildPhaseCargoCommand = "cargo leptos build --release";
            checkPhaseCargoCommand = "true";

            # We install by hand — no cargo build log to parse.
            doNotPostBuildInstallCargoBinaries = true;

            installPhaseCommand = ''
              mkdir -p $out/bin $out/share/webby
              cp target/release/webby $out/bin/
              cp -r target/site       $out/share/webby/site
              cp -r migrations         $out/share/webby/migrations
            '';
          });

          # ── Legacy scripts (dev + build-push image) ──────────────────────
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
            if [ -f .env ]; then set -a; source .env; set +a; echo "Loaded .env"; fi
            ${commonEnv}
            echo "Running migrations..."
            ${pkgs.sqlx-cli}/bin/sqlx migrate run
            echo "Starting dev server..."
            ${pkgs.cargo-leptos}/bin/cargo-leptos watch
          '';

          buildPushScript = pkgs.writeShellScriptBin "webby-build-push" ''
            set -e
            if [ -f .env ]; then set -a; source .env; set +a; fi
            ${commonEnv}
            IMAGE="''${WEBBY_IMAGE:-registry.example.com/webby:latest}"
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
            echo "Pushing $IMAGE..."
            ${pkgs.podman}/bin/podman push "$IMAGE"
            echo "Done."
          '';
        in
        {
          packages = {
            default = webby;
            webby = webby;
          };

          overlays.default = final: prev: { webby = webby; };

          devShells.default = pkgs.mkShell {
            buildInputs = with pkgs; [
              rustToolchain
              cargo-leptos
              wasm-bindgen-cli
              sqlx-cli
              podman
              binaryen
              patchelf
              dart-sass
              openssl
              pkg-config
            ];

            shellHook = ''
              if [ -f .env ]; then set -a; source .env; set +a; echo "Loaded .env"; fi
              export OPENSSL_DIR="${pkgs.openssl.dev}"
              export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
              export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
              export CC="${pkgs.stdenv.cc}/bin/cc"
              export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"

              echo "Webby dev environment loaded"
              echo ""
              echo "Commands:"
              echo "  cargo leptos watch    - Dev server with hot reload"
              echo "  nix build             - Build webby package (bin + site + migrations)"
              echo "  nix run .#dev         - Same as cargo leptos watch, via flake app"
              echo "  nix run .#build-push  - Build release image and push (uses WEBBY_IMAGE)"
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

      # ── System-agnostic outputs (modules, example config) ──────────────────
      commonOutputs = {
        nixosModules = {
          server = import ./nix/modules/server.nix;
          runner = import ./nix/modules/runner.nix;
          default = { imports = [ self.nixosModules.server self.nixosModules.runner ]; };
        };

        # A minimal, evaluatable example demonstrating both modules.
        nixosConfigurations.example = nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          modules = [
            self.nixosModules.default
            ({ ... }: {
              # Bare minimum so the config evaluates.
              boot.loader.grub.device = "nodev";
              fileSystems."/" = { device = "/dev/null"; fsType = "ext4"; };
              system.stateVersion = "24.11";

              services.webby-server = {
                enable = true;
                port = 8080;
                package = self.packages.x86_64-linux.webby;
              };

              services.webby-runner = {
                package = self.packages.x86_64-linux.webby;
                instances.example = {
                  server = "http://localhost:8080";
                  # Runs the default shell. Uncomment for a sandboxed session:
                  # command = "podman run --rm -it -v /var/lib/webby/work:/work debian:stable bash";
                };
              };
            })
          ];
        };
      };
    in
    perSystem // commonOutputs;
}
