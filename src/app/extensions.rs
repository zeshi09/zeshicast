//! The extension screen's data (Q5.5): for each command, script and binary an
//! extension brings, what it asked for, what the manifest allows, what it was
//! granted (`declared ∩ manifest`) and why it is blocked, if it is.

use super::Zeshicast;
use crate::search::commands::{capability_denial, parse_capabilities};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionDetail {
    pub id: String,
    pub name: String,
    pub version: String,
    /// The manifest's `capabilities`: the ceiling for everything below.
    pub manifest: Vec<String>,
    pub items: Vec<ExtensionItemDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionItemDetail {
    /// `Command`, `Script` or `Binary`.
    pub kind: &'static str,
    pub name: String,
    /// What the item asked for (a command's `permissions`; scripts and
    /// binaries always need `shell`).
    pub declared: Vec<String>,
    pub granted: Vec<String>,
    /// Declared but not granted: the manifest does not allow it, or the name
    /// is not a capability.
    pub missing: Vec<String>,
    /// Why the item does not run at all, as its blocked row says.
    pub blocked: Option<String>,
}

impl ExtensionItemDetail {
    fn new(
        kind: &'static str,
        name: String,
        declared: Vec<String>,
        granted: Vec<String>,
        blocked: Option<String>,
    ) -> Self {
        let missing = declared
            .iter()
            .filter(|capability| !granted.contains(capability))
            .cloned()
            .collect();
        Self {
            kind,
            name,
            declared,
            granted,
            missing,
            blocked,
        }
    }
}

impl ExtensionItemDetail {
    /// One line for the extension screen: what was asked, granted and not.
    pub fn grant_line(&self) -> String {
        let list = |names: &[String]| {
            if names.is_empty() {
                "nothing".to_string()
            } else {
                names.join(", ")
            }
        };
        let mut line = format!(
            "asked: {} · granted: {}",
            list(&self.declared),
            list(&self.granted)
        );
        if !self.missing.is_empty() {
            line.push_str(&format!(" · not granted: {}", self.missing.join(", ")));
        }
        line
    }
}

/// Canonical names (`exec` → `shell`); a name that is no capability stays as
/// written, so the screen can show it as missing.
fn canonical(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let label = parse_capabilities(std::slice::from_ref(name))
            .first()
            .map(|capability| capability.label().to_string())
            .unwrap_or_else(|| name.trim().to_string());
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
}

const SHELL: &str = "shell";

impl Zeshicast {
    pub fn extension_details(&self) -> Vec<ExtensionDetail> {
        self.extensions
            .iter()
            .map(|manifest| {
                let origin = &manifest.origin;
                let manifest_caps = canonical(&origin.capabilities);
                let manifest_has_shell = manifest_caps.iter().any(|c| c == SHELL);
                let mut items = Vec::new();

                for entry in &self.commands {
                    if entry.origin.as_ref().map(|o| &o.id) != Some(&origin.id) {
                        continue;
                    }
                    let granted = entry
                        .capabilities
                        .iter()
                        .map(|capability| capability.label().to_string())
                        .collect();
                    items.push(ExtensionItemDetail::new(
                        "Command",
                        entry.name.clone(),
                        canonical(&entry.declared_permissions),
                        granted,
                        capability_denial(entry).map(str::to_string),
                    ));
                }

                for script in &self.scripts {
                    if script.origin.as_ref().map(|o| &o.id) != Some(&origin.id) {
                        continue;
                    }
                    let has_shell = script.has_shell_capability();
                    items.push(ExtensionItemDetail::new(
                        "Script",
                        script.title.clone(),
                        vec![SHELL.to_string()],
                        if has_shell {
                            vec![SHELL.to_string()]
                        } else {
                            Vec::new()
                        },
                        (!has_shell).then(|| {
                            crate::search::scripts::BLOCKED_SCRIPT_LACKS_SHELL.to_string()
                        }),
                    ));
                }

                for binary in &manifest.declared_binaries {
                    let name = binary
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| binary.display().to_string());
                    items.push(ExtensionItemDetail::new(
                        "Binary",
                        name,
                        vec![SHELL.to_string()],
                        if manifest_has_shell {
                            vec![SHELL.to_string()]
                        } else {
                            Vec::new()
                        },
                        manifest.binaries_blocked.clone(),
                    ));
                }

                ExtensionDetail {
                    id: origin.id.clone(),
                    name: origin.name.clone(),
                    version: origin.version.clone(),
                    manifest: manifest_caps,
                    items,
                }
            })
            .collect()
    }

    pub fn extension_detail(&self, id: &str) -> Option<ExtensionDetail> {
        self.extension_details()
            .into_iter()
            .find(|detail| detail.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn app_with(dir: &Path) -> Zeshicast {
        let config_dir = dir.to_path_buf();
        let root = dir.join("extensions/git");
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(
            root.join("log.toml"),
            "name = \"Git Log\"\ncommand = \"git log\"\npermissions = [\"shell\", \"network\", \"teleport\"]\n",
        )
        .unwrap();
        std::fs::write(
            root.join("open.toml"),
            "name = \"Git Web\"\nmode = \"json\"\ncommand = \"echo []\"\npermissions = [\"open_url\"]\n",
        )
        .unwrap();
        std::fs::write(
            root.join("scripts/status.sh"),
            "#!/bin/sh\n# @raycast.schemaVersion 1\n# @raycast.title Git Status\necho\n",
        )
        .unwrap();
        let manifest = crate::extensions::parse_extension_manifest(
            &root,
            r#"
id = "you.git"
name = "Git"
version = "0.1.0"
capabilities = ["exec", "open_url"]
commands = ["log.toml", "open.toml"]
scripts = ["scripts/status.sh"]
binaries = ["bin/git-rpc"]
"#,
        )
        .unwrap();
        let mut app = super::super::tests::test_app(config_dir, std::collections::HashMap::new());
        let (commands, scripts) =
            super::super::load_commands_and_scripts(dir, &[], std::slice::from_ref(&manifest));
        app.commands = commands;
        app.scripts = scripts;
        app.extensions = vec![manifest];
        app
    }

    #[test]
    fn the_extension_screen_explains_each_grant_and_block() {
        let dir =
            std::env::temp_dir().join(format!("zeshicast-extension-detail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let app = app_with(&dir);
        let detail = app
            .extension_detail("you.git")
            .expect("the extension is listed");
        assert_eq!(
            detail.manifest,
            vec!["shell", "open_url"],
            "aliases are named canonically"
        );

        let item = |name: &str| {
            detail
                .items
                .iter()
                .find(|item| item.name == name)
                .unwrap_or_else(|| panic!("{name} missing from {:?}", detail.items))
        };

        let log = item("Git Log");
        assert_eq!(log.declared, vec!["shell", "network", "teleport"]);
        assert_eq!(log.granted, vec!["shell"], "granted = declared ∩ manifest");
        assert_eq!(log.missing, vec!["network", "teleport"]);
        assert_eq!(log.blocked, None);
        assert_eq!(
            log.grant_line(),
            "asked: shell, network, teleport · granted: shell · not granted: network, teleport"
        );

        // A JSON command needs `shell` to produce anything; it asked only for
        // `open_url`, so it is blocked although open_url was granted.
        let web = item("Git Web");
        assert_eq!(web.granted, vec!["open_url"]);
        assert!(web.blocked.as_deref().unwrap().contains("shell"));

        let status = item("Git Status");
        assert_eq!((status.kind, status.blocked.clone()), ("Script", None));
        assert_eq!(status.granted, vec!["shell"]);

        // The binary is listed although the loader refused it.
        let rpc = item("git-rpc");
        assert_eq!(rpc.kind, "Binary");
        assert!(rpc.blocked.as_deref().unwrap().contains("protocol"));

        assert!(app.extension_detail("someone.else").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
