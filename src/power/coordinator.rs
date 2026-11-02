use crate::kernel::{bore, scx};
use crate::power::profiles;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerPreset {
    Battery = 0,
    Balanced = 1,
    LowLatency = 2,
    Gaming = 3,
}

static ACTIVE_PRESET: AtomicU8 = AtomicU8::new(1); // Default to Balanced

impl PowerPreset {
    pub fn name(&self) -> &'static str {
        match self {
            PowerPreset::Battery => "Battery / Silent",
            PowerPreset::Balanced => "Balanced",
            PowerPreset::LowLatency => "Low-Latency / Workstation",
            PowerPreset::Gaming => "Max Performance / Gaming",
        }
    }

    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => PowerPreset::Battery,
            2 => PowerPreset::LowLatency,
            3 => PowerPreset::Gaming,
            _ => PowerPreset::Balanced,
        }
    }

    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

pub fn get_active_preset() -> PowerPreset {
    PowerPreset::from_u8(ACTIVE_PRESET.load(Ordering::SeqCst))
}

pub fn set_active_preset(preset: PowerPreset) {
    ACTIVE_PRESET.store(preset.as_u8(), Ordering::SeqCst);
}

/// Applies a complete Mithrandir power & kernel scheduler preset.
pub fn apply_preset(preset: PowerPreset) -> Result<(), String> {
    let (ppd_profile, governor, epp, boost, aspm, bore_val, scx_sched, scx_mode) = match preset {
        PowerPreset::Battery => (
            "power-saver",
            "powersave",
            "power",
            false,
            "powersupersave",
            false,
            "scx_lavd",
            scx::ScxMode::PowerSave.as_u32(),
        ),
        PowerPreset::Balanced => (
            "balanced",
            "schedutil",
            "balance_performance",
            true,
            "default",
            true,
            "scx_bpfland",
            scx::ScxMode::Auto.as_u32(),
        ),
        PowerPreset::LowLatency => (
            "performance",
            "performance",
            "balance_performance",
            true,
            "default",
            true,
            "scx_lavd",
            scx::ScxMode::LowLatency.as_u32(),
        ),
        PowerPreset::Gaming => (
            "performance",
            "performance",
            "performance",
            true,
            "default",
            true,
            "scx_flash",
            scx::ScxMode::Gaming.as_u32(),
        ),
    };

    // 1. power-profiles-daemon (unprivileged D-Bus call)
    let _ = profiles::set_power_profiles_daemon_profile(ppd_profile);

    // 2. SCX Scheduler switch (via D-Bus or scxctl)
    if scx::is_scx_supported() {
        let (_active_mode, supported) = scx::get_scx_info();
        if supported.iter().any(|s| s == scx_sched) {
            let _ = scx::switch_scheduler(scx_sched, scx_mode);
        } else if let Some(first_avail) = supported.first() {
            let _ = scx::switch_scheduler(first_avail, scx_mode);
        } else if preset == PowerPreset::Balanced {
            let _ = scx::stop_scheduler();
        }
    }

    // 3. Hardware Tunables (sysfs & procfs)
    // Attempt direct write first
    let mut needs_privileged_helper = false;

    if bore::is_bore_supported() {
        if let Err(e) = bore::set_bore_state(bore_val) {
            if is_permission_error(&e) {
                needs_privileged_helper = true;
            }
        }
    }

    if let Err(e) = profiles::set_governor(governor) {
        if is_permission_error(&e) {
            needs_privileged_helper = true;
        }
    }

    if let Err(e) = profiles::set_epp(epp) {
        if is_permission_error(&e) {
            needs_privileged_helper = true;
        }
    }

    if profiles::is_turbo_boost_supported() {
        if let Err(e) = profiles::set_turbo_boost(boost) {
            if is_permission_error(&e) {
                needs_privileged_helper = true;
            }
        }
    }

    let _ = profiles::set_aspm_policy(aspm);

    // 4. If direct write needed root elevation, call privileged helper via pkexec
    if needs_privileged_helper {
        run_privileged_helper(Some(bore_val), Some(governor), Some(epp), Some(boost), Some(aspm), false)?;
    }

    set_active_preset(preset);
    Ok(())
}

fn is_permission_error(e: &str) -> bool {
    let el = e.to_lowercase();
    el.contains("permission denied") || el.contains("os error 13") || el.contains("operation not permitted")
}

/// Executes the privileged helper `mithrandir --apply-kernel-state` via pkexec.
pub fn run_privileged_helper(
    bore: Option<bool>,
    governor: Option<&str>,
    epp: Option<&str>,
    boost: Option<bool>,
    aspm: Option<&str>,
    persist: bool,
) -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "mithrandir".to_string());

    let mut cmd = std::process::Command::new("pkexec");
    cmd.arg(&current_exe).arg("--apply-kernel-state");

    if let Some(b) = bore {
        cmd.arg("--bore").arg(if b { "1" } else { "0" });
    }
    if let Some(g) = governor {
        cmd.arg("--governor").arg(g);
    }
    if let Some(ep) = epp {
        cmd.arg("--epp").arg(ep);
    }
    if let Some(bt) = boost {
        cmd.arg("--boost").arg(if bt { "1" } else { "0" });
    }
    if let Some(asp) = aspm {
        cmd.arg("--aspm").arg(asp);
    }
    if persist {
        cmd.arg("--persist");
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to invoke pkexec helper: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Privileged helper failed: {}", err.trim()))
    }
}

/// Directly applies kernel & power tunables with root privileges (called by `mithrandir --apply-kernel-state`).
pub fn apply_kernel_state_privileged(
    bore: Option<bool>,
    governor: Option<&str>,
    epp: Option<&str>,
    boost: Option<bool>,
    aspm: Option<&str>,
    persist: bool,
) -> Result<(), String> {
    if let Some(b) = bore {
        if bore::is_bore_supported() {
            let _ = bore::set_bore_state(b);
        }
        if persist {
            let _ = bore::persist_bore_state(b);
        }
    }

    if let Some(gov) = governor {
        let _ = profiles::set_governor(gov);
    }

    if let Some(ep) = epp {
        let _ = profiles::set_epp(ep);
    }

    if let Some(bt) = boost {
        if profiles::is_turbo_boost_supported() {
            let _ = profiles::set_turbo_boost(bt);
        }
    }

    if let Some(asp) = aspm {
        let _ = profiles::set_aspm_policy(asp);
    }

    Ok(())
}
