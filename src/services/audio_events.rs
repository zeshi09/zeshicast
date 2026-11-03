//! Volume changes as they happen (L3): `pw-dump --monitor` only signals
//! that the PipeWire graph changed; one `wpctl get-volume` per burst then
//! reads the levels. PipeWire is silent while idle, so this costs nothing
//! until something changes, and it keeps the levels fresh while the
//! palette is hidden.

use std::io::BufRead;
use std::time::Duration;

use super::audio::AudioDeviceSnapshot;

/// The default output and input levels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioLevels {
    pub output: Option<AudioDeviceSnapshot>,
    pub input: Option<AudioDeviceSnapshot>,
}

/// Splits `pw-dump --monitor` output into its batches: each is a JSON array
/// whose closing `]` stands alone at the start of a line.
#[derive(Default)]
pub(crate) struct BatchSplitter {
    current: String,
}

impl BatchSplitter {
    /// Feed one line; returns a finished batch.
    pub(crate) fn push(&mut self, line: &str) -> Option<String> {
        self.current.push_str(line);
        self.current.push('\n');
        if line.trim_end() == "]" {
            return Some(std::mem::take(&mut self.current));
        }
        None
    }
}

/// Whether a batch can change a default device's level: an audio sink or
/// source changed, or the default device did.
pub(crate) fn touches_levels(batch: &str) -> bool {
    [
        "\"Audio/Sink\"",
        "\"Audio/Source\"",
        "default.audio.sink",
        "default.audio.source",
    ]
    .iter()
    .any(|needle| batch.contains(needle))
}

/// Which change to show as an OSD, if any (O3). The microphone shows only
/// when its mute flips: calls and browsers move its volume on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelChange {
    Output { percent: u8, muted: bool },
    InputMute { muted: bool },
}

pub(crate) fn level_changes(before: &AudioLevels, after: &AudioLevels) -> Vec<LevelChange> {
    let mut changes = Vec::new();
    if let (Some(old), Some(new)) = (&before.output, &after.output)
        && (old.volume_percent != new.volume_percent || old.muted != new.muted)
    {
        changes.push(LevelChange::Output {
            percent: new.volume_percent,
            muted: new.muted,
        });
    }
    if let (Some(old), Some(new)) = (&before.input, &after.input)
        && old.muted != new.muted
    {
        changes.push(LevelChange::InputMute { muted: new.muted });
    }
    changes
}

/// Watch PipeWire until `on_levels` returns `false`. Each burst of graph
/// changes is read once, 40 ms after it starts. Returns at once without
/// `pw-dump` (the poller still reads the levels while the palette shows).
pub fn watch(mut on_levels: impl FnMut(AudioLevels) -> bool) {
    loop {
        let child = crate::process::die_with_parent(&mut std::process::Command::new("pw-dump"))
            .args(["--monitor", "--no-colors"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            log::info!("pw-dump not found: volume changes are polled, not watched");
            return;
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        let (batches, receive) = std::sync::mpsc::channel::<bool>();
        // Read on its own thread, so the burst window below can time out.
        std::thread::spawn(move || {
            let mut splitter = BatchSplitter::default();
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(batch) = splitter.push(&line)
                    && batches.send(touches_levels(&batch)).is_err()
                {
                    break;
                }
            }
        });
        loop {
            match receive.recv() {
                Ok(true) => {}
                Ok(false) => continue,
                Err(_) => break,
            }
            // Let the rest of the burst arrive, then read once.
            while receive.recv_timeout(Duration::from_millis(40)).is_ok() {}
            let levels = AudioLevels {
                output: super::audio::read_default_volume("@DEFAULT_AUDIO_SINK@"),
                input: super::audio::read_default_volume("@DEFAULT_AUDIO_SOURCE@"),
            };
            if !on_levels(levels) {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        // PipeWire restarted (a session switch): come back shortly.
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(percent: u8, muted: bool) -> Option<AudioDeviceSnapshot> {
        Some(AudioDeviceSnapshot {
            name: None,
            volume_percent: percent,
            muted,
        })
    }

    #[test]
    fn monitor_output_splits_into_batches() {
        let mut splitter = BatchSplitter::default();
        let lines = [
            "[",
            "  {",
            "    \"id\": 41,",
            "  }",
            "]",
            "[",
            "  { \"id\": 7 }",
            "]",
        ];
        let batches: Vec<String> = lines
            .iter()
            .filter_map(|line| splitter.push(line))
            .collect();
        assert_eq!(batches.len(), 2);
        assert!(batches[0].contains("\"id\": 41"));
        assert!(batches[1].contains("\"id\": 7"));
    }

    #[test]
    fn only_sink_source_and_default_changes_count() {
        assert!(touches_levels(
            r#"[{"info":{"props":{"media.class":"Audio/Sink"}}}]"#
        ));
        assert!(touches_levels(
            r#"[{"metadata":[{"key":"default.audio.sink"}]}]"#
        ));
        assert!(!touches_levels(
            r#"[{"info":{"props":{"media.class":"Stream/Output/Audio"}}}]"#
        ));
        assert!(!touches_levels(
            r#"[{"info":{"props":{"media.class":"Video/Source"}}}]"#
        ));
    }

    #[test]
    fn microphone_volume_drift_is_not_a_change_to_show() {
        let before = AudioLevels {
            output: device(40, false),
            input: device(70, false),
        };
        let drift = AudioLevels {
            output: device(40, false),
            input: device(55, false),
        };
        assert!(
            level_changes(&before, &drift).is_empty(),
            "AGC moved the mic: no OSD"
        );
        let louder = AudioLevels {
            output: device(45, false),
            input: device(70, false),
        };
        assert_eq!(
            level_changes(&before, &louder),
            vec![LevelChange::Output {
                percent: 45,
                muted: false
            }]
        );
        let mic_off = AudioLevels {
            output: device(40, false),
            input: device(70, true),
        };
        assert_eq!(
            level_changes(&before, &mic_off),
            vec![LevelChange::InputMute { muted: true }]
        );
        assert!(
            level_changes(&AudioLevels::default(), &louder).is_empty(),
            "first read: nothing to compare"
        );
    }
}
