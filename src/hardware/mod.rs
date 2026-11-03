pub mod battery;
pub mod display;
pub mod memory;
pub mod network;
pub mod storage;
pub mod system;

use std::path::{Path, PathBuf};

pub use battery::BatteryInfo as HardwareBatteryInfo;
pub use display::DisplayInfo;
pub use memory::MemoryInfo as HardwareMemoryInfo;
pub use network::NetworkCardInfo;
pub use storage::DiskInfo as HardwareDiskInfo;
pub use system::SystemInfo;

pub fn resolve_sys_path(path_str: &str) -> PathBuf {
    let p = Path::new(path_str);
    if p.exists() {
        return p.to_path_buf();
    }
    let host_p = Path::new("/run/host").join(path_str.trim_start_matches('/'));
    if host_p.exists() {
        return host_p;
    }
    p.to_path_buf()
}
