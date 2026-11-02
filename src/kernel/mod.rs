pub mod bore;
pub mod scx;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelInfo {
    pub kernel_release: String,
    pub is_cachyos: bool,
    pub bore_supported: bool,
    pub bore_enabled: bool,
    pub scx_supported: bool,
    pub scx_active_scheduler: String,
    pub scx_mode: u32,
    pub supported_scx_schedulers: Vec<String>,
}

pub fn detect_kernel() -> KernelInfo {
    let kernel_release = bore::get_kernel_release();
    let is_cachyos = bore::is_cachyos_kernel(&kernel_release);
    let bore_supported = bore::is_bore_supported();
    let bore_enabled = bore::get_bore_state().unwrap_or(false);
    let scx_supported = scx::is_scx_supported();
    let scx_active_scheduler = scx::get_active_scheduler();
    let (scx_mode, supported_scx_schedulers) = scx::get_scx_info();

    KernelInfo {
        kernel_release,
        is_cachyos,
        bore_supported,
        bore_enabled,
        scx_supported,
        scx_active_scheduler,
        scx_mode,
        supported_scx_schedulers,
    }
}
