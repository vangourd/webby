# Webby — Project Kanban

```
webby/
├── [X] Web push notifications
│   ├── [X] Bell UI + browser permission + subscription flow
│   ├── [X] Subscriptions stored in SQLite
│   ├── [X] send_push_notification() utility
│   └── [X] VAPID keys generated and stored in .env for local dev
│
├── [ ] Agent real backend
│   ├── [ ] Server fn dispatch from send button
│   └── [ ] Stream responses back to UI
│
├── [ ] Agent list from DB
│
├── [ ] Agent status polling / SSE
│
├── [ ] Notification triggers (wire send_push_notification to agent events)
│
├── [%] Terminal view (broker/relay architecture)
│   ├── [X] Runner↔server WebSocket protocol (binary PTY data + JSON control text frames)
│   ├── [X] Server-side relay — RunnerRegistry, runner/terminal WS handlers, /api/runners
│   ├── [X] Runner binary (webby-runner --server <url> --name <name>) — dials out, spawns PTY
│   ├── [ ] xterm.js Leptos component — browser↔server WS, full PTY rendering
│   └── [ ] Runner registry UI — pick which runner/machine to connect to
│
├── [ ] Smart agent UI (parallel track)
│   ├── [ ] Structured agent loop owning tool dispatch
│   ├── [ ] WASM policy modules — intercept, allow/deny/transform tool calls
│   ├── [ ] Claude API integration with tool use
│   └── [ ] Agent UI views (tool call log, approval queue, etc.)
│
├── [ ] K8s deployment
│   ├── [ ] Store VAPID keys as OpenBao secret
│   ├── [ ] Configure Vault Agent sidecar annotations to inject secret as pod env vars
│   ├── [ ] PVC for SQLite /data volume
│   ├── [ ] Push image to registry.logan.systems
│   └── [ ] Apply manifests and smoke test
│
├── [ ] Auth / user identity (oauth2-proxy headers)
│
├── [ ] File attachments / context uploads
│
└── [ ] Multi-agent orchestration
    ├── [ ] Agent delegation
    └── [ ] Dependency graph in UI
```
