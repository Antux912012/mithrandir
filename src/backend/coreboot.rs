use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::backend::host_bridge::HostBridge;
use crate::hardware::resolve_sys_path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorebootCmosOption {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorebootStatus {
    pub detected: bool,
    pub variant: String,
    pub bios_vendor: String,
    pub bios_version: String,
    pub bios_date: String,
    pub has_sysfs_table: bool,
    pub payload_hint: String,
    pub has_nvramtool: bool,
    pub has_cbfstool: bool,
    pub has_cbmem: bool,
    pub has_flashrom: bool,
    pub cmos_options: Vec<CorebootCmosOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorebootPersonalisationConfig {
    pub bootsplash_image: Option<String>,
    pub resolution: String,
    pub color_normal: String,
    pub color_highlight: String,
    pub font_path: Option<String>,
    pub timeout: i32,
}

impl Default for CorebootPersonalisationConfig {
    fn default() -> Self {
        Self {
            bootsplash_image: None,
            resolution: "1920x1080".to_string(),
            color_normal: "light-gray/black".to_string(),
            color_highlight: "white/blue".to_string(),
            font_path: None,
            timeout: 5,
        }
    }
}

pub struct CorebootManager;

impl CorebootManager {
    /// Pure helper to classify Coreboot firmware and payload hints from DMI strings and sysfs presence.
    pub fn classify_from_dmi(
        bios_vendor: &str,
        bios_version: &str,
        has_sysfs_table: bool,
    ) -> (bool, String, String) {
        let vendor_l = bios_vendor.to_lowercase();
        let version_l = bios_version.to_lowercase();

        let is_coreboot = has_sysfs_table
            || vendor_l.contains("coreboot")
            || vendor_l.contains("libreboot")
            || vendor_l.contains("mrchromebox")
            || vendor_l.contains("dasharo")
            || vendor_l.contains("star labs")
            || vendor_l.contains("system76")
            || version_l.contains("coreboot")
            || version_l.contains("mrchromebox")
            || version_l.contains("dasharo")
            || version_l.contains("libreboot");

        let variant = if !is_coreboot {
            "Standard UEFI / Legacy BIOS".to_string()
        } else if vendor_l.contains("dasharo") || version_l.contains("dasharo") {
            "Dasharo (Coreboot)".to_string()
        } else if vendor_l.contains("mrchromebox") || version_l.contains("mrchromebox") {
            "MrChromebox (Coreboot)".to_string()
        } else if vendor_l.contains("libreboot") || version_l.contains("libreboot") {
            "Libreboot (Coreboot)".to_string()
        } else if vendor_l.contains("system76") || version_l.contains("system76") {
            "System76 Open Firmware (Coreboot)".to_string()
        } else if vendor_l.contains("star labs") || version_l.contains("starlabs") {
            "Star Labs (Coreboot)".to_string()
        } else {
            "Coreboot Open-Source Firmware".to_string()
        };

        let payload_hint = if !is_coreboot {
            if Path::new("/sys/firmware/efi").exists() || Path::new("/run/host/sys/firmware/efi").exists() {
                "Vendor UEFI".to_string()
            } else {
                "Legacy BIOS".to_string()
            }
        } else if version_l.contains("edk2")
            || version_l.contains("uefi")
            || version_l.contains("mrchromebox")
            || Path::new("/sys/firmware/efi").exists()
        {
            "EDK2 / Tianocore UEFI Payload".to_string()
        } else if version_l.contains("grub") || vendor_l.contains("libreboot") {
            "GRUB2 Native Payload".to_string()
        } else if version_l.contains("heads") {
            "Heads Verified Boot Payload".to_string()
        } else if version_l.contains("seabios") {
            "SeaBIOS Legacy Payload".to_string()
        } else {
            "Coreboot Payload (Inspect via cbmem/cbfstool)".to_string()
        };

        (is_coreboot, variant, payload_hint)
    }

    /// Probes the active host for Coreboot firmware, utilities, and live CMOS options.
    pub fn detect() -> CorebootStatus {
        let bios_vendor = read_dmi_field("/sys/class/dmi/id/bios_vendor")
            .unwrap_or_else(|| "Unknown".to_string());
        let bios_version = read_dmi_field("/sys/class/dmi/id/bios_version")
            .unwrap_or_else(|| "Unknown".to_string());
        let bios_date = read_dmi_field("/sys/class/dmi/id/bios_date")
            .unwrap_or_else(|| "Unknown".to_string());

        let has_sysfs_table = resolve_sys_path("/sys/firmware/coreboot").exists();
        let (detected, variant, payload_hint) =
            Self::classify_from_dmi(&bios_vendor, &bios_version, has_sysfs_table);

        let has_nvramtool = HostBridge::which("nvramtool").is_some();
        let has_cbfstool = HostBridge::which("cbfstool").is_some();
        let has_cbmem = HostBridge::which("cbmem").is_some();
        let has_flashrom = HostBridge::which("flashrom").is_some();

        let cmos_options = if detected && has_nvramtool {
            HostBridge::run_command("nvramtool", &["-a"])
                .ok()
                .filter(|(code, _, _)| *code == 0)
                .map(|(_, stdout, _)| Self::parse_nvramtool_output(&stdout))
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        CorebootStatus {
            detected,
            variant,
            bios_vendor,
            bios_version,
            bios_date,
            has_sysfs_table,
            payload_hint,
            has_nvramtool,
            has_cbfstool,
            has_cbmem,
            has_flashrom,
            cmos_options,
        }
    }

    /// Parses `nvramtool -a` output (`key = value`) into structured CMOS options.
    pub fn parse_nvramtool_output(stdout: &str) -> Vec<CorebootCmosOption> {
        let mut options = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let key = k.trim().to_string();
                let value = v.trim().to_string();
                if !key.is_empty() && !value.is_empty() {
                    options.push(CorebootCmosOption { key, value });
                }
            }
        }
        options
    }

    /// Validates a Coreboot CMOS option key and value before passing to `nvramtool -w`.
    pub fn validate_cmos_option(key: &str, value: &str) -> Result<(), String> {
        let valid_token = |s: &str| {
            !s.is_empty()
                && s.len() <= 64
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
        };
        if !valid_token(key) || key.contains(' ') {
            return Err(format!("Invalid Coreboot CMOS key '{}'", key));
        }
        if !valid_token(value) {
            return Err(format!("Invalid Coreboot CMOS value '{}'", value));
        }
        Ok(())
    }

    /// Updates a Coreboot CMOS NVRAM option using `nvramtool -w key=value`.
    pub fn set_cmos_option(key: &str, value: &str) -> Result<String, String> {
        Self::validate_cmos_option(key, value)?;
        let status = Self::detect();
        if !status.detected {
            return Err("Coreboot firmware was not detected on this machine; refusing to modify CMOS NVRAM.".to_string());
        }
        if !status.has_nvramtool {
            return Err("nvramtool is not installed on the host.".to_string());
        }

        let assignment = format!("{}={}", key, value);
        let (code, stdout, stderr) = HostBridge::run_privileged(&["nvramtool", "-w", &assignment])?;
        if code == 0 {
            Ok(format!("Updated Coreboot CMOS option {} = {}", key, value))
        } else {
            Err(format!("nvramtool exited with code {}: {} {}", code, stdout, stderr).trim().to_string())
        }
    }

    /// Generates a Coreboot GRUB2 payload personalisation snippet (`grub.cfg` / `grubtest.cfg`).
    pub fn generate_coreboot_grub_payload_cfg(cfg: &CorebootPersonalisationConfig) -> String {
        let mut lines = vec![
            "# Coreboot GRUB2 Payload Personalisation — Generated by Mithrandir".to_string(),
            "# Embed into CBFS as etc/grub.cfg or source from /boot/grub/grub.cfg".to_string(),
            String::new(),
            format!("set timeout={}", cfg.timeout.clamp(-1, 3600)),
            format!("set gfxmode={}", if cfg.resolution.trim().is_empty() { "auto" } else { cfg.resolution.trim() }),
            "set gfxpayload=keep".to_string(),
            "insmod all_video".to_string(),
            "insmod gfxterm".to_string(),
            "insmod png".to_string(),
            "insmod jpeg".to_string(),
            "terminal_output gfxterm".to_string(),
        ];

        if let Some(ref font) = cfg.font_path {
            let trimmed = font.trim();
            if !trimmed.is_empty() {
                lines.push(format!("loadfont {}", trimmed));
            }
        }

        if !cfg.color_normal.trim().is_empty() {
            lines.push(format!("set menu_color_normal={}", cfg.color_normal.trim()));
            lines.push(format!("set color_normal={}", cfg.color_normal.trim()));
        }
        if !cfg.color_highlight.trim().is_empty() {
            lines.push(format!("set menu_color_highlight={}", cfg.color_highlight.trim()));
            lines.push(format!("set color_highlight={}", cfg.color_highlight.trim()));
        }

        if let Some(ref img) = cfg.bootsplash_image {
            let trimmed = img.trim();
            if !trimmed.is_empty() {
                lines.push("insmod gfxterm_background".to_string());
                lines.push(format!("background_image -m stretch {}", trimmed));
            }
        }

        let mut out = lines.join("\n");
        out.push('\n');
        out
    }

    /// Generates safe, reviewable CLI commands to prepare a baseline 4:2:0 JPEG bootsplash
    /// and stage it into a backed-up Coreboot ROM image with `cbfstool` (never auto-flashes SPI).
    pub fn generate_cbfs_bootsplash_commands(
        rom_path: &str,
        image_path: &str,
        resolution: &str,
    ) -> Vec<String> {
        let rom = if rom_path.trim().is_empty() { "coreboot-working.rom" } else { rom_path.trim() };
        let img = if image_path.trim().is_empty() { "bootsplash.png" } else { image_path.trim() };
        let res = if resolution.trim().is_empty() || resolution.trim() == "auto" {
            "1920x1080"
        } else {
            resolution.trim()
        };

        vec![
            "# 1. Back up your current SPI flash ROM before modifying anything:".to_string(),
            "sudo flashrom -p internal -r coreboot-backup.rom && cp coreboot-backup.rom coreboot-working.rom".to_string(),
            "# 2. Convert image to Coreboot-compatible baseline JPEG (4:2:0 chroma subsampling, non-progressive):".to_string(),
            format!(
                "magick '{}' -resize '{}!' -sampling-factor 4:2:0 -interlace none -quality 90 bootsplash.jpg",
                img.replace('\'', "'\\''"),
                res.replace('\'', "")
            ),
            "# 3. Remove any previous bootsplash from CBFS (ignore error if absent) and add the new one:".to_string(),
            format!("cbfstool '{}' remove -n bootsplash.jpg 2>/dev/null || true", rom.replace('\'', "'\\''")),
            format!("cbfstool '{}' add -f bootsplash.jpg -n bootsplash.jpg -t bootsplash", rom.replace('\'', "'\\''")),
            "# 4. Verify CBFS layout before flashing (ensure you have an external recovery programmer):".to_string(),
            format!("cbfstool '{}' print", rom.replace('\'', "'\\''")),
        ]
    }
}

fn read_dmi_field(path_str: &str) -> Option<String> {
    let p = resolve_sys_path(path_str);
    fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
