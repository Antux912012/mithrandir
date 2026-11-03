use std::collections::HashMap;
use std::fs;
use std::path::Path;
use super::resolve_sys_path;

#[derive(Debug, Clone)]
pub struct BatteryInfo {
    pub name: String,
    pub model_name: String,
    pub manufacturer: String,
    pub serial_number: String,
    pub technology: String,
    pub status: String,
    pub percentage: u32,
    pub health_percent: f32,
    pub energy_now_wh: f32,
    pub energy_full_wh: f32,
    pub energy_full_design_wh: f32,
    pub voltage_now_v: f32,
    pub voltage_design_v: f32,
    pub power_rate_w: f32,
    pub cycle_count: u32,
    pub fru_number: String,
    pub compatible_frus: Vec<String>,
    pub recommended_search: String,
}

impl BatteryInfo {
    pub fn collect_all(device_model: &str) -> Vec<Self> {
        let mut batteries = Vec::new();
        let power_supply_path = resolve_sys_path("/sys/class/power_supply");
        if !power_supply_path.exists() {
            return batteries;
        }

        if let Ok(entries) = fs::read_dir(power_supply_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let type_path = path.join("type");
                if let Ok(typ) = fs::read_to_string(&type_path) {
                    if typ.trim().eq_ignore_ascii_case("battery") {
                        if let Some(bat) = Self::from_sysfs(&path, device_model) {
                            batteries.push(bat);
                        }
                    }
                }
            }
        }

        batteries
    }

    fn from_sysfs(path: &Path, device_model: &str) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_string();
        let uevent_path = path.join("uevent");
        let mut map = HashMap::new();

        if let Ok(content) = fs::read_to_string(uevent_path) {
            for line in content.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }

        let model_name = map.get("POWER_SUPPLY_MODEL_NAME")
            .cloned()
            .unwrap_or_else(|| read_file_string(&path.join("model_name")).unwrap_or_else(|| "Internal Battery".into()));

        let manufacturer = map.get("POWER_SUPPLY_MANUFACTURER")
            .cloned()
            .unwrap_or_else(|| read_file_string(&path.join("manufacturer")).unwrap_or_else(|| "OEM".into()));

        let serial_number = map.get("POWER_SUPPLY_SERIAL_NUMBER")
            .cloned()
            .unwrap_or_else(|| read_file_string(&path.join("serial_number")).unwrap_or_else(|| "N/A".into()));

        let technology = map.get("POWER_SUPPLY_TECHNOLOGY")
            .cloned()
            .unwrap_or_else(|| read_file_string(&path.join("technology")).unwrap_or_else(|| "Li-poly".into()));

        let status = map.get("POWER_SUPPLY_STATUS")
            .cloned()
            .unwrap_or_else(|| read_file_string(&path.join("status")).unwrap_or_else(|| "Unknown".into()));

        let capacity: u32 = map.get("POWER_SUPPLY_CAPACITY")
            .and_then(|s| s.parse().ok())
            .or_else(|| read_file_u32(&path.join("capacity")))
            .unwrap_or(0);

        // Energies in uWh
        let energy_now_uwh = read_energy_uwh(&map, path, "ENERGY_NOW", "CHARGE_NOW");
        let energy_full_uwh = read_energy_uwh(&map, path, "ENERGY_FULL", "CHARGE_FULL");
        let energy_design_uwh = read_energy_uwh(&map, path, "ENERGY_FULL_DESIGN", "CHARGE_FULL_DESIGN");

        // Voltages in uV
        let voltage_now_uv: f32 = map.get("POWER_SUPPLY_VOLTAGE_NOW")
            .and_then(|s| s.parse().ok())
            .or_else(|| read_file_u64(&path.join("voltage_now")))
            .unwrap_or(0) as f32;

        let voltage_min_design_uv: f32 = map.get("POWER_SUPPLY_VOLTAGE_MIN_DESIGN")
            .and_then(|s| s.parse().ok())
            .or_else(|| read_file_u64(&path.join("voltage_min_design")))
            .unwrap_or(0) as f32;

        let power_now_uw: f32 = map.get("POWER_SUPPLY_POWER_NOW")
            .and_then(|s| s.parse().ok())
            .or_else(|| read_file_u64(&path.join("power_now")))
            .unwrap_or(0) as f32;

        let cycle_count: u32 = map.get("POWER_SUPPLY_CYCLE_COUNT")
            .and_then(|s| s.parse().ok())
            .or_else(|| read_file_u32(&path.join("cycle_count")))
            .unwrap_or(0);

        let energy_now_wh = (energy_now_uwh as f32) / 1_000_000.0;
        let energy_full_wh = (energy_full_uwh as f32) / 1_000_000.0;
        let energy_design_wh = (energy_design_uwh as f32) / 1_000_000.0;

        let health_percent = if energy_design_wh > 0.0 {
            ((energy_full_wh / energy_design_wh) * 100.0).clamp(0.0, 100.0)
        } else {
            100.0
        };

        let voltage_now_v = voltage_now_uv / 1_000_000.0;
        let voltage_design_v = if voltage_min_design_uv > 0.0 {
            voltage_min_design_uv / 1_000_000.0
        } else {
            voltage_now_v
        };
        let power_rate_w = power_now_uw / 1_000_000.0;

        // Extract FRU / Part Number
        let fru_number = extract_fru_part(&model_name);
        let compatible_frus = get_compatible_frus(&fru_number);

        let recommended_search = if !fru_number.is_empty() {
            format!("{} battery {}", device_model, fru_number)
        } else {
            format!("{} replacement battery", device_model)
        };

        Some(Self {
            name,
            model_name,
            manufacturer,
            serial_number,
            technology,
            status,
            percentage: capacity,
            health_percent,
            energy_now_wh,
            energy_full_wh,
            energy_full_design_wh: energy_design_wh,
            voltage_now_v,
            voltage_design_v,
            power_rate_w,
            cycle_count,
            fru_number,
            compatible_frus,
            recommended_search,
        })
    }
}

pub fn extract_fru_part(raw_model: &str) -> String {
    let clean = raw_model.trim();
    // Common Lenovo prefixes: LNV-01AV436 or 01AV436
    if let Some(pos) = clean.rfind('-') {
        clean[pos + 1..].to_string()
    } else {
        clean.to_string()
    }
}

pub fn get_compatible_frus(fru: &str) -> Vec<String> {
    match fru.to_uppercase().as_str() {
        "01AV436" => vec![
            "01AV435 (Celxpert)".into(),
            "01AV437 (LG)".into(),
            "01AV400 / SB10J78997".into(),
            "01AV401 / SB10J78998".into(),
            "01AV402 / SB10J78999".into(),
            "SB10K97593 (OEM ASM)".into(),
        ],
        "01AV435" => vec![
            "01AV436 (SMP)".into(),
            "01AV437 (LG)".into(),
            "SB10K97592 (OEM ASM)".into(),
        ],
        "01AV421" => vec![
            "01AV419".into(),
            "01AV420".into(),
            "SB10K97586".into(),
        ],
        _ => Vec::new(),
    }
}

fn read_energy_uwh(map: &HashMap<String, String>, path: &Path, energy_key: &str, charge_key: &str) -> u64 {
    if let Some(val) = map.get(&format!("POWER_SUPPLY_{}", energy_key)).and_then(|s| s.parse().ok()) {
        return val;
    }
    if let Some(val) = read_file_u64(&path.join(energy_key.to_lowercase())) {
        return val;
    }
    // Fallback: charge in uAh * ~11.25V approx if charge is provided
    if let Some(charge_uah) = map.get(&format!("POWER_SUPPLY_{}", charge_key)).and_then(|s| s.parse::<u64>().ok()) {
        return (charge_uah as f64 * 11.25) as u64;
    }
    0
}

fn read_file_string(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_file_u32(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok())
}

fn read_file_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok())
}
