//! `zeshicast --extension-lint <dir>` (Q5.3).
//!
//! The loader is deliberately forgiving: a typo in a manifest key, a script
//! without metadata or a command without `shell` is skipped or shown as
//! blocked, never an error. That keeps the palette working, but it leaves an
//! author guessing why their extension does nothing. The linter runs the same
//! parsers the host uses and says, for each such case, what was ignored and
//! why. The rules mirror `resources/extension.schema.json` and
//! `docs/extensions.md`.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::action::Capability;
use crate::search::commands::{parse_capabilities, parse_command_entry};
use crate::services::extension_protocol::PROTOCOL_VERSION;

/// Keys the host reads from `extension.toml`.
const MANIFEST_KEYS: [&str; 8] = [
    "id",
    "name",
    "version",
    "protocol",
    "capabilities",
    "commands",
    "scripts",
    "binaries",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The host ignores or blocks something the author meant to ship.
    Error,
    /// It works, but not quite as written.
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// Stable rule id, for scripts and for searching the docs.
    pub rule: &'static str,
    pub file: PathBuf,
    pub message: String,
}

impl Finding {
    fn error(rule: &'static str, file: &Path, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            rule,
            file: file.to_path_buf(),
            message: message.into(),
        }
    }

    fn warning(rule: &'static str, file: &Path, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(rule, file, message)
        }
    }
}

/// Every problem in the extension at `dir`, errors first.
pub fn lint_extension(dir: &Path) -> Vec<Finding> {
    let mut findings = lint_unsorted(dir);
    findings.sort_by_key(|finding| finding.severity != Severity::Error);
    findings
}

pub fn has_errors(findings: &[Finding]) -> bool {
    findings
        .iter()
        .any(|finding| finding.severity == Severity::Error)
}

/// One line per finding: `error[rule] file: message`.
pub fn render_text(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|finding| {
            let severity = match finding.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            format!(
                "{severity}[{}] {}: {}\n",
                finding.rule,
                finding.file.display(),
                finding.message
            )
        })
        .collect()
}

pub fn render_json(findings: &[Finding]) -> String {
    serde_json::to_string_pretty(findings).unwrap_or_else(|_| "[]".to_string())
}

fn lint_unsorted(dir: &Path) -> Vec<Finding> {
    let manifest_path = dir.join("extension.toml");
    let content = match std::fs::read_to_string(&manifest_path) {
        Ok(content) => content,
        Err(error) => {
            return vec![Finding::error(
                "manifest-missing",
                &manifest_path,
                format!("cannot read the manifest: {error}"),
            )];
        }
    };
    let table = match content.parse::<toml::Table>() {
        Ok(table) => table,
        Err(error) => {
            // toml's message already names the line and column.
            return vec![Finding::error(
                "toml-syntax",
                &manifest_path,
                error.to_string().trim().to_string(),
            )];
        }
    };

    let mut findings = Vec::new();
    let file = manifest_path.as_path();

    for key in ["id", "name", "version"] {
        match table.get(key) {
            Some(toml::Value::String(value)) if !value.trim().is_empty() => {}
            Some(toml::Value::String(_)) | None => findings.push(Finding::error(
                "required-key",
                file,
                format!("`{key}` is required; without it the extension is not loaded"),
            )),
            Some(_) => findings.push(Finding::error(
                "required-key",
                file,
                format!("`{key}` must be a string; the extension is not loaded"),
            )),
        }
    }

    for key in table.keys() {
        if !MANIFEST_KEYS.contains(&key.as_str()) {
            let hint = closest(key, &MANIFEST_KEYS)
                .map(|known| format!(" (did you mean `{known}`?)"))
                .unwrap_or_default();
            findings.push(Finding::error(
                "unknown-key",
                file,
                format!("unknown key `{key}` is ignored{hint}"),
            ));
        }
    }

    let capability_names = string_array(&table, "capabilities", file, &mut findings);
    let capabilities = lint_capabilities(&capability_names, file, &mut findings);
    let has_shell = capabilities.contains(&Capability::Shell);

    let commands = lint_paths(dir, &table, "commands", file, &mut findings);
    let scripts = lint_paths(dir, &table, "scripts", file, &mut findings);
    let binaries = lint_paths(dir, &table, "binaries", file, &mut findings);

    for path in &commands {
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            findings.push(Finding::error(
                "command-not-toml",
                file,
                format!(
                    "`commands` takes *.toml files only; {} is ignored (executables go in `binaries`)",
                    relative(dir, path)
                ),
            ));
        } else if path.is_file() {
            lint_command(path, &capabilities, &mut findings);
        }
    }

    for path in scripts.iter().filter(|path| path.is_file()) {
        if crate::search::scripts::parse_script_entry(path).is_none() {
            findings.push(Finding::error(
                "script-metadata",
                path,
                "not loaded: needs `@raycast.schemaVersion` and `@raycast.title` comments in the first 50 lines",
            ));
        }
    }
    if !scripts.is_empty() && !has_shell {
        findings.push(Finding::error(
            "needs-shell",
            file,
            "scripts are blocked: the manifest lacks capabilities = [\"shell\"]",
        ));
    }

    if !binaries.is_empty() {
        if !has_shell {
            findings.push(Finding::error(
                "needs-shell",
                file,
                "`binaries` are not loaded: the manifest lacks capabilities = [\"shell\"]",
            ));
        }
        lint_protocol(table.get("protocol"), file, &mut findings);
        for path in binaries.iter().filter(|path| path.is_file()) {
            if !is_executable(path) {
                findings.push(Finding::error(
                    "binary-not-executable",
                    path,
                    "not executable; run `chmod +x` on it",
                ));
            }
        }
    } else if table.contains_key("protocol") {
        findings.push(Finding::warning(
            "protocol",
            file,
            "`protocol` only matters for `binaries`, and there are none",
        ));
    }

    findings
}

fn lint_protocol(value: Option<&toml::Value>, file: &Path, findings: &mut Vec<Finding>) {
    let message = match value {
        Some(toml::Value::Integer(version)) if *version == i64::from(PROTOCOL_VERSION) => return,
        None => {
            format!("`binaries` need `protocol = {PROTOCOL_VERSION}`; without it they never run")
        }
        Some(toml::Value::Integer(version)) => format!(
            "`protocol = {version}` is not supported (this zeshicast speaks {PROTOCOL_VERSION}); the binaries never run"
        ),
        Some(_) => format!("`protocol` must be a number: `protocol = {PROTOCOL_VERSION}`"),
    };
    findings.push(Finding::error("protocol", file, message));
}

fn lint_capabilities(
    names: &[String],
    file: &Path,
    findings: &mut Vec<Finding>,
) -> Vec<Capability> {
    let mut capabilities = Vec::new();
    for name in names {
        let Some(capability) = parse_capabilities(std::slice::from_ref(name)).pop() else {
            let canonical = canonical_capabilities();
            let hint = closest(name, &canonical)
                .map(|known| format!(" (did you mean `{known}`?)"))
                .unwrap_or_default();
            findings.push(Finding::error(
                "capability-unknown",
                file,
                format!("unknown capability `{name}` grants nothing{hint}"),
            ));
            continue;
        };
        if name != capability.label() {
            findings.push(Finding::warning(
                "capability-alias",
                file,
                format!(
                    "`{name}` is read as `{}`; write that instead",
                    capability.label()
                ),
            ));
        }
        capabilities.push(capability);
    }
    capabilities
}

/// A command inside an extension gets `its permissions ∩ the manifest's`.
fn lint_command(path: &Path, manifest: &[Capability], findings: &mut Vec<Finding>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    if let Err(error) = content.parse::<toml::Table>() {
        findings.push(Finding::error(
            "toml-syntax",
            path,
            error.to_string().trim().to_string(),
        ));
        return;
    }
    let Some(command) = parse_command_entry(&content) else {
        findings.push(Finding::error(
            "command-invalid",
            path,
            "not loaded: needs `name`, and `command` (shell/json mode) or `program` (argv mode)",
        ));
        return;
    };

    let declared = parse_capabilities(&command.permissions);
    if !declared.contains(&Capability::Shell) {
        findings.push(Finding::error(
            "needs-shell",
            path,
            "blocked: every command runs code and needs permissions = [\"shell\"]",
        ));
    } else if !manifest.contains(&Capability::Shell) {
        findings.push(Finding::error(
            "needs-shell",
            path,
            "blocked: the manifest's capabilities lack \"shell\", so the command cannot have it",
        ));
    }
    for capability in declared {
        if !manifest.contains(&capability) && capability != Capability::Shell {
            findings.push(Finding::warning(
                "permission-capped",
                path,
                format!(
                    "asks for `{}`, which the manifest does not grant; it is dropped",
                    capability.label()
                ),
            ));
        }
    }
}

/// The entries of `key`, joined to `dir`, after reporting the ones the
/// loader drops. Existence is checked here too.
fn lint_paths(
    dir: &Path,
    table: &toml::Table,
    key: &str,
    file: &Path,
    findings: &mut Vec<Finding>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for value in string_array(table, key, file, findings) {
        let path = Path::new(&value);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            findings.push(Finding::error(
                "path-unsafe",
                file,
                format!("`{value}` in `{key}` is ignored: paths must stay inside the extension directory"),
            ));
            continue;
        }
        let full = dir.join(path);
        if !full.is_file() {
            findings.push(Finding::error(
                "path-missing",
                file,
                format!("`{value}` in `{key}` does not exist"),
            ));
        }
        paths.push(full);
    }
    paths
}

fn string_array(
    table: &toml::Table,
    key: &str,
    file: &Path,
    findings: &mut Vec<Finding>,
) -> Vec<String> {
    let Some(value) = table.get(key) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        findings.push(Finding::error(
            "wrong-type",
            file,
            format!("`{key}` must be a list of strings: {key} = [\"…\"]"),
        ));
        return Vec::new();
    };
    let mut strings = Vec::new();
    for item in items {
        match item.as_str().map(str::trim) {
            Some(text) if !text.is_empty() => strings.push(text.to_string()),
            _ => findings.push(Finding::error(
                "wrong-type",
                file,
                format!("`{key}` has an entry that is not a non-empty string: {item}"),
            )),
        }
    }
    strings
}

fn canonical_capabilities() -> Vec<&'static str> {
    [
        Capability::Shell,
        Capability::Network,
        Capability::Filesystem,
        Capability::ClipboardRead,
        Capability::ClipboardWrite,
        Capability::OpenUrl,
        Capability::OpenPath,
    ]
    .into_iter()
    .map(Capability::label)
    .collect()
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn relative(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir).unwrap_or(path).display().to_string()
}

/// The known word within two edits of `word`, for "did you mean".
fn closest<'a>(word: &str, known: &[&'a str]) -> Option<&'a str> {
    known
        .iter()
        .map(|candidate| (edit_distance(word, candidate), *candidate))
        .filter(|(distance, _)| *distance <= 2)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != *cb);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Extension(PathBuf);

    impl Extension {
        fn new(tag: &str, manifest: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "zeshicast-lint-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("extension.toml"), manifest).unwrap();
            Self(dir)
        }

        fn file(self, name: &str, content: &str, executable: bool) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            let mode = if executable { 0o755 } else { 0o644 };
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            self
        }

        fn rules(&self) -> Vec<(Severity, &'static str)> {
            lint_extension(&self.0)
                .into_iter()
                .map(|finding| (finding.severity, finding.rule))
                .collect()
        }

        fn messages(&self) -> String {
            render_text(&lint_extension(&self.0))
        }
    }

    impl Drop for Extension {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const HEAD: &str = "id = \"t.x\"\nname = \"X\"\nversion = \"1\"\n";
    const SHELL_COMMAND: &str =
        "name = \"Hi\"\nmode = \"argv\"\nprogram = \"true\"\npermissions = [\"shell\"]\n";
    const SCRIPT: &str = "#!/bin/sh\n# @raycast.schemaVersion 1\n# @raycast.title Hi\ntrue\n";

    fn errors(rules: &[(Severity, &'static str)]) -> Vec<&'static str> {
        rules
            .iter()
            .filter(|(severity, _)| *severity == Severity::Error)
            .map(|(_, rule)| *rule)
            .collect()
    }

    #[test]
    fn the_template_extension_passes() {
        let template =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/examples/extension-template");
        let findings = lint_extension(&template);
        assert!(findings.is_empty(), "{}", render_text(&findings));
    }

    #[test]
    fn a_missing_manifest_and_broken_toml_are_errors() {
        let dir = std::env::temp_dir().join(format!("zeshicast-lint-none-{}", std::process::id()));
        assert_eq!(lint_extension(&dir)[0].rule, "manifest-missing");

        let broken = Extension::new("toml", "id = \"x\nname = 1");
        assert_eq!(errors(&broken.rules()), ["toml-syntax"]);
        assert!(broken.messages().contains("line"), "{}", broken.messages());
    }

    #[test]
    fn required_keys_must_be_non_empty_strings() {
        let ext = Extension::new("required", "id = \"\"\nname = 3\n");
        assert_eq!(
            errors(&ext.rules()),
            ["required-key", "required-key", "required-key"]
        );
    }

    #[test]
    fn unknown_keys_are_errors_with_a_suggestion() {
        let ext = Extension::new("unknown", &format!("{HEAD}capabilites = [\"shell\"]\n"));
        assert_eq!(errors(&ext.rules()), ["unknown-key"]);
        assert!(ext.messages().contains("did you mean `capabilities`"));
    }

    #[test]
    fn unknown_capabilities_are_errors_and_aliases_warnings() {
        let ext = Extension::new(
            "caps",
            &format!("{HEAD}capabilities = [\"shel\", \"net\"]\n"),
        );
        assert_eq!(
            ext.rules(),
            [
                (Severity::Error, "capability-unknown"),
                (Severity::Warning, "capability-alias")
            ]
        );
        assert!(ext.messages().contains("did you mean `shell`"));
        assert!(ext.messages().contains("read as `network`"));
    }

    #[test]
    fn absolute_parent_and_missing_paths_are_errors() {
        let ext = Extension::new(
            "paths",
            &format!(
                "{HEAD}capabilities = [\"shell\"]\ncommands = [\"/etc/a.toml\", \"../b.toml\", \"gone.toml\"]\n"
            ),
        );
        assert_eq!(
            errors(&ext.rules()),
            ["path-unsafe", "path-unsafe", "path-missing"]
        );
    }

    #[test]
    fn lists_must_hold_strings() {
        let ext = Extension::new(
            "types",
            &format!("{HEAD}commands = \"a.toml\"\nscripts = [1]\n"),
        );
        assert_eq!(errors(&ext.rules()), ["wrong-type", "wrong-type"]);
    }

    #[test]
    fn binaries_need_shell_protocol_and_the_exec_bit() {
        let ext = Extension::new("binaries", &format!("{HEAD}binaries = [\"bin/tool\"]\n")).file(
            "bin/tool",
            "#!/bin/sh\n",
            false,
        );
        assert_eq!(
            errors(&ext.rules()),
            ["needs-shell", "protocol", "binary-not-executable"]
        );

        let future = Extension::new(
            "future",
            &format!("{HEAD}protocol = 7\ncapabilities = [\"shell\"]\nbinaries = [\"bin/tool\"]\n"),
        )
        .file("bin/tool", "#!/bin/sh\n", true);
        assert_eq!(errors(&future.rules()), ["protocol"]);
        assert!(
            future
                .messages()
                .contains("`protocol = 7` is not supported")
        );
    }

    #[test]
    fn a_protocol_without_binaries_is_a_warning() {
        let ext = Extension::new("protocol-only", &format!("{HEAD}protocol = 1\n"));
        assert_eq!(ext.rules(), [(Severity::Warning, "protocol")]);
    }

    #[test]
    fn commands_must_be_toml_valid_and_have_shell() {
        let ext = Extension::new(
            "commands",
            &format!(
                "{HEAD}capabilities = [\"shell\"]\ncommands = [\"tool\", \"bad.toml\", \"empty.toml\", \"no-shell.toml\"]\n"
            ),
        )
        .file("tool", "#!/bin/sh\n", true)
        .file("bad.toml", "name = ", false)
        .file("empty.toml", "description = \"no name\"\n", false)
        .file("no-shell.toml", "name = \"A\"\ncommand = \"true\"\n", false);
        assert_eq!(
            errors(&ext.rules()),
            [
                "command-not-toml",
                "toml-syntax",
                "command-invalid",
                "needs-shell"
            ]
        );
    }

    #[test]
    fn the_manifest_ceiling_blocks_and_caps_command_permissions() {
        let command = "name = \"A\"\ncommand = \"true\"\npermissions = [\"shell\", \"network\"]\n";
        let ext = Extension::new("ceiling", &format!("{HEAD}commands = [\"a.toml\"]\n"))
            .file("a.toml", command, false);
        assert_eq!(
            ext.rules(),
            [
                (Severity::Error, "needs-shell"),
                (Severity::Warning, "permission-capped")
            ]
        );

        let granted = Extension::new(
            "granted",
            &format!("{HEAD}capabilities = [\"shell\"]\ncommands = [\"a.toml\"]\n"),
        )
        .file("a.toml", SHELL_COMMAND, false);
        assert!(granted.rules().is_empty(), "{}", granted.messages());
    }

    #[test]
    fn scripts_need_metadata_and_shell() {
        let ext = Extension::new(
            "scripts",
            &format!("{HEAD}scripts = [\"ok.sh\", \"bare.sh\"]\n"),
        )
        .file("ok.sh", SCRIPT, true)
        .file("bare.sh", "#!/bin/sh\ntrue\n", true);
        assert_eq!(errors(&ext.rules()), ["script-metadata", "needs-shell"]);
    }

    #[test]
    fn json_output_is_machine_readable() {
        let ext = Extension::new("json", &format!("{HEAD}protocol = 1\n"));
        let parsed: serde_json::Value =
            serde_json::from_str(&render_json(&lint_extension(&ext.0))).unwrap();
        assert_eq!(parsed[0]["severity"], "warning");
        assert_eq!(parsed[0]["rule"], "protocol");
        assert!(
            parsed[0]["file"]
                .as_str()
                .unwrap()
                .ends_with("extension.toml")
        );
        assert!(!has_errors(&lint_extension(&ext.0)));
    }

    #[test]
    fn suggestions_stay_close() {
        assert_eq!(closest("binarys", &MANIFEST_KEYS), Some("binaries"));
        assert_eq!(closest("description", &MANIFEST_KEYS), None);
    }
}
