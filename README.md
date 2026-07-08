# Webby

Leptos 0.6 SSR app with SQLite, deployed to K8s via `registry.logan.systems`.

## Dev

```bash
nix develop --command cargo leptos watch
```

SQLite DB is created automatically on first run. No infra containers needed.

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
