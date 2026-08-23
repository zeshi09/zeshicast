# Privacy

This document lists what zeshicast stores locally and how to control retention.

## Stored Data

Zeshicast stores data under the current user's home directory.

| Path | Contents |
| --- | --- |
| `~/.config/zeshicast/preferences.toml` | UI, integration, privacy, AI, translation, and extension preferences. May contain API keys. |
| `~/.config/zeshicast/zeshicast.db` | SQLite database for clipboard history and action usage history. Uses schema migrations via `PRAGMA user_version` and WAL journal mode; the `zeshicast.db-wal` and `zeshicast.db-shm` sidecar files hold transient WAL state for the same data. |
| `~/.cache/zeshicast/clipboard/` | Cached PNG files for image clipboard entries. |
| `~/.config/zeshicast/commands/*.toml` | Custom command definitions and per-command defaults. |
| `~/.config/zeshicast/extensions/*/extension.toml` | Local extension metadata, version, capabilities, and command/script file lists. |
| `~/.config/zeshicast/extensions/*/` | Local extension command TOMLs and scripts referenced by extension manifests. |
| `~/.config/zeshicast/quicklinks.txt` | User quicklinks. |
| `~/.config/zeshicast/snippets.txt` | User snippets. |
| `~/.config/zeshicast/aliases.txt` | User aliases. |
| `~/.config/zeshicast/pins.txt` | User pins. |
| `~/.config/zeshicast/calc_history.json` | Recent calculator results. |
| `~/.config/zeshicast/dnd` | One-byte do-not-disturb flag (`1` or `0`) persisted by the built-in notification daemon so DND survives restarts. |
| `~/.local/share/fonts/zeshicast/` | The bundled Outfit and JetBrains Mono font files, unpacked on first launch (no user data). |

Legacy text files such as `clipboard.txt`, `recent.txt`, and
`frequencies.txt` may be read during migration to SQLite, but active clipboard
and usage history are stored in SQLite.

## Clipboard History

The GTK daemon records clipboard text and, when enabled, image clipboard entries.
Image entries store a sentinel row in SQLite plus a PNG file in
`~/.cache/zeshicast/clipboard/`.

Privacy preferences:

- `clipboard_history_enabled = "false"` disables installing clipboard watchers
  on startup.
- `clipboard_private_mode = "true"` pauses recording new clipboard entries
  without deleting existing history.
- `clipboard_capture_images = "false"` keeps text capture enabled but skips
  image PNG caching.
- `clipboard_retention = "100"` controls how many clipboard rows are kept.
  Pruning runs when new entries are inserted and when the setting is changed
  through the GTK UI or CLI. If you lower the value by editing
  `preferences.toml` manually while the daemon is stopped, rows above the new
  limit are removed on the next insert or preference write rather than
  immediately at startup.

When clipboard rows are pruned, unreferenced cached image PNGs are pruned too.
Clearing clipboard history deletes SQLite clipboard rows and cached PNG files.
Deleting an individual image entry removes the cached PNG when no remaining row
references it.

## Notification History

When enabled, zeshicast can own `org.freedesktop.Notifications` and record
notifications into its in-memory notification store for the current daemon
session.

Controls:

- `notifications_enabled = "false"` hides notification features.
- `notifications_history_enabled = "false"` prevents starting the built-in
  notification server.

Only one notification daemon can own `org.freedesktop.Notifications`. If another
daemon owns it, zeshicast cannot record that notification stream.

## AI And Translation Requests

AI and translation actions send the prompt text to the configured endpoint. For
remote providers, that endpoint may receive user prompts and any API key required
for the request. Local Ollama-style endpoints keep requests on the configured
host.

## Export Behavior

`zeshicast --export [file]` creates a config archive with secret-like preference
keys removed by default. Keys ending in `_api_key` or containing `secret`,
`token`, or `password` are excluded.

Use `zeshicast --export [file] --include-secrets` only when the destination is a
trusted backup location. Import validates archive paths and rejects symlinks
before replacing the config directory.

## File Permissions

Sensitive local state is written with `0600` on Unix where applicable:

- `preferences.toml`;
- `aliases.txt`;
- `pins.txt`;
- `snippets.txt`;
- `calc_history.json`;
- `zeshicast.db`.

Writes for these files are atomic where possible: zeshicast writes a temporary
file in the same directory, syncs it, then renames it over the target.
