# Claude Code Configuration

**This is a PUBLIC repository.** Do not reference private hostnames, registries, IPs, credentials, personal email/handles, or internal-tool URLs in code or docs. Use placeholder values (`registry.example.com`, `webby.example.com`, env vars) and let deployers configure real values themselves.

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
# Build release image and push (set WEBBY_IMAGE to your registry target)
WEBBY_IMAGE=registry.example.com/webby:latest nix run .#build-push

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
- Push target is set by `WEBBY_IMAGE` env var; the flake app expects the operator to configure their own registry
- OAuth is handled at the cluster ingress layer (oauth2-proxy), not in the app
