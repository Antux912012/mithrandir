use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use crate::cleaners::flatpak_guard::{get_running_flatpaks, is_protected_path};

/// Validates whether a given path is strictly within the allowed Flatpak cache root
/// and does NOT escape via symlinks or touch protected configuration / user data paths.
pub fn is_safe_flatpak_cache_path(path: &Path, app_cache_root: &Path) -> bool {
    // 1. Never allow deleting the app cache root folder itself
    if path == app_cache_root {
        return false;
    }

    // 2. Cache root MUST be named "cache" or flatpak-specific system cache
    let root_name = app_cache_root.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if root_name != "cache" && root_name != "flatpak" && !root_name.starts_with("flatpak-cache-") {
        return false;
    }

    // 3. Strict containment: path must start with app_cache_root
    if !path.starts_with(app_cache_root) {
        return false;
    }

    // 4. Explicitly reject any path segment matching protected Flatpak XDG locations
    let path_str = path.to_string_lossy();
    if path_str.contains("/config/") || path_str.ends_with("/config")
        || path_str.contains("/data/") || path_str.ends_with("/data")
        || path_str.contains("/.local/state")
        || path_str.contains("/.local/share")
    {
        return false;
    }

    // 5. Verify symlink safety (never follow symlinks pointing outside the cache root)
    if path.is_symlink() {
        if let Ok(target) = fs::read_link(path) {
            let resolved = if target.is_relative() {
                path.parent().unwrap_or(app_cache_root).join(&target)
            } else {
                target
            };

            let norm_resolved = match resolved.canonicalize() {
                Ok(c) => c,
                Err(_) => resolved,
            };
            let norm_root = match app_cache_root.canonicalize() {
                Ok(c) => c,
                Err(_) => app_cache_root.to_path_buf(),
            };

            if !norm_resolved.starts_with(&norm_root) {
                return false;
            }
        } else {
            return false;
        }
    }

    // 6. Signature & file type protection
    !is_protected_path(path, app_cache_root)
}

/// Safely cleans a single Flatpak application's cache directory: `~/.var/app/<app-id>/cache`.
/// Never deletes the `cache/` node itself, and strictly guards `config`, `data`, and `.local`.
pub fn clean_flatpak_app_cache(app_dir: &Path) -> Result<(u64, usize), std::io::Error> {
    let cache_dir = app_dir.join("cache");
    if !cache_dir.exists() || cache_dir.is_symlink() || !cache_dir.is_dir() {
        return Ok((0, 0));
    }

    // Crucial safety assertions: ensure cache_dir is not aliased to config/data
    assert_eq!(cache_dir.file_name().and_then(|n| n.to_str()), Some("cache"));
    if let (Ok(cfg_can), Ok(cache_can)) = (app_dir.join("config").canonicalize(), cache_dir.canonicalize()) {
        assert_ne!(cfg_can, cache_can, "Security failure: cache directory points to config!");
    }
    if let (Ok(data_can), Ok(cache_can)) = (app_dir.join("data").canonicalize(), cache_dir.canonicalize()) {
        assert_ne!(data_can, cache_can, "Security failure: cache directory points to data!");
    }

    let mut freed_bytes = 0u64;
    let mut freed_files = 0usize;

    // Iterate through children inside cache_dir (bottom-up traversal)
    for entry in WalkDir::new(&cache_dir)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .flatten()
    {
        let p = entry.path();
        if p == cache_dir {
            continue; // NEVER delete the cache directory node itself
        }

        if !is_safe_flatpak_cache_path(p, &cache_dir) {
            continue;
        }

        if entry.file_type().is_file() || entry.file_type().is_symlink() {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if fs::remove_file(p).is_ok() {
                freed_bytes += len;
                freed_files += 1;
            }
        } else if entry.file_type().is_dir() {
            // Only prune directory if it became empty
            let _ = fs::remove_dir(p);
        }
    }

    Ok((freed_bytes, freed_files))
}

/// Scans a single Flatpak application's cache directory without deleting.
pub fn scan_flatpak_app_cache(app_dir: &Path) -> (u64, usize) {
    let cache_dir = app_dir.join("cache");
    if !cache_dir.exists() || cache_dir.is_symlink() || !cache_dir.is_dir() {
        return (0, 0);
    }

    let mut total_bytes = 0u64;
    let mut total_files = 0usize;

    for entry in WalkDir::new(&cache_dir).follow_links(false).into_iter().flatten() {
        let p = entry.path();
        if p == cache_dir || !is_safe_flatpak_cache_path(p, &cache_dir) {
            continue;
        }

        if entry.file_type().is_file() || entry.file_type().is_symlink() {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            total_bytes += len;
            total_files += 1;
        }
    }

    (total_bytes, total_files)
}

/// Scans all Flatpak application caches inside `base_var_app_dir` (typically `~/.var/app`).
pub fn scan_flatpak_cache(base_var_app_dir: &Path) -> (u64, usize) {
    scan_flatpak_cache_with_running(base_var_app_dir, &get_running_flatpaks())
}

/// Scans Flatpak application caches, skipping running application IDs.
pub fn scan_flatpak_cache_with_running(base_var_app_dir: &Path, running_apps: &HashSet<String>) -> (u64, usize) {
    if !base_var_app_dir.exists() || !base_var_app_dir.is_dir() {
        return (0, 0);
    }

    let mut total_bytes = 0u64;
    let mut total_files = 0usize;

    if let Ok(entries) = fs::read_dir(base_var_app_dir) {
        for entry in entries.flatten() {
            let app_id = entry.file_name().to_string_lossy().to_string();
            if running_apps.contains(&app_id) {
                continue;
            }

            let (bytes, count) = scan_flatpak_app_cache(&entry.path());
            total_bytes += bytes;
            total_files += count;
        }
    }

    // Also scan user and system Flatpak repository caches
    let (sys_bytes, sys_count) = scan_system_flatpak_caches();
    total_bytes += sys_bytes;
    total_files += sys_count;

    (total_bytes, total_files)
}

/// Safely cleans all Flatpak application caches inside `base_var_app_dir`.
pub fn clean_flatpak_cache(base_var_app_dir: &Path) -> (u64, usize) {
    clean_flatpak_cache_with_running(base_var_app_dir, &get_running_flatpaks())
}

/// Cleans Flatpak application caches, skipping running application IDs.
pub fn clean_flatpak_cache_with_running(base_var_app_dir: &Path, running_apps: &HashSet<String>) -> (u64, usize) {
    if !base_var_app_dir.exists() || !base_var_app_dir.is_dir() {
        return (0, 0);
    }

    let mut freed_bytes = 0u64;
    let mut freed_files = 0usize;

    if let Ok(entries) = fs::read_dir(base_var_app_dir) {
        for entry in entries.flatten() {
            let app_id = entry.file_name().to_string_lossy().to_string();
            if running_apps.contains(&app_id) {
                eprintln!("Skipping running Flatpak app: {}", app_id);
                continue;
            }

            if let Ok((b, f)) = clean_flatpak_app_cache(&entry.path()) {
                freed_bytes += b;
                freed_files += f;
            }
        }
    }

    // Also clean user and system Flatpak repository caches
    let (sys_bytes, sys_count) = clean_system_flatpak_caches();
    freed_bytes += sys_bytes;
    freed_files += sys_count;

    (freed_bytes, freed_files)
}

/// Scans system/user Flatpak repository caches: `~/.cache/flatpak/` and `/var/tmp/flatpak-cache-*`.
pub fn scan_system_flatpak_caches() -> (u64, usize) {
    let mut total_bytes = 0u64;
    let mut total_files = 0usize;

    if let Ok(home) = std::env::var("HOME") {
        let user_cache = PathBuf::from(home).join(".cache/flatpak");
        if user_cache.exists() && user_cache.is_dir() {
            for entry in WalkDir::new(&user_cache).follow_links(false).into_iter().flatten() {
                if entry.file_type().is_file() || entry.file_type().is_symlink() {
                    total_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                    total_files += 1;
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir("/var/tmp") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("flatpak-cache-") {
                let p = entry.path();
                for e in WalkDir::new(&p).follow_links(false).into_iter().flatten() {
                    if e.file_type().is_file() || e.file_type().is_symlink() {
                        total_bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
                        total_files += 1;
                    }
                }
            }
        }
    }

    (total_bytes, total_files)
}

/// Cleans system/user Flatpak repository caches: `~/.cache/flatpak/` and `/var/tmp/flatpak-cache-*`.
pub fn clean_system_flatpak_caches() -> (u64, usize) {
    let mut freed_bytes = 0u64;
    let mut freed_files = 0usize;

    if let Ok(home) = std::env::var("HOME") {
        let user_cache = PathBuf::from(home).join(".cache/flatpak");
        if user_cache.exists() && user_cache.is_dir() {
            for entry in WalkDir::new(&user_cache).contents_first(true).follow_links(false).into_iter().flatten() {
                let p = entry.path();
                if p == user_cache {
                    continue;
                }
                if entry.file_type().is_file() || entry.file_type().is_symlink() {
                    let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    if fs::remove_file(p).is_ok() {
                        freed_bytes += len;
                        freed_files += 1;
                    }
                } else if entry.file_type().is_dir() {
                    let _ = fs::remove_dir(p);
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir("/var/tmp") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("flatpak-cache-") {
                let p = entry.path();
                for e in WalkDir::new(&p).contents_first(true).follow_links(false).into_iter().flatten() {
                    let sub = e.path();
                    if sub == p {
                        continue;
                    }
                    if e.file_type().is_file() || e.file_type().is_symlink() {
                        let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                        if fs::remove_file(sub).is_ok() {
                            freed_bytes += len;
                            freed_files += 1;
                        }
                    } else if e.file_type().is_dir() {
                        let _ = fs::remove_dir(sub);
                    }
                }
            }
        }
    }

    (freed_bytes, freed_files)
}

/// Helper that resolves default Flatpak cache directory: `~/.var/app`.
pub fn get_default_var_app_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".var/app")
}

pub fn scan_default_flatpak_cache() -> (u64, usize) {
    scan_flatpak_cache(&get_default_var_app_dir())
}

pub fn clean_default_flatpak_cache() -> (u64, usize) {
    clean_flatpak_cache(&get_default_var_app_dir())
}
