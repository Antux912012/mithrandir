use std::fs;
use std::path::Path;
use zbus::blocking::Connection;

pub const DEFAULT_CPU_SYSFS_DIR: &str = "/sys/devices/system/cpu";
pub const DEFAULT_PCIE_ASPM_PATH: &str = "/sys/module/pcie_aspm/parameters/policy";

pub const INTEL_NO_TURBO_REL: &str = "intel_pstate/no_turbo";
pub const AMD_BOOST_REL: &str = "cpufreq/boost";

pub const HADESS_DEST: &str = "net.hadess.PowerProfiles";
pub const HADESS_PATH: &str = "/net/hadess/PowerProfiles";
pub const HADESS_INTERFACE: &str = "net.hadess.PowerProfiles";

pub const UPOWER_DEST: &str = "org.freedesktop.UPower.PowerProfiles";
pub const UPOWER_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
pub const UPOWER_INTERFACE: &str = "org.freedesktop.UPower.PowerProfiles";

// ================= CPU FREQUENCY GOVERNORS =================

pub fn get_available_governors() -> Vec<String> {
    get_available_governors_at(Path::new(DEFAULT_CPU_SYSFS_DIR))
}

pub fn get_available_governors_at(cpu_dir: &Path) -> Vec<String> {
    let p = cpu_dir.join("cpu0/cpufreq/scaling_available_governors");
    if let Ok(content) = fs::read_to_string(p) {
        return content
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
    }
    vec![
        "performance".to_string(),
        "powersave".to_string(),
        "schedutil".to_string(),
        "ondemand".to_string(),
    ]
}

pub fn get_current_governor() -> Option<String> {
    get_current_governor_at(Path::new(DEFAULT_CPU_SYSFS_DIR))
}

pub fn get_current_governor_at(cpu_dir: &Path) -> Option<String> {
    let p = cpu_dir.join("cpu0/cpufreq/scaling_governor");
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

pub fn set_governor(governor: &str) -> Result<(), String> {
    set_governor_at(Path::new(DEFAULT_CPU_SYSFS_DIR), governor)
}

pub fn set_governor_at(cpu_dir: &Path, governor: &str) -> Result<(), String> {
    let mut any_written = false;
    if let Ok(entries) = fs::read_dir(cpu_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with("cpu") && fname[3..].chars().all(|c| c.is_ascii_digit()) {
                let gov_path = entry.path().join("cpufreq/scaling_governor");
                if gov_path.exists() {
                    fs::write(&gov_path, governor.trim())
                        .map_err(|e| format!("Failed to set governor on {}: {}", gov_path.display(), e))?;
                    any_written = true;
                }
            }
        }
    }
    if !any_written {
        return Err("No cpufreq scaling_governor files found".to_string());
    }
    Ok(())
}

// ================= ENERGY PERFORMANCE PREFERENCE (EPP) =================

pub fn get_available_epp() -> Vec<String> {
    get_available_epp_at(Path::new(DEFAULT_CPU_SYSFS_DIR))
}

pub fn get_available_epp_at(cpu_dir: &Path) -> Vec<String> {
    let p = cpu_dir.join("cpu0/cpufreq/energy_performance_available_preferences");
    if let Ok(content) = fs::read_to_string(p) {
        return content
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
    }
    vec![
        "default".to_string(),
        "performance".to_string(),
        "balance_performance".to_string(),
        "balance_power".to_string(),
        "power".to_string(),
    ]
}

pub fn get_current_epp() -> Option<String> {
    get_current_epp_at(Path::new(DEFAULT_CPU_SYSFS_DIR))
}

pub fn get_current_epp_at(cpu_dir: &Path) -> Option<String> {
    let p = cpu_dir.join("cpu0/cpufreq/energy_performance_preference");
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

pub fn set_epp(epp: &str) -> Result<(), String> {
    set_epp_at(Path::new(DEFAULT_CPU_SYSFS_DIR), epp)
}

pub fn set_epp_at(cpu_dir: &Path, epp: &str) -> Result<(), String> {
    let mut any_written = false;
    if let Ok(entries) = fs::read_dir(cpu_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with("cpu") && fname[3..].chars().all(|c| c.is_ascii_digit()) {
                let epp_path = entry.path().join("cpufreq/energy_performance_preference");
                if epp_path.exists() {
                    fs::write(&epp_path, epp.trim())
                        .map_err(|e| format!("Failed to set EPP on {}: {}", epp_path.display(), e))?;
                    any_written = true;
                }
            }
        }
    }
    if !any_written {
        return Err("No energy_performance_preference files found".to_string());
    }
    Ok(())
}

// ================= CPU TURBO BOOST =================

pub fn is_turbo_boost_supported() -> bool {
    is_turbo_boost_supported_at(Path::new(DEFAULT_CPU_SYSFS_DIR))
}

pub fn is_turbo_boost_supported_at(cpu_dir: &Path) -> bool {
    cpu_dir.join(INTEL_NO_TURBO_REL).exists() || cpu_dir.join(AMD_BOOST_REL).exists()
}

pub fn is_turbo_boost_enabled() -> Option<bool> {
    is_turbo_boost_enabled_at(Path::new(DEFAULT_CPU_SYSFS_DIR))
}

pub fn is_turbo_boost_enabled_at(cpu_dir: &Path) -> Option<bool> {
    // Intel: 0 = Turbo enabled, 1 = Turbo disabled
    let intel_path = cpu_dir.join(INTEL_NO_TURBO_REL);
    if let Ok(content) = fs::read_to_string(&intel_path) {
        return match content.trim() {
            "0" => Some(true),
            "1" => Some(false),
            _ => None,
        };
    }

    // AMD: 1 = Boost enabled, 0 = Boost disabled
    let amd_path = cpu_dir.join(AMD_BOOST_REL);
    if let Ok(content) = fs::read_to_string(&amd_path) {
        return match content.trim() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        };
    }

    None
}

pub fn set_turbo_boost(enable: bool) -> Result<(), String> {
    set_turbo_boost_at(Path::new(DEFAULT_CPU_SYSFS_DIR), enable)
}

pub fn set_turbo_boost_at(cpu_dir: &Path, enable: bool) -> Result<(), String> {
    let mut updated = false;

    let intel_path = cpu_dir.join(INTEL_NO_TURBO_REL);
    if intel_path.exists() {
        let val = if enable { "0\n" } else { "1\n" };
        fs::write(&intel_path, val)
            .map_err(|e| format!("Failed to set Intel Turbo Boost: {}", e))?;
        updated = true;
    }

    let amd_path = cpu_dir.join(AMD_BOOST_REL);
    if amd_path.exists() {
        let val = if enable { "1\n" } else { "0\n" };
        fs::write(&amd_path, val)
            .map_err(|e| format!("Failed to set AMD Boost: {}", e))?;
        updated = true;
    }

    if !updated {
        return Err("No CPU turbo boost control interface detected".to_string());
    }
    Ok(())
}

// ================= PCIE ASPM =================

pub fn get_current_aspm_policy() -> Option<String> {
    get_current_aspm_policy_at(Path::new(DEFAULT_PCIE_ASPM_PATH))
}

pub fn get_current_aspm_policy_at(aspm_path: &Path) -> Option<String> {
    let content = fs::read_to_string(aspm_path).ok()?;
    if let (Some(start), Some(end)) = (content.find('['), content.find(']')) {
        if end > start + 1 {
            return Some(content[start + 1..end].trim().to_string());
        }
    }
    let trimmed = content.trim();
    if !trimmed.is_empty() {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub fn set_aspm_policy(policy: &str) -> Result<(), String> {
    set_aspm_policy_at(Path::new(DEFAULT_PCIE_ASPM_PATH), policy)
}

pub fn set_aspm_policy_at(aspm_path: &Path, policy: &str) -> Result<(), String> {
    if aspm_path.exists() {
        fs::write(aspm_path, policy.trim())
            .map_err(|e| format!("Failed to write PCIe ASPM policy to {}: {}", aspm_path.display(), e))
    } else {
        Ok(()) // Gracefully ignore if ASPM tunable does not exist
    }
}

// ================= POWER PROFILES DAEMON (D-BUS) =================

pub fn get_power_profiles_daemon_active_profile() -> Result<String, String> {
    let conn = Connection::system().map_err(|e| format!("System D-Bus unavailable: {}", e))?;

    // Try net.hadess.PowerProfiles first
    if let Ok(proxy) = zbus::blocking::Proxy::new(&conn, HADESS_DEST, HADESS_PATH, HADESS_INTERFACE) {
        if let Ok(active) = proxy.get_property::<String>("ActiveProfile") {
            return Ok(active);
        }
    }

    // Try org.freedesktop.UPower.PowerProfiles fallback
    if let Ok(proxy) = zbus::blocking::Proxy::new(&conn, UPOWER_DEST, UPOWER_PATH, UPOWER_INTERFACE) {
        if let Ok(active) = proxy.get_property::<String>("ActiveProfile") {
            return Ok(active);
        }
    }

    Err("power-profiles-daemon is not active or reachable on system bus".to_string())
}

pub fn set_power_profiles_daemon_profile(profile: &str) -> Result<(), String> {
    let conn = Connection::system().map_err(|e| format!("System D-Bus unavailable: {}", e))?;

    // Try net.hadess.PowerProfiles
    if let Ok(proxy) = zbus::blocking::Proxy::new(&conn, HADESS_DEST, HADESS_PATH, HADESS_INTERFACE) {
        if proxy.set_property("ActiveProfile", profile).is_ok() {
            return Ok(());
        }
    }

    // Try org.freedesktop.UPower.PowerProfiles fallback
    if let Ok(proxy) = zbus::blocking::Proxy::new(&conn, UPOWER_DEST, UPOWER_PATH, UPOWER_INTERFACE) {
        if proxy.set_property("ActiveProfile", profile).is_ok() {
            return Ok(());
        }
    }

    Err(format!(
        "Failed to set profile '{}' via power-profiles-daemon D-Bus",
        profile
    ))
}
