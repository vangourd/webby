# Webby runner — systemd install

Two supported patterns:

| Pattern | Unit file | Runs as | When to pick it |
|---|---|---|---|
| **User template** (recommended for personal desktops) | `user/webby-runner@.service` | your own uid | You want multiple sessions on your workstation, each with your env / workspace access (VR, dev, projects). Linger keeps them alive when you're not logged in. |
| **System single** | `webby-runner.service` | dedicated `webby-runner` user | Headless server, one runner, no user account involved. |

Both call the same `webby runner …` under the hood.

---

## User template (multi-instance)

### One-time setup

```bash
# Persist user services across logout / on boot
loginctl enable-linger $USER

# Install the template unit
mkdir -p ~/.config/systemd/user
cp packaging/systemd/user/webby-runner@.service ~/.config/systemd/user/

# Per-instance config dir
mkdir -p ~/.config/webby-runner
```

Point `WEBBY_BIN` (in each env file, or globally in `~/.config/environment.d/`) at wherever `webby` actually lives. For a Nix dev setup:

```bash
export WEBBY_BIN=$HOME/dev/webby/target/release/webby
```

For a normal install: `/usr/local/bin/webby` (the unit's default).

### Per-instance env files

Each instance = one file at `~/.config/webby-runner/<name>.env`. Environment variables understood by the unit:

| Var | Purpose | Default |
|---|---|---|
| `WEBBY_SERVER_URL` | Webby server (http/https auto-rewritten to ws/wss) | required |
| `WEBBY_RUNNER_NAME` | Name shown in the Agents list; identity across reconnects | the systemd instance name (`%i`) |
| `WEBBY_SHELL` | Shell to spawn | `bash` (from webby's default) |
| `WEBBY_COMMAND` | Full command line to spawn instead of a shell (via `sh -c`) | unset |
| `WEBBY_BIN` | Path to the `webby` binary | `/usr/local/bin/webby` |

Anything else in the env file (e.g. `LEPTOS_SITE_ADDR`, `CARGO_TARGET_DIR`) is inherited by the spawned shell — very useful for isolating dev builds.

### The three-project example

```bash
# ~/.config/webby-runner/vr.env — raw shell, full desktop access
WEBBY_SERVER_URL=http://localhost:8080
# Everything else defaults: name=vr, shell=bash, no --command.
```

```bash
# ~/.config/webby-runner/webby-dev.env — hack on webby itself
WEBBY_SERVER_URL=http://localhost:8080
# Isolate compile output and dev ports so the main webby session isn't touched
CARGO_TARGET_DIR=/home/YOU/dev/webby/target-dev
LEPTOS_SITE_ADDR=127.0.0.1:8081
LEPTOS_RELOAD_PORT=3002
# Land in the repo automatically
WEBBY_COMMAND=cd $HOME/dev/webby && exec bash
```

```bash
# ~/.config/webby-runner/chunkker.env — desktop-hosted for GPU
WEBBY_SERVER_URL=http://localhost:8080
# mkdir -p so the service still starts before you've cloned the repo
WEBBY_COMMAND=mkdir -p $HOME/dev/chunkker && cd $HOME/dev/chunkker && exec bash
```

### Start them

```bash
systemctl --user daemon-reload
systemctl --user enable --now webby-runner@vr.service
systemctl --user enable --now webby-runner@webby-dev.service
systemctl --user enable --now webby-runner@chunkker.service

# Check
systemctl --user status 'webby-runner@*'
journalctl --user -u webby-runner@vr.service -f
```

Now three "agents" show up in the Webby UI. From your Surface you attach to any of them; the runner-side sessions persist across your logout/login on the desktop.

### Editing an instance

```bash
$EDITOR ~/.config/webby-runner/vr.env
systemctl --user restart webby-runner@vr.service
```

Or add a drop-in for one instance without touching the template:

```bash
systemctl --user edit webby-runner@vr.service
```

### Troubleshooting

- **`Failed to load environment files: No such file or directory`** — you haven't created `~/.config/webby-runner/<name>.env` yet, or you exited your editor without saving.
- **`No such file or directory` on `WEBBY_BIN` path** — after editing `~/.config/environment.d/webby.conf`, the systemd user manager caches the old env. Fix: `systemctl --user daemon-reexec` (or log out and back in). Or just put `WEBBY_BIN=...` in each instance's env file to avoid the reload dance.
- **Unit reports "active" but shell exited immediately** — likely your `WEBBY_COMMAND` uses `cd $DIR && exec bash` and `$DIR` doesn't exist. `cd` fails, `&&` short-circuits, no shell is exec'd. Use `mkdir -p $DIR && cd $DIR && exec bash` to be forgiving.

---

## System single-instance (headless servers)

For a K8s node, home-lab box, or any machine where you just want one always-on runner tied to a service account.

### Prereqs

1. `webby` binary at `/usr/local/bin/webby` (or edit `ExecStart`).
2. Service user:

   ```bash
   sudo useradd --system --create-home --home-dir /var/lib/webby-runner \
     --shell /usr/sbin/nologin webby-runner
   ```

### Install

```bash
sudo cp packaging/systemd/webby-runner.service     /etc/systemd/system/
sudo cp packaging/systemd/webby-runner.env.example /etc/webby-runner.env
sudo chown root:webby-runner /etc/webby-runner.env
sudo chmod 640               /etc/webby-runner.env
sudo $EDITOR /etc/webby-runner.env

sudo systemctl daemon-reload
sudo systemctl enable --now webby-runner.service
journalctl -u webby-runner.service -f
```

### Custom command (podman, ssh into microvm, etc.)

```bash
sudo systemctl edit webby-runner.service
```

```ini
[Service]
ExecStart=
ExecStart=/usr/local/bin/webby runner ${WEBBY_SERVER_URL} \
  --name ${WEBBY_RUNNER_NAME} \
  --command "podman run --rm -it -v /var/lib/webby-runner/work:/work registry.example.com/webby-sandbox:latest bash"
```

### GPU access (backlog, but noted here)

```bash
sudo usermod -aG render,video webby-runner
```

In the drop-in:

```ini
[Service]
SupplementaryGroups=render video
DeviceAllow=/dev/dri/* rw
DeviceAllow=/dev/nvidia* rw
```
