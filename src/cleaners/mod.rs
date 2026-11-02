pub mod flatpak_guard;
pub mod flatpak;

pub use flatpak_guard::{get_running_flatpaks, is_protected_path, STRICT_PROTECTED_APP_SUBDIRS};
pub use flatpak::{
    clean_default_flatpak_cache, clean_flatpak_app_cache, clean_flatpak_cache,
    clean_flatpak_cache_with_running, clean_system_flatpak_caches, get_default_var_app_dir,
    is_safe_flatpak_cache_path, scan_default_flatpak_cache, scan_flatpak_app_cache,
    scan_flatpak_cache, scan_flatpak_cache_with_running, scan_system_flatpak_caches,
};
