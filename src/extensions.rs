use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionOrigin {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExtensionManifest {
    pub(crate) origin: ExtensionOrigin,
    /// Command definitions: TOML files only (as documented in the README).
    pub(crate) commands: Vec<PathBuf>,
    pub(crate) scripts: Vec<PathBuf>,
    /// Executable JSON-RPC extensions. Previously such binaries could be
    /// smuggled through `commands`; they now need their own field and the
    /// manifest must grant the `shell` capability (B-5).
    pub(crate) binaries: Vec<PathBuf>,
    /// Why the declared `binaries` were not loaded, when they were refused
    /// before any spawn (Q5.1: protocol version). Shown as a blocked result.
    pub(crate) binaries_blocked: Option<String>,
    /// Every binary the manifest lists, loaded or not (Q5.5 shows them all).
    pub(crate) declared_binaries: Vec<PathBuf>,
}

pub(crate) fn load_extension_manifests(config_dir: &Path) -> Vec<ExtensionManifest> {
    let extensions_dir = config_dir.join("extensions");
    let Ok(entries) = fs::read_dir(&extensions_dir) else {
        return Vec::new();
    };

    let mut manifests = entries
        .flatten()
        .filter_map(|entry| {
            let root = entry.path();
            // `file_type()` does not follow symlinks, so a symlinked directory
            // cannot pull manifests in from outside the extensions dir.
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            let content = fs::read_to_string(root.join("extension.toml")).ok()?;
            parse_extension_manifest(&root, &content)
        })
        .collect::<Vec<_>>();
    manifests.sort_by(|a, b| a.origin.id.cmp(&b.origin.id));
    manifests
}

pub(crate) fn parse_extension_manifest(root: &Path, input: &str) -> Option<ExtensionManifest> {
    let table = input.parse::<toml::Table>().ok()?;
    let id = toml_required_string(&table, "id")?;
    let name = toml_required_string(&table, "name")?;
    let version = toml_required_string(&table, "version")?;
    let capabilities = toml_string_array(&table, "capabilities");
    let commands = manifest_paths(root, &table, "commands")
        .into_iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .collect();
    let scripts = manifest_paths(root, &table, "scripts");
    let declared_binaries = manifest_paths(root, &table, "binaries");
    // Executing extension code requires the manifest to say so, and a binary
    // written for another protocol would misread our requests (or we its
    // replies): refuse both here, so they are never spawned.
    let binaries_blocked = if declared_binaries.is_empty() {
        None
    } else if !grants_shell_capability(&capabilities) {
        Some(BLOCKED_BINARIES_LACK_SHELL.to_string())
    } else {
        protocol_mismatch(table.get("protocol"))
    };
    let binaries = if binaries_blocked.is_some() {
        Vec::new()
    } else {
        declared_binaries.clone()
    };

    Some(ExtensionManifest {
        origin: ExtensionOrigin {
            id,
            name,
            version,
            capabilities,
        },
        commands,
        scripts,
        binaries,
        binaries_blocked,
        declared_binaries,
    })
}

const BLOCKED_BINARIES_LACK_SHELL: &str =
    "Blocked: extension binaries need capabilities = [\"shell\"]";

/// The reason a manifest's `protocol` rules its binaries out, if it does.
fn protocol_mismatch(value: Option<&toml::Value>) -> Option<String> {
    use crate::services::extension_protocol::PROTOCOL_VERSION;
    match value {
        None => Some(format!(
            "Blocked: extension manifest lacks protocol = {PROTOCOL_VERSION}"
        )),
        Some(toml::Value::Integer(version)) if *version == i64::from(PROTOCOL_VERSION) => None,
        Some(toml::Value::Integer(version)) if *version > i64::from(PROTOCOL_VERSION) => {
            Some(format!(
                "Blocked: extension needs protocol {version}; this zeshicast speaks \
                 {PROTOCOL_VERSION} (update zeshicast)"
            ))
        }
        Some(toml::Value::Integer(version)) => Some(format!(
            "Blocked: extension speaks protocol {version}; this zeshicast speaks \
             {PROTOCOL_VERSION} (update the extension)"
        )),
        Some(_) => Some(format!(
            "Blocked: extension protocol must be a number (protocol = {PROTOCOL_VERSION})"
        )),
    }
}

fn grants_shell_capability(capabilities: &[String]) -> bool {
    capabilities.iter().any(|capability| {
        let normalized = capability.trim().to_lowercase().replace('-', "_");
        normalized == "shell" || normalized == "run" || normalized == "exec"
    })
}

fn manifest_paths(root: &Path, table: &toml::Table, key: &str) -> Vec<PathBuf> {
    toml_string_array(table, key)
        .into_iter()
        .filter_map(|value| safe_relative_path(&value))
        .map(|path| root.join(path))
        .collect()
}

fn safe_relative_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value.trim());
    if path.as_os_str().is_empty() || path.is_absolute() {
        return None;
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return None;
    }
    Some(path.to_path_buf())
}

fn toml_required_string(table: &toml::Table, key: &str) -> Option<String> {
    toml_optional_string(table, key).filter(|value| !value.is_empty())
}

fn toml_optional_string(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get(key)?
        .as_str()
        .map(|value| value.trim().to_string())
}

fn toml_string_array(table: &toml::Table, key: &str) -> Vec<String> {
    table
        .get(key)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zeshicast-extension-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn manifest_parses_relative_commands_and_scripts() {
        let root = PathBuf::from("/tmp/example-extension");
        let manifest = parse_extension_manifest(
            &root,
            r#"
id = "example.git-tools"
name = "Git Tools"
version = "0.1.0"
capabilities = ["shell", "filesystem"]
commands = ["git-log.toml"]
scripts = ["scripts/status.sh"]
"#,
        )
        .unwrap();

        assert_eq!(manifest.origin.id, "example.git-tools");
        assert_eq!(manifest.origin.name, "Git Tools");
        assert_eq!(manifest.origin.version, "0.1.0");
        assert_eq!(manifest.origin.capabilities, vec!["shell", "filesystem"]);
        assert_eq!(manifest.commands, vec![root.join("git-log.toml")]);
        assert_eq!(manifest.scripts, vec![root.join("scripts/status.sh")]);
    }

    #[test]
    fn manifest_rejects_absolute_and_parent_paths() {
        let root = PathBuf::from("/tmp/example-extension");
        let manifest = parse_extension_manifest(
            &root,
            r#"
id = "example.safe"
name = "Safe"
version = "0.1.0"
commands = ["/etc/passwd", "../escape.toml", "ok.toml"]
"#,
        )
        .unwrap();

        assert_eq!(manifest.commands, vec![root.join("ok.toml")]);
    }

    #[test]
    fn manifest_commands_rejects_non_toml() {
        let root = PathBuf::from("/tmp/example-extension");
        let manifest = parse_extension_manifest(
            &root,
            r#"
id = "example.mixed"
name = "Mixed"
version = "0.1.0"
protocol = 1
capabilities = ["shell"]
commands = ["git-log.toml", "bin/tool"]
binaries = ["bin/tool"]
"#,
        )
        .unwrap();

        // B-5: `commands` is TOML-only; executables live in `binaries`.
        assert_eq!(manifest.commands, vec![root.join("git-log.toml")]);
        assert_eq!(manifest.binaries, vec![root.join("bin/tool")]);
    }

    #[test]
    fn manifest_binaries_require_shell() {
        let root = PathBuf::from("/tmp/example-extension");
        let denied = parse_extension_manifest(
            &root,
            r#"
id = "example.no-shell"
name = "No Shell"
version = "0.1.0"
capabilities = ["network"]
binaries = ["bin/tool"]
"#,
        )
        .unwrap();
        assert!(denied.binaries.is_empty());
        assert_eq!(
            denied.declared_binaries.len(),
            1,
            "still listed, for the extension screen"
        );
        assert!(
            denied
                .binaries_blocked
                .as_deref()
                .unwrap()
                .contains("shell")
        );

        let granted = parse_extension_manifest(
            &root,
            r#"
id = "example.with-shell"
name = "With Shell"
version = "0.1.0"
protocol = 1
capabilities = ["shell"]
binaries = ["bin/tool"]
"#,
        )
        .unwrap();
        assert_eq!(granted.binaries, vec![root.join("bin/tool")]);
    }

    fn binary_manifest(protocol_line: &str) -> ExtensionManifest {
        let root = PathBuf::from("/tmp/example-extension");
        parse_extension_manifest(
            &root,
            &format!(
                r#"
id = "example.rpc"
name = "Rpc"
version = "0.1.0"
{protocol_line}
capabilities = ["shell"]
commands = ["one.toml"]
binaries = ["bin/tool"]
"#
            ),
        )
        .unwrap()
    }

    #[test]
    fn binaries_load_when_the_protocol_matches() {
        let manifest = binary_manifest("protocol = 1");
        assert_eq!(manifest.binaries.len(), 1);
        assert_eq!(manifest.binaries_blocked, None);
    }

    #[test]
    fn binaries_are_refused_for_another_or_missing_protocol() {
        for (line, expected) in [
            ("", "lacks protocol = 1"),
            ("protocol = 2", "needs protocol 2"),
            ("protocol = 0", "speaks protocol 0"),
            ("protocol = \"1\"", "must be a number"),
        ] {
            let manifest = binary_manifest(line);
            assert!(
                manifest.binaries.is_empty(),
                "{line:?} must not load binaries"
            );
            let reason = manifest.binaries_blocked.expect("a reason is kept");
            assert!(reason.starts_with("Blocked"), "{reason}");
            assert!(reason.contains(expected), "{line:?}: {reason}");
            // The rest of the extension is not a JSON-RPC client and stays.
            assert_eq!(manifest.commands.len(), 1, "{line:?}");
        }
    }

    #[test]
    fn manifests_without_binaries_need_no_protocol() {
        let manifest = parse_extension_manifest(
            Path::new("/tmp/example-extension"),
            "id = \"a\"\nname = \"A\"\nversion = \"1\"\ncommands = [\"one.toml\"]\n",
        )
        .unwrap();
        assert_eq!(manifest.binaries_blocked, None);
    }

    fn repo_file(path: &str) -> String {
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
    }

    /// The template in `packaging/examples/extension-template` is what
    /// `docs/extensions.md` tells authors to copy: it must load as is.
    #[test]
    fn the_extension_template_loads_with_its_binary() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/examples/extension-template");
        let manifest = parse_extension_manifest(
            &root,
            &repo_file("packaging/examples/extension-template/extension.toml"),
        )
        .expect("the template manifest parses");
        assert_eq!(manifest.binaries_blocked, None);
        assert_eq!(manifest.binaries, vec![root.join("bin/template.sh")]);
        for path in manifest.commands.iter().chain(&manifest.binaries) {
            assert!(path.exists(), "{} is missing", path.display());
        }
    }

    /// `resources/extension.schema.json` is the editor-side copy of the
    /// rules above; it must name the same protocol and capabilities.
    #[test]
    fn the_manifest_schema_matches_the_code() {
        let schema: serde_json::Value =
            serde_json::from_str(&repo_file("resources/extension.schema.json")).unwrap();
        assert_eq!(
            schema["properties"]["protocol"]["const"],
            crate::services::extension_protocol::PROTOCOL_VERSION
        );
        let listed: Vec<&str> = schema["properties"]["capabilities"]["items"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let parsed = crate::search::commands::parse_capabilities(
            &listed
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
        );
        assert_eq!(parsed.len(), listed.len(), "every listed capability parses");
        for capability in parsed {
            assert!(
                listed.contains(&capability.label()),
                "{}",
                capability.label()
            );
        }
        // And the other way round: a new capability must reach the schema.
        // The match stops compiling when a variant is added.
        use crate::action::Capability;
        let every = [
            Capability::Shell,
            Capability::Network,
            Capability::Filesystem,
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
            Capability::OpenUrl,
            Capability::OpenPath,
        ];
        for capability in every {
            match capability {
                Capability::Shell
                | Capability::Network
                | Capability::Filesystem
                | Capability::ClipboardRead
                | Capability::ClipboardWrite
                | Capability::OpenUrl
                | Capability::OpenPath => {}
            }
            assert!(
                listed.contains(&capability.label()),
                "{}",
                capability.label()
            );
        }
    }

    #[test]
    fn load_manifests_scans_one_level() {
        let config = test_dir("load");
        let extension_dir = config.join("extensions/example");
        fs::create_dir_all(&extension_dir).unwrap();
        fs::write(
            extension_dir.join("extension.toml"),
            r#"
id = "example.one"
name = "One"
version = "1.0.0"
commands = ["one.toml"]
"#,
        )
        .unwrap();

        let manifests = load_extension_manifests(&config);
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].origin.id, "example.one");
        assert_eq!(manifests[0].commands, vec![extension_dir.join("one.toml")]);

        fs::remove_dir_all(config).ok();
    }
}
