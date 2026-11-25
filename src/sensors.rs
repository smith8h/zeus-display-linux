//! CPU readings straight from the kernel: hwmon for temperature, /proc/stat for load,
//! powercap (RAPL) for package power and cpufreq for clock speed.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Copy, Default)]
pub struct Reading {
    pub temp_c: Option<f64>,
    pub load: f64,
    pub power_w: Option<f64>,
    pub clock_ghz: Option<f64>,
}

pub struct Sensors {
    temp: Option<PathBuf>,
    energy: Option<PathBuf>,
    energy_range: u64,
    last_cpu: Option<(u64, u64)>,
    last_energy: Option<(u64, Instant)>,
    freq_files: Vec<PathBuf>,
}

impl Sensors {
    pub fn new() -> Self {
        let energy = Path::new("/sys/class/powercap/intel-rapl:0/energy_uj").to_path_buf();
        let energy_ok = fs::read_to_string(&energy).is_ok();
        let energy_range = read_u64(Path::new("/sys/class/powercap/intel-rapl:0/max_energy_range_uj"))
            .unwrap_or(u64::MAX);

        let mut freq_files: Vec<PathBuf> = fs::read_dir("/sys/devices/system/cpu")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                let n = e.file_name();
                let n = n.to_string_lossy();
                n.starts_with("cpu") && n[3..].chars().all(|c| c.is_ascii_digit()) && n.len() > 3
            })
            .map(|e| e.path().join("cpufreq/scaling_cur_freq"))
            .filter(|p| p.exists())
            .collect();
        freq_files.sort();

        let mut sensors = Self {
            temp: find_temp_sensor(),
            energy: energy_ok.then_some(energy),
            energy_range,
            last_cpu: None,
            last_energy: None,
            freq_files,
        };
        sensors.read(); // prime the deltas
        sensors
    }

    pub fn read(&mut self) -> Reading {
        Reading {
            temp_c: self.temp.as_deref().and_then(read_u64).map(|m| m as f64 / 1000.0),
            load: self.read_load(),
            power_w: self.read_power(),
            clock_ghz: self.read_clock(),
        }
    }

    fn read_load(&mut self) -> f64 {
        let Some((busy, total)) = read_proc_stat() else { return 0.0 };
        let load = match self.last_cpu {
            Some((b0, t0)) if total > t0 => (busy.saturating_sub(b0)) as f64 / (total - t0) as f64 * 100.0,
            _ => 0.0,
        };
        self.last_cpu = Some((busy, total));
        load.clamp(0.0, 100.0)
    }

    fn read_power(&mut self) -> Option<f64> {
        let now = Instant::now();
        let energy = read_u64(self.energy.as_deref()?)?;
        let previous = self.last_energy.replace((energy, now));
        let (e0, t0) = previous?;
        let dt = now.duration_since(t0).as_secs_f64();
        if dt < 0.05 {
            return None;
        }
        let delta = if energy >= e0 { energy - e0 } else { self.energy_range - e0 + energy };
        Some(delta as f64 / 1_000_000.0 / dt)
    }

    fn read_clock(&self) -> Option<f64> {
        let khz: Vec<u64> = self.freq_files.iter().filter_map(|p| read_u64(p)).collect();
        if khz.is_empty() {
            return None;
        }
        Some(khz.iter().sum::<u64>() as f64 / khz.len() as f64 / 1_000_000.0)
    }
}

/// CPU model for the window subtitle, e.g. "AMD Ryzen 5 7600X".
pub fn cpu_model() -> String {
    let info = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let name = info
        .lines()
        .find_map(|l| l.strip_prefix("model name").and_then(|r| r.split_once(':')).map(|(_, v)| v.trim()))
        .unwrap_or("Processor");
    let name = name.split(" with ").next().unwrap_or(name);
    name.replace("(R)", "")
        .replace("(TM)", "")
        .trim_end_matches(" Processor")
        .split_whitespace()
        .filter(|w| !w.ends_with("-Core"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_proc_stat() -> Option<(u64, u64)> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().next()?.strip_prefix("cpu ")?;
    let v: Vec<u64> = line.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest time is already in user/nice)
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

/// Picks the best CPU temperature input, matching what Windows tools call "Core (Tctl/Tdie)"
/// on AMD and "CPU Package" on Intel.
fn find_temp_sensor() -> Option<PathBuf> {
    // (driver name, preferred label) in priority order; None = first temp input.
    const PREFERRED: &[(&str, Option<&str>)] = &[
        ("k10temp", Some("Tctl")),
        ("k10temp", Some("Tdie")),
        ("zenpower", Some("Tdie")),
        ("zenpower", Some("Tctl")),
        ("coretemp", Some("Package id 0")),
        ("k10temp", None),
        ("coretemp", None),
        ("cpu_thermal", None),
        ("acpitz", None),
    ];

    let hwmons: Vec<(String, PathBuf)> = fs::read_dir("/sys/class/hwmon")
        .ok()?
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let name = fs::read_to_string(dir.join("name")).ok()?.trim().to_string();
            Some((name, dir))
        })
        .collect();

    for (driver, label) in PREFERRED {
        for (_, dir) in hwmons.iter().filter(|(n, _)| n == driver) {
            let mut inputs: Vec<PathBuf> = fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let f = p.file_name().unwrap_or_default().to_string_lossy();
                    f.starts_with("temp") && f.ends_with("_input")
                })
                .collect();
            inputs.sort();
            let hit = match label {
                Some(label) => inputs.into_iter().find(|p| {
                    let label_file = p.to_string_lossy().replace("_input", "_label");
                    fs::read_to_string(label_file).map(|l| l.trim() == *label).unwrap_or(false)
                }),
                None => inputs.into_iter().next(),
            };
            if let Some(p) = hit.filter(|p| read_u64(p).is_some()) {
                return Some(p);
            }
        }
    }
    None
}
