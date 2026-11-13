use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use crate::backend::host_bridge::HostBridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanCategory {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub file_count: usize,
    pub size_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanResult {
    pub freed_bytes: u64,
    pub freed_mb: f64,
    pub freed_files: usize,
}

pub struct CleanerEngine;

impl CleanerEngine {
    pub fn is_path_safe(target: &Path) -> bool {
        let canonical = match target.canonicalize() {
            Ok(c) => c,
            Err(_) => target.to_path_buf(),
        };

        let forbidden: &[&str] = &[
            "/", "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/lib64",
            "/opt", "/proc", "/root", "/run", "/sbin", "/sys", "/usr", "/var",
        ];

        for f in forbidden {
            if canonical == Path::new(f) {
                return false;
            }
        }

        if let Ok(home) = std::env::var("HOME") {
            if canonical == Path::new(&home) {
                return false;
            }
        }

        true
    }

    pub fn scan_path(path: &Path) -> (u64, usize) {
        Self::scan_path_excluding(path, &[])
    }

    pub fn scan_path_excluding(path: &Path, exclude_subdirs: &[&str]) -> (u64, usize) {
        if !path.exists() {
            return (0, 0);
        }

        WalkDir::new(path)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 1 && e.file_type().is_dir() {
                    let fname = e.file_name().to_string_lossy();
                    !exclude_subdirs.contains(&fname.as_ref())
                } else {
                    true
                }
            })
            .par_bridge()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .fold(|| (0u64, 0usize), |(s, c), len| (s + len, c + 1))
            .reduce(|| (0, 0), |(s1, c1), (s2, c2)| (s1 + s2, c1 + c2))
    }

    pub const PKG_CACHE_DIRS: &'static [&'static str] = &[
        "/var/cache/libdnf5",
        "/var/cache/dnf",
        "/var/cache/apt/archives",
        "/var/cache/pacman/pkg",
    ];

    pub fn get_category_definitions() -> Vec<CleanCategory> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let home_p = PathBuf::from(&home);

        let pkg_cache_path = Self::PKG_CACHE_DIRS
            .iter()
            .map(Path::new)
            .find(|p| p.exists())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/var/cache"));

        let cats = [
            ("thumbnails", "Thumbnail Cache", "Cached image and video thumbnails", home_p.join(".cache/thumbnails")),
            ("user_cache", "User Application Cache", "Application cache files in ~/.cache (excluding browsers, dev, thumbnails)", home_p.join(".cache")),
            ("browser_cache", "Web Browser Caches", "Firefox, Chrome, Chromium, Brave, and Edge caches", home_p.join(".cache/mozilla")),
            ("stremio_cache", "Stremio Streaming Cache", "Torrent parts and media stream cache", home_p.join(".config/stremio/stremio-cache")),
            ("dev_cache", "Developer & Package Caches", "Pip, Cargo, and npm package cache directories", home_p.join(".cache/pip")),
            ("flatpak_cache", "Flatpak Applications Cache", "Safe application caches in ~/.var/app/*/cache (preserves configs, profiles & running apps)", home_p.join(".var/app")),
            ("snap_cache", "Snap Package Cache", "Snap cache data in ~/snap", home_p.join("snap")),
            ("trash", "Trash Bin", "Deleted files in user Trash", home_p.join(".local/share/Trash")),
            ("tmp", "Temporary Files", "Volatile user files in /tmp", PathBuf::from("/tmp")),
            ("package_cache", "Package Manager Cache", "Cached APT, DNF, or Pacman package archives", pkg_cache_path),
        ];

        cats.into_iter()
            .map(|(id, name, desc, path)| CleanCategory {
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                path,
                size_bytes: 0,
                file_count: 0,
                size_mb: 0.0,
            })
            .collect()
    }

    fn browser_cache_dirs(home: &Path) -> [PathBuf; 5] {
        [
            home.join(".cache/mozilla"),
            home.join(".cache/google-chrome"),
            home.join(".cache/chromium"),
            home.join(".cache/BraveSoftware"),
            home.join(".cache/microsoft-edge"),
        ]
    }

    fn dev_cache_dirs(home: &Path) -> [PathBuf; 3] {
        [
            home.join(".cache/pip"),
            home.join(".npm/_cacache"),
            home.join(".cargo/registry/cache"),
        ]
    }

    fn stremio_cache_dirs(home: &Path) -> [PathBuf; 2] {
        [
            home.join(".config/stremio/stremio-cache"),
            PathBuf::from("/tmp/stremio-cache"),
        ]
    }

    /// Returns only `.cache` directories inside `~/snap/<app>/<rev>/.cache` (including `common/.cache`),
    /// preserving all Snap application user configs and profiles.
    pub fn snap_cache_dirs(snap_root: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let Ok(apps) = fs::read_dir(snap_root) else {
            return dirs;
        };
        for app_entry in apps.flatten() {
            let app_path = app_entry.path();
            if !app_path.is_dir() {
                continue;
            }
            let Ok(revs) = fs::read_dir(&app_path) else {
                continue;
            };
            for rev_entry in revs.flatten() {
                let cache_dir = rev_entry.path().join(".cache");
                if cache_dir.is_dir() {
                    dirs.push(cache_dir);
                }
            }
        }
        dirs
    }

    fn sum_dirs(dirs: &[PathBuf]) -> (u64, usize) {
        dirs.iter().fold((0u64, 0usize), |(sb, sc), p| {
            let (b, c) = Self::scan_path(p);
            (sb + b, sc + c)
        })
    }

    fn clean_dirs(dirs: &[PathBuf]) -> (u64, usize) {
        dirs.iter().fold((0u64, 0usize), |(sb, sc), p| {
            let (b, c) = Self::clean_path(p);
            (sb + b, sc + c)
        })
    }

    pub fn get_categories() -> Vec<CleanCategory> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let home_ref = PathBuf::from(&home);

        Self::get_category_definitions()
            .into_par_iter()
            .map(|mut cat| {
                let (size_bytes, file_count) = match cat.id.as_str() {
                    "tmp" => Self::scan_user_tmp(),
                    "user_cache" => {
                        let exclusions = ["thumbnails", "mozilla", "google-chrome", "chromium", "BraveSoftware", "microsoft-edge", "pip"];
                        Self::scan_path_excluding(&cat.path, &exclusions)
                    }
                    "browser_cache" => Self::sum_dirs(&Self::browser_cache_dirs(&home_ref)),
                    "stremio_cache" => Self::sum_dirs(&Self::stremio_cache_dirs(&home_ref)),
                    "dev_cache" => Self::sum_dirs(&Self::dev_cache_dirs(&home_ref)),
                    "flatpak_cache" => crate::cleaners::scan_flatpak_cache(&cat.path),
                    "snap_cache" => Self::sum_dirs(&Self::snap_cache_dirs(&cat.path)),
                    "package_cache" => {
                        let pkg_dirs: Vec<PathBuf> = Self::PKG_CACHE_DIRS
                            .iter()
                            .map(PathBuf::from)
                            .filter(|p| p.exists())
                            .collect();
                        Self::sum_dirs(&pkg_dirs)
                    }
                    _ => Self::scan_path(&cat.path),
                };

                cat.size_bytes = size_bytes;
                cat.file_count = file_count;
                cat.size_mb = ((size_bytes as f64 / (1024.0 * 1024.0)) * 10.0).round() / 10.0;
                cat
            })
            .collect()
    }

    pub fn scan_user_tmp() -> (u64, usize) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let current_uid = unsafe { libc::getuid() };
            let tmp = Path::new("/tmp");
            if !tmp.exists() {
                return (0, 0);
            }

            WalkDir::new(tmp)
                .min_depth(1)
                .into_iter()
                .par_bridge()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    if let Ok(meta) = e.path().symlink_metadata() {
                        meta.is_file() && meta.uid() == current_uid
                    } else {
                        false
                    }
                })
                .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
                .fold(|| (0u64, 0usize), |(s, c), len| (s + len, c + 1))
                .reduce(|| (0, 0), |(s1, c1), (s2, c2)| (s1 + s2, c1 + c2))
        }
        #[cfg(not(unix))]
        {
            (0, 0)
        }
    }

    pub fn clean_path(path: &Path) -> (u64, usize) {
        Self::clean_path_excluding(path, &[])
    }

    pub fn clean_path_excluding(path: &Path, exclude_subdirs: &[&str]) -> (u64, usize) {
        if !Self::is_path_safe(path) || !path.exists() {
            return (0, 0);
        }

        let files: Vec<(PathBuf, u64)> = WalkDir::new(path)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 1 && e.file_type().is_dir() {
                    let fname = e.file_name().to_string_lossy();
                    !exclude_subdirs.contains(&fname.as_ref())
                } else {
                    true
                }
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| {
                let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                (e.into_path(), len)
            })
            .collect();

        files
            .into_par_iter()
            .map(|(f, size)| {
                if fs::remove_file(&f).is_ok() {
                    (size, 1usize)
                } else {
                    (0, 0)
                }
            })
            .reduce(|| (0, 0), |(s1, c1), (s2, c2)| (s1 + s2, c1 + c2))
    }

    pub fn clean_user_tmp() -> (u64, usize) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let current_uid = unsafe { libc::getuid() };
            let tmp = Path::new("/tmp");
            let files: Vec<PathBuf> = WalkDir::new(tmp)
                .min_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    if let Ok(meta) = e.path().symlink_metadata() {
                        meta.is_file() && meta.uid() == current_uid
                    } else {
                        false
                    }
                })
                .map(|e| e.path().to_path_buf())
                .collect();

            files
                .into_par_iter()
                .map(|f| {
                    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                    if fs::remove_file(&f).is_ok() {
                        (size, 1usize)
                    } else {
                        (0, 0)
                    }
                })
                .reduce(|| (0, 0), |(s1, c1), (s2, c2)| (s1 + s2, c1 + c2))
        }
        #[cfg(not(unix))]
        {
            (0, 0)
        }
    }

    pub fn clean_category(cat_id: &str) -> CleanResult {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let home_p = PathBuf::from(&home);

        let (bytes, count) = match cat_id {
            "thumbnails" => Self::clean_path(&home_p.join(".cache/thumbnails")),
            "user_cache" => {
                let exclusions = ["thumbnails", "mozilla", "google-chrome", "chromium", "BraveSoftware", "microsoft-edge", "pip"];
                Self::clean_path_excluding(&home_p.join(".cache"), &exclusions)
            }
            "browser_cache" => Self::clean_dirs(&Self::browser_cache_dirs(&home_p)),
            "stremio_cache" => Self::clean_dirs(&Self::stremio_cache_dirs(&home_p)),
            "dev_cache" => Self::clean_dirs(&Self::dev_cache_dirs(&home_p)),
            "flatpak_cache" => crate::cleaners::clean_flatpak_cache(&home_p.join(".var/app")),
            "snap_cache" => Self::clean_dirs(&Self::snap_cache_dirs(&home_p.join("snap"))),
            "trash" => Self::clean_path(&home_p.join(".local/share/Trash")),
            "tmp" => Self::clean_user_tmp(),
            "package_cache" => {
                let pkg_dirs: Vec<PathBuf> = Self::PKG_CACHE_DIRS
                    .iter()
                    .map(PathBuf::from)
                    .filter(|p| p.exists())
                    .collect();
                let (before_b, before_c) = Self::sum_dirs(&pkg_dirs);
                let _ = HostBridge::run_privileged(&["sh", "-c", "apt-get clean 2>/dev/null || dnf clean all 2>/dev/null || pacman -Scc --noconfirm 2>/dev/null || true"]);
                let _ = HostBridge::run_privileged(&["journalctl", "--vacuum-time=7d"]);
                let (after_b, after_c) = Self::sum_dirs(&pkg_dirs);
                (before_b.saturating_sub(after_b), before_c.saturating_sub(after_c))
            }
            _ => (0, 0),
        };

        CleanResult {
            freed_bytes: bytes,
            freed_mb: ((bytes as f64 / (1024.0 * 1024.0)) * 10.0).round() / 10.0,
            freed_files: count,
        }
    }
}
