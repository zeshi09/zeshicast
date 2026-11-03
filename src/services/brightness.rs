//! Laptop backlight (B1, B2): read from `/sys/class/backlight`, written
//! through logind's `Session.SetBrightness`, which lets the active
//! session's user change it without root or udev rules. External monitors
//! (DDC/CI) are not handled.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlight {
    /// The directory name under `/sys/class/backlight`, as logind wants it.
    pub name: String,
    pub path: PathBuf,
    pub brightness: u32,
    pub max: u32,
}

impl Backlight {
    pub fn percent(&self) -> u8 {
        if self.max == 0 {
            return 0;
        }
        ((u64::from(self.brightness) * 100 + u64::from(self.max) / 2) / u64::from(self.max))
            .min(100) as u8
    }
}

/// `firmware` interfaces drive the panel through ACPI/EC and are what the
/// firmware keys move; `platform` next; `raw` (the GPU's own) last.
fn type_rank(kind: &str) -> u8 {
    match kind.trim() {
        "firmware" => 0,
        "platform" => 1,
        _ => 2,
    }
}

fn read_u32(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The preferred backlight under `root` (`/sys/class/backlight`).
pub fn find_backlight(root: &Path) -> Option<Backlight> {
    let mut found: Vec<(u8, Backlight)> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let max = read_u32(&path.join("max_brightness")).filter(|max| *max > 0)?;
            let brightness = read_u32(&path.join("brightness"))?;
            let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default();
            Some((
                type_rank(&kind),
                Backlight {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path,
                    brightness: brightness.min(max),
                    max,
                },
            ))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.name.cmp(&b.1.name)));
    found.into_iter().next().map(|(_, backlight)| backlight)
}

/// `zeshicast-gtk --brightness +N|-N|N` (B2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrightnessRequest {
    Up(u8),
    Down(u8),
    Set(u8),
}

/// Lowest a key takes the panel: a black screen is not a brightness.
pub const MIN_PERCENT: u8 = 1;

impl BrightnessRequest {
    pub fn from_args(args: &[String]) -> Option<Result<Self, String>> {
        let position = args.iter().position(|arg| arg == "--brightness")?;
        let usage = "usage: --brightness +N | -N | N (N in percent, 1-100)";
        let Some(value) = args.get(position + 1) else {
            return Some(Err(usage.to_string()));
        };
        let (sign, digits) = match value.as_bytes().first() {
            Some(b'+') => (Some(true), &value[1..]),
            Some(b'-') => (Some(false), &value[1..]),
            _ => (None, value.as_str()),
        };
        let Ok(amount) = digits.trim_end_matches('%').parse::<u8>() else {
            return Some(Err(usage.to_string()));
        };
        if amount == 0 || amount > 100 {
            return Some(Err(usage.to_string()));
        }
        Some(Ok(match sign {
            Some(true) => Self::Up(amount),
            Some(false) => Self::Down(amount),
            None => Self::Set(amount),
        }))
    }

    /// The raw value to write for `backlight`, never below [`MIN_PERCENT`].
    pub fn target(&self, backlight: &Backlight) -> u32 {
        let current = i32::from(backlight.percent());
        let percent = match *self {
            Self::Up(step) => current + i32::from(step),
            Self::Down(step) => current - i32::from(step),
            Self::Set(value) => i32::from(value),
        }
        .clamp(i32::from(MIN_PERCENT), 100) as u64;
        let raw = (percent * u64::from(backlight.max) + 50) / 100;
        (raw as u32).clamp(1, backlight.max)
    }
}

/// Apply `request` to the preferred backlight; the new state on success.
#[cfg(feature = "desktop")]
pub fn apply(request: BrightnessRequest) -> Result<Backlight, String> {
    use glib::variant::ToVariant;
    let root = Path::new("/sys/class/backlight");
    let backlight = find_backlight(root).ok_or("no backlight in /sys/class/backlight")?;
    let value = request.target(&backlight);
    let bus = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE)
        .map_err(|error| format!("no system bus: {error}"))?;
    bus.call_sync(
        Some("org.freedesktop.login1"),
        "/org/freedesktop/login1/session/auto",
        "org.freedesktop.login1.Session",
        "SetBrightness",
        Some(&("backlight", backlight.name.as_str(), value).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        1000,
        gio::Cancellable::NONE,
    )
    .map_err(|error| format!("logind refused the brightness: {error}"))?;
    Ok(Backlight {
        brightness: value,
        ..backlight
    })
}

/// Call `on_change` with the backlight each time the kernel reports a
/// change (firmware keys, another tool), until it returns `false`. Returns
/// at once without a backlight or without `udevadm`.
pub fn watch(mut on_change: impl FnMut(Backlight) -> bool) {
    use std::io::BufRead;
    let root = Path::new("/sys/class/backlight");
    if find_backlight(root).is_none() {
        return;
    }
    let child = crate::process::die_with_parent(&mut std::process::Command::new("udevadm"))
        .args(["monitor", "--kernel", "--subsystem-match=backlight"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        return;
    };
    for line in std::io::BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if !line.contains(" change ") {
            continue;
        }
        if let Some(backlight) = find_backlight(root)
            && !on_change(backlight)
        {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(root: &Path, name: &str, kind: &str, brightness: u32, max: u32) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("type"), format!("{kind}\n")).unwrap();
        std::fs::write(dir.join("brightness"), brightness.to_string()).unwrap();
        std::fs::write(dir.join("max_brightness"), max.to_string()).unwrap();
    }

    #[test]
    fn the_firmware_backlight_is_preferred_over_the_gpu_one() {
        let root = tempfile::tempdir().unwrap();
        device(root.path(), "amdgpu_bl1", "raw", 128, 255);
        device(root.path(), "acpi_video0", "firmware", 30, 100);
        let found = find_backlight(root.path()).unwrap();
        assert_eq!(found.name, "acpi_video0");
        assert_eq!(found.percent(), 30);

        let none = tempfile::tempdir().unwrap();
        assert_eq!(find_backlight(none.path()), None, "a desktop has none");
        device(none.path(), "broken", "raw", 5, 0);
        assert_eq!(
            find_backlight(none.path()),
            None,
            "max 0 is not a backlight"
        );
    }

    #[test]
    fn requests_parse_and_never_go_dark() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        let parse = |list: &[&str]| BrightnessRequest::from_args(&args(list));
        assert_eq!(
            parse(&["z", "--brightness", "+10"]),
            Some(Ok(BrightnessRequest::Up(10)))
        );
        assert_eq!(
            parse(&["z", "--brightness", "-5%"]),
            Some(Ok(BrightnessRequest::Down(5)))
        );
        assert_eq!(
            parse(&["z", "--brightness", "60"]),
            Some(Ok(BrightnessRequest::Set(60)))
        );
        assert!(matches!(parse(&["z", "--brightness", "0"]), Some(Err(_))));
        assert!(matches!(parse(&["z", "--brightness"]), Some(Err(_))));
        assert_eq!(parse(&["z", "--volume", "+5"]), None);

        let panel = Backlight {
            name: "intel_backlight".to_string(),
            path: PathBuf::new(),
            brightness: 9600,
            max: 96000,
        };
        assert_eq!(panel.percent(), 10);
        assert_eq!(BrightnessRequest::Up(10).target(&panel), 19200);
        assert_eq!(
            BrightnessRequest::Down(50).target(&panel),
            960,
            "floored at 1 %"
        );
        assert_eq!(BrightnessRequest::Set(100).target(&panel), 96000);
        let coarse = Backlight {
            max: 7,
            brightness: 1,
            ..panel
        };
        assert_eq!(BrightnessRequest::Down(10).target(&coarse), 1, "never 0");
    }
}
