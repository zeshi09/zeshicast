# Privacy

This document lists what zeshicast stores locally and how to control retention.

## Stored Data

Zeshicast stores data under the current user's home directory.

| Path | Contents |
| --- | --- |
| `~/.config/zeshicast/preferences.toml` | UI, integration, privacy, AI, translation, and extension preferences. May contain API keys. |
| `~/.config/zeshicast/zeshicast.db` | SQLite database for clipboard history, action usage history, the extension permission log and the file index (paths of files and folders under `$HOME`, see below). Uses schema migrations via `PRAGMA user_version` and WAL journal mode; the `zeshicast.db-wal` and `zeshicast.db-shm` sidecar files hold transient WAL state for the same data. |
| `~/.config/zeshicast/db.key` | The database key (32 bytes, 0600), when it is not kept in the keyring; see "Encryption at rest". |
| `~/.cache/zeshicast/clipboard/` | Cached PNG files for image clipboard entries. |
| `~/.config/zeshicast/commands/*.toml` | Custom command definitions and per-command defaults. |
| `~/.config/zeshicast/extensions/*/extension.toml` | Local extension metadata, version, capabilities, and command/script file lists. |
| `~/.config/zeshicast/extensions/*/` | Local extension command TOMLs and scripts referenced by extension manifests. |
| `~/.config/zeshicast/quicklinks.txt` | User quicklinks. |
| `~/.config/zeshicast/snippets.txt` | Snippets written before SQLite; imported once, then ignored and no longer written. |
| `~/.config/zeshicast/aliases.txt` | User aliases. |
| `~/.config/zeshicast/pins.txt` | User pins. |
| `~/.config/zeshicast/calc_history.json` | Recent calculator results. |
| `~/.config/zeshicast/dnd` | One-byte do-not-disturb flag (`1` or `0`) persisted by the built-in notification daemon so DND survives restarts. |
| `~/.local/share/fonts/zeshicast/` | The bundled Outfit and JetBrains Mono font files, unpacked on first launch (no user data). |

Legacy text files such as `clipboard.txt`, `recent.txt`, and
`frequencies.txt` may be read during migration to SQLite, but active clipboard
and usage history are stored in SQLite.

## Encryption At Rest

Clipboard history and snippets (title, prefix, content, tags) are encrypted
in `zeshicast.db` with ChaCha20-Poly1305, so a copy of the database (a
backup, a synced dotfiles folder) shows none of them without the key.

The key is kept in the Secret Service keyring (gnome-keyring, KeePassXC,
KWallet) as "zeshicast database key"; without a keyring that answers, in
`~/.config/zeshicast/db.key` (0600). Rows written before encryption are
encrypted the first time the key is available, and their plaintext is
zeroed in the file (`secure_delete`) and in the WAL.

If the key is lost or does not match, zeshicast shows no clipboard history
and no snippets, keeps the encrypted data, and does not make a new key over
it: a notification says so. Restore the key, or start over with
`zeshicast --reset-encrypted-data`, which deletes clipboard history,
snippets and the key.

`--export --include-history` puts the key into the archive next to the
database, so the history restores on another machine; such an archive is as
sensitive as the history itself. A safe export carries neither.

Not encrypted: the image files under `~/.cache/zeshicast/clipboard/`, usage
history, the file index, the extension permission log, and legacy files
from before SQLite (`clipboard.txt`, `recent.txt`, `frequencies.txt`),
which zeshicast reads once and never deletes; remove them by hand.

## Clipboard History

The GTK daemon records every copy through `wl-paste --watch`, in the
richest format it keeps: an image (`image/png`, cached as a PNG in
`~/.cache/zeshicast/clipboard/`), copied files (`text/uri-list`, shown as
their paths), rich text (`text/html` stored next to its plain text), or plain
text. Pasting an entry puts it back in that format, offered together with
its plain text so a terminal still gets text; "Paste as Plain Text" in the
action panel pastes only the text.

Never recorded:

- a copy marked as a password (`x-kde-passwordManagerHint`, set by KeePassXC
  and KDE apps);
- a copy made while an app listed in `clipboard_ignore_apps` is focused. The
  default lists password managers (KeePassXC, Bitwarden, 1Password, Enpass);
  an entry matches an app id exactly or by its last part (`KeePassXC` covers
  `org.keepassxc.KeePassXC`). Set it to an empty value to ignore no app.

Privacy preferences:

- `clipboard_history_enabled = "false"` disables installing clipboard watchers
  on startup.
- `clipboard_private_mode = "true"` pauses recording new clipboard entries,
  in every format, without deleting existing history.
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

## File Index

File search reads an index of up to ~12,000 files and folders under `$HOME`
(`Desktop`, `Downloads`, `Documents`, `Projects`, `Code` first, then the rest,
five levels deep). Since Q6.1 the index is stored in the `files` table of
`zeshicast.db`: paths and folder modification times, never file contents.
It is updated live with inotify while the daemon runs.

Hidden entries (`.name`), `target`, `node_modules` and similar build or cache
folders are never indexed. `files_exclude` keeps more out: a comma separated
list of names (`Videos, vm`) and paths (`~/Private`, `/home/me/secret`);
everything below an excluded folder is left out too, and rows already stored
for it are dropped on the next start. `files_in_root = "false"` keeps
files out of the main search; `file …` still searches them.

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
