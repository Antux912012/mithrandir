use adw::prelude::*;
use libadwaita as adw;
use crate::hardware::battery::BatteryInfo;
use crate::hardware::display::DisplayInfo;
use crate::hardware::memory::MemoryInfo;
use crate::hardware::storage::DiskInfo;
use crate::hardware::system::SystemInfo;
use super::helpers::{make_copy_row, make_search_action_row};

pub fn build_overview_view(
    sys: &SystemInfo,
    batteries: &[BatteryInfo],
    mem: &MemoryInfo,
    disks: &[DiskInfo],
    displays: &[DisplayInfo],
    overlay: Option<&adw::ToastOverlay>,
) -> gtk4::Widget {
    let page = adw::PreferencesPage::new();

    // Hero / Device Group
    let device_grp = adw::PreferencesGroup::builder()
        .title("System Overview")
        .description("Hardware summary and primary machine identification")
        .build();

    let model_row = make_copy_row(
        "Device Model",
        &sys.full_device_name(),
        &sys.full_device_name(),
        overlay,
        Some("computer-symbolic"),
    );
    device_grp.add(&model_row);

    let mtm_row = make_copy_row(
        "Machine Type Model (MTM)",
        &sys.product_name,
        &sys.product_name,
        overlay,
        Some("dialog-information-symbolic"),
    );
    device_grp.add(&mtm_row);

    let chassis_row = adw::ActionRow::builder()
        .title("Chassis Form Factor")
        .subtitle(&sys.chassis_type)
        .use_markup(false)
        .build();
    let chassis_icon = gtk4::Image::from_icon_name("preferences-desktop-display-symbolic");
    chassis_row.add_prefix(&chassis_icon);
    device_grp.add(&chassis_row);

    page.add(&device_grp);

    // Component Quick Summary Group
    let comp_grp = adw::PreferencesGroup::builder()
        .title("Quick Component Summary")
        .description("Core sub-assemblies detected")
        .build();

    // Processor
    let cpu_sub = format!("{} ({} Cores, {} Threads)", sys.cpu_model, sys.cpu_cores, sys.cpu_threads);
    let cpu_row = make_copy_row("Processor (CPU)", &cpu_sub, &sys.cpu_model, overlay, Some("processor-symbolic"));
    comp_grp.add(&cpu_row);

    // Memory
    let mem_sub = format!("{:.1} GB Total Installed ({})", mem.total_gib, mem.form_factor);
    let mem_row = adw::ActionRow::builder().title("Installed Memory").subtitle(&mem_sub).use_markup(false).build();
    mem_row.add_prefix(&gtk4::Image::from_icon_name("media-flash-symbolic"));
    comp_grp.add(&mem_row);

    // Storage
    if let Some(first_disk) = disks.first() {
        let disk_sub = format!("{} ({:.0} GB, {})", first_disk.model, first_disk.capacity_gb, first_disk.interface_type);
        let disk_row = adw::ActionRow::builder().title("Primary Storage").subtitle(&disk_sub).use_markup(false).build();
        disk_row.add_prefix(&gtk4::Image::from_icon_name("drive-harddisk-symbolic"));
        comp_grp.add(&disk_row);
    }

    // Battery
    if let Some(bat) = batteries.first() {
        let bat_sub = format!("{:.1}% Health ({}) — {:.1} Wh remaining", bat.health_percent, bat.status, bat.energy_now_wh);
        let bat_row = adw::ActionRow::builder().title("Battery Status").subtitle(&bat_sub).use_markup(false).build();
        let bat_icon = if bat.health_percent > 70.0 { "battery-good-symbolic" } else { "battery-caution-symbolic" };
        bat_row.add_prefix(&gtk4::Image::from_icon_name(bat_icon));
        comp_grp.add(&bat_row);
    }

    // Display
    if let Some(disp) = displays.first() {
        let disp_sub = format!("{} ({})", disp.panel_model, disp.resolution);
        let disp_row = adw::ActionRow::builder().title("Primary Display").subtitle(&disp_sub).use_markup(false).build();
        disp_row.add_prefix(&gtk4::Image::from_icon_name("video-display-symbolic"));
        comp_grp.add(&disp_row);
    }

    page.add(&comp_grp);

    // Firmware & Motherboard Group
    let fw_grp = adw::PreferencesGroup::builder()
        .title("Firmware &amp; Motherboard")
        .build();

    let bios_sub = format!("Version {} (Released {})", sys.bios_version, sys.bios_date);
    let bios_row = make_copy_row("UEFI / BIOS Firmware", &bios_sub, &sys.bios_version, overlay, Some("application-x-firmware-symbolic"));
    fw_grp.add(&bios_row);

    let board_row = adw::ActionRow::builder()
        .title("Motherboard ID")
        .subtitle(&sys.board_name)
        .use_markup(false)
        .build();
    board_row.add_prefix(&gtk4::Image::from_icon_name("system-run-symbolic"));
    fw_grp.add(&board_row);

    page.add(&fw_grp);

    // Replacement Search Action
    let search_grp = adw::PreferencesGroup::builder()
        .title("Replacement Lookup")
        .description("Quick query to search replacement parts for this exact unit")
        .build();
    let search_row = make_search_action_row(
        "Search Compatible Parts",
        &sys.replacement_search_query(),
        overlay,
    );
    search_grp.add(&search_row);
    page.add(&search_grp);

    page.upcast()
}
