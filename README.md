# Webby

Leptos 0.6 SSR app with SQLite, deployed to K8s via `registry.example.com`.

## Dev

```bash
nix develop --command cargo leptos watch
```

SQLite DB is created automatically on first run. No infra containers needed.

## Single binary: server + runner

`webby` is a single binary that runs either as the web server or as a PTY runner that connects back to a server.

```bash
# Server (default — same as `webby serve`)
webby

# Runner — attach this machine to a webby server as a terminal target
webby runner http://webby.example.com --name my-box
```

Runner options:

- `--name <NAME>` — human-readable name (defaults to `/etc/hostname`). Doubles as identity across reconnects.
- `--shell <SHELL>` — shell to spawn (default: `bash`). Ignored if `--command` is set.
- `--command <CMD>` — full command line to spawn instead of a shell, executed via `sh -c` so quotes/pipes/redirects work.

Both `http(s)://` and `ws(s)://` URLs are accepted; `http` → `ws`, `https` → `wss`.

### Sandboxed sessions

Use `--command` to wrap the session in podman, microvm.nix + ssh, or anything else. Each runner process = one session; run multiple for multiple sessions (proper multi-session UI is backlogged).

```bash
# Raw shell (default)
webby runner http://webby --name laptop-raw

# Podman-sandboxed session with a mounted work dir and full egress
webby runner http://webby --name laptop-sbx \
  --command 'podman run --rm -it \
    -v $HOME/webby-sbx:/work -w /work \
    --network bridge \
    registry.example.com/webby-sandbox:latest bash'

# NixOS microvm via SSH (Linux + KVM only; guest exposes sshd)
webby runner http://webby --name laptop-microvm \
  --command 'ssh -tt -o StrictHostKeyChecking=accept-new sbx@webby-microvm.local'
```

The runner treats the child process's PTY as the session — reconnects, resize, and multi-watcher all work regardless of what's on the other end.

Bundling both roles in one binary keeps deployment trivial — the same image on the K8s side and on a laptop can serve or act as a runner.

### NixOS install (server + runner as flake modules)

The flake exposes both a package and two NixOS modules:

```nix
# your NixOS flake.nix
{
  inputs.webby.url = "github:you/webby";
  outputs = { self, nixpkgs, webby, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        webby.nixosModules.default
        {
          services.webby-server = {
            enable = true;
            package = webby.packages.x86_64-linux.webby;
            port = 8080;
            openFirewall = true;
          };

          services.webby-runner = {
            package = webby.packages.x86_64-linux.webby;
            instances = {
              raw = { server = "http://localhost:8080"; };
              sbx = {
                server = "http://localhost:8080";
                command = "podman run --rm -it -v /var/lib/webby-runner/work:/work debian:stable bash";
              };
            };
          };
        }
      ];
    };
  };
}
```

Individual modules: `webby.nixosModules.server` and `webby.nixosModules.runner`. Sanity-check the wiring locally with `nix build .#nixosConfigurations.example.config.system.build.toplevel`.

### Runner as a systemd service (non-NixOS)

Two patterns under `packaging/systemd/`:

- **User template** (`user/webby-runner@.service`) — recommended for personal desktops. One template, N instances, each with its own env file at `~/.config/webby-runner/<name>.env`. Runs as you. `loginctl enable-linger $USER` keeps sessions alive across logout / on boot.
- **System single** (`webby-runner.service`) — one always-on runner as a dedicated service user. For headless servers, K8s nodes, etc.

See `packaging/systemd/README.md` for step-by-step. The user template README covers a three-project layout: raw shell for VR debugging, isolated `cargo leptos watch` on port 8081 for hacking on webby itself, and a `chunkker` session for GPU-hosted app development.

## Build & deploy

```bash
nix run .#build-push
kubectl apply -f manifests/
```

Once the manifests are applied, subsequent deploys are just a push — no `kubectl` needed:

```bash
nix run .#build-push
```

The deployment uses [Keel](https://keel.sh) (`keel.sh/policy: force`) to watch the registry and automatically restart the pod when a new `:latest` image appears. `imagePullPolicy: Always` ensures the new image is pulled on restart.

> The manifests in this repo use `registry.example.com` and `webby.example.com` as placeholders. Real hostnames live in the IAC repo.

## Gotchas

### wasm-bindgen version pin

`wasm-bindgen` in `Cargo.toml` is pinned to an exact version (`=0.2.x`) that **must match** the `wasm-bindgen-cli` binary shipped by nixpkgs. If you bump nixpkgs and the build fails with a schema version mismatch, check the CLI version:

```bash
nix develop --command wasm-bindgen --version
```

Then update the pin in `Cargo.toml` and regenerate the lock entry:

```bash
cargo update -p wasm-bindgen --precise <new-version>
```
