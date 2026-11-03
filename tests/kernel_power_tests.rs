use mithrandir::kernel::{bore, scx, detect_kernel};
use mithrandir::power::{
    coordinator::{get_active_preset, set_active_preset, PowerPreset},
    profiles,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_bore_kernel_detection() {
    assert!(bore::is_cachyos_kernel_release("6.11.0-cachyos"));
    assert!(bore::is_cachyos_kernel_release("6.12.0-bore"));
    assert!(bore::is_cachyos_kernel_release("7.3.1-cachyos-debian"));
    assert!(!bore::is_cachyos_kernel_release("6.1.0-28-amd64-vanilla-mock"));
}

#[test]
fn test_bore_state_read_write_mock() {
    let dir = tempdir().unwrap();
    let sched_bore_path = dir.path().join("sched_bore");

    // Initially does not exist
    assert!(!bore::is_bore_supported_at(&sched_bore_path));
    assert_eq!(bore::get_bore_state_from_path(&sched_bore_path), None);

    // Write "1\n" -> active
    fs::write(&sched_bore_path, "1\n").unwrap();
    assert!(bore::is_bore_supported_at(&sched_bore_path));
    assert_eq!(bore::get_bore_state_from_path(&sched_bore_path), Some(true));

    // Write "0\n" -> vanilla / EEVDF
    fs::write(&sched_bore_path, "0\n").unwrap();
    assert_eq!(bore::get_bore_state_from_path(&sched_bore_path), Some(false));

    // Test set_bore_state_to_path
    assert!(bore::set_bore_state_to_path(&sched_bore_path, true).is_ok());
    assert_eq!(fs::read_to_string(&sched_bore_path).unwrap().trim(), "1");

    assert!(bore::set_bore_state_to_path(&sched_bore_path, false).is_ok());
    assert_eq!(fs::read_to_string(&sched_bore_path).unwrap().trim(), "0");
}

#[test]
fn test_bore_persist_sysctl_conf_mock() {
    let dir = tempdir().unwrap();
    let conf_path = dir.path().join("sysctl.d/99-mithrandir-scheduler.conf");

    assert!(bore::persist_bore_state_to_path(&conf_path, true).is_ok());
    let content = fs::read_to_string(&conf_path).unwrap();
    assert!(content.contains("kernel.sched_bore = 1"));

    assert!(bore::persist_bore_state_to_path(&conf_path, false).is_ok());
    let content = fs::read_to_string(&conf_path).unwrap();
    assert!(content.contains("kernel.sched_bore = 0"));
}

#[test]
fn test_scx_mode_conversion() {
    assert_eq!(scx::ScxMode::from_u32(0), scx::ScxMode::Auto);
    assert_eq!(scx::ScxMode::from_u32(1), scx::ScxMode::Gaming);
    assert_eq!(scx::ScxMode::from_u32(2), scx::ScxMode::PowerSave);
    assert_eq!(scx::ScxMode::from_u32(3), scx::ScxMode::LowLatency);
    assert_eq!(scx::ScxMode::from_u32(4), scx::ScxMode::Server);
    assert_eq!(scx::ScxMode::from_u32(99), scx::ScxMode::Auto);

    assert_eq!(scx::ScxMode::Gaming.as_u32(), 1);
    assert_eq!(scx::ScxMode::PowerSave.as_u32(), 2);
}

#[test]
fn test_scx_sysfs_read_mock() {
    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state");
    let ops_dir = dir.path().join("root");
    fs::create_dir_all(&ops_dir).unwrap();
    let ops_path = ops_dir.join("ops");

    assert!(!scx::is_scx_supported_at(&state_path));
    assert_eq!(scx::get_sysfs_scheduler_state_at(&state_path), None);
    assert_eq!(scx::get_sysfs_active_scheduler_at(&ops_path), "none");

    fs::write(&state_path, "enabled\n").unwrap();
    fs::write(&ops_path, "bpfland\n").unwrap();

    assert!(scx::is_scx_supported_at(&state_path));
    assert_eq!(scx::get_sysfs_scheduler_state_at(&state_path), Some("enabled".to_string()));
    assert_eq!(scx::get_sysfs_active_scheduler_at(&ops_path), "bpfland");
}

#[test]
fn test_cpu_governors_and_epp_mock() {
    let dir = tempdir().unwrap();
    let cpu0_cpufreq = dir.path().join("cpu0/cpufreq");
    let cpu1_cpufreq = dir.path().join("cpu1/cpufreq");
    fs::create_dir_all(&cpu0_cpufreq).unwrap();
    fs::create_dir_all(&cpu1_cpufreq).unwrap();

    fs::write(
        cpu0_cpufreq.join("scaling_available_governors"),
        "performance powersave schedutil\n",
    )
    .unwrap();
    fs::write(cpu0_cpufreq.join("scaling_governor"), "schedutil\n").unwrap();
    fs::write(cpu1_cpufreq.join("scaling_governor"), "schedutil\n").unwrap();

    let govs = profiles::get_available_governors_at(dir.path());
    assert_eq!(govs, vec!["performance", "powersave", "schedutil"]);

    assert_eq!(profiles::get_current_governor_at(dir.path()), Some("schedutil".to_string()));

    // Set governor across cores
    assert!(profiles::set_governor_at(dir.path(), "performance").is_ok());
    assert_eq!(fs::read_to_string(cpu0_cpufreq.join("scaling_governor")).unwrap().trim(), "performance");
    assert_eq!(fs::read_to_string(cpu1_cpufreq.join("scaling_governor")).unwrap().trim(), "performance");

    // EPP
    fs::write(
        cpu0_cpufreq.join("energy_performance_available_preferences"),
        "default performance balance_performance power\n",
    )
    .unwrap();
    fs::write(cpu0_cpufreq.join("energy_performance_preference"), "balance_performance\n").unwrap();
    fs::write(cpu1_cpufreq.join("energy_performance_preference"), "balance_performance\n").unwrap();

    let epps = profiles::get_available_epp_at(dir.path());
    assert_eq!(epps, vec!["default", "performance", "balance_performance", "power"]);
    assert_eq!(profiles::get_current_epp_at(dir.path()), Some("balance_performance".to_string()));

    assert!(profiles::set_epp_at(dir.path(), "power").is_ok());
    assert_eq!(fs::read_to_string(cpu0_cpufreq.join("energy_performance_preference")).unwrap().trim(), "power");
    assert_eq!(fs::read_to_string(cpu1_cpufreq.join("energy_performance_preference")).unwrap().trim(), "power");
}

#[test]
fn test_turbo_boost_mock() {
    let dir = tempdir().unwrap();

    // Intel pstate
    let intel_dir = dir.path().join("intel_pstate");
    fs::create_dir_all(&intel_dir).unwrap();
    let no_turbo = intel_dir.join("no_turbo");

    fs::write(&no_turbo, "0\n").unwrap(); // 0 = Turbo enabled
    assert!(profiles::is_turbo_boost_supported_at(dir.path()));
    assert_eq!(profiles::is_turbo_boost_enabled_at(dir.path()), Some(true));

    // Disable turbo
    assert!(profiles::set_turbo_boost_at(dir.path(), false).is_ok());
    assert_eq!(fs::read_to_string(&no_turbo).unwrap().trim(), "1");
    assert_eq!(profiles::is_turbo_boost_enabled_at(dir.path()), Some(false));

    // Enable turbo
    assert!(profiles::set_turbo_boost_at(dir.path(), true).is_ok());
    assert_eq!(fs::read_to_string(&no_turbo).unwrap().trim(), "0");
    assert_eq!(profiles::is_turbo_boost_enabled_at(dir.path()), Some(true));
}

#[test]
fn test_mithrandir_presets_metadata() {
    assert_eq!(PowerPreset::Battery.name(), "Battery / Silent");
    assert_eq!(PowerPreset::Balanced.name(), "Balanced");
    assert_eq!(PowerPreset::LowLatency.name(), "Low-Latency / Workstation");
    assert_eq!(PowerPreset::Gaming.name(), "Max Performance / Gaming");

    assert_eq!(PowerPreset::from_u8(0), PowerPreset::Battery);
    assert_eq!(PowerPreset::from_u8(1), PowerPreset::Balanced);
    assert_eq!(PowerPreset::from_u8(2), PowerPreset::LowLatency);
    assert_eq!(PowerPreset::from_u8(3), PowerPreset::Gaming);

    set_active_preset(PowerPreset::Gaming);
    assert_eq!(get_active_preset(), PowerPreset::Gaming);

    set_active_preset(PowerPreset::Balanced);
    assert_eq!(get_active_preset(), PowerPreset::Balanced);
}

#[test]
fn test_detect_kernel_live_fallback() {
    let kinfo = detect_kernel();
    assert!(!kinfo.kernel_release.is_empty());
    // On systems without SCX loaded, active scheduler should be "none" or name
    assert!(!kinfo.scx_active_scheduler.is_empty());
}

#[test]
fn test_scx_normalize_and_descriptions() {
    assert_eq!(scx::normalize_scheduler_name("bpfland"), "scx_bpfland");
    assert_eq!(scx::normalize_scheduler_name("bpfland_1.0.8_g123456"), "scx_bpfland");
    assert_eq!(scx::normalize_scheduler_name("scx_lavd"), "scx_lavd");
    assert_eq!(scx::normalize_scheduler_name("rustland"), "scx_rustland");
    assert_eq!(scx::normalize_scheduler_name("simple"), "scx_simple");
    assert_eq!(scx::normalize_scheduler_name("none"), "none");
    assert_eq!(scx::normalize_scheduler_name("unknown"), "none");
    assert_eq!(scx::normalize_scheduler_name(""), "none");

    for &sched in scx::KNOWN_SCX_SCHEDULERS {
        let desc = scx::scheduler_description(sched);
        assert!(!desc.is_empty());
        assert_ne!(desc, "Pluggable sched-ext (SCX) BPF kernel scheduler");
    }

    assert_eq!(scx::ScxMode::Gaming.label(), "Gaming");
    assert_eq!(scx::ScxMode::PowerSave.cli_name(), "powersave");
    assert_eq!(
        scx::ScxMode::Gaming.flags_for_scheduler("scx_bpfland"),
        &["-m", "performance"]
    );
    assert_eq!(
        scx::ScxMode::PowerSave.flags_for_scheduler("scx_lavd"),
        &["--powersave"]
    );
}

#[test]
fn test_scx_distro_install_plans() {
    use mithrandir::os_info::LinuxDistro;

    let ubuntu_no_scx = scx::get_install_plan_for_distro(LinuxDistro::Ubuntu, "Ubuntu 24.04 LTS", false);
    assert!(ubuntu_no_scx.command.contains("ppa:arighi/sched-ext"));
    assert!(ubuntu_no_scx.command.contains("linux-generic-wip"));
    assert!(ubuntu_no_scx.requires_reboot_for_kernel);

    let ubuntu_with_scx = scx::get_install_plan_for_distro(LinuxDistro::Ubuntu, "Ubuntu 24.10", true);
    assert!(ubuntu_with_scx.command.contains("ppa:arighi/sched-ext"));
    assert!(!ubuntu_with_scx.requires_reboot_for_kernel);

    let fedora_plan = scx::get_install_plan_for_distro(LinuxDistro::Fedora, "Fedora Linux 41", true);
    assert!(fedora_plan.command.contains("scx-scheds"));

    let arch_plan = scx::get_install_plan_for_distro(LinuxDistro::Arch, "Arch Linux", true);
    assert!(arch_plan.command.contains("pacman"));
    assert!(arch_plan.command.contains("scx-scheds"));

    let suse_plan = scx::get_install_plan_for_distro(LinuxDistro::OpenSuse, "openSUSE Tumbleweed", true);
    assert!(suse_plan.command.contains("zypper"));
}
