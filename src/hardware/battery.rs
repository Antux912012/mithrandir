use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use super::resolve_sys_path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatteryFirmwareInfo {
    pub ec_firmware_release: String,
    pub bios_vendor: String,
    pub bios_version: String,
    pub bios_date: String,
    pub acpi_device: String,
    pub manufacture_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub name: String,
    pub model_name: String,
    pub manufacturer: String,
    pub serial_number: String,
    pub technology: String,
    pub status: String,
    pub capacity_level: String,
    pub present: bool,
    pub percentage: u32,
    pub health_percent: f32,
    pub wear_percent: f32,
    pub energy_now_wh: f32,
    pub energy_full_wh: f32,
    pub energy_full_design_wh: f32,
    pub charge_now_mah: f32,
    pub charge_full_mah: f32,
    pub charge_full_design_mah: f32,
    pub voltage_now_v: f32,
    pub voltage_design_v: f32,
    pub voltage_max_design_v: Option<f32>,
    pub current_now_ma: f32,
    pub power_rate_w: f32,
    pub temperature_c: Option<f32>,
    pub cycle_count: u32,
    pub cycle_count_reported: bool,
    pub cycle_wear_rating: String,
    pub charge_start_threshold: Option<u32>,
    pub charge_end_threshold: Option<u32>,
    pub charge_behaviour: Option<String>,
    pub alarm_wh: Option<f32>,
    pub time_remaining_minutes: Option<u32>,
    pub ac_adapter_online: Option<bool>,
    pub ac_adapter_name: Option<String>,
    pub estimated_cells: u32,
    pub firmware: BatteryFirmwareInfo,
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

        let ac_info = detect_ac_adapter(&power_supply_path);
        let fw_base = collect_host_firmware_base();

        if let Ok(entries) = fs::read_dir(&power_supply_path) {
            let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            dirs.sort();
            for path in dirs {
                let type_path = path.join("type");
                if let Ok(typ) = fs::read_to_string(&type_path) {
                    if typ.trim().eq_ignore_ascii_case("battery") {
                        if let Some(bat) =
                            Self::from_sysfs_with_context(&path, device_model, &ac_info, &fw_base)
                        {
                            batteries.push(bat);
                        }
                    }
                }
            }
        }

        batteries
    }

    /// Parses a single battery sysfs directory using supplied AC and firmware context.
    /// Public so unit tests can exercise full report generation against mock sysfs trees.
    pub fn from_sysfs_with_context(
        path: &Path,
        device_model: &str,
        ac_info: &(Option<bool>, Option<String>),
        fw_base: &BatteryFirmwareInfo,
    ) -> Option<Self> {
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

        let present = map
            .get("POWER_SUPPLY_PRESENT")
            .and_then(|s| s.parse::<u32>().ok())
            .or_else(|| read_file_u32(&path.join("present")))
            .map(|v| v != 0)
            .unwrap_or(true);

        let model_name = map
            .get("POWER_SUPPLY_MODEL_NAME")
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| read_file_string(&path.join("model_name")).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "Internal Battery".into());

        let manufacturer = map
            .get("POWER_SUPPLY_MANUFACTURER")
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| read_file_string(&path.join("manufacturer")).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "OEM".into());

        let serial_number = map
            .get("POWER_SUPPLY_SERIAL_NUMBER")
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| read_file_string(&path.join("serial_number")).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "N/A".into());

        let technology = map
            .get("POWER_SUPPLY_TECHNOLOGY")
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| read_file_string(&path.join("technology")).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "Li-poly".into());

        let status = map
            .get("POWER_SUPPLY_STATUS")
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| read_file_string(&path.join("status")).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "Unknown".into());

        let capacity_level = map
            .get("POWER_SUPPLY_CAPACITY_LEVEL")
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| read_file_string(&path.join("capacity_level")).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "Normal".into());

        let capacity: u32 = map
            .get("POWER_SUPPLY_CAPACITY")
            .and_then(|s| s.parse().ok())
            .or_else(|| read_file_u32(&path.join("capacity")))
            .unwrap_or(0)
            .min(100);

        // Voltages in uV (read first so charge_uAh -> energy_uWh conversion uses real battery voltage!)
        let voltage_now_uv: f64 = map
            .get("POWER_SUPPLY_VOLTAGE_NOW")
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| read_file_u64(&path.join("voltage_now")))
            .unwrap_or(0) as f64;

        let voltage_min_design_uv: f64 = map
            .get("POWER_SUPPLY_VOLTAGE_MIN_DESIGN")
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| read_file_u64(&path.join("voltage_min_design")))
            .unwrap_or(0) as f64;

        let voltage_max_design_v: Option<f32> = map
            .get("POWER_SUPPLY_VOLTAGE_MAX_DESIGN")
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| read_file_u64(&path.join("voltage_max_design")))
            .filter(|&v| v > 0)
            .map(|v| (v as f32) / 1_000_000.0);

        let voltage_now_v = (voltage_now_uv / 1_000_000.0) as f32;
        let voltage_design_v = if voltage_min_design_uv > 0.0 {
            (voltage_min_design_uv / 1_000_000.0) as f32
        } else {
            voltage_now_v
        };
        let conversion_voltage = if voltage_design_v > 0.5 {
            voltage_design_v as f64
        } else if voltage_now_v > 0.5 {
            voltage_now_v as f64
        } else {
            11.25
        };

        // Energies (uWh) & Charges (uAh)
        let (energy_now_uwh, charge_now_uah) = read_energy_and_charge(
            &map,
            path,
            "ENERGY_NOW",
            "CHARGE_NOW",
            conversion_voltage,
        );
        let (energy_full_uwh, charge_full_uah) = read_energy_and_charge(
            &map,
            path,
            "ENERGY_FULL",
            "CHARGE_FULL",
            conversion_voltage,
        );
        let (energy_design_uwh, charge_design_uah) = read_energy_and_charge(
            &map,
            path,
            "ENERGY_FULL_DESIGN",
            "CHARGE_FULL_DESIGN",
            conversion_voltage,
        );

        let energy_now_wh = (energy_now_uwh as f32) / 1_000_000.0;
        let energy_full_wh = (energy_full_uwh as f32) / 1_000_000.0;
        let energy_design_wh = (energy_design_uwh as f32) / 1_000_000.0;

        let charge_now_mah = (charge_now_uah as f32) / 1_000.0;
        let charge_full_mah = (charge_full_uah as f32) / 1_000.0;
        let charge_full_design_mah = (charge_design_uah as f32) / 1_000.0;

        // Current (uA) and Power (uW) — derive whichever is absent
        let raw_current_ua: Option<i64> = map
            .get("POWER_SUPPLY_CURRENT_NOW")
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| read_file_i64(&path.join("current_now")));

        let raw_power_uw: Option<i64> = map
            .get("POWER_SUPPLY_POWER_NOW")
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| read_file_i64(&path.join("power_now")));

        let power_rate_w = match raw_power_uw {
            Some(pw) if pw != 0 => (pw.unsigned_abs() as f32) / 1_000_000.0,
            _ => match raw_current_ua {
                Some(cur) if cur != 0 && voltage_now_uv > 0.0 => {
                    ((cur.unsigned_abs() as f64 * voltage_now_uv) / 1e12) as f32
                }
                _ => 0.0,
            },
        };

        let current_now_ma = match raw_current_ua {
            Some(cur) if cur != 0 => (cur.unsigned_abs() as f32) / 1_000.0,
            _ if power_rate_w > 0.0 && voltage_now_v > 0.5 => (power_rate_w / voltage_now_v) * 1000.0,
            _ => 0.0,
        };

        // Cycle count (distinguish "reported as 0" vs "not exposed")
        let cycle_opt = map
            .get("POWER_SUPPLY_CYCLE_COUNT")
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| read_file_i64(&path.join("cycle_count")));
        let (cycle_count, cycle_count_reported) = match cycle_opt {
            Some(c) if c >= 0 => (c as u32, true),
            _ => (0, false),
        };
        let cycle_wear_rating = Self::assess_cycle_wear(cycle_count, cycle_count_reported).to_string();

        // Charge thresholds & behaviour
        let charge_start_threshold = read_file_u32(&path.join("charge_control_start_threshold"))
            .or_else(|| read_file_u32(&path.join("charge_start_threshold")));
        let charge_end_threshold = read_file_u32(&path.join("charge_control_end_threshold"))
            .or_else(|| read_file_u32(&path.join("charge_stop_threshold")));
        let charge_behaviour = read_file_string(&path.join("charge_behaviour"))
            .map(|raw| parse_active_bracket_token(&raw));

        let alarm_wh = read_file_u64(&path.join("alarm"))
            .filter(|&a| a > 0)
            .map(|a| (a as f32) / 1_000_000.0);

        let temperature_c = read_battery_temperature_c(&map, path);

        let health_percent = if energy_design_wh > 0.0 {
            ((energy_full_wh / energy_design_wh) * 100.0).clamp(0.0, 100.0)
        } else {
            100.0
        };
        let wear_percent = (100.0 - health_percent).clamp(0.0, 100.0);

        let time_remaining_minutes = if power_rate_w > 0.2 {
            let status_l = status.to_lowercase();
            if status_l.contains("discharging") && energy_now_wh > 0.0 {
                Some(((energy_now_wh / power_rate_w) * 60.0).round() as u32)
            } else if status_l.contains("charging") && energy_full_wh > energy_now_wh {
                Some((((energy_full_wh - energy_now_wh) / power_rate_w) * 60.0).round() as u32)
            } else {
                None
            }
        } else {
            None
        };

        let estimated_cells = Self::estimate_cell_count(voltage_design_v);

        let acpi_device = fs::read_link(path.join("device"))
            .ok()
            .and_then(|target| target.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "ACPI Power Supply".to_string());

        let manufacture_date = read_manufacture_date(path);

        let mut firmware = fw_base.clone();
        firmware.acpi_device = acpi_device;
        firmware.manufacture_date = manufacture_date;

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
            capacity_level,
            present,
            percentage: capacity,
            health_percent,
            wear_percent,
            energy_now_wh,
            energy_full_wh,
            energy_full_design_wh: energy_design_wh,
            charge_now_mah,
            charge_full_mah,
            charge_full_design_mah,
            voltage_now_v,
            voltage_design_v,
            voltage_max_design_v,
            current_now_ma,
            power_rate_w,
            temperature_c,
            cycle_count,
            cycle_count_reported,
            cycle_wear_rating,
            charge_start_threshold,
            charge_end_threshold,
            charge_behaviour,
            alarm_wh,
            time_remaining_minutes,
            ac_adapter_online: ac_info.0,
            ac_adapter_name: ac_info.1.clone(),
            estimated_cells,
            firmware,
            fru_number,
            compatible_frus,
            recommended_search,
        })
    }

    /// Classifies battery cycle count into a human-readable wear rating.
    pub fn assess_cycle_wear(cycles: u32, reported: bool) -> &'static str {
        if !reported {
            return "Not reported by firmware";
        }
        match cycles {
            0..=100 => "Excellent (Low Cycles)",
            101..=300 => "Good (Healthy Wear)",
            301..=600 => "Moderate Wear",
            601..=900 => "High Wear",
            _ => "Aged (Consider Replacement)",
        }
    }

    /// Estimates lithium battery series cell count from nominal design voltage (~3.7V–3.85V per cell).
    pub fn estimate_cell_count(voltage_design_v: f32) -> u32 {
        if voltage_design_v < 2.0 {
            return 0;
        }
        ((voltage_design_v / 3.8).round() as u32).max(1)
    }

    /// Formats minutes as `Xh Ym` or `Ym`.
    pub fn format_duration(mins: u32) -> String {
        let h = mins / 60;
        let m = mins % 60;
        if h > 0 {
            format!("{}h {}m", h, m)
        } else {
            format!("{}m", m)
        }
    }

    /// Generates a formatted single-battery diagnostic report section.
    pub fn generate_text_report(&self, device_model: &str) -> String {
        let mut lines = Vec::new();
        lines.push(format!("=== Battery Diagnostic Report: {} ({}) ===", self.name, device_model));
        lines.push(format!("Status:               {} ({}% — Level: {})", self.status, self.percentage, self.capacity_level));
        if let Some(online) = self.ac_adapter_online {
            let ac_name = self.ac_adapter_name.as_deref().unwrap_or("Mains");
            lines.push(format!(
                "AC Power Adapter:     {} ({})",
                if online { "Connected" } else { "Disconnected / On Battery" },
                ac_name
            ));
        }
        if let Some(mins) = self.time_remaining_minutes {
            let label = if self.status.to_lowercase().contains("charging")
                && !self.status.to_lowercase().contains("discharging")
            {
                "Est. Time to Full"
            } else {
                "Est. Time to Empty"
            };
            lines.push(format!("{:<22}{}", format!("{}:", label), Self::format_duration(mins)));
        }

        lines.push(String::new());
        lines.push("-- Health & Charge/Discharge Cycles --".to_string());
        lines.push(format!(
            "Health / Wear:        {:.1}% health ({:.1}% capacity wear)",
            self.health_percent, self.wear_percent
        ));
        lines.push(format!(
            "Cycle Count:          {} ({})",
            if self.cycle_count_reported {
                format!("{} cycles", self.cycle_count)
            } else {
                "N/A".to_string()
            },
            self.cycle_wear_rating
        ));
        lines.push(format!(
            "Energy Capacity:      {:.2} Wh now / {:.2} Wh full / {:.2} Wh design",
            self.energy_now_wh, self.energy_full_wh, self.energy_full_design_wh
        ));
        lines.push(format!(
            "Charge Capacity:      {:.0} mAh now / {:.0} mAh full / {:.0} mAh design",
            self.charge_now_mah, self.charge_full_mah, self.charge_full_design_mah
        ));
        if self.charge_start_threshold.is_some() || self.charge_end_threshold.is_some() {
            lines.push(format!(
                "Charge Thresholds:    Start {}% / Stop {}%",
                self.charge_start_threshold
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "N/A".into()),
                self.charge_end_threshold
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "N/A".into()),
            ));
        }
        if let Some(ref beh) = self.charge_behaviour {
            lines.push(format!("Charge Behaviour:     {}", beh));
        }

        lines.push(String::new());
        lines.push("-- Electrical & Thermal Telemetry --".to_string());
        let max_v_str = self
            .voltage_max_design_v
            .map(|v| format!(", Max Design: {:.2} V", v))
            .unwrap_or_default();
        lines.push(format!(
            "Voltage:              {:.3} V current (Nominal Design: {:.2} V{})",
            self.voltage_now_v, self.voltage_design_v, max_v_str
        ));
        lines.push(format!(
            "Power & Current:      {:.2} W ({:.0} mA)",
            self.power_rate_w, self.current_now_ma
        ));
        if let Some(temp) = self.temperature_c {
            lines.push(format!("Temperature:          {:.1} °C", temp));
        }
        if let Some(alarm) = self.alarm_wh {
            lines.push(format!("Low-Battery Alarm:    {:.2} Wh", alarm));
        }

        lines.push(String::new());
        lines.push("-- Firmware & Hardware Identity --".to_string());
        lines.push(format!("EC Firmware Release:  {}", self.firmware.ec_firmware_release));
        lines.push(format!(
            "System BIOS / UEFI:   {} {} ({})",
            self.firmware.bios_vendor, self.firmware.bios_version, self.firmware.bios_date
        ));
        lines.push(format!("ACPI Device Node:     {}", self.firmware.acpi_device));
        lines.push(format!(
            "Model / FRU:          {} (FRU: {})",
            self.model_name, self.fru_number
        ));
        let cells_str = if self.estimated_cells > 0 {
            format!("{}-Cell ", self.estimated_cells)
        } else {
            String::new()
        };
        lines.push(format!(
            "Manufacturer / Tech:  {} • {}{} (Serial: {})",
            self.manufacturer, cells_str, self.technology, self.serial_number
        ));
        if let Some(ref mdate) = self.firmware.manufacture_date {
            lines.push(format!("Manufacture Date:     {}", mdate));
        }

        lines.join("\n")
    }

    /// Generates a full Markdown battery diagnostic report across all detected batteries.
    pub fn generate_markdown_report(batteries: &[BatteryInfo], device_model: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("# Mithrandir Battery Diagnostic Report\n\n"));
        out.push_str(&format!("- **Device:** {}\n", device_model));
        out.push_str(&format!("- **Batteries Detected:** {}\n\n", batteries.len()));

        if batteries.is_empty() {
            out.push_str("_No internal batteries detected on this system._\n");
            return out;
        }

        for bat in batteries {
            out.push_str(&format!("## Battery `{}` — {} ({})\n\n", bat.name, bat.model_name, bat.manufacturer));
            out.push_str("| Metric | Value |\n| :--- | :--- |\n");
            out.push_str(&format!("| Operating Status | {} ({}% — {}) |\n", bat.status, bat.percentage, bat.capacity_level));
            if let Some(online) = bat.ac_adapter_online {
                out.push_str(&format!(
                    "| AC Adapter | {} ({}) |\n",
                    if online { "Connected" } else { "Disconnected" },
                    bat.ac_adapter_name.as_deref().unwrap_or("Mains")
                ));
            }
            if let Some(mins) = bat.time_remaining_minutes {
                out.push_str(&format!("| Estimated Time | {} |\n", Self::format_duration(mins)));
            }
            out.push_str(&format!("| Battery Health | {:.1}% (Wear: {:.1}%) |\n", bat.health_percent, bat.wear_percent));
            out.push_str(&format!(
                "| Charge/Discharge Cycles | {} ({}) |\n",
                if bat.cycle_count_reported { format!("{}", bat.cycle_count) } else { "N/A".into() },
                bat.cycle_wear_rating
            ));
            out.push_str(&format!(
                "| Energy (Now / Full / Design) | {:.2} Wh / {:.2} Wh / {:.2} Wh |\n",
                bat.energy_now_wh, bat.energy_full_wh, bat.energy_full_design_wh
            ));
            out.push_str(&format!(
                "| Charge (Now / Full / Design) | {:.0} mAh / {:.0} mAh / {:.0} mAh |\n",
                bat.charge_now_mah, bat.charge_full_mah, bat.charge_full_design_mah
            ));
            out.push_str(&format!(
                "| Voltage (Current / Design) | {:.3} V / {:.2} V |\n",
                bat.voltage_now_v, bat.voltage_design_v
            ));
            out.push_str(&format!("| Power & Current Draw | {:.2} W ({:.0} mA) |\n", bat.power_rate_w, bat.current_now_ma));
            if let Some(temp) = bat.temperature_c {
                out.push_str(&format!("| Temperature | {:.1} °C |\n", temp));
            }
            if bat.charge_start_threshold.is_some() || bat.charge_end_threshold.is_some() {
                out.push_str(&format!(
                    "| Charge Thresholds | Start {}% / Stop {}% |\n",
                    bat.charge_start_threshold.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
                    bat.charge_end_threshold.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
                ));
            }
            if let Some(ref beh) = bat.charge_behaviour {
                out.push_str(&format!("| Charge Behaviour | `{}` |\n", beh));
            }
            out.push_str(&format!("| EC Firmware Release | `{}` |\n", bat.firmware.ec_firmware_release));
            out.push_str(&format!(
                "| System BIOS / UEFI | {} {} ({}) |\n",
                bat.firmware.bios_vendor, bat.firmware.bios_version, bat.firmware.bios_date
            ));
            out.push_str(&format!("| ACPI Device Node | `{}` |\n", bat.firmware.acpi_device));
            out.push_str(&format!("| Chemistry & Cells | {}-Cell {} |\n", bat.estimated_cells, bat.technology));
            out.push_str(&format!("| Serial Number | `{}` |\n", bat.serial_number));
            out.push_str(&format!("| Primary Part / FRU | `{}` |\n\n", bat.fru_number));
        }

        out
    }

    /// Exports the full Markdown battery report to `path`.
    pub fn export_report_to_file(
        batteries: &[BatteryInfo],
        device_model: &str,
        path: &Path,
    ) -> Result<String, String> {
        let md = Self::generate_markdown_report(batteries, device_model);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed creating report directory {}: {}", parent.display(), e))?;
        }
        fs::write(path, md)
            .map_err(|e| format!("Failed writing battery report to {}: {}", path.display(), e))?;
        Ok(path.to_string_lossy().to_string())
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

fn detect_ac_adapter(power_supply_path: &Path) -> (Option<bool>, Option<String>) {
    if let Ok(entries) = fs::read_dir(power_supply_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(typ) = read_file_string(&p.join("type")) {
                if typ.eq_ignore_ascii_case("mains") {
                    let online = read_file_u32(&p.join("online")).map(|v| v != 0);
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string());
                    return (online, name);
                }
            }
        }
    }
    (None, None)
}

fn collect_host_firmware_base() -> BatteryFirmwareInfo {
    let read_dmi = |file: &str| {
        let p = resolve_sys_path(&format!("/sys/class/dmi/id/{}", file));
        read_file_string(&p).filter(|s| !s.is_empty())
    };
    BatteryFirmwareInfo {
        ec_firmware_release: read_dmi("ec_firmware_release")
            .unwrap_or_else(|| "Not reported by DMI".into()),
        bios_vendor: read_dmi("bios_vendor").unwrap_or_else(|| "Unknown".into()),
        bios_version: read_dmi("bios_version").unwrap_or_else(|| "Unknown".into()),
        bios_date: read_dmi("bios_date").unwrap_or_else(|| "Unknown".into()),
        acpi_device: String::new(),
        manufacture_date: None,
    }
}

fn read_energy_and_charge(
    map: &HashMap<String, String>,
    path: &Path,
    energy_key: &str,
    charge_key: &str,
    voltage_v: f64,
) -> (u64, u64) {
    let energy_uwh = map
        .get(&format!("POWER_SUPPLY_{}", energy_key))
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| read_file_u64(&path.join(energy_key.to_lowercase())));

    let charge_uah = map
        .get(&format!("POWER_SUPPLY_{}", charge_key))
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| read_file_u64(&path.join(charge_key.to_lowercase())));

    match (energy_uwh, charge_uah) {
        (Some(e), Some(c)) => (e, c),
        (Some(e), None) => {
            let derived_c = if voltage_v > 0.5 {
                (e as f64 / voltage_v) as u64
            } else {
                0
            };
            (e, derived_c)
        }
        (None, Some(c)) => {
            let derived_e = (c as f64 * voltage_v) as u64;
            (derived_e, c)
        }
        (None, None) => (0, 0),
    }
}

fn read_battery_temperature_c(map: &HashMap<String, String>, path: &Path) -> Option<f32> {
    // 1. Direct sysfs `temp` attribute (tenths of degree Celsius)
    if let Some(raw) = map
        .get("POWER_SUPPLY_TEMP")
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| read_file_i64(&path.join("temp")))
    {
        let c = (raw as f32) / 10.0;
        if (-20.0..=100.0).contains(&c) {
            return Some(c);
        }
    }

    // 2. Child hwmon*/temp1_input (millidegrees Celsius)
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with("hwmon") {
                if let Some(milli) = read_file_i64(&entry.path().join("temp1_input")) {
                    let c = (milli as f32) / 1000.0;
                    if (-20.0..=100.0).contains(&c) {
                        return Some(c);
                    }
                }
            }
        }
    }

    None
}

fn read_manufacture_date(path: &Path) -> Option<String> {
    let year = read_file_u32(&path.join("manufacture_year"))?;
    let month = read_file_u32(&path.join("manufacture_month")).unwrap_or(1);
    let day = read_file_u32(&path.join("manufacture_day")).unwrap_or(1);
    if year > 1990 {
        Some(format!("{:04}-{:02}-{:02}", year, month, day))
    } else {
        None
    }
}

/// Extracts the bracketed active option from Linux sysfs strings like `[auto] inhibit-charge force-discharge`.
pub fn parse_active_bracket_token(raw: &str) -> String {
    if let Some(start) = raw.find('[') {
        if let Some(end) = raw[start + 1..].find(']') {
            return raw[start + 1..start + 1 + end].to_string();
        }
    }
    raw.trim().to_string()
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

fn read_file_i64(path: &Path) -> Option<i64> {
    fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok())
}
