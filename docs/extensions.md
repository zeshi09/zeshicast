# Writing zeshicast extensions

An extension is a directory under `~/.config/zeshicast/extensions/` with an
`extension.toml` manifest. It can bring three kinds of things:

| Kind | What it is | Runs code? |
| --- | --- | --- |
| `commands` | TOML command definitions: open a URL, run a program, produce a list of results | `argv`/`shell`/`json` modes do |
| `scripts` | Script files with Raycast-style metadata comments | yes |
| `binaries` | Executables that answer search queries over JSON-RPC | yes |

Start from the template: copy `packaging/examples/extension-template/` to
`~/.config/zeshicast/extensions/<name>/`, change `id` and `name`, restart the
daemon (`systemctl --user restart zeshicast`). Its `bin/template.sh` needs
`jq`; `bin/template.py` is the same extension with the Python standard
library only.

Everything here is enforced by the host, not by convention. Every rule
about permissions below names the test that checks it.

## The manifest

```toml
#:schema https://raw.githubusercontent.com/zeshi09/zeshicast/main/resources/extension.schema.json
id = "you.git-tools"          # required, stable, reverse-domain style
name = "Git Tools"            # required, shown in results
version = "0.1.0"             # required, your own version
protocol = 1                  # required when `binaries` is set
capabilities = ["shell", "open_url"]
commands = ["log.toml"]
scripts = ["scripts/status.sh"]
binaries = ["bin/git-tools"]
```

`resources/extension.schema.json` is a JSON Schema for the manifest; editors
with TOML schema support (Taplo, Even Better TOML) pick it up from the
`#:schema` line.

Paths are relative to the extension directory. Absolute paths and `..` are
dropped, and so is a symlinked extension directory.
<!-- test: manifest_rejects_absolute_and_parent_paths -->
`commands` takes `*.toml` files only; an executable listed there is ignored.
Executables belong in `binaries`.
<!-- test: manifest_commands_rejects_non_toml -->

## Capabilities

| Capability | Allows |
| --- | --- |
| `shell` | running code: shell and argv commands, scripts, `binaries`, extension items |
| `network` | opening `http(s)` URLs (implies `open_url`) |
| `open_url` | opening `http(s)` URLs |
| `filesystem` | opening local paths and `file://` URLs (implies `open_path`) |
| `open_path` | opening local paths and `file://` URLs |
| `clipboard_write` | putting text on the clipboard |
| `clipboard_read` | reading clipboard history |

The manifest's `capabilities` are a **ceiling, not a grant**. A command gets
`its own permissions ∩ the manifest's capabilities`, so a command cannot give
itself `shell` inside an extension that does not have it.
<!-- test: command_cannot_self_grant_shell_inside_empty_manifest -->

Anything that runs code needs `shell`, and the user confirms it before it
runs:

- a shell-mode command without `shell` is shown as blocked;
  <!-- test: gateway_denies_shell_without_capability -->
- argv commands run a program too: they need `permissions = ["shell"]`, and
  they always ask, even though they bypass the shell;
  <!-- test: argv_command_requires_confirmation -->
- extension scripts need `shell` in the manifest, and they ask even when the
  script says `@raycast.needsConfirmation false`;
  <!-- test: extension_script_without_shell_capability_is_blocked -->
  <!-- test: extension_script_ignores_needs_confirmation_false -->
- `binaries` are not loaded at all without `shell`; searching for the
  extension's name shows a blocked row saying so.
  <!-- test: manifest_binaries_require_shell -->

A result that asks for a capability the extension lacks is not dropped. It
is shown with a `Blocked: … requires …` subtitle and does nothing, so the
missing permission is visible.

### What the user sees

In the Extension Browser (Ctrl+B), Enter on a row of your extension opens its
screen: what the manifest allows, and for each command, script and binary
what it asked for, what it was granted (`asked ∩ manifest`), what was not
granted, and why it is blocked, if it is.
<!-- test: the_extension_screen_explains_each_grant_and_block -->

Every time something your extension produced asks for a capability, the
decision goes to a permission log the user can read in the Extension Browser
(Ctrl+B): `granted`, `confirmed`, `cancelled` or `denied`, with the
capability and the command line, URL or item id. A reply effect your
manifest does not allow shows up there as `denied`. `docs/security.md`
describes the log.
<!-- test: every_extension_request_is_logged_with_its_outcome -->

## Commands

A command file has the same format as one in `~/.config/zeshicast/commands/`;
the README documents the modes (`argv`, `shell`, `json`), arguments,
`[preferences]`, `[env]` and placeholders. Inside an extension, their
`permissions` are capped by the manifest.

Placeholders (`{{query}}`, `{{arg:name}}`, `{{clipboard}}`, …) are escaped for
the context they land in, so a typed value cannot inject shell syntax. In
`argv` mode each placeholder becomes part of one argument and no shell is
involved. Prefer `argv` unless you need pipes or redirects.

A `json`-mode command prints results on stdout. Each result's `action` is
gated like everything else: `open_url` needs `network`/`open_url`, a
`file://` URL needs `filesystem`/`open_path`, `copy` needs `clipboard_write`,
`shell` needs `shell`. Results inherit the producing command's capabilities,
not more.
<!-- test: json_result_inherits_producer_capabilities -->
<!-- test: json_file_url_requires_open_path_capability -->

## Scripts

Script commands use the Raycast/Vicinae metadata format: a comment block
within the first 50 lines.

```sh
#!/bin/sh
# @raycast.schemaVersion 1
# @raycast.title Git Status
# @raycast.packageName Git Tools
# @raycast.mode fullOutput
# @raycast.argument1 { "type": "text", "placeholder": "Path", "optional": true }
git -C "${1:-.}" status --short
```

`schemaVersion` and `title` are required. `@vicinae.*` keys work too. Modes:
`fullOutput` (default), `compact`, `silent`, `inline`. Argument types: `text`,
`dropdown` (with `data`), `number`, `path`, `bool`, plus `optional` and
`percentEncoded`. The script runs directly, without `sh -c`, so a path with
`;` in it is a file name.
<!-- test: script_path_with_semicolon_is_not_shell_interpreted -->

## Binaries: the JSON-RPC protocol

### Protocol

A binary declared in `binaries` is an executable that zeshicast starts **once
per request**. It reads **one line** of JSON-RPC 2.0 from stdin, writes **one
line** to stdout and exits. There is no handshake and no long-lived session.

The manifest must say which protocol version the binary speaks:
`protocol = 1`. If it is missing, older, newer or not a number, the binary is
never started; searching for the extension's name shows a blocked row
saying which side to update.
<!-- test: binaries_are_refused_for_another_or_missing_protocol -->
<!-- test: an_extension_refused_for_its_protocol_shows_why_without_running -->
Every request also carries the host's version as `params.protocol`, so a
binary can refuse a host it does not know (the template does).

| Limit | Value |
| --- | --- |
| `search` answer | 400 ms, otherwise the extension gives no results for that query |
| `execute` answer | 5 s |
| Reply size | 1 MiB, one line |
| After answering | 0.5 s to exit, then killed |
| stderr | discarded |

On a timeout the whole process group is killed, so background children do
not survive either.
<!-- test: a_grandchild_holding_stdout_does_not_hang_the_timeout -->
<!-- test: lingering_child_is_reaped_after_answering -->

### `search`

Called as the user types, for every non-empty query except `file …`. Identical (binary, query) pairs
are answered from a 30-second cache, so a binary is not started again for the
same text; errors are not cached.
<!-- test: an_identical_query_does_not_spawn_the_extension_twice -->

```json
{"jsonrpc":"2.0","id":1,"method":"search","params":{"protocol":1,"query":"deploy"}}
```

The result is an array of items:

```json
{"jsonrpc":"2.0","id":1,"result":[
  {"id":"docs","title":"Open the docs","subtitle":"web","icon":"emblem-web-symbolic",
   "score":90,"open_url":"https://example.org/docs"},
  {"id":"copy","title":"Copy the token name","copy_text":"DEPLOY_TOKEN"},
  {"id":"deploy-prod","title":"Deploy to prod"}
]}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | your identifier, sent back in `execute` |
| `title` | yes | items with an empty title are skipped |
| `subtitle` | no | defaults to the extension name |
| `icon` | no | icon name from the theme |
| `score` | no | ranking; defaults to 80 for the first item, 2 less for each next |
| `open_url` | no | Enter opens it (needs `network`/`open_url`; `file://` needs `filesystem`/`open_path`) |
| `copy_text` | no | Enter copies it (needs `clipboard_write`) |

An item with neither `open_url` nor `copy_text` is an **extension item**: on
Enter, after a confirmation, zeshicast calls your `execute` with its `id`. The
`id` is never run as a command line.
<!-- test: an_extension_item_never_becomes_a_shell_command -->
<!-- test: gate_action_intent_blocks_an_extension_item_without_shell -->

### `execute`

```json
{"jsonrpc":"2.0","id":1,"method":"execute","params":{"protocol":1,"id":"deploy-prod","arguments":{}}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"success":true,"output":"deployed 3 services"}}
```

| Field | Meaning |
| --- | --- |
| `success` | `false` reports `output` (or a generic message) as a failure |
| `output` | shown as a notification |
| `notify` | shown as a notification when there is no `output` |
| `open_url` | opened, if the manifest allows it |
| `copy_text` | copied, if the manifest allows it |

A reply cannot do more than the manifest allows: an `open_url` or
`copy_text` the extension has no capability for is dropped, and a `file://`
URL needs `filesystem`/`open_path` even with `network`.
<!-- test: an_extension_reply_effect_needs_the_capability_it_asks_for -->
<!-- test: a_reply_cannot_reach_the_filesystem_through_a_file_url -->

### Errors

Answer with a JSON-RPC error object; the message reaches the user as a
notification for `execute` and is dropped silently for `search` (the next
keystroke asks again).

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"unknown method"}}
```

### Versioning

`protocol` changes only for something an existing binary could misread: a
new required method, a changed field meaning, a removed field. Adding an
optional field to a request or a reply does not change it, so ignore fields
you do not know.

## Publishing an extension

Put the extension in a git repository with `extension.toml` at the top and
add an entry to the registry index; users install it with
`zeshicast --extension-install <id>`, Nix users with
`mkZeshicastExtension`. `docs/extensions-registry.md` has the entry format and
every check an install runs.

## Testing an extension

Lint it first. The loader skips what it cannot use without complaining (a
mistyped key, a script without metadata, a command without `shell`); the
linter runs the same parsers and says what would be ignored or blocked:

```sh
zeshicast --extension-lint ~/.config/zeshicast/extensions/template
zeshicast --extension-lint --json ./my-extension   # for editors and CI
```

It exits 1 when there is an error. Errors are things the host ignores or
blocks; warnings are things that work, but not as written (a capability
alias, a permission the manifest does not grant, `protocol` without
`binaries`). Rule ids such as `needs-shell` or `path-unsafe` are stable.

Run the binary by hand with a request on stdin:

```sh
echo '{"jsonrpc":"2.0","id":1,"method":"search","params":{"protocol":1,"query":"x"}}' \
  | ~/.config/zeshicast/extensions/template/bin/template.sh
```

Then check it in the CLI, which runs the same search without the GUI:
`zeshicast <query>`. Extensions are read when the daemon starts, so restart
it after editing the manifest.
