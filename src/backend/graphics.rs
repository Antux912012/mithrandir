use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicsCapabilities {
    pub vulkan_available: bool,
    pub opengl_available: bool,
    pub dri_device_found: bool,
    pub card_name: String,
    pub active_renderer: String,
}

pub struct GraphicsManager;

impl GraphicsManager {
    /// Detects if Vulkan ICD drivers or runtime libraries are present.
    pub fn is_vulkan_available() -> bool {
        let icd_paths = [
            "/usr/share/vulkan/icd.d",
            "/etc/vulkan/icd.d",
            "/run/host/usr/share/vulkan/icd.d",
        ];

        for path in &icd_paths {
            if let Ok(entries) = std::fs::read_dir(path) {
                if entries.filter_map(Result::ok).any(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "json")
                        .unwrap_or(false)
                }) {
                    return true;
                }
            }
        }

        Path::new("/usr/lib/x86_64-linux-gnu/libvulkan.so.1").exists()
            || Path::new("/usr/lib64/libvulkan.so.1").exists()
            || Path::new("/usr/lib/libvulkan.so.1").exists()
    }

    /// Detects if OpenGL / DRM hardware acceleration devices exist.
    pub fn is_opengl_available() -> bool {
        Path::new("/dev/dri/renderD128").exists() || Path::new("/dev/dri/card0").exists()
    }

    /// Scans graphics capabilities on the host.
    pub fn detect_capabilities() -> GraphicsCapabilities {
        let vulkan = Self::is_vulkan_available();
        let opengl = Self::is_opengl_available();
        let dri = Path::new("/dev/dri").is_dir();

        let mut card_name = "Unknown Graphics Device".to_string();
        for card_idx in [0, 1] {
            let vendor_path = format!("/sys/class/drm/card{}/device/vendor", card_idx);
            if let Ok(vendor_str) = std::fs::read_to_string(&vendor_path) {
                let v = vendor_str.trim().to_lowercase();
                if v.contains("0x8086") {
                    card_name = "Intel Graphics (Mesa)".to_string();
                    break;
                } else if v.contains("0x10de") {
                    card_name = "NVIDIA GPU".to_string();
                    break;
                } else if v.contains("0x1002") {
                    card_name = "AMD Radeon (Mesa)".to_string();
                    break;
                }
            }
        }

        let active_renderer = std::env::var("GSK_RENDERER").unwrap_or_else(|_| "auto".to_string());

        GraphicsCapabilities {
            vulkan_available: vulkan,
            opengl_available: opengl,
            dri_device_found: dri,
            card_name,
            active_renderer,
        }
    }

    /// Applies GSK renderer environment variable for GTK4 scene graph rendering.
    pub fn apply_acceleration_settings(hw_accel_enabled: bool, preferred_renderer: &str) {
        if !hw_accel_enabled || preferred_renderer == "cairo" || preferred_renderer == "software" {
            // Force CPU software rendering (Cairo)
            std::env::set_var("GSK_RENDERER", "cairo");
            return;
        }

        match preferred_renderer {
            "vulkan" => {
                if Self::is_vulkan_available() {
                    std::env::set_var("GSK_RENDERER", "vulkan");
                } else if Self::is_opengl_available() {
                    std::env::set_var("GSK_RENDERER", "gl");
                }
            }
            "gl" | "opengl" => {
                if Self::is_opengl_available() {
                    std::env::set_var("GSK_RENDERER", "gl");
                }
            }
            _ => {
                // Auto detection: prefer Vulkan if ICDs present, fallback to OpenGL
                if Self::is_vulkan_available() {
                    std::env::set_var("GSK_RENDERER", "vulkan");
                } else if Self::is_opengl_available() {
                    std::env::set_var("GSK_RENDERER", "gl");
                } else {
                    std::env::set_var("GSK_RENDERER", "cairo");
                }
            }
        }
    }
}
