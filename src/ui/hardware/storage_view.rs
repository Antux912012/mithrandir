use adw::prelude::*;
use libadwaita as adw;
use crate::hardware::storage::DiskInfo;
use super::helpers::{make_copy_row, make_search_action_row};

pub fn build_storage_view(
    disks: &[DiskInfo],
    overlay: Option<&adw::ToastOverlay>,
) -> gtk4::Widget {
    let page = adw::PreferencesPage::new();

    if disks.is_empty() {
        let empty_grp = adw::PreferencesGroup::builder()
            .title("No Storage Drives Detected")
            .description("No block devices were accessible.")
            .build();
        page.add(&empty_grp);
        return page.upcast();
    }

    for disk in disks {
        let disk_grp = adw::PreferencesGroup::builder()
            .title(&format!("Drive: {}", gtk4::glib::markup_escape_text(&disk.model)))
            .description(&format!(
                "Physical Device {} ({:.0} GB)",
                gtk4::glib::markup_escape_text(&disk.name),
                disk.capacity_gb
            ))
            .build();

        let model_row = make_copy_row(
            "Installed SSD Model",
            &disk.model,
            &disk.model,
            overlay,
            Some("drive-harddisk-symbolic"),
        );
        disk_grp.add(&model_row);

        let cap_row = adw::ActionRow::builder()
            .title("Capacity")
            .subtitle(&format!("{:.1} GB", disk.capacity_gb))
            .use_markup(false)
            .build();
        cap_row.add_prefix(&gtk4::Image::from_icon_name("folder-symbolic"));
        disk_grp.add(&cap_row);

        let iface_row = make_copy_row(
            "Drive Interface",
            &disk.interface_type,
            &disk.interface_type,
            overlay,
            Some("preferences-system-network-symbolic"),
        );
        disk_grp.add(&iface_row);

        let form_row = make_copy_row(
            "Physical Form Factor",
            &disk.form_factor,
            &disk.form_factor,
            overlay,
            Some("view-grid-symbolic"),
        );
        disk_grp.add(&form_row);

        let rec_row = adw::ActionRow::builder()
            .title("Upgrade Compatibility")
            .subtitle(&disk.upgrade_recommendation)
            .use_markup(false)
            .build();
        rec_row.add_prefix(&gtk4::Image::from_icon_name("emblem-ok-symbolic"));
        disk_grp.add(&rec_row);

        let search_row = make_search_action_row(
            "Search Compatible SSD Upgrades",
            &disk.recommended_search,
            overlay,
        );
        disk_grp.add(&search_row);

        page.add(&disk_grp);
    }

    page.upcast()
}
