use std::fs;
use mithrandir::backend::distro::DistroFamily;
use mithrandir::backend::grub::{GrubConfig, GrubLiveStatus, GrubPersonalisation};
use mithrandir::backend::plymouth::{InitramfsBackend, PlymouthManager};

#[test]
fn test_parse_plymouth_manifest_bgrt_and_script() {
    let tmp = tempfile::tempdir().unwrap();
    let theme_dir = tmp.path().join("nord-bgrt");
    fs::create_dir_all(&theme_dir).unwrap();
    let thumb = theme_dir.join("watermark.png");
    fs::write(&thumb, b"PNG_MOCK").unwrap();

    let bgrt_manifest = r#"
[Plymouth Theme]
Name=Nord BGRT
Description=Seamless OEM logo splash with Nord spinner
ModuleName=two-step

[two-step]
ImageDir=/usr/share/plymouth/themes/nord-bgrt
"#;

    let parsed = PlymouthManager::parse_manifest(
        bgrt_manifest,
        "nord-bgrt",
        "/usr/share/plymouth/themes/nord-bgrt/nord-bgrt.plymouth",
        Some(&theme_dir),
    )
    .expect("should parse valid .plymouth manifest");

    assert_eq!(parsed.id, "nord-bgrt");
    assert_eq!(parsed.name, "Nord BGRT");
    assert_eq!(parsed.module_name, "two-step");
    assert_eq!(parsed.preview_image.as_deref(), Some(thumb.to_string_lossy().as_ref()));

    // Missing ModuleName should return None
    let invalid_manifest = "[Plymouth Theme]\nName=Broken\n";
    assert!(PlymouthManager::parse_manifest(invalid_manifest, "broken", "", None).is_none());
}

#[test]
fn test_parse_and_update_plymouthd_conf() {
    let initial = r#"# Administrator customizations go in this file
[Daemon]
Theme=spinner
ShowDelay=5
DeviceTimeout=8

[OtherSection]
KeepMe=true
"#;
    let (theme, delay, timeout) = PlymouthManager::parse_plymouthd_conf(initial);
    assert_eq!(theme.as_deref(), Some("spinner"));
    assert_eq!(delay, 5);
    assert_eq!(timeout, 8);

    let updated = PlymouthManager::update_plymouthd_conf(initial, "bgrt", 0, 12);
    assert!(updated.contains("# Administrator customizations go in this file"));
    assert!(updated.contains("Theme=bgrt"));
    assert!(updated.contains("ShowDelay=0"));
    assert!(updated.contains("DeviceTimeout=12"));
    assert!(updated.contains("[OtherSection]\nKeepMe=true"));
    assert!(!updated.contains("Theme=spinner"));

    // Empty file creates [Daemon] section cleanly
    let fresh = PlymouthManager::update_plymouthd_conf("", "nord-splash", 0, 8);
    assert!(fresh.contains("[Daemon]"));
    assert!(fresh.contains("Theme=nord-splash"));
}

#[test]
fn test_mkinitcpio_plymouth_hook_detection() {
    let with_hook = r#"
# HOOKS=(base udev autodetect modconf block filesystems keyboard fsck)
MODULES=()
HOOKS=(base udev plymouth autodetect microcode modconf kms keyboard keymap consolefont block filesystems fsck)
"#;
    assert!(PlymouthManager::check_mkinitcpio_hook(with_hook));

    let without_hook = r#"
# HOOKS=(base udev plymouth)
HOOKS=(base udev autodetect modconf block filesystems keyboard fsck)
"#;
    assert!(!PlymouthManager::check_mkinitcpio_hook(without_hook));
}

#[test]
fn test_initramfs_backend_selection_across_distros() {
    assert_eq!(
        PlymouthManager::select_initramfs_backend(
            DistroFamily::Debian,
            false,
            true,
            false,
            false,
            false
        ),
        InitramfsBackend::UpdateInitramfs
    );
    assert_eq!(
        PlymouthManager::select_initramfs_backend(
            DistroFamily::Fedora,
            false,
            false,
            true,
            false,
            false
        ),
        InitramfsBackend::Dracut
    );
    assert_eq!(
        PlymouthManager::select_initramfs_backend(
            DistroFamily::Arch,
            false,
            false,
            false,
            true,
            false
        ),
        InitramfsBackend::Mkinitcpio
    );
    assert_eq!(
        PlymouthManager::select_initramfs_backend(
            DistroFamily::Arch,
            false,
            false,
            false,
            false,
            true
        ),
        InitramfsBackend::Booster
    );
    assert_eq!(
        PlymouthManager::select_initramfs_backend(
            DistroFamily::Fedora,
            true,
            false,
            true,
            false,
            false
        ),
        InitramfsBackend::ImmutableReadOnly
    );
}

#[test]
fn test_theme_id_and_archive_path_validation() {
    assert!(PlymouthManager::validate_theme_id("bgrt").is_ok());
    assert!(PlymouthManager::validate_theme_id("nord-arch_v2.1").is_ok());
    assert!(PlymouthManager::validate_theme_id("../evil").is_err());
    assert!(PlymouthManager::validate_theme_id("foo;rm -rf /").is_err());
    assert!(PlymouthManager::validate_theme_id("").is_err());

    // Valid staged theme directory
    let tmp = tempfile::tempdir().unwrap();
    let tdir = tmp.path().join("mytheme");
    fs::create_dir_all(&tdir).unwrap();
    fs::write(
        tdir.join("mytheme.plymouth"),
        "[Plymouth Theme]\nName=My Theme\nModuleName=script\n",
    )
    .unwrap();

    let (id, root) = PlymouthManager::validate_staged_theme_dir(tmp.path())
        .expect("valid staged theme should pass");
    assert_eq!(id, "mytheme");
    assert_eq!(root, tdir);

    // Symlink inside staged dir must be rejected
    #[cfg(unix)]
    {
        let link_path = tdir.join("bad_link");
        std::os::unix::fs::symlink("/etc/passwd", &link_path).unwrap();
        assert!(PlymouthManager::validate_staged_theme_dir(tmp.path()).is_err());
    }
}

#[test]
fn test_grub_splash_readiness_audit() {
    let dummy_status = GrubLiveStatus {
        cfg_path: "/boot/grub/grub.cfg".into(),
        cfg_exists: true,
        cfg_size_bytes: 1024,
        cfg_modified_time: "now".into(),
        backup_path: "/boot/grub/grub.cfg.bak".into(),
        backup_exists: false,
        backup_size_bytes: 0,
        backup_modified_time: "".into(),
        default_path: "/etc/default/grub".into(),
        default_exists: true,
        default_size_bytes: 512,
        default_modified_time: "now".into(),
        default_backup_path: "/etc/default/grub.bak".into(),
        default_backup_exists: false,
        default_backup_size_bytes: 0,
        default_backup_modified_time: "".into(),
        default_dropins_dir: None,
        default_dropins_count: 0,
        mithrandir_dropin_exists: false,
        strategy_label: "Test".into(),
    };

    let missing_cfg = GrubConfig {
        timeout: 5,
        default_entry: "0".into(),
        cmdline: "loglevel=3".into(),
        live_status: dummy_status.clone(),
        menu_entries: vec![],
        structured_entries: vec![],
        personalisation: GrubPersonalisation::default(),
        available_themes: vec![],
        available_fonts: vec![],
    };

    let r_bad = PlymouthManager::audit_readiness(
        DistroFamily::Arch,
        &missing_cfg,
        true,
        Some("HOOKS=(base udev block filesystems)"),
        true,
    );
    assert!(!r_bad.grub_has_quiet);
    assert!(!r_bad.grub_has_splash);
    assert!(!r_bad.grub_gfxpayload_keep);
    assert_eq!(r_bad.mkinitcpio_has_plymouth_hook, Some(false));
    assert!(r_bad.nvidia_kms_recommended);
    assert!(r_bad.warnings.len() >= 4);

    let ready_cfg = GrubConfig {
        timeout: 5,
        default_entry: "0".into(),
        cmdline: "quiet splash nvidia-drm.modeset=1".into(),
        live_status: dummy_status,
        menu_entries: vec![],
        structured_entries: vec![],
        personalisation: GrubPersonalisation {
            gfxpayload_linux: Some("keep".into()),
            ..Default::default()
        },
        available_themes: vec![],
        available_fonts: vec![],
    };

    let r_good = PlymouthManager::audit_readiness(
        DistroFamily::Arch,
        &ready_cfg,
        true,
        Some("HOOKS=(base udev plymouth block filesystems)"),
        true,
    );
    assert!(r_good.grub_has_quiet);
    assert!(r_good.grub_has_splash);
    assert!(r_good.grub_gfxpayload_keep);
    assert_eq!(r_good.mkinitcpio_has_plymouth_hook, Some(true));
    assert!(!r_good.nvidia_kms_recommended);
    assert!(r_good.warnings.is_empty());
}
