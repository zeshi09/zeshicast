use std::process::Command;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioSnapshot {
    pub output: Option<AudioDeviceSnapshot>,
    pub input: Option<AudioDeviceSnapshot>,
    pub streams: Vec<AudioStreamSnapshot>,
    /// All available output (sink) devices, default flagged.
    pub output_devices: Vec<AudioDeviceOption>,
    /// All available input (source) devices, default flagged.
    pub input_devices: Vec<AudioDeviceOption>,
}

/// A selectable audio device (sink or source) from `wpctl status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceOption {
    pub id: Option<u32>,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceSnapshot {
    pub name: Option<String>,
    pub volume_percent: u8,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioStreamSnapshot {
    pub id: Option<u32>,
    pub name: String,
    pub volume_percent: Option<u8>,
    pub muted: bool,
}

pub fn audio_snapshot() -> AudioSnapshot {
    let status = read_wpctl_status().unwrap_or_default();
    AudioSnapshot {
        output: read_wpctl_volume("@DEFAULT_AUDIO_SINK@").map(|mut device| {
            device.name = parse_wpctl_status_default_name(&status, "Sinks");
            device
        }),
        input: read_wpctl_volume("@DEFAULT_AUDIO_SOURCE@").map(|mut device| {
            device.name = parse_wpctl_status_default_name(&status, "Sources");
            device
        }),
        streams: parse_wpctl_streams(&status),
        output_devices: parse_wpctl_status_devices(&status, "Sinks"),
        input_devices: parse_wpctl_status_devices(&status, "Sources"),
    }
}

/// Parse all device rows under a `wpctl status` section (e.g. "Sinks" /
/// "Sources"), flagging the default one (marked with `*`).
fn parse_wpctl_status_devices(output: &str, section: &str) -> Vec<AudioDeviceOption> {
    let mut devices = Vec::new();
    let mut in_audio = false;
    let mut in_section = false;

    for line in output.lines() {
        // Top-level headers ("Audio", "Video", …) sit flush-left. `wpctl status`
        // repeats Sinks/Sources under Video, so scope strictly to the Audio tree.
        if !line.is_empty() && !line.starts_with(|c: char| c.is_whitespace()) {
            in_audio = line.trim() == "Audio";
            in_section = false;
            continue;
        }

        let trimmed = clean_wpctl_tree_prefix(line);
        if trimmed.ends_with(':') {
            in_section = in_audio && trimmed.trim_end_matches(':') == section;
            continue;
        }

        if !in_section || trimmed.is_empty() || !trimmed.contains('.') {
            continue;
        }

        let is_default = trimmed.starts_with('*');
        if let Some((id, name, _, _)) = parse_wpctl_node_line(trimmed) {
            devices.push(AudioDeviceOption {
                id,
                name,
                is_default,
            });
        }
    }

    devices
}

fn read_wpctl_status() -> Option<String> {
    let output = Command::new("wpctl").arg("status").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn read_wpctl_volume(target: &str) -> Option<AudioDeviceSnapshot> {
    let output = Command::new("wpctl")
        .args(["get-volume", target])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_wpctl_volume(&String::from_utf8_lossy(&output.stdout))
}

fn parse_wpctl_volume(output: &str) -> Option<AudioDeviceSnapshot> {
    let mut volume = None;
    let mut muted = false;

    for part in output.split_whitespace() {
        if part.eq_ignore_ascii_case("[MUTED]") {
            muted = true;
        } else if volume.is_none() {
            volume = part.parse::<f32>().ok();
        }
    }

    let volume = volume?;
    let volume_percent = (volume * 100.0).round().clamp(0.0, 150.0) as u8;
    Some(AudioDeviceSnapshot {
        name: None,
        volume_percent,
        muted,
    })
}

fn parse_wpctl_status_default_name(output: &str, section: &str) -> Option<String> {
    let mut in_section = false;

    for line in output.lines() {
        let trimmed = clean_wpctl_tree_prefix(line);
        if trimmed.ends_with(':') {
            in_section = trimmed.trim_end_matches(':') == section;
            continue;
        }

        if in_section && trimmed.contains('*') {
            return parse_wpctl_node_line(trimmed).map(|(_, name, _, _)| name);
        }
    }

    None
}

fn parse_wpctl_streams(output: &str) -> Vec<AudioStreamSnapshot> {
    let mut streams = Vec::new();
    let mut in_streams = false;

    for line in output.lines() {
        let trimmed = clean_wpctl_tree_prefix(line);
        if trimmed.ends_with(':') {
            in_streams = trimmed.trim_end_matches(':') == "Streams";
            continue;
        }

        if !in_streams || trimmed.is_empty() || !trimmed.contains('.') {
            continue;
        }

        if let Some((id, name, volume_percent, muted)) = parse_wpctl_node_line(trimmed) {
            streams.push(AudioStreamSnapshot {
                id,
                name,
                volume_percent,
                muted,
            });
        }
    }

    streams
}

fn parse_wpctl_node_line(line: &str) -> Option<(Option<u32>, String, Option<u8>, bool)> {
    let cleaned = clean_wpctl_tree_prefix(line).trim_start_matches('*').trim();
    let (id_part, rest) = cleaned.split_once('.')?;
    let id = id_part.trim().parse().ok();
    let muted = rest.contains("[MUTED]");
    let volume_percent = rest
        .split("[vol:")
        .nth(1)
        .and_then(|tail| tail.split(']').next())
        .and_then(|value| value.trim().parse::<f32>().ok())
        .map(|value| (value * 100.0).round().clamp(0.0, 150.0) as u8);
    let name = rest
        .split("[vol:")
        .next()
        .unwrap_or(rest)
        .trim()
        .trim_end_matches('*')
        .trim()
        .to_string();

    (!name.is_empty()).then_some((id, name, volume_percent, muted))
}

fn clean_wpctl_tree_prefix(line: &str) -> &str {
    line.trim()
        .trim_start_matches(['│', '├', '└', '─', ' '])
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpctl_parser_extracts_volume() {
        assert_eq!(
            parse_wpctl_volume("Volume: 0.65\n"),
            Some(AudioDeviceSnapshot {
                name: None,
                volume_percent: 65,
                muted: false,
            })
        );
    }

    #[test]
    fn wpctl_parser_extracts_mute_state() {
        assert_eq!(
            parse_wpctl_volume("Volume: 0.42 [MUTED]\n"),
            Some(AudioDeviceSnapshot {
                name: None,
                volume_percent: 42,
                muted: true,
            })
        );
    }

    #[test]
    fn wpctl_status_parser_enumerates_all_devices() {
        let output = r#"
Audio
 ├─ Sinks:
 │  *   48. Creative Stage SE Pro [vol: 0.60]
 │      50. HDMI Output [vol: 0.40]
 ├─ Sources:
 │  *   49. fifine Microphone Pro [vol: 1.00]
 └─ Streams:
        73. Zed [vol: 0.42]
Video
 ├─ Sinks:
 ├─ Sources:
 │  *   87. Web-camera DQ5MF3F1 (V4L2)
"#;

        let sinks = parse_wpctl_status_devices(output, "Sinks");
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[0].name, "Creative Stage SE Pro");
        assert!(sinks[0].is_default);
        assert_eq!(sinks[0].id, Some(48));
        assert_eq!(sinks[1].name, "HDMI Output");
        assert!(!sinks[1].is_default);

        // The Video tree's Sources (webcam) must NOT leak into audio inputs.
        let sources = parse_wpctl_status_devices(output, "Sources");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "fifine Microphone Pro");
        assert!(sources[0].is_default);
    }

    #[test]
    fn wpctl_status_parser_extracts_default_names_and_streams() {
        let output = r#"
Audio
 ├─ Sinks:
 │  *   48. Creative Stage SE Pro [vol: 0.60]
 ├─ Sources:
 │  *   49. fifine Microphone Pro [vol: 1.00]
 └─ Streams:
        73. Zed [vol: 0.42]
        74. Browser [vol: 0.80] [MUTED]
"#;

        assert_eq!(
            parse_wpctl_status_default_name(output, "Sinks").as_deref(),
            Some("Creative Stage SE Pro")
        );
        assert_eq!(
            parse_wpctl_status_default_name(output, "Sources").as_deref(),
            Some("fifine Microphone Pro")
        );
        assert_eq!(
            parse_wpctl_streams(output),
            vec![
                AudioStreamSnapshot {
                    id: Some(73),
                    name: "Zed".to_string(),
                    volume_percent: Some(42),
                    muted: false,
                },
                AudioStreamSnapshot {
                    id: Some(74),
                    name: "Browser".to_string(),
                    volume_percent: Some(80),
                    muted: true,
                },
            ]
        );
    }
}
