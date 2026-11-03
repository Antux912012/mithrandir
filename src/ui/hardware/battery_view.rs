use adw::prelude::*;
use libadwaita as adw;
use crate::hardware::battery::BatteryInfo;
use super::helpers::{make_copy_row, make_search_action_row};

pub fn build_battery_view(
    batteries: &[BatteryInfo],
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

    for bat in batteries {
        // Battery Health Group
        let health_grp = adw::PreferencesGroup::builder()
            .title(&format!("Battery Health &amp; Status ({})", gtk4::glib::markup_escape_text(&bat.name)))
            .description("Real-time battery metrics and capacity wear level")
            .build();

        // Health Progress Bar Row
        let health_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        let health_label = gtk4::Label::builder()
            .label(&format!("Battery Health: {:.1}%", bat.health_percent))
            .css_classes(["heading"])
            .halign(gtk4::Align::Start)
            .build();

        let progress = gtk4::ProgressBar::builder()
            .fraction((bat.health_percent / 100.0) as f64)
            .show_text(true)
            .text(&format!("{:.1} Wh / {:.1} Wh Design", bat.energy_full_wh, bat.energy_full_design_wh))
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

        // Metrics Rows
        let status_row = adw::ActionRow::builder()
            .title("Operating State")
            .subtitle(&format!("{} (Charge: {}%)", bat.status, bat.percentage))
            .use_markup(false)
            .build();
        status_row.add_prefix(&gtk4::Image::from_icon_name("battery-symbolic"));
        health_grp.add(&status_row);

        let charge_sub = format!("{:.2} Wh current charge rate", bat.power_rate_w);
        let rate_row = adw::ActionRow::builder()
            .title("Power Rate")
            .subtitle(&charge_sub)
            .use_markup(false)
            .build();
        rate_row.add_prefix(&gtk4::Image::from_icon_name("power-profile-balanced-symbolic"));
        health_grp.add(&rate_row);

        let volt_sub = format!("{:.2} V (Nominal Rating: {:.2} V)", bat.voltage_now_v, bat.voltage_design_v);
        let volt_row = adw::ActionRow::builder()
            .title("Voltage Rating")
            .subtitle(&volt_sub)
            .use_markup(false)
            .build();
        volt_row.add_prefix(&gtk4::Image::from_icon_name("preferences-system-symbolic"));
        health_grp.add(&volt_row);

        let cycles_sub = if bat.cycle_count > 0 { format!("{} cycles", bat.cycle_count) } else { "Not reported by firmware".into() };
        let cycles_row = adw::ActionRow::builder()
            .title("Cycle Count")
            .subtitle(&cycles_sub)
            .use_markup(false)
            .build();
        cycles_row.add_prefix(&gtk4::Image::from_icon_name("view-refresh-symbolic"));
        health_grp.add(&cycles_row);

        page.add(&health_grp);

        // Replacement Parts Group
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

        let specs_sub = format!("{:.1} V, {:.1} Wh (3-Cell Lithium-Polymer)", bat.voltage_design_v, bat.energy_full_design_wh);
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
