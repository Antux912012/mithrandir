use adw::prelude::*;
use libadwaita as adw;
use crate::hardware::network::NetworkCardInfo;
use super::helpers::{make_copy_row, make_search_action_row};

pub fn build_network_view(
    adapters: &[NetworkCardInfo],
    overlay: Option<&adw::ToastOverlay>,
) -> gtk4::Widget {
    let page = adw::PreferencesPage::new();

    if adapters.is_empty() {
        let empty_grp = adw::PreferencesGroup::builder()
            .title("No Network Interfaces Found")
            .description("No active physical networking devices detected.")
            .build();
        page.add(&empty_grp);
        return page.upcast();
    }

    for adapter in adapters {
        let net_grp = adw::PreferencesGroup::builder()
            .title(&format!("Adapter: {}", gtk4::glib::markup_escape_text(&adapter.interface_name)))
            .description(gtk4::glib::markup_escape_text(&adapter.device_type))
            .build();

        let model_row = make_copy_row(
            "Installed Chipset",
            &adapter.chipset_name,
            &adapter.chipset_name,
            overlay,
            Some("network-wireless-symbolic"),
        );
        net_grp.add(&model_row);

        let form_row = make_copy_row(
            "Slot / Form Factor",
            &adapter.form_factor,
            &adapter.form_factor,
            overlay,
            Some("preferences-system-network-symbolic"),
        );
        net_grp.add(&form_row);

        let up_row = adw::ActionRow::builder()
            .title("Upgrade Compatibility")
            .subtitle(&adapter.upgrade_option)
            .use_markup(false)
            .build();
        up_row.add_prefix(&gtk4::Image::from_icon_name("starred-symbolic"));
        net_grp.add(&up_row);

        let search_row = make_search_action_row(
            "Search Compatible Wireless Card Upgrades",
            &adapter.recommended_search,
            overlay,
        );
        net_grp.add(&search_row);

        page.add(&net_grp);
    }

    page.upcast()
}
