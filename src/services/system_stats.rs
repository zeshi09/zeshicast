use std::fs;
use std::io;
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct SystemSnapshot {
    pub load_average: Option<f32>,
    pub cpu_count: Option<usize>,
    pub memory_total_kib: Option<u64>,
    pub memory_available_kib: Option<u64>,
    pub disk_total_kib: Option<u64>,
    pub disk_used_kib: Option<u64>,
    pub uptime_seconds: Option<u64>,
    pub process_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSummary {
    pub pid: u32,
    pub name: String,
    pub memory_kib: Option<u64>,
}

impl SystemSnapshot {
    pub fn memory_used_kib(&self) -> Option<u64> {
        Some(self.memory_total_kib? - self.memory_available_kib?)
    }

    pub fn memory_used_percent(&self) -> Option<f32> {
        let total = self.memory_total_kib?;
        if total == 0 {
            return None;
        }
        Some(self.memory_used_kib()? as f32 * 100.0 / total as f32)
    }

    pub fn disk_used_percent(&self) -> Option<f32> {
        let total = self.disk_total_kib?;
        if total == 0 {
            return None;
        }
        Some(self.disk_used_kib? as f32 * 100.0 / total as f32)
    }
}

pub fn system_snapshot() -> SystemSnapshot {
    let disk = read_root_disk_usage().ok();
    SystemSnapshot {
        load_average: read_load_average().ok(),
        cpu_count: read_cpu_count().ok(),
        memory_total_kib: read_meminfo_value("MemTotal").ok(),
        memory_available_kib: read_meminfo_value("MemAvailable").ok(),
        disk_total_kib: disk.map(|usage| usage.0),
        disk_used_kib: disk.map(|usage| usage.1),
        uptime_seconds: read_uptime_seconds().ok(),
        process_count: read_process_count().ok(),
    }
}

fn read_cpu_count() -> io::Result<usize> {
    let contents = fs::read_to_string("/proc/cpuinfo")?;
    let count = contents
        .lines()
        .filter(|line| line.starts_with("processor"))
        .count();
    if count == 0 {
        Err(io::Error::new(io::ErrorKind::InvalidData, "no processors found"))
    } else {
        Ok(count)
    }
}

pub fn top_processes_by_memory(limit: usize) -> Vec<ProcessSummary> {
    let Ok(mut processes) = read_process_summaries() else {
        return Vec::new();
    };

    processes.sort_by(|left, right| {
        right
            .memory_kib
            .unwrap_or_default()
            .cmp(&left.memory_kib.unwrap_or_default())
            .then_with(|| left.name.cmp(&right.name))
    });
    processes.truncate(limit);
    processes
}

fn read_load_average() -> io::Result<f32> {
    let contents = fs::read_to_string("/proc/loadavg")?;
    contents
        .split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid /proc/loadavg"))
}

fn read_meminfo_value(key: &str) -> io::Result<u64> {
    let contents = fs::read_to_string("/proc/meminfo")?;
    parse_meminfo_value(&contents, key)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing meminfo key"))
}

fn parse_meminfo_value(contents: &str, key: &str) -> Option<u64> {
    contents.lines().find_map(|line| {
        let (name, rest) = line.split_once(':')?;
        (name == key).then(|| rest.split_whitespace().next()?.parse().ok())?
    })
}

fn read_uptime_seconds() -> io::Result<u64> {
    let contents = fs::read_to_string("/proc/uptime")?;
    contents
        .split_whitespace()
        .next()
        .and_then(|value| value.split('.').next())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid /proc/uptime"))
}

fn read_process_count() -> io::Result<usize> {
    Ok(fs::read_dir("/proc")?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.bytes().all(|byte| byte.is_ascii_digit()))
        })
        .count())
}

fn read_root_disk_usage() -> io::Result<(u64, u64)> {
    let output = Command::new("df").args(["-kP", "/"]).output()?;
    if !output.status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "df failed"));
    }
    parse_df_root_usage(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid df output"))
}

fn read_process_summaries() -> io::Result<Vec<ProcessSummary>> {
    let mut processes = Vec::new();
    for entry in fs::read_dir("/proc")?.filter_map(Result::ok) {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        if let Some(process) = parse_process_status(pid, &status) {
            processes.push(process);
        }
    }
    Ok(processes)
}

fn parse_process_status(pid: u32, contents: &str) -> Option<ProcessSummary> {
    let mut name = None;
    let mut memory_kib = None;

    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("Name:") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("VmRSS:") {
            memory_kib = rest.split_whitespace().next()?.parse().ok();
        }
    }

    Some(ProcessSummary {
        pid,
        name: name?,
        memory_kib,
    })
}

fn parse_df_root_usage(output: &str) -> Option<(u64, u64)> {
    let line = output.lines().nth(1)?;
    let mut fields = line.split_whitespace();
    fields.next()?;
    let total = fields.next()?.parse().ok()?;
    let used = fields.next()?.parse().ok()?;
    Some((total, used))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_parser_extracts_values() {
        let contents = "\
MemTotal:       16384000 kB
MemFree:         1000000 kB
MemAvailable:   8000000 kB
";

        assert_eq!(parse_meminfo_value(contents, "MemTotal"), Some(16_384_000));
        assert_eq!(
            parse_meminfo_value(contents, "MemAvailable"),
            Some(8_000_000)
        );
        assert_eq!(parse_meminfo_value(contents, "SwapTotal"), None);
    }

    #[test]
    fn memory_percent_uses_available_memory() {
        let snapshot = SystemSnapshot {
            memory_total_kib: Some(100),
            memory_available_kib: Some(25),
            ..SystemSnapshot::default()
        };

        assert_eq!(snapshot.memory_used_kib(), Some(75));
        assert_eq!(snapshot.memory_used_percent(), Some(75.0));
    }

    #[test]
    fn df_parser_extracts_root_usage() {
        let output = "\
Filesystem     1024-blocks    Used Available Capacity Mounted on
/dev/root         1000000   250000    750000      25% /
";

        assert_eq!(parse_df_root_usage(output), Some((1_000_000, 250_000)));
    }

    #[test]
    fn process_status_parser_extracts_name_and_memory() {
        let status = "\
Name:	firefox
State:	S (sleeping)
VmRSS:	  204800 kB
";

        assert_eq!(
            parse_process_status(42, status),
            Some(ProcessSummary {
                pid: 42,
                name: "firefox".to_string(),
                memory_kib: Some(204_800),
            })
        );
    }
}
