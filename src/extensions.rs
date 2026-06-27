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
    pub(crate) commands: Vec<PathBuf>,
    pub(crate) scripts: Vec<PathBuf>,
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
            if !root.is_dir() {
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
    let commands = manifest_paths(root, &table, "commands");
    let scripts = manifest_paths(root, &table, "scripts");

    Some(ExtensionManifest {
        origin: ExtensionOrigin {
            id,
            name,
            version,
            capabilities,
        },
        commands,
        scripts,
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
