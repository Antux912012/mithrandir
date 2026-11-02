use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use tempfile::tempdir;
use mithrandir::{
    clean_flatpak_app_cache, clean_flatpak_cache, clean_flatpak_cache_with_running,
    is_safe_flatpak_cache_path, scan_flatpak_cache,
};

#[test]
fn test_flatpak_safe_cleaning_preserves_zen_and_profiles() {
    let temp = tempdir().expect("Failed to create temporary directory");
    let var_app = temp.path();

    let zen_app_dir = var_app.join("app.zen_browser.zen");
    let zen_profile_dir = zen_app_dir.join(".zen/Default (release)");
    fs::create_dir_all(&zen_profile_dir).expect("Failed to create mock .zen directory");

    let prefs_path = zen_profile_dir.join("prefs.js");
    fs::write(&prefs_path, "user_pref('browser.startup.homepage', 'about:blank');").unwrap();

    let sessions_path = zen_profile_dir.join("zen-sessions.jsonlz4");
    fs::write(&sessions_path, b"mozLz40\0mock_session_stream").unwrap();

    let places_path = zen_profile_dir.join("places.sqlite");
    fs::write(&places_path, b"SQLite format 3\0mock_history_database").unwrap();

    // Cache structure
    let cache_dir = zen_app_dir.join("cache");
    let cache_entries_dir = cache_dir.join("zen/Default (release)/cache2/entries");
    fs::create_dir_all(&cache_entries_dir).expect("Failed to create mock cache entries directory");

    let dummy_cache_file = cache_entries_dir.join("dummy_cache.bin");
    fs::write(&dummy_cache_file, b"volatile_network_cache_block_12345").unwrap();

    // Symlink inside cache/ pointing outside to .zen/.../places.sqlite
    let symlink_path = cache_dir.join("symlink_to_places");
    symlink("../.zen/Default (release)/places.sqlite", &symlink_path).unwrap();

    // Internal protected file misplaced inside cache/
    let misplaced_db = cache_dir.join("cookies.sqlite");
    fs::write(&misplaced_db, b"SQLite format 3\0mock_cookies").unwrap();

    // Protected backup directory inside cache/
    let backups_dir = cache_dir.join("bookmarkbackups");
    fs::create_dir_all(&backups_dir).unwrap();
    let backup_file = backups_dir.join("bookmarks.jsonlz4");
    fs::write(&backup_file, b"mozLz40\0mock_bookmarks").unwrap();

    // Scan should only count un-protected files (dummy_cache.bin)
    let (scanned_bytes, scanned_count) = scan_flatpak_cache(var_app);
    assert_eq!(scanned_count, 1, "Only dummy_cache.bin should be marked for deletion");
    assert_eq!(scanned_bytes, b"volatile_network_cache_block_12345".len() as u64);

    // Run clean
    let (freed_bytes, freed_count) = clean_flatpak_cache(var_app);
    assert_eq!(freed_count, 1);
    assert_eq!(freed_bytes, b"volatile_network_cache_block_12345".len() as u64);

    // Assertions:
    // 1. dummy_cache.bin MUST be deleted
    assert!(!dummy_cache_file.exists(), "dummy_cache.bin MUST be deleted by cleaner");

    // 2. Profile files MUST remain intact and untouched
    assert!(prefs_path.exists(), "prefs.js MUST remain intact");
    assert_eq!(
        fs::read_to_string(&prefs_path).unwrap(),
        "user_pref('browser.startup.homepage', 'about:blank');"
    );

    assert!(sessions_path.exists(), "zen-sessions.jsonlz4 MUST remain intact");
    assert_eq!(fs::read(&sessions_path).unwrap(), b"mozLz40\0mock_session_stream");

    assert!(places_path.exists(), "places.sqlite MUST remain intact");
    assert_eq!(fs::read(&places_path).unwrap(), b"SQLite format 3\0mock_history_database");

    // 3. Symlink pointing outside MUST be preserved
    assert!(symlink_path.is_symlink(), "Symlink pointing outside cache MUST be preserved");

    // 4. Misplaced protected files & directories inside cache MUST be preserved
    assert!(misplaced_db.exists(), "cookies.sqlite in cache must be preserved by signature filter");
    assert!(backup_file.exists(), "bookmarkbackups in cache must be preserved by directory filter");

    // 5. The cache directory itself MUST still exist!
    assert!(cache_dir.exists(), "The cache directory node itself must not be deleted");
}

#[test]
fn test_flatpak_never_touches_config_data_or_local_state() {
    let temp = tempdir().expect("Failed to create tempdir");
    let app_dir = temp.path().join("org.mozilla.Firefox");

    // Flatpak XDG Directory hierarchy
    let cache_dir = app_dir.join("cache");
    let config_dir = app_dir.join("config");
    let data_dir = app_dir.join("data");
    let state_dir = app_dir.join(".local/state");
    let custom_persist_dir = app_dir.join(".pki");

    fs::create_dir_all(&cache_dir).unwrap();
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(&state_dir).unwrap();
    fs::create_dir_all(&custom_persist_dir).unwrap();

    // Critical persistent files
    let config_file = config_dir.join("user-settings.json");
    fs::write(&config_file, b"{\"theme\": \"nord\", \"vault\": true}").unwrap();

    let data_file = data_dir.join("accounts.db");
    fs::write(&data_file, b"critical_credentials_db").unwrap();

    let state_file = state_dir.join("window-state.v1");
    fs::write(&state_file, b"x=10,y=20").unwrap();

    let custom_file = custom_persist_dir.join("cert.db");
    fs::write(&custom_file, b"crypto_key_cert").unwrap();

    // Non-protected cache file
    let cache_file = cache_dir.join("fontconfig-cache.bin");
    fs::write(&cache_file, b"temporary_fontconfig_cache").unwrap();

    // Execute app cache clean
    let res = clean_flatpak_app_cache(&app_dir);
    assert!(res.is_ok());

    let (freed_bytes, freed_count) = res.unwrap();
    assert_eq!(freed_count, 1, "Only fontconfig-cache.bin must be cleaned");
    assert_eq!(freed_bytes, b"temporary_fontconfig_cache".len() as u64);

    // Verify cache file deleted, but cache directory node remains
    assert!(!cache_file.exists());
    assert!(cache_dir.exists(), "cache directory must remain present");

    // Verify all protected XDG dirs and files remain untouched
    assert!(config_file.exists(), "config file MUST NEVER be touched");
    assert!(data_file.exists(), "data file MUST NEVER be touched");
    assert!(state_file.exists(), ".local/state file MUST NEVER be touched");
    assert!(custom_file.exists(), "custom persist dir MUST NEVER be touched");

    assert_eq!(fs::read(&config_file).unwrap(), b"{\"theme\": \"nord\", \"vault\": true}");
    assert_eq!(fs::read(&data_file).unwrap(), b"critical_credentials_db");
    assert_eq!(fs::read(&state_file).unwrap(), b"x=10,y=20");
    assert_eq!(fs::read(&custom_file).unwrap(), b"crypto_key_cert");
}

#[test]
fn test_flatpak_skip_running_application() {
    let temp = tempdir().expect("Failed to create temporary directory");
    let var_app = temp.path();

    let firefox_dir = var_app.join("org.mozilla.firefox");
    let cache_dir = firefox_dir.join("cache/entries");
    fs::create_dir_all(&cache_dir).unwrap();
    let cache_file = cache_dir.join("active_session_cache.tmp");
    fs::write(&cache_file, b"active_firefox_session_data").unwrap();

    let mut running = HashSet::new();
    running.insert("org.mozilla.firefox".to_string());

    let (freed_bytes, freed_count) = clean_flatpak_cache_with_running(var_app, &running);
    assert_eq!(freed_count, 0, "Running applications must be skipped");
    assert_eq!(freed_bytes, 0);
    assert!(cache_file.exists(), "Cache of running application must not be deleted");
}

#[test]
fn test_is_safe_flatpak_cache_path() {
    let cache_root = Path::new("/home/user/.var/app/org.mozilla.firefox/cache");

    // Root itself is NOT safe to delete
    assert!(!is_safe_flatpak_cache_path(cache_root, cache_root));

    // Paths outside cache root
    assert!(!is_safe_flatpak_cache_path(Path::new("/home/user/.var/app/org.mozilla.firefox/config"), cache_root));
    assert!(!is_safe_flatpak_cache_path(Path::new("/home/user/.var/app/org.mozilla.firefox/data"), cache_root));
    assert!(!is_safe_flatpak_cache_path(Path::new("/home/user/.var/app/org.mozilla.firefox/.local/state"), cache_root));
    assert!(!is_safe_flatpak_cache_path(Path::new("/home/user/.var/app/org.mozilla.firefox"), cache_root));

    // Normal volatile cache file inside cache root
    assert!(is_safe_flatpak_cache_path(&cache_root.join("mesa_shader_cache.bin"), cache_root));
    assert!(is_safe_flatpak_cache_path(&cache_root.join("thumbnails/normal/icon.png"), cache_root));

    // Protected signatures inside cache root
    assert!(!is_safe_flatpak_cache_path(&cache_root.join("cookies.sqlite"), cache_root));
    assert!(!is_safe_flatpak_cache_path(&cache_root.join("prefs.js"), cache_root));
    assert!(!is_safe_flatpak_cache_path(&cache_root.join("bookmarkbackups/data.bin"), cache_root));
}
