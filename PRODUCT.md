# Product

<!-- impeccable:product-schema 1 -->

## Platform

linux

## Users

- Power users, developers, and Linux enthusiasts operating modern Wayland desktop environments (primarily tiling compositors such as Niri, Hyprland, and Sway, especially on distributions like NixOS and Arch).
- Users who need instant, keyboard-driven access to launch applications, search files, execute shell commands, manage windows, control media/audio, and inspect system telemetry without context switching or launching separate terminal windows.

## Product Purpose

- Provide a blazing-fast (<50ms display latency), local-first Linux command center and keyboard launcher inspired by Raycast v2 and Vicinae.
- Eliminate fragmented standalone status tools by bundling a cohesive, self-contained background service layer (built-in notification server, MPRIS media control, audio device management, NetworkManager status, text/image clipboard history, and local AI prompts).
- Success means instantaneous keyboard responsiveness, complete absence of UI freezing or crashes, strict local data ownership, and seamless integration with Wayland compositors.

## Positioning

- Unlike Raycast (macOS-focused, closed source, cloud dependencies) or Vicinae (monolithic TypeScript/React-based extension runtime without native Linux service daemon integration), Zeshicast is a compiled Rust application with an integrated freedesktop notification daemon, Wayland layer-shell overlay, and a strict fail-closed capability execution model.
- Zero telemetry, zero external network requests without explicit user intent, local SQLite storage with strict permissions (`0600`), and reproducible deployment via Nix flake and NixOS/Home Manager modules.

## Operating Context

- **Compositor & Session:** Wayland graphical sessions utilizing `gtk4-layer-shell` or standard overlay windowing; optimized for Niri, Hyprland, and Sway.
- **Daemon Lifecycle:** Headless or background resident GTK daemon running as a `systemd` user service (`zeshicast.service` / `zeshicast-gtk.service`), maintaining a warm search index, listening on D-Bus for `org.freedesktop.Notifications`, and capturing clipboard events.
- **System Tools:** Interacts with local CLI and IPC tools: `wpctl` (WirePlumber audio), `nmcli` (NetworkManager), `niri msg` / `hyprctl` / `swaymsg` (compositor window actions), `wl-clipboard` (`wl-copy`/`wl-paste`), and `brightnessctl`.
- **Local AI & Translation:** Connects to local endpoints (Ollama at `http://localhost:11434`, LibreTranslate at `http://localhost:5000`) with fallback to OpenAI-compatible endpoints.

## Capabilities and Constraints

- **Confirmed Capabilities:**
  - Root search across XDG desktop applications, files (`$HOME`), calculator expressions, shell commands, process inspection/termination, and system actions (lock, suspend, restart, power off).
  - Built-in freedesktop D-Bus notification daemon with session history and persistent Do Not Disturb (DND).
  - Clipboard history manager capturing text and image PNGs with retention pruning and private mode.
  - Media player controls (MPRIS over D-Bus with album art, timeline, scrubber) and PipeWire/WirePlumber audio controls.
  - Network overview and Wi-Fi connection manager via NetworkManager.
  - Custom commands (argv, shell, and JSON modes) with typed arguments, input forms, and dynamic placeholders (`{{query}}`, `{{clipboard}}`, `{{date}}`, etc.).
  - Action panel (`Ctrl+K`), quicklinks, snippets, aliases, and item pinning.
  - Local AI chat view and quick translation queries.
  - Multi-view direct launch flags (`--view clipboard`, `--view dashboard`, etc.) and headless CLI tool (`zeshicast`).
- **Confirmed Constraints:**
  - **Fail-Closed Security:** Extensions run with restricted permissions (`capabilities = ["shell"]` required for shell execution; declared capabilities in extension manifests act as an upper bound).
  - **Zero Crashes Policy:** No panics or unhandled `unwrap()` calls on D-Bus, network, or user input data.
  - **Zero Telemetry:** Strictly local-first; no analytics or remote logging.
  - **Wayland / Linux First:** No macOS, Windows, or X11 legacy support targets.

## Brand Commitments

- **Name:** Zeshicast.
- **Tone & Voice:** "Quiet Linux Cockpit" — dense, calm, precise, utilitarian, and distraction-free.
- **Visual References:** Raycast v2 layout density and handoff styling (`design_handoff_zeshicast/zeshicast-gtk4.css`, `design_handoff_zeshicast/zeshicast.html`).
- **Typography Identity:** `Outfit` for UI headings and titles, `JetBrains Mono` for code, shortcuts, and keycaps, fallback to `Inter` and `Noto Sans`.
- **System Theme Harmony:** Follows active GTK theme colors (`@window_bg_color`, `@window_fg_color`, `@accent_color`), accented by four semantic color tokens (`ac_purple`: `#8B7CF8`, `ac_amber`: `#F5A623`, `ac_green`: `#4BD98A`, `ac_red`: `#FF6B5F`).

## Evidence on Hand

- Production code implementing 102+ passing automated tests (`src/`, `Cargo.toml`).
- Working Nix flake (`flake.nix`) and Home Manager / NixOS module configurations.
- Design handoff artifacts and CSS tokens: `design_handoff_zeshicast/zeshicast-gtk4.css` and screenshot references (`docs/img/`).
- Master architecture and roadmap documentation: `docs/DESIGN.md`, `docs/MASTER_PLAN.md`, `docs/product-plan-2026-09-19.md`, `docs/security.md`, `docs/privacy.md`.

## Product Principles

- **Instantaneous Velocity:** Sub-50ms window activation, non-blocking GTK event loop, and asynchronous background tasks via Tokio / GLib channels.
- **Fail-Closed & Transparent:** Capabilities must be explicitly declared and granted; unauthorized actions are blocked with clear explanations rather than silent failures or permissive defaults.
- **Self-Contained Autonomy:** Essential desktop services (notifications, media, clipboard) live within the resident process without demanding external daemon sprawl (no swaync, dunst, or playerctl dependencies).
- **Keyboard Supremacy:** Every view, item, and sub-action must be fully navigatable, searchable, and executable without touching the mouse.
- **Respect Local Truth:** Zero telemetry, local-only storage (`0600`), and zero network calls without intentional user action.

## Accessibility & Inclusion

- High-contrast text legibility respecting system themes and dark modes.
- Monospace keycap badges (`[Enter]`, `[Ctrl+K]`) with explicit visual cues for all keyboard shortcuts.
- Clear visual warning distinctions (`ac_red` / `#FF6B5F`) for destructive actions (process kill, system shutdown, clipboard clear).
- Semantic keyboard navigation skipping non-actionable headers and separator rows.
