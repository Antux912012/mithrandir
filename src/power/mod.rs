pub mod coordinator;
pub mod profiles;

pub use coordinator::{
    apply_kernel_state_privileged, apply_preset, get_active_preset, run_privileged_helper,
    set_active_preset, PowerPreset,
};
pub use profiles::{
    get_available_epp, get_available_governors, get_current_aspm_policy, get_current_epp,
    get_current_governor, get_power_profiles_daemon_active_profile, is_turbo_boost_enabled,
    is_turbo_boost_supported, set_aspm_policy, set_epp, set_governor,
    set_power_profiles_daemon_profile, set_turbo_boost,
};

use std::sync::atomic::{AtomicBool, Ordering};

static POWER_SAVER_STATE: AtomicBool = AtomicBool::new(false);

/// Hardware-level Power Saver controller managing:
/// - Intel P-State CPU Turbo Boost (`intel_pstate/no_turbo`)
/// - Energy Performance Preference EPP (`cpufreq/energy_performance_preference`)
/// - PCIe Active State Power Management ASPM (`pcie_aspm/parameters/policy`)
pub struct PowerManager;

impl PowerManager {
    /// Detects if hardware-level power saving policies are supported on this machine.
    pub fn is_supported() -> bool {
        std::path::Path::new("/sys/devices/system/cpu/intel_pstate/no_turbo").exists()
            || std::path::Path::new("/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference").exists()
            || std::path::Path::new("/sys/module/pcie_aspm/parameters/policy").exists()
    }

    /// Reads sysfs hardware state at startup to determine if power saver mode is active.
    pub fn is_power_saver_active() -> bool {
        // 1. Intel P-State no_turbo check (1 = Turbo Disabled = Power Saver active)
        if let Ok(content) = std::fs::read_to_string("/sys/devices/system/cpu/intel_pstate/no_turbo") {
            if content.trim() == "1" {
                POWER_SAVER_STATE.store(true, Ordering::SeqCst);
                return true;
            }
        }

        // 2. CPU0 EPP check
        if let Ok(content) = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference") {
            let val = content.trim();
            if val == "power" || val == "powersave" {
                POWER_SAVER_STATE.store(true, Ordering::SeqCst);
                return true;
            }
        }

        // 3. PCIe ASPM check
        if let Ok(content) = std::fs::read_to_string("/sys/module/pcie_aspm/parameters/policy") {
            if content.contains("[powersupersave]") {
                POWER_SAVER_STATE.store(true, Ordering::SeqCst);
                return true;
            }
        }

        POWER_SAVER_STATE.load(Ordering::SeqCst)
    }

    /// Updates internal cached power saver flag.
    pub fn set_cached_state(active: bool) {
        POWER_SAVER_STATE.store(active, Ordering::SeqCst);
    }

    /// Toggles hardware power saving settings across CPU and PCIe bus.
    pub fn set_power_saver(enable: bool) -> Result<(), String> {
        let preset = if enable {
            PowerPreset::Battery
        } else {
            PowerPreset::Balanced
        };

        match coordinator::apply_preset(preset) {
            Ok(_) => {
                POWER_SAVER_STATE.store(enable, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
