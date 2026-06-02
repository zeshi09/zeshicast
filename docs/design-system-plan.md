# Zeshicast Design System Plan

Goal: define a production-ready visual direction for Zeshicast as a Linux
command center inspired by Raycast v2, Vicinae, and Stitch-style design-system
handoff. This is not a pixel clone. It is a native Rust/GTK design plan focused
on speed, scanability, keyboard-first use, and Linux control surfaces.

## References

- Raycast v2: fresh visual language, cleaner compact mode, root search with
  favourites, root file search, Action Panel aliases/hotkeys, refreshed
  Clipboard History, Quick AI composer, AI Chat, and richer settings search.
- Vicinae: fixed-size launcher shell, 60px search header, dense list delegates,
  section headers, footer actions, status bar, theme tokens, and action panel
  popovers.
- Stitch workflow: start from intent and feeling, extract a reusable design
  system, define screens and components in a DESIGN.md-like document, then
  iterate quickly into implementation.

## Product Feel

Zeshicast should feel like a quiet Linux cockpit:

- Fast: the first screen is always search, never a marketing dashboard.
- Dense: information is compact, aligned, and keyboard-scannable.
- Native: GTK controls, system icon names, Wayland-friendly layout.
- Capable: root search, control center, process monitor, network/media panels,
  snippets, clipboard, extensions, and local AI all feel like parts of one app.
- Calm: no decorative gradients, no floating page cards, no oversized hero UI.

## Design Tokens

### Window

- Width: 860px default.
- Height: 600px default.
- Border radius: 12px.
- Outer border: 1px, subtle foreground alpha.
- Background: near-system window background with 0.985 opacity.
- Shadow: soft, deep, single shadow; no colored glow.

### Spacing

- Base unit: 4px.
- Search horizontal padding: 14-16px.
- List row horizontal padding: 14-16px.
- View page margin: 14px.
- Section gap: 8px.
- Control gap: 8px.

### Typography

- Default stack: `Outfit, Inter, Noto Sans, sans-serif`.
- Base size: 15px.
- Search: base + 2px.
- Secondary text: base - 3px, minimum 11px.
- Section headers: secondary size, 600 weight.
- Title labels: 500 weight, one line.
- Avoid negative letter spacing.
- Avoid multi-line row titles in root search.

### Color Roles

Use semantic roles rather than hard-coded theme names:

- `window`: main background.
- `surface`: slightly raised panels and cards.
- `surface_muted`: lower emphasis areas like footer/status.
- `border`: subtle separators.
- `text`: primary text.
- `text_muted`: secondary metadata.
- `accent`: selected row, progress, primary controls.
- `danger`: destructive actions.
- `success`: healthy status.
- `warning`: degraded status.

Default dark palette:

```text
window        #111216
surface       #17191f
surface_muted #14161b
border        #2a2d36
text          #eceff4
text_muted    #9aa3b2
accent        #8ab4f8
danger        #ff6b5f
success       #6dd58c
warning       #f4c76b
```

## Shell Layout

Root shell structure:

```text
+----------------------------------------------------------+
| Search input                                             | 60
+----------------------------------------------------------+
| Section header                                           | 28
| Result row                                               | 52
| Result row                                               | 52
| Result row                                               | 52
| ... scroll                                               |
+----------------------------------------------------------+
| Footer actions: Enter Run   Ctrl+K Actions   Ctrl+Enter  | 40
+----------------------------------------------------------+
| Status strip: time net battery volume media              | 34
+----------------------------------------------------------+
```

Notes:

- Search header is visually separated by a thin divider.
- Root list scrolls independently.
- Footer actions and status strip are separate surfaces.
- Section headers are non-selectable rows.
- Selected rows never change height.

## Root Search Template

Root search should use sections whenever the query is empty or broad:

```text
+----------------------------------------------------------+
| Search apps, commands, files, snippets, AI...            |
+----------------------------------------------------------+
| Favourites                                               |
| [icon] Firefox                         App               |
| [icon] AI Chat                         Zeshicast         |
| [icon] Dashboard                       Zeshicast         |
| [icon] Lock Screen                     System            |
|                                                          |
| Recent                                                   |
| [icon] Project README                  File              |
| [icon] Translate selected text         Command           |
|                                                          |
| Command Center                                           |
| [icon] Network                         Zeshicast         |
| [icon] Media                           Zeshicast         |
| [icon] Notifications                   Zeshicast         |
+----------------------------------------------------------+
```

Result row anatomy:

```text
[28 icon]  Title  muted subtitle................  [Category]
```

Rules:

- Title and subtitle share one horizontal baseline.
- Subtitle disappears first when width is constrained.
- Category/accessory stays right-aligned.
- Icons are 28px in root search, 20-24px in secondary views.
- A row is selectable only if it maps to an action.

## Action Panel Template

Action Panel should feel like a command palette inside the launcher, not a
separate dialog.

```text
+----------------------------------------------------------+
| Action Panel                                             |
| Firefox                                                  |
| Launch /usr/bin/firefox                                  |
+----------------------------------------------------------+
| Search actions                                           |
+----------------------------------------------------------+
| Primary                                                  |
| [>] Run                                                  |
|                                                          |
| Manage                                                   |
| [pin] Pin                                                |
| [tag] Set Alias                                          |
| [copy] Copy Value                                        |
| [folder] Open Containing Folder                          |
+----------------------------------------------------------+
```

Rules:

- Actions are grouped by semantic section: Primary, Manage, Clipboard, Danger.
- Destructive actions use `danger` text/icon role.
- Alias/hotkey controls should be first-class, matching Raycast v2's direction
  of assigning aliases/hotkeys from the Action Panel.

## Dashboard Template

Dashboard is a control center, not a home page.

```text
+----------------------------------------------------------+
| 14:32:08                                      Sun, 31 May |
+----------------------------------------------------------+
| [CPU Load  32%     mini graph] [Memory  61%   mini graph] |
| [Disk      48%     mini graph] [Temp    54 C  cpu]        |
+----------------------------------------------------------+
| Network                   | Audio                         |
| Wi-Fi up  192.168.1.10    | Output 48%  Mic muted         |
| DNS 1.1.1.1               | [Mute] [Settings]             |
+----------------------------------------------------------+
| Media                     | Notifications                 |
| Spotify Playing Track     | DND off  12 history           |
| [Prev] [Play] [Next]      | [DND] [Close All] [Panel]     |
+----------------------------------------------------------+
| [Network] [Media] [System] [Notify] [AI]                  |
+----------------------------------------------------------+
```

Card rules:

- Cards are repeated information blocks, radius 8px max.
- No card inside another card.
- Cards use uniform padding: 12px.
- Metric cards expose value, status, and optional tiny progress/sparkline.
- Quick controls are icon buttons when the action is familiar.

## System Monitor Template

```text
+----------------------------------------------------------+
| System Monitor                                           |
+----------------------------------------------------------+
| [Load graph........] 0.81 / 16 cores                      |
| [Memory graph......] 61%  9.8 / 16.0 GiB                  |
| [Disk graph........] 48%  450 / 930 GiB                   |
| [Temp graph........] 54.0 C cpu                           |
+----------------------------------------------------------+
| Top Processes                                            |
| [icon] firefox        pid 1234  1420 MiB RSS [bar]        |
| [icon] code           pid 2222   980 MiB RSS [bar]        |
| [icon] rust-analyzer  pid 3333   620 MiB RSS [bar]        |
+----------------------------------------------------------+
| [Terminate selected]                                      |
+----------------------------------------------------------+
```

Future graph rules:

- Store a small rolling history in memory per visible view.
- Use simple GTK drawing area or progress bars first.
- Do not poll process lists while root search is active.

## Network View Template

```text
+----------------------------------------------------------+
| Network                                                  |
+----------------------------------------------------------+
| Interfaces                                               |
| [wifi] wlan0   up     192.168.1.10/24                    |
| [eth]  enp4s0  down   aa:bb:cc:dd:ee:ff                  |
|                                                          |
| DNS                                                      |
| [dns] 1.1.1.1, 8.8.8.8                                   |
|                                                          |
| Available Wi-Fi                                          |
| [wifi] Home       87%  WPA2                              |
| [wifi] Cafe       42%  open                              |
|                                                          |
| Active VPN                                               |
| [vpn] Work VPN   vpn active                              |
+----------------------------------------------------------+
| [Connect] [Disconnect] [Copy IP] [Copy MAC]              |
+----------------------------------------------------------+
```

Rules:

- Sections are explicit.
- Interface actions operate only on interface rows.
- Wi-Fi actions operate only on Wi-Fi rows.
- Disable or no-op invalid actions rather than guessing row offsets.

## Media View Template

```text
+----------------------------------------------------------+
| Media                                                    |
+----------------------------------------------------------+
| Track Title                                              |
| Artist - Album                                           |
| Spotify  Playing                                        |
|                                                          |
| [Previous] [Play/Pause] [Next]                           |
+----------------------------------------------------------+
| Output                                                   |
| Speakers  48%                                           |
+----------------------------------------------------------+
```

## Notifications View Template

```text
+----------------------------------------------------------+
| Notifications                                            |
+----------------------------------------------------------+
| Backend dunst     History 12     DND off                 |
| [DND] [Close All] [Panel]                                |
+----------------------------------------------------------+
| History                                                  |
| [app] Mail        New message       Project update       |
| [app] Browser     Download complete ~/Downloads/file     |
+----------------------------------------------------------+
```

## Clipboard View Template

Raycast v2 refreshed Clipboard History and emphasizes source/original context.
Zeshicast should move there incrementally:

```text
+----------------------------------------------------------+
| Clipboard History                                        |
+----------------------------------------------------------+
| [text] API token preview...          2 min ago  Text     |
| [file] 3 files copied                8 min ago  Files    |
| [url]  https://example.com           1 h ago    Browser  |
+----------------------------------------------------------+
| Enter Copy   Delete Remove   Ctrl+Delete Clear           |
+----------------------------------------------------------+
```

Future fields:

- source app,
- MIME/source type,
- custom name,
- grouped file copies.

## AI Chat Template

Raycast v2 points toward a richer Quick AI composer and AI Chat with memory,
skills, agents, and tool details. Zeshicast should keep this local-first:

```text
+----------------------------------------------------------+
| AI Chat                                      local model  |
+----------------------------------------------------------+
| Answer area                                               |
|                                                          |
| ...                                                      |
+----------------------------------------------------------+
| Context: clipboard                                       |
| Ask local AI...                                          |
+----------------------------------------------------------+
| [Use Clipboard] [Copy] [Save Snippet]              [Ask] |
+----------------------------------------------------------+
```

Future:

- Streaming answer.
- Stop/cancel.
- Conversation list.
- Local memory profile.
- Tool call disclosure.

## Settings Template

Raycast v2 adds search in settings sidebar. Zeshicast settings should become a
two-pane view:

```text
+----------------------------------------------------------+
| Settings search                                          |
+--------------------+-------------------------------------+
| General            | UI font family                      |
| Features           | UI font size                        |
| AI                 | Status strip items                  |
| Network            | Dashboard refresh interval          |
| Extensions         | ...                                 |
+--------------------+-------------------------------------+
| [Cancel] [Save]                                          |
+----------------------------------------------------------+
```

## Card Components

### MetricCard

Use for CPU, memory, disk, temp, battery:

```text
+--------------------------+
| Label              Value |
| subtitle                 |
| [progress/sparkline]     |
+--------------------------+
```

### ControlCard

Use for network/audio/media/notifications:

```text
+--------------------------+
| Title                    |
| current state            |
| [icon] [icon] [icon]     |
+--------------------------+
```

### StatusChip

Small text accessory in rows/status strip:

```text
[Category] [DND off] [Wi-Fi up] [48%]
```

Rules:

- Chip height: 18-22px.
- Radius: 6px.
- Text size: secondary.
- No more than 3 chips in one root row.

## GTK Implementation Plan

1. Root list grouping
   - Add section rows for empty query: Favourites, Recent, Command Center.
   - Keep non-selectable section headers.
   - Maintain row-to-action mapping independent from raw row index.

2. Row primitives
   - Split `result_row` into root row, section header, compact secondary row.
   - Use horizontal title/subtitle/accessory layout for root search.
   - Keep two-line rows only in detail views where content needs it.

3. Footer
   - Replace loose icon action bar with a footer row:
     `Enter Run`, `Ctrl+K Actions`, `Ctrl+Enter Copy`.
   - Keep icon buttons available but visually label primary actions.

4. Dashboard cards
   - Introduce `MetricCard` and `ControlCard` helpers in `ui/widgets.rs`.
   - Rebuild dashboard with a 2-column grid-like GTK layout.
   - Keep cards at radius 8px or less.

5. Settings redesign
   - Add settings search entry.
   - Group preferences by section.
   - Keep write behavior unchanged.

6. AI Chat composer
   - Move input to bottom composer area.
   - Add context chip for clipboard.
   - Add model/provider chip in header.

7. Theme tokens
   - Move CSS values into preferences-ready semantic tokens.
   - Add `ui_density = compact|comfortable`.
   - Add `ui_theme = system|dark|light`.

## Non-Goals

- Do not clone Raycast or Vicinae pixel-for-pixel.
- Do not introduce web UI inside GTK.
- Do not add decorative backgrounds, gradient orbs, or marketing sections.
- Do not make dashboard the first screen.
- Do not make cards nested.

## Source Notes

- Raycast v2 manual highlights the refreshed look, compact mode, root search
  favourites, root file search, Action Panel alias/hotkey assignment,
  refreshed Clipboard History, Quick AI composer, AI Chat memory/skills/agents,
  and settings search.
- Vicinae QML shows the target shell structure: 60px search header, stack
  content area, status/footer row, dense 41px list delegates, section headers,
  and theme-driven semantic colors.
- Stitch's public design direction is useful as a workflow: describe intent,
  extract a design system, keep an agent-friendly design document, and iterate
  from screen templates into implementation.

## Stitch Design Brief

Use this section as a Stitch-ready source prompt.

### Master Prompt

Design a native Linux keyboard launcher and command center named Zeshicast.
It is built with Rust and GTK, inspired by Raycast v2 and Vicinae, but it must
feel Linux-native and local-first rather than a clone. The first screen is a
fast root search, not a landing page. The product includes app/file launch,
system controls, compositor/window actions, clipboard history, snippets,
extensions, dashboard/status widgets, network/media/notification views, process
monitoring, and local AI chat.

Create a dark, dense, calm command-center interface with a 60px top search
header, scrollable root results grouped by sections, compact result rows with
icon/title/muted subtitle/accessory, bottom footer actions, a separate status
strip, and metric/control cards for dashboard views.

Prioritize scanability, predictable keyboard interaction, stable dimensions,
and text that never clips even with Linux font fallbacks. Do not create a
website, hero screen, decorative gradient/orb background, oversized marketing
cards, nested cards, or mobile bottom navigation.

### Screens To Generate

- Root Search Empty State.
- Root Search With Query.
- Action Panel.
- Dashboard.
- System Monitor.
- Network View.
- Notifications View.
- Clipboard History.
- AI Chat.
- Settings.

### Visual Mood

Quiet Linux cockpit, dark translucent native window, dense command list, subtle
separators, precise typography, useful status signals, calm professional
surface.

### Negative Prompt

Do not clone Raycast or Vicinae pixel-for-pixel. Do not use glassy decorative
blobs, one-note purple/blue gradients, beige/brown palettes, stock imagery, or
landing-page composition. The app must look like a tool opened dozens of times
per day.

## Component States

### CommandRow

Use in Root Search, Extensions, Snippets, Clipboard, Network, and process lists.

```text
[Icon 28] [Title][Subtitle muted]                     [Accessory]
```

States:

- normal
- hover
- selected
- disabled
- warning
- destructive

Acceptance criteria:

- Height is stable across all states.
- Title and subtitle elide instead of clipping.
- Accessory remains right-aligned.
- Section headers are not selectable.

### SectionHeader

Non-selectable row for list groups:

```text
Favourites
Recent
Command Center
Interfaces
Available Wi-Fi
History
```

### FooterAction

Footer actions should be readable text+shortcut items, not only icon buttons:

```text
Run  Enter
Actions  Ctrl+K
Copy  Ctrl+Enter
```

### MetricCard

Use for CPU, memory, disk, temp, battery:

```text
+--------------------------+
| Label              Value |
| subtitle                 |
| [progress/sparkline]     |
+--------------------------+
```

States:

- normal
- warning
- critical
- unavailable

### ControlCard

Use for network, audio, media, notifications:

```text
+--------------------------+
| Title                    |
| current state            |
| [icon] [icon] [icon]     |
+--------------------------+
```

### Composer

Use for AI Chat and future Quick AI:

```text
[context chips]
[input]
[Use Clipboard] [Save] [Ask]
```

States:

- idle
- sending
- streaming
- cancelled
- error

## Screen Inventory

| Screen | Primary job | Components |
| --- | --- | --- |
| Root Search | Find and run anything | SearchHeader, SectionHeader, CommandRow, FooterAction, StatusStrip |
| Action Panel | Choose secondary action | SearchHeader, SectionHeader, CommandRow |
| Dashboard | Glance/control PC state | MetricCard, ControlCard, StatusChip |
| System Monitor | Inspect/terminate processes | MetricCard, CommandRow, process bars |
| Network | Inspect/copy/connect network | SectionHeader, CommandRow, FooterAction |
| Media | Inspect/control playback | ControlCard, icon buttons |
| Notifications | DND/history controls | StatusChip, CommandRow |
| Clipboard | Browse/copy/delete history | CommandRow, FooterAction |
| Snippets | Browse/copy snippets | CommandRow, SectionHeader |
| Extensions | Inspect command catalog | CommandRow, permission chips |
| AI Chat | Ask local model quickly | Composer, answer panel, StatusChip |
| Settings | Configure app | SearchHeader, sidebar, form rows |

## Stitch Acceptance Criteria

Root Search:

- Empty query shows grouped sections and first actionable row selected.
- Keyboard navigation skips section headers.
- Row height does not change on hover or selection.
- Long app names elide, not clip.
- Linux font fallback does not cut glyph tops or bottoms.

Dashboard:

- No page-level floating cards.
- Cards are only repeated metric/control blocks.
- All controls fit inside 860x600.
- Status updates do not resize the layout.

System Monitor:

- Process list remains scrollable.
- Kill action only targets selected process rows.
- Graph/progress bars do not force row height changes.

Settings:

- Preference labels do not overflow into inputs.
- Appearance preferences include font and density.
- Settings can later become searchable/two-pane.

AI Chat:

- Asking local AI never blocks GTK main loop.
- Composer remains visible while answer area scrolls.
- Model/provider/context are visible as chips.

## Open Design Decisions

- Should root search always show grouped sections, or only when query is empty?
- Should Dashboard keep footer/status strip visible while open?
- Should compact mode reduce window height to search-only until navigation?
- Should quick AI answers appear inline in Root Search or always open AI Chat?
- Should theme tokens follow GTK theme by default or ship a Zeshicast dark
  palette independent from GTK?
