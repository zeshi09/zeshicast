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
| Run Enter      Actions Ctrl+K      Copy Ctrl+Enter       | 40
+----------------------------------------------------------+
| 14:32:08  Wi-Fi up  Battery 82%  Vol 48%                 | 34
+----------------------------------------------------------+
```

## Tokens

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

Typography:

- Font stack: `Outfit, Inter, Noto Sans, sans-serif`
- Base: 15px
- Search: 17px
- Secondary: 12px
- Section: 12px semibold

Geometry:

- Window: 860x600
- Search header: 60px
- Root row: 52px comfortable, 44-48px compact
- Section header: 28px
- Footer: 40px
- Status strip: 34px
- Cards: radius 8px max

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
- Dashboard cards fit inside 860x600.
- No nested cards.
- No gradient/orb decoration.
- Root search remains the first screen.

## Implementation Order

1. Finish root grouping: Favourites, Recent, Command Center.
2. Replace icon-only footer with text+shortcut footer actions.
3. Introduce `MetricCard` and `ControlCard` GTK helpers.
4. Rebuild Dashboard on cards.
5. Rebuild Action Panel with sections.
6. Move AI Chat input into bottom composer.
7. Redesign Settings into searchable/two-pane view.
