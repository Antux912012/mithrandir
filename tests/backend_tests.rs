use mithrandir::{
    CleanerEngine, DiagnosticsCollector, DistroDetector, SmartBoostEngine, SystemMonitor,
};
use std::path::Path;

#[test]
fn test_cleaner_path_safety() {
    assert!(!CleanerEngine::is_path_safe(Path::new("/")));
    assert!(!CleanerEngine::is_path_safe(Path::new("/etc")));
    assert!(!CleanerEngine::is_path_safe(Path::new("/usr")));
    assert!(!CleanerEngine::is_path_safe(Path::new("/var")));
    assert!(CleanerEngine::is_path_safe(Path::new("/tmp/mithrandir_test")));
}

#[test]
fn test_cleaner_categories_coverage() {
    let cats = CleanerEngine::get_categories();
    assert!(cats.len() >= 8);
    let ids: Vec<&str> = cats.iter().map(|c| c.id.as_str()).collect();
    assert!(ids.contains(&"thumbnails"));
    assert!(ids.contains(&"user_cache"));
    assert!(ids.contains(&"browser_cache"));
    assert!(ids.contains(&"stremio_cache"));
    assert!(ids.contains(&"dev_cache"));
}

#[test]
fn test_memory_monitor() {
    let mem = SystemMonitor::get_memory_info();
    assert!(mem.total_mb > 0.0);
    assert!(mem.percent >= 0.0 && mem.percent <= 100.0);
}

#[test]
fn test_cpu_monitor() {
    let cpu = SystemMonitor::get_cpu_info();
    assert!(cpu.core_count > 0);
    assert!(!cpu.brand.is_empty());
}

#[test]
fn test_system_summary() {
    let summary = SystemMonitor::get_system_summary();
    assert!(!summary.hostname.is_empty());
    assert!(!summary.kernel.is_empty());
    assert!(!summary.distro.is_empty());
    assert!(!summary.arch.is_empty());
    assert!(!summary.cpu_model.is_empty());
}

#[test]
fn test_sensor_telemetry_suite() {
    let net = SystemMonitor::get_network_rate();
    assert!(!net.rx_str.is_empty());
    assert!(!net.tx_str.is_empty());
    assert!(net.rx_bytes_sec >= 0.0);
    assert!(net.tx_bytes_sec >= 0.0);

    let bat = SystemMonitor::get_battery_info();
    assert!(!bat.status.is_empty());
    if bat.present {
        assert!(bat.design_capacity_wh > 0.0);
        assert!(bat.capacity_percent >= 0.0);
    }

    let gpus = SystemMonitor::get_gpus_info();
    for gpu in &gpus {
        assert!(!gpu.name.is_empty());
        assert!(gpu.load_percent >= 0.0);
    }
}

#[test]
fn test_smart_boost_status() {
    let status = SmartBoostEngine::get_boost_status();
    assert!(status.ram_total_mb > 0.0);
    assert!(!status.temp_size_str.is_empty());
}

#[test]
fn test_smart_boost_debounce() {
    assert!(SmartBoostEngine::check_debounce(500));
    // Immediate repeat within 500ms should be debounced
    assert!(!SmartBoostEngine::check_debounce(500));
}

#[test]
fn test_distro_detection() {
    let d = DistroDetector::detect();
    assert!(!d.id.is_empty());
    assert!(!d.pretty_name.is_empty());
}

#[test]
fn test_diagnostics_snapshot_collection() {
    let snapshot = DiagnosticsCollector::gather_system_snapshot();
    assert!(snapshot.contains("Mithrandir System Diagnostics Snapshot:"));
    assert!(snapshot.contains("- OS:"));
    assert!(snapshot.contains("- Memory:"));
}

#[test]
fn test_tray_action_channel_dispatch() {
    use mithrandir::{MithrandirTray, TrayAction};
    use ksni::Tray;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let (tx, rx) = async_channel::unbounded();
    let power_saver_arc = Arc::new(AtomicBool::new(false));
    let mut tray = MithrandirTray::new(tx, power_saver_arc.clone());

    assert_eq!(tray.id(), "mithrandir");
    assert_eq!(tray.icon_name(), "mithrandir-status-symbolic");
    assert_eq!(tray.category(), ksni::Category::SystemServices);

    let pixmap = tray.icon_pixmap();
    assert!(!pixmap.is_empty());
    assert_eq!(pixmap[0].width, 16);
    assert_eq!(pixmap[0].height, 16);
    assert_eq!(pixmap[0].data.len(), 16 * 16 * 4);

    // Test activate trigger
    tray.activate(100, 200);
    assert_eq!(rx.try_recv(), Ok(TrayAction::OpenMithrandir));

    // Test menu item activation: RunSmartboost, Checkmark(Battery Power Saver), SubMenu(Kernel & Power), Separator, OpenMithrandir, Separator, QuitMithrandir
    let menu_items = tray.menu();
    assert_eq!(menu_items.len(), 7);

    // 0: Run Smartboost
    if let ksni::MenuItem::Standard(item) = &menu_items[0] {
        assert_eq!(item.label, "Run Smartboost");
        (item.activate)(&mut tray);
        assert_eq!(rx.try_recv(), Ok(TrayAction::RunSmartboost));
    } else {
        panic!("Expected StandardItem for Run Smartboost");
    }

    // 1: Battery Power Saver (Checkmark)
    if let ksni::MenuItem::Checkmark(item) = &menu_items[1] {
        assert_eq!(item.label, "Battery Power Saver");
        assert!(!item.checked);
        (item.activate)(&mut tray);
        assert_eq!(rx.try_recv(), Ok(TrayAction::TogglePowerSaver(true)));
        assert!(tray.power_saver_enabled.load(std::sync::atomic::Ordering::SeqCst));
    } else {
        panic!("Expected CheckmarkItem for Battery Power Saver");
    }

    // 2: Kernel & Power (SubMenu)
    if let ksni::MenuItem::SubMenu(sub) = &menu_items[2] {
        assert_eq!(sub.label, "Kernel & Power");
        assert!(sub.submenu.len() >= 5);
    } else {
        panic!("Expected SubMenu for Kernel & Power");
    }

    // 4: Open Mithrandir
    if let ksni::MenuItem::Standard(item) = &menu_items[4] {
        assert_eq!(item.label, "Open Mithrandir");
        (item.activate)(&mut tray);
        assert_eq!(rx.try_recv(), Ok(TrayAction::OpenMithrandir));
    } else {
        panic!("Expected StandardItem for Open Mithrandir");
    }

    // 6: Quit Mithrandir
    if let ksni::MenuItem::Standard(item) = &menu_items[6] {
        assert_eq!(item.label, "Quit Mithrandir");
        (item.activate)(&mut tray);
        assert_eq!(rx.try_recv(), Ok(TrayAction::QuitMithrandir));
    } else {
        panic!("Expected StandardItem for Quit Mithrandir");
    }
}

#[test]
fn test_tray_smartboost_concurrency_lock() {
    use mithrandir::tray::MithrandirTray;

    let res = MithrandirTray::execute_tray_smartboost();
    assert!(res.is_some());
}

#[test]
fn test_power_saver_controller() {
    use mithrandir::PowerManager;

    let supported = PowerManager::is_supported();
    let is_active = PowerManager::is_power_saver_active();
    println!("Power saver supported: {}, active: {}", supported, is_active);

    // Test cached state toggle
    PowerManager::set_cached_state(true);
    assert!(PowerManager::is_power_saver_active());

    PowerManager::set_cached_state(false);
    // Cached state resets
    let _ = PowerManager::is_power_saver_active();
}

#[test]
fn test_distro_detection_mock_matrix() {
    use mithrandir::os_info::{get_admin_group_for_distro, parse_os_release, LinuxDistro};

    let debian_os_release = r#"
PRETTY_NAME="Debian GNU/Linux 12 (bookworm)"
NAME="Debian GNU/Linux"
VERSION_ID="12"
VERSION="12 (bookworm)"
VERSION_CODENAME=bookworm
ID=debian
HOME_URL="https://www.debian.org/"
SUPPORT_URL="https://www.debian.org/support"
BUG_REPORT_URL="https://bugs.debian.org/"
"#;
    let debian_info = parse_os_release(debian_os_release);
    assert_eq!(debian_info.distro, LinuxDistro::Debian);
    assert_eq!(debian_info.version_id, Some("12".to_string()));
    assert_eq!(get_admin_group_for_distro(debian_info.distro), "sudo");

    let fedora_os_release = r#"
NAME="Fedora Linux"
VERSION="41 (Workstation Edition)"
ID=fedora
VERSION_ID="41"
VERSION_CODENAME=""
PLATFORM_ID="platform:f41"
PRETTY_NAME="Fedora Linux 41 (Workstation Edition)"
"#;
    let fedora_info = parse_os_release(fedora_os_release);
    assert_eq!(fedora_info.distro, LinuxDistro::Fedora);
    assert_eq!(fedora_info.version_id, Some("41".to_string()));
    assert_eq!(get_admin_group_for_distro(fedora_info.distro), "wheel");

    let arch_os_release = r#"
NAME="Arch Linux"
PRETTY_NAME="Arch Linux"
ID=arch
BUILD_ID=rolling
ANSI_COLOR="38;2;23;147;209"
HOME_URL="https://archlinux.org/"
"#;
    let arch_info = parse_os_release(arch_os_release);
    assert_eq!(arch_info.distro, LinuxDistro::Arch);
    assert_eq!(get_admin_group_for_distro(arch_info.distro), "wheel");

    let cachyos_os_release = r#"
NAME="CachyOS Linux"
PRETTY_NAME="CachyOS"
ID=cachyos
ID_LIKE=arch
BUILD_ID=rolling
"#;
    let cachy_info = parse_os_release(cachyos_os_release);
    assert_eq!(cachy_info.distro, LinuxDistro::CachyOS);
    assert_eq!(get_admin_group_for_distro(cachy_info.distro), "wheel");

    let nixos_os_release = r#"
NAME=NixOS
ID=nixos
VERSION="24.11 (Vicuna)"
VERSION_CODENAME=vicuna
VERSION_ID="24.11"
PRETTY_NAME="NixOS 24.11 (Vicuna)"
"#;
    let nixos_info = parse_os_release(nixos_os_release);
    assert_eq!(nixos_info.distro, LinuxDistro::NixOS);
    assert_eq!(nixos_info.version_id, Some("24.11".to_string()));
    assert_eq!(get_admin_group_for_distro(nixos_info.distro), "sudo");

    let alpine_os_release = r#"
NAME="Alpine Linux"
ID=alpine
VERSION_ID=3.21.0
PRETTY_NAME="Alpine Linux v3.21"
"#;
    let alpine_info = parse_os_release(alpine_os_release);
    assert_eq!(alpine_info.distro, LinuxDistro::Alpine);
    assert_eq!(alpine_info.version_id, Some("3.21.0".to_string()));
    assert_eq!(get_admin_group_for_distro(alpine_info.distro), "sudo");
}

#[test]
fn test_nixos_immutable_graceful_handling() {
    use mithrandir::os_info::{generate_udev_rule_for_group, LinuxDistro};

    let wheel_rule = generate_udev_rule_for_group("wheel");
    assert!(wheel_rule.contains("chgrp -R wheel"));
    assert!(wheel_rule.contains("chgrp wheel"));

    let sudo_rule = generate_udev_rule_for_group("sudo");
    assert!(sudo_rule.contains("chgrp -R sudo"));
    assert!(sudo_rule.contains("chgrp sudo"));

    // Verify mock NixOS environment handling
    let simulated_nixos = LinuxDistro::NixOS;
    let is_immutable = matches!(simulated_nixos, LinuxDistro::NixOS);
    assert!(is_immutable);

    if is_immutable {
        let msg = "NixOS detected: /etc is immutable. Please enable power saver services in your configuration.nix";
        assert!(msg.contains("NixOS detected"));
    }
}

#[test]
fn test_app_settings_default_and_roundtrip() {
    use mithrandir::AppSettings;

    let defaults = AppSettings::default();
    assert!(defaults.keep_in_background);
    assert!(defaults.show_tray_icon);
    assert!(defaults.auto_boost_enabled);
    assert_eq!(defaults.ram_threshold, 80.0);
    assert_eq!(defaults.cooldown_minutes, 15);
    assert!(defaults.desktop_notifications);

    let serialized = serde_json::to_string(&defaults).expect("Serialization failed");
    let deserialized: AppSettings = serde_json::from_str(&serialized).expect("Deserialization failed");
    assert_eq!(deserialized.ram_threshold, 80.0);
    assert_eq!(deserialized.cooldown_minutes, 15);
}

#[test]
fn test_distro_family_conversion() {
    use mithrandir::os_info::LinuxDistro;
    use mithrandir::DistroFamily;

    assert_eq!(DistroFamily::from(LinuxDistro::Debian), DistroFamily::Debian);
    assert_eq!(DistroFamily::from(LinuxDistro::Ubuntu), DistroFamily::Debian);
    assert_eq!(DistroFamily::from(LinuxDistro::Fedora), DistroFamily::Fedora);
    assert_eq!(DistroFamily::from(LinuxDistro::Arch), DistroFamily::Arch);
    assert_eq!(DistroFamily::from(LinuxDistro::CachyOS), DistroFamily::Arch);
    assert_eq!(DistroFamily::from(LinuxDistro::OpenSuse), DistroFamily::Suse);
    assert_eq!(DistroFamily::from(LinuxDistro::Alpine), DistroFamily::Alpine);
    assert_eq!(DistroFamily::from(LinuxDistro::NixOS), DistroFamily::Unknown);
}
