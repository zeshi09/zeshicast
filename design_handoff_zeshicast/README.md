# Handoff: zeshicast Launcher

## Overview

zeshicast is a Raycast-inspired application launcher for Linux, built with **Rust + GTK4** on Wayland/Niri. It provides a floating window triggered by a global hotkey (`Super+Space`), containing a unified search bar, a Command Center with built-in views, and an Action Panel.

This document is the complete design reference for implementing the UI. The bundled HTML prototype (`zeshicast.html`) is a **hi-fidelity design mock** — not production code. Implement it in your existing Rust + GTK4 codebase using GTK4 widgets, the provided `zeshicast-gtk4.css` stylesheet, and the patterns described below.

---

## Fidelity

**High-fidelity.** The prototype is pixel-accurate in terms of colors, typography, spacing, component sizing, animations, and interactions. Recreate it faithfully using GTK4 widgets and the provided CSS. Where a GTK4 widget doesn't support a web CSS feature exactly, use the closest native equivalent (noted per-component below).

---

## Design Tokens

### Colors
All colors are expressed as GTK4 `alpha(@variable, n)` tokens in `zeshicast-gtk4.css`. For hardcoded values:

| Token | Value | Usage |
|---|---|---|
| Window background | `#13131C` | Launcher window |
| Desktop background | `#0A0A11` | Body / stage |
| Accent (default) | `#8B7CF8` | Selection, badges, highlights |
| Danger | `#FF6B5F` | Kill button, critical metrics |
| Warning | `#F5A623` | Medium metrics (CPU >60%) |
| Success | `#4BD98A` | Copy confirmation |
| Text primary | `rgba(255,255,255,0.93)` | Titles, values |
| Text secondary | `rgba(255,255,255,0.48)` | Subtitles, labels |
| Text muted | `rgba(255,255,255,0.28)` | Section headers, timestamps |
| Border subtle | `rgba(255,255,255,0.07)` | Card borders, dividers |
| Border strong | `rgba(255,255,255,0.12)` | Window outer ring |
| Surface | `rgba(255,255,255,0.038)` | Cards, rows background |
| Surface raised | `rgba(255,255,255,0.08)` | Selected rows |
| Inset accent | `inset 3px 0 0 <accent>` | Left border on selected rows |

### Typography
| Role | Font | Size | Weight | Letter-spacing |
|---|---|---|---|---|
| Search input | Outfit / Noto Sans | 17px | 400 | -0.02em |
| Result title | Outfit / Noto Sans | 15px | 500 | -0.012em |
| Result subtitle | Outfit / Noto Sans | 12px | 400 | normal |
| Section header | Outfit / Noto Sans | 11px | 600 | +0.075em, uppercase |
| Code / monospace | JetBrains Mono / Fira Code | 12–13px | 400 | -0.01em |
| Dashboard clock | Outfit / Noto Sans | 44px | 700 | -0.035em |
| Metric value | Outfit / Noto Sans | 26px | 700 | -0.025em |
| Status strip | Outfit / Noto Sans | 11–12px | 500–600 | -0.01em |

### Spacing
| Token | Value |
|---|---|
| Window border-radius | 14px |
| Card border-radius | 10px |
| Chip border-radius | 6–7px |
| Row height (compact) | 44px |
| Row height (comfortable) | 52px |
| Window width | 680px |
| Horizontal padding | 14px |
| Section header padding | 10px top, 3px bottom |

### Shadows
```
Window:
  box-shadow:
    0 0 0 1px rgba(255,255,255,0.09),
    inset 0 1px 0 rgba(255,255,255,0.06),
    0 4px 12px rgba(0,0,0,0.28),
    0 18px 52px rgba(0,0,0,0.42),
    0 44px 100px rgba(0,0,0,0.54);
```
In GTK4, the compositor (Niri/KWin) handles window blur and depth. Use `box-shadow` in CSS for the inner ring only; the deep shadow layers are rendered by the compositor via `window-shadow` or layer-shell protocol.

---

## Window & Stage

- **Trigger:** Global hotkey `Super+Space` (configurable)
- **Window type:** GTK4 `GtkWindow` with `gtk_layer_shell` — layer `OVERLAY`, anchored to center, exclusive zone `-1`
- **Size:** 680px wide, height auto (min ~200px, max ~700px)
- **Position:** Centered horizontally, ~12vh from top
- **Border-radius:** 14px — set via `GtkWindow { border-radius: 14px }` CSS + `gtk_window_set_decorated(false)`
- **Background:** Semi-opaque `#13131C` with compositor blur behind (request via `wl_surface` blur hint if compositor supports it)
- **Open animation:** `scale(0.96) translateY(-5px) opacity(0)` → `scale(1) translateY(0) opacity(1)`, 130ms, `cubic-bezier(0.34, 1.56, 0.64, 1)`
- **Close:** Reverse of open, triggered by Escape or focus-out

---

## Structure

```
GtkWindow
└── GtkBox (vertical)
    ├── SearchBar          (60px fixed height)
    ├── ContentArea        (max 344px, scrollable)
    │   └── [active view]
    ├── ActionBar          (38px fixed height)
    └── StatusStrip        (≈38px)
```

The **ActionPanel** overlays the ContentArea absolutely when `Ctrl+K` is pressed.

---

## Screen 1: Search View (default)

The main view. Shows grouped results as the user types.

### Search Bar
- Height: 60px, `padding: 0 14px`
- `GtkEntry` — no border, transparent background, 17px Outfit, caret color = accent
- Placeholder: `"Search for apps and commands…"`
- Right side: `Ctrl+K` hotkey badge (11px monospace, `rgba(255,255,255,0.38)`, pill with border)
- When a sub-view is active: replace with a Back button (`‹ ViewName`) + hide input
- When `=` prefix detected: show "Calculator" mode badge left of input

### Results List
- Groups: **Favourites**, **Recent**, **Command Center**, **Applications**
- When query is empty: show all groups with section headers
- When filtering: hide section headers, flatten results
- Section header: 11px, 600 weight, 0.075em tracking, uppercase, `rgba(255,255,255,0.28)`, `padding: 10px 14px 3px`

### Result Row
- Height: 44px (compact) or 52px (comfortable) — user configurable
- Layout: `[icon 18px] [title + subtitle flex] [pill if selected]`
- **App icon:** rounded square (`border-radius: ~26% of size`), colored bg + letter, `border: 1px solid rgba(255,255,255,0.055)`
- **Command icon:** circle, `background: rgba(255,255,255,0.055)`, symbol glyph
- **Selected state:** `background: rgba(255,255,255,0.08)`, `box-shadow: inset 3px 0 0 <accent>`
- **Hover state:** `background: rgba(255,255,255,0.04)`
- **Category pill** (visible only when selected): `padding: 2px 7px`, `background: rgba(255,255,255,0.055)`, 11px, "Application" or "Command"
- **Entrance animation:** `translateY(3px) opacity(0)` → normal, 130ms ease, staggered 9ms per row (max 5 rows)

### Calculator Mode (`=` prefix)
- Shows inline result below expression
- Expression: 12px monospace, muted
- Result: 24px bold monospace, primary color
- Copy badge: `⌃C` on right

### No Results
- Centered text: 13px, `rgba(255,255,255,0.26)`, `"No results for "<query>"`

---

## Screen 2: Dashboard

Accessed via Command Center → "Dashboard" or `▦` result.

### Layout
```
[Clock + date]
[Stat chips row]
[2×2 metric cards grid]
[3-column control cards]
```

### Clock
- 44px, 700 weight, -0.035em tracking, tabular-nums
- Blinking colon separator: `opacity: 0.3`, `animation: blink 1s step-end infinite`
- Date below: 13px, `rgba(255,255,255,0.36)`

### Stat Chips
- `padding: 4px 9px`, `border-radius: 7px`, `background: rgba(255,255,255,0.05)`, `border: 1px solid rgba(255,255,255,0.08)`
- Label: 11px, 500w, muted · Value: 12px, 600w, `rgba(255,255,255,0.76)`
- Items: Uptime, Battery, Procs, Workspace

### Metric Cards (2×2 grid, 7px gap)
- `padding: 13px 14px`, `border-radius: 10px`, `background: rgba(255,255,255,0.038)`, `border: 1px solid rgba(255,255,255,0.07)`
- Label: 11px, 600w, uppercase, 0.065em tracking
- Value: 26px, 700w, tabular-nums · Unit: 12px, muted
- Progress bar: 3px height, `border-radius: 2px`, color = accent (normal) / `#F5A623` (>65%) / `#FF6B5F` (>85%)
- Bar animates in on mount: width 0 → value, 600ms `cubic-bezier(0.34, 1.1, 0.64, 1)`, delayed 60ms + card delay
- Hover: `background: rgba(255,255,255,0.065)`, `translateY(-1px)`, 140ms

### Control Cards (3-column, 7px gap)
- Same card style as metric cards
- Icon: 26×26px, `border-radius: 7px`, accent-tinted when active
- Label: 11px, 600w, `rgba(255,255,255,0.45)` · Value: 14px, 600w · Sub: 11px, muted

---

## Screen 3: AI Chat

Accessed via Command Center → "AI Chat" or pressing `Tab` from search (carries query over).

### Layout
```
[Model selector bar]
[Messages scroll area, flex-1]
[Input row]
```

### Model Selector
- 3 buttons: `llama3.2:3b`, `mistral:7b`, `phi3:mini`
- Active: `background: <accent>22`, `border: 1px solid <accent>50`, color = accent
- Inactive: `background: rgba(255,255,255,0.04)`, muted
- Font: JetBrains Mono, 11px

### Message Bubbles
- User: `background: <accent>11`, `border: 1px solid <accent>20`, `border-radius: 10px 10px 3px 10px`, right-aligned
- Assistant: `background: rgba(255,255,255,0.04)`, `border: 1px solid rgba(255,255,255,0.07)`, `border-radius: 10px 10px 10px 3px`, left-aligned
- `padding: 8px 12px`, 13px, line-height 1.55

### Streaming Cursor
- 2px wide, 13px tall, accent color, `animation: blink 0.6s step-end infinite`
- Inline after streamed text

### Input Row
- `GtkTextView` (multiline), 14px, 28px height single-line
- Send button: 26×26px, `border-radius: 7px`, accent fill when input non-empty, `↑` glyph

---

## Screen 4: Clipboard History

Split-panel layout.

### Left Panel (216px wide)
- List of clipboard entries
- Row: 52px height, `padding: 0 12px`
- Type icon: 16×16px, `border-radius: 4px` — accent-tinted when selected
- Entry text: 12px, truncated to 1 line — monospace for code/url
- Timestamp: 10px, muted, below text
- Selected: `background: rgba(255,255,255,0.08)`, `inset 3px 0 0 <accent>`

### Right Panel (flex-1)
- **Meta bar** (top): type badge (accent-tinted) + character count + word count (text only) + timestamp
- **Content area:** `<pre>` or `GtkTextView` read-only, full text with word-wrap, 12–13px, monospace for code
- **Copy button** (bottom): full width, accent fill, "Copy to clipboard" label → "✓ Copied!" with `#4BD98A` tint on success, 1.4s timeout

---

## Screen 5: Emoji Picker

### Layout
```
[Search input]
[Category tab bar]
[Emoji grid]
[Copy confirmation strip]
```

- Search: standard search input, 14px
- Category tabs: horizontally scrollable, accent-tinted active tab
- Grid: `display: flex; flex-wrap: wrap; gap: 2px`, 36×36px emoji buttons, 22px emoji font size, `border-radius: 8px`
- Hover/active: `background: <accent>22`, `border: 1px solid <accent>50`
- On click: copy to clipboard, show confirmation strip at bottom

---

## Screen 6: Font Browser

### Layout
```
[Search + preview text input bar]
[Font list (scrollable)]
```

- Search: left half · Preview text input: right half (150px), editable
- Font row: 52px height, label (10px, uppercase, muted) + preview text rendered at 16px **in that font**
- Uses `GtkLabel` with `font-family` override per row, or `Pango` font description
- Hover: `background: rgba(255,255,255,0.04)`

---

## Screen 7: Network

### Layout
- List of Wi-Fi networks, each row 48px
- Signal bars: 4 bars, 3px wide each, heights 4/7/10/13px, filled = `rgba(255,255,255,0.72)`, empty = `rgba(255,255,255,0.14)`
- Connected row: `background: <accent>0A`, `inset 3px 0 0 <accent>`, name 600w
- Status text: accent color when connected, else muted + security indicator
- Connect/Disconnect button: right-aligned, 11px, `border-radius: 6px`

---

## Screen 8: Audio

### Layout
```
[Output section header]
[Output device list]
[Output volume row (mute btn + slider + value)]
[Input section header]
[Input device list]
[Input volume row]
```

- Device rows: 42px, radio dot indicator (7×7px circle), accent when active
- Volume slider: `GtkScale`, `accentColor: <accent>`, flex-1
- Mute button: 28×28px, `border-radius: 7px` — red tint when muted

---

## Screen 9: Media (Now Playing)

### Layout
```
[Album art (80×80px)] + [Title / Artist / Album]
[Scrubber (GtkScale)]
[Time labels row]
[Playback controls row]
```

- Album art: `border-radius: 11px`, gradient fill as placeholder
- Scrubber: full width `GtkScale`, updates every second while playing
- Controls: ⏮ ⏪ ⏸/▶ ⏩ ⏭ — play/pause is larger (42×42px accent circle), others 32×32px subtle

---

## Screen 10: Notifications

- Each notification: `minHeight: 56px`, `padding: 10px 14px`
- App icon: 32×32px, `border-radius: 8px`, emoji
- App name: 11px, muted · Timestamp: 10px, faint · Title: 13px, 500w · Body: 12px, muted, truncated
- Dismiss (×) button: 18×18px, appears on hover, `rgba(255,70,70,0.2)` bg
- Empty state: "All caught up ✓" centered, muted

---

## Screen 11: System Monitor

### Resource Overview Panel (top)

**CPU row:**
- Label (30px fixed) + value (38px, color-coded) + 8 per-core bars (6px wide each, 20px tall, `border-radius: 2px`) + SVG sparkline (114×28px)
- Per-core bars: fill from bottom, accent color, animated height
- Sparkline: SVG `<polyline>` + gradient fill `<polygon>`, redrawn on interval
- Color coding: normal = accent, >60% = `#F5A623`, >80% = `#FF6B5F`

**RAM row:**
- Segmented progress bar: used (red) + cached (accent at 50% opacity), 5px height
- Value: `X.X / 16 GB` + legend chips

**Network row:**
- ↓ and ↑ chips with live values in MB/s
- Disk I/O text right-aligned

**Update interval:** 1800ms

### Process Table
- Filter input in header (monospace, transparent)
- Sort buttons: CPU / MEM, right-aligned, accent when active + `↓` indicator
- Row: 34px height
  - Process name: flex-1, 12px monospace, truncated
  - Mini progress bar: 36×3px, width = `cpu/25 * 100%` capped at 100%
  - CPU%: 40px right-aligned, accent when >8%
  - MEM: 44px right-aligned, accent when >500MB
  - Kill (×): 16×16px, visible on row hover — `rgba(255,70,70,0.2)` bg, `#FF6B5F` text

---

## Screen 12: Preferences

Split layout: nav sidebar (138px) + settings pane (flex-1).

### Sidebar
- Sections: General, Appearance, Keyboard, Extensions, Privacy, About
- Row: 34px, selected = `background: rgba(255,255,255,0.07)`, `inset 3px 0 0 <accent>`

### Settings Pane
- Each field: `minHeight: 36px`, label (flex-1, 13px) + control (right)
- Control types:
  - **toggle:** `GtkSwitch` styled as pill, 34×18px, accent fill when on
  - **range:** `GtkScale`, 90px wide
  - **radio:** segmented button group, accent-tinted active
  - **text:** monospace value display (read-only in prototype; edit in real impl)
  - **info:** monospace read-only value

---

## Screen 13: Extensions

List of built-in extensions, each 52px row.

- Icon: 32×32px, `border-radius: 8px` — accent bg when enabled, muted when disabled
- Row opacity: 1.0 when enabled, 0.5 when disabled
- Toggle: `GtkSwitch` (pill style, see Preferences)
- Header shows active count: `"Built-in · N of M active"`

---

## Action Panel (overlay)

Triggered by `Ctrl+K`. Overlays the content area absolutely.

- **Slide-in animation:** `translateX(12px) opacity(0)` → normal, 120ms `cubic-bezier(0.25, 0.46, 0.45, 0.94)`
- Header: "ACTIONS" label + filter input + Esc button
- Sections: Primary, Manage, Danger
- Row: 38px, same selection style as result rows
- Danger section: `#FF6B5F` text
- Hotkey badges: monospace pill, `rgba(255,255,255,0.38)`

---

## Action Bar (footer)

- Height: 38px, `border-top: 1px solid rgba(255,255,255,0.06)`
- Left: icon buttons (⊟ ◈) — 26×26px, hover shows subtle bg
- Right: `···` more button → opens Action Panel
- Center: result count when overflow (`N of M ↕`)

---

## Status Strip (bottom)

- Height: ~38px, `padding: 7px 14px 10px`
- Time (12px, 600w, tabular-nums) + Date (11px, muted) on left
- Status chips on right: WiFi, Battery, Audio, Workspace
- Active chips: accent-tinted bg + border

---

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `Super+Space` | Open launcher |
| `Escape` | Close panel / go back / clear query |
| `↑` / `↓` | Navigate results |
| `Enter` | Launch selected item |
| `Tab` | Jump to AI Chat with current query |
| `Ctrl+K` | Open/close Action Panel |
| `=` prefix | Calculator mode |
| `/` prefix | File search mode (planned) |

---

## GTK4 Implementation Notes

- **No `backdrop-filter`** — request blur from compositor via `wl_surface` hints or Niri's `blur` config
- **No `clip-path`** — use `GtkPicture` with mask or `cairo_clip` in draw handler
- **No `::before`/`::after`** — use extra `GtkBox` children instead
- **`alpha(@window_fg_color, n)`** — GTK4's built-in color function; replaces `rgba()` for theme-aware values
- **`@accent_color`** — GTK4 named color, maps to user's system accent; override in app CSS for default
- **SVG Sparkline** — use `GtkDrawingArea` with Cairo; draw polyline + gradient fill in `draw` callback
- **Per-core bars** — `GtkBox` of `GtkProgressBar` widgets with custom CSS height
- **Token replacements** in `zeshicast-gtk4.css** — do a string-replace pass before loading:
  - `__FONT_SIZE__` → 15
  - `__SUBTITLE_SIZE__` → 12
  - `__SEARCH_SIZE__` → 17
  - `__ROW_HEIGHT__` → 44 (compact) or 52 (comfortable)
  - `__DASHBOARD_CLOCK_SIZE__` → 44

---

## Files in This Package

| File | Description |
|---|---|
| `zeshicast.html` | Hi-fi interactive prototype — open in browser to explore all screens |
| `zeshicast-gtk4.css` | GTK4 CSS stylesheet with token placeholders, ready for Rust str::replace |
| `README.md` | This document |

---

## How to Use This in Claude Code

1. Open a Claude Code session in your `zeshicast` Rust project root
2. Attach this `README.md` and `zeshicast-gtk4.css`
3. Say: _"Implement the UI described in README.md. Use the GTK4 CSS from zeshicast-gtk4.css after replacing the `__TOKEN__` placeholders. Start with the window shell and search bar, then add views one by one."_
4. Reference specific screens by name (e.g. "now implement the System Monitor screen")
