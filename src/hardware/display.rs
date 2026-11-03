use std::fs;
use std::path::Path;
use super::resolve_sys_path;

#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub connector: String,
    pub status: String,
    pub resolution: String,
    pub manufacturer: String,
    pub panel_model: String,
    pub replacement_specs: String,
    pub recommended_search: String,
}

impl DisplayInfo {
    pub fn collect_all(device_model: &str) -> Vec<Self> {
        let mut displays = Vec::new();
        let drm_dir = resolve_sys_path("/sys/class/drm");
        if !drm_dir.exists() {
            return displays;
        }

        if let Ok(entries) = fs::read_dir(drm_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let status_path = path.join("status");
                if let Ok(status_str) = fs::read_to_string(&status_path) {
                    if status_str.trim().eq_ignore_ascii_case("connected") {
                        let connector = entry.file_name().to_string_lossy().to_string();
                        let resolution = read_resolution(&path).unwrap_or_else(|| "1920x1080".into());

                        let (manufacturer, panel_model) = parse_edid(&path.join("edid"));

                        let replacement_specs = format!("eDP 30-pin interface, {} IPS panel", resolution);

                        let recommended_search = if !panel_model.is_empty() && panel_model != "Generic Panel" && panel_model != "Standard Panel" && panel_model != "OEM Compatible Panel" {
                            format!("{} replacement screen 30-pin", panel_model)
                        } else {
                            format!("{} LCD screen replacement", device_model)
                        };

                        displays.push(Self {
                            connector,
                            status: "Connected (Active)".into(),
                            resolution,
                            manufacturer,
                            panel_model,
                            replacement_specs,
                            recommended_search,
                        });
                    }
                }
            }
        }

        displays
    }
}

fn read_resolution(path: &Path) -> Option<String> {
    let modes_path = path.join("modes");
    if let Ok(content) = fs::read_to_string(modes_path) {
        if let Some(first) = content.lines().next() {
            let trimmed = first.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn parse_edid(edid_path: &Path) -> (String, String) {
    if !edid_path.exists() {
        return ("Generic".into(), "Standard Panel".into());
    }

    if let Ok(bytes) = fs::read(edid_path) {
        return parse_edid_bytes(&bytes);
    }

    ("Generic".into(), "Standard Panel".into())
}

pub fn parse_edid_bytes(bytes: &[u8]) -> (String, String) {
    if bytes.len() >= 128 {
        // Manufacturer ID at bytes 8-9 (3 5-bit characters)
        let mfg_raw = ((bytes[8] as u16) << 8) | (bytes[9] as u16);
        let c1 = (((mfg_raw >> 10) & 0x1f) as u8) + b'A' - 1;
        let c2 = (((mfg_raw >> 5) & 0x1f) as u8) + b'A' - 1;
        let c3 = ((mfg_raw & 0x1f) as u8) + b'A' - 1;

        let mfg_code = format!("{}{}{}", c1 as char, c2 as char, c3 as char);
        let manufacturer_name = match mfg_code.as_str() {
            "LGD" => "LG Display (LGD)".into(),
            "BOE" => "BOE Technology".into(),
            "AUO" => "AU Optronics (AUO)".into(),
            "INX" | "CMO" => "Innolux / Chi Mei".into(),
            "SEC" => "Samsung Display".into(),
            "SHP" => "Sharp".into(),
            "IVO" => "InfoVision (IVO)".into(),
            _ => mfg_code,
        };

        // Search descriptors at offsets 54, 72, 90, 108
        let mut panel_model = String::new();
        for offset in [54, 72, 90, 108] {
            if offset + 18 <= bytes.len() {
                let desc = &bytes[offset..offset + 18];
                // Descriptor header: 00 00 00 [type] [flags]
                if desc[0] == 0 && desc[1] == 0 && desc[2] == 0 {
                    let desc_type = desc[3];
                    if desc_type == 0xfc || desc_type == 0xfe {
                        let text: String = desc[5..]
                            .iter()
                            .take_while(|&&b| b != 0x0a && b != 0x00)
                            .map(|&b| b as char)
                            .collect();
                        let clean = text.trim();
                        if !clean.is_empty()
                            && (desc_type == 0xfc || clean.contains('-') || clean.chars().any(|c| c.is_ascii_digit()))
                        {
                            panel_model = clean.to_string();
                        }
                    }
                }
            }
        }

        if panel_model.is_empty() {
            panel_model = "OEM Compatible Panel".into();
        }

        return (manufacturer_name, panel_model);
    }

    ("Generic".into(), "Standard Panel".into())
}
