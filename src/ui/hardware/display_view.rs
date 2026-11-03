use adw::prelude::*;
use libadwaita as adw;
use crate::hardware::display::DisplayInfo;
use super::helpers::{make_copy_row, make_search_action_row};

pub fn build_display_view(
    displays: &[DisplayInfo],
    overlay: Option<&adw::ToastOverlay>,
) -> gtk4::Widget {
    let page = adw::PreferencesPage::new();

    if displays.is_empty() {
        let empty_grp = adw::PreferencesGroup::builder()
            .title("No Display Panels Detected")
            .description("No active DRM video outputs were found.")
            .build();
        page.add(&empty_grp);
        return page.upcast();
    }

    for disp in displays {
        let disp_grp = adw::PreferencesGroup::builder()
            .title(&format!("Display: {}", gtk4::glib::markup_escape_text(&disp.connector)))
            .description("Internal LCD panel and video output specifications")
            .build();

        let panel_row = make_copy_row(
            "Exact Panel Model Number (from EDID)",
            &disp.panel_model,
            &disp.panel_model,
            overlay,
            Some("video-display-symbolic"),
        );
        disp_grp.add(&panel_row);

        let mfg_row = adw::ActionRow::builder()
            .title("Panel Manufacturer")
            .subtitle(&disp.manufacturer)
            .use_markup(false)
            .build();
        mfg_row.add_prefix(&gtk4::Image::from_icon_name("org.gnome.Settings-symbolic"));
        disp_grp.add(&mfg_row);

        let res_row = make_copy_row(
            "Native Display Resolution",
            &disp.resolution,
            &disp.resolution,
            overlay,
            Some("preferences-desktop-display-symbolic"),
        );
        disp_grp.add(&res_row);

        let specs_sub = format!("{}, {}", disp.replacement_specs, disp.status);
        let specs_row = adw::ActionRow::builder()
            .title("Interface Connector & State")
            .subtitle(&specs_sub)
            .use_markup(false)
            .build();
        specs_row.add_prefix(&gtk4::Image::from_icon_name("dialog-information-symbolic"));
        disp_grp.add(&specs_row);

        let search_row = make_search_action_row(
            "Search Replacement Screen Online",
            &disp.recommended_search,
            overlay,
        );
        disp_grp.add(&search_row);

        page.add(&disp_grp);
    }

    page.upcast()
}
