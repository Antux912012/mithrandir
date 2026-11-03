use std::fs;
use std::path::Path;
use super::resolve_sys_path;

#[derive(Debug, Clone)]
pub struct NetworkCardInfo {
    pub interface_name: String,
    pub device_type: String,
    pub chipset_name: String,
    pub form_factor: String,
    pub upgrade_option: String,
    pub recommended_search: String,
}

impl NetworkCardInfo {
    pub fn collect_all(device_model: &str) -> Vec<Self> {
        let mut adapters = Vec::new();

        let net_dir = resolve_sys_path("/sys/class/net");
        if net_dir.exists() {
            if let Ok(entries) = fs::read_dir(net_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name == "lo"
                        || name.starts_with("virbr")
                        || name.starts_with("docker")
                        || name.starts_with("podman")
                    {
                        continue;
                    }

                    let path = entry.path();
                    let is_wireless = path.join("wireless").exists() || path.join("phy80211").exists();
                    let device_type = if is_wireless {
                        "Wi-Fi Wireless Adapter".into()
                    } else {
                        "Ethernet Controller".into()
                    };

                    let chipset_name = detect_pci_device_name(&path).unwrap_or_else(|| {
                        if is_wireless {
                            "Intel / Broadcom Wi-Fi Module".into()
                        } else {
                            "Gigabit Ethernet".into()
                        }
                    });

                    let form_factor = if is_wireless {
                        "M.2 2230 Key E (Replaceable)".into()
                    } else {
                        "Integrated RJ-45 Controller".into()
                    };

                    let upgrade_option = if is_wireless {
                        "Upgrade to Intel AX210 Wi-Fi 6E / BT 5.3 M.2 2230 module".into()
                    } else {
                        "Integrated on Motherboard".into()
                    };

                    let recommended_search = if is_wireless {
                        format!("{} wifi card upgrade M.2 2230", device_model)
                    } else {
                        format!("{} ethernet replacement", device_model)
                    };

                    adapters.push(Self {
                        interface_name: name,
                        device_type,
                        chipset_name,
                        form_factor,
                        upgrade_option,
                        recommended_search,
                    });
                }
            }
        }

        adapters
    }
}

pub fn detect_pci_device_name(net_path: &Path) -> Option<String> {
    let device_link = net_path.join("device");
    if device_link.exists() {
        let vendor = fs::read_to_string(device_link.join("vendor")).ok()?.trim().to_string();
        let device = fs::read_to_string(device_link.join("device")).ok()?.trim().to_string();

        return identify_pci_network_device(&vendor, &device);
    }
    None
}

pub fn identify_pci_network_device(vendor: &str, device: &str) -> Option<String> {
    if vendor == "0x8086" {
        // Intel
        match device {
            "0x24fd" => Some("Intel Dual Band Wireless-AC 8265".into()),
            "0x095a" => Some("Intel Dual Band Wireless-AC 7265".into()),
            "0x3165" => Some("Intel Dual Band Wireless-AC 3165".into()),
            "0x2723" => Some("Intel Wi-Fi 6 AX200".into()),
            "0x2725" => Some("Intel Wi-Fi 6E AX210".into()),
            _ => Some(format!("Intel Wireless Device ({})", device)),
        }
    } else if vendor == "0x10ec" {
        // Realtek
        Some(format!("Realtek Wireless Controller ({})", device))
    } else if vendor == "0x14e4" {
        // Broadcom
        Some(format!("Broadcom Wireless Controller ({})", device))
    } else if vendor == "0x17cb" {
        // Qualcomm Atheros
        Some(format!("Qualcomm Atheros Wireless ({})", device))
    } else {
        None
    }
}
