use std::path::Path;
use mithrandir::backend::grub::GrubManager;

#[test]
fn test_grub_cfg_and_backup_path_resolution() {
    let cfg_path = GrubManager::detect_grub_cfg_path();
    assert!(!cfg_path.to_string_lossy().is_empty());

    let default_path = GrubManager::detect_default_grub_path();
    assert!(!default_path.to_string_lossy().is_empty());

    let test_path = Path::new("/boot/grub/grub.cfg");
    let bak_path = GrubManager::get_backup_path(test_path);
    assert_eq!(bak_path, Path::new("/boot/grub/grub.cfg.bak"));

    let test_efi = Path::new("/boot/efi/EFI/fedora/grub.cfg");
    let bak_efi = GrubManager::get_backup_path(test_efi);
    assert_eq!(bak_efi, Path::new("/boot/efi/EFI/fedora/grub.cfg.bak"));

    let test_default = Path::new("/etc/default/grub");
    let bak_default = GrubManager::get_backup_path(test_default);
    assert_eq!(bak_default, Path::new("/etc/default/grub.bak"));
}

#[test]
fn test_grub_live_status_collection() {
    let status = GrubManager::get_live_status();
    assert!(!status.cfg_path.is_empty());
    assert!(!status.backup_path.is_empty());
    assert!(status.backup_path.ends_with(".bak"));

    assert!(!status.default_path.is_empty());
    assert!(!status.default_backup_path.is_empty());
    assert!(status.default_backup_path.ends_with(".bak"));

    let cfg = GrubManager::read_config();
    assert!(cfg.timeout >= 0);
    assert!(!cfg.default_entry.is_empty());
    assert!(!cfg.live_status.cfg_path.is_empty());
    assert!(!cfg.live_status.default_path.is_empty());
}

#[test]
fn test_parse_menu_entries_from_grub_cfg() {
    let sample_cfg = r#"
# Sample GRUB configuration
set default="0"
set timeout=5

menuentry 'Debian GNU/Linux' --class debian --class gnu-linux --class gnu --class os $menuentry_id_option 'gnulinux-simple-123' {
    load_video
    insmod gzio
}

submenu 'Advanced options for Debian GNU/Linux' $menuentry_id_option 'gnulinux-advanced-123' {
    menuentry 'Debian GNU/Linux, with Linux 6.12.0' --class debian --class gnu-linux {
        echo 'Loading Linux...'
    }
    menuentry "Debian GNU/Linux, with Linux 6.12.0 (recovery mode)" --class debian {
        echo 'Loading recovery...'
    }
}

menuentry "UEFI Firmware Settings" $menuentry_id_option 'uefi-firmware' {
    fwsetup
}
"#;

    let entries = GrubManager::parse_menu_entries(sample_cfg);
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0], "Debian GNU/Linux");
    assert_eq!(entries[1], "Debian GNU/Linux, with Linux 6.12.0");
    assert_eq!(entries[2], "Debian GNU/Linux, with Linux 6.12.0 (recovery mode)");
    assert_eq!(entries[3], "UEFI Firmware Settings");
}

#[test]
fn test_update_default_grub_content() {
    let initial = r#"# Initial GRUB file
GRUB_DEFAULT=0
GRUB_TIMEOUT=0
GRUB_DISTRIBUTOR="Debian"
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash"
GRUB_CMDLINE_LINUX=""
"#;

    let updated = GrubManager::update_default_grub_content(
        initial,
        10,
        "2",
        Some("quiet splash mitigations=off"),
    );

    assert!(updated.contains("GRUB_TIMEOUT=10"));
    assert!(updated.contains("GRUB_DEFAULT=\"2\""));
    assert!(updated.contains("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash mitigations=off\""));
    assert!(updated.contains("GRUB_DISTRIBUTOR=\"Debian\""));

    // Test inserting missing keys
    let partial = "GRUB_DISTRIBUTOR=\"Custom\"\n";
    let populated = GrubManager::update_default_grub_content(partial, 7, "saved", None);
    assert!(populated.contains("GRUB_TIMEOUT=7"));
    assert!(populated.contains("GRUB_DEFAULT=\"saved\""));
    assert!(populated.contains("GRUB_DISTRIBUTOR=\"Custom\""));
}

#[test]
fn test_distro_aware_grub_strategy_selection() {
    use mithrandir::backend::distro::DistroFamily;
    use mithrandir::backend::grub::GrubStrategy;
    use mithrandir::os_info::{parse_os_release, LinuxDistro};

    // Gentoo & OpenMandriva detection
    let gentoo = parse_os_release("ID=gentoo\nPRETTY_NAME=\"Gentoo Linux\"\n");
    assert_eq!(gentoo.distro, LinuxDistro::Gentoo);
    assert_eq!(DistroFamily::from(gentoo.distro), DistroFamily::Gentoo);

    let mandriva = parse_os_release("ID=openmandriva\nPRETTY_NAME=\"OpenMandriva Lx\"\n");
    assert_eq!(mandriva.distro, LinuxDistro::Mandriva);
    assert_eq!(DistroFamily::from(mandriva.distro), DistroFamily::Mandriva);

    // Debian/Ubuntu with /etc/default/grub.d -> DropIn
    let deb = GrubManager::select_strategy_for(
        DistroFamily::Debian,
        false,
        true,
        false,
        false,
        true,
        false,
        Some("update-grub".to_string()),
    );
    assert_eq!(deb.strategy, GrubStrategy::DropIn);

    // Fedora with BLS + grubby -> FedoraBlsGrubby
    let fed = GrubManager::select_strategy_for(
        DistroFamily::Fedora,
        false,
        false,
        true,
        true,
        true,
        false,
        Some("grub2-mkconfig".to_string()),
    );
    assert_eq!(fed.strategy, GrubStrategy::FedoraBlsGrubby);

    // Arch / Gentoo / openSUSE -> DirectDefaultGrub
    for fam in [DistroFamily::Arch, DistroFamily::Gentoo, DistroFamily::Suse] {
        let plan = GrubManager::select_strategy_for(
            fam,
            false,
            false,
            false,
            false,
            true,
            false,
            Some("grub-mkconfig".to_string()),
        );
        assert_eq!(plan.strategy, GrubStrategy::DirectDefaultGrub);
    }

    // Immutable OS or systemd-boot without GRUB -> UnsupportedBootloader
    let imm = GrubManager::select_strategy_for(
        DistroFamily::Unknown,
        true,
        false,
        false,
        false,
        false,
        false,
        None,
    );
    assert_eq!(imm.strategy, GrubStrategy::UnsupportedBootloader);

    let sd_boot = GrubManager::select_strategy_for(
        DistroFamily::Arch,
        false,
        false,
        false,
        false,
        false,
        true,
        None,
    );
    assert_eq!(sd_boot.strategy, GrubStrategy::UnsupportedBootloader);
}

#[test]
fn test_grub_input_validation_and_dropin_and_host_path() {
    use mithrandir::backend::host_bridge::HostBridge;

    assert!(GrubManager::validate_grub_inputs(5, "0", Some("quiet splash")).is_ok());
    assert!(GrubManager::validate_grub_inputs(-1, "saved", None).is_ok());
    assert!(GrubManager::validate_grub_inputs(4000, "0", None).is_err());
    assert!(GrubManager::validate_grub_inputs(5, "0\" ; rm -rf /", None).is_err());
    assert!(GrubManager::validate_grub_inputs(5, "0", Some("quiet $(id)")).is_err());
    assert!(GrubManager::validate_grub_inputs(5, "0", Some("quiet\nsplash")).is_err());

    let dropin = GrubManager::build_dropin_content(5, "0", Some("quiet splash"));
    assert!(dropin.contains("GRUB_TIMEOUT=5"));
    assert!(dropin.contains("GRUB_DEFAULT=\"0\""));
    assert!(dropin.contains("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\""));

    assert_eq!(
        HostBridge::host_path(Path::new("/run/host/etc/default/grub")),
        "/etc/default/grub"
    );
    assert_eq!(
        HostBridge::host_path(Path::new("/boot/grub/grub.cfg")),
        "/boot/grub/grub.cfg"
    );
}
