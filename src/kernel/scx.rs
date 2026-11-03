use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use zbus::blocking::Connection;

use crate::backend::host_bridge::HostBridge;
use crate::os_info::{detect_os, LinuxDistro};

pub const DEFAULT_SCX_STATE_PATH: &str = "/sys/kernel/sched_ext/state";
pub const DEFAULT_SCX_OPS_PATH: &str = "/sys/kernel/sched_ext/root/ops";

pub const SCX_DBUS_DEST: &str = "org.scx.Loader";
pub const SCX_DBUS_PATH: &str = "/org/scx/Loader";
pub const SCX_DBUS_INTERFACE: &str = "org.scx.Loader";

/// Complete list of official sched-ext (`scx`) userspace/BPF schedulers from https://github.com/sched-ext/scx
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
    "scx_simple",
    "scx_central",
    "scx_nest",
    "scx_qmap",
    "scx_rlfifo",
    "scx_tickless",
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
        "scx_simple" => "Minimal vtime/FIFO reference BPF scheduler from upstream sched-ext",
        "scx_central" => "Central dispatcher tickless scheduler minimizing timer interrupt overhead",
        "scx_nest" => "Core-nesting scheduler keeping active cores warm for frequency boost gains",
        "scx_qmap" => "Queued BPF map reference scheduler with multi-level priority queues",
        "scx_rlfifo" => "Simple Rust userland FIFO scheduler built on the scx_rlfifo framework",
        "scx_tickless" => "Tickless scheduler routing timer ticks to a dedicated housekeeping CPU pool",
        "scx_flatcg" => "Flattened cgroup hierarchy BPF scheduler for high-density container workloads",
        "scx_userland" => "Minimal userspace C ordered-list reference scheduler",
        _ => "Pluggable sched-ext (SCX) BPF kernel scheduler",
    }
}

/// Normalizes a raw scheduler name from `/sys/kernel/sched_ext/root/ops` or D-Bus
/// (e.g., `"bpfland"` or `"bpfland_1.0.8_g..."`) into its canonical binary name (`"scx_bpfland"`).
pub fn normalize_scheduler_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") || trimmed.eq_ignore_ascii_case("unknown") {
        return "none".to_string();
    }

    // Direct match against known schedulers
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

/// Queries the active scheduler via `org.scx.Loader` D-Bus, falling back to normalized sysfs `ops`.
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

    // If sysfs state is explicitly disabled, no SCX scheduler is active
    if let Some(state) = get_sysfs_scheduler_state() {
        if state == "disabled" {
            return "none".to_string();
        }
    }

    normalize_scheduler_name(&get_sysfs_active_scheduler())
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

    // Also discover all available scx_* binaries on the host (including /usr/sbin for Ubuntu/Debian)
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

    // 2. Try scxctl command line via HostBridge (handles both modern `--sched` and positional syntax)
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
        let mode_num = mode.to_string();
        if let Ok((0, _, _)) =
            HostBridge::run_command("scxctl", &["switch", &normalized, "--mode", &mode_num])
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
        "Scheduler '{}' is not installed on this system. Use 'Install / Update SCX' to install sched-ext.",
        normalized
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

    // 2. Fallback to scxctl command line via HostBridge
    if which_binary("scxctl") {
        if let Ok((0, _, _)) = HostBridge::run_command("scxctl", &["stop"]) {
            return Ok(());
        }
    }

    // If already stopped, return Ok immediately
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
        "rm -f /etc/scx_loader/config.toml 2>/dev/null || true; \
         systemctl disable scx_loader.service scx.service 2>/dev/null || true"
            .to_string()
    } else {
        format!(
            "mkdir -p /etc/scx_loader /etc/default && \
             printf '# Generated by Mithrandir Kernel & Power Manager\\ndefault_sched = \"{sched}\"\\ndefault_mode = {mode_str}\\n' > /etc/scx_loader/config.toml && \
             printf '# Generated by Mithrandir Kernel & Power Manager\\nSCX_SCHEDULER={sched}\\nSCX_FLAGS=\"{flags}\"\\n' > /etc/default/scx && \
             (systemctl enable scx_loader.service 2>/dev/null || systemctl enable scx.service 2>/dev/null || true)",
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

#[derive(Debug, Clone)]
pub struct ScxInstallPlan {
    pub distro_name: String,
    pub summary: String,
    pub command: String,
    pub requires_reboot_for_kernel: bool,
}

/// Generates a distro-aware installation plan for `sched-ext` (`scx`) based on https://github.com/sched-ext/scx/blob/main/INSTALL.md
pub fn get_install_plan() -> ScxInstallPlan {
    let os = detect_os();
    get_install_plan_for_distro(os.distro, &os.pretty_name, is_scx_supported())
}

pub fn get_install_plan_for_distro(
    distro: LinuxDistro,
    pretty_name: &str,
    scx_kernel_supported: bool,
) -> ScxInstallPlan {
    match distro {
        LinuxDistro::Ubuntu => {
            let pkg_list = if scx_kernel_supported {
                "scx"
            } else {
                "scx linux-generic-wip"
            };
            ScxInstallPlan {
                distro_name: pretty_name.to_string(),
                summary: format!(
                    "Ubuntu PPA (ppa:arighi/sched-ext) — installs {}",
                    pkg_list
                ),
                command: format!(
                    "export DEBIAN_FRONTEND=noninteractive; \
                     apt-get update && \
                     apt-get install -y software-properties-common && \
                     add-apt-repository -y --enable-source ppa:arighi/sched-ext && \
                     apt-get update && \
                     (apt-get install -y {} || apt-get install -y scx)",
                    pkg_list
                ),
                requires_reboot_for_kernel: !scx_kernel_supported,
            }
        }
        LinuxDistro::Debian => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: "APT scx package (with automated upstream github.com/sched-ext/scx build fallback)".to_string(),
            command: "export DEBIAN_FRONTEND=noninteractive; \
                      apt-get update && \
                      (apt-get install -y scx || (\
                          apt-get install -y git build-essential meson ninja-build pkg-config clang llvm libbpf-dev libelf-dev libzstd-dev zlib1g-dev protobuf-compiler libseccomp-dev cargo rustc && \
                          rm -rf /tmp/mithrandir-scx-build && \
                          git clone --depth 1 https://github.com/sched-ext/scx.git /tmp/mithrandir-scx-build && \
                          cd /tmp/mithrandir-scx-build && \
                          meson setup build --prefix=/usr && \
                          ninja -C build && \
                          ninja -C build install && \
                          rm -rf /tmp/mithrandir-scx-build\
                      ))"
                .to_string(),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
        LinuxDistro::Fedora => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: "DNF scx-scheds & scx-tools (with CachyOS COPR fallback)".to_string(),
            command: "dnf install -y scx-scheds scx-tools || (\
                      dnf copr enable -y bieszczaders/kernel-cachyos-addons && \
                      dnf install -y scx-scheds scx-tools\
                      )"
                .to_string(),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
        LinuxDistro::Arch | LinuxDistro::CachyOS => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: "Pacman scx-scheds & scx-tools".to_string(),
            command: "pacman -Sy --noconfirm --needed scx-scheds scx-tools".to_string(),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
        LinuxDistro::OpenSuse => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: "Zypper scx package".to_string(),
            command: "zypper --non-interactive install scx".to_string(),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
        _ => ScxInstallPlan {
            distro_name: pretty_name.to_string(),
            summary: "Source build & install from https://github.com/sched-ext/scx".to_string(),
            command: "rm -rf /tmp/mithrandir-scx-build && \
                      git clone --depth 1 https://github.com/sched-ext/scx.git /tmp/mithrandir-scx-build && \
                      cd /tmp/mithrandir-scx-build && \
                      meson setup build --prefix=/usr && \
                      ninja -C build && \
                      ninja -C build install && \
                      rm -rf /tmp/mithrandir-scx-build"
                .to_string(),
            requires_reboot_for_kernel: !scx_kernel_supported,
        },
    }
}

/// Installs `sched-ext` (`scx`) packages for the current distribution, streaming log lines to `on_line`.
pub fn install_scx_streaming<F>(mut on_line: F) -> Result<ScxInstallPlan, String>
where
    F: FnMut(String),
{
    let plan = get_install_plan();
    on_line(format!("📦 Installing sched-ext for {} ({})", plan.distro_name, plan.summary));
    let code = HostBridge::spawn_streaming("sh", &["-c", &plan.command], true, &mut on_line)?;
    if code == 0 {
        Ok(plan)
    } else {
        Err(format!("sched-ext installation exited with status code {}", code))
    }
}
