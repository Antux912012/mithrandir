use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::backend::distro::{DistroDetector, DistroFamily};
use crate::backend::host_bridge::HostBridge;
use crate::os_info::is_immutable_system;

pub const MITHRANDIR_DROPIN_FILENAME: &str = "99-mithrandir.cfg";

pub const GRUB_CFG_CANDIDATES: &[&str] = &[
    "/boot/grub/grub.cfg",
    "/boot/grub2/grub.cfg",
    "/boot/efi/EFI/fedora/grub.cfg",
    "/boot/efi/EFI/redhat/grub.cfg",
    "/boot/efi/EFI/centos/grub.cfg",
    "/boot/efi/EFI/almalinux/grub.cfg",
    "/boot/efi/EFI/rocky/grub.cfg",
    "/boot/efi/EFI/opensuse/grub.cfg",
    "/boot/efi/EFI/debian/grub.cfg",
    "/boot/efi/EFI/ubuntu/grub.cfg",
    "/boot/efi/EFI/arch/grub.cfg",
    "/etc/grub2.cfg",
    "/etc/grub2-efi.cfg",
    "/run/host/boot/grub/grub.cfg",
    "/run/host/boot/grub2/grub.cfg",
    "/run/host/boot/efi/EFI/fedora/grub.cfg",
    "/run/host/boot/efi/EFI/debian/grub.cfg",
    "/run/host/boot/efi/EFI/ubuntu/grub.cfg",
];

pub const GRUB_DEFAULT_CANDIDATES: &[&str] = &[
    "/etc/default/grub",
    "/etc/sysconfig/grub",
    "/run/host/etc/default/grub",
    "/run/host/etc/sysconfig/grub",
];

pub const GRUB_DEFAULT_D_CANDIDATES: &[&str] = &[
    "/etc/default/grub.d",
    "/run/host/etc/default/grub.d",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrubStrategy {
    /// Writes an isolated `/etc/default/grub.d/99-mithrandir.cfg` drop-in (Debian/Ubuntu/derivatives).
    DropIn,
    /// Uses `grubby` for BLS kernel entries/cmdline + `/etc/default/grub` for timeout (Fedora/RHEL/OpenMandriva BLS).
    FedoraBlsGrubby,
    /// Hardened atomic update of `/etc/default/grub` with `.bak` snapshot (Arch, CachyOS, Gentoo, openSUSE, Mandriva).
    DirectDefaultGrub,
    /// Immutable OS (NixOS/OSTree) or non-GRUB bootloader (e.g. systemd-boot).
    UnsupportedBootloader,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrubStrategyPlan {
    pub strategy: GrubStrategy,
    pub family: DistroFamily,
    pub mkconfig_cmd: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrubLiveStatus {
    pub cfg_path: String,
    pub cfg_exists: bool,
    pub cfg_size_bytes: u64,
    pub cfg_modified_time: String,
    pub backup_path: String,
    pub backup_exists: bool,
    pub backup_size_bytes: u64,
    pub backup_modified_time: String,

    pub default_path: String,
    pub default_exists: bool,
    pub default_size_bytes: u64,
    pub default_modified_time: String,
    pub default_backup_path: String,
    pub default_backup_exists: bool,
    pub default_backup_size_bytes: u64,
    pub default_backup_modified_time: String,
    pub default_dropins_dir: Option<String>,
    pub default_dropins_count: usize,
    #[serde(default)]
    pub strategy_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrubConfig {
    pub timeout: i32,
    pub default_entry: String,
    pub cmdline: String,
    pub live_status: GrubLiveStatus,
    pub menu_entries: Vec<String>,
}

pub struct GrubManager;

impl GrubManager {
    /// Detects the active grub.cfg path on the host system.
    pub fn detect_grub_cfg_path() -> PathBuf {
        for candidate in GRUB_CFG_CANDIDATES {
            let p = Path::new(candidate);
            if p.exists() {
                return p.to_path_buf();
            }
        }
        PathBuf::from("/boot/grub/grub.cfg")
    }

    /// Detects the active /etc/default/grub path.
    pub fn detect_default_grub_path() -> PathBuf {
        for candidate in GRUB_DEFAULT_CANDIDATES {
            let p = Path::new(candidate);
            if p.exists() {
                return p.to_path_buf();
            }
        }
        PathBuf::from("/etc/default/grub")
    }

    /// Detects if an /etc/default/grub.d directory exists on the system.
    pub fn detect_default_grub_d_dir() -> Option<PathBuf> {
        for candidate in GRUB_DEFAULT_D_CANDIDATES {
            let p = Path::new(candidate);
            if p.is_dir() {
                return Some(p.to_path_buf());
            }
        }
        None
    }

    /// Counts valid .cfg drop-in snippets in the drop-ins directory.
    pub fn count_default_dropins(dir: &Path) -> usize {
        if let Ok(entries) = fs::read_dir(dir) {
            entries
                .flatten()
                .filter(|e| {
                    if let Ok(ft) = e.file_type() {
                        ft.is_file() && e.file_name().to_string_lossy().ends_with(".cfg")
                    } else {
                        false
                    }
                })
                .count()
        } else {
            0
        }
    }

    /// Derives the backup file path for a given grub.cfg or /etc/default/grub.
    pub fn get_backup_path(cfg_path: &Path) -> PathBuf {
        PathBuf::from(format!("{}.bak", cfg_path.display()))
    }

    /// Gets size, human-readable modification timestamp, and presence.
    pub fn get_file_metadata(path: &Path) -> (bool, u64, String) {
        if let Ok(meta) = fs::metadata(path) {
            let size = meta.len();
            let modified = meta.modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|dur| format_unix_timestamp(dur.as_secs() as i64))
                .unwrap_or_else(|| "Unknown".to_string());
            (true, size, modified)
        } else {
            (false, 0, "Not found".to_string())
        }
    }

    /// Returns the live real-time status of both grub.cfg and /etc/default/grub.
    pub fn get_live_status() -> GrubLiveStatus {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);

        let default_path = Self::detect_default_grub_path();
        let default_backup_path = Self::get_backup_path(&default_path);

        let (cfg_exists, cfg_size_bytes, cfg_modified_time) = Self::get_file_metadata(&cfg_path);
        let (backup_exists, backup_size_bytes, backup_modified_time) = Self::get_file_metadata(&backup_path);

        let (default_exists, default_size_bytes, default_modified_time) = Self::get_file_metadata(&default_path);
        let (default_backup_exists, default_backup_size_bytes, default_backup_modified_time) =
            Self::get_file_metadata(&default_backup_path);

        let dropins_dir = Self::detect_default_grub_d_dir();
        let default_dropins_count = dropins_dir.as_ref().map(|d| Self::count_default_dropins(d)).unwrap_or(0);
        let default_dropins_dir = dropins_dir.map(|d| d.to_string_lossy().to_string());
        let strategy_label = Self::detect_strategy_plan().label;

        GrubLiveStatus {
            cfg_path: cfg_path.to_string_lossy().to_string(),
            cfg_exists,
            cfg_size_bytes,
            cfg_modified_time,
            backup_path: backup_path.to_string_lossy().to_string(),
            backup_exists,
            backup_size_bytes,
            backup_modified_time,

            default_path: default_path.to_string_lossy().to_string(),
            default_exists,
            default_size_bytes,
            default_modified_time,
            default_backup_path: default_backup_path.to_string_lossy().to_string(),
            default_backup_exists,
            default_backup_size_bytes,
            default_backup_modified_time,
            default_dropins_dir,
            default_dropins_count,
            strategy_label,
        }
    }

    /// Parses key GRUB parameters from configuration file content.
    pub fn parse_default_grub_vars(
        content: &str,
        timeout: &mut i32,
        default_entry: &mut String,
        cmdline: &mut String,
    ) {
        for line in content.lines() {
            let l = line.trim();
            if let Some(val) = l.strip_prefix("GRUB_TIMEOUT=") {
                let v = val.trim_matches('"').trim_matches('\'');
                if let Ok(t) = v.parse::<i32>() {
                    *timeout = t;
                }
            } else if let Some(val) = l.strip_prefix("GRUB_DEFAULT=") {
                *default_entry = val.trim_matches('"').trim_matches('\'').to_string();
            } else if let Some(val) = l.strip_prefix("GRUB_CMDLINE_LINUX_DEFAULT=") {
                *cmdline = val.trim_matches('"').trim_matches('\'').to_string();
            }
        }
    }

    /// Reads the complete GRUB configuration and available boot entries.
    pub fn read_config() -> GrubConfig {
        let live_status = Self::get_live_status();
        let default_path = Self::detect_default_grub_path();

        let mut timeout = 5;
        let mut default_entry = "0".to_string();
        let mut cmdline = "quiet splash".to_string();

        if let Ok(content) = fs::read_to_string(&default_path) {
            Self::parse_default_grub_vars(&content, &mut timeout, &mut default_entry, &mut cmdline);
        }

        // Parse any /etc/default/grub.d/*.cfg drop-in overrides if present
        if let Some(dropins_path_str) = &live_status.default_dropins_dir {
            let dropins_dir = Path::new(dropins_path_str);
            if let Ok(entries) = fs::read_dir(dropins_dir) {
                let mut dropin_files = Vec::new();
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("cfg") {
                        dropin_files.push(path);
                    }
                }
                dropin_files.sort();
                for file in dropin_files {
                    if let Ok(content) = fs::read_to_string(&file) {
                        Self::parse_default_grub_vars(&content, &mut timeout, &mut default_entry, &mut cmdline);
                    }
                }
            }
        }

        // Parse menu entries from grub.cfg, grub.cfg.bak, or /boot/vmlinuz
        let mut menu_entries = Vec::new();
        let cfg_path = Path::new(&live_status.cfg_path);
        let backup_path = Path::new(&live_status.backup_path);

        if let Ok(content) = fs::read_to_string(cfg_path) {
            menu_entries = Self::parse_menu_entries(&content);
        } else if let Ok(content) = fs::read_to_string(backup_path) {
            menu_entries = Self::parse_menu_entries(&content);
        }

        if menu_entries.is_empty() {
            menu_entries = Self::detect_installed_kernels();
        }

        if menu_entries.is_empty() {
            menu_entries.push("Default Kernel (0)".to_string());
        }

        GrubConfig {
            timeout,
            default_entry,
            cmdline,
            live_status,
            menu_entries,
        }
    }

    /// Parses menuentry titles from a grub.cfg content string.
    pub fn parse_menu_entries(content: &str) -> Vec<String> {
        let mut entries = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("menuentry ") {
                if let Some(start) = trimmed.find(|c| c == '\'' || c == '"') {
                    let quote_char = trimmed.as_bytes()[start] as char;
                    if let Some(end) = trimmed[start + 1..].find(quote_char) {
                        let title = &trimmed[start + 1..start + 1 + end];
                        if !title.is_empty() && !entries.contains(&title.to_string()) {
                            entries.push(title.to_string());
                        }
                    }
                }
            }
        }
        entries
    }

    /// Detects installed kernel images in /boot as fallback boot entries.
    pub fn detect_installed_kernels() -> Vec<String> {
        let mut kernels = Vec::new();
        let boot_candidates = ["/boot", "/run/host/boot"];
        for b in boot_candidates {
            let p = Path::new(b);
            if let Ok(dir_entries) = fs::read_dir(p) {
                for entry in dir_entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.starts_with("vmlinuz-") {
                        let kversion = fname.trim_start_matches("vmlinuz-");
                        let title = format!("Linux {}", kversion);
                        if !kernels.contains(&title) {
                            kernels.push(title);
                        }
                    }
                }
            }
        }
        kernels.sort();
        kernels
    }

    /// Checks if BootLoaderSpec (BLS) is enabled via /etc/default/grub or /boot/loader/entries.
    pub fn is_bls_enabled(default_grub_content: &str) -> bool {
        let bls_flag = default_grub_content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == "GRUB_ENABLE_BLSCFG=true"
                || trimmed == "GRUB_ENABLE_BLSCFG=\"true\""
                || trimmed == "GRUB_ENABLE_BLSCFG='true'"
        });
        bls_flag
            || Path::new("/boot/loader/entries").is_dir()
            || Path::new("/run/host/boot/loader/entries").is_dir()
    }

    /// Checks whether systemd-boot appears to be the active bootloader without GRUB.
    pub fn detect_systemd_boot_without_grub(has_grub_cfg: bool) -> bool {
        !has_grub_cfg
            && (Path::new("/boot/efi/loader/entries").is_dir()
                || Path::new("/boot/loader/entries").is_dir()
                || Path::new("/run/host/boot/efi/loader/entries").is_dir()
                || Path::new("/run/host/boot/loader/entries").is_dir())
    }

    /// Detects the preferred GRUB config generator binary on the host.
    pub fn detect_mkconfig_cmd(family: DistroFamily) -> Option<String> {
        let candidates: &[&str] = match family {
            DistroFamily::Debian => &["update-grub", "grub-mkconfig", "grub2-mkconfig"],
            DistroFamily::Fedora | DistroFamily::Suse | DistroFamily::Mandriva => {
                &["grub2-mkconfig", "grub-mkconfig", "update-grub"]
            }
            DistroFamily::Arch | DistroFamily::Gentoo | DistroFamily::Alpine | DistroFamily::Unknown => {
                &["grub-mkconfig", "grub2-mkconfig", "update-grub"]
            }
        };

        for bin in candidates {
            if HostBridge::which(bin).is_some() {
                return Some((*bin).to_string());
            }
        }
        None
    }

    /// Pure strategy selector combining distro family with runtime bootloader capabilities.
    pub fn select_strategy_for(
        family: DistroFamily,
        immutable: bool,
        has_dropin_dir: bool,
        has_grubby: bool,
        bls_enabled: bool,
        has_grub_cfg: bool,
        has_systemd_boot: bool,
        mkconfig_cmd: Option<String>,
    ) -> GrubStrategyPlan {
        if immutable {
            return GrubStrategyPlan {
                strategy: GrubStrategy::UnsupportedBootloader,
                family,
                mkconfig_cmd,
                label: "Immutable OS detected (declarative bootloader config required)".to_string(),
            };
        }

        if has_systemd_boot && !has_grub_cfg {
            return GrubStrategyPlan {
                strategy: GrubStrategy::UnsupportedBootloader,
                family,
                mkconfig_cmd,
                label: "systemd-boot detected (GRUB2 config inactive)".to_string(),
            };
        }

        if matches!(family, DistroFamily::Fedora | DistroFamily::Mandriva) && has_grubby && bls_enabled {
            return GrubStrategyPlan {
                strategy: GrubStrategy::FedoraBlsGrubby,
                family,
                mkconfig_cmd,
                label: format!("{:?} BLS + grubby + grub2-mkconfig", family),
            };
        }

        if has_dropin_dir && matches!(family, DistroFamily::Debian | DistroFamily::Unknown) {
            return GrubStrategyPlan {
                strategy: GrubStrategy::DropIn,
                family,
                mkconfig_cmd,
                label: format!("Drop-in (/etc/default/grub.d/{})", MITHRANDIR_DROPIN_FILENAME),
            };
        }

        GrubStrategyPlan {
            strategy: GrubStrategy::DirectDefaultGrub,
            family,
            mkconfig_cmd,
            label: format!("{:?} hardened /etc/default/grub + .bak snapshot", family),
        }
    }

    /// Probes the active host OS and bootloader capabilities to choose the safest GRUB strategy.
    pub fn detect_strategy_plan() -> GrubStrategyPlan {
        let distro = DistroDetector::detect();
        let default_path = Self::detect_default_grub_path();
        let default_content = fs::read_to_string(&default_path).unwrap_or_default();
        let cfg_exists = Self::detect_grub_cfg_path().exists();
        let has_dropin_dir = Self::detect_default_grub_d_dir().is_some();
        let has_grubby = HostBridge::which("grubby").is_some();
        let bls_enabled = Self::is_bls_enabled(&default_content);
        let has_systemd_boot = Self::detect_systemd_boot_without_grub(cfg_exists);
        let mkconfig_cmd = Self::detect_mkconfig_cmd(distro.family);

        Self::select_strategy_for(
            distro.family,
            is_immutable_system(),
            has_dropin_dir,
            has_grubby,
            bls_enabled,
            cfg_exists,
            has_systemd_boot,
            mkconfig_cmd,
        )
    }

    /// Validates GRUB parameter inputs before writing shell-sourced configuration files.
    pub fn validate_grub_inputs(
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
    ) -> Result<(), String> {
        if !(-1..=3600).contains(&timeout) {
            return Err(format!(
                "Invalid GRUB_TIMEOUT {}: must be between -1 and 3600 seconds",
                timeout
            ));
        }

        for (field, val) in [("GRUB_DEFAULT", default_entry)]
            .into_iter()
            .chain(cmdline.map(|c| ("GRUB_CMDLINE_LINUX_DEFAULT", c)))
        {
            if val.contains('\n')
                || val.contains('\r')
                || val.contains('\0')
                || val.contains('"')
                || val.contains('`')
                || val.contains("$(")
            {
                return Err(format!(
                    "{} contains disallowed shell or control characters (newlines, quotes, backticks, or $())",
                    field
                ));
            }
        }

        Ok(())
    }

    /// Builds an isolated `/etc/default/grub.d/99-mithrandir.cfg` drop-in file.
    pub fn build_dropin_content(
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
    ) -> String {
        let mut lines = vec![
            "# Managed by Mithrandir — delete this file and run update-grub to revert.".to_string(),
            format!("GRUB_TIMEOUT={}", timeout),
            format!("GRUB_DEFAULT=\"{}\"", default_entry),
        ];
        if let Some(cmd) = cmdline {
            lines.push(format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"", cmd));
        }
        let mut out = lines.join("\n");
        out.push('\n');
        out
    }

    fn write_host_file_privileged(host_path: &str, content: &str) -> Result<(), String> {
        let (code, stdout, stderr) = HostBridge::run_privileged_with_stdin(&["tee", host_path], content)?;
        if code != 0 {
            return Err(format!("Failed writing {} (code {}): {} {}", host_path, code, stdout, stderr).trim().to_string());
        }
        let (chmod_code, _, chmod_err) = HostBridge::run_privileged(&["chmod", "644", host_path])?;
        if chmod_code != 0 {
            return Err(format!("Failed setting permissions on {}: {}", host_path, chmod_err.trim()));
        }
        Ok(())
    }

    fn backup_host_file_if_missing(src_exists: bool, host_src: &str, host_bak: &str) -> Result<(), String> {
        if !src_exists || Path::new(host_bak).exists() {
            return Ok(());
        }
        let (code, stdout, stderr) = HostBridge::run_privileged(&["cp", "-p", host_src, host_bak])?;
        if code != 0 {
            return Err(format!("Failed creating backup {} (code {}): {} {}", host_bak, code, stdout, stderr).trim().to_string());
        }
        let _ = HostBridge::run_privileged(&["chmod", "644", host_bak]);
        Ok(())
    }

    fn regenerate_grub_cfg(mkconfig_cmd: Option<&str>, host_cfg: &str) -> Result<(), String> {
        let cmd = mkconfig_cmd.ok_or_else(|| {
            "No GRUB generator (update-grub, grub2-mkconfig, or grub-mkconfig) found on host".to_string()
        })?;

        let (code, stdout, stderr) = match cmd {
            "update-grub" => HostBridge::run_privileged(&["update-grub"])?,
            "grub2-mkconfig" => HostBridge::run_privileged(&["grub2-mkconfig", "-o", host_cfg])?,
            "grub-mkconfig" => HostBridge::run_privileged(&["grub-mkconfig", "-o", host_cfg])?,
            other => HostBridge::run_privileged(&[other, "-o", host_cfg])?,
        };

        if code == 0 {
            Ok(())
        } else {
            Err(format!("{} exited with code {}: {} {}", cmd, code, stdout, stderr).trim().to_string())
        }
    }

    /// Generates a backup of the active grub.cfg (grub.cfg.bak) and /etc/default/grub (/etc/default/grub.bak).
    pub fn create_backup() -> Result<String, String> {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();
        let default_bak = Self::get_backup_path(&default_path);

        let host_cfg = HostBridge::host_path(&cfg_path);
        let host_bak = HostBridge::host_path(&backup_path);
        let host_def = HostBridge::host_path(&default_path);
        let host_def_bak = HostBridge::host_path(&default_bak);

        let mut backed_up = Vec::new();
        if cfg_path.exists() {
            let (code, stdout, stderr) = HostBridge::run_privileged(&["cp", "-p", &host_cfg, &host_bak])?;
            if code != 0 {
                return Err(format!("Backup of {} failed (code {}): {} {}", host_cfg, code, stdout, stderr).trim().to_string());
            }
            let _ = HostBridge::run_privileged(&["chmod", "644", &host_bak]);
            backed_up.push(host_bak);
        }
        if default_path.exists() {
            let (code, stdout, stderr) = HostBridge::run_privileged(&["cp", "-p", &host_def, &host_def_bak])?;
            if code != 0 {
                return Err(format!("Backup of {} failed (code {}): {} {}", host_def, code, stdout, stderr).trim().to_string());
            }
            let _ = HostBridge::run_privileged(&["chmod", "644", &host_def_bak]);
            backed_up.push(host_def_bak);
        }

        if backed_up.is_empty() {
            Err(format!("No active GRUB files found at {} or {}", host_cfg, host_def))
        } else {
            Ok(format!("Backup successfully generated: {}", backed_up.join(" and ")))
        }
    }

    /// Recovers the original configuration by removing Mithrandir's drop-in and/or restoring `.bak` files.
    pub fn restore_backup() -> Result<String, String> {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();
        let default_bak = Self::get_backup_path(&default_path);
        let dropin_path = Self::detect_default_grub_d_dir().map(|d| d.join(MITHRANDIR_DROPIN_FILENAME));

        let dropin_exists = dropin_path.as_ref().is_some_and(|p| p.exists());
        if !backup_path.exists() && !default_bak.exists() && !dropin_exists {
            return Err(format!(
                "No backup files or Mithrandir drop-in found (checked {} and {})",
                backup_path.display(),
                default_bak.display()
            ));
        }

        let host_cfg = HostBridge::host_path(&cfg_path);
        let host_bak = HostBridge::host_path(&backup_path);
        let host_def = HostBridge::host_path(&default_path);
        let host_def_bak = HostBridge::host_path(&default_bak);

        let mut restored = Vec::new();

        if let Some(ref dp) = dropin_path {
            if dp.exists() {
                let host_dp = HostBridge::host_path(dp);
                let (code, stdout, stderr) = HostBridge::run_privileged(&["rm", "-f", &host_dp])?;
                if code != 0 {
                    return Err(format!("Failed removing drop-in {} (code {}): {} {}", host_dp, code, stdout, stderr).trim().to_string());
                }
                restored.push(format!("removed {}", host_dp));
            }
        }

        if backup_path.exists() {
            let (code, stdout, stderr) = HostBridge::run_privileged(&["cp", "-p", &host_bak, &host_cfg])?;
            if code != 0 {
                return Err(format!("Restore of {} failed (code {}): {} {}", host_cfg, code, stdout, stderr).trim().to_string());
            }
            restored.push(host_bak);
        }

        if default_bak.exists() {
            let (code, stdout, stderr) = HostBridge::run_privileged(&["cp", "-p", &host_def_bak, &host_def])?;
            if code != 0 {
                return Err(format!("Restore of {} failed (code {}): {} {}", host_def, code, stdout, stderr).trim().to_string());
            }
            restored.push(host_def_bak);
        }

        let plan = Self::detect_strategy_plan();
        if plan.mkconfig_cmd.is_some() {
            Self::regenerate_grub_cfg(plan.mkconfig_cmd.as_deref(), &host_cfg)?;
        }

        Ok(format!("Configuration successfully restored ({})", restored.join(", ")))
    }

    /// Modifies GRUB using the safest distro-specific strategy and regenerates `grub.cfg`,
    /// ensuring backups are preserved before any edit.
    pub fn save_and_update(timeout: i32, default_entry: &str, cmdline: Option<&str>) -> Result<(), String> {
        Self::validate_grub_inputs(timeout, default_entry, cmdline)?;

        let plan = Self::detect_strategy_plan();
        if plan.strategy == GrubStrategy::UnsupportedBootloader {
            return Err(format!(
                "GRUB updates are disabled on this system: {}",
                plan.label
            ));
        }

        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();
        let default_bak = Self::get_backup_path(&default_path);

        let host_cfg = HostBridge::host_path(&cfg_path);
        let host_bak = HostBridge::host_path(&backup_path);
        let host_def = HostBridge::host_path(&default_path);
        let host_def_bak = HostBridge::host_path(&default_bak);

        Self::backup_host_file_if_missing(cfg_path.exists(), &host_cfg, &host_bak)?;

        match plan.strategy {
            GrubStrategy::DropIn => {
                let dropins_dir = Self::detect_default_grub_d_dir()
                    .ok_or_else(|| "/etc/default/grub.d not found".to_string())?;
                let host_dropin = HostBridge::host_path(&dropins_dir.join(MITHRANDIR_DROPIN_FILENAME));
                let content = Self::build_dropin_content(timeout, default_entry, cmdline);
                Self::write_host_file_privileged(&host_dropin, &content)?;
            }
            GrubStrategy::FedoraBlsGrubby => {
                Self::backup_host_file_if_missing(default_path.exists(), &host_def, &host_def_bak)?;
                let original_content = fs::read_to_string(&default_path).unwrap_or_default();
                let updated_content = Self::update_default_grub_content(&original_content, timeout, default_entry, cmdline);
                Self::write_host_file_privileged(&host_def, &updated_content)?;

                if let Some(cmd) = cmdline {
                    let trimmed = cmd.trim();
                    if !trimmed.is_empty() {
                        let args_flag = format!("--args={}", trimmed);
                        let (code, stdout, stderr) =
                            HostBridge::run_privileged(&["grubby", "--update-kernel=ALL", &args_flag])?;
                        if code != 0 {
                            return Err(format!(
                                "grubby kernel args update failed (code {}): {} {}",
                                code, stdout, stderr
                            )
                            .trim()
                            .to_string());
                        }
                    }
                }
            }
            GrubStrategy::DirectDefaultGrub => {
                Self::backup_host_file_if_missing(default_path.exists(), &host_def, &host_def_bak)?;
                let original_content = fs::read_to_string(&default_path).unwrap_or_default();
                let updated_content = Self::update_default_grub_content(&original_content, timeout, default_entry, cmdline);
                Self::write_host_file_privileged(&host_def, &updated_content)?;
            }
            GrubStrategy::UnsupportedBootloader => unreachable!(),
        }

        Self::regenerate_grub_cfg(plan.mkconfig_cmd.as_deref(), &host_cfg)
    }

    /// Safely updates key-value pairs in /etc/default/grub content.
    pub fn update_default_grub_content(
        content: &str,
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
    ) -> String {
        let mut timeout_found = false;
        let mut default_found = false;
        let mut cmdline_found = false;

        let mut lines: Vec<String> = content.lines().map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("GRUB_TIMEOUT=") || trimmed.starts_with("#GRUB_TIMEOUT=") {
                timeout_found = true;
                format!("GRUB_TIMEOUT={}", timeout)
            } else if trimmed.starts_with("GRUB_DEFAULT=") || trimmed.starts_with("#GRUB_DEFAULT=") {
                default_found = true;
                format!("GRUB_DEFAULT=\"{}\"", default_entry)
            } else if let Some(new_cmd) = cmdline {
                if trimmed.starts_with("GRUB_CMDLINE_LINUX_DEFAULT=") || trimmed.starts_with("#GRUB_CMDLINE_LINUX_DEFAULT=") {
                    cmdline_found = true;
                    format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"", new_cmd)
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        }).collect();

        if !timeout_found {
            lines.push(format!("GRUB_TIMEOUT={}", timeout));
        }
        if !default_found {
            lines.push(format!("GRUB_DEFAULT=\"{}\"", default_entry));
        }
        if let Some(new_cmd) = cmdline {
            if !cmdline_found {
                lines.push(format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"", new_cmd));
            }
        }

        let mut res = lines.join("\n");
        res.push('\n');
        res
    }
}

fn format_unix_timestamp(secs: i64) -> String {
    let mut tm = libc::tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    let time_t = secs as libc::time_t;
    unsafe {
        libc::localtime_r(&time_t, &mut tm);
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}
