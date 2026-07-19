# Webby — Project Kanban

```
webby/
├── [X] Web push notifications
│   ├── [X] Bell UI + browser permission + subscription flow
│   ├── [X] Subscriptions stored in SQLite
│   ├── [X] send_push_notification() utility
│   └── [X] VAPID keys generated and stored in .env for local dev
│
├── [X] Single-binary refactor
│   ├── [X] `webby serve` (default) and `webby runner <URL>` subcommands
│   ├── [X] http(s):// URLs auto-rewritten to ws(s)://
│   └── [X] README section documenting both modes
│
├── [%] Terminal view (broker/relay architecture)
│   ├── [X] Runner↔server WebSocket protocol (binary PTY + JSON control)
│   ├── [X] Server-side relay — RunnerRegistry, runner/terminal WS handlers, /api/runners
│   ├── [X] Runner binary (webby runner) — dials out, spawns PTY
│   ├── [X] xterm.js Leptos component — WS wire-up, resize via FitAddon+ResizeObserver
│   ├── [X] Runner registry UI — pick which runner to attach to
│   ├── [X] Vendored xterm assets (no CDN) served from /vendor/
│   ├── [X] Runner reconnect with backoff — shell survives WS drops
│   ├── [X] Multi-watcher: HashMap fan-out, tmux-style shared shell
│   ├── [X] Reconnect identity — same `--name` reuses runner_id, watchers stay attached
│   ├── [X] Grace period (30s) before tearing down handle on runner disconnect
│   └── [X] Size replay to reconnected runner
│
├── [ ] Notification triggers
│   ├── [ ] Bell (`\x07`) detection in relay → push notify
│   ├── [ ] Idle-after-activity detection → push notify (catches stuck sleep loops)
│   ├── [ ] Shared debounce so bell + idle don't double-fire
│   └── [ ] Extension<SqlitePool> plumbed into runner handler
│
├── [%] Sessions on the runner (one process per session, crash-isolated)
│   ├── [X] Runner `--command <CMD>` flag — override spawned process (executed via `sh -c`)
│   ├── [ ] Runner `--sandbox podman` preset — wrap in `podman run` with mounted workdir + jj + egress
│   ├── [ ] Runner `--sandbox microvm` preset — microvm.nix guest, ssh -tt into it (Linux+KVM only)
│   ├── [ ] Sandbox container image (jj + git + curl + toolchains) pushed to registry.example.com
│   ├── [ ] `webby session-manager` — long-lived per-host WS to server, spawns `webby runner` children on demand
│   ├── [ ] UI: "new session" button + session list per host + type picker
│   └── [ ] Session type presets: raw-shell | podman-sbx | microvm-sbx | claude | codex | pi.dev
│
├── [ ] Command-safety policy
│   ├── [ ] Shell AST parser (grammar-based, not NLP)
│   ├── [ ] WASM policy engine (Wasmtime/Extism, or OPA-compiled-to-WASM)
│   ├── [ ] Declarative rules: allowlist tools, block egress hosts, deny writes outside cwd
│   ├── [ ] Escalation flow → push notify → mobile approve/deny → runner executes
│   └── [ ] Same policy WASM runs in browser preview, server, and runner
│
├── [ ] xterm hardening
│   ├── [ ] Audit escape-sequence surface (OSC 52 clipboard, OSC 8 links, DECRQSS queries)
│   └── [ ] Disable/allowlist by xterm config, not WASM
│
├── [ ] Custom agent harness
│   ├── [ ] OpenRouter integration (any-model backend)
│   ├── [ ] Agent loop owning tool dispatch
│   ├── [ ] Tool calls gated by command-safety policy above
│   ├── [ ] Approval queue UI
│   └── [ ] Tool call log per session
│
├── [ ] Agent backend (server-driven, not runner-driven)
│   ├── [ ] Server fn dispatch from send button
│   ├── [ ] Stream responses back to UI
│   ├── [ ] Agent list from DB
│   └── [ ] Agent status polling / SSE
│
├── [ ] K8s deployment
│   ├── [ ] Store VAPID keys as OpenBao secret
│   ├── [ ] Vault Agent sidecar annotations to inject secret as pod env vars
│   ├── [ ] PVC for SQLite /data volume
│   ├── [ ] Push image to registry.example.com
│   └── [ ] Apply manifests and smoke test
│
├── [%] Runner packaging & install
│   ├── [X] systemd system unit (`webby-runner.service`) + env file — single-instance
│   ├── [X] systemd user template (`user/webby-runner@.service`) — multi-instance, per-instance env, works with linger
│   ├── [X] Nix flake package (`packages.default`) via crane + cargo-leptos
│   ├── [X] NixOS module for server (`nixosModules.server`) — `services.webby-server.{enable,port,dataDir,openFirewall,...}`
│   ├── [X] NixOS module for runner (`nixosModules.runner`) — `services.webby-runner.instances.<name>`
│   ├── [X] `nixosConfigurations.example` — reference/testing config
│   ├── [ ] Nix build of Debian .deb (via `dpkg-deb` or `mkDerivation` + `nix-bundle`)
│   ├── [ ] Nix build of RPM (via `rpm-tools` in nixpkgs)
│   ├── [ ] macOS `launchd` plist for headless-Mac runners
│   ├── [ ] Windows service wrapper (nssm or native)
│   ├── [ ] `webby runner install` bootstrap command — writes config, enables service, joins server
│   └── [ ] Signed release binaries per platform
│
├── [ ] GPU-backed inference resources
│   ├── [ ] Runner declares GPU capability on Hello (device count, VRAM, driver ver)
│   ├── [ ] Server exposes GPU runners as "inference endpoints" (llama.cpp / vLLM / ollama)
│   ├── [ ] Route agent-harness model calls through discovered GPU runners
│   ├── [ ] Fall through to OpenRouter when no local GPU available or model unsupported
│   └── [ ] systemd unit adds `webby-runner` to `render`/`video` groups
│
├── [ ] Auth / user identity (oauth2-proxy headers)
│
├── [ ] File attachments / context uploads
│
└── [ ] Multi-agent orchestration
    ├── [ ] Agent delegation
    └── [ ] Dependency graph in UI
```

## Deferred / discussed and decided against

- **WASM sandbox around the shell/Claude process** — WASI can't `fork/exec` native binaries; wrong tool for wrapping subprocess execution. WASM is retained for the policy engine only.
- **Firecracker microVMs** — measured in weeks (rootfs + kernel + tap networking + serial-only console → SSH-agent inside VM). Doesn't run on macOS/Windows either. Ladder if kernel-level isolation is later needed: podman → podman+gVisor(runsc) → Kata → raw Firecracker.
- **NLP for command safety** — shell isn't natural language; use grammar-based AST parsing.
