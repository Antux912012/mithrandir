use gtk4::prelude::*;
use gtk4::{Align, Button, Image, ScrolledWindow, StringList};
use libadwaita as adw;
use adw::prelude::*;
use adw::{ActionRow, ComboRow, PreferencesGroup, PreferencesPage, SwitchRow, Toast, ToastOverlay};
use std::rc::Rc;
use crate::kernel::{bore, scx, detect_kernel};
use crate::power::{
    coordinator::{apply_preset, get_active_preset, run_privileged_helper, PowerPreset},
    profiles::{
        get_available_epp, get_available_governors, get_current_aspm_policy, get_current_epp,
        get_current_governor, is_turbo_boost_enabled, is_turbo_boost_supported,
    },
};

pub struct KernelPowerPage;

impl KernelPowerPage {
    pub fn build(toast_overlay: Option<ToastOverlay>) -> ScrolledWindow {
        let scroller = ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .kinetic_scrolling(true)
            .build();

        let page = PreferencesPage::new();

        let kinfo = detect_kernel();

        // ---------------------------------------------------------
        // 1. Quick Presets Group
        // ---------------------------------------------------------
        let preset_group = PreferencesGroup::builder()
            .title("System Power &amp; Scheduler Presets")
            .description("Coordinated hardware power profile, CPU scaling governor, and kernel scheduler")
            .build();

        let presets = [
            (
                PowerPreset::Battery,
                "Battery / Silent",
                "Quiet operation, minimal clock speeds, ASPM powersupersave, BORE off / EEVDF",
                "battery-level-20-symbolic",
            ),
            (
                PowerPreset::Balanced,
                "Balanced",
                "Dynamic schedutil/bpfland scaling for daily responsive desktop performance",
                "power-profile-balanced-symbolic",
            ),
            (
                PowerPreset::LowLatency,
                "Low-Latency / Workstation",
                "Prioritizes frame pacing, interactive responsiveness, and audio/video production",
                "speedometer-symbolic",
            ),
            (
                PowerPreset::Gaming,
                "Max Performance / Gaming",
                "Maximum clocks, turbo boost unlocked, scx_flash scheduler, EPP performance",
                "applications-games-symbolic",
            ),
        ];

        let active_preset = get_active_preset();
        let toast_overlay_rc = Rc::new(toast_overlay);
        let preset_btns: Rc< std::cell::RefCell<Vec<(PowerPreset, Button)>> > =
            Rc::new(std::cell::RefCell::new(Vec::new()));

        for (preset, title, subtitle, icon) in presets {
            let row = ActionRow::builder()
                .title(title)
                .subtitle(subtitle)
                .use_markup(false)
                .activatable(true)
                .build();
            row.add_prefix(&Image::from_icon_name(icon));

            let apply_btn = Button::builder()
                .label(if preset == active_preset { "Active" } else { "Apply" })
                .valign(Align::Center)
                .build();

            if preset == active_preset {
                apply_btn.add_css_class("suggested-action");
            }

            preset_btns.borrow_mut().push((preset, apply_btn.clone()));

            let toast_rc = toast_overlay_rc.clone();
            let all_btns = preset_btns.clone();
            apply_btn.connect_clicked(move |_| {
                match apply_preset(preset) {
                    Ok(_) => {
                        for (p, b) in all_btns.borrow().iter() {
                            if *p == preset {
                                b.set_label("Active");
                                b.add_css_class("suggested-action");
                            } else {
                                b.set_label("Apply");
                                b.remove_css_class("suggested-action");
                            }
                        }
                        let is_battery = preset == PowerPreset::Battery;
                        crate::tray::sync_tray_power_saver(is_battery);
                        crate::ui::DashboardPage::update_ui_power_saver(is_battery);
                        if let Some(ref overlay) = *toast_rc {
                            overlay.add_toast(Toast::new(&format!("Applied {} preset successfully", preset.name())));
                        }
                    }
                    Err(e) => {
                        if let Some(ref overlay) = *toast_rc {
                            overlay.add_toast(Toast::new(&format!("Error applying preset: {}", e)));
                        }
                    }
                }
            });

            row.add_suffix(&apply_btn);
            preset_group.add(&row);
        }

        page.add(&preset_group);

        // ---------------------------------------------------------
        // 2. Kernel & CachyOS Scheduler Group
        // ---------------------------------------------------------
        let sched_group = PreferencesGroup::builder()
            .title("Kernel &amp; CPU Schedulers")
            .description("CachyOS BORE and sched-ext (SCX) userspace BPF scheduler configuration")
            .build();

        // Host Kernel Info Row
        let kinfo_row = ActionRow::builder()
            .title("Kernel Release")
            .subtitle(format!(
                "{} ({})",
                kinfo.kernel_release,
                if kinfo.is_cachyos { "CachyOS Optimized" } else { "Standard Linux" }
            ))
            .build();
        kinfo_row.add_prefix(&Image::from_icon_name("emblem-system-symbolic"));
        sched_group.add(&kinfo_row);

        // BORE Switch Row
        let bore_supported = kinfo.bore_supported;
        let bore_switch = SwitchRow::builder()
            .title("BORE Scheduler (Burst-Oriented Response Enhancer)")
            .subtitle(if bore_supported {
                "Dynamically adjusts process priority for bursty interactive GUI tasks"
            } else {
                "BORE sysctl interface (/proc/sys/kernel/sched_bore) not detected in current kernel"
            })
            .active(kinfo.bore_enabled)
            .sensitive(bore_supported)
            .build();
        bore_switch.add_prefix(&Image::from_icon_name("cpu-symbolic"));

        let toast_rc_bore = toast_overlay_rc.clone();
        bore_switch.connect_active_notify(move |row| {
            let active = row.is_active();
            if bore::is_bore_supported() {
                let res = bore::set_bore_state(active)
                    .or_else(|_| run_privileged_helper(Some(active), None, None, None, None, false));
                match res {
                    Ok(_) => {
                        let _ = bore::persist_bore_state(active);
                        if let Some(ref overlay) = *toast_rc_bore {
                            overlay.add_toast(Toast::new(if active {
                                "BORE scheduler enabled"
                            } else {
                                "Vanilla EEVDF scheduler enabled"
                            }));
                        }
                    }
                    Err(e) => {
                        if let Some(ref overlay) = *toast_rc_bore {
                            overlay.add_toast(Toast::new(&format!("Failed to toggle BORE: {}", e)));
                        }
                    }
                }
            }
        });
        sched_group.add(&bore_switch);

        // SCX Pluggable Scheduler
        let mut scx_schedulers = vec!["none".to_string()];
        scx_schedulers.extend(kinfo.supported_scx_schedulers.clone());

        let scx_model = StringList::new(&scx_schedulers.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let current_scx_idx = scx_schedulers
            .iter()
            .position(|s| s == &kinfo.scx_active_scheduler)
            .unwrap_or(0) as u32;

        let scx_row = ComboRow::builder()
            .title("sched-ext (SCX) Pluggable Scheduler")
            .subtitle("Dynamic BPF userland scheduler backend (CachyOS scx_bpfland, scx_lavd, etc.)")
            .model(&scx_model)
            .selected(current_scx_idx)
            .sensitive(kinfo.scx_supported)
            .build();
        scx_row.add_prefix(&Image::from_icon_name("system-run-symbolic"));

        let scx_sched_list = scx_schedulers.clone();
        let toast_rc_scx = toast_overlay_rc.clone();
        scx_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if idx < scx_sched_list.len() {
                let chosen = &scx_sched_list[idx];
                if chosen == "none" {
                    let _ = scx::stop_scheduler();
                    if let Some(ref overlay) = *toast_rc_scx {
                        overlay.add_toast(Toast::new("Stopped sched-ext scheduler"));
                    }
                } else {
                    match scx::switch_scheduler(chosen, scx::ScxMode::Auto.as_u32()) {
                        Ok(_) => {
                            if let Some(ref overlay) = *toast_rc_scx {
                                overlay.add_toast(Toast::new(&format!("Switched scheduler to {}", chosen)));
                            }
                        }
                        Err(e) => {
                            if let Some(ref overlay) = *toast_rc_scx {
                                overlay.add_toast(Toast::new(&format!("Failed to switch SCX: {}", e)));
                            }
                        }
                    }
                }
            }
        });
        sched_group.add(&scx_row);

        page.add(&sched_group);

        // ---------------------------------------------------------
        // 3. CPU Frequency Scaling & Power Profile Group
        // ---------------------------------------------------------
        let cpu_group = PreferencesGroup::builder()
            .title("CPU Frequency &amp; Energy Performance")
            .description("Hardware cpufreq governors, Energy Performance Preferences (EPP), and Turbo Boost")
            .build();

        // CPU Scaling Governor
        let mut available_govs = get_available_governors();
        if available_govs.is_empty() {
            available_govs = vec!["powersave".to_string(), "schedutil".to_string(), "performance".to_string()];
        }
        let cur_gov = get_current_governor().unwrap_or_else(|| "powersave".to_string());
        let cur_gov_idx = available_govs.iter().position(|g| g == &cur_gov).unwrap_or(0) as u32;

        let gov_model = StringList::new(&available_govs.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let gov_row = ComboRow::builder()
            .title("CPU Scaling Governor")
            .subtitle("Kernel cpufreq frequency governor applied across all CPU cores")
            .model(&gov_model)
            .selected(cur_gov_idx)
            .build();
        gov_row.add_prefix(&Image::from_icon_name("utilities-system-monitor-symbolic"));

        let govs_list = available_govs.clone();
        let toast_rc_gov = toast_overlay_rc.clone();
        gov_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if idx < govs_list.len() {
                let gov = &govs_list[idx];
                let res = crate::power::profiles::set_governor(gov)
                    .or_else(|_| run_privileged_helper(None, Some(gov), None, None, None, false));
                match res {
                    Ok(_) => {
                        if let Some(ref overlay) = *toast_rc_gov {
                            overlay.add_toast(Toast::new(&format!("Set CPU governor to {}", gov)));
                        }
                    }
                    Err(e) => {
                        if let Some(ref overlay) = *toast_rc_gov {
                            overlay.add_toast(Toast::new(&format!("Failed to set governor: {}", e)));
                        }
                    }
                }
            }
        });
        cpu_group.add(&gov_row);

        // EPP (Energy Performance Preference)
        let mut available_epp = get_available_epp();
        if available_epp.is_empty() {
            available_epp = vec![
                "power".to_string(),
                "balance_power".to_string(),
                "balance_performance".to_string(),
                "performance".to_string(),
            ];
        }
        let cur_epp = get_current_epp().unwrap_or_else(|| "balance_performance".to_string());
        let cur_epp_idx = available_epp.iter().position(|e| e == &cur_epp).unwrap_or(0) as u32;

        let epp_model = StringList::new(&available_epp.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let epp_row = ComboRow::builder()
            .title("Energy Performance Preference (EPP)")
            .subtitle("CPU hardware energy bias preference (Intel HWP / AMD CPPC)")
            .model(&epp_model)
            .selected(cur_epp_idx)
            .build();
        epp_row.add_prefix(&Image::from_icon_name("power-profile-balanced-symbolic"));

        let epp_list = available_epp.clone();
        let toast_rc_epp = toast_overlay_rc.clone();
        epp_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if idx < epp_list.len() {
                let epp = &epp_list[idx];
                let res = crate::power::profiles::set_epp(epp)
                    .or_else(|_| run_privileged_helper(None, None, Some(epp), None, None, false));
                match res {
                    Ok(_) => {
                        if let Some(ref overlay) = *toast_rc_epp {
                            overlay.add_toast(Toast::new(&format!("Set EPP to {}", epp)));
                        }
                    }
                    Err(e) => {
                        if let Some(ref overlay) = *toast_rc_epp {
                            overlay.add_toast(Toast::new(&format!("Failed to set EPP: {}", e)));
                        }
                    }
                }
            }
        });
        cpu_group.add(&epp_row);

        // Turbo Boost Switch
        let turbo_supported = is_turbo_boost_supported();
        let turbo_enabled = is_turbo_boost_enabled().unwrap_or(true);
        let turbo_switch = SwitchRow::builder()
            .title("CPU Turbo / Core Boost")
            .subtitle(if turbo_supported {
                "Allow CPU cores to exceed base clock frequency under heavy thermal budget"
            } else {
                "Hardware Turbo Boost sysfs interface not exposed or not supported"
            })
            .active(turbo_enabled)
            .sensitive(turbo_supported)
            .build();
        turbo_switch.add_prefix(&Image::from_icon_name("applications-utilities-symbolic"));

        let toast_rc_turbo = toast_overlay_rc.clone();
        turbo_switch.connect_active_notify(move |row| {
            let active = row.is_active();
            let res = crate::power::profiles::set_turbo_boost(active)
                .or_else(|_| run_privileged_helper(None, None, None, Some(active), None, false));
            match res {
                Ok(_) => {
                    if let Some(ref overlay) = *toast_rc_turbo {
                        overlay.add_toast(Toast::new(if active {
                            "Turbo Boost enabled"
                        } else {
                            "Turbo Boost disabled (energy saver)"
                        }));
                    }
                }
                Err(e) => {
                    if let Some(ref overlay) = *toast_rc_turbo {
                        overlay.add_toast(Toast::new(&format!("Failed to set Turbo Boost: {}", e)));
                    }
                }
            }
        });
        cpu_group.add(&turbo_switch);

        page.add(&cpu_group);

        // ---------------------------------------------------------
        // 4. PCIe Bus & Persistence Group
        // ---------------------------------------------------------
        let pcie_group = PreferencesGroup::builder()
            .title("Bus Power &amp; Persistence")
            .description("PCIe Active State Power Management (ASPM) and boot-time persistent tunables")
            .build();

        let aspm_policies = ["default", "performance", "powersave", "powersupersave"];
        let cur_aspm = get_current_aspm_policy().unwrap_or_else(|| "default".to_string());
        let cur_aspm_idx = aspm_policies.iter().position(|p| *p == cur_aspm).unwrap_or(0) as u32;
        let aspm_model = StringList::new(&aspm_policies);
        let aspm_row = ComboRow::builder()
            .title("PCIe ASPM Link State Power Policy")
            .subtitle("Controls power state transitions on PCIe buses, GPUs, NVMe, and network cards")
            .model(&aspm_model)
            .selected(cur_aspm_idx)
            .build();
        aspm_row.add_prefix(&Image::from_icon_name("drive-harddisk-symbolic"));

        let toast_rc_aspm = toast_overlay_rc.clone();
        aspm_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if idx < aspm_policies.len() {
                let policy = aspm_policies[idx];
                let res = crate::power::profiles::set_aspm_policy(policy)
                    .or_else(|_| run_privileged_helper(None, None, None, None, Some(policy), false));
                match res {
                    Ok(_) => {
                        if let Some(ref overlay) = *toast_rc_aspm {
                            overlay.add_toast(Toast::new(&format!("Set PCIe ASPM policy to {}", policy)));
                        }
                    }
                    Err(e) => {
                        if let Some(ref overlay) = *toast_rc_aspm {
                            overlay.add_toast(Toast::new(&format!("Failed to set ASPM: {}", e)));
                        }
                    }
                }
            }
        });
        pcie_group.add(&aspm_row);

        // Save tunables button
        let persist_row = ActionRow::builder()
            .title("Save State as Default Across Boots")
            .subtitle("Writes configuration to /etc/sysctl.d/99-mithrandir-scheduler.conf via Polkit helper")
            .build();
        persist_row.add_prefix(&Image::from_icon_name("document-save-symbolic"));

        let persist_btn = Button::builder()
            .label("Persist Settings")
            .valign(Align::Center)
            .build();

        let toast_rc_persist = toast_overlay_rc.clone();
        let bore_switch_clone = bore_switch.clone();
        let gov_row_clone = gov_row.clone();
        let epp_row_clone = epp_row.clone();
        let turbo_switch_clone = turbo_switch.clone();
        let aspm_row_clone = aspm_row.clone();

        persist_btn.connect_clicked(move |_| {
            let b_val = bore_switch_clone.is_active();
            let g_idx = gov_row_clone.selected() as usize;
            let g_val = available_govs.get(g_idx).cloned();
            let e_idx = epp_row_clone.selected() as usize;
            let e_val = available_epp.get(e_idx).cloned();
            let t_val = turbo_switch_clone.is_active();
            let a_idx = aspm_row_clone.selected() as usize;
            let a_val = aspm_policies.get(a_idx).map(|s| s.to_string());

            match run_privileged_helper(
                Some(b_val),
                g_val.as_deref(),
                e_val.as_deref(),
                Some(t_val),
                a_val.as_deref(),
                true,
            ) {
                Ok(_) => {
                    if let Some(ref overlay) = *toast_rc_persist {
                        overlay.add_toast(Toast::new("Kernel & power settings persisted successfully"));
                    }
                }
                Err(e) => {
                    if let Some(ref overlay) = *toast_rc_persist {
                        overlay.add_toast(Toast::new(&format!("Error persisting settings: {}", e)));
                    }
                }
            }
        });

        persist_row.add_suffix(&persist_btn);
        pcie_group.add(&persist_row);

        page.add(&pcie_group);

        scroller.set_child(Some(&page));
        scroller
    }
}
