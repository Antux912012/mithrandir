use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::backend::distro::{DistroDetector, DistroFamily};
use crate::backend::grub::{GrubConfig, GrubManager};
use crate::backend::host_bridge::HostBridge;
use crate::os_info::is_immutable_system;

pub const PLYMOUTH_THEME_DIRS: &[&str] = &[
    "/usr/share/plymouth/themes",
    "/run/host/usr/share/plymouth/themes",
];

pub const PLYMOUTH_CONF_CANDIDATES: &[&str] = &[
    "/etc/plymouth/plymouthd.conf",
    "/run/host/etc/plymouth/plymouthd.conf",
];

pub const PLYMOUTH_DEFAULTS_CANDIDATES: &[&str] = &[
    "/usr/share/plymouth/plymouthd.defaults",
    "/run/host/usr/share/plymouth/plymouthd.defaults",
];

const MAX_ARCHIVE_BYTES: u64 = 50 * 1024 * 1024; // 50 MiB uncompressed safety cap

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InitramfsBackend {
    UpdateInitramfs,   // Debian / Ubuntu
    Dracut,            // Fedora / RHEL / openSUSE / OpenMandriva
    Mkinitcpio,        // Arch / CachyOS
    Booster,           // Arch / CachyOS alternative
    ImmutableReadOnly, // NixOS / OSTree / Kepler OS
    Unknown,
}

impl InitramfsBackend {
    pub fn label(&self) -> &'static str {
        match self {
            Self::UpdateInitramfs => "update-initramfs (Debian/Ubuntu)",
            Self::Dracut => "dracut (Fedora/RHEL/openSUSE)",
            Self::Mkinitcpio => "mkinitcpio (Arch/CachyOS)",
            Self::Booster => "booster (Arch/CachyOS)",
            Self::ImmutableReadOnly => "Immutable OS (Read-Only / Declarative)",
            Self::Unknown => "No supported initramfs generator detected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlymouthTheme {
    pub id: String,
    pub name: String,
    pub description: String,
    pub module_name: String,
    pub manifest_path: String,
    pub preview_image: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlymouthReadiness {
    pub plymouth_installed: bool,
    pub grub_has_quiet: bool,
    pub grub_has_splash: bool,
    pub grub_gfxpayload_keep: bool,
    pub mkinitcpio_has_plymouth_hook: Option<bool>,
    pub nvidia_kms_recommended: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlymouthConfig {
    pub active_theme: String,
    pub show_delay_secs: u32,
    pub device_timeout_secs: u32,
    pub conf_path: String,
    pub backup_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlymouthStatus {
    pub family: DistroFamily,
    pub initramfs_backend: InitramfsBackend,
    pub config: PlymouthConfig,
    pub readiness: PlymouthReadiness,
    pub themes: Vec<PlymouthTheme>,
    pub preview_supported: bool,
}

pub struct PlymouthManager;

impl PlymouthManager {
    /// Selects the host initramfs backend from distro family and detected CLI tools.
    pub fn select_initramfs_backend(
        family: DistroFamily,
        immutable: bool,
        has_update_initramfs: bool,
        has_dracut: bool,
        has_mkinitcpio: bool,
        has_booster: bool,
    ) -> InitramfsBackend {
        if immutable {
            return InitramfsBackend::ImmutableReadOnly;
        }
        match family {
            DistroFamily::Debian if has_update_initramfs => InitramfsBackend::UpdateInitramfs,
            DistroFamily::Fedora | DistroFamily::Suse | DistroFamily::Mandriva if has_dracut => {
                InitramfsBackend::Dracut
            }
            DistroFamily::Arch => {
                if has_mkinitcpio {
                    InitramfsBackend::Mkinitcpio
                } else if has_booster {
                    InitramfsBackend::Booster
                } else if has_dracut {
                    InitramfsBackend::Dracut
                } else {
                    InitramfsBackend::Unknown
                }
            }
            _ => {
                if has_update_initramfs {
                    InitramfsBackend::UpdateInitramfs
                } else if has_dracut {
                    InitramfsBackend::Dracut
                } else if has_mkinitcpio {
                    InitramfsBackend::Mkinitcpio
                } else if has_booster {
                    InitramfsBackend::Booster
                } else {
                    InitramfsBackend::Unknown
                }
            }
        }
    }

    /// Validates a Plymouth theme directory identifier to prevent path traversal or shell injection.
    pub fn validate_theme_id(theme_id: &str) -> Result<(), String> {
        let trimmed = theme_id.trim();
        if trimmed.is_empty() || trimmed.len() > 64 {
            return Err("Plymouth theme ID must be 1–64 characters.".to_string());
        }
        if trimmed.contains("..")
            || !trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(format!(
                "Invalid Plymouth theme ID '{}': only [A-Za-z0-9._-] allowed",
                theme_id
            ));
        }
        Ok(())
    }

    /// Parses a `.plymouth` INI manifest into a `PlymouthTheme`.
    pub fn parse_manifest(
        content: &str,
        id: &str,
        manifest_path: &str,
        theme_dir: Option<&Path>,
    ) -> Option<PlymouthTheme> {
        let mut section = String::new();
        let mut name: Option<String> = None;
        let mut description = String::new();
        let mut module_name: Option<String> = None;
        let mut image_dir: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed[1..trimmed.len() - 1].trim().to_string();
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let key = k.trim();
                let val = v.trim().to_string();
                if section.eq_ignore_ascii_case("Plymouth Theme") {
                    match key {
                        "Name" if !val.is_empty() => name = Some(val),
                        "Description" => description = val,
                        "ModuleName" if !val.is_empty() => module_name = Some(val),
                        _ => {}
                    }
                } else if key == "ImageDir" && !val.is_empty() {
                    image_dir = Some(val);
                }
            }
        }

        let module = module_name?;
        let display_name = name.unwrap_or_else(|| id.to_string());
        let preview_image = Self::resolve_preview_thumbnail(theme_dir, image_dir.as_deref());

        Some(PlymouthTheme {
            id: id.to_string(),
            name: display_name,
            description,
            module_name: module,
            manifest_path: manifest_path.to_string(),
            preview_image,
            is_active: false,
        })
    }

    /// Locates a candidate PNG thumbnail inside a theme directory or its declared `ImageDir`.
    fn resolve_preview_thumbnail(theme_dir: Option<&Path>, image_dir: Option<&str>) -> Option<String> {
        let mut search_dirs: Vec<PathBuf> = Vec::new();
        if let Some(td) = theme_dir {
            search_dirs.push(td.to_path_buf());
        }
        if let Some(idir) = image_dir {
            let p = Path::new(idir);
            if p.is_dir() {
                search_dirs.push(p.to_path_buf());
            } else {
                let host_p = Path::new("/run/host").join(idir.trim_start_matches('/'));
                if host_p.is_dir() {
                    search_dirs.push(host_p);
                }
            }
        }

        let preferred_names = [
            "watermark.png",
            "logo.png",
            "header-image.png",
            "background.png",
            "splash.png",
            "animation-0001.png",
            "throbber-0001.png",
        ];

        for dir in &search_dirs {
            for candidate in preferred_names {
                let p = dir.join(candidate);
                if p.is_file() {
                    return Some(p.to_string_lossy().to_string());
                }
            }
        }

        for dir in &search_dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                let mut pngs: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_file()
                            && p.extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                    })
                    .collect();
                pngs.sort();
                if let Some(first) = pngs.first() {
                    return Some(first.to_string_lossy().to_string());
                }
            }
        }

        None
    }

    /// Parses `/etc/plymouth/plymouthd.conf` `[Daemon]` keys (`Theme`, `ShowDelay`, `DeviceTimeout`).
    pub fn parse_plymouthd_conf(content: &str) -> (Option<String>, u32, u32) {
        let mut in_daemon = false;
        let mut theme: Option<String> = None;
        let mut show_delay: u32 = 0;
        let mut device_timeout: u32 = 8;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_daemon = trimmed[1..trimmed.len() - 1].trim().eq_ignore_ascii_case("Daemon");
                continue;
            }
            if !in_daemon {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let key = k.trim();
                let val = v.trim();
                match key {
                    "Theme" if !val.is_empty() => theme = Some(val.to_string()),
                    "ShowDelay" => {
                        if let Ok(d) = val.parse::<u32>() {
                            show_delay = d;
                        }
                    }
                    "DeviceTimeout" => {
                        if let Ok(t) = val.parse::<u32>() {
                            device_timeout = t;
                        }
                    }
                    _ => {}
                }
            }
        }

        (theme, show_delay, device_timeout)
    }

    /// Updates `[Daemon]` keys in `plymouthd.conf` while preserving comments and other sections.
    pub fn update_plymouthd_conf(
        content: &str,
        theme_id: &str,
        show_delay: u32,
        device_timeout: u32,
    ) -> String {
        let mut out_lines: Vec<String> = Vec::new();
        let mut has_daemon = false;
        let mut in_daemon = false;
        let mut wrote_theme = false;
        let mut wrote_delay = false;
        let mut wrote_timeout = false;

        let flush_missing = |lines: &mut Vec<String>, wt: bool, wd: bool, wto: bool| {
            if !wt {
                lines.push(format!("Theme={}", theme_id));
            }
            if !wd {
                lines.push(format!("ShowDelay={}", show_delay));
            }
            if !wto {
                lines.push(format!("DeviceTimeout={}", device_timeout));
            }
        };

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                if in_daemon {
                    flush_missing(&mut out_lines, wrote_theme, wrote_delay, wrote_timeout);
                    wrote_theme = true;
                    wrote_delay = true;
                    wrote_timeout = true;
                }
                let sec_name = trimmed[1..trimmed.len() - 1].trim();
                in_daemon = sec_name.eq_ignore_ascii_case("Daemon");
                if in_daemon {
                    has_daemon = true;
                }
                out_lines.push(line.to_string());
                continue;
            }

            if in_daemon && !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with(';') {
                if let Some((k, _)) = trimmed.split_once('=') {
                    match k.trim() {
                        "Theme" => {
                            if !wrote_theme {
                                out_lines.push(format!("Theme={}", theme_id));
                                wrote_theme = true;
                            }
                            continue;
                        }
                        "ShowDelay" => {
                            if !wrote_delay {
                                out_lines.push(format!("ShowDelay={}", show_delay));
                                wrote_delay = true;
                            }
                            continue;
                        }
                        "DeviceTimeout" => {
                            if !wrote_timeout {
                                out_lines.push(format!("DeviceTimeout={}", device_timeout));
                                wrote_timeout = true;
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            out_lines.push(line.to_string());
        }

        if in_daemon {
            flush_missing(&mut out_lines, wrote_theme, wrote_delay, wrote_timeout);
        } else if !has_daemon {
            if !out_lines.is_empty() {
                out_lines.push(String::new());
            }
            out_lines.push("[Daemon]".to_string());
            out_lines.push(format!("Theme={}", theme_id));
            out_lines.push(format!("ShowDelay={}", show_delay));
            out_lines.push(format!("DeviceTimeout={}", device_timeout));
        }

        let mut res = out_lines.join("\n");
        res.push('\n');
        res
    }

    /// Checks whether `/etc/mkinitcpio.conf` has an active `HOOKS=(...)` line containing `plymouth`.
    pub fn check_mkinitcpio_hook(conf_content: &str) -> bool {
        let mut hooks_str = String::new();
        let mut collecting = false;

        for line in conf_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("HOOKS=") {
                collecting = true;
                hooks_str.clear();
                hooks_str.push_str(rest);
                if rest.contains(')') {
                    collecting = false;
                }
                continue;
            }
            if collecting {
                hooks_str.push(' ');
                hooks_str.push_str(trimmed);
                if trimmed.contains(')') {
                    collecting = false;
                }
            }
        }

        let inner = hooks_str
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim_matches('"')
            .trim_matches('\'');

        inner.split_whitespace().any(|tok| tok == "plymouth")
    }

    /// Audits GRUB kernel flags, `gfxpayload=keep`, Arch `mkinitcpio` hooks, and NVIDIA KMS readiness.
    pub fn audit_readiness(
        family: DistroFamily,
        grub_cfg: &GrubConfig,
        plymouth_installed: bool,
        mkinitcpio_content: Option<&str>,
        has_nvidia: bool,
    ) -> PlymouthReadiness {
        let tokens: Vec<&str> = grub_cfg.cmdline.split_whitespace().collect();
        let grub_has_quiet = tokens.contains(&"quiet");
        let grub_has_splash = tokens.contains(&"splash") || tokens.contains(&"rhgb");
        let grub_gfxpayload_keep = grub_cfg
            .personalisation
            .gfxpayload_linux
            .as_deref()
            .is_some_and(|v| v.trim() == "keep");

        let mkinitcpio_has_plymouth_hook = if family == DistroFamily::Arch {
            mkinitcpio_content.map(Self::check_mkinitcpio_hook)
        } else {
            None
        };

        let nvidia_kms_recommended = has_nvidia
            && !tokens.iter().any(|t| *t == "nvidia-drm.modeset=1" || *t == "nvidia_drm.modeset=1");

        let mut warnings = Vec::new();
        if !plymouth_installed {
            warnings.push("Plymouth binary is not installed on the host system.".to_string());
        }
        if !grub_has_quiet || !grub_has_splash {
            warnings.push(
                "GRUB kernel command line is missing 'quiet' and/or 'splash' (required to display graphical boot splash)."
                    .to_string(),
            );
        }
        if !grub_gfxpayload_keep {
            warnings.push(
                "GRUB_GFXPAYLOAD_LINUX=keep is not set (recommended for flicker-free framebuffer handoff)."
                    .to_string(),
            );
        }
        if mkinitcpio_has_plymouth_hook == Some(false) {
            warnings.push(
                "Arch /etc/mkinitcpio.conf HOOKS=(...) does not include 'plymouth'.".to_string(),
            );
        }
        if nvidia_kms_recommended {
            warnings.push(
                "NVIDIA GPU detected without 'nvidia-drm.modeset=1'; Plymouth may fall back to text mode."
                    .to_string(),
            );
        }

        PlymouthReadiness {
            plymouth_installed,
            grub_has_quiet,
            grub_has_splash,
            grub_gfxpayload_keep,
            mkinitcpio_has_plymouth_hook,
            nvidia_kms_recommended,
            warnings,
        }
    }

    /// Discovers installed Plymouth themes across standard system directories.
    pub fn scan_themes(active_theme: &str) -> Vec<PlymouthTheme> {
        let mut themes = Vec::new();
        let mut seen_ids = HashSet::new();

        for dir_str in PLYMOUTH_THEME_DIRS {
            let base = Path::new(dir_str);
            let Ok(entries) = fs::read_dir(base) else {
                continue;
            };
            for entry in entries.flatten() {
                let theme_dir = entry.path();
                if !theme_dir.is_dir() {
                    continue;
                }
                let id = entry.file_name().to_string_lossy().to_string();
                if !seen_ids.insert(id.clone()) {
                    continue;
                }

                // Prefer <id>.plymouth, fallback to first *.plymouth in the directory
                let primary_manifest = theme_dir.join(format!("{}.plymouth", id));
                let manifest_path = if primary_manifest.is_file() {
                    Some(primary_manifest)
                } else {
                    fs::read_dir(&theme_dir).ok().and_then(|rd| {
                        rd.flatten()
                            .map(|e| e.path())
                            .find(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("plymouth"))
                    })
                };

                if let Some(mp) = manifest_path {
                    if let Ok(content) = fs::read_to_string(&mp) {
                        let host_mp = HostBridge::host_path(&mp);
                        if let Some(mut theme) =
                            Self::parse_manifest(&content, &id, &host_mp, Some(&theme_dir))
                        {
                            theme.is_active = theme.id == active_theme;
                            themes.push(theme);
                        }
                    }
                }
            }
        }

        themes.sort_by(|a, b| {
            b.is_active
                .cmp(&a.is_active)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        themes
    }

    /// Probes active Plymouth configuration, installed themes, initramfs backend, and readiness.
    pub fn detect() -> PlymouthStatus {
        let distro = DistroDetector::detect();
        let immutable = is_immutable_system();
        let plymouth_installed =
            HostBridge::which("plymouth").is_some() || HostBridge::which("plymouthd").is_some();

        let has_update_initramfs = HostBridge::which("update-initramfs").is_some();
        let has_dracut = HostBridge::which("dracut").is_some();
        let has_mkinitcpio = HostBridge::which("mkinitcpio").is_some();
        let has_booster = HostBridge::which("booster").is_some();

        let initramfs_backend = Self::select_initramfs_backend(
            distro.family,
            immutable,
            has_update_initramfs,
            has_dracut,
            has_mkinitcpio,
            has_booster,
        );

        let conf_path_buf = PLYMOUTH_CONF_CANDIDATES
            .iter()
            .map(Path::new)
            .find(|p| p.exists())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/etc/plymouth/plymouthd.conf"));

        let backup_exists = PathBuf::from(format!("{}.bak", conf_path_buf.display())).exists();

        let conf_content = fs::read_to_string(&conf_path_buf).unwrap_or_default();
        let (mut active_theme_opt, mut show_delay_secs, mut device_timeout_secs) =
            Self::parse_plymouthd_conf(&conf_content);

        if active_theme_opt.is_none() {
            for def_cand in PLYMOUTH_DEFAULTS_CANDIDATES {
                if let Ok(def_content) = fs::read_to_string(def_cand) {
                    let (t, d, to) = Self::parse_plymouthd_conf(&def_content);
                    if active_theme_opt.is_none() {
                        active_theme_opt = t;
                    }
                    if show_delay_secs == 0 && d > 0 {
                        show_delay_secs = d;
                    }
                    if device_timeout_secs == 8 && to != 8 {
                        device_timeout_secs = to;
                    }
                }
            }
        }

        if active_theme_opt.is_none() && HostBridge::which("plymouth-set-default-theme").is_some() {
            if let Ok((0, out, _)) = HostBridge::run_command("plymouth-set-default-theme", &[]) {
                let t = out.trim().to_string();
                if !t.is_empty() {
                    active_theme_opt = Some(t);
                }
            }
        }

        let active_theme = active_theme_opt.unwrap_or_else(|| "spinner".to_string());
        let themes = Self::scan_themes(&active_theme);

        let grub_cfg = GrubManager::read_config();
        let mkinitcpio_content = ["/etc/mkinitcpio.conf", "/run/host/etc/mkinitcpio.conf"]
            .iter()
            .find_map(|p| fs::read_to_string(p).ok());

        let has_nvidia = Path::new("/proc/driver/nvidia/version").exists()
            || Path::new("/sys/module/nvidia").exists();

        let readiness = Self::audit_readiness(
            distro.family,
            &grub_cfg,
            plymouth_installed,
            mkinitcpio_content.as_deref(),
            has_nvidia,
        );

        let preview_supported = plymouth_installed
            && (HostBridge::which("plymouth-x11").is_some()
                || std::env::var("DISPLAY").is_ok()
                || std::env::var("WAYLAND_DISPLAY").is_ok());

        PlymouthStatus {
            family: distro.family,
            initramfs_backend,
            config: PlymouthConfig {
                active_theme,
                show_delay_secs,
                device_timeout_secs,
                conf_path: HostBridge::host_path(&conf_path_buf),
                backup_exists,
            },
            readiness,
            themes,
            preview_supported,
        }
    }

    /// Applies `theme_id`, `show_delay`, and `device_timeout` atomically and rebuilds initramfs
    /// while streaming live output lines to `on_line`.
    pub fn apply_theme_streaming<F>(
        theme_id: &str,
        show_delay: u32,
        device_timeout: u32,
        mut on_line: F,
    ) -> Result<(), String>
    where
        F: FnMut(String),
    {
        Self::validate_theme_id(theme_id)?;
        let status = Self::detect();
        if status.initramfs_backend == InitramfsBackend::ImmutableReadOnly {
            return Err("Immutable OS detected: configure Plymouth declaratively (e.g. rpm-ostree initramfs or NixOS boot.plymouth.theme).".to_string());
        }
        if status.initramfs_backend == InitramfsBackend::Unknown {
            return Err("No supported initramfs generator (update-initramfs, dracut, mkinitcpio, or booster) found on host.".to_string());
        }

        let target_theme = status
            .themes
            .iter()
            .find(|t| t.id == theme_id)
            .ok_or_else(|| format!("Plymouth theme '{}' not found on host.", theme_id))?;

        let conf_read_path = PLYMOUTH_CONF_CANDIDATES
            .iter()
            .map(Path::new)
            .find(|p| p.exists())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/etc/plymouth/plymouthd.conf"));
        let existing_conf = fs::read_to_string(&conf_read_path).unwrap_or_default();
        let new_conf = Self::update_plymouthd_conf(
            &existing_conf,
            theme_id,
            show_delay.min(60),
            device_timeout.clamp(1, 120),
        );

        let backend_tag = match status.initramfs_backend {
            InitramfsBackend::UpdateInitramfs => "update-initramfs",
            InitramfsBackend::Dracut => "dracut",
            InitramfsBackend::Mkinitcpio => "mkinitcpio",
            InitramfsBackend::Booster => "booster",
            _ => "unknown",
        };
        let has_setter = if HostBridge::which("plymouth-set-default-theme").is_some() {
            "1"
        } else {
            "0"
        };

        const SCRIPT: &str = r#"
set -e
theme="$1"; manifest="$2"; backend="$3"; has_setter="$4"; conf_b64="$5"
conf="/etc/plymouth/plymouthd.conf"
bak="/etc/plymouth/plymouthd.conf.bak"
mkdir -p /etc/plymouth
if [ -e "$conf" ] && [ ! -e "$bak" ]; then
  cp -p "$conf" "$bak"
  chmod 644 "$bak"
fi
tmp="${conf}.mithrandir.tmp"
printf '%s' "$conf_b64" | base64 -d > "$tmp"
chmod 644 "$tmp"
mv -f "$tmp" "$conf"

echo "[Mithrandir] Updated $conf (Theme=$theme)"
if [ "$backend" = "dracut" ] && [ "$has_setter" = "1" ]; then
  echo "[Mithrandir] Running plymouth-set-default-theme -R $theme ..."
  plymouth-set-default-theme -R "$theme"
  exit 0
fi

if [ "$has_setter" = "1" ]; then
  plymouth-set-default-theme "$theme" || true
fi

if [ "$backend" = "update-initramfs" ]; then
  if command -v update-alternatives >/dev/null 2>&1 && [ -f "$manifest" ]; then
    update-alternatives --install /usr/share/plymouth/themes/default.plymouth default.plymouth "$manifest" 150 || true
    update-alternatives --set default.plymouth "$manifest" || true
  fi
  echo "[Mithrandir] Rebuilding initramfs (update-initramfs -u)..."
  update-initramfs -u
elif [ "$backend" = "mkinitcpio" ]; then
  echo "[Mithrandir] Rebuilding initramfs (mkinitcpio -P)..."
  mkinitcpio -P
elif [ "$backend" = "booster" ]; then
  echo "[Mithrandir] Rebuilding initramfs (booster build)..."
  booster build --force
elif [ "$backend" = "dracut" ]; then
  echo "[Mithrandir] Rebuilding initramfs (dracut --regenerate-all --force)..."
  dracut --regenerate-all --force
fi
"#;

        let encoded = base64_encode(new_conf.as_bytes());
        let code = HostBridge::spawn_streaming(
            "sh",
            &[
                "-c",
                SCRIPT,
                "mithrandir-plymouth-apply",
                theme_id,
                &target_theme.manifest_path,
                backend_tag,
                has_setter,
                &encoded,
            ],
            true,
            &mut on_line,
        )?;

        if code == 0 {
            Ok(())
        } else {
            Err(format!("Plymouth apply/initramfs rebuild exited with code {}", code))
        }
    }

    /// Restores `/etc/plymouth/plymouthd.conf.bak` and rebuilds the initramfs.
    pub fn revert_to_backup_streaming<F>(mut on_line: F) -> Result<String, String>
    where
        F: FnMut(String),
    {
        let status = Self::detect();
        if !status.config.backup_exists {
            return Err("No /etc/plymouth/plymouthd.conf.bak backup file exists.".to_string());
        }
        if status.initramfs_backend == InitramfsBackend::ImmutableReadOnly {
            return Err("Immutable OS detected; restore is disabled.".to_string());
        }

        let bak_path = PLYMOUTH_CONF_CANDIDATES
            .iter()
            .map(|p| PathBuf::from(format!("{}.bak", p)))
            .find(|p| p.exists())
            .ok_or_else(|| "Backup file not found".to_string())?;
        let bak_content = fs::read_to_string(&bak_path).unwrap_or_default();
        let (prev_theme, _, _) = Self::parse_plymouthd_conf(&bak_content);
        let prev_name = prev_theme.unwrap_or_else(|| "default".to_string());

        let backend_tag = match status.initramfs_backend {
            InitramfsBackend::UpdateInitramfs => "update-initramfs",
            InitramfsBackend::Dracut => "dracut",
            InitramfsBackend::Mkinitcpio => "mkinitcpio",
            InitramfsBackend::Booster => "booster",
            _ => "unknown",
        };

        const SCRIPT: &str = r#"
set -e
prev="$1"; backend="$2"
cp -p /etc/plymouth/plymouthd.conf.bak /etc/plymouth/plymouthd.conf
chmod 644 /etc/plymouth/plymouthd.conf
echo "[Mithrandir] Restored /etc/plymouth/plymouthd.conf (Theme=$prev)"
if [ "$backend" = "dracut" ] && command -v plymouth-set-default-theme >/dev/null 2>&1; then
  plymouth-set-default-theme -R "$prev"
elif [ "$backend" = "update-initramfs" ]; then
  if command -v plymouth-set-default-theme >/dev/null 2>&1; then
    plymouth-set-default-theme "$prev" || true
  fi
  update-initramfs -u
elif [ "$backend" = "mkinitcpio" ]; then
  if command -v plymouth-set-default-theme >/dev/null 2>&1; then
    plymouth-set-default-theme "$prev" || true
  fi
  mkinitcpio -P
elif [ "$backend" = "booster" ]; then
  booster build --force
elif [ "$backend" = "dracut" ]; then
  dracut --regenerate-all --force
fi
"#;

        let code = HostBridge::spawn_streaming(
            "sh",
            &["-c", SCRIPT, "mithrandir-plymouth-revert", &prev_name, backend_tag],
            true,
            &mut on_line,
        )?;

        if code == 0 {
            Ok(format!("Restored previous Plymouth theme '{}'", prev_name))
        } else {
            Err(format!("Plymouth revert exited with code {}", code))
        }
    }

    /// Runs a bounded non-destructive preview of `theme_id` (3..=15s) with guaranteed `plymouth quit` teardown.
    pub fn preview_theme(theme_id: &str, duration_secs: u32) -> Result<(), String> {
        Self::validate_theme_id(theme_id)?;
        let secs = duration_secs.clamp(3, 15).to_string();

        const SCRIPT: &str = r#"
set -e
theme="$1"; secs="$2"
cleanup() {
  plymouth quit 2>/dev/null || true
}
trap cleanup EXIT INT TERM
if command -v plymouth-set-default-theme >/dev/null 2>&1; then
  orig=$(plymouth-set-default-theme 2>/dev/null || true)
fi
plymouthd --mode=boot --tty=/dev/tty1 2>/dev/null || plymouthd 2>/dev/null || true
plymouth --show-splash
sleep "$secs"
"#;

        let (code, stdout, stderr) = HostBridge::run_privileged(&[
            "sh",
            "-c",
            SCRIPT,
            "mithrandir-plymouth-preview",
            theme_id,
            &secs,
        ])?;

        if code == 0 {
            Ok(())
        } else {
            Err(format!("Plymouth preview failed (code {}): {} {}", code, stdout, stderr)
                .trim()
                .to_string())
        }
    }

    /// Validates an archive or theme directory in an unprivileged staging folder and installs it
    /// into `/usr/share/plymouth/themes/<name>`.
    pub fn import_theme_archive(archive_path: &Path) -> Result<String, String> {
        if !archive_path.exists() {
            return Err(format!("Selected file does not exist: {}", archive_path.display()));
        }

        let staging = StagingDir::new()?;

        if archive_path.is_dir() {
            copy_dir_safe(archive_path, staging.path())?;
        } else {
            let fname = archive_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            let src_str = archive_path.to_string_lossy().to_string();
            let dst_str = staging.path().to_string_lossy().to_string();

            let (code, stdout, stderr) = if fname.ends_with(".zip") {
                HostBridge::run_command("unzip", &["-q", "-n", &src_str, "-d", &dst_str])?
            } else if fname.ends_with(".tar.gz")
                || fname.ends_with(".tgz")
                || fname.ends_with(".tar.xz")
                || fname.ends_with(".tar.bz2")
                || fname.ends_with(".tar")
            {
                HostBridge::run_command("tar", &["-xf", &src_str, "-C", &dst_str])?
            } else {
                return Err("Unsupported archive format. Use .tar.gz, .tar.xz, .zip, or a theme folder.".to_string());
            };

            if code != 0 {
                return Err(format!("Failed extracting theme archive (code {}): {} {}", code, stdout, stderr).trim().to_string());
            }
        }

        let (theme_id, theme_root) = Self::validate_staged_theme_dir(staging.path())?;
        let src_dir_str = theme_root.to_string_lossy().to_string();

        const INSTALL_SCRIPT: &str = r#"
set -e
src="$1"; id="$2"
dest="/usr/share/plymouth/themes/$id"
mkdir -p /usr/share/plymouth/themes
rm -rf "$dest"
cp -r "$src" "$dest"
chmod -R a+rX "$dest"
"#;

        let (code, stdout, stderr) = HostBridge::run_privileged(&[
            "sh",
            "-c",
            INSTALL_SCRIPT,
            "mithrandir-plymouth-import",
            &src_dir_str,
            &theme_id,
        ])?;

        if code == 0 {
            Ok(theme_id)
        } else {
            Err(format!("Failed installing theme '{}' (code {}): {} {}", theme_id, code, stdout, stderr).trim().to_string())
        }
    }

    /// Validates an extracted staging directory: checks size limit, rejects symlinks/traversal,
    /// and locates the `.plymouth` manifest.
    pub fn validate_staged_theme_dir(root: &Path) -> Result<(String, PathBuf), String> {
        let mut total_bytes: u64 = 0;
        let mut manifests: Vec<PathBuf> = Vec::new();
        let mut stack = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir)
                .map_err(|e| format!("Failed reading staged dir {}: {}", dir.display(), e))?;
            for entry in entries.flatten() {
                let path = entry.path();
                let meta = fs::symlink_metadata(&path)
                    .map_err(|e| format!("Failed inspecting {}: {}", path.display(), e))?;

                if meta.file_type().is_symlink() {
                    return Err(format!(
                        "Rejected theme archive: symbolic links are disallowed ({})",
                        path.display()
                    ));
                }

                for comp in path.components() {
                    if matches!(comp, Component::ParentDir) {
                        return Err("Rejected theme archive: path traversal '..' detected".to_string());
                    }
                }

                if meta.is_dir() {
                    stack.push(path);
                } else {
                    total_bytes += meta.len();
                    if total_bytes > MAX_ARCHIVE_BYTES {
                        return Err("Rejected theme archive: uncompressed size exceeds 50 MiB limit".to_string());
                    }
                    if path.extension().and_then(|e| e.to_str()) == Some("plymouth") {
                        manifests.push(path);
                    }
                }
            }
        }

        if manifests.len() != 1 {
            return Err(format!(
                "Expected exactly 1 .plymouth manifest file in theme, found {}",
                manifests.len()
            ));
        }

        let manifest_path = &manifests[0];
        let theme_root = manifest_path
            .parent()
            .ok_or_else(|| "Invalid manifest parent".to_string())?
            .to_path_buf();

        let id = manifest_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "Invalid .plymouth filename".to_string())?
            .to_string();
        Self::validate_theme_id(&id)?;

        let content = fs::read_to_string(manifest_path)
            .map_err(|e| format!("Failed reading manifest: {}", e))?;
        if Self::parse_manifest(&content, &id, "", Some(&theme_root)).is_none() {
            return Err("Invalid .plymouth manifest: missing [Plymouth Theme] or ModuleName=".to_string());
        }

        Ok((id, theme_root))
    }

    /// Appends `quiet` and `splash` to GRUB kernel arguments and sets `GRUB_GFXPAYLOAD_LINUX=keep`.
    pub fn enable_grub_splash_flags() -> Result<(), String> {
        let cfg = GrubManager::read_config();
        let mut tokens: Vec<String> = cfg.cmdline.split_whitespace().map(|s| s.to_string()).collect();
        if !tokens.iter().any(|t| t == "quiet") {
            tokens.push("quiet".to_string());
        }
        if !tokens.iter().any(|t| t == "splash" || t == "rhgb") {
            tokens.push("splash".to_string());
        }
        let new_cmdline = tokens.join(" ");

        let mut pers = cfg.personalisation.clone();
        pers.gfxpayload_linux = Some("keep".to_string());
        if pers.terminal_output.is_none() {
            pers.terminal_output = Some("gfxterm".to_string());
        }

        GrubManager::save_and_update_with_personalisation(
            cfg.timeout,
            &cfg.default_entry,
            Some(&new_cmdline),
            Some(&pers),
        )
    }
}

fn copy_dir_safe(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {}", dst.display(), e))?;
    let entries = fs::read_dir(src).map_err(|e| format!("read_dir {}: {}", src.display(), e))?;
    for entry in entries.flatten() {
        let p = entry.path();
        let meta = fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err(format!("Symlinks are disallowed in theme import: {}", p.display()));
        }
        let target = dst.join(entry.file_name());
        if meta.is_dir() {
            copy_dir_safe(&p, &target)?;
        } else {
            fs::copy(&p, &target).map_err(|e| format!("copy {}: {}", p.display(), e))?;
        }
    }
    Ok(())
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) & 0x3F] as char);
        out.push(TABLE[(triple >> 12) & 0x3F] as char);
        out.push(if chunk.len() > 1 { TABLE[(triple >> 6) & 0x3F] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[triple & 0x3F] as char } else { '=' });
    }
    out
}

struct StagingDir(PathBuf);

impl StagingDir {
    fn new() -> Result<Self, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "mithrandir-plymouth-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed creating staging dir {}: {}", dir.display(), e))?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
