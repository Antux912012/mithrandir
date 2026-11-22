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
    assert!(cfg.timeout >= -1);
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

#[test]
fn test_structured_submenu_indices_and_bls_entries() {
    let sample_cfg = r#"
menuentry 'Debian GNU/Linux' --class debian {
    load_video
}
submenu 'Advanced options for Debian GNU/Linux' {
    menuentry 'Debian GNU/Linux, with Linux 6.12.0' {
        echo 'Loading Linux...'
    }
    menuentry "Debian GNU/Linux, with Linux 6.12.0 (recovery mode)" {
        echo 'Loading recovery...'
    }
}
menuentry "UEFI Firmware Settings" {
    fwsetup
}
"#;
    let structured = GrubManager::parse_structured_menu_entries(sample_cfg);
    assert_eq!(structured.len(), 4);
    assert_eq!(structured[0].grub_id, "0");
    assert_eq!(structured[0].submenu_title, None);
    assert_eq!(structured[1].grub_id, "1>0");
    assert_eq!(
        structured[1].submenu_title.as_deref(),
        Some("Advanced options for Debian GNU/Linux")
    );
    assert_eq!(structured[2].grub_id, "1>1");
    assert_eq!(structured[3].grub_id, "2");
    assert_eq!(structured[3].title, "UEFI Firmware Settings");

    // Mock BLS directory
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("entry-old.conf"),
        "title Fedora Linux (6.11.0)\nversion 6.11.0\nlinux /vmlinuz-6.11.0\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("entry-new.conf"),
        "title Fedora Linux (6.12.4)\nversion 6.12.4\nlinux /vmlinuz-6.12.4\n",
    )
    .unwrap();
    let bls = GrubManager::parse_bls_entries_from_dir(tmp.path());
    assert_eq!(bls.len(), 2);
    assert_eq!(bls[0].title, "Fedora Linux (6.12.4)");
    assert_eq!(bls[0].grub_id, "0");
    assert_eq!(bls[1].title, "Fedora Linux (6.11.0)");
    assert_eq!(bls[1].grub_id, "1");
}

#[test]
fn test_grub_parsing_no_duplicates_and_personalisation() {
    use mithrandir::backend::grub::GrubPersonalisation;

    // Inline comments, export prefix, and GRUB_CMDLINE_LINUX fallback
    let messy = r#"
# GRUB_TIMEOUT=5
export GRUB_TIMEOUT=12 # custom delay
GRUB_DEFAULT='1>0'
GRUB_CMDLINE_LINUX="rhgb quiet"
GRUB_THEME="/boot/grub/themes/nord/theme.txt"
GRUB_COLOR_NORMAL="light-gray/black"
GRUB_COLOR_HIGHLIGHT="white/blue"
GRUB_GFXMODE="1920x1080x32"
GRUB_GFXPAYLOAD_LINUX="keep"
GRUB_TERMINAL_OUTPUT="gfxterm"
GRUB_FONT="/boot/grub/fonts/unicode.pf2"
GRUB_TIMEOUT_STYLE="menu"
"#;

    let mut t = 0;
    let mut d = String::new();
    let mut c = String::new();
    GrubManager::parse_default_grub_vars(messy, &mut t, &mut d, &mut c);
    assert_eq!(t, 12);
    assert_eq!(d, "1>0");
    assert_eq!(c, "rhgb quiet");

    let mut pers = GrubPersonalisation::default();
    GrubManager::parse_personalisation_vars(messy, &mut pers);
    assert_eq!(pers.theme.as_deref(), Some("/boot/grub/themes/nord/theme.txt"));
    assert_eq!(pers.color_normal.as_deref(), Some("light-gray/black"));
    assert_eq!(pers.color_highlight.as_deref(), Some("white/blue"));
    assert_eq!(pers.gfxmode.as_deref(), Some("1920x1080x32"));
    assert_eq!(pers.gfxpayload_linux.as_deref(), Some("keep"));
    assert_eq!(pers.terminal_output.as_deref(), Some("gfxterm"));
    assert_eq!(pers.font.as_deref(), Some("/boot/grub/fonts/unicode.pf2"));
    assert_eq!(pers.timeout_style.as_deref(), Some("menu"));
    assert!(GrubManager::validate_personalisation(&pers).is_ok());

    // Updating a file with both `# GRUB_TIMEOUT=5` and active `GRUB_TIMEOUT=12` must NOT produce duplicates
    let updated = GrubManager::update_default_grub_full(messy, 7, "saved", Some("quiet splash"), Some(&pers));
    let timeout_active_count = updated
        .lines()
        .filter(|l| l.trim_start().starts_with("GRUB_TIMEOUT="))
        .count();
    assert_eq!(timeout_active_count, 1);
    assert!(updated.contains("GRUB_TIMEOUT=7"));
    assert!(updated.contains("GRUB_SAVEDEFAULT=true"));
    assert!(updated.contains("GRUB_GFXMODE=\"1920x1080x32\""));

    // Clearing theme comments it out cleanly
    let mut cleared = pers.clone();
    cleared.theme = None;
    let updated_cleared = GrubManager::update_default_grub_full(messy, 5, "0", Some("quiet"), Some(&cleared));
    assert!(!updated_cleared.lines().any(|l| l.trim_start().starts_with("GRUB_THEME=")));

    // Validation catches bad colors, ${VAR}, trailing backslash, empty default
    assert!(GrubManager::validate_grub_color_pair("light-gray/black").is_ok());
    assert!(GrubManager::validate_grub_color_pair("neon-pink/black").is_err());
    assert!(GrubManager::validate_grub_color_pair("white").is_err());
    assert!(GrubManager::validate_grub_inputs(5, "", None).is_err());
    assert!(GrubManager::validate_grub_inputs(5, "0", Some("quiet ${HOME}")).is_err());
    assert!(GrubManager::validate_grub_inputs(5, "0", Some("quiet\\")).is_err());

    // Cmdline removal diff for Fedora grubby
    let removed = GrubManager::diff_cmdline_removed("quiet splash mitigations=off", "quiet splash");
    assert_eq!(removed, vec!["mitigations=off"]);
}

#[test]
fn test_coreboot_classification_cmos_and_payload_generators() {
    use mithrandir::backend::coreboot::{CorebootManager, CorebootPersonalisationConfig};

    let (is_cb, variant, payload) =
        CorebootManager::classify_from_dmi("3mdeb", "Dasharo (coreboot+UEFI) v1.2.0", true);
    assert!(is_cb);
    assert!(variant.contains("Dasharo"));
    assert!(payload.contains("UEFI"));

    let (is_cb_std, variant_std, _) =
        CorebootManager::classify_from_dmi("LENOVO", "N2CET67W (1.50)", false);
    assert!(!is_cb_std);
    assert!(variant_std.contains("Standard"));

    let cmos = CorebootManager::parse_nvramtool_output(
        "boot_option = Fallback\ndebug_level = Spew\n# comment\nwlan = Enable\n",
    );
    assert_eq!(cmos.len(), 3);
    assert_eq!(cmos[0].key, "boot_option");
    assert_eq!(cmos[0].value, "Fallback");
    assert!(CorebootManager::validate_cmos_option("wlan", "Enable").is_ok());
    assert!(CorebootManager::validate_cmos_option("bad;key", "Enable").is_err());

    let cfg = CorebootPersonalisationConfig {
        bootsplash_image: Some("/boot/grub/splash.png".into()),
        resolution: "1920x1080".into(),
        color_normal: "light-gray/black".into(),
        color_highlight: "white/blue".into(),
        font_path: Some("/boot/grub/fonts/unicode.pf2".into()),
        timeout: 5,
    };
    let snippet = CorebootManager::generate_coreboot_grub_payload_cfg(&cfg);
    assert!(snippet.contains("set gfxmode=1920x1080"));
    assert!(snippet.contains("set menu_color_normal=light-gray/black"));
    assert!(snippet.contains("loadfont /boot/grub/fonts/unicode.pf2"));
    assert!(snippet.contains("background_image -m stretch /boot/grub/splash.png"));

    let cmds = CorebootManager::generate_cbfs_bootsplash_commands(
        "coreboot.rom",
        "/home/user/logo.png",
        "1920x1080",
    );
    assert!(cmds.iter().any(|c| c.contains("flashrom -p internal -r")));
    assert!(cmds.iter().any(|c| c.contains("4:2:0")));
    assert!(cmds.iter().any(|c| c.contains("cbfstool 'coreboot.rom' add -f bootsplash.jpg")));
}
