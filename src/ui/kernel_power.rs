use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Image, Orientation, ScrolledWindow, StringList};
use libadwaita as adw;
use adw::prelude::*;
use adw::{
    ActionRow, ButtonContent, ComboRow, PreferencesGroup, PreferencesPage, SwitchRow, Toast,
    ToastOverlay,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::kernel::{bore, detect_kernel, scx, KernelInfo};
use crate::power::{
    coordinator::{apply_preset, get_active_preset, run_privileged_helper, PowerPreset},
    profiles::{
        get_available_epp, get_available_governors, get_current_aspm_policy, get_current_epp,
        get_current_governor, is_turbo_boost_enabled, is_turbo_boost_supported,
    },
};

pub struct KernelPowerPage;

impl KernelPowerPage {
    fn format_scx_status_subtitle(kinfo: &KernelInfo) -> String {
        let sysfs_state = scx::get_sysfs_scheduler_state().unwrap_or_else(|| {
            if kinfo.scx_supported {
                "disabled".to_string()
            } else {
                "unsupported by running kernel".to_string()
            }
        });
        let count = kinfo.supported_scx_schedulers.len();
        let plan = scx::get_install_plan();
        if count == 0 {
            format!(
                "Kernel state: {} • No scx_* binaries found • Ready to install via {}",
                sysfs_state, plan.summary
            )
        } else {
            format!(
                "Kernel state: {} • Active ops: {} • {} scx_* schedulers installed ({})",
                sysfs_state,
                kinfo.scx_active_scheduler,
                count,
                kinfo.supported_scx_schedulers.join(", ")
            )
        }
    }

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
                "Quiet operation, minimal clock speeds, ASPM powersupersave, scx_lavd powersave",
                "battery-level-20-symbolic",
            ),
            (
                PowerPreset::Balanced,
                "Balanced",
                "Dynamic schedutil/scx_bpfland scaling for daily responsive desktop performance",
                "power-profile-balanced-symbolic",
            ),
            (
                PowerPreset::LowLatency,
                "Low-Latency / Workstation",
                "Prioritizes frame pacing, interactive responsiveness, and scx_lavd low-latency mode",
                "speedometer-symbolic",
            ),
            (
                PowerPreset::Gaming,
                "Max Performance / Gaming",
                "Maximum clocks, turbo boost unlocked, scx_flash gaming scheduler, EPP performance",
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
        // 2. Kernel & CachyOS / sched-ext Scheduler Group
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
                if kinfo.is_cachyos {
                    "CachyOS Optimized"
                } else if kinfo.scx_supported {
                    "sched-ext BPF Enabled"
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

        // SCX Pluggable Scheduler ComboRow
        let mut initial_scx_schedulers = vec!["none".to_string()];
        initial_scx_schedulers.extend(kinfo.supported_scx_schedulers.clone());
        let scx_sched_list: Rc<RefCell<Vec<String>>> =
            Rc::new(RefCell::new(initial_scx_schedulers.clone()));

        let scx_model = StringList::new(
            &initial_scx_schedulers
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        );
        let current_scx_idx = initial_scx_schedulers
            .iter()
            .position(|s| s == &kinfo.scx_active_scheduler)
            .unwrap_or(0) as u32;

        let scx_row = ComboRow::builder()
            .title("sched-ext (SCX) Pluggable Scheduler")
            .subtitle(scx::scheduler_description(&kinfo.scx_active_scheduler))
            .use_markup(false)
            .model(&scx_model)
            .selected(current_scx_idx)
            .sensitive(kinfo.scx_supported && !kinfo.supported_scx_schedulers.is_empty())
            .build();
        scx_row.add_prefix(&Image::from_icon_name("system-run-symbolic"));
        sched_group.add(&scx_row);

        // SCX Operating Mode ComboRow
        let mode_labels: Vec<&str> = scx::ScxMode::all().iter().map(|m| m.label()).collect();
        let mode_model = StringList::new(&mode_labels);
        let initial_mode_idx = (kinfo.scx_mode as usize).min(mode_labels.len().saturating_sub(1)) as u32;

        let scx_mode_row = ComboRow::builder()
            .title("sched-ext (SCX) Operating Profile Mode")
            .subtitle("Tuning profile passed to org.scx.Loader / scxctl / scx_* daemon flags")
            .use_markup(false)
            .model(&mode_model)
            .selected(initial_mode_idx)
            .sensitive(kinfo.scx_supported && !kinfo.supported_scx_schedulers.is_empty())
            .build();
        scx_mode_row.add_prefix(&Image::from_icon_name("speedometer-symbolic"));
        
        sched_group.add(&scx_mode_row);

        // SCX Kernel & Package Status + One-Click Installer Row
        let scx_status_row = ActionRow::builder()
            .title("sched-ext (SCX) Status &amp; Package Manager")
            .subtitle(Self::format_scx_status_subtitle(&kinfo))
            .use_markup(false)
            .build();
        scx_status_row.add_prefix(&Image::from_icon_name("software-update-available-symbolic"));

        let scx_action_box = GtkBox::new(Orientation::Horizontal, 8);
        scx_action_box.set_valign(Align::Center);

        let refresh_scx_btn = Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Refresh sched-ext status and installed schedulers")
            .valign(Align::Center)
            .build();
        refresh_scx_btn.add_css_class("flat");

        let install_btn_label = if kinfo.supported_scx_schedulers.is_empty() {
            "Install SCX"
        } else {
            "Update SCX"
        };
        let install_scx_content = ButtonContent::builder()
            .icon_name("system-software-install-symbolic")
            .label(install_btn_label)
            .build();
        let install_scx_btn = Button::builder()
            .child(&install_scx_content)
            .tooltip_text("Install or update sched-ext (scx) packages for your Linux distribution")
            .valign(Align::Center)
            .build();
        if kinfo.supported_scx_schedulers.is_empty() {
            install_scx_btn.add_css_class("suggested-action");
        }

        scx_action_box.append(&refresh_scx_btn);
        scx_action_box.append(&install_scx_btn);
        scx_status_row.add_suffix(&scx_action_box);
        sched_group.add(&scx_status_row);

        // Wire SCX Scheduler & Mode selection handlers
        let scx_list_for_sched = scx_sched_list.clone();
        let toast_rc_scx = toast_overlay_rc.clone();
        let scx_mode_for_sched = scx_mode_row.clone();
        let scx_status_for_sched = scx_status_row.clone();
        let updating_scx = updating_ui.clone();
        scx_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            let list = scx_list_for_sched.borrow();
            if idx < list.len() {
                let chosen = &list[idx];
                row.set_subtitle(scx::scheduler_description(chosen));
                if updating_scx.get() {
                    return;
                }
                let mode_val = scx_mode_for_sched.selected();
                if chosen == "none" {
                    match scx::stop_scheduler() {
                        Ok(_) => {
                            if let Some(ref overlay) = *toast_rc_scx {
                                overlay.add_toast(Toast::new(
                                    "Stopped sched-ext scheduler (reverted to kernel EEVDF/BORE)",
                                ));
                            }
                        }
                        Err(e) => {
                            if let Some(ref overlay) = *toast_rc_scx {
                                overlay.add_toast(Toast::new(&format!(
                                    "Failed to stop SCX: {}",
                                    e
                                )));
                            }
                        }
                    }
                } else {
                    match scx::switch_scheduler(chosen, mode_val) {
                        Ok(_) => {
                            let mode_label = scx::ScxMode::from_u32(mode_val).label();
                            if let Some(ref overlay) = *toast_rc_scx {
                                overlay.add_toast(Toast::new(&format!(
                                    "Switched scheduler to {} ({})",
                                    chosen, mode_label
                                )));
                            }
                        }
                        Err(e) => {
                            if let Some(ref overlay) = *toast_rc_scx {
                                overlay.add_toast(Toast::new(&format!(
                                    "Failed to switch SCX: {}",
                                    e
                                )));
                            }
                        }
                    }
                }
                let latest = detect_kernel();
                scx_status_for_sched.set_subtitle(&Self::format_scx_status_subtitle(&latest));
            }
        });

        let scx_list_for_mode = scx_sched_list.clone();
        let scx_row_for_mode = scx_row.clone();
        let scx_status_for_mode = scx_status_row.clone();
        let toast_rc_mode = toast_overlay_rc.clone();
        let updating_mode = updating_ui.clone();
        scx_mode_row.connect_selected_notify(move |mode_row| {
            if updating_mode.get() {
                return;
            }
            let sched_idx = scx_row_for_mode.selected() as usize;
            let list = scx_list_for_mode.borrow();
            if sched_idx < list.len() {
                let chosen = &list[sched_idx];
                if chosen != "none" {
                    let mode_val = mode_row.selected();
                    let mode_label = scx::ScxMode::from_u32(mode_val).label();
                    match scx::switch_scheduler(chosen, mode_val) {
                        Ok(_) => {
                            if let Some(ref overlay) = *toast_rc_mode {
                                overlay.add_toast(Toast::new(&format_applied_mode(
                                    chosen, mode_label,
                                )));
                            }
                        }
                        Err(e) => {
                            if let Some(ref overlay) = *toast_rc_mode {
                                overlay.add_toast(Toast::new(&format!(
                                    "Failed to apply SCX mode: {}",
                                    e
                                )));
                            }
                        }
                    }
                    let latest = detect_kernel();
                    scx_status_for_mode.set_subtitle(&Self::format_scx_status_subtitle(&latest));
                }
            }
        });

        // Helper closure to refresh SCX widgets from live system state
        let refresh_scx_widgets = {
            let scx_sched_list = scx_sched_list.clone();
            let scx_row = scx_row.clone();
            let scx_mode_row = scx_mode_row.clone();
            let scx_status_row = scx_status_row.clone();
            let install_scx_btn = install_scx_btn.clone();
            let install_scx_content = install_scx_content.clone();
            let updating_ui = updating_ui.clone();
            Rc::new(move || {
                let latest = detect_kernel();
                updating_ui.set(true);

                let mut new_scheds = vec!["none".to_string()];
                new_scheds.extend(latest.supported_scx_schedulers.clone());
                *scx_sched_list.borrow_mut() = new_scheds.clone();

                let new_model = StringList::new(
                    &new_scheds.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                );
                scx_row.set_model(Some(&new_model));

                let idx = new_scheds
                    .iter()
                    .position(|s| s == &latest.scx_active_scheduler)
                    .unwrap_or(0) as u32;
                scx_row.set_selected(idx);
                scx_row.set_subtitle(scx::scheduler_description(&latest.scx_active_scheduler));
                let can_use_scx =
                    latest.scx_supported && !latest.supported_scx_schedulers.is_empty();
                scx_row.set_sensitive(can_use_scx);
                scx_mode_row.set_sensitive(can_use_scx);
                scx_mode_row.set_selected((latest.scx_mode as usize).min(4) as u32);
                scx_status_row.set_subtitle(&Self::format_scx_status_subtitle(&latest));

                if latest.supported_scx_schedulers.is_empty() {
                    install_scx_content.set_label("Install SCX");
                    install_scx_btn.add_css_class("suggested-action");
                } else {
                    install_scx_content.set_label("Update SCX");
                    install_scx_btn.remove_css_class("suggested-action");
                }

                updating_ui.set(false);
            })
        };

        let refresh_cb = refresh_scx_widgets.clone();
        let toast_rc_refresh = toast_overlay_rc.clone();
        refresh_scx_btn.connect_clicked(move |_| {
            refresh_cb();
            if let Some(ref overlay) = *toast_rc_refresh {
                overlay.add_toast(Toast::new("Refreshed sched-ext kernel & package status"));
            }
        });

        let refresh_after_install = refresh_scx_widgets.clone();
        let status_row_install = scx_status_row.clone();
        let install_btn_clone = install_scx_btn.clone();
        let install_content_clone = install_scx_content.clone();
        let toast_rc_install = toast_overlay_rc.clone();
        install_scx_btn.connect_clicked(move |_| {
            install_btn_clone.set_sensitive(false);
            install_content_clone.set_label("Installing...");
            status_row_install.set_subtitle("Starting sched-ext package installation via Polkit...");

            enum InstallMsg {
                Line(String),
                Done(Result<scx::ScxInstallPlan, String>),
            }

            let (tx, rx) = std::sync::mpsc::channel::<InstallMsg>();
            std::thread::spawn(move || {
                let tx_line = tx.clone();
                let res = scx::install_scx_streaming(move |line| {
                    let _ = tx_line.send(InstallMsg::Line(line));
                });
                let _ = tx.send(InstallMsg::Done(res));
            });

            let status_row_poll = status_row_install.clone();
            let install_btn_poll = install_btn_clone.clone();
            let refresh_poll = refresh_after_install.clone();
            let toast_poll = toast_rc_install.clone();

            gtk4::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        InstallMsg::Line(line) => {
                            let clean = line.trim();
                            if !clean.is_empty() {
                                status_row_poll.set_subtitle(clean);
                            }
                        }
                        InstallMsg::Done(res) => {
                            install_btn_poll.set_sensitive(true);
                            refresh_poll();
                            match res {
                                Ok(plan) => {
                                    let note = if plan.requires_reboot_for_kernel
                                        && !scx::is_scx_supported()
                                    {
                                        "sched-ext installed! Reboot into the new sched-ext enabled kernel to activate."
                                    } else {
                                        "sched-ext (SCX) schedulers installed and ready!"
                                    };
                                    if let Some(ref overlay) = *toast_poll {
                                        overlay.add_toast(Toast::new(note));
                                    }
                                }
                                Err(e) => {
                                    if let Some(ref overlay) = *toast_poll {
                                        overlay.add_toast(Toast::new(&format!(
                                            "SCX installation failed: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            return gtk4::glib::ControlFlow::Break;
                        }
                    }
                }
                gtk4::glib::ControlFlow::Continue
            });
        });

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
            let refresh_scx = refresh_scx_widgets.clone();
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
                        refresh_scx();
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
            .subtitle("Writes configuration to /etc/sysctl.d/99-mithrandir-scheduler.conf and /etc/scx_loader/config.toml")
            .use_markup(false)
            .build();
        persist_row.add_prefix(&Image::from_icon_name("document-save-symbolic"));

        let persist_btn = Button::builder()
            .label("Persist Settings")
            .valign(Align::Center)
            .build();

        let toast_rc_persist = toast_overlay_rc.clone();
        let bore_switch_clone = bore_switch.clone();
        let scx_row_clone = scx_row.clone();
        let scx_mode_clone = scx_mode_row.clone();
        let scx_list_persist = scx_sched_list.clone();
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

            let scx_idx = scx_row_clone.selected() as usize;
            let selected_scx = scx_list_persist.borrow().get(scx_idx).cloned();
            let selected_mode = scx::ScxMode::from_u32(scx_mode_clone.selected());

            let res = run_privileged_helper(
                Some(b_val),
                g_val.as_deref(),
                e_val.as_deref(),
                Some(t_val),
                a_val.as_deref(),
                true,
            )
            .and_then(|_| {
                if let Some(ref sched) = selected_scx {
                    if scx::is_scx_supported() {
                        let _ = scx::set_scx_boot_persistence(sched, selected_mode);
                    }
                }
                Ok(())
            });

            match res {
                Ok(_) => {
                    if let Some(ref overlay) = *toast_rc_persist {
                        overlay.add_toast(Toast::new(
                            "Kernel, SCX & power settings persisted successfully",
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

fn format_applied_mode(scheduler: &str, mode_label: &str) -> String {
    format!("Applied {} mode to {}", mode_label, scheduler)
}
