use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::backend::host_bridge::HostBridge;

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
    "/run/host/etc/default/grub",
];

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

    /// Derives the backup file path for a given grub.cfg.
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

    /// Returns the live real-time status of grub.cfg and grub.cfg.bak.
    pub fn get_live_status() -> GrubLiveStatus {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);

        let (cfg_exists, cfg_size_bytes, cfg_modified_time) = Self::get_file_metadata(&cfg_path);
        let (backup_exists, backup_size_bytes, backup_modified_time) = Self::get_file_metadata(&backup_path);

        GrubLiveStatus {
            cfg_path: cfg_path.to_string_lossy().to_string(),
            cfg_exists,
            cfg_size_bytes,
            cfg_modified_time,
            backup_path: backup_path.to_string_lossy().to_string(),
            backup_exists,
            backup_size_bytes,
            backup_modified_time,
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
            for line in content.lines() {
                let l = line.trim();
                if let Some(val) = l.strip_prefix("GRUB_TIMEOUT=") {
                    let v = val.trim_matches('"').trim_matches('\'');
                    if let Ok(t) = v.parse::<i32>() {
                        timeout = t;
                    }
                } else if let Some(val) = l.strip_prefix("GRUB_DEFAULT=") {
                    default_entry = val.trim_matches('"').trim_matches('\'').to_string();
                } else if let Some(val) = l.strip_prefix("GRUB_CMDLINE_LINUX_DEFAULT=") {
                    cmdline = val.trim_matches('"').trim_matches('\'').to_string();
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

    /// Generates a backup of the active grub.cfg (grub.cfg.bak) and /etc/default/grub.
    pub fn create_backup() -> Result<String, String> {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();
        let default_bak = PathBuf::from(format!("{}.bak", default_path.display()));

        let script = format!(
            "mkdir -p \"$(dirname '{bak}')\" && \
             if [ -f '{cfg}' ]; then cp -p '{cfg}' '{bak}' && chmod 644 '{bak}'; fi && \
             if [ -f '{def}' ]; then cp -p '{def}' '{def_bak}' && chmod 644 '{def_bak}'; fi",
            cfg = cfg_path.display(),
            bak = backup_path.display(),
            def = default_path.display(),
            def_bak = default_bak.display(),
        );

        let (code, stdout, stderr) = HostBridge::run_privileged(&["sh", "-c", &script])?;
        if code == 0 {
            Ok(format!("Backup successfully generated at {}", backup_path.display()))
        } else {
            Err(format!("Backup failed (code {}): {} {}", code, stdout, stderr).trim().to_string())
        }
    }

    /// Recovers the original configuration from grub.cfg.bak and /etc/default/grub.bak.
    pub fn restore_backup() -> Result<String, String> {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();
        let default_bak = PathBuf::from(format!("{}.bak", default_path.display()));

        if !backup_path.exists() {
            return Err(format!("Backup file does not exist: {}", backup_path.display()));
        }

        let script = format!(
            "if [ ! -f '{bak}' ]; then echo 'Backup not found' >&2; exit 1; fi && \
             cp -p '{bak}' '{cfg}' && \
             if [ -f '{def_bak}' ]; then cp -p '{def_bak}' '{def}'; fi && \
             (update-grub || grub2-mkconfig -o '{cfg}' || grub-mkconfig -o '{cfg}' || true)",
            bak = backup_path.display(),
            cfg = cfg_path.display(),
            def_bak = default_bak.display(),
            def = default_path.display(),
        );

        let (code, stdout, stderr) = HostBridge::run_privileged(&["sh", "-c", &script])?;
        if code == 0 {
            Ok(format!("Configuration successfully restored from {}", backup_path.display()))
        } else {
            Err(format!("Restore failed (code {}): {} {}", code, stdout, stderr).trim().to_string())
        }
    }

    /// Modifies /etc/default/grub and regenerates the active grub.cfg,
    /// ensuring a backup is automatically preserved first.
    pub fn save_and_update(timeout: i32, default_entry: &str, cmdline: Option<&str>) -> Result<(), String> {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();

        let original_content = fs::read_to_string(&default_path).unwrap_or_default();
        let updated_content = Self::update_default_grub_content(&original_content, timeout, default_entry, cmdline);

        let tmp_path = format!("/tmp/mithrandir-grub-{}.tmp", std::process::id());
        fs::write(&tmp_path, updated_content)
            .map_err(|e| format!("Failed to stage temporary GRUB config: {}", e))?;

        let script = format!(
            "if [ -f '{cfg}' ] && [ ! -f '{bak}' ]; then cp -p '{cfg}' '{bak}' && chmod 644 '{bak}'; fi && \
             if [ -f '{def}' ] && [ ! -f '{def}.bak' ]; then cp -p '{def}' '{def}.bak' && chmod 644 '{def}.bak'; fi && \
             cp -p '{tmp}' '{def}' && chmod 644 '{def}' && rm -f '{tmp}' && \
             (update-grub || grub2-mkconfig -o '{cfg}' || grub-mkconfig -o '{cfg}')",
            cfg = cfg_path.display(),
            bak = backup_path.display(),
            def = default_path.display(),
            tmp = tmp_path,
        );

        let (code, stdout, stderr) = HostBridge::run_privileged(&["sh", "-c", &script])?;
        let _ = fs::remove_file(&tmp_path);

        if code == 0 {
            Ok(())
        } else {
            Err(format!("GRUB update exited with code {}: {} {}", code, stdout, stderr).trim().to_string())
        }
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
