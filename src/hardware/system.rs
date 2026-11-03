use std::fs;
use super::resolve_sys_path;

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub vendor: String,
    pub product_name: String,
    pub product_version: String,
    pub bios_version: String,
    pub bios_date: String,
    pub board_name: String,
    pub chassis_type: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub cpu_threads: usize,
}

impl SystemInfo {
    pub fn collect() -> Self {
        let vendor = read_sysfs_string("/sys/class/dmi/id/sys_vendor").unwrap_or_else(|| "Unknown Vendor".into());
        let product_name = read_sysfs_string("/sys/class/dmi/id/product_name").unwrap_or_else(|| "Unknown Model".into());
        let product_version = read_sysfs_string("/sys/class/dmi/id/product_version").unwrap_or_else(|| "Standard".into());
        let bios_version = read_sysfs_string("/sys/class/dmi/id/bios_version").unwrap_or_else(|| "Unknown BIOS".into());
        let bios_date = read_sysfs_string("/sys/class/dmi/id/bios_date").unwrap_or_else(|| "Unknown Date".into());
        let board_name = read_sysfs_string("/sys/class/dmi/id/board_name").unwrap_or_else(|| "Unknown Board".into());
        let raw_chassis = read_sysfs_string("/sys/class/dmi/id/chassis_type").unwrap_or_else(|| "3".into());
        let chassis_type = decode_chassis_type(&raw_chassis);

        let (cpu_model, cpu_cores, cpu_threads) = parse_cpuinfo();

        Self {
            vendor,
            product_name,
            product_version,
            bios_version,
            bios_date,
            board_name,
            chassis_type,
            cpu_model,
            cpu_cores,
            cpu_threads,
        }
    }

    pub fn full_device_name(&self) -> String {
        if self.product_version.to_lowercase() != "none"
            && !self.product_version.is_empty()
            && self.product_version.to_lowercase() != "standard"
        {
            format!("{} {}", self.vendor, self.product_version)
        } else {
            format!("{} {}", self.vendor, self.product_name)
        }
    }

    pub fn replacement_search_query(&self) -> String {
        format!("{} {} parts replacement", self.vendor, self.product_version)
    }
}

fn read_sysfs_string(path_str: &str) -> Option<String> {
    let path = resolve_sys_path(path_str);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

pub fn decode_chassis_type(code_str: &str) -> String {
    let code: u32 = code_str.parse().unwrap_or(0);
    match code {
        3 => "Desktop".into(),
        8 => "Portable".into(),
        9 => "Laptop".into(),
        10 => "Notebook".into(),
        11 => "Hand Held".into(),
        12 => "Docking Station".into(),
        13 => "All in One".into(),
        14 => "Sub Notebook".into(),
        30 => "Tablet".into(),
        31 => "Convertible 2-in-1".into(),
        32 => "Detachable".into(),
        _ => "Personal Computer".into(),
    }
}

pub fn parse_cpuinfo() -> (String, usize, usize) {
    let mut model = "Unknown CPU".to_string();
    let mut threads = 0;
    let mut core_ids = std::collections::HashSet::new();

    let path = resolve_sys_path("/proc/cpuinfo");
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if line.starts_with("model name") {
                if let Some(pos) = line.find(':') {
                    if model == "Unknown CPU" {
                        model = line[pos + 1..].trim().to_string();
                    }
                }
            } else if line.starts_with("processor") {
                threads += 1;
            } else if line.starts_with("core id") {
                if let Some(pos) = line.find(':') {
                    let id_str = line[pos + 1..].trim();
                    core_ids.insert(id_str.to_string());
                }
            }
        }
    }

    let cores = if !core_ids.is_empty() {
        core_ids.len()
    } else if threads > 0 {
        threads / 2
    } else {
        1
    };

    (
        model,
        if cores == 0 { 1 } else { cores },
        if threads == 0 { 1 } else { threads },
    )
}
