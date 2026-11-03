use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use zbus::blocking::Connection;

use crate::backend::host_bridge::HostBridge;
use crate::os_info::{detect_os, LinuxDistro};

pub const DEFAULT_SCX_STATE_PATH: &str = "/sys/kernel/sched_ext/state";
pub const DEFAULT_SCX_OPS_PATH: &str = "/sys/kernel/sched_ext/root/ops";

pub const SCX_DBUS_DEST: &str = "org.scx.Loader";
pub const SCX_DBUS_PATH: &str = "/org/scx/Loader";
pub const SCX_DBUS_INTERFACE: &str = "org.scx.Loader";

pub const SCX_BUNDLE_VERSION: &str = "1.1.4";
pub const MITHRANDIR_GITHUB_RELEASE_BASE: &str =
    "https://github.com/Antux912012/mithrandir/releases/latest/download";

/// Complete list of official sched-ext (`scx` v1.1.4) userspace/BPF schedulers from https://github.com/sched-ext/scx
pub const KNOWN_SCX_SCHEDULERS: &[&str] = &[
    "scx_bpfland",
    "scx_lavd",
    "scx_flash",
    "scx_rusty",
    "scx_cosmos",
    "scx_p2dq",
    "scx_beerland",
    "scx_rustland",
    "scx_layered",
    "scx_tickless",
    "scx_cake",
    "scx_eevdf",
    "scx_mitosis",
    "scx_rlfifo",
    "scx_simple",
    "scx_central",
    "scx_nest",
    "scx_qmap",
    "scx_flatcg",
    "scx_userland",
];

static LAST_SCX_MODE: AtomicU32 = AtomicU32::new(0);

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

    pub fn all() -> &'static [ScxMode] {
        &[
            ScxMode::Auto,
            ScxMode::Gaming,
            ScxMode::PowerSave,
            ScxMode::LowLatency,
            ScxMode::Server,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ScxMode::Auto => "Auto (Default)",
            ScxMode::Gaming => "Gaming",
            ScxMode::PowerSave => "Power Save",
            ScxMode::LowLatency => "Low Latency",
            ScxMode::Server => "Server / Throughput",
        }
    }

    pub fn cli_name(&self) -> &'static str {
        match self {
            ScxMode::Auto => "auto",
            ScxMode::Gaming => "gaming",
            ScxMode::PowerSave => "powersave",
            ScxMode::LowLatency => "lowlatency",
            ScxMode::Server => "server",
        }
    }

    /// Returns scheduler-specific CLI flags when launching an `scx_*` binary directly.
    pub fn flags_for_scheduler(&self, scheduler: &str) -> &'static [&'static str] {
        match (self, scheduler) {
            (ScxMode::Auto, _) => &[],
            (ScxMode::Gaming, "scx_bpfland") => &["-m", "performance"],
            (ScxMode::Gaming, "scx_lavd") => &["--performance"],
            (ScxMode::Gaming, "scx_flash") => &["-m", "performance"],
            (ScxMode::Gaming, "scx_p2dq") => &["--sched-mode", "performance"],
            (ScxMode::PowerSave, "scx_bpfland") => &["-m", "powersave"],
            (ScxMode::PowerSave, "scx_lavd") => &["--powersave"],
            (ScxMode::PowerSave, "scx_flash") => &["-m", "powersave"],
            (ScxMode::PowerSave, "scx_p2dq") => &["--sched-mode", "efficiency"],
            (ScxMode::LowLatency, "scx_bpfland") => &["-s", "5000", "-S", "500"],
            (ScxMode::LowLatency, "scx_lavd") => &["--performance"],
            (ScxMode::LowLatency, "scx_flash") => &["-m", "all"],
            (ScxMode::Server, "scx_bpfland") => &["-s", "20000"],
            (ScxMode::Server, "scx_cosmos") => &["-s", "20000"],
            _ => &[],
        }
    }
}

/// Returns a concise human-readable description of a given `sched-ext` scheduler.
pub fn scheduler_description(name: &str) -> &'static str {
    match name {
        "none" => "Default kernel scheduler (BORE / EEVDF) — sched-ext BPF scheduler disabled",
        "scx_bpfland" => "Interactive vruntime BPF scheduler prioritizing desktop responsiveness & gaming",
        "scx_lavd" => "Latency-criticality Aware Virtual Deadline scheduler for gaming & low-latency audio",
        "scx_flash" => "EDF low-latency scheduler ensuring consistent multimedia & gaming frame pacing",
        "scx_rusty" => "Multi-domain load-balancing BPF/Rust scheduler for general & multi-CCD workloads",
        "scx_cosmos" => "Lightweight general-purpose scheduler optimized for CPU cache locality & low overhead",
        "scx_p2dq" => "Pick-2 Dispatch Queues scheduler with LLC locality & dynamic load balancing",
        "scx_beerland" => "Cache-locality & throughput oriented BPF scheduler related to bpfland",
        "scx_rustland" => "Userspace Rust scheduler prioritizing interactive tasks over heavy background load",
        "scx_layered" => "Configurable multi-layer BPF scheduler for fine-grained workload isolation",
        "scx_tickless" => "Tickless scheduler routing timer ticks to a dedicated housekeeping CPU pool",
        "scx_cake" => "CAKE-inspired low-latency fair queueing BPF CPU scheduler for gaming & desktop",
        "scx_eevdf" => "BPF implementation of Earliest Eligible Virtual Deadline First scheduling",
        "scx_mitosis" => "Dynamic cell-partitioning BPF scheduler for multi-core & cgroup isolation",
        "scx_rlfifo" => "Simple Rust userland FIFO scheduler built on the scx_rlfifo framework",
        "scx_simple" => "Minimal vtime/FIFO reference BPF scheduler from upstream sched-ext",
        "scx_central" => "Central dispatcher tickless scheduler minimizing timer interrupt overhead",
        "scx_nest" => "Core-nesting scheduler keeping active cores warm for frequency boost gains",
        "scx_qmap" => "Queued BPF map reference scheduler with multi-level priority queues",
        "scx_flatcg" => "Flattened cgroup hierarchy BPF scheduler for high-density container workloads",
        "scx_userland" => "Minimal userspace C ordered-list reference scheduler",
        _ => "Pluggable sched-ext (SCX) BPF kernel scheduler",
    }
}

/// Normalizes a raw scheduler name from `/sys/kernel/sched_ext/root/ops` or D-Bus
/// (e.g., `"bpfland"` or `"bpfland_1.1.4_g..."`) into its canonical binary name (`"scx_bpfland"`).
pub fn normalize_scheduler_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("none")
        || trimmed.eq_ignore_ascii_case("unknown")
    {
        return "none".to_string();
    }

    for &known in KNOWN_SCX_SCHEDULERS {
        let short = known.trim_start_matches("scx_");
        if trimmed == known
            || trimmed == short
            || trimmed.starts_with(&format!("{}_", known))
            || trimmed.starts_with(&format!("{}-", known))
            || trimmed.starts_with(&format!("{}_", short))
            || trimmed.starts_with(&format!("{}-", short))
        {
            return known.to_string();
        }
    }

    if trimmed.starts_with("scx_") {
        trimmed.to_string()
    } else {
        format!("scx_{}", trimmed)
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

/// Reads the raw active sched-ext scheduler name from `/sys/kernel/sched_ext/root/ops`.
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

/// Inspects `/boot/config-*` to provide a helpful diagnostic if the currently running kernel lacks `CONFIG_SCHED_CLASS_EXT=y`.
pub fn get_kernel_scx_diagnostic() -> String {
    if is_scx_supported() {
        let state = get_sysfs_scheduler_state().unwrap_or_else(|| "disabled".to_string());
        return format!("Kernel sched_ext active ({})", state);
    }

    // Check if another installed kernel in /boot has CONFIG_SCHED_CLASS_EXT=y
    let mut scx_kernels = Vec::new();
    if let Ok(entries) = fs::read_dir("/boot") {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if let Some(kver) = fname.strip_prefix("config-") {
                if let Ok(cfg) = fs::read_to_string(entry.path()) {
                    if cfg.lines().any(|l| l.trim() == "CONFIG_SCHED_CLASS_EXT=y") {
                        scx_kernels.push(kver.to_string());
                    }
                }
            }
        }
    }

    if !scx_kernels.is_empty() {
        scx_kernels.sort();
        format!(
            "Current kernel lacks CONFIG_SCHED_CLASS_EXT=y (available in /boot: {})",
            scx_kernels.join(", ")
        )
    } else {
        "Current kernel compiled without CONFIG_SCHED_CLASS_EXT=y (enable in CachyOS Kernel Updater)".to_string()
    }
}

/// Queries the active scheduler via `org.scx.Loader` D-Bus, falling back to normalized sysfs `ops` or `/etc/default/scx`.
pub fn get_active_scheduler() -> String {
    if let Ok(conn) = Connection::system() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            SCX_DBUS_DEST,
            SCX_DBUS_PATH,
            SCX_DBUS_INTERFACE,
        ) {
            if let Ok(sched) = proxy.get_property::<String>("CurrentScheduler") {
                let normalized = normalize_scheduler_name(&sched);
                if normalized != "none" {
                    return normalized;
                }
            }
        }
    }

    if let Some(state) = get_sysfs_scheduler_state() {
        if state == "disabled" {
            return "none".to_string();
        }
    }

    let sysfs_sched = normalize_scheduler_name(&get_sysfs_active_scheduler());
    if sysfs_sched != "none" {
        return sysfs_sched;
    }

    // If kernel doesn't have sched_ext active right now, check configured default in /etc/default/scx
    if !is_scx_supported() {
        if let Ok(content) = fs::read_to_string("/etc/default/scx") {
            for line in content.lines() {
                if let Some(val) = line.trim().strip_prefix("SCX_SCHEDULER=") {
                    let clean = val.trim_matches('"').trim_matches('\'').trim();
                    let norm = normalize_scheduler_name(clean);
                    if norm != "none" {
                        return norm;
                    }
                }
            }
        }
    }

    "none".to_string()
}

/// Retrieves scheduler mode and list of supported schedulers installed on the system.
pub fn get_scx_info() -> (u32, Vec<String>) {
    let mut dbus_mode = None;
    let mut scheds = Vec::new();

    if let Ok(conn) = Connection::system() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            SCX_DBUS_DEST,
            SCX_DBUS_PATH,
            SCX_DBUS_INTERFACE,
        ) {
            if let Ok(mode) = proxy.get_property::<u32>("SchedulerMode") {
                dbus_mode = Some(mode);
                LAST_SCX_MODE.store(mode, Ordering::SeqCst);
            }
            if let Ok(supported) = proxy.get_property::<Vec<String>>("SupportedSchedulers") {
                for s in supported {
                    let norm = normalize_scheduler_name(&s);
                    if norm != "none" && !scheds.contains(&norm) {
                        scheds.push(norm);
                    }
                }
            }
        }
    }

    for discovered in discover_installed_schedulers() {
        if !scheds.contains(&discovered) {
            scheds.push(discovered);
        }
    }

    let mode = dbus_mode.unwrap_or_else(|| LAST_SCX_MODE.load(Ordering::SeqCst));
    (mode, scheds)
}

/// Discovers installed `scx_*` scheduler binaries across standard bin/sbin directories and Flatpak host.
pub fn discover_installed_schedulers() -> Vec<String> {
    let mut found = Vec::new();

    if HostBridge::is_flatpak() {
        let candidates = KNOWN_SCX_SCHEDULERS.join(" ");
        let script = format!(
            "PATH=\"$PATH:/usr/bin:/usr/sbin:/usr/local/bin:/usr/local/sbin:/bin:/sbin\"; \
             for s in {}; do command -v \"$s\" >/dev/null 2>&1 && echo \"$s\"; done",
            candidates
        );
        if let Ok((0, stdout, _)) = HostBridge::run_command("sh", &["-c", &script]) {
            for line in stdout.lines() {
                let name = line.trim();
                if !name.is_empty() && !found.contains(&name.to_string()) {
                    found.push(name.to_string());
                }
            }
            return found;
        }
    }

    for &name in KNOWN_SCX_SCHEDULERS {
        if which_binary(name) {
            found.push(name.to_string());
        }
    }

    found
}

/// Checks if a binary exists in standard bin/sbin directories, `$PATH`, or on the Flatpak host.
pub fn which_binary(name: &str) -> bool {
    let standard_dirs = [
        "/usr/bin",
        "/usr/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/bin",
        "/sbin",
    ];
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
    if HostBridge::is_flatpak() {
        let check_cmd = format!(
            "PATH=\"$PATH:/usr/bin:/usr/sbin:/usr/local/bin:/usr/local/sbin:/bin:/sbin\" command -v {} >/dev/null 2>&1",
            name
        );
        if let Ok((0, _, _)) = HostBridge::run_command("sh", &["-c", &check_cmd]) {
            return true;
        }
    }
    false
}

fn is_valid_scheduler_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.starts_with("scx_")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Switches active SCX scheduler via:
/// 1. `org.scx.Loader` D-Bus service
/// 2. `scxctl` CLI via `HostBridge`
/// 3. Direct privileged execution of the `scx_*` daemon binary on the host
/// If the running kernel does not currently expose `/sys/kernel/sched_ext`, persists the choice to `/etc/default/scx` and `/etc/scx_loader/config.toml` for the next boot into an SCX-enabled kernel.
pub fn switch_scheduler(scheduler: &str, mode: u32) -> Result<(), String> {
    let normalized = normalize_scheduler_name(scheduler);
    if normalized == "none" {
        return stop_scheduler();
    }
    if !is_valid_scheduler_identifier(&normalized) {
        return Err(format!("Invalid scheduler identifier: {}", scheduler));
    }

    let scx_mode = ScxMode::from_u32(mode);
    LAST_SCX_MODE.store(scx_mode.as_u32(), Ordering::SeqCst);

    // If the currently running kernel lacks CONFIG_SCHED_CLASS_EXT=y, save configuration for next boot
    if !is_scx_supported() {
        set_scx_boot_persistence(&normalized, scx_mode)?;
        return Ok(());
    }

    // 1. Try org.scx.Loader D-Bus interface
    if let Ok(conn) = Connection::system() {
        if let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            SCX_DBUS_DEST,
            SCX_DBUS_PATH,
            SCX_DBUS_INTERFACE,
        ) {
            let call_res: Result<(), zbus::Error> =
                proxy.call("SwitchScheduler", &(normalized.as_str(), mode));
            if call_res.is_ok() {
                return Ok(());
            }
        }
    }

    // 2. Try scxctl command line via HostBridge
    if which_binary("scxctl") {
        let short_name = normalized.trim_start_matches("scx_");
        let mode_str = scx_mode.cli_name();
        if let Ok((0, _, _)) =
            HostBridge::run_command("scxctl", &["switch", "--sched", short_name, "--mode", mode_str])
        {
            return Ok(());
        }
        if let Ok((0, _, _)) =
            HostBridge::run_command("scxctl", &["start", "--sched", short_name, "--mode", mode_str])
        {
            return Ok(());
        }
    }

    // 3. Fallback: Direct execution of the `scx_*` binary via Polkit / HostBridge
    if which_binary(&normalized) {
        let flags = scx_mode.flags_for_scheduler(&normalized).join(" ");
        let script = format!(
            "export PATH=\"$PATH:/usr/bin:/usr/sbin:/usr/local/bin:/usr/local/sbin:/bin:/sbin\"; \
             BIN=$(command -v {sched}); \
             if [ -z \"$BIN\" ]; then echo 'Binary {sched} not found' >&2; exit 127; fi; \
             pkill -TERM -x 'scx_[a-z0-9_]+' 2>/dev/null || true; \
             sleep 0.2; \
             pkill -KILL -x 'scx_[a-z0-9_]+' 2>/dev/null || true; \
             if [ -n \"{flags}\" ]; then \
                 nohup \"$BIN\" {flags} >/tmp/mithrandir-{sched}.log 2>&1 & \
                 sleep 0.4; \
                 if ! kill -0 $! 2>/dev/null; then \
                     nohup \"$BIN\" >/tmp/mithrandir-{sched}.log 2>&1 & \
                     sleep 0.4; \
                 fi; \
             else \
                 nohup \"$BIN\" >/tmp/mithrandir-{sched}.log 2>&1 & \
                 sleep 0.4; \
             fi; \
             if pgrep -x '{sched}' >/dev/null 2>&1 || [ \"$(cat /sys/kernel/sched_ext/state 2>/dev/null)\" = 'enabled' ]; then \
                 exit 0; \
             else \
                 cat /tmp/mithrandir-{sched}.log >&2 2>/dev/null || true; \
                 exit 1; \
             fi",
            sched = normalized,
            flags = flags
        );

        let (code, _, stderr) = HostBridge::run_privileged(&["sh", "-c", &script])?;
        if code == 0 {
            return Ok(());
        } else {
            return Err(format!(
                "Failed to launch {}: {}",
                normalized,
                if stderr.trim().is_empty() {
                    "kernel sched_ext rejected or exited immediately"
                } else {
                    stderr.trim()
                }
            ));
        }
    }

    Err(format!(
        "Scheduler '{}' is not installed on this system. Click 'Install SCX' to install sched-ext.",
        normalized
    ))
}

/// Stops active SCX scheduler and reverts to default kernel scheduler (BORE / EEVDF).
pub fn stop_scheduler() -> Result<(), String> {
    if !is_scx_supported() {
        let _ = set_scx_boot_persistence("none", ScxMode::Auto);
        return Ok(());
    }

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

    // 2. Fallback to scxctl command line via HostBridge
    if which_binary("scxctl") {
        if let Ok((0, _, _)) = HostBridge::run_command("scxctl", &["stop"]) {
            return Ok(());
        }
    }

    if get_active_scheduler() == "none" {
        return Ok(());
    }

    // 3. Fallback: Terminate any running scx_* daemon via Polkit / HostBridge
    let stop_script = "pkill -TERM -x 'scx_[a-z0-9_]+' 2>/dev/null || true; \
                       sleep 0.2; \
                       pkill -KILL -x 'scx_[a-z0-9_]+' 2>/dev/null || true";
    let (code, _, stderr) = HostBridge::run_privileged(&["sh", "-c", stop_script])?;
    if code == 0 || get_active_scheduler() == "none" {
        Ok(())
    } else {
        Err(format!("Failed to stop SCX scheduler: {}", stderr.trim()))
    }
}

/// Persists the selected SCX scheduler and mode across reboots in `/etc/scx_loader/config.toml` and `/etc/default/scx`.
pub fn set_scx_boot_persistence(scheduler: &str, mode: ScxMode) -> Result<(), String> {
    let normalized = normalize_scheduler_name(scheduler);
    if normalized != "none" && !is_valid_scheduler_identifier(&normalized) {
        return Err(format!("Invalid scheduler identifier: {}", scheduler));
    }

    let flags = mode.flags_for_scheduler(&normalized).join(" ");
    let script = if normalized == "none" {
        "rm -f /etc/scx_loader/config.toml /etc/default/scx 2>/dev/null || true; \
         systemctl disable scx_loader.service scx.service 2>/dev/null || true"
            .to_string()
    } else {
        format!(
            "mkdir -p /etc/scx_loader /etc/default && \
             printf '# Generated by Mithrandir Kernel & Power Manager\\ndefault_sched = \"{sched}\"\\ndefault_mode = {mode_str}\\n' > /etc/scx_loader/config.toml && \
             printf '# Generated by Mithrandir Kernel & Power Manager\\nSCX_SCHEDULER={sched}\\nSCX_FLAGS=\"{flags}\"\\n' > /etc/default/scx && \
             (systemctl enable scx.service 2>/dev/null || systemctl enable scx_loader.service 2>/dev/null || true)",
            sched = normalized,
            mode_str = match mode {
                ScxMode::Auto => "\"Auto\"",
                ScxMode::Gaming => "\"Gaming\"",
                ScxMode::PowerSave => "\"PowerSave\"",
                ScxMode::LowLatency => "\"LowLatency\"",
                ScxMode::Server => "\"Server\"",
            },
            flags = flags
        )
    };

    let (code, _, stderr) = HostBridge::run_privileged(&["sh", "-c", &script])?;
    if code == 0 {
        Ok(())
    } else {
        Err(format!("Failed to persist SCX configuration: {}", stderr.trim()))
    }
}

/// Locates the bundled precompiled `scx-scheds-1.1.4-x86_64.tar.xz` archive if present inside Mithrandir's package.
pub fn find_bundled_scx_archive() -> Option<PathBuf> {
    let archive_name = format!("scx-scheds-{}-x86_64.tar.xz", SCX_BUNDLE_VERSION);
    let mut candidates = vec![
        PathBuf::from(format!("/usr/share/mithrandir/scx/{}", archive_name)),
        PathBuf::from(format!("/usr/local/share/mithrandir/scx/{}", archive_name)),
        PathBuf::from(format!("/app/share/mithrandir/scx/{}", archive_name)),
        PathBuf::from(format!("data/scx/{}", archive_name)),
        PathBuf::from(format!("dist/scx/{}", archive_name)),
    ];

    if let Ok(appdir) = std::env::var("APPDIR") {
        candidates.insert(
            0,
            Path::new(&appdir)
                .join("usr/share/mithrandir/scx")
                .join(&archive_name),
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            candidates.insert(
                0,
                bin_dir
                    .join("../share/mithrandir/scx")
                    .join(&archive_name),
            );
            candidates.insert(0, bin_dir.join("../data/scx").join(&archive_name));
            candidates.insert(0, bin_dir.join("../../data/scx").join(&archive_name));
        }
    }

    candidates.into_iter().find(|p| p.is_file())
}

#[derive(Debug, Clone)]
pub struct ScxInstallPlan {
    pub distro_name: String,
    pub summary: String,
    pub command: String,
    pub requires_reboot_for_kernel: bool,
}

/// Generates a 3-stage hybrid installation plan for `sched-ext` (`scx` v1.1.4):
/// 1. Embedded precompiled `scx-scheds-1.1.4-x86_64.tar.xz` bundle (instant offline install)
/// 2. Mithrandir GitHub Releases precompiled `.deb` / `.rpm` / `.tar.xz` package download
/// 3. Upstream Cargo source build (`https://github.com/sched-ext/scx.git` `--profile=release-tiny`)
pub fn get_install_plan() -> ScxInstallPlan {
    let os = detect_os();
    get_install_plan_for_distro(os.distro, &os.pretty_name, is_scx_supported())
}

pub fn get_install_plan_for_distro(
    distro: LinuxDistro,
    pretty_name: &str,
    scx_kernel_supported: bool,
) -> ScxInstallPlan {
    let deb_url = format!(
        "{}/scx-scheds_{}_amd64.deb",
        MITHRANDIR_GITHUB_RELEASE_BASE, SCX_BUNDLE_VERSION
    );
    let rpm_url = format!(
        "{}/scx-scheds-{}-1.x86_64.rpm",
        MITHRANDIR_GITHUB_RELEASE_BASE, SCX_BUNDLE_VERSION
    );
    let tar_url = format!(
        "{}/scx-scheds-{}-x86_64.tar.xz",
        MITHRANDIR_GITHUB_RELEASE_BASE, SCX_BUNDLE_VERSION
    );

    let cargo_source_fallback = format!(
        "rm -rf /tmp/mithrandir-scx-src && \
         git clone --branch v{ver} --depth 1 https://github.com/sched-ext/scx.git /tmp/mithrandir-scx-src && \
         cd /tmp/mithrandir-scx-src && \
         cargo build --profile=release-tiny -p scx_bpfland -p scx_lavd -p scx_flash -p scx_rusty -p scx_cosmos -p scx_p2dq -p scx_beerland -p scx_rustland -p scx_layered -p scx_tickless -p scx_cake -p scx_eevdf -p scx_mitosis -p scx_rlfifo && \
         install -m 0755 target/release-tiny/scx_* /usr/bin/ && \
         rm -rf /tmp/mithrandir-scx-src",
        ver = SCX_BUNDLE_VERSION
    );

    match distro {
        LinuxDistro::Ubuntu => {
            let pkg_extra = if scx_kernel_supported {
                ""
            } else {
                "apt-get install -y linux-generic-wip 2>/dev/null || true; "
            };
            ScxInstallPlan {
                distro_name: pretty_name.to_string(),
                summary: format!(
                    "Precompiled scx-scheds v{} (.deb / bundle) + ppa:arighi/sched-ext fallback",
                    SCX_BUNDLE_VERSION
                ),
                command: format!(
                    "export DEBIAN_FRONTEND=noninteractive; \
                     if [ -n \"$MITHRANDIR_SCX_BUNDLE\" ] && [ -f \"$MITHRANDIR_SCX_BUNDLE\" ]; then \
                         echo '==> Extracting bundled precompiled sched-ext v{ver} binaries...'; \
                         tar -C / -xJf \"$MITHRANDIR_SCX_BUNDLE\" && systemctl daemon-reload 2>/dev/null || true; \
                     elif curl -fsSL \"{deb_url}\" -o /tmp/scx-scheds.deb; then \
                         echo '==> Installing scx-scheds_{ver}_amd64.deb from Mithrandir GitHub Release...'; \
                         dpkg -i /tmp/scx-scheds.deb || apt-get install -f -y; \
                         rm -f /tmp/scx-scheds.deb; \
                     else \
                         apt-get update && apt-get install -y software-properties-common && \
                         add-apt-repository -y --enable-source ppa:arighi/sched-ext && \
                         apt-get update && (apt-get install -y scx || ({cargo_fb})); \
                     fi; \
                     {pkg_extra}",
                    ver = SCX_BUNDLE_VERSION,
                    deb_url = deb_url,
                    cargo_fb = cargo_source_fallback,
                    pkg_extra = pkg_extra
                ),
                requires_reboot_for_kernel: !scx_kernel_supported,
            }
        }
        LinuxDistro::Debian => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: format!(
                "Precompiled scx-scheds_{}_amd64.deb (Mithrandir bundle / GitHub Release + Cargo source fallback)",
                SCX_BUNDLE_VERSION
            ),
            command: format!(
                "export DEBIAN_FRONTEND=noninteractive; \
                 if [ -n \"$MITHRANDIR_SCX_BUNDLE\" ] && [ -f \"$MITHRANDIR_SCX_BUNDLE\" ]; then \
                     echo '==> Extracting bundled precompiled sched-ext v{ver} binaries...'; \
                     tar -C / -xJf \"$MITHRANDIR_SCX_BUNDLE\" && systemctl daemon-reload 2>/dev/null || true; \
                 elif curl -fsSL \"{deb_url}\" -o /tmp/scx-scheds.deb; then \
                     echo '==> Installing scx-scheds_{ver}_amd64.deb from Mithrandir GitHub Release...'; \
                     dpkg -i /tmp/scx-scheds.deb || (apt-get update && apt-get install -f -y); \
                     rm -f /tmp/scx-scheds.deb; \
                 elif curl -fsSL \"{tar_url}\" -o /tmp/scx-scheds.tar.xz; then \
                     tar -C / -xJf /tmp/scx-scheds.tar.xz && rm -f /tmp/scx-scheds.tar.xz; \
                 else \
                     apt-get update && apt-get install -y git build-essential pkg-config clang llvm libbpf-dev libelf-dev libzstd-dev zlib1g-dev protobuf-compiler libseccomp-dev cargo rustc && \
                     {cargo_fb}; \
                 fi",
                ver = SCX_BUNDLE_VERSION,
                deb_url = deb_url,
                tar_url = tar_url,
                cargo_fb = cargo_source_fallback
            ),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
        LinuxDistro::Fedora => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: format!(
                "Precompiled scx-scheds-{}-1.x86_64.rpm (Mithrandir bundle / GitHub Release + COPR/Cargo fallback)",
                SCX_BUNDLE_VERSION
            ),
            command: format!(
                "if [ -n \"$MITHRANDIR_SCX_BUNDLE\" ] && [ -f \"$MITHRANDIR_SCX_BUNDLE\" ]; then \
                     echo '==> Extracting bundled precompiled sched-ext v{ver} binaries...'; \
                     tar -C / -xJf \"$MITHRANDIR_SCX_BUNDLE\" && systemctl daemon-reload 2>/dev/null || true; \
                 elif curl -fsSL \"{rpm_url}\" -o /tmp/scx-scheds.rpm; then \
                     echo '==> Installing scx-scheds-{ver}-1.x86_64.rpm from Mithrandir GitHub Release...'; \
                     rpm -Uvh --replacepkgs /tmp/scx-scheds.rpm || dnf install -y /tmp/scx-scheds.rpm; \
                     rm -f /tmp/scx-scheds.rpm; \
                 else \
                     dnf install -y scx-scheds scx-tools || (dnf copr enable -y bieszczaders/kernel-cachyos-addons && dnf install -y scx-scheds scx-tools) || ({cargo_fb}); \
                 fi",
                ver = SCX_BUNDLE_VERSION,
                rpm_url = rpm_url,
                cargo_fb = cargo_source_fallback
            ),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
        LinuxDistro::Arch | LinuxDistro::CachyOS => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: format!(
                "Pacman scx-scheds & scx-tools (with Mithrandir v{} bundle fallback)",
                SCX_BUNDLE_VERSION
            ),
            command: format!(
                "pacman -Sy --noconfirm --needed scx-scheds scx-tools || (\
                 if [ -n \"$MITHRANDIR_SCX_BUNDLE\" ] && [ -f \"$MITHRANDIR_SCX_BUNDLE\" ]; then \
                     tar -C / -xJf \"$MITHRANDIR_SCX_BUNDLE\"; \
                 elif curl -fsSL \"{tar_url}\" -o /tmp/scx-scheds.tar.xz; then \
                     tar -C / -xJf /tmp/scx-scheds.tar.xz && rm -f /tmp/scx-scheds.tar.xz; \
                 else \
                     {cargo_fb}; \
                 fi)",
                tar_url = tar_url,
                cargo_fb = cargo_source_fallback
            ),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
        LinuxDistro::OpenSuse => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: format!(
                "Zypper / Precompiled scx-scheds-{}-1.x86_64.rpm",
                SCX_BUNDLE_VERSION
            ),
            command: format!(
                "if [ -n \"$MITHRANDIR_SCX_BUNDLE\" ] && [ -f \"$MITHRANDIR_SCX_BUNDLE\" ]; then \
                     tar -C / -xJf \"$MITHRANDIR_SCX_BUNDLE\" && systemctl daemon-reload 2>/dev/null || true; \
                 elif curl -fsSL \"{rpm_url}\" -o /tmp/scx-scheds.rpm; then \
                     rpm -Uvh --replacepkgs /tmp/scx-scheds.rpm || zypper --non-interactive install /tmp/scx-scheds.rpm; \
                     rm -f /tmp/scx-scheds.rpm; \
                 else \
                     zypper --non-interactive install scx || ({cargo_fb}); \
                 fi",
                rpm_url = rpm_url,
                cargo_fb = cargo_source_fallback
            ),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
        _ => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: format!(
                "Precompiled scx-scheds-{}-x86_64.tar.xz + Cargo source build fallback",
                SCX_BUNDLE_VERSION
            ),
            command: format!(
                "if [ -n \"$MITHRANDIR_SCX_BUNDLE\" ] && [ -f \"$MITHRANDIR_SCX_BUNDLE\" ]; then \
                     tar -C / -xJf \"$MITHRANDIR_SCX_BUNDLE\" && systemctl daemon-reload 2>/dev/null || true; \
                 elif curl -fsSL \"{tar_url}\" -o /tmp/scx-scheds.tar.xz; then \
                     tar -C / -xJf /tmp/scx-scheds.tar.xz && rm -f /tmp/scx-scheds.tar.xz; \
                 else \
                     {cargo_fb}; \
                 fi",
                tar_url = tar_url,
                cargo_fb = cargo_source_fallback
            ),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
    }
}

/// Installs `sched-ext` (`scx` v1.1.4) packages for the current distribution, staging any embedded bundle for host access.
pub fn install_scx_streaming<F>(mut on_line: F) -> Result<ScxInstallPlan, String>
where
    F: FnMut(String),
{
    let plan = get_install_plan();
    on_line(format!(
        "📦 Installing sched-ext v{} for {} ({})",
        SCX_BUNDLE_VERSION, plan.distro_name, plan.summary
    ));

    // If a bundled archive exists (including inside Flatpak /app/share/mithrandir/scx or AppImage $APPDIR),
    // stage a copy into the user's XDG cache directory so the host root process can read it directly.
    let mut staged_bundle_path: Option<PathBuf> = None;
    if let Some(bundle) = find_bundled_scx_archive() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let cache_dir = Path::new(&home).join(".cache");
        let _ = fs::create_dir_all(&cache_dir);
        let staged = cache_dir.join(format!("mithrandir-scx-{}.tar.xz", SCX_BUNDLE_VERSION));
        if fs::copy(&bundle, &staged).is_ok() {
            on_line(format!(
                "✓ Found embedded sched-ext v{} binary bundle ({})",
                SCX_BUNDLE_VERSION,
                bundle.display()
            ));
            staged_bundle_path = Some(staged);
        } else {
            staged_bundle_path = Some(bundle);
        }
    }

    let full_cmd = if let Some(ref staged) = staged_bundle_path {
        format!(
            "export MITHRANDIR_SCX_BUNDLE=\"{}\"; {}",
            staged.display(),
            plan.command
        )
    } else {
        plan.command.clone()
    };

    let code = HostBridge::spawn_streaming("sh", &["-c", &full_cmd], true, &mut on_line)?;

    if let Some(ref staged) = staged_bundle_path {
        if staged.starts_with(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())) {
            let _ = fs::remove_file(staged);
        }
    }

    if code == 0 {
        on_line("✅ sched-ext (scx) schedulers installed into /usr/bin!".to_string());
        Ok(plan)
    } else {
        Err(format!(
            "sched-ext installation exited with status code {}",
            code
        ))
    }
}
