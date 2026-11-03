use std::fs;
use std::path::Path;
use super::resolve_sys_path;

#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub name: String,
    pub model: String,
    pub capacity_gb: f32,
    pub interface_type: String,
    pub form_factor: String,
    pub upgrade_recommendation: String,
    pub recommended_search: String,
}

impl DiskInfo {
    pub fn collect_all(device_model: &str) -> Vec<Self> {
        let mut disks = Vec::new();
        let block_dir = resolve_sys_path("/sys/block");
        if !block_dir.exists() {
            return disks;
        }

        if let Ok(entries) = fs::read_dir(block_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("loop")
                    || name.starts_with("zram")
                    || name.starts_with("dm-")
                    || name.starts_with("sr")
                {
                    continue;
                }

                let path = entry.path();
                let model = read_device_model(&path).unwrap_or_else(|| "Internal Storage".into());
                let capacity_gb = read_capacity_gb(&path);
                if capacity_gb < 1.0 {
                    continue;
                }

                let is_nvme = name.starts_with("nvme");
                let interface_type = if is_nvme {
                    "M.2 NVMe PCIe".into()
                } else if name.starts_with("sd") {
                    "SATA III".into()
                } else if name.starts_with("mmc") {
                    "eMMC Embedded".into()
                } else {
                    "Solid State Drive".into()
                };

                let form_factor = if is_nvme {
                    "M.2 2280 (Single/Double Sided)".into()
                } else if name.starts_with("sd") {
                    "2.5-inch 7mm SATA".into()
                } else {
                    "Internal Drive".into()
                };

                let upgrade_recommendation = if is_nvme {
                    "M.2 2280 PCIe 3.0 or 4.0 x4 NVMe SSD (e.g., Samsung 980, WD Black SN770, Crucial P3/T500)".into()
                } else {
                    "Standard 2.5\" SATA III SSD (e.g., Crucial MX500, Samsung 870 EVO)".into()
                };

                let recommended_search = if is_nvme {
                    format!("{} SSD upgrade M.2 2280 NVMe", device_model)
                } else {
                    format!("{} 2.5 SATA SSD replacement", device_model)
                };

                disks.push(Self {
                    name,
                    model,
                    capacity_gb,
                    interface_type,
                    form_factor,
                    upgrade_recommendation,
                    recommended_search,
                });
            }
        }

        disks
    }
}

fn read_device_model(path: &Path) -> Option<String> {
    let candidates = [
        path.join("device/model"),
        path.join("device/device/model"),
        path.join("device/name"),
    ];

    for c in candidates {
        if c.exists() {
            if let Ok(content) = fs::read_to_string(c) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

fn read_capacity_gb(path: &Path) -> f32 {
    let size_path = path.join("size");
    if let Ok(content) = fs::read_to_string(size_path) {
        if let Ok(sectors) = content.trim().parse::<u64>() {
            // sectors are 512 bytes each
            let bytes = sectors * 512;
            return (bytes as f32) / 1_000_000_000.0;
        }
    }
    0.0
}
