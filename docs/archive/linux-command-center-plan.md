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
- Current output device.
- Volume.

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
- `dashboard_poll_interval_ms = 2000`
- `media_enabled = true`
- `notifications_enabled = false`
- `network_enabled = true`
- `ai_provider = "ollama"`
- `ai_endpoint = "http://localhost:11434"`
- `ai_model = ""`

Defaults should preserve launcher speed and avoid background work unless a view
or status item needs it.

Status: base done. Preferences UI exposes feature toggles for dashboard, network,
media, notifications, and AI. Root search respects these toggles while keeping
the current defaults enabled.

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
to 2000 ms. Dashboard includes quick controls for Wi-Fi, Bluetooth, DND,
output mute, lock, and suspend. Audio output/input state comes from
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

Status: base done. A lightweight network status view opens with `Ctrl+N` or the
`network` command and lists interfaces from `/sys/class/net` with MAC/IP
details from `/sys`, `ip -o addr`, and DNS servers from `/etc/resolv.conf`.
NetworkManager DBus integration is intentionally left for the improvement phase.
The view can copy the selected interface IP or MAC, disconnect selected
interfaces, connect open/known Wi-Fi networks through `nmcli`, shows available
Wi-Fi networks from `nmcli` when available, and lists active VPN/WireGuard
connections.

Media status also has a first lightweight view: `Ctrl+M` or the `media` command
opens an MPRIS snapshot through `playerctl` when available, exposes
previous/play-pause/next controls, and shows a harmless empty state otherwise.
Root search also exposes `media`/`player`/`mpris` playback actions.

Notifications have a first optional status view through the `notifications`
command, `Ctrl+U`, and the Dashboard `Notify` button. It detects `swaync` or
`dunst` state when their control CLIs are available and otherwise stays in a
harmless empty state. Root search exposes `notify`/`dnd` actions for DND and
dismiss controls, and the Notifications view exposes DND, close-all, and panel
buttons. When `dunstctl history` returns JSON, the view also lists recent
history entries. Rich per-notification actions remain part of the improvement
phase.

### Phase E: Local AI

- Add Ollama-compatible client.
- Add `ai <prompt>` quick result.
- Add in-launcher AI chat view.
- Add actions to copy/save answer as snippet.

Status: base done. `ai <prompt>` now uses the Ollama-compatible local
`/api/generate` path by default and copies the answer to clipboard. `Ctrl+I` or
the `AI Chat` command opens an in-launcher local AI prompt using the same
`ollama_endpoint` and `ollama_model` preferences, falling back to
`http://localhost:11434` for the endpoint. Answers can be copied or saved as
AI-tagged snippets. The chat request runs off the GTK main loop so the launcher
does not freeze while waiting for the local model. AI Chat can seed the prompt
from the latest clipboard item. OpenAI-compatible quick AI remains available
with `ai_provider = "openai"`.

## Base Plan Completion

The baseline command-center plan is implemented enough to move into iterative
product improvements. Remaining work is intentionally classified as polish or
backend depth rather than missing MVP surface:

- Replace CLI-backed integrations with DBus/native backends where that improves
  reliability: NetworkManager, MPRIS, notification history/actions.
- Add richer AI chat behavior: streaming output, cancellation, conversation
  memory, and follow-up threading.
- Add compositor/workspace summaries to Dashboard and Status Strip.
- Improve per-view keyboard actions, accessibility labels, and visual density.

## Product Guardrails

- Root launcher must stay fast.
- Status/dashboard features must be optional.
- Compositor/system integrations must live behind service traits.
- Do not make the launcher depend on one desktop environment.
- Prefer native GTK widgets and keyboard-first navigation.
- Add new background polling only when there is a clear visible feature using
  that data.
