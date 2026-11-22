use adw::prelude::*;
use libadwaita as adw;
use std::path::PathBuf;
use crate::hardware::battery::BatteryInfo;
use super::helpers::{make_copy_row, make_search_action_row};

pub fn build_battery_view(
    batteries: &[BatteryInfo],
    device_model: &str,
    overlay: Option<&adw::ToastOverlay>,
) -> gtk4::Widget {
    let page = adw::PreferencesPage::new();

    if batteries.is_empty() {
        let empty_grp = adw::PreferencesGroup::builder()
            .title("No Battery Detected")
            .description("No internal battery was found on this system (desktop or virtualized).")
            .build();
        page.add(&empty_grp);
        return page.upcast();
    }

    // ---------------------------------------------------------
    // 1. Battery Diagnostic Report Export & Preview Group
    // ---------------------------------------------------------
    let report_grp = adw::PreferencesGroup::builder()
        .title("Battery Diagnostic Report")
        .description("Full telemetry snapshot covering charge/discharge cycles, electrical state, thresholds, and EC/BIOS firmware")
        .build();

    let report_md = BatteryInfo::generate_markdown_report(batteries, device_model);
    let report_text = batteries
        .iter()
        .map(|b| b.generate_text_report(device_model))
        .collect::<Vec<_>>()
        .join("\n\n");

    let report_actions_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .valign(gtk4::Align::Center)
        .build();

    let copy_report_content = adw::ButtonContent::builder()
        .label("Copy Report")
        .icon_name("edit-copy-symbolic")
        .build();
    let copy_report_btn = gtk4::Button::builder()
        .child(&copy_report_content)
        .valign(gtk4::Align::Center)
        .build();

    let export_report_content = adw::ButtonContent::builder()
        .label("Export (.md)")
        .icon_name("document-save-symbolic")
        .build();
    let export_report_btn = gtk4::Button::builder()
        .child(&export_report_content)
        .css_classes(["suggested-action"])
        .valign(gtk4::Align::Center)
        .build();

    {
        let md_for_copy = report_md.clone();
        let overlay_copy = overlay.cloned();
        copy_report_btn.connect_clicked(move |btn| {
            btn.display().clipboard().set_text(&md_for_copy);
            if let Some(ref ov) = overlay_copy {
                let toast = adw::Toast::new("Battery diagnostic report copied to clipboard");
                toast.set_timeout(3);
                ov.add_toast(toast);
            }
        });
    }

    {
        let bats_clone = batteries.to_vec();
        let model_clone = device_model.to_string();
        let overlay_export = overlay.cloned();
        export_report_btn.connect_clicked(move |btn| {
            let base_dir = std::env::var("HOME")
                .map(PathBuf::from)
                .map(|home| {
                    let docs = home.join("Documents");
                    if docs.is_dir() { docs } else { home }
                })
                .unwrap_or_else(|_| PathBuf::from("/tmp"));
            let filename = format!("mithrandir-battery-report-{}.md", current_date_stamp());
            let target = base_dir.join(filename);

            match BatteryInfo::export_report_to_file(&bats_clone, &model_clone, &target) {
                Ok(saved_path) => {
                    btn.display().clipboard().set_text(&saved_path);
                    if let Some(ref ov) = overlay_export {
                        let toast = adw::Toast::new(&format!("Battery report saved to {}", saved_path));
                        toast.set_timeout(4);
                        ov.add_toast(toast);
                    }
                }
                Err(err) => {
                    if let Some(ref ov) = overlay_export {
                        let toast = adw::Toast::new(&format!("Export failed: {}", err));
                        toast.set_timeout(4);
                        ov.add_toast(toast);
                    }
                }
            }
        });
    }

    report_actions_box.append(&copy_report_btn);
    report_actions_box.append(&export_report_btn);

    let report_action_row = adw::ActionRow::builder()
        .title("Generate &amp; Export Battery Report")
        .subtitle(&format!(
            "{} battery unit(s) detected — copy formatted Markdown or save to ~/Documents",
            batteries.len()
        ))
        .use_markup(false)
        .build();
    report_action_row.set_title("Generate & Export Battery Report");
    report_action_row.add_prefix(&gtk4::Image::from_icon_name("document-properties-symbolic"));
    report_action_row.add_suffix(&report_actions_box);
    report_grp.add(&report_action_row);

    // Inline expandable formatted report preview
    let report_preview_expander = adw::ExpanderRow::builder()
        .title("Inspect Full Diagnostic Report")
        .subtitle("Expand to view raw cycles, voltage, capacity, thresholds, and EC/BIOS firmware report")
        .use_markup(false)
        .build();
    report_preview_expander.add_prefix(&gtk4::Image::from_icon_name("utilities-terminal-symbolic"));

    let report_label = gtk4::Label::builder()
        .label(&report_text)
        .selectable(true)
        .xalign(0.0)
        .wrap(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(14)
        .margin_end(14)
        .css_classes(["monospace"])
        .build();
    let preview_row = gtk4::ListBoxRow::builder()
        .activatable(false)
        .selectable(false)
        .child(&report_label)
        .build();
    report_preview_expander.add_row(&preview_row);
    report_grp.add(&report_preview_expander);

    page.add(&report_grp);

    for bat in batteries {
        // ---------------------------------------------------------
        // 2. Battery Health, Status & Cycles Group
        // ---------------------------------------------------------
        let health_grp = adw::PreferencesGroup::builder()
            .title(&format!(
                "Battery Health &amp; Cycles ({})",
                gtk4::glib::markup_escape_text(&bat.name)
            ))
            .description("Real-time capacity wear, charge/discharge cycle count, and charge thresholds")
            .build();

        // Health Progress Bar Card
        let health_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        let health_label = gtk4::Label::builder()
            .label(&format!(
                "Battery Health: {:.1}%  (Wear: {:.1}%)",
                bat.health_percent, bat.wear_percent
            ))
            .css_classes(["heading"])
            .halign(gtk4::Align::Start)
            .build();

        let progress = gtk4::ProgressBar::builder()
            .fraction((bat.health_percent / 100.0) as f64)
            .show_text(true)
            .text(&format!(
                "{:.2} Wh Full / {:.2} Wh Design ({:.0} / {:.0} mAh)",
                bat.energy_full_wh,
                bat.energy_full_design_wh,
                bat.charge_full_mah,
                bat.charge_full_design_mah
            ))
            .build();

        if bat.health_percent < 50.0 {
            progress.add_css_class("error");
        } else if bat.health_percent < 75.0 {
            progress.add_css_class("warning");
        } else {
            progress.add_css_class("success");
        }

        health_box.append(&health_label);
        health_box.append(&progress);

        let card = gtk4::Frame::builder()
            .child(&health_box)
            .css_classes(["card"])
            .build();
        health_grp.add(&card);

        // Operating Status & AC Mains
        let eta_str = bat
            .time_remaining_minutes
            .map(|m| {
                let verb = if bat.status.to_lowercase().contains("charging")
                    && !bat.status.to_lowercase().contains("discharging")
                {
                    "to full"
                } else {
                    "remaining"
                };
                format!(" • Est. {} {}", BatteryInfo::format_duration(m), verb)
            })
            .unwrap_or_default();

        let status_row = adw::ActionRow::builder()
            .title("Battery Status & Charge Level")
            .subtitle(&format!(
                "{} • {}% ({} level){}",
                bat.status, bat.percentage, bat.capacity_level, eta_str
            ))
            .use_markup(false)
            .build();
        status_row.add_prefix(&gtk4::Image::from_icon_name("battery-symbolic"));
        health_grp.add(&status_row);

        if let Some(online) = bat.ac_adapter_online {
            let ac_name = bat.ac_adapter_name.as_deref().unwrap_or("Mains");
            let ac_row = adw::ActionRow::builder()
                .title("AC Power Adapter")
                .subtitle(&format!(
                    "{} ({})",
                    if online { "Connected / Online" } else { "Disconnected / Running on Battery" },
                    ac_name
                ))
                .use_markup(false)
                .build();
            ac_row.add_prefix(&gtk4::Image::from_icon_name("ac-adapter-symbolic"));
            health_grp.add(&ac_row);
        }

        // Charge / Discharge Cycle Count
        let cycles_sub = if bat.cycle_count_reported {
            format!("{} cycles — {}", bat.cycle_count, bat.cycle_wear_rating)
        } else {
            "Not reported by battery firmware / EC".to_string()
        };
        let cycles_row = adw::ActionRow::builder()
            .title("Charge / Discharge Cycles")
            .subtitle(&cycles_sub)
            .use_markup(false)
            .build();
        cycles_row.add_prefix(&gtk4::Image::from_icon_name("view-refresh-symbolic"));
        health_grp.add(&cycles_row);

        // Charge Thresholds & Behaviour (when exposed by ThinkPad/ASUS/Dell/EC driver)
        if bat.charge_start_threshold.is_some()
            || bat.charge_end_threshold.is_some()
            || bat.charge_behaviour.is_some()
        {
            let start_s = bat
                .charge_start_threshold
                .map(|v| format!("{}%", v))
                .unwrap_or_else(|| "N/A".into());
            let end_s = bat
                .charge_end_threshold
                .map(|v| format!("{}%", v))
                .unwrap_or_else(|| "N/A".into());
            let beh_s = bat
                .charge_behaviour
                .as_deref()
                .map(|b| format!(" • Behaviour: {}", b))
                .unwrap_or_default();
            let thresh_row = adw::ActionRow::builder()
                .title("Charge Thresholds & Policy")
                .subtitle(&format!("Start: {} • Stop: {}{}", start_s, end_s, beh_s))
                .use_markup(false)
                .build();
            thresh_row.add_prefix(&gtk4::Image::from_icon_name("security-high-symbolic"));
            health_grp.add(&thresh_row);
        }

        page.add(&health_grp);

        // ---------------------------------------------------------
        // 3. Electrical & Thermal Telemetry Group
        // ---------------------------------------------------------
        let elec_grp = adw::PreferencesGroup::builder()
            .title("Electrical &amp; Thermal Telemetry")
            .description("Live voltage rails, power draw, current, stored energy, and temperature")
            .build();

        let max_v_suffix = bat
            .voltage_max_design_v
            .map(|v| format!(" • Max Design: {:.2} V", v))
            .unwrap_or_default();
        let volt_sub = format!(
            "{:.3} V live (Nominal Design: {:.2} V{})",
            bat.voltage_now_v, bat.voltage_design_v, max_v_suffix
        );
        let volt_row = adw::ActionRow::builder()
            .title("Voltage Telemetry")
            .subtitle(&volt_sub)
            .use_markup(false)
            .build();
        volt_row.add_prefix(&gtk4::Image::from_icon_name("preferences-system-symbolic"));
        elec_grp.add(&volt_row);

        let rate_dir = if bat.power_rate_w <= 0.05 {
            "Idle / Trickle".to_string()
        } else if bat.status.to_lowercase().contains("discharging") {
            format!("-{:.2} W discharging ({:.0} mA)", bat.power_rate_w, bat.current_now_ma)
        } else if bat.status.to_lowercase().contains("charging") {
            format!("+{:.2} W charging ({:.0} mA)", bat.power_rate_w, bat.current_now_ma)
        } else {
            format!("{:.2} W ({:.0} mA)", bat.power_rate_w, bat.current_now_ma)
        };
        let rate_row = adw::ActionRow::builder()
            .title("Power & Current Draw")
            .subtitle(&rate_dir)
            .use_markup(false)
            .build();
        rate_row.add_prefix(&gtk4::Image::from_icon_name("power-profile-balanced-symbolic"));
        elec_grp.add(&rate_row);

        let cap_sub = format!(
            "{:.2} Wh remaining ({:.0} mAh) • {:.2} Wh last full • {:.2} Wh design",
            bat.energy_now_wh, bat.charge_now_mah, bat.energy_full_wh, bat.energy_full_design_wh
        );
        let cap_row = adw::ActionRow::builder()
            .title("Energy & Charge Capacity")
            .subtitle(&cap_sub)
            .use_markup(false)
            .build();
        cap_row.add_prefix(&gtk4::Image::from_icon_name("drive-harddisk-symbolic"));
        elec_grp.add(&cap_row);

        if let Some(temp) = bat.temperature_c {
            let temp_row = adw::ActionRow::builder()
                .title("Battery Pack Temperature")
                .subtitle(&format!("{:.1} °C", temp))
                .use_markup(false)
                .build();
            temp_row.add_prefix(&gtk4::Image::from_icon_name("weather-clear-symbolic"));
            elec_grp.add(&temp_row);
        }

        if let Some(alarm) = bat.alarm_wh {
            let alarm_row = adw::ActionRow::builder()
                .title("Low-Battery Firmware Alarm")
                .subtitle(&format!("{:.2} Wh threshold", alarm))
                .use_markup(false)
                .build();
            alarm_row.add_prefix(&gtk4::Image::from_icon_name("dialog-warning-symbolic"));
            elec_grp.add(&alarm_row);
        }

        page.add(&elec_grp);

        // ---------------------------------------------------------
        // 4. Battery & Embedded Controller Firmware Group
        // ---------------------------------------------------------
        let fw_grp = adw::PreferencesGroup::builder()
            .title("Battery &amp; Controller Firmware")
            .description("Embedded Controller (EC), BIOS/UEFI power management, and ACPI node details")
            .build();

        let ec_row = make_copy_row(
            "Embedded Controller (EC) Firmware",
            &bat.firmware.ec_firmware_release,
            &bat.firmware.ec_firmware_release,
            overlay,
            Some("application-x-firmware-symbolic"),
        );
        fw_grp.add(&ec_row);

        let bios_sub = format!(
            "{} • Version {} (Released {})",
            bat.firmware.bios_vendor, bat.firmware.bios_version, bat.firmware.bios_date
        );
        let bios_row = make_copy_row(
            "System BIOS / UEFI Firmware",
            &bios_sub,
            &bat.firmware.bios_version,
            overlay,
            Some("computer-symbolic"),
        );
        fw_grp.add(&bios_row);

        let acpi_row = adw::ActionRow::builder()
            .title("ACPI Power Supply Device")
            .subtitle(&format!("{} (/sys/class/power_supply/{})", bat.firmware.acpi_device, bat.name))
            .use_markup(false)
            .build();
        acpi_row.add_prefix(&gtk4::Image::from_icon_name("system-run-symbolic"));
        fw_grp.add(&acpi_row);

        if let Some(ref mdate) = bat.firmware.manufacture_date {
            let mdate_row = adw::ActionRow::builder()
                .title("Battery Manufacture Date")
                .subtitle(mdate)
                .use_markup(false)
                .build();
            mdate_row.add_prefix(&gtk4::Image::from_icon_name("x-office-calendar-symbolic"));
            fw_grp.add(&mdate_row);
        }

        page.add(&fw_grp);

        // ---------------------------------------------------------
        // 5. Replacement Parts Group
        // ---------------------------------------------------------
        let part_grp = adw::PreferencesGroup::builder()
            .title("Replacement Battery Information")
            .description("Official part numbers and specifications for online purchasing")
            .build();

        let fru_sub = format!("{} (Model: {})", bat.fru_number, bat.model_name);
        let fru_row = make_copy_row(
            "Primary Part / FRU Number",
            &fru_sub,
            &bat.fru_number,
            overlay,
            Some("emblem-default-symbolic"),
        );
        part_grp.add(&fru_row);

        let mfg_sub = format!("{} • {} (Serial: {})", bat.manufacturer, bat.technology, bat.serial_number);
        let mfg_row = adw::ActionRow::builder()
            .title("Cell Manufacturer")
            .subtitle(&mfg_sub)
            .use_markup(false)
            .build();
        mfg_row.add_prefix(&gtk4::Image::from_icon_name("org.gnome.Settings-symbolic"));
        part_grp.add(&mfg_row);

        let cell_desc = if bat.estimated_cells > 0 {
            format!("{}-Cell {}", bat.estimated_cells, bat.technology)
        } else {
            bat.technology.clone()
        };
        let specs_sub = format!(
            "{:.2} V, {:.1} Wh ({})",
            bat.voltage_design_v, bat.energy_full_design_wh, cell_desc
        );
        let specs_row = adw::ActionRow::builder()
            .title("Target Form Factor & Ratings")
            .subtitle(&specs_sub)
            .use_markup(false)
            .build();
        specs_row.add_prefix(&gtk4::Image::from_icon_name("dialog-information-symbolic"));
        part_grp.add(&specs_row);

        // Compatible Alternatives Expander
        if !bat.compatible_frus.is_empty() {
            let expander = adw::ExpanderRow::builder()
                .title("Interchangeable / Compatible FRU Numbers")
                .subtitle("Alternative OEM parts directly compatible with this machine")
                .use_markup(false)
                .build();
            expander.add_prefix(&gtk4::Image::from_icon_name("emblem-documents-symbolic"));

            for compat in &bat.compatible_frus {
                let code_only = compat.split_whitespace().next().unwrap_or(compat);
                let row = make_copy_row(
                    "Compatible FRU",
                    compat,
                    code_only,
                    overlay,
                    Some("starred-symbolic"),
                );
                expander.add_row(&row);
            }
            part_grp.add(&expander);
        }

        // Online Parts Search Row
        let search_row = make_search_action_row(
            "Search Replacement Battery Online",
            &bat.recommended_search,
            overlay,
        );
        part_grp.add(&search_row);

        page.add(&part_grp);
    }

    page.upcast()
}

fn current_date_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    let mut tm = libc::tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    unsafe {
        libc::localtime_r(&secs, &mut tm);
    }
    format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday
    )
}
