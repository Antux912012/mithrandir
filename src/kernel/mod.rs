pub mod bore;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelInfo {
    pub kernel_release: String,
    pub is_cachyos: bool,
    pub bore_supported: bool,
    pub bore_enabled: bool,
}

pub fn detect_kernel() -> KernelInfo {
    let kernel_release = bore::get_kernel_release();
    let is_cachyos = bore::is_cachyos_kernel(&kernel_release);
    let bore_supported = bore::is_bore_supported();
    let bore_enabled = bore::get_bore_state().unwrap_or(false);

    KernelInfo {
        kernel_release,
        is_cachyos,
        bore_supported,
        bore_enabled,
    }
}
