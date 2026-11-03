//! `zeshicast --extension-install <id>` (Q5.6): install one extension from the
//! git registry, only on an explicit command.
//!
//! A registry entry pins a tarball by URL, revision and sha256 and lists the
//! capabilities the extension gets. Installing refuses, before anything lands
//! in `extensions/`:
//!
//! - an entry without `capabilities`, or with a malformed id/hash/URL;
//! - an archive whose sha256 differs from the entry;
//! - an archive with links, absolute paths, `..` or more than one top
//!   directory;
//! - a manifest whose id differs from the entry, or that asks for a capability
//!   the entry does not list;
//! - a manifest the linter finds errors in;
//! - an extension that is already installed.
//!
//! The archive is unpacked into a staging directory next to the destination
//! and renamed into place only after the user confirmed the permissions.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// Where `--extension-install` reads the registry when `--registry` is not
/// given: the index repository's `registry.toml`.
pub const DEFAULT_REGISTRY: &str =
    "https://raw.githubusercontent.com/zeshi09/zeshicast-extensions/main/registry.toml";

/// Largest archive or registry downloaded.
const DOWNLOAD_LIMIT: u64 = 32 * 1024 * 1024;
/// Largest unpacked extension.
const UNPACKED_LIMIT: u64 = 128 * 1024 * 1024;
const MEMBER_LIMIT: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    pub id: String,
    pub url: String,
    pub rev: String,
    pub sha256: String,
    pub capabilities: Vec<String>,
}

/// What the user is asked to confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub id: String,
    pub name: String,
    pub version: String,
    pub rev: String,
    pub capabilities: Vec<String>,
    pub commands: usize,
    pub scripts: usize,
    pub binaries: usize,
    pub destination: PathBuf,
}

impl InstallPlan {
    pub fn render(&self) -> String {
        let capabilities = if self.capabilities.is_empty() {
            "none".to_string()
        } else {
            self.capabilities.join(", ")
        };
        format!(
            "{} {} ({}) at {}\n  capabilities: {capabilities}\n  \
             {} commands, {} scripts, {} binaries\n  into {}\n",
            self.name,
            self.version,
            self.id,
            self.rev,
            self.commands,
            self.scripts,
            self.binaries,
            self.destination.display()
        )
    }
}

/// Find `id` in a registry file and check the entry is complete.
///
/// ```toml
/// [[extension]]
/// id = "you.git-tools"
/// url = "https://github.com/you/git-tools/archive/<rev>.tar.gz"
/// rev = "<commit>"
/// sha256 = "<64 hex digits of the tarball>"
/// capabilities = ["shell"]
/// ```
pub fn find_entry(registry: &str, id: &str) -> Result<RegistryEntry, String> {
    let table: toml::Table = registry
        .parse()
        .map_err(|error| format!("the registry is not valid TOML: {error}"))?;
    let entries = table
        .get("extension")
        .and_then(toml::Value::as_array)
        .ok_or("the registry has no [[extension]] entries")?;
    let entry = entries
        .iter()
        .filter_map(toml::Value::as_table)
        .find(|entry| entry.get("id").and_then(toml::Value::as_str) == Some(id))
        .ok_or_else(|| format!("`{id}` is not in the registry"))?;

    let text = |key: &str| {
        entry
            .get(key)
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("the registry entry for `{id}` has no `{key}`"))
    };
    let url = text("url")?;
    let rev = text("rev")?;
    let sha256 = text("sha256")?.to_ascii_lowercase();
    // The registry states the permissions up front, so the user can judge an
    // extension before downloading it; an entry that does not is refused
    // rather than read as "none".
    let capabilities = entry
        .get("capabilities")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("the registry entry for `{id}` has no `capabilities` list"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("`capabilities` of `{id}` must be strings"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    check_id(id)?;
    if !url.starts_with("https://") {
        return Err(format!("the archive of `{id}` must be an https:// URL"));
    }
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("the sha256 of `{id}` is not 64 hex digits"));
    }
    Ok(RegistryEntry {
        id: id.to_string(),
        url,
        rev,
        sha256,
        capabilities: canonical(&capabilities),
    })
}

/// The id becomes a directory name under `extensions/`, so it may not
/// contain a separator, start with a dot or be `..`.
fn check_id(id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "`{id}` is not a valid extension id (letters, digits, . _ -; not starting with .)"
        ))
    }
}

fn canonical(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let label = crate::search::commands::parse_capabilities(std::slice::from_ref(name))
            .first()
            .map(|capability| capability.label().to_string())
            .unwrap_or_else(|| name.trim().to_string());
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
}

/// Install `entry` under `config_dir/extensions/<id>`.
///
/// `fetch` downloads the archive (the CLI uses HTTPS; tests pass bytes in),
/// `confirm` shows the plan and returns whether to go on.
pub fn install(
    entry: &RegistryEntry,
    config_dir: &Path,
    fetch: impl FnOnce(&str) -> io::Result<Vec<u8>>,
    confirm: impl FnOnce(&InstallPlan) -> bool,
) -> Result<PathBuf, String> {
    check_id(&entry.id)?;
    let extensions_dir = config_dir.join("extensions");
    fs::create_dir_all(&extensions_dir)
        .map_err(|error| format!("cannot create {}: {error}", extensions_dir.display()))?;
    if fs::symlink_metadata(&extensions_dir)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Err(format!(
            "{} must be a real directory",
            extensions_dir.display()
        ));
    }
    let destination = extensions_dir.join(&entry.id);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(format!(
            "`{}` is already installed at {}; remove it first",
            entry.id,
            destination.display()
        ));
    }

    let archive = fetch(&entry.url).map_err(|error| format!("download failed: {error}"))?;
    let digest = format!("{:x}", Sha256::digest(&archive));
    if digest != entry.sha256 {
        return Err(format!(
            "sha256 mismatch for `{}`: the registry says {}, the download is {digest}",
            entry.id, entry.sha256
        ));
    }

    let staging = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&extensions_dir)
        .map_err(|error| format!("cannot create a staging directory: {error}"))?;
    let unpacked = unpack(&archive, staging.path())?;

    let manifest_text = fs::read_to_string(unpacked.join("extension.toml"))
        .map_err(|_| "the archive has no extension.toml at its top".to_string())?;
    let manifest = crate::extensions::parse_extension_manifest(&unpacked, &manifest_text)
        .ok_or("extension.toml lacks id, name or version")?;
    if manifest.origin.id != entry.id {
        return Err(format!(
            "the archive is `{}`, not `{}`",
            manifest.origin.id, entry.id
        ));
    }
    let asked = canonical(&manifest.origin.capabilities);
    let beyond: Vec<_> = asked
        .iter()
        .filter(|capability| !entry.capabilities.contains(capability))
        .cloned()
        .collect();
    if !beyond.is_empty() {
        return Err(format!(
            "the manifest asks for {} that the registry entry does not list",
            beyond.join(", ")
        ));
    }
    let findings = crate::extension_lint::lint_extension(&unpacked);
    if crate::extension_lint::has_errors(&findings) {
        return Err(format!(
            "the extension does not pass --extension-lint:\n{}",
            crate::extension_lint::render_text(&findings)
        ));
    }

    let plan = InstallPlan {
        id: entry.id.clone(),
        name: manifest.origin.name.clone(),
        version: manifest.origin.version.clone(),
        rev: entry.rev.clone(),
        capabilities: asked,
        commands: manifest.commands.len(),
        scripts: manifest.scripts.len(),
        binaries: manifest.declared_binaries.len(),
        destination: destination.clone(),
    };
    if !confirm(&plan) {
        return Err("cancelled; nothing was installed".to_string());
    }

    fs::rename(&unpacked, &destination)
        .map_err(|error| format!("cannot move into {}: {error}", destination.display()))?;
    Ok(destination)
}

/// Unpack a `.tar.gz` with exactly one top directory into `staging`; return
/// that directory. Links, absolute paths and `..` are refused, so nothing is
/// written outside `staging`.
fn unpack(archive: &[u8], staging: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut top: Option<std::ffi::OsString> = None;
    let mut members = 0usize;
    let mut unpacked = 0u64;
    let entries = tar
        .entries()
        .map_err(|error| format!("not a .tar.gz archive: {error}"))?;
    for member in entries {
        let mut member = member.map_err(|error| format!("broken archive: {error}"))?;
        members += 1;
        if members > MEMBER_LIMIT {
            return Err(format!("the archive has more than {MEMBER_LIMIT} members"));
        }
        let path = member
            .path()
            .map_err(|error| format!("broken archive: {error}"))?
            .into_owned();
        let kind = member.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(format!("the archive contains a link: {}", path.display()));
        }
        // GitHub tarballs start with a pax header carrying the commit id.
        if matches!(
            kind,
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader
        ) {
            continue;
        }
        let mut components = path.components();
        let first = match components.next() {
            Some(Component::Normal(first)) => first.to_os_string(),
            _ => return Err(format!("unsafe path in the archive: {}", path.display())),
        };
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(format!("unsafe path in the archive: {}", path.display()));
        }
        match &top {
            None => top = Some(first),
            Some(top) if *top != first => {
                return Err("the archive must hold one top directory".to_string());
            }
            Some(_) => {}
        }

        let target = staging.join(&path);
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            continue;
        }
        if !kind.is_file() {
            return Err(format!(
                "the archive contains a special file: {}",
                path.display()
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mode = member.header().mode().unwrap_or(0o644) & 0o755;
        let mut file = fs::File::create(&target).map_err(|error| error.to_string())?;
        let budget = UNPACKED_LIMIT.saturating_sub(unpacked) + 1;
        let written = io::copy(&mut (&mut member).take(budget), &mut file)
            .map_err(|error| format!("broken archive: {error}"))?;
        unpacked += written;
        if unpacked > UNPACKED_LIMIT {
            return Err(format!(
                "the archive unpacks to more than {UNPACKED_LIMIT} bytes"
            ));
        }
        fs::set_permissions(&target, fs::Permissions::from_mode(mode))
            .map_err(|error| error.to_string())?;
    }
    let top = top.ok_or("the archive is empty")?;
    Ok(staging.join(top))
}

/// Read a registry from a file, or download it from an https:// URL.
pub fn read_registry(source: &str) -> Result<String, String> {
    if source.starts_with("https://") {
        let bytes = download(source).map_err(|error| format!("{source}: {error}"))?;
        String::from_utf8(bytes).map_err(|_| format!("{source} is not UTF-8"))
    } else {
        fs::read_to_string(source).map_err(|error| format!("{source}: {error}"))
    }
}

/// HTTPS download with a size cap.
pub fn download(url: &str) -> io::Result<Vec<u8>> {
    if !url.starts_with("https://") {
        return Err(io::Error::other("only https:// URLs are downloaded"));
    }
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(io::Error::other)?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(DOWNLOAD_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > DOWNLOAD_LIMIT {
        return Err(io::Error::other(format!(
            "larger than {DOWNLOAD_LIMIT} bytes"
        )));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("zeshicast-install-{name}-"))
            .tempdir()
            .unwrap()
    }

    /// A `.tar.gz` of `files` (path, contents, mode).
    fn tarball(files: &[(&str, &str, u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (path, contents, mode) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(*mode);
            header.set_entry_type(tar::EntryType::Regular);
            // `append_data` refuses `..`; write the name raw so the test can
            // build the archive an attacker would.
            let name = header.as_old_mut().name.as_mut();
            name[..path.len()].copy_from_slice(path.as_bytes());
            header.set_cksum();
            builder.append(&header, contents.as_bytes()).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    const MANIFEST: &str = "id = \"you.hello\"\nname = \"Hello\"\nversion = \"1.0.0\"\n\
                            capabilities = [\"shell\"]\ncommands = [\"hello.toml\"]\n";
    const COMMAND: &str = "name = \"Hello\"\nmode = \"argv\"\nprogram = \"notify-send\"\n\
                           args = [\"hello\"]\npermissions = [\"shell\"]\n";

    fn good_archive() -> Vec<u8> {
        tarball(&[
            ("hello-abc/extension.toml", MANIFEST, 0o644),
            ("hello-abc/hello.toml", COMMAND, 0o644),
        ])
    }

    fn entry_for(archive: &[u8], capabilities: &str) -> RegistryEntry {
        let registry = format!(
            "[[extension]]\nid = \"you.hello\"\nurl = \"https://example.org/hello.tar.gz\"\n\
             rev = \"abc\"\nsha256 = \"{:x}\"\ncapabilities = {capabilities}\n",
            Sha256::digest(archive)
        );
        find_entry(&registry, "you.hello").unwrap()
    }

    fn install_bytes(
        entry: &RegistryEntry,
        root: &Path,
        archive: Vec<u8>,
    ) -> Result<PathBuf, String> {
        install(entry, root, move |_| Ok(archive), |_| true)
    }

    fn installed_ids(root: &Path) -> Vec<String> {
        fs::read_dir(root.join("extensions"))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn an_extension_installs_into_the_root_sandbox() {
        let root = temp_root("ok");
        let archive = good_archive();
        let entry = entry_for(&archive, "[\"shell\"]");
        let mut shown = None;
        let destination = install(
            &entry,
            root.path(),
            |url| {
                assert_eq!(url, "https://example.org/hello.tar.gz");
                Ok(archive)
            },
            |plan| {
                shown = Some(plan.clone());
                true
            },
        )
        .unwrap();
        assert_eq!(destination, root.path().join("extensions/you.hello"));
        assert!(destination.join("hello.toml").is_file());
        let plan = shown.expect("the user saw the plan");
        assert_eq!(plan.capabilities, vec!["shell"]);
        assert!(plan.render().contains("capabilities: shell"));
        assert_eq!(
            installed_ids(root.path()),
            vec!["you.hello"],
            "no staging left"
        );
    }

    #[test]
    fn a_tampered_archive_is_refused_by_its_hash() {
        let root = temp_root("hash");
        let entry = entry_for(&good_archive(), "[\"shell\"]");
        let tampered = tarball(&[
            ("hello-abc/extension.toml", MANIFEST, 0o644),
            (
                "hello-abc/hello.toml",
                "name = \"Evil\"\ncommand = \"rm -rf ~\"\n",
                0o644,
            ),
        ]);
        let error = install_bytes(&entry, root.path(), tampered).unwrap_err();
        assert!(error.contains("sha256 mismatch"), "{error}");
        assert!(installed_ids(root.path()).is_empty());
    }

    #[test]
    fn an_entry_without_capabilities_is_refused() {
        let registry = "[[extension]]\nid = \"you.hello\"\nurl = \"https://example.org/a.tar.gz\"\n\
                        rev = \"abc\"\nsha256 = \""
            .to_string()
            + &"0".repeat(64) + "\"\n";
        let error = find_entry(&registry, "you.hello").unwrap_err();
        assert!(error.contains("capabilities"), "{error}");
    }

    #[test]
    fn registry_entries_must_be_well_formed() {
        let base = |id: &str, url: &str, sha: &str| {
            format!(
                "[[extension]]\nid = \"{id}\"\nurl = \"{url}\"\nrev = \"r\"\n\
                 sha256 = \"{sha}\"\ncapabilities = []\n"
            )
        };
        let sha = "a".repeat(64);
        assert!(find_entry(&base("ok.id", "https://x/a.tgz", &sha), "ok.id").is_ok());
        assert!(find_entry(&base("ok.id", "http://x/a.tgz", &sha), "ok.id").is_err());
        assert!(find_entry(&base("ok.id", "https://x/a.tgz", "abc"), "ok.id").is_err());
        assert!(find_entry(&base("../evil", "https://x/a.tgz", &sha), "../evil").is_err());
        assert!(find_entry(&base("a/b", "https://x/a.tgz", &sha), "a/b").is_err());
        assert!(find_entry(&base(".hidden", "https://x/a.tgz", &sha), ".hidden").is_err());
        assert!(find_entry(&base("ok.id", "https://x/a.tgz", &sha), "other").is_err());
    }

    #[test]
    fn an_archive_cannot_write_outside_the_extensions_directory() {
        for (name, archive) in [
            (
                "parent",
                tarball(&[
                    ("hello-abc/extension.toml", MANIFEST, 0o644),
                    ("hello-abc/../../escaped", "x", 0o644),
                ]),
            ),
            (
                "absolute",
                tarball(&[("/tmp/zeshicast-escaped", "x", 0o644)]),
            ),
            (
                "two-tops",
                tarball(&[
                    ("hello-abc/extension.toml", MANIFEST, 0o644),
                    ("other/extension.toml", MANIFEST, 0o644),
                ]),
            ),
        ] {
            let root = temp_root(name);
            let entry = entry_for(&archive, "[\"shell\"]");
            let error = install_bytes(&entry, root.path(), archive).unwrap_err();
            assert!(
                error.contains("unsafe path") || error.contains("one top directory"),
                "{name}: {error}"
            );
            assert!(installed_ids(root.path()).is_empty(), "{name}");
            assert!(!root.path().join("escaped").exists(), "{name}");
        }
    }

    #[test]
    fn a_manifest_cannot_ask_for_more_than_the_registry_lists() {
        let root = temp_root("more");
        let archive = good_archive();
        let entry = entry_for(&archive, "[]");
        let error = install_bytes(&entry, root.path(), archive).unwrap_err();
        assert!(error.contains("shell"), "{error}");
        assert!(installed_ids(root.path()).is_empty());
    }

    #[test]
    fn the_archive_must_be_the_extension_the_entry_names() {
        let root = temp_root("id");
        let archive = tarball(&[(
            "x/extension.toml",
            "id = \"someone.else\"\nname = \"X\"\nversion = \"1\"\n",
            0o644,
        )]);
        let entry = entry_for(&archive, "[]");
        let error = install_bytes(&entry, root.path(), archive).unwrap_err();
        assert!(error.contains("someone.else"), "{error}");
    }

    #[test]
    fn cancelling_or_reinstalling_installs_nothing() {
        let root = temp_root("cancel");
        let archive = good_archive();
        let entry = entry_for(&archive, "[\"shell\"]");
        let copy = archive.clone();
        let error = install(&entry, root.path(), move |_| Ok(copy), |_| false).unwrap_err();
        assert!(error.contains("cancelled"), "{error}");
        assert!(installed_ids(root.path()).is_empty(), "staging removed");

        install_bytes(&entry, root.path(), archive.clone()).unwrap();
        let error = install_bytes(&entry, root.path(), archive).unwrap_err();
        assert!(error.contains("already installed"), "{error}");
    }

    #[test]
    fn an_extension_that_fails_the_linter_is_refused() {
        let root = temp_root("lint");
        let archive = tarball(&[(
            "hello-abc/extension.toml",
            "id = \"you.hello\"\nname = \"Hello\"\nversion = \"1\"\ncomands = [\"x.toml\"]\n",
            0o644,
        )]);
        let entry = entry_for(&archive, "[]");
        let error = install_bytes(&entry, root.path(), archive).unwrap_err();
        assert!(error.contains("unknown-key"), "{error}");
    }
}
