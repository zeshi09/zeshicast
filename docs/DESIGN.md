# Zeshicast Design Brief

Design a native Linux keyboard launcher and command center built with Rust and
GTK. It is inspired by Raycast v2 and Vicinae, but should feel Linux-native,
local-first, dense, calm, and keyboard-driven.

The first screen is always root search. Do not make a landing page.

## Mood

Quiet Linux cockpit. Dark native command surface. Dense rows. Precise
typography. Subtle separators. Useful live status. No decorative background.

## Core Shell

```text
+----------------------------------------------------------+
| Search apps, commands, files, snippets, AI...            | 60
+----------------------------------------------------------+
| Favourites                                               | 28
| [icon] Firefox              browser app          [App]   | 52
| [icon] AI Chat              local model          [Core]  | 52
| [icon] Lock Screen          loginctl             [Sys]   | 52
|                                                          |
| Command Center                                           |
| [icon] Dashboard            system overview      [Core]  |
| [icon] Network              Wi-Fi / VPN / DNS    [Core]  |
+----------------------------------------------------------+
| [⊟] [◈] [⎘] [↵]        Actions ⌃K                        | 38
+----------------------------------------------------------+
| 14:32:08  Wi-Fi up  Battery 82%  Vol 48%                 |
+----------------------------------------------------------+
```

Note: the footer shipped as a compact icon action bar (`.action-bar`, 38px
min-height): icon buttons for Show in Files / Pin / Copy / Run expose their
shortcuts via tooltips, and only the `Actions ⌃K` button carries a text label.
The status strip is padding-based (no fixed height). The earlier plan to move
to fully labelled text+shortcut footer actions was superseded by the Raycast
v2 handoff.

## Tokens

Base color roles are delegated to the active GTK theme (`@window_bg_color`,
`@window_fg_color`, `@accent_color`, ...), so the launcher follows the user's
system theme instead of shipping its own surface/text palette. Only four
accent aliases are defined in code (`src/ui/style.rs`):

```text
ac_purple     #8B7CF8   (accent)
ac_amber      #F5A623   (warning)
ac_green      #4BD98A   (success)
ac_red        #FF6B5F   (danger)
```

The source of truth for colors is the Raycast v2 design handoff CSS
(`design_handoff_zeshicast/zeshicast-gtk4.css`).

Typography:

- Font stack: `Outfit, Inter, Noto Sans, sans-serif`
- Base: 15px
- Search: 17px
- Secondary: 12px
- Section: 12px semibold

Geometry:

- Window: 900x760 (`default_width` / `default_height` in `src/ui/launcher.rs`)
- Window frame radius: 14px (`.launcher-frame`)
- Search header: 60px
- Root row: 52px comfortable, 44px compact
- Section header: 28px
- Action bar (footer): 38px min-height
- Status strip: padding-based, no fixed height
- Cards: radius 10px

## Components

CommandRow:

```text
[Icon 28] [Title][Muted subtitle]              [Accessory]
```

States: normal, hover, selected, disabled, warning, destructive.

SectionHeader:

```text
Favourites
Recent
Command Center
```

FooterAction:

```text
Run Enter
Actions Ctrl+K
Copy Ctrl+Enter
```

MetricCard:

```text
+--------------------------+
| Label              Value |
| subtitle                 |
| [progress/sparkline]     |
+--------------------------+
```

ControlCard:

```text
+--------------------------+
| Title                    |
| current state            |
| [icon] [icon] [icon]     |
+--------------------------+
```

Composer:

```text
[context chips]
[input]
[Use Clipboard] [Save] [Ask]
```

## Screens

Root Search:

- Empty query: Favourites, Recent, Command Center.
- Query: ranked results, grouped only when useful.
- No results: fallback suggestions `shell`, `ai`, `file`, `clip`, `translate`.

Action Panel:

- Header with selected action.
- Search actions input.
- Sections: Primary, Manage, Clipboard, Danger.
- Alias/hotkey assignment should become first-class.

Dashboard:

```text
+----------------------------------------------------------+
| 14:32:08                                      Sun, 31 May |
+----------------------------------------------------------+
| [CPU 32% graph] [Memory 61% graph]                       |
| [Disk 48% graph] [Temp 54 C cpu]                         |
| [Network state + controls] [Audio state + controls]      |
| [Media controls]          [Notifications controls]       |
+----------------------------------------------------------+
```

System Monitor:

- Metric cards for load, memory, disk, temp.
- Scrollable process list.
- Per-process memory bars.
- Terminate selected process action.

Network:

- Sections: Interfaces, DNS, Available Wi-Fi, Active VPN.
- Row actions operate only on compatible rows.

AI Chat:

- Header with provider/model chip.
- Scrollable answer area.
- Bottom composer stays visible.
- Use clipboard, copy answer, save snippet.

Settings:

- Future two-pane layout with sections:
  General, Appearance, Features, AI, Integrations, Extensions.

## Acceptance Criteria

- Text never clips with Linux fallback fonts.
- Section headers are not selectable.
- Keyboard navigation skips non-action rows.
- Selected/hover states never change row height.
- Dashboard cards fit inside 900x760.
- No nested cards.
- No gradient/orb decoration.
- Root search remains the first screen.

## Implementation Order

1. Finish root grouping: Favourites, Recent, Command Center.
2. Footer actions (superseded decision: the compact icon action bar was kept
   in the final design; see the note under Core Shell).
3. Introduce `MetricCard` and `ControlCard` GTK helpers.
4. Rebuild Dashboard on cards.
5. Rebuild Action Panel with sections.
6. Move AI Chat input into bottom composer.
7. Redesign Settings into searchable/two-pane view.
