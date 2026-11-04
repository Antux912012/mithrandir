use gtk4::prelude::*;
use gtk4::{Align, Button, Image, ScrolledWindow, StringList};
use libadwaita as adw;
use adw::prelude::*;
use adw::{
    ActionRow, ComboRow, PreferencesGroup, PreferencesPage, SwitchRow, Toast, ToastOverlay,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::kernel::{bore, detect_kernel};
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
        let toast_overlay_rc = Rc::new(toast_overlay);
        let updating_ui = Rc::new(Cell::new(false));

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
                "Quiet operation, minimal clock speeds, Turbo Boost off, and PCIe ASPM powersupersave",
                "battery-level-20-symbolic",
            ),
            (
                PowerPreset::Balanced,
                "Balanced",
                "Dynamic schedutil scaling and BORE scheduler for daily responsive desktop performance",
                "power-profile-balanced-symbolic",
            ),
            (
                PowerPreset::LowLatency,
                "Low-Latency / Workstation",
                "Prioritizes interactive responsiveness, BORE scheduler, and performance governor",
                "speedometer-symbolic",
            ),
            (
                PowerPreset::Gaming,
                "Max Performance / Gaming",
                "Maximum clocks, Turbo Boost unlocked, BORE scheduler, and EPP performance",
                "applications-games-symbolic",
            ),
        ];

        let active_preset = get_active_preset();
        let preset_btns: Rc<RefCell<Vec<(PowerPreset, Button)>>> =
            Rc::new(RefCell::new(Vec::new()));

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
            row.add_suffix(&apply_btn);
            preset_group.add(&row);
        }

        page.add(&preset_group);

        // ---------------------------------------------------------
        // 2. Kernel & CachyOS BORE Scheduler Group
        // ---------------------------------------------------------
        let sched_group = PreferencesGroup::builder()
            .title("Kernel &amp; CPU Schedulers")
            .description("CachyOS BORE and kernel CPU scheduler configuration")
            .build();

        // Host Kernel Info Row
        let kinfo_row = ActionRow::builder()
            .title("Kernel Release")
            .subtitle(format!(
                "{} ({})",
                kinfo.kernel_release,
                if kinfo.is_cachyos {
                    "CachyOS Optimized"
                } else if kinfo.bore_supported {
                    "BORE Scheduler Available"
                } else {
                    "Standard Linux"
                }
            ))
            .use_markup(false)
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
            .use_markup(false)
            .active(kinfo.bore_enabled)
            .sensitive(bore_supported)
            .build();
        bore_switch.add_prefix(&Image::from_icon_name("cpu-symbolic"));

        let toast_rc_bore = toast_overlay_rc.clone();
        let updating_bore = updating_ui.clone();
        bore_switch.connect_active_notify(move |row| {
            if updating_bore.get() {
                return;
            }
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
            available_govs = vec![
                "powersave".to_string(),
                "schedutil".to_string(),
                "performance".to_string(),
            ];
        }
        let cur_gov = get_current_governor().unwrap_or_else(|| "powersave".to_string());
        let cur_gov_idx = available_govs.iter().position(|g| g == &cur_gov).unwrap_or(0) as u32;

        let gov_model =
            StringList::new(&available_govs.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let gov_row = ComboRow::builder()
            .title("CPU Scaling Governor")
            .subtitle("Kernel cpufreq frequency governor applied across all CPU cores")
            .use_markup(false)
            .model(&gov_model)
            .selected(cur_gov_idx)
            .build();
        gov_row.add_prefix(&Image::from_icon_name("utilities-system-monitor-symbolic"));

        let govs_list = available_govs.clone();
        let toast_rc_gov = toast_overlay_rc.clone();
        let updating_gov = updating_ui.clone();
        gov_row.connect_selected_notify(move |row| {
            if updating_gov.get() {
                return;
            }
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
                            overlay
                                .add_toast(Toast::new(&format!("Failed to set governor: {}", e)));
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

        let epp_model =
            StringList::new(&available_epp.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let epp_row = ComboRow::builder()
            .title("Energy Performance Preference (EPP)")
            .subtitle("CPU hardware energy bias preference (Intel HWP / AMD CPPC)")
            .use_markup(false)
            .model(&epp_model)
            .selected(cur_epp_idx)
            .build();
        epp_row.add_prefix(&Image::from_icon_name("power-profile-balanced-symbolic"));

        let epp_list = available_epp.clone();
        let toast_rc_epp = toast_overlay_rc.clone();
        let updating_epp = updating_ui.clone();
        epp_row.connect_selected_notify(move |row| {
            if updating_epp.get() {
                return;
            }
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

        // Turbo Boost / Core Performance Boost
        let turbo_supported = is_turbo_boost_supported();
        let turbo_enabled = is_turbo_boost_enabled().unwrap_or(true);
        let turbo_switch = SwitchRow::builder()
            .title("CPU Turbo Boost / Core Performance Boost")
            .subtitle(if turbo_supported {
                "Allow processor cores to opportunistically boost above base clock frequency"
            } else {
                "Hardware turbo boost sysfs control not exposed on this CPU driver"
            })
            .use_markup(false)
            .active(turbo_enabled)
            .sensitive(turbo_supported)
            .build();
        turbo_switch.add_prefix(&Image::from_icon_name("applications-utilities-symbolic"));

        let toast_rc_turbo = toast_overlay_rc.clone();
        let updating_turbo = updating_ui.clone();
        turbo_switch.connect_active_notify(move |row| {
            if updating_turbo.get() {
                return;
            }
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
            .use_markup(false)
            .model(&aspm_model)
            .selected(cur_aspm_idx)
            .build();
        aspm_row.add_prefix(&Image::from_icon_name("drive-harddisk-symbolic"));

        let toast_rc_aspm = toast_overlay_rc.clone();
        let updating_aspm = updating_ui.clone();
        aspm_row.connect_selected_notify(move |row| {
            if updating_aspm.get() {
                return;
            }
            let idx = row.selected() as usize;
            if idx < aspm_policies.len() {
                let policy = aspm_policies[idx];
                let res = crate::power::profiles::set_aspm_policy(policy)
                    .or_else(|_| run_privileged_helper(None, None, None, None, Some(policy), false));
                match res {
                    Ok(_) => {
                        if let Some(ref overlay) = *toast_rc_aspm {
                            overlay.add_toast(Toast::new(&format!(
                                "Set PCIe ASPM policy to {}",
                                policy
                            )));
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

        // Connect Preset buttons now that all page widgets exist so they sync live UI state
        for (preset, btn) in preset_btns.borrow().iter() {
            let preset = *preset;
            let all_btns = preset_btns.clone();
            let toast_rc = toast_overlay_rc.clone();
            let updating_preset = updating_ui.clone();
            let bore_sw = bore_switch.clone();
            let gov_r = gov_row.clone();
            let gov_vec = available_govs.clone();
            let epp_r = epp_row.clone();
            let epp_vec = available_epp.clone();
            let turbo_sw = turbo_switch.clone();
            let aspm_r = aspm_row.clone();

            btn.connect_clicked(move |_| {
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

                        // Refresh all widgets on KernelPowerPage to match live system state
                        updating_preset.set(true);
                        if let Some(b_state) = bore::get_bore_state() {
                            bore_sw.set_active(b_state);
                        }
                        if let Some(g_cur) = get_current_governor() {
                            if let Some(pos) = gov_vec.iter().position(|g| g == &g_cur) {
                                gov_r.set_selected(pos as u32);
                            }
                        }
                        if let Some(e_cur) = get_current_epp() {
                            if let Some(pos) = epp_vec.iter().position(|e| e == &e_cur) {
                                epp_r.set_selected(pos as u32);
                            }
                        }
                        if let Some(t_cur) = is_turbo_boost_enabled() {
                            turbo_sw.set_active(t_cur);
                        }
                        if let Some(a_cur) = get_current_aspm_policy() {
                            if let Some(pos) = aspm_policies.iter().position(|p| *p == a_cur) {
                                aspm_r.set_selected(pos as u32);
                            }
                        }
                        updating_preset.set(false);

                        let is_battery = preset == PowerPreset::Battery;
                        crate::tray::sync_tray_power_saver(is_battery);
                        crate::ui::DashboardPage::update_ui_power_saver(is_battery);
                        if let Some(ref overlay) = *toast_rc {
                            overlay.add_toast(Toast::new(&format!(
                                "Applied {} preset successfully",
                                preset.name()
                            )));
                        }
                    }
                    Err(e) => {
                        if let Some(ref overlay) = *toast_rc {
                            overlay.add_toast(Toast::new(&format!(
                                "Error applying preset: {}",
                                e
                            )));
                        }
                    }
                }
            });
        }

        // Save tunables button
        let persist_row = ActionRow::builder()
            .title("Save State as Default Across Boots")
            .subtitle("Writes configuration to /etc/sysctl.d/99-mithrandir-scheduler.conf")
            .use_markup(false)
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

            let res = run_privileged_helper(
                Some(b_val),
                g_val.as_deref(),
                e_val.as_deref(),
                Some(t_val),
                a_val.as_deref(),
                true,
            );

            match res {
                Ok(_) => {
                    if let Some(ref overlay) = *toast_rc_persist {
                        overlay.add_toast(Toast::new(
                            "Kernel & power settings persisted successfully",
                        ));
                    }
                }
                Err(e) => {
                    if let Some(ref overlay) = *toast_rc_persist {
                        overlay.add_toast(Toast::new(&format!(
                            "Error persisting settings: {}",
                            e
                        )));
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
