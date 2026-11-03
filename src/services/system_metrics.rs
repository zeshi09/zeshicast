//! Deeper System Monitor metrics (Q7.3): pressure stall information, I/O
//! per disk and GPU load. Every reader takes its root path and every parser
//! tolerates missing fields, so a kernel without PSI or a GPU without
//! counters leaves that part empty instead of breaking the view.

use std::path::Path;
use std::time::Duration;

/// `/proc/pressure/<resource>`: share of time (10 s average) tasks stalled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pressure {
    /// Some task stalled.
    pub some_avg10: f32,
    /// All tasks stalled; the CPU file has no meaningful `full` line.
    pub full_avg10: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PressureSnapshot {
    pub cpu: Option<Pressure>,
    pub memory: Option<Pressure>,
    pub io: Option<Pressure>,
}

pub(crate) fn parse_pressure(text: &str) -> Option<Pressure> {
    let avg10 = |kind: &str| {
        text.lines()
            .find(|line| line.starts_with(kind))?
            .split_whitespace()
            .find_map(|field| field.strip_prefix("avg10="))?
            .parse::<f32>()
            .ok()
    };
    Some(Pressure {
        some_avg10: avg10("some ")?,
        full_avg10: avg10("full "),
    })
}

pub(crate) fn read_pressure(root: &Path) -> PressureSnapshot {
    let read = |name: &str| {
        std::fs::read_to_string(root.join(name))
            .ok()
            .and_then(|text| parse_pressure(&text))
    };
    PressureSnapshot {
        cpu: read("cpu"),
        memory: read("memory"),
        io: read("io"),
    }
}

/// Cumulative counters of one whole disk from `/proc/diskstats`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiskCounters {
    pub(crate) name: String,
    pub(crate) sectors_read: u64,
    pub(crate) sectors_written: u64,
    /// Milliseconds the device had I/O in flight.
    pub(crate) io_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiskRate {
    pub name: String,
    pub read_bytes_per_sec: f64,
    pub write_bytes_per_sec: f64,
    /// Share of the interval the disk was busy, 0–100.
    pub busy_percent: f32,
}

/// Partitions, loop, RAM and device-mapper devices would count the same
/// I/O twice or show noise; only whole physical disks are listed.
fn is_whole_disk(name: &str) -> bool {
    let virtual_device = ["loop", "ram", "zram", "dm-", "sr", "fd", "md"]
        .iter()
        .any(|prefix| name.starts_with(prefix));
    if virtual_device {
        return false;
    }
    let ends_in_digit = name.chars().last().is_some_and(|c| c.is_ascii_digit());
    if name.starts_with("nvme") || name.starts_with("mmcblk") {
        // nvme0n1 / mmcblk0 are disks, nvme0n1p1 / mmcblk0p1 partitions.
        return !name.rsplit_once('p').is_some_and(|(head, tail)| {
            head.chars().last().is_some_and(|c| c.is_ascii_digit())
                && !tail.is_empty()
                && tail.chars().all(|c| c.is_ascii_digit())
        });
    }
    // sda, vdb, xvdc: a trailing digit means a partition.
    !ends_in_digit
}

pub(crate) fn parse_diskstats(text: &str) -> Vec<DiskCounters> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let name = *fields.get(2)?;
            if !is_whole_disk(name) {
                return None;
            }
            let number = |index: usize| fields.get(index)?.parse::<u64>().ok();
            Some(DiskCounters {
                name: name.to_string(),
                sectors_read: number(5)?,
                sectors_written: number(9)?,
                io_ms: number(12).unwrap_or(0),
            })
        })
        .collect()
}

/// Rates between two samples `elapsed` apart. A disk missing from the
/// first sample, or whose counters went backwards (a reset), is left out.
pub(crate) fn disk_rates(
    previous: &[DiskCounters],
    current: &[DiskCounters],
    elapsed: Duration,
) -> Vec<DiskRate> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return Vec::new();
    }
    current
        .iter()
        .filter_map(|now| {
            let before = previous.iter().find(|disk| disk.name == now.name)?;
            let delta = |a: u64, b: u64| b.checked_sub(a);
            let read = delta(before.sectors_read, now.sectors_read)?;
            let written = delta(before.sectors_written, now.sectors_written)?;
            let busy_ms = delta(before.io_ms, now.io_ms)?;
            Some(DiskRate {
                name: now.name.clone(),
                // diskstats sectors are always 512 bytes.
                read_bytes_per_sec: read as f64 * 512.0 / seconds,
                write_bytes_per_sec: written as f64 * 512.0 / seconds,
                busy_percent: ((busy_ms as f64 / (seconds * 1000.0)) * 100.0).clamp(0.0, 100.0)
                    as f32,
            })
        })
        .collect()
}

pub(crate) fn read_diskstats(path: &Path) -> Vec<DiskCounters> {
    std::fs::read_to_string(path)
        .map(|text| parse_diskstats(&text))
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuSnapshot {
    /// `amdgpu`, `i915`, `nvidia`.
    pub driver: String,
    pub busy_percent: Option<u8>,
    pub vram_used_mib: Option<u64>,
    pub vram_total_mib: Option<u64>,
    /// Current / maximum clock, where that is all the driver exposes (i915).
    pub clock_mhz: Option<(u32, u32)>,
}

/// GPUs under `/sys/class/drm` (`card0`, `card1`; connectors skipped).
pub(crate) fn read_gpus(drm: &Path) -> Vec<GpuSnapshot> {
    let Ok(entries) = std::fs::read_dir(drm) else {
        return Vec::new();
    };
    let mut cards: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("card"))
                .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
        })
        .collect();
    cards.sort();
    cards.iter().filter_map(|card| read_gpu(card)).collect()
}

fn read_number<T: std::str::FromStr>(path: &Path) -> Option<T> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_gpu(card: &Path) -> Option<GpuSnapshot> {
    let device = card.join("device");
    let driver = std::fs::read_link(device.join("driver"))
        .ok()?
        .file_name()?
        .to_string_lossy()
        .to_string();
    let mib =
        |name: &str| read_number::<u64>(&device.join(name)).map(|bytes| bytes / (1024 * 1024));
    let snapshot = match driver.as_str() {
        "amdgpu" => GpuSnapshot {
            busy_percent: read_number::<u8>(&device.join("gpu_busy_percent")).filter(|p| *p <= 100),
            vram_used_mib: mib("mem_info_vram_used"),
            vram_total_mib: mib("mem_info_vram_total"),
            clock_mhz: None,
            driver,
        },
        "i915" | "xe" => GpuSnapshot {
            busy_percent: None,
            vram_used_mib: None,
            vram_total_mib: None,
            clock_mhz: read_number::<u32>(&card.join("gt_cur_freq_mhz"))
                .zip(read_number::<u32>(&card.join("gt_max_freq_mhz"))),
            driver,
        },
        // nouveau and friends expose nothing useful; nvidia is read through
        // nvidia-smi, and only when asked for.
        _ => return None,
    };
    Some(snapshot)
}

/// `nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total
/// --format=csv,noheader,nounits`: one line per GPU.
pub(crate) fn parse_nvidia_smi(text: &str) -> Vec<GpuSnapshot> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            let number = |index: usize| {
                fields
                    .get(index)
                    .and_then(|field| field.parse::<u64>().ok())
            };
            GpuSnapshot {
                driver: "nvidia".to_string(),
                busy_percent: number(0).filter(|p| *p <= 100).map(|p| p as u8),
                vram_used_mib: number(1),
                vram_total_mib: number(2),
                clock_mhz: None,
            }
        })
        .collect()
}

pub(crate) fn nvidia_smi_gpus() -> Vec<GpuSnapshot> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .stderr(std::process::Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => {
            parse_nvidia_smi(&String::from_utf8_lossy(&output.stdout))
        }
        _ => Vec::new(),
    }
}

/// Everything the monitor shows beyond the basic snapshot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetricsSnapshot {
    pub pressure: PressureSnapshot,
    pub disks: Vec<DiskRate>,
    pub gpus: Vec<GpuSnapshot>,
}

/// Human-readable lines for the monitor; parts with no data are left out.
pub fn metric_lines(metrics: &MetricsSnapshot) -> Vec<(&'static str, String)> {
    let mut lines = Vec::new();
    let pressure = [
        ("CPU", metrics.pressure.cpu),
        ("Mem", metrics.pressure.memory),
        ("IO", metrics.pressure.io),
    ]
    .iter()
    .filter_map(|(name, pressure)| pressure.map(|p| format!("{name} {:.1}%", p.some_avg10)))
    .collect::<Vec<_>>();
    if !pressure.is_empty() {
        lines.push(("PSI", pressure.join(" · ")));
    }
    for disk in &metrics.disks {
        lines.push((
            "IO",
            format!(
                "{}  R {} · W {} · {:.0}% busy",
                disk.name,
                rate(disk.read_bytes_per_sec),
                rate(disk.write_bytes_per_sec),
                disk.busy_percent
            ),
        ));
    }
    for gpu in &metrics.gpus {
        let mut parts = Vec::new();
        if let Some(busy) = gpu.busy_percent {
            parts.push(format!("{busy}%"));
        }
        if let (Some(used), Some(total)) = (gpu.vram_used_mib, gpu.vram_total_mib) {
            parts.push(format!("VRAM {used} / {total} MiB"));
        }
        if let Some((current, max)) = gpu.clock_mhz {
            parts.push(format!("{current} / {max} MHz"));
        }
        if !parts.is_empty() {
            lines.push(("GPU", format!("{}  {}", gpu.driver, parts.join(" · "))));
        }
    }
    lines
}

fn rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1024.0 {
        format!("{bytes_per_sec:.0} B/s")
    } else if bytes_per_sec < 1024.0 * 1024.0 {
        format!("{:.0} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.1} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_reads_avg10_and_tolerates_a_missing_full_line() {
        let cpu = "some avg10=1.25 avg60=0.14 avg300=0.17 total=15279239317\n";
        assert_eq!(
            parse_pressure(cpu),
            Some(Pressure {
                some_avg10: 1.25,
                full_avg10: None
            })
        );
        let memory = "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n\
                      full avg10=3.50 avg60=0.00 avg300=0.00 total=0\n";
        assert_eq!(parse_pressure(memory).unwrap().full_avg10, Some(3.5));
        assert_eq!(parse_pressure(""), None);
        assert_eq!(parse_pressure("some avg60=1.0\n"), None);
        let missing = read_pressure(Path::new("/nonexistent/pressure"));
        assert_eq!(missing, PressureSnapshot::default());
    }

    const DISKSTATS: &str = "\
 259       0 nvme0n1 18371790 3032272 1775205314 2899748 41962343 25843990 4844387904 179790197 0 36560340 191871038
 259       1 nvme0n1p1 18371710 3032272 1775201834 2899734 41962343 25843990 4844387904 179790197 0 37625076 184407530
   8       0 sda 100 0 2000 10 50 0 1000 5 0 100 15
   8       1 sda1 100 0 2000 10 50 0 1000 5 0 100 15
   7       0 loop0 5 0 10 0 0 0 0 0 0 1 0
 253       0 dm-0 1 0 2 0 0 0 0 0 0 0 0
 179       0 mmcblk0 1 0 2 0 0 0 0 0 0 0 0
 179       1 mmcblk0p1 1 0 2 0 0 0 0 0 0 0 0
   8      16 sdb 1 0 2
";

    #[test]
    fn diskstats_lists_whole_disks_only_and_skips_short_lines() {
        let names: Vec<_> = parse_diskstats(DISKSTATS)
            .into_iter()
            .map(|disk| disk.name)
            .collect();
        assert_eq!(names, vec!["nvme0n1", "sda", "mmcblk0"]);
    }

    #[test]
    fn disk_rates_come_from_two_samples() {
        let before = vec![DiskCounters {
            name: "nvme0n1".to_string(),
            sectors_read: 1000,
            sectors_written: 0,
            io_ms: 0,
        }];
        let after = vec![
            DiskCounters {
                name: "nvme0n1".to_string(),
                sectors_read: 1000 + 4096,
                sectors_written: 2048,
                io_ms: 500,
            },
            DiskCounters {
                name: "new".to_string(),
                sectors_read: 1,
                sectors_written: 1,
                io_ms: 1,
            },
        ];
        let rates = disk_rates(&before, &after, Duration::from_secs(2));
        assert_eq!(rates.len(), 1, "a disk seen once has no rate yet");
        assert_eq!(rates[0].read_bytes_per_sec, 4096.0 * 512.0 / 2.0);
        assert_eq!(rates[0].write_bytes_per_sec, 2048.0 * 512.0 / 2.0);
        assert_eq!(rates[0].busy_percent, 25.0);
        // Counters that went backwards (a reset) give no rate, not a huge one.
        assert!(disk_rates(&after, &before, Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn gpus_are_read_from_sysfs_with_missing_counters() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let drm = root.path().join("drm");
        let driver_dir = |name: &str| {
            let path = root.path().join("drivers").join(name);
            std::fs::create_dir_all(&path).unwrap();
            path
        };
        // card0: amdgpu with every counter.
        let amd = drm.join("card0/device");
        std::fs::create_dir_all(&amd).unwrap();
        symlink(driver_dir("amdgpu"), amd.join("driver")).unwrap();
        std::fs::write(amd.join("gpu_busy_percent"), "37\n").unwrap();
        std::fs::write(amd.join("mem_info_vram_used"), (512u64 << 20).to_string()).unwrap();
        std::fs::write(amd.join("mem_info_vram_total"), (8192u64 << 20).to_string()).unwrap();
        // card1: i915 without a max clock file.
        let intel = drm.join("card1");
        std::fs::create_dir_all(intel.join("device")).unwrap();
        symlink(driver_dir("i915"), intel.join("device/driver")).unwrap();
        std::fs::write(intel.join("gt_cur_freq_mhz"), "300").unwrap();
        // A connector and an unknown driver are skipped.
        std::fs::create_dir_all(drm.join("card0-DP-1")).unwrap();
        let other = drm.join("card2/device");
        std::fs::create_dir_all(&other).unwrap();
        symlink(driver_dir("nouveau"), other.join("driver")).unwrap();

        let gpus = read_gpus(&drm);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].busy_percent, Some(37));
        assert_eq!(
            (gpus[0].vram_used_mib, gpus[0].vram_total_mib),
            (Some(512), Some(8192))
        );
        assert_eq!(gpus[1].driver, "i915");
        assert_eq!(gpus[1].clock_mhz, None, "no max clock, no clock pair");

        let lines = metric_lines(&MetricsSnapshot {
            gpus,
            ..Default::default()
        });
        assert_eq!(
            lines,
            vec![("GPU", "amdgpu  37% · VRAM 512 / 8192 MiB".to_string())]
        );
    }

    #[test]
    fn nvidia_smi_lines_parse_and_odd_fields_are_dropped() {
        let gpus = parse_nvidia_smi("45, 2048, 8192\n[N/A], 10, 4096\n\n");
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].busy_percent, Some(45));
        assert_eq!(gpus[1].busy_percent, None);
        assert_eq!(gpus[1].vram_total_mib, Some(4096));
    }

    #[test]
    fn an_empty_snapshot_shows_no_lines() {
        assert!(metric_lines(&MetricsSnapshot::default()).is_empty());
    }
}
