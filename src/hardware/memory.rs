use std::fs;
use super::resolve_sys_path;

#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total_gib: f32,
    pub available_gib: f32,
    pub used_gib: f32,
    pub form_factor: String,
    pub speed_recommendation: String,
    pub max_capacity_estimate: String,
    pub pin_count: String,
    pub recommended_search: String,
}

impl MemoryInfo {
    pub fn collect(device_model: &str, cpu_model: &str) -> Self {
        let (total_kb, avail_kb) = parse_meminfo();
        let total_gib = (total_kb as f32) / (1024.0 * 1024.0);
        let available_gib = (avail_kb as f32) / (1024.0 * 1024.0);
        let used_gib = (total_gib - available_gib).max(0.0);

        let (form_factor, speed_recommendation, pin_count, max_capacity_estimate) = deduce_memory_specs(cpu_model);

        let recommended_search = format!("{} RAM upgrade {} SODIMM", device_model, form_factor);

        Self {
            total_gib,
            available_gib,
            used_gib,
            form_factor,
            speed_recommendation,
            max_capacity_estimate,
            pin_count,
            recommended_search,
        }
    }
}

pub fn parse_meminfo() -> (u64, u64) {
    let mut total_kb = 0;
    let mut avail_kb = 0;

    let path = resolve_sys_path("/proc/meminfo");
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = parse_kb_line(line);
            } else if line.starts_with("MemAvailable:") {
                avail_kb = parse_kb_line(line);
            }
        }
    }
    (total_kb, avail_kb)
}

fn parse_kb_line(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|val| val.parse().ok())
        .unwrap_or(0)
}

pub fn deduce_memory_specs(cpu_model: &str) -> (String, String, String, String) {
    let lower = cpu_model.to_lowercase();
    if lower.contains("12th")
        || lower.contains("13th")
        || lower.contains("14th")
        || lower.contains("core ultra")
        || lower.contains("ryzen 7000")
        || lower.contains("ryzen 8000")
    {
        (
            "DDR5".into(),
            "4800 MHz / 5600 MHz".into(),
            "262-pin SO-DIMM (1.1V)".into(),
            "Up to 64 GB (2x 32 GB)".into(),
        )
    } else if lower.contains("i3-") || lower.contains("i5-") || lower.contains("i7-") || lower.contains("ryzen") {
        (
            "DDR4".into(),
            "2133 MHz / 2400 MHz / 2666 MHz / 3200 MHz".into(),
            "260-pin SO-DIMM (1.2V Non-ECC)".into(),
            "Up to 32 GB (2x 16 GB)".into(),
        )
    } else {
        (
            "DDR4 / LPDDR4".into(),
            "Standard SODIMM".into(),
            "260-pin SO-DIMM".into(),
            "Up to 32 GB".into(),
        )
    }
}
