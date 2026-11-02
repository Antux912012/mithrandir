use std::fs;
use std::path::Path;
use zbus::blocking::Connection;

pub const DEFAULT_SCX_STATE_PATH: &str = "/sys/kernel/sched_ext/state";
pub const DEFAULT_SCX_OPS_PATH: &str = "/sys/kernel/sched_ext/root/ops";

pub const SCX_DBUS_DEST: &str = "org.scx.Loader";
pub const SCX_DBUS_PATH: &str = "/org/scx/Loader";
pub const SCX_DBUS_INTERFACE: &str = "org.scx.Loader";

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScxMode {
    Auto = 0,
    Gaming = 1,
    PowerSave = 2,
    LowLatency = 3,
    Server = 4,
}

impl ScxMode {
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => ScxMode::Gaming,
            2 => ScxMode::PowerSave,
            3 => ScxMode::LowLatency,
            4 => ScxMode::Server,
            _ => ScxMode::Auto,
        }
    }

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

/// Checks if sched-ext (SCX) is supported by the running kernel via sysfs.
pub fn is_scx_supported() -> bool {
    is_scx_supported_at(Path::new(DEFAULT_SCX_STATE_PATH))
}

pub fn is_scx_supported_at(state_path: &Path) -> bool {
    state_path.exists()
}

/// Reads the raw state string from sysfs (e.g., "enabled", "disabled").
pub fn get_sysfs_scheduler_state() -> Option<String> {
    get_sysfs_scheduler_state_at(Path::new(DEFAULT_SCX_STATE_PATH))
}

pub fn get_sysfs_scheduler_state_at(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Reads the active sched-ext scheduler name from `/sys/kernel/sched_ext/root/ops`.
pub fn get_sysfs_active_scheduler() -> String {
    get_sysfs_active_scheduler_at(Path::new(DEFAULT_SCX_OPS_PATH))
}

pub fn get_sysfs_active_scheduler_at(ops_path: &Path) -> String {
    if let Ok(content) = fs::read_to_string(ops_path) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "none".to_string()
}

/// Queries the active scheduler via `org.scx.Loader` D-Bus, falling back to sysfs.
pub fn get_active_scheduler() -> String {
    if let Ok(conn) = Connection::system() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            SCX_DBUS_DEST,
            SCX_DBUS_PATH,
            SCX_DBUS_INTERFACE,
        ) {
            if let Ok(sched) = proxy.get_property::<String>("CurrentScheduler") {
                let trimmed = sched.trim();
                if !trimmed.is_empty() && trimmed != "unknown" {
                    return trimmed.to_string();
                }
            }
        }
    }

    get_sysfs_active_scheduler()
}

/// Retrieves scheduler mode and list of supported schedulers.
pub fn get_scx_info() -> (u32, Vec<String>) {
    if let Ok(conn) = Connection::system() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            SCX_DBUS_DEST,
            SCX_DBUS_PATH,
            SCX_DBUS_INTERFACE,
        ) {
            let mode = proxy.get_property::<u32>("SchedulerMode").unwrap_or(0);
            let supported = proxy.get_property::<Vec<String>>("SupportedSchedulers").unwrap_or_default();
            if !supported.is_empty() {
                return (mode, supported);
            }
        }
    }

    // Fallback: discover available scx_* binaries on system
    let mut scheds = Vec::new();
    let known_candidates = [
        "scx_bpfland",
        "scx_lavd",
        "scx_rusty",
        "scx_flash",
        "scx_cosmos",
        "scx_beerland",
        "scx_p2dq",
        "scx_rustland",
    ];

    for name in &known_candidates {
        if which_binary(name) {
            scheds.push(name.to_string());
        }
    }

    (0, scheds)
}

fn which_binary(name: &str) -> bool {
    let standard_dirs = ["/usr/bin", "/usr/local/bin", "/bin"];
    for dir in &standard_dirs {
        if Path::new(dir).join(name).exists() {
            return true;
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for entry in std::env::split_paths(&path_var) {
            if entry.join(name).exists() {
                return true;
            }
        }
    }
    false
}

/// Switches active SCX scheduler via `org.scx.Loader` D-Bus, falling back to `scxctl`.
pub fn switch_scheduler(scheduler: &str, mode: u32) -> Result<(), String> {
    // 1. Try D-Bus interface
    if let Ok(conn) = Connection::system() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            SCX_DBUS_DEST,
            SCX_DBUS_PATH,
            SCX_DBUS_INTERFACE,
        ) {
            let call_res: Result<(), zbus::Error> = proxy.call("SwitchScheduler", &(scheduler, mode));
            if call_res.is_ok() {
                return Ok(());
            }
        }
    }

    // 2. Fallback to scxctl command line
    if which_binary("scxctl") {
        let output = std::process::Command::new("scxctl")
            .arg("switch")
            .arg(scheduler)
            .arg("--mode")
            .arg(mode.to_string())
            .output()
            .map_err(|e| format!("Failed to execute scxctl: {}", e))?;

        if output.status.success() {
            return Ok(());
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("scxctl switch failed: {}", err.trim()));
        }
    }

    Err(format!(
        "Failed to switch to scheduler {}: org.scx.Loader D-Bus unavailable and scxctl not found",
        scheduler
    ))
}

/// Stops active SCX scheduler and reverts to default kernel scheduler (BORE / EEVDF).
pub fn stop_scheduler() -> Result<(), String> {
    // 1. Try D-Bus interface
    if let Ok(conn) = Connection::system() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            SCX_DBUS_DEST,
            SCX_DBUS_PATH,
            SCX_DBUS_INTERFACE,
        ) {
            let call_res: Result<(), zbus::Error> = proxy.call("StopScheduler", &());
            if call_res.is_ok() {
                return Ok(());
            }
        }
    }

    // 2. Fallback to scxctl command line
    if which_binary("scxctl") {
        let output = std::process::Command::new("scxctl")
            .arg("stop")
            .output()
            .map_err(|e| format!("Failed to execute scxctl: {}", e))?;

        if output.status.success() {
            return Ok(());
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("scxctl stop failed: {}", err.trim()));
        }
    }

    // If already stopped, that is acceptable
    if get_sysfs_active_scheduler() == "none" {
        return Ok(());
    }

    Err("Failed to stop SCX scheduler: org.scx.Loader D-Bus unavailable and scxctl not found".to_string())
}
