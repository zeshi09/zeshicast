# Extension registry

The registry is a list of extensions in git, not a store. There is no server:
an index file names a tarball, pins it by hash and states the permissions
the extension gets. Nothing is downloaded until you run
`zeshicast --extension-install <id>`, and nothing is installed until you
confirm.

## The index

The default index is `registry.toml` in the
[`zeshi09/zeshicast-extensions`](https://github.com/zeshi09/zeshicast-extensions)
repository; `--registry <file|https-url>` reads another one.

```toml
[[extension]]
id = "you.git-tools"
url = "https://github.com/you/git-tools/archive/3f2c1e9.tar.gz"
rev = "3f2c1e9"
sha256 = "8fd623ee4ab09c677f6ceb6ecf7c4efc77e40f516b825fdeb539bdd5753a0d42"
capabilities = ["shell", "open_url"]
```

| Field | Meaning |
| --- | --- |
| `id` | The manifest's `id`; also the directory name under `extensions/`. Letters, digits, `.`, `_`, `-`; not starting with `.` |
| `url` | An `https://` `.tar.gz` with one top directory holding `extension.toml` (a GitHub archive link works) |
| `rev` | The revision the tarball was made from, shown before installing |
| `sha256` | The tarball's sha256, 64 hex digits |
| `capabilities` | Everything the extension may ask for. Required, even when empty |

Every field is required. An entry without `capabilities` is refused rather
than read as "none": a reviewer must have written the list down.
<!-- test: an_entry_without_capabilities_is_refused -->
<!-- test: registry_entries_must_be_well_formed -->

To compute the hash: `curl -sL <url> | sha256sum`.

## What `--extension-install` checks

```sh
zeshicast --extension-install you.git-tools
zeshicast --extension-install you.git-tools --registry ./registry.toml --root /tmp/sandbox
```

In order, refusing at the first failure:

1. The entry is complete and well formed (above).
2. The download's sha256 is the entry's. A changed tarball is not unpacked.
   <!-- test: a_tampered_archive_is_refused_by_its_hash -->
3. The archive unpacks into a staging directory inside `extensions/` without
   links, absolute paths, `..` or a second top directory, so it cannot write
   anywhere else.
   <!-- test: an_archive_cannot_write_outside_the_extensions_directory -->
4. Its `extension.toml` has the entry's `id`.
   <!-- test: the_archive_must_be_the_extension_the_entry_names -->
5. The manifest asks for no capability the entry does not list.
   <!-- test: a_manifest_cannot_ask_for_more_than_the_registry_lists -->
6. `zeshicast --extension-lint` finds no errors.
   <!-- test: an_extension_that_fails_the_linter_is_refused -->
7. You see the name, version, revision, capabilities and what it brings,
   and answer `y`. Without a terminal, only `--yes` installs.
8. It is moved to `extensions/<id>`. An installed extension is not
   overwritten; remove its directory first. Cancelling leaves nothing behind.
   <!-- test: cancelling_or_reinstalling_installs_nothing -->

`--root <dir>` installs into `<dir>/extensions/` instead of
`~/.config/zeshicast/extensions/`, to try an extension in a sandbox.
<!-- test: an_extension_installs_into_the_root_sandbox -->

Restart the daemon afterwards: `systemctl --user restart zeshicast`.

## Nix

On NixOS or Home Manager, skip the registry and pin the source yourself:

```nix
let
  git-tools = inputs.zeshicast.lib.${system}.mkZeshicastExtension {
    id = "you.git-tools";
    src = pkgs.fetchFromGitHub {
      owner = "you"; repo = "git-tools"; rev = "3f2c1e9"; hash = "sha256-…";
    };
    capabilities = [ "shell" "open_url" ];
  };
in {
  xdg.configFile."zeshicast/extensions/you.git-tools".source = git-tools;
}
```

The build runs `zeshicast --extension-lint --allow <capabilities>`, so it
fails when the manifest asks for more than the list, or when the manifest's
`id` differs.
<!-- test: allow_lists_the_capabilities_a_packager_accepts -->

## Adding an extension to the index

Open a pull request against the index repository with one `[[extension]]`
entry. The reviewer checks that `capabilities` is what the extension needs,
that the code at `rev` does what its README says, and that
`zeshicast --extension-lint` passes.
