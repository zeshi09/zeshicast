# Linux Command Center Plan

Goal: evolve Zeshicast from a Raycast/Vicinae-style launcher into a native
Linux command center: launcher, compositor companion, control center, personal
workspace, and local AI assistant in one keyboard-first GTK app.

This plan is separate from the Vicinae parity roadmap. Vicinae/Raycast remain
UX references, but this direction is about making Zeshicast more Linux-native
and local-first than a direct clone.

## Product Thesis

Zeshicast should become a fast local command center for Linux:

- Launch apps, files, URLs, scripts, snippets, and workflows.
- Control the system, compositor, windows, media, network, notifications, and
  power state from the keyboard.
- Provide a lightweight dashboard for users who want less dependence on bars or
  separate control-center widgets.
- Provide quick local AI interactions without opening OpenWebUI, a browser, or
  a coding agent.
- Stay open-source, native Rust/GTK, Wayland-aware, and extension-friendly.

## Core Surfaces

### Root Launcher

- Search input.
- Ranked result list.
- Action panel.
- Footer/status strip with small live status items.
- Navigation stack for richer views.

The launcher must remain fast and uncluttered. Dashboard and status features
should be visible only as compact signals until the user opens the relevant
view.

### Status Strip

Small persistent strip in the root launcher, intended as a glanceable
replacement for the most useful bar indicators.

Initial items:

- Clock.
- Date.
- Basic network state.
- Basic battery state.
- Basic media state.
- Optional hints for current view.

Status: base done. The root launcher now has a live clock/date strip and compact
network/battery/audio/media text signals. It can be disabled with
`show_status_strip = false`; visible items are controlled through
`status_items`.

Additional future items:

- Network state.
- Battery.
- Volume.
- Microphone mute state.
- Media title/player.
- Notification count.
- Current workspace/window state.

Status: base done. Settings allow disabling the strip or selecting which items are
shown.

### Dashboard View

Open with a command such as `dashboard` or a shortcut like `Ctrl+D`.

Sections:

- Large clock/date.
- CPU, memory, disk, battery.
- Network state and IP.
- Audio output/input and volume.
- Media playback.
- Notification summary.
- Compositor/workspace/window summary.
- Quick toggles: Wi-Fi, Bluetooth, DND, mute, lock, suspend.

The dashboard should be optional and cheap to keep updated.

### System Monitor View

- CPU load.
- Memory use.
- Disk use.
- Battery and power state.
- Top processes.
- Kill/process actions.
- Temperatures later, if available.

This view should be separate from root search so expensive polling does not
slow down normal launcher use.

### Network View

- Current network.
- Available Wi-Fi networks.
- Connect/disconnect.
- VPN status.
- IP/DNS details.
- Copy IP actions.

Implementation should abstract over available Linux backends, starting with
NetworkManager where present.

### Media View

- MPRIS players.
- Current track.
- Play/pause/next/previous.
- Volume and audio devices live in the dedicated Audio View (`audio` / `Ctrl+A`).

This should also expose media actions in root search.

### Notifications View

- Notification history.
- Dismiss selected/all.
- DND toggle.
- Open related application where possible.

The notification service should be optional, because not every user wants the
launcher to act as a notification center.

### Local AI

Two surfaces:

- Quick AI result view: `ai <question>` streams or shows a short answer.
- AI Chat view: lightweight prompt/answer interface inside the launcher.

Initial provider:

- Ollama-compatible local HTTP API.

Later providers:

- OpenAI-compatible endpoints.
- Custom local servers.

Core actions:

- Copy answer.
- Continue conversation.
- Ask follow-up.
- Save answer as snippet.
- Use clipboard or selected text as context.

This is not meant to replace OpenWebUI. It is for fast, disposable questions.

## Architecture

Add service modules over time:

```text
src/services/
  clock.rs
  system_stats.rs
  network.rs
  media.rs
  notifications.rs
  battery.rs
  ai.rs

src/ui/
  status_strip.rs
  dashboard.rs
  system_monitor.rs
  network_view.rs
  media_view.rs
  notifications_view.rs
  ai_chat.rs
```

Use snapshot-style APIs first:

```text
StatusProvider -> StatusSnapshot
DashboardProvider -> DashboardSnapshot
```

Polling rules:

- Root status strip: very cheap, usually 1 second for clock and slower for
  system data.
- Dashboard: 1-2 second refresh.
- Heavy data such as process lists, Wi-Fi scans, and disk stats: refresh only
  when the view is visible or on explicit refresh.

## Settings

Required preferences:

- `show_status_strip = true`
- `status_items = ["clock", "date"]`
- `dashboard_enabled = true`
- `dashboard_poll_interval_ms = 1000`
- `media_enabled = true`
- `notifications_enabled = true`
- `network_enabled = true`
- `ai_provider = "ollama"`
- `ollama_endpoint = "http://localhost:11434"`
- `ollama_model = "llama3.2:3b"`
- `ai_endpoint = "https://api.openai.com/v1"`
- `ai_model = "gpt-4o-mini"`

Defaults should preserve launcher speed and avoid background work unless a view
or status item needs it.

Status: base done. Preferences UI exposes feature toggles for dashboard, network,
media, notifications, and AI. Root search respects these toggles while keeping
the current defaults enabled (defaults match `PREFERENCE_DEFAULTS` in
`src/ui/preferences.rs`).

## Implementation Order

### Phase A: Status Strip

- Add a status strip widget in the root launcher.
- Show clock and date.
- Update clock once per second.
- Make the strip isolated in `ui/status_strip.rs`.
- Add preferences later for enabling/disabling items.

### Phase B: Dashboard Skeleton

- Add `LauncherView::Dashboard`.
- Add dashboard view with clock/date and placeholder sections.
- Open with `Ctrl+D` and a root command.
- Wire cheap refresh only while visible.

Status: base done. `Ctrl+D` opens an in-window dashboard with clock/date, uptime,
load average, memory usage, root disk usage, network status/address,
battery/power state, media playback status, notification state, and process
count. It also links directly to Network, Media, and AI views.
Dashboard/System Monitor refresh uses `dashboard_poll_interval_ms`, defaulting
to 1000 ms. Dashboard includes quick controls for Wi-Fi, Bluetooth, DND,
output mute, lock, and suspend (kept for IPC/keyboard bindings; the visible
row is hidden in the current UI). Audio output/input state comes from
`wpctl get-volume` when available.

### Phase C: System Snapshot

- Add service for CPU/memory/disk basics.
- Show system snapshot in dashboard.
- Add a full system monitor view later.

Status: base done. A lightweight `/proc` plus `df`-based system snapshot service
provides load average, memory usage, root disk usage, uptime, and process count
without extra Rust dependencies. A first System Monitor view is available via
the `system monitor` command and `Ctrl+T`; it adds top processes by RSS.
Battery status is read from `/sys/class/power_supply` and shown in Dashboard
when available. System Monitor can terminate the selected process with Delete
or its stop button. It also reads `/sys/class/thermal` and shows the hottest
thermal zone when the kernel exposes temperature sensors.

### Phase D: Network/Media/Notifications

- Add NetworkManager-backed network snapshot.
- Add MPRIS-backed media snapshot/actions.
- Add optional notification history service.

Status: done. A lightweight network status view opens with `Ctrl+N` or the
`network` command and lists interfaces from `/sys/class/net` with MAC/IP
details from `/sys`, `ip -o addr`, and DNS servers from `/etc/resolv.conf`.
NetworkManager native D-Bus integration (`org.freedesktop.NetworkManager` via
GIO) queries Wi-Fi access points and active VPN/WireGuard connections directly
in-process, falling back cleanly to `nmcli`.
The view can copy the selected interface IP or MAC, disconnect selected
interfaces, connect open/known Wi-Fi networks through `nmcli`, shows available
Wi-Fi networks, and lists active VPN/WireGuard connections.

Media status also has a first lightweight view: `Ctrl+M` or the `media` command
opens a native MPRIS snapshot over the session D-Bus via gio — no external
`playerctl` dependency (`services/media.rs`) — exposes
previous/play-pause/next controls, and shows a harmless empty state otherwise.
Root search also exposes `media`/`player`/`mpris` playback actions.

Notifications are handled by zeshicast's own built-in freedesktop notification
daemon: it owns the `org.freedesktop.Notifications` session-bus name
(`services/notifications.rs` + `ui/notify_server.rs`), records every incoming
notification into an in-memory history for the current daemon session, and
persists only the DND flag (`~/.config/zeshicast/dnd`). The view opens through
the `notifications` command, `Ctrl+U`, and the Dashboard; when another daemon
owns the bus name, zeshicast stays in a harmless empty state and cannot record
that notification stream. Root search exposes `notify`/`dnd` actions for DND
and dismiss controls, and the Notifications view exposes DND, close-all, and
panel buttons. Rich per-notification actions remain part of the improvement
phase. External daemons such as `swaync` or `dunst` play no role: disable them
so zeshicast can acquire the name.

### Phase E: Local AI

- Add Ollama-compatible client.
- Add `ai <prompt>` quick result.
- Add in-launcher AI chat view with streaming, markdown, and multi-turn context.
- Add actions to copy/save answer as snippet.

Status: done. `ai <prompt>` uses the Ollama-compatible local `/api/generate`
path and copies the answer to clipboard. `Ctrl+I` or the `AI Chat` command opens
an in-launcher local AI chat supporting streaming token rendering, cancelation,
message history conversation context (`/api/chat`), markdown formatting, model
selection, and clipboard context insertion. Answers can be copied or saved as
AI-tagged snippets. The chat request runs off the GTK main loop so the launcher
does not freeze while waiting for the local model. OpenAI-compatible quick AI
remains available with `ai_provider = "openai"`.

## Base Plan Completion

The baseline command-center plan is implemented with native Linux integrations:

- Native DBus backends: NetworkManager (Wi-Fi & active VPNs), MPRIS (playback &
  metadata), and notification daemon (`org.freedesktop.Notifications`).
- AI chat behavior: streaming token output, cancellation, multi-turn conversation
  memory via `/api/chat`, and snippet persistence.
- Compositor/workspace summaries in Dashboard and Status Strip.
- Keyboard-first navigation across 13 domain views.

## Product Guardrails

- Root launcher must stay fast.
- Status/dashboard features must be optional.
- Compositor/system integrations must live behind service traits.
- Do not make the launcher depend on one desktop environment.
- Prefer native GTK widgets and keyboard-first navigation.
- Add new background polling only when there is a clear visible feature using
  that data.
