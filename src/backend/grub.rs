use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
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

pub const GRUB_THEME_DIRS: &[&str] = &[
    "/boot/grub/themes",
    "/boot/grub2/themes",
    "/usr/share/grub/themes",
    "/run/host/boot/grub/themes",
    "/run/host/boot/grub2/themes",
    "/run/host/usr/share/grub/themes",
];

pub const GRUB_FONT_DIRS: &[&str] = &[
    "/boot/grub/fonts",
    "/boot/grub2/fonts",
    "/usr/share/grub",
    "/run/host/boot/grub/fonts",
    "/run/host/boot/grub2/fonts",
    "/run/host/usr/share/grub",
];

/// Official 16-color GRUB2 palette names (`black` as background = transparent over image).
pub const GRUB_COLOR_PALETTE: &[&str] = &[
    "black",
    "blue",
    "green",
    "cyan",
    "red",
    "magenta",
    "brown",
    "light-gray",
    "dark-gray",
    "light-blue",
    "light-green",
    "light-cyan",
    "light-red",
    "light-magenta",
    "yellow",
    "white",
];

pub const GRUB_GFXMODE_PRESETS: &[&str] = &[
    "auto",
    "3840x2160",
    "2560x1440",
    "1920x1080x32",
    "1920x1080",
    "1920x1080,auto",
    "1600x900",
    "1366x768",
    "1280x800",
    "1280x720",
    "1024x768",
    "800x600",
];

pub const GRUB_GFXPAYLOAD_OPTIONS: &[&str] = &["keep", "auto", "text"];

pub const GRUB_TERMINAL_OUTPUTS: &[&str] = &["gfxterm", "console", "vga_text", "serial"];

pub const GRUB_TIMEOUT_STYLES: &[&str] = &["menu", "countdown", "hidden"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrubColorPreset {
    pub name: &'static str,
    pub normal: &'static str,
    pub highlight: &'static str,
}

pub const GRUB_COLOR_PRESETS: &[GrubColorPreset] = &[
    GrubColorPreset {
        name: "Default / System",
        normal: "",
        highlight: "",
    },
    GrubColorPreset {
        name: "Nord Frost",
        normal: "light-gray/black",
        highlight: "white/blue",
    },
    GrubColorPreset {
        name: "Matrix Terminal",
        normal: "light-green/black",
        highlight: "black/light-green",
    },
    GrubColorPreset {
        name: "Solarized Dark",
        normal: "light-cyan/black",
        highlight: "yellow/blue",
    },
    GrubColorPreset {
        name: "Gruvbox Warm",
        normal: "brown/black",
        highlight: "yellow/dark-gray",
    },
    GrubColorPreset {
        name: "High Contrast",
        normal: "white/black",
        highlight: "black/white",
    },
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrubMenuEntry {
    pub title: String,
    /// Exact GRUB_DEFAULT value (e.g. "0", "2", or "1>0" for submenu children).
    pub grub_id: String,
    pub submenu_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrubThemeInfo {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrubFontInfo {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrubPersonalisation {
    pub theme: Option<String>,
    pub background: Option<String>,
    pub color_normal: Option<String>,
    pub color_highlight: Option<String>,
    pub font: Option<String>,
    pub gfxmode: Option<String>,
    pub gfxpayload_linux: Option<String>,
    pub terminal_output: Option<String>,
    pub timeout_style: Option<String>,
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
    pub mithrandir_dropin_exists: bool,
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
    #[serde(default)]
    pub structured_entries: Vec<GrubMenuEntry>,
    #[serde(default)]
    pub personalisation: GrubPersonalisation,
    #[serde(default)]
    pub available_themes: Vec<GrubThemeInfo>,
    #[serde(default)]
    pub available_fonts: Vec<GrubFontInfo>,
}

/// Cached host binary probe results so periodic status polling never spawns `which` on the UI thread.
struct CachedHostTools {
    family: DistroFamily,
    immutable: bool,
    has_grubby: bool,
    mkconfig_cmd: Option<String>,
}

static HOST_TOOLS_CACHE: OnceLock<CachedHostTools> = OnceLock::new();

fn cached_host_tools() -> &'static CachedHostTools {
    HOST_TOOLS_CACHE.get_or_init(|| {
        let distro = DistroDetector::detect();
        let immutable = is_immutable_system();
        let has_grubby = HostBridge::which("grubby").is_some();
        let mkconfig_cmd = GrubManager::detect_mkconfig_cmd(distro.family);
        CachedHostTools {
            family: distro.family,
            immutable,
            has_grubby,
            mkconfig_cmd,
        }
    })
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
            let modified = meta
                .modified()
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
    /// Uses cached host binary detection so polling never spawns subprocesses on the UI thread.
    pub fn get_live_status() -> GrubLiveStatus {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);

        let default_path = Self::detect_default_grub_path();
        let default_backup_path = Self::get_backup_path(&default_path);

        let (cfg_exists, cfg_size_bytes, cfg_modified_time) = Self::get_file_metadata(&cfg_path);
        let (backup_exists, backup_size_bytes, backup_modified_time) =
            Self::get_file_metadata(&backup_path);

        let (default_exists, default_size_bytes, default_modified_time) =
            Self::get_file_metadata(&default_path);
        let (default_backup_exists, default_backup_size_bytes, default_backup_modified_time) =
            Self::get_file_metadata(&default_backup_path);

        let dropins_dir = Self::detect_default_grub_d_dir();
        let default_dropins_count = dropins_dir
            .as_ref()
            .map(|d| Self::count_default_dropins(d))
            .unwrap_or(0);
        let mithrandir_dropin_exists = dropins_dir
            .as_ref()
            .is_some_and(|d| d.join(MITHRANDIR_DROPIN_FILENAME).exists());
        let default_dropins_dir = dropins_dir.map(|d| d.to_string_lossy().to_string());
        let strategy_label = Self::detect_strategy_plan_cached(cfg_exists, &default_path).label;

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
            mithrandir_dropin_exists,
            strategy_label,
        }
    }

    /// Parses a single shell assignment line (`[export ]KEY=VALUE [# comment]`).
    fn parse_shell_assignment(line: &str) -> Option<(&str, String)> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        let stripped = trimmed.strip_prefix("export ").map(str::trim_start).unwrap_or(trimmed);
        let (key, raw_val) = stripped.split_once('=')?;
        let key = key.trim();
        let raw_val = raw_val.trim();

        let value = if let Some(rest) = raw_val.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                rest[..end].to_string()
            } else {
                rest.to_string()
            }
        } else if let Some(rest) = raw_val.strip_prefix('\'') {
            if let Some(end) = rest.find('\'') {
                rest[..end].to_string()
            } else {
                rest.to_string()
            }
        } else {
            // Unquoted value: strip trailing inline `# comment`
            raw_val
                .split_once('#')
                .map(|(v, _)| v.trim())
                .unwrap_or(raw_val)
                .to_string()
        };

        Some((key, value))
    }

    /// Parses key GRUB parameters from configuration file content.
    pub fn parse_default_grub_vars(
        content: &str,
        timeout: &mut i32,
        default_entry: &mut String,
        cmdline: &mut String,
    ) {
        let mut saw_cmdline_default = false;
        let mut fallback_cmdline_linux = None;

        for line in content.lines() {
            if let Some((key, val)) = Self::parse_shell_assignment(line) {
                match key {
                    "GRUB_TIMEOUT" => {
                        if let Ok(t) = val.parse::<i32>() {
                            *timeout = t;
                        }
                    }
                    "GRUB_DEFAULT" => {
                        *default_entry = val;
                    }
                    "GRUB_CMDLINE_LINUX_DEFAULT" => {
                        *cmdline = val;
                        saw_cmdline_default = true;
                    }
                    "GRUB_CMDLINE_LINUX" => {
                        fallback_cmdline_linux = Some(val);
                    }
                    _ => {}
                }
            }
        }

        // On Fedora/RHEL systems GRUB_CMDLINE_LINUX is used when GRUB_CMDLINE_LINUX_DEFAULT is absent.
        if !saw_cmdline_default {
            if let Some(linux_cmd) = fallback_cmdline_linux {
                if !linux_cmd.is_empty() {
                    *cmdline = linux_cmd;
                }
            }
        }
    }

    /// Parses GRUB personalisation parameters (theme, background, colors, font, resolution, etc.).
    pub fn parse_personalisation_vars(content: &str, pers: &mut GrubPersonalisation) {
        let opt = |v: String| {
            let t = v.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        };

        for line in content.lines() {
            if let Some((key, val)) = Self::parse_shell_assignment(line) {
                match key {
                    "GRUB_THEME" => pers.theme = opt(val),
                    "GRUB_BACKGROUND" => pers.background = opt(val),
                    "GRUB_COLOR_NORMAL" => pers.color_normal = opt(val),
                    "GRUB_COLOR_HIGHLIGHT" => pers.color_highlight = opt(val),
                    "GRUB_FONT" => pers.font = opt(val),
                    "GRUB_GFXMODE" => pers.gfxmode = opt(val),
                    "GRUB_GFXPAYLOAD_LINUX" => pers.gfxpayload_linux = opt(val),
                    "GRUB_TERMINAL_OUTPUT" => pers.terminal_output = opt(val),
                    "GRUB_TIMEOUT_STYLE" => pers.timeout_style = opt(val),
                    _ => {}
                }
            }
        }
    }

    /// Discovers installed GRUB themes (`theme.txt`) across standard system paths.
    pub fn discover_themes() -> Vec<GrubThemeInfo> {
        let mut themes = Vec::new();
        let mut seen_paths = HashSet::new();

        for dir_str in GRUB_THEME_DIRS {
            let dir = Path::new(dir_str);
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let theme_dir = entry.path();
                    if theme_dir.is_dir() {
                        let theme_txt = theme_dir.join("theme.txt");
                        if theme_txt.is_file() {
                            let host_p = HostBridge::host_path(&theme_txt);
                            if seen_paths.insert(host_p.clone()) {
                                let name = entry.file_name().to_string_lossy().to_string();
                                themes.push(GrubThemeInfo { name, path: host_p });
                            }
                        }
                    }
                }
            }
        }

        themes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        themes
    }

    /// Discovers installed GRUB `.pf2` bitmap fonts across standard system paths.
    pub fn discover_fonts() -> Vec<GrubFontInfo> {
        let mut fonts = Vec::new();
        let mut seen_paths = HashSet::new();

        for dir_str in GRUB_FONT_DIRS {
            let dir = Path::new(dir_str);
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && path.extension().and_then(|e| e.to_str()) == Some("pf2")
                    {
                        let host_p = HostBridge::host_path(&path);
                        if seen_paths.insert(host_p.clone()) {
                            let name = path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
                            fonts.push(GrubFontInfo { name, path: host_p });
                        }
                    }
                }
            }
        }

        fonts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        fonts
    }

    /// Reads the complete GRUB configuration, personalisation, themes/fonts, and boot entries.
    pub fn read_config() -> GrubConfig {
        let live_status = Self::get_live_status();
        let default_path = Self::detect_default_grub_path();

        let mut timeout = 5;
        let mut default_entry = "0".to_string();
        let mut cmdline = "quiet splash".to_string();
        let mut personalisation = GrubPersonalisation::default();
        let mut default_content_raw = String::new();

        if let Ok(content) = fs::read_to_string(&default_path) {
            Self::parse_default_grub_vars(&content, &mut timeout, &mut default_entry, &mut cmdline);
            Self::parse_personalisation_vars(&content, &mut personalisation);
            default_content_raw = content;
        }

        // Parse any /etc/default/grub.d/*.cfg drop-in overrides in lexicographical order
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
                        Self::parse_default_grub_vars(
                            &content,
                            &mut timeout,
                            &mut default_entry,
                            &mut cmdline,
                        );
                        Self::parse_personalisation_vars(&content, &mut personalisation);
                    }
                }
            }
        }

        // Parse menu entries from grub.cfg, BLS entries, grub.cfg.bak, or /boot/vmlinuz
        let mut structured_entries = Vec::new();
        let cfg_path = Path::new(&live_status.cfg_path);
        let backup_path = Path::new(&live_status.backup_path);

        let cfg_content_opt = fs::read_to_string(cfg_path)
            .ok()
            .or_else(|| fs::read_to_string(backup_path).ok());

        let bls_active = Self::is_bls_enabled(&default_content_raw)
            || cfg_content_opt
                .as_ref()
                .is_some_and(|c| c.lines().any(|l| l.trim() == "blscfg"));

        if bls_active {
            structured_entries.extend(Self::parse_bls_entries());
        }

        if let Some(ref content) = cfg_content_opt {
            let parsed = Self::parse_structured_menu_entries(content);
            if structured_entries.is_empty() {
                structured_entries = parsed;
            } else {
                // Append non-BLS entries (such as UEFI Firmware Settings) with offset indices
                let base_idx = structured_entries.len();
                for (offset, mut item) in parsed.into_iter().enumerate() {
                    if !structured_entries.iter().any(|e| e.title == item.title) {
                        if item.submenu_title.is_none() {
                            item.grub_id = (base_idx + offset).to_string();
                        }
                        structured_entries.push(item);
                    }
                }
            }
        }

        if structured_entries.is_empty() {
            let kernels = Self::detect_installed_kernels();
            structured_entries = kernels
                .into_iter()
                .enumerate()
                .map(|(idx, title)| GrubMenuEntry {
                    title,
                    grub_id: idx.to_string(),
                    submenu_title: None,
                })
                .collect();
        }

        if structured_entries.is_empty() {
            structured_entries.push(GrubMenuEntry {
                title: "Default Kernel (0)".to_string(),
                grub_id: "0".to_string(),
                submenu_title: None,
            });
        }

        let menu_entries: Vec<String> = structured_entries.iter().map(|e| e.title.clone()).collect();
        let available_themes = Self::discover_themes();
        let available_fonts = Self::discover_fonts();

        GrubConfig {
            timeout,
            default_entry,
            cmdline,
            live_status,
            menu_entries,
            structured_entries,
            personalisation,
            available_themes,
            available_fonts,
        }
    }

    /// Extracts the first quoted string ('...' or "...") from a grub.cfg header line.
    fn extract_quoted_title(line: &str) -> Option<String> {
        let start = line.find(|c| c == '\'' || c == '"')?;
        let quote_char = line.as_bytes()[start] as char;
        let end = line[start + 1..].find(quote_char)?;
        let title = &line[start + 1..start + 1 + end];
        if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    }

    /// Parses `menuentry` and `submenu` blocks from grub.cfg with accurate GRUB2 indices
    /// (e.g., `"0"`, `"1>0"`, `"1>1"`, `"2"`).
    pub fn parse_structured_menu_entries(content: &str) -> Vec<GrubMenuEntry> {
        let mut entries = Vec::new();
        let mut top_idx: usize = 0;
        let mut in_submenu = false;
        let mut submenu_brace_depth: i32 = 0;
        let mut current_submenu_idx: usize = 0;
        let mut current_submenu_title = String::new();
        let mut sub_idx: usize = 0;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if !in_submenu && trimmed.starts_with("submenu ") {
                if let Some(title) = Self::extract_quoted_title(trimmed) {
                    in_submenu = true;
                    current_submenu_idx = top_idx;
                    current_submenu_title = title;
                    top_idx += 1;
                    sub_idx = 0;
                    submenu_brace_depth = count_braces(trimmed);
                    if submenu_brace_depth <= 0 {
                        in_submenu = false;
                    }
                    continue;
                }
            }

            if trimmed.starts_with("menuentry ") {
                if let Some(title) = Self::extract_quoted_title(trimmed) {
                    if in_submenu {
                        let grub_id = format!("{}>{}", current_submenu_idx, sub_idx);
                        sub_idx += 1;
                        if !entries.iter().any(|e: &GrubMenuEntry| e.title == title && e.grub_id == grub_id) {
                            entries.push(GrubMenuEntry {
                                title,
                                grub_id,
                                submenu_title: Some(current_submenu_title.clone()),
                            });
                        }
                    } else {
                        let grub_id = top_idx.to_string();
                        top_idx += 1;
                        if !entries.iter().any(|e: &GrubMenuEntry| e.title == title && e.grub_id == grub_id) {
                            entries.push(GrubMenuEntry {
                                title,
                                grub_id,
                                submenu_title: None,
                            });
                        }
                    }
                }
            }

            if in_submenu {
                submenu_brace_depth += count_braces(trimmed);
                if submenu_brace_depth <= 0 {
                    in_submenu = false;
                    submenu_brace_depth = 0;
                }
            }
        }

        entries
    }

    /// Parses flat menuentry titles from a grub.cfg content string (kept for backwards compatibility).
    pub fn parse_menu_entries(content: &str) -> Vec<String> {
        let mut entries = Vec::new();
        for item in Self::parse_structured_menu_entries(content) {
            if !entries.contains(&item.title) {
                entries.push(item.title);
            }
        }
        entries
    }

    /// Parses BootLoaderSpec (BLS) `.conf` entries from `/boot/loader/entries`.
    pub fn parse_bls_entries() -> Vec<GrubMenuEntry> {
        for candidate in ["/boot/loader/entries", "/run/host/boot/loader/entries"] {
            let dir = Path::new(candidate);
            if dir.is_dir() {
                let parsed = Self::parse_bls_entries_from_dir(dir);
                if !parsed.is_empty() {
                    return parsed;
                }
            }
        }
        Vec::new()
    }

    /// Pure testable parser for a directory of BootLoaderSpec `.conf` files.
    pub fn parse_bls_entries_from_dir(dir: &Path) -> Vec<GrubMenuEntry> {
        let mut raw_entries: Vec<(String, String)> = Vec::new();
        let Ok(read_dir) = fs::read_dir(dir) else {
            return Vec::new();
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("conf") {
                if let Ok(content) = fs::read_to_string(&path) {
                    let mut title = None;
                    let mut version = String::new();
                    for line in content.lines() {
                        let t = line.trim();
                        if let Some(rest) = t.strip_prefix("title ") {
                            title = Some(rest.trim().to_string());
                        } else if let Some(rest) = t.strip_prefix("version ") {
                            version = rest.trim().to_string();
                        }
                    }
                    if let Some(t) = title {
                        if !t.is_empty() {
                            raw_entries.push((version, t));
                        }
                    }
                }
            }
        }

        // BLS displays newest version first (descending version sort)
        raw_entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        raw_entries
            .into_iter()
            .enumerate()
            .map(|(idx, (_, title))| GrubMenuEntry {
                title,
                grub_id: idx.to_string(),
                submenu_title: None,
            })
            .collect()
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
            Self::parse_shell_assignment(line)
                .is_some_and(|(k, v)| k == "GRUB_ENABLE_BLSCFG" && v.eq_ignore_ascii_case("true"))
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
            DistroFamily::Arch
            | DistroFamily::Gentoo
            | DistroFamily::Alpine
            | DistroFamily::Unknown => &["grub-mkconfig", "grub2-mkconfig", "update-grub"],
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

        if matches!(family, DistroFamily::Fedora | DistroFamily::Mandriva)
            && has_grubby
            && bls_enabled
        {
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

    fn detect_strategy_plan_cached(cfg_exists: bool, default_path: &Path) -> GrubStrategyPlan {
        let tools = cached_host_tools();
        let default_content = fs::read_to_string(default_path).unwrap_or_default();
        let has_dropin_dir = Self::detect_default_grub_d_dir().is_some();
        let bls_enabled = Self::is_bls_enabled(&default_content);
        let has_systemd_boot = Self::detect_systemd_boot_without_grub(cfg_exists);

        Self::select_strategy_for(
            tools.family,
            tools.immutable,
            has_dropin_dir,
            tools.has_grubby,
            bls_enabled,
            cfg_exists,
            has_systemd_boot,
            tools.mkconfig_cmd.clone(),
        )
    }

    /// Probes the active host OS and bootloader capabilities to choose the safest GRUB strategy.
    pub fn detect_strategy_plan() -> GrubStrategyPlan {
        let default_path = Self::detect_default_grub_path();
        let cfg_exists = Self::detect_grub_cfg_path().exists();
        Self::detect_strategy_plan_cached(cfg_exists, &default_path)
    }

    fn check_shell_safe(field: &str, val: &str) -> Result<(), String> {
        if val.contains('\n')
            || val.contains('\r')
            || val.contains('\0')
            || val.contains('"')
            || val.contains('`')
            || val.contains("$(")
            || val.contains("${")
            || val.ends_with('\\')
        {
            return Err(format!(
                "{} contains disallowed shell or control characters (newlines, quotes, backticks, $(), ${{}}, or trailing backslash)",
                field
            ));
        }
        Ok(())
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

        if default_entry.trim().is_empty() {
            return Err("GRUB_DEFAULT cannot be empty (use 0, saved, or a menu entry ID)".to_string());
        }

        for (field, val) in [("GRUB_DEFAULT", default_entry)]
            .into_iter()
            .chain(cmdline.map(|c| ("GRUB_CMDLINE_LINUX_DEFAULT", c)))
        {
            Self::check_shell_safe(field, val)?;
        }

        Ok(())
    }

    /// Validates a GRUB2 `foreground/background` color pair against the official 16-color palette.
    pub fn validate_grub_color_pair(pair: &str) -> Result<(), String> {
        let trimmed = pair.trim();
        let Some((fg, bg)) = trimmed.split_once('/') else {
            return Err(format!(
                "Invalid GRUB color '{}': expected format 'foreground/background' (e.g. 'light-gray/black')",
                pair
            ));
        };
        if !GRUB_COLOR_PALETTE.contains(&fg) || !GRUB_COLOR_PALETTE.contains(&bg) {
            return Err(format!(
                "Invalid GRUB color '{}': foreground and background must be from GRUB's 16-color palette",
                pair
            ));
        }
        Ok(())
    }

    /// Validates all GRUB personalisation fields before persisting to `/etc/default/grub` or drop-in.
    pub fn validate_personalisation(pers: &GrubPersonalisation) -> Result<(), String> {
        if let Some(ref theme) = pers.theme {
            Self::check_shell_safe("GRUB_THEME", theme)?;
        }
        if let Some(ref bg) = pers.background {
            Self::check_shell_safe("GRUB_BACKGROUND", bg)?;
        }
        if let Some(ref font) = pers.font {
            Self::check_shell_safe("GRUB_FONT", font)?;
        }
        if let Some(ref cn) = pers.color_normal {
            Self::validate_grub_color_pair(cn)?;
        }
        if let Some(ref ch) = pers.color_highlight {
            Self::validate_grub_color_pair(ch)?;
        }
        if let Some(ref mode) = pers.gfxmode {
            let valid = !mode.trim().is_empty()
                && mode
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, 'x' | 'X' | ',' | ';' | ' '));
            if !valid {
                return Err(format!(
                    "Invalid GRUB_GFXMODE '{}': use 'auto' or WxH (e.g. '1920x1080x32')",
                    mode
                ));
            }
        }
        if let Some(ref payload) = pers.gfxpayload_linux {
            let valid = !payload.trim().is_empty()
                && payload
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, 'x' | 'X' | ','));
            if !valid {
                return Err(format!("Invalid GRUB_GFXPAYLOAD_LINUX '{}'", payload));
            }
        }
        if let Some(ref term) = pers.terminal_output {
            let valid = term
                .split_whitespace()
                .all(|tok| GRUB_TERMINAL_OUTPUTS.contains(&tok));
            if term.trim().is_empty() || !valid {
                return Err(format!(
                    "Invalid GRUB_TERMINAL_OUTPUT '{}': allowed values are {}",
                    term,
                    GRUB_TERMINAL_OUTPUTS.join(", ")
                ));
            }
        }
        if let Some(ref style) = pers.timeout_style {
            if !GRUB_TIMEOUT_STYLES.contains(&style.trim()) {
                return Err(format!(
                    "Invalid GRUB_TIMEOUT_STYLE '{}': allowed values are {}",
                    style,
                    GRUB_TIMEOUT_STYLES.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// Computes kernel command-line arguments that were removed between `old_cmdline` and `new_cmdline`.
    pub fn diff_cmdline_removed(old_cmdline: &str, new_cmdline: &str) -> Vec<String> {
        let new_tokens: HashSet<&str> = new_cmdline.split_whitespace().collect();
        let mut removed = Vec::new();
        for tok in old_cmdline.split_whitespace() {
            if !new_tokens.contains(tok) && !removed.iter().any(|r| r == tok) {
                removed.push(tok.to_string());
            }
        }
        removed
    }

    /// Builds an isolated `/etc/default/grub.d/99-mithrandir.cfg` drop-in file.
    pub fn build_dropin_content(
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
    ) -> String {
        Self::build_dropin_content_full(timeout, default_entry, cmdline, None)
    }

    /// Builds an isolated drop-in file including GRUB personalisation settings.
    pub fn build_dropin_content_full(
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
        personalisation: Option<&GrubPersonalisation>,
    ) -> String {
        let mut lines = vec![
            "# Managed by Mithrandir — delete this file and run update-grub to revert.".to_string(),
            format!("GRUB_TIMEOUT={}", timeout),
            format!("GRUB_DEFAULT=\"{}\"", default_entry),
        ];
        if default_entry.trim() == "saved" {
            lines.push("GRUB_SAVEDEFAULT=true".to_string());
        }
        if let Some(cmd) = cmdline {
            lines.push(format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"", cmd));
        }

        if let Some(p) = personalisation {
            if let Some(ref style) = p.timeout_style {
                lines.push(format!("GRUB_TIMEOUT_STYLE=\"{}\"", style));
            }
            if let Some(ref term) = p.terminal_output {
                lines.push(format!("GRUB_TERMINAL_OUTPUT=\"{}\"", term));
            }
            if let Some(ref mode) = p.gfxmode {
                lines.push(format!("GRUB_GFXMODE=\"{}\"", mode));
            }
            if let Some(ref payload) = p.gfxpayload_linux {
                lines.push(format!("GRUB_GFXPAYLOAD_LINUX=\"{}\"", payload));
            }
            if let Some(ref font) = p.font {
                lines.push(format!("GRUB_FONT=\"{}\"", font));
            }
            if let Some(ref theme) = p.theme {
                lines.push(format!("GRUB_THEME=\"{}\"", theme));
            }
            if let Some(ref bg) = p.background {
                lines.push(format!("GRUB_BACKGROUND=\"{}\"", bg));
            }
            if let Some(ref cn) = p.color_normal {
                lines.push(format!("GRUB_COLOR_NORMAL=\"{}\"", cn));
            }
            if let Some(ref ch) = p.color_highlight {
                lines.push(format!("GRUB_COLOR_HIGHLIGHT=\"{}\"", ch));
            }
        }

        let mut out = lines.join("\n");
        out.push('\n');
        out
    }

    /// Generates a backup of the active grub.cfg and /etc/default/grub in a single privileged prompt.
    pub fn create_backup() -> Result<String, String> {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();
        let default_bak = Self::get_backup_path(&default_path);

        let host_cfg = HostBridge::host_path(&cfg_path);
        let host_bak = HostBridge::host_path(&backup_path);
        let host_def = HostBridge::host_path(&default_path);
        let host_def_bak = HostBridge::host_path(&default_bak);

        let cfg_arg = if cfg_path.exists() { host_cfg.as_str() } else { "" };
        let def_arg = if default_path.exists() { host_def.as_str() } else { "" };

        if cfg_arg.is_empty() && def_arg.is_empty() {
            return Err(format!(
                "No active GRUB files found at {} or {}",
                host_cfg, host_def
            ));
        }

        const SCRIPT: &str = r#"
set -e
cfg="$1"; bak="$2"; def="$3"; def_bak="$4"
if [ -n "$cfg" ] && [ -e "$cfg" ]; then
  cp -p "$cfg" "$bak"
  chmod 644 "$bak"
fi
if [ -n "$def" ] && [ -e "$def" ]; then
  cp -p "$def" "$def_bak"
  chmod 644 "$def_bak"
fi
"#;

        let (code, stdout, stderr) = HostBridge::run_privileged(&[
            "sh",
            "-c",
            SCRIPT,
            "mithrandir-grub-backup",
            cfg_arg,
            &host_bak,
            def_arg,
            &host_def_bak,
        ])?;

        if code != 0 {
            return Err(format!("GRUB backup failed (code {}): {} {}", code, stdout, stderr)
                .trim()
                .to_string());
        }

        let mut backed_up = Vec::new();
        if !cfg_arg.is_empty() {
            backed_up.push(host_bak);
        }
        if !def_arg.is_empty() {
            backed_up.push(host_def_bak);
        }

        Ok(format!(
            "Backup successfully generated: {}",
            backed_up.join(" and ")
        ))
    }

    /// Recovers the original configuration by removing Mithrandir's drop-in and/or restoring `.bak` files
    /// in a single privileged prompt.
    pub fn restore_backup() -> Result<String, String> {
        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();
        let default_bak = Self::get_backup_path(&default_path);
        let dropin_path =
            Self::detect_default_grub_d_dir().map(|d| d.join(MITHRANDIR_DROPIN_FILENAME));

        let dropin_exists = dropin_path.as_ref().is_some_and(|p| p.exists());
        let cfg_bak_exists = backup_path.exists();
        let def_bak_exists = default_bak.exists();

        if !cfg_bak_exists && !def_bak_exists && !dropin_exists {
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
        let host_dropin = dropin_path
            .as_ref()
            .filter(|p| p.exists())
            .map(|p| HostBridge::host_path(p))
            .unwrap_or_default();

        let plan = Self::detect_strategy_plan();
        // Only regenerate grub.cfg if /etc/default/grub or drop-in changed; if ONLY grub.cfg.bak
        // was restored, running mkconfig would overwrite the restored grub.cfg!
        let need_regen = if (def_bak_exists || dropin_exists) && plan.mkconfig_cmd.is_some() {
            "1"
        } else {
            "0"
        };
        let mkcmd = plan.mkconfig_cmd.as_deref().unwrap_or("");

        const SCRIPT: &str = r#"
set -e
dropin="$1"; cfg="$2"; bak="$3"; def="$4"; def_bak="$5"; mkcmd="$6"; need_regen="$7"
if [ -n "$dropin" ] && [ -e "$dropin" ]; then
  rm -f "$dropin"
fi
if [ -n "$bak" ] && [ -e "$bak" ]; then
  cp -p "$bak" "$cfg"
fi
if [ -n "$def_bak" ] && [ -e "$def_bak" ]; then
  cp -p "$def_bak" "$def"
fi
if [ "$need_regen" = "1" ] && [ -n "$mkcmd" ]; then
  if [ "$mkcmd" = "update-grub" ]; then
    update-grub
  else
    "$mkcmd" -o "$cfg"
  fi
fi
"#;

        let (code, stdout, stderr) = HostBridge::run_privileged(&[
            "sh",
            "-c",
            SCRIPT,
            "mithrandir-grub-restore",
            &host_dropin,
            &host_cfg,
            if cfg_bak_exists { &host_bak } else { "" },
            &host_def,
            if def_bak_exists { &host_def_bak } else { "" },
            mkcmd,
            need_regen,
        ])?;

        if code != 0 {
            return Err(format!("GRUB restore failed (code {}): {} {}", code, stdout, stderr)
                .trim()
                .to_string());
        }

        let mut restored = Vec::new();
        if dropin_exists {
            restored.push(format!("removed {}", host_dropin));
        }
        if cfg_bak_exists {
            restored.push(host_bak);
        }
        if def_bak_exists {
            restored.push(host_def_bak);
        }

        Ok(format!(
            "Configuration successfully restored ({})",
            restored.join(", ")
        ))
    }

    /// Modifies core GRUB parameters using the safest distro-specific strategy and regenerates `grub.cfg`.
    pub fn save_and_update(
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
    ) -> Result<(), String> {
        Self::save_and_update_with_personalisation(timeout, default_entry, cmdline, None)
    }

    /// Modifies GRUB parameters and optional personalisation settings in a single atomic privileged operation.
    pub fn save_and_update_with_personalisation(
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
        personalisation: Option<&GrubPersonalisation>,
    ) -> Result<(), String> {
        Self::validate_grub_inputs(timeout, default_entry, cmdline)?;
        if let Some(p) = personalisation {
            Self::validate_personalisation(p)?;
        }

        let plan = Self::detect_strategy_plan();
        if plan.strategy == GrubStrategy::UnsupportedBootloader {
            return Err(format!(
                "GRUB updates are disabled on this system: {}",
                plan.label
            ));
        }

        let mkcmd = plan.mkconfig_cmd.as_deref().ok_or_else(|| {
            "No GRUB generator (update-grub, grub2-mkconfig, or grub-mkconfig) found on host"
                .to_string()
        })?;

        let cfg_path = Self::detect_grub_cfg_path();
        let backup_path = Self::get_backup_path(&cfg_path);
        let default_path = Self::detect_default_grub_path();
        let default_bak = Self::get_backup_path(&default_path);

        let host_cfg = HostBridge::host_path(&cfg_path);
        let host_bak = HostBridge::host_path(&backup_path);
        let host_def = HostBridge::host_path(&default_path);
        let host_def_bak = HostBridge::host_path(&default_bak);

        let original_default = fs::read_to_string(&default_path).unwrap_or_default();

        let (target_path, new_content, backup_def_src, run_grubby, grubby_add, grubby_rm) =
            match plan.strategy {
                GrubStrategy::DropIn => {
                    let dropins_dir = Self::detect_default_grub_d_dir()
                        .ok_or_else(|| "/etc/default/grub.d not found".to_string())?;
                    let host_dropin =
                        HostBridge::host_path(&dropins_dir.join(MITHRANDIR_DROPIN_FILENAME));
                    let content = Self::build_dropin_content_full(
                        timeout,
                        default_entry,
                        cmdline,
                        personalisation,
                    );
                    (host_dropin, content, String::new(), "0", String::new(), String::new())
                }
                GrubStrategy::FedoraBlsGrubby => {
                    let mut old_t = 5;
                    let mut old_d = String::new();
                    let mut old_cmd = String::new();
                    Self::parse_default_grub_vars(
                        &original_default,
                        &mut old_t,
                        &mut old_d,
                        &mut old_cmd,
                    );

                    let updated = Self::update_default_grub_full(
                        &original_default,
                        timeout,
                        default_entry,
                        cmdline,
                        personalisation,
                    );

                    let (add_s, rm_s) = if let Some(new_cmd) = cmdline {
                        let removed = Self::diff_cmdline_removed(&old_cmd, new_cmd);
                        (new_cmd.trim().to_string(), removed.join(" "))
                    } else {
                        (String::new(), String::new())
                    };
                    let do_grubby = if !add_s.is_empty() || !rm_s.is_empty() {
                        "1"
                    } else {
                        "0"
                    };

                    (host_def.clone(), updated, host_def.clone(), do_grubby, add_s, rm_s)
                }
                GrubStrategy::DirectDefaultGrub => {
                    let updated = Self::update_default_grub_full(
                        &original_default,
                        timeout,
                        default_entry,
                        cmdline,
                        personalisation,
                    );
                    (host_def.clone(), updated, host_def.clone(), "0", String::new(), String::new())
                }
                GrubStrategy::UnsupportedBootloader => unreachable!(),
            };

        const SCRIPT: &str = r#"
set -e
target="$1"; cfg="$2"; cfg_bak="$3"; def="$4"; def_bak="$5"; mkcmd="$6"; run_grubby="$7"; grubby_add="$8"; grubby_rm="$9"
if [ -n "$cfg" ] && [ -e "$cfg" ] && [ ! -e "$cfg_bak" ]; then
  cp -p "$cfg" "$cfg_bak"
  chmod 644 "$cfg_bak"
fi
if [ -n "$def" ] && [ -e "$def" ] && [ ! -e "$def_bak" ]; then
  cp -p "$def" "$def_bak"
  chmod 644 "$def_bak"
fi
mkdir -p "$(dirname "$target")"
tmp="${target}.mithrandir.tmp"
cat > "$tmp"
chmod 644 "$tmp"
mv -f "$tmp" "$target"
if [ "$run_grubby" = "1" ]; then
  if [ -n "$grubby_rm" ]; then
    grubby --update-kernel=ALL --remove-args="$grubby_rm"
  fi
  if [ -n "$grubby_add" ]; then
    grubby --update-kernel=ALL --args="$grubby_add"
  fi
fi
if [ "$mkcmd" = "update-grub" ]; then
  update-grub
else
  "$mkcmd" -o "$cfg"
fi
"#;

        let (code, stdout, stderr) = HostBridge::run_privileged_with_stdin(
            &[
                "sh",
                "-c",
                SCRIPT,
                "mithrandir-grub-save",
                &target_path,
                &host_cfg,
                &host_bak,
                &backup_def_src,
                &host_def_bak,
                mkcmd,
                run_grubby,
                &grubby_add,
                &grubby_rm,
            ],
            &new_content,
        )?;

        if code == 0 {
            Ok(())
        } else {
            Err(format!("GRUB update exited with code {}: {} {}", code, stdout, stderr)
                .trim()
                .to_string())
        }
    }

    /// Converts a TrueType/OpenType font into a GRUB `.pf2` bitmap font via `grub-mkfont` / `grub2-mkfont`.
    pub fn convert_font_to_pf2(
        font_file: &str,
        size_px: u32,
        output_name: &str,
    ) -> Result<String, String> {
        let font_trimmed = font_file.trim();
        if font_trimmed.is_empty() {
            return Err("Please select a .ttf or .otf source font file.".to_string());
        }
        Self::check_shell_safe("Source font path", font_trimmed)?;

        if !(8..=64).contains(&size_px) {
            return Err("Font size must be between 8 and 64 pixels.".to_string());
        }

        let clean_name: String = output_name
            .trim()
            .trim_end_matches(".pf2")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .collect();
        if clean_name.is_empty() {
            return Err("Output font name must contain alphanumeric characters.".to_string());
        }

        let mkfont_bin = if HostBridge::which("grub-mkfont").is_some() {
            "grub-mkfont"
        } else if HostBridge::which("grub2-mkfont").is_some() {
            "grub2-mkfont"
        } else {
            return Err("Neither grub-mkfont nor grub2-mkfont was found on the host.".to_string());
        };

        let out_dir = if Path::new("/boot/grub2/fonts").is_dir()
            || Path::new("/run/host/boot/grub2/fonts").is_dir()
        {
            "/boot/grub2/fonts"
        } else {
            "/boot/grub/fonts"
        };
        let out_path = format!("{}/{}-{}.pf2", out_dir, clean_name, size_px);
        let size_arg = format!("{}", size_px);

        const SCRIPT: &str = r#"
set -e
bin="$1"; size="$2"; out_dir="$3"; out_path="$4"; src="$5"
mkdir -p "$out_dir"
"$bin" -s "$size" -o "$out_path" "$src"
chmod 644 "$out_path"
"#;

        let (code, stdout, stderr) = HostBridge::run_privileged(&[
            "sh",
            "-c",
            SCRIPT,
            "mithrandir-grub-mkfont",
            mkfont_bin,
            &size_arg,
            out_dir,
            &out_path,
            font_trimmed,
        ])?;

        if code == 0 {
            Ok(out_path)
        } else {
            Err(format!("{} failed (code {}): {} {}", mkfont_bin, code, stdout, stderr)
                .trim()
                .to_string())
        }
    }

    /// Safely updates key-value pairs in /etc/default/grub content (backwards-compatible wrapper).
    pub fn update_default_grub_content(
        content: &str,
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
    ) -> String {
        Self::update_default_grub_full(content, timeout, default_entry, cmdline, None)
    }

    /// Safely updates core and personalisation keys in `/etc/default/grub` without duplicating
    /// keys when both commented (`# GRUB_...=`) and active (`GRUB_...=`) lines exist.
    pub fn update_default_grub_full(
        content: &str,
        timeout: i32,
        default_entry: &str,
        cmdline: Option<&str>,
        personalisation: Option<&GrubPersonalisation>,
    ) -> String {
        // Desired key updates: Some(rendered_line) = set/replace, None = comment out if active.
        let mut desired: Vec<(&'static str, Option<String>)> = vec![
            ("GRUB_TIMEOUT", Some(format!("GRUB_TIMEOUT={}", timeout))),
            ("GRUB_DEFAULT", Some(format!("GRUB_DEFAULT=\"{}\"", default_entry))),
        ];
        if default_entry.trim() == "saved" {
            desired.push(("GRUB_SAVEDEFAULT", Some("GRUB_SAVEDEFAULT=true".to_string())));
        }
        if let Some(new_cmd) = cmdline {
            desired.push((
                "GRUB_CMDLINE_LINUX_DEFAULT",
                Some(format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"", new_cmd)),
            ));
        }

        if let Some(p) = personalisation {
            let quoted = |k: &str, v: &Option<String>| {
                v.as_ref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("{}=\"{}\"", k, s))
            };
            desired.push(("GRUB_TIMEOUT_STYLE", quoted("GRUB_TIMEOUT_STYLE", &p.timeout_style)));
            desired.push((
                "GRUB_TERMINAL_OUTPUT",
                quoted("GRUB_TERMINAL_OUTPUT", &p.terminal_output),
            ));
            desired.push(("GRUB_GFXMODE", quoted("GRUB_GFXMODE", &p.gfxmode)));
            desired.push((
                "GRUB_GFXPAYLOAD_LINUX",
                quoted("GRUB_GFXPAYLOAD_LINUX", &p.gfxpayload_linux),
            ));
            desired.push(("GRUB_FONT", quoted("GRUB_FONT", &p.font)));
            desired.push(("GRUB_THEME", quoted("GRUB_THEME", &p.theme)));
            desired.push(("GRUB_BACKGROUND", quoted("GRUB_BACKGROUND", &p.background)));
            desired.push(("GRUB_COLOR_NORMAL", quoted("GRUB_COLOR_NORMAL", &p.color_normal)));
            desired.push((
                "GRUB_COLOR_HIGHLIGHT",
                quoted("GRUB_COLOR_HIGHLIGHT", &p.color_highlight),
            ));
        }

        // Pass 1: find which managed keys already have an active (uncommented) definition.
        let mut has_active_key: HashSet<&'static str> = HashSet::new();
        for line in content.lines() {
            if let Some((k, _)) = Self::parse_shell_assignment(line) {
                for (managed_key, _) in &desired {
                    if k == *managed_key {
                        has_active_key.insert(*managed_key);
                    }
                }
            }
        }

        // Pass 2: update lines in-place without producing duplicate active definitions.
        let mut applied_keys: HashSet<&'static str> = HashSet::new();
        let mut out_lines: Vec<String> = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim_start();
            let mut handled = false;

            // Check active assignment match first
            if let Some((k, _)) = Self::parse_shell_assignment(line) {
                if let Some((managed_key, target_val)) = desired.iter().find(|(mk, _)| *mk == k) {
                    handled = true;
                    match target_val {
                        Some(rendered) => {
                            if applied_keys.insert(*managed_key) {
                                out_lines.push(rendered.clone());
                            }
                            // Subsequent duplicate active definitions of the same key are dropped
                        }
                        None => {
                            // Personalisation option cleared: comment it out cleanly
                            out_lines.push(format!("# {}", trimmed));
                        }
                    }
                }
            }

            // If not active, check commented `#KEY=...` or `# KEY=...` ONLY when no active key exists
            if !handled && trimmed.starts_with('#') {
                let after_hash = trimmed.trim_start_matches('#').trim_start();
                for (managed_key, target_val) in &desired {
                    if let Some(rendered) = target_val {
                        if !has_active_key.contains(managed_key)
                            && !applied_keys.contains(managed_key)
                            && matches_key_prefix(after_hash, managed_key)
                        {
                            applied_keys.insert(*managed_key);
                            out_lines.push(rendered.clone());
                            handled = true;
                            break;
                        }
                    }
                }
            }

            if !handled {
                out_lines.push(line.to_string());
            }
        }

        // Pass 3: append any desired active keys that weren't present anywhere in the file.
        for (managed_key, target_val) in &desired {
            if let Some(rendered) = target_val {
                if !applied_keys.contains(managed_key) {
                    out_lines.push(rendered.clone());
                }
            }
        }

        let mut res = out_lines.join("\n");
        res.push('\n');
        res
    }
}

fn matches_key_prefix(s: &str, key: &str) -> bool {
    let stripped = s.strip_prefix("export ").map(str::trim_start).unwrap_or(s);
    stripped
        .strip_prefix(key)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

fn count_braces(line: &str) -> i32 {
    let mut delta = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    for ch in line.chars() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '{' if !in_single && !in_double => delta += 1,
            '}' if !in_single && !in_double => delta -= 1,
            _ => {}
        }
    }
    delta
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
