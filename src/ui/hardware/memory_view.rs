use adw::prelude::*;
use libadwaita as adw;
use crate::hardware::memory::MemoryInfo;
use super::helpers::{make_copy_row, make_search_action_row};

pub fn build_memory_view(
    mem: &MemoryInfo,
    overlay: Option<&adw::ToastOverlay>,
) -> gtk4::Widget {
    let page = adw::PreferencesPage::new();

    // Memory Utilization Group
    let util_grp = adw::PreferencesGroup::builder()
        .title("Installed System Memory (RAM)")
        .description("Current physical capacity and operating status")
        .build();

    let mem_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let mem_label = gtk4::Label::builder()
        .label(&format!("{:.1} GB Installed Memory", mem.total_gib))
        .css_classes(["heading"])
        .halign(gtk4::Align::Start)
        .build();

    let used_fraction = (mem.used_gib / mem.total_gib.max(1.0)) as f64;
    let progress = gtk4::ProgressBar::builder()
        .fraction(used_fraction)
        .show_text(true)
        .text(&format!("{:.1} GB in use ({:.0}%) / {:.1} GB free", mem.used_gib, used_fraction * 100.0, mem.available_gib))
        .build();

    mem_box.append(&mem_label);
    mem_box.append(&progress);

    let card = gtk4::Frame::builder()
        .child(&mem_box)
        .css_classes(["card"])
        .build();
    util_grp.add(&card);

    let total_row = adw::ActionRow::builder()
        .title("Total Memory Recognized")
        .subtitle(&format!("{:.2} GiB", mem.total_gib))
        .build();
    total_row.add_prefix(&gtk4::Image::from_icon_name("media-flash-symbolic"));
    util_grp.add(&total_row);

    page.add(&util_grp);

    // RAM Upgrade & Replacement Group
    let upgrade_grp = adw::PreferencesGroup::builder()
        .title("Memory Upgrade & Replacement Guide")
        .description("Crucial specifications for purchasing compatible RAM modules")
        .build();

    let form_row = make_copy_row(
        "Module Form Factor",
        &format!("{} SO-DIMM (Laptop Standard)", mem.form_factor),
        &format!("{} SODIMM", mem.form_factor),
        overlay,
        Some("view-list-bullet-symbolic"),
    );
    upgrade_grp.add(&form_row);

    let pin_row = adw::ActionRow::builder()
        .title("Pin Configuration & Voltage")
        .subtitle(&mem.pin_count)
        .build();
    pin_row.add_prefix(&gtk4::Image::from_icon_name("dialog-information-symbolic"));
    upgrade_grp.add(&pin_row);

    let speed_row = make_copy_row(
        "Supported Speeds / Bus",
        &mem.speed_recommendation,
        &mem.speed_recommendation,
        overlay,
        Some("emblem-synchronizing-symbolic"),
    );
    upgrade_grp.add(&speed_row);

    let max_row = adw::ActionRow::builder()
        .title("Maximum System Capacity")
        .subtitle(&mem.max_capacity_estimate)
        .build();
    max_row.add_prefix(&gtk4::Image::from_icon_name("starred-symbolic"));
    upgrade_grp.add(&max_row);

    let search_row = make_search_action_row(
        "Find Compatible RAM Modules",
        &mem.recommended_search,
        overlay,
    );
    upgrade_grp.add(&search_row);

    page.add(&upgrade_grp);

    page.upcast()
}
