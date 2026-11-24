//! Carico del computer per la Plancia (ADR-0126): CPU, memoria e disco principale in percentuale.
//! Letture leggere e in sola lettura; il risultato resta in cache per 3 secondi.

use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const CACHE: Duration = Duration::from_secs(3);

fn clamp(p: f64) -> f64 {
    (p.clamp(0.0, 100.0) * 10.0).round() / 10.0
}

/// «MemTotal» e «MemAvailable» di /proc/meminfo: percentuale usata.
pub fn parse_meminfo(text: &str) -> Option<f64> {
    let field = |name: &str| -> Option<f64> {
        text.lines()
            .find(|l| l.starts_with(name))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    (total > 0.0).then(|| clamp((total - available) / total * 100.0))
}

/// Tempo totale e tempo inattivo dalla prima riga di /proc/stat.
fn proc_stat_times(text: &str) -> Option<(f64, f64)> {
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let values: Vec<f64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    if values.len() < 5 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0.0);
    Some((values.iter().take(8).sum(), idle))
}

/// Percentuale di CPU usata fra due letture di /proc/stat.
pub fn parse_proc_stat(before: &str, after: &str) -> Option<f64> {
    let (t0, i0) = proc_stat_times(before)?;
    let (t1, i1) = proc_stat_times(after)?;
    let total = t1 - t0;
    (total > 0.0).then(|| clamp((1.0 - (i1 - i0) / total) * 100.0))
}

/// Memoria usata da `vm_stat` di macOS (pagine attive, cablate e compresse) rispetto a `hw.memsize`.
pub fn parse_vm_stat(text: &str, memsize: f64) -> Option<f64> {
    let page_size: f64 = text
        .lines()
        .next()?
        .split("page size of")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |name: &str| -> f64 {
        text.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|v| v.trim().trim_end_matches('.').parse().ok())
            .unwrap_or(0.0)
    };
    let used =
        (pages("Pages active") + pages("Pages wired down") + pages("Pages occupied by compressor"))
            * page_size;
    (memsize > 0.0).then(|| clamp(used / memsize * 100.0))
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    crate::hide_console(&mut cmd);
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn cpu_percent() -> Option<f64> {
    match std::env::consts::OS {
        "linux" => {
            let a = std::fs::read_to_string("/proc/stat").ok()?;
            std::thread::sleep(Duration::from_millis(200));
            parse_proc_stat(&a, &std::fs::read_to_string("/proc/stat").ok()?)
        }
        "macos" => {
            let out = run("/bin/ps", &["-A", "-o", "%cpu="])?;
            let total: f64 = out.lines().filter_map(|l| l.trim().replace(',', ".").parse::<f64>().ok()).sum();
            let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
            Some(clamp(total / cores))
        }
        "windows" => run(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", "(Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average"],
        )?
        .trim()
        .replace(',', ".")
        .parse()
        .ok()
        .map(clamp),
        _ => None,
    }
}

fn memory_percent() -> Option<f64> {
    match std::env::consts::OS {
        "linux" => parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?),
        "macos" => {
            let size: f64 = run("/usr/sbin/sysctl", &["-n", "hw.memsize"])?
                .trim()
                .parse()
                .ok()?;
            parse_vm_stat(&run("/usr/bin/vm_stat", &[])?, size)
        }
        "windows" => {
            let out = run(
                "powershell.exe",
                &["-NoProfile", "-NonInteractive", "-Command", "$o = Get-CimInstance Win32_OperatingSystem; \"$($o.TotalVisibleMemorySize) $($o.FreePhysicalMemory)\""],
            )?;
            let mut parts = out.split_whitespace().filter_map(|v| v.parse::<f64>().ok());
            let (total, free) = (parts.next()?, parts.next()?);
            (total > 0.0).then(|| clamp((total - free) / total * 100.0))
        }
        _ => None,
    }
}

fn disk_percent() -> Option<f64> {
    let disks = crate::toolbox::disks().ok()?;
    let list = disks["disks"].as_array()?;
    let pick = list
        .iter()
        .find(|d| matches!(d["mount"].as_str(), Some("/") | Some("C:")))
        .or_else(|| list.first())?;
    pick["used_percent"].as_f64().map(clamp)
}

/// Statistiche correnti; i valori non leggibili su questo sistema sono `null`.
pub fn stats() -> Value {
    static LAST: Mutex<Option<(Instant, Value)>> = Mutex::new(None);
    let mut guard = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, value)) = guard.as_ref() {
        if at.elapsed() < CACHE {
            return value.clone();
        }
    }
    let value = json!({
        "cpu_percent": cpu_percent(), "memory_percent": memory_percent(), "disk_percent": disk_percent(),
        "cores": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        "os": std::env::consts::OS,
    });
    *guard = Some((Instant::now(), value.clone()));
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_memory_and_cpu_are_parsed() {
        let mem = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    4000000 kB\n";
        assert_eq!(parse_meminfo(mem), Some(75.0));
        assert_eq!(parse_meminfo("niente"), None);
        let a = "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 1 1 1 1\n";
        let b = "cpu  150 0 150 900 0 0 0 0 0 0\n";
        assert_eq!(parse_proc_stat(a, b), Some(50.0));
        assert_eq!(parse_proc_stat(a, a), None);
    }

    #[test]
    fn macos_memory_is_computed_from_vm_stat() {
        let text = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:                               10000.\nPages active:                            200000.\nPages wired down:                         50000.\nPages occupied by compressor:             10000.\n";
        let used = (200000.0 + 50000.0 + 10000.0) * 16384.0;
        let total = 16.0 * 1024.0 * 1024.0 * 1024.0;
        assert_eq!(
            parse_vm_stat(text, total),
            Some(clamp(used / total * 100.0))
        );
        assert_eq!(parse_vm_stat("boh", total), None);
    }

    #[test]
    fn stats_have_the_expected_shape_and_are_cached() {
        let s = stats();
        for key in [
            "cpu_percent",
            "memory_percent",
            "disk_percent",
            "cores",
            "os",
        ] {
            assert!(s.get(key).is_some(), "{key}");
        }
        for key in ["cpu_percent", "memory_percent", "disk_percent"] {
            if let Some(v) = s[key].as_f64() {
                assert!((0.0..=100.0).contains(&v), "{key} {v}");
            }
        }
        assert_eq!(stats(), s, "seconda lettura dalla cache");
    }
}
