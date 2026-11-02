pub mod backend;
pub mod cleaners;
pub mod tray;
pub mod power;
pub mod kernel;
pub mod os_info;
pub mod ai;

#[cfg(feature = "gui")]
pub mod ui;

pub use ai::{
    AIClient, AIOptimizerEngine, BuiltinLinuxAssistant, ChatMessage, ChatResponse,
    GgufModelInfo, LlamaCppStatus, LocalLLMManager, OptimizationRecommendation,
    OptimizationReport, SafetyGuard, get_recommended_nemotron_models,
};

pub use tray::{MithrandirTray, TrayAction};
pub use power::{
    apply_kernel_state_privileged, apply_preset, get_active_preset, set_active_preset, PowerManager,
    PowerPreset,
};
pub use kernel::{detect_kernel, KernelInfo};
pub use os_info::{detect_os, get_admin_group, LinuxDistro, OsDetails};
pub use cleaners::{
    clean_default_flatpak_cache, clean_flatpak_app_cache, clean_flatpak_cache,
    clean_flatpak_cache_with_running, clean_system_flatpak_caches, get_default_var_app_dir,
    get_running_flatpaks, is_protected_path, is_safe_flatpak_cache_path,
    scan_default_flatpak_cache, scan_flatpak_app_cache, scan_flatpak_cache,
    scan_flatpak_cache_with_running, scan_system_flatpak_caches,
};

pub use backend::{
    AppSettings, AutostartManager, BackgroundDaemon, BatteryInfo, BtrfsManager, CleanCategory,
    CleanerEngine, DiagnosticsCollector, DistroDetector, DistroFamily, GrubManager,
    HostBridge, NotificationManager, PackageManager, ProcessManager, ServiceManager,
    SmartBoostEngine, SystemMonitor,
};
