# Claude Code Configuration

## Environment Setup
**IMPORTANT**: Always use Nix for this project. Do not use system binaries.

```bash
# Enter development environment
nix develop

# Run dev server (migrations run automatically on startup)
nix develop --command cargo leptos watch

# OR via flake app:
nix run .#dev
```

## Build & Deploy

```bash
# Build release image and push to registry.logan.systems
nix run .#build-push

# Apply K8s manifests (namespace must exist first)
kubectl create namespace webby --dry-run=client -o yaml | kubectl apply -f -
kubectl apply -f manifests/
```

## Compilation Verification
`cargo check` alone is not sufficient for Leptos. Use:

```bash
nix develop --command cargo leptos build
```

This runs both the SSR (server) and WASM (client/hydrate) build pipelines.

## Database
- **Local dev**: SQLite file at `./webby.db` (auto-created, WAL mode)
- **K8s**: SQLite at `/data/webby.db` on a PVC mounted at `/data`
- Migrations live in `migrations/` and run automatically on startup
- Add new migrations: `nix develop --command sqlx migrate add <name>`

SQLite is single-writer, so the K8s deployment uses `replicas: 1` and `strategy: Recreate`.

## Styling
- SCSS only — no Tailwind
- Files in `style/`

## Notes
- All tooling must come from the Nix flake, not system PATH
- SQLite is bundled into the binary (no libsqlite3 runtime dep)
- Push target: `registry.logan.systems` — no auth needed on LAN
- OAuth is handled at the cluster ingress layer (oauth2-proxy), not in the app
