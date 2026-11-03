use adw::prelude::*;
use libadwaita as adw;

pub fn make_copy_row(
    title: &str,
    subtitle: &str,
    copy_text: &str,
    toast_overlay: Option<&adw::ToastOverlay>,
    icon_name: Option<&str>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .use_markup(false)
        .build();

    if let Some(icon) = icon_name {
        let img = gtk4::Image::from_icon_name(icon);
        img.add_css_class("accent");
        row.add_prefix(&img);
    }

    let copy_btn = gtk4::Button::builder()
        .icon_name("edit-copy-symbolic")
        .valign(gtk4::Align::Center)
        .has_frame(false)
        .tooltip_text("Copy to clipboard")
        .build();

    let text_to_copy = copy_text.to_string();
    let overlay_opt = toast_overlay.cloned();
    copy_btn.connect_clicked(move |btn| {
        let display = btn.display();
        display.clipboard().set_text(&text_to_copy);
        if let Some(ref overlay) = overlay_opt {
            let toast = adw::Toast::new(&format!("Copied: {}", text_to_copy));
            toast.set_timeout(2);
            overlay.add_toast(toast);
        }
    });

    row.add_suffix(&copy_btn);
    row
}

pub fn make_search_action_row(
    title: &str,
    search_query: &str,
    toast_overlay: Option<&adw::ToastOverlay>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(search_query)
        .use_markup(false)
        .build();

    let img = gtk4::Image::from_icon_name("system-search-symbolic");
    img.add_css_class("accent");
    row.add_prefix(&img);

    let btn_content = adw::ButtonContent::builder()
        .label("Copy Search Query")
        .icon_name("edit-copy-symbolic")
        .build();
    let copy_btn = gtk4::Button::builder()
        .child(&btn_content)
        .valign(gtk4::Align::Center)
        .css_classes(["suggested-action"])
        .tooltip_text("Copy search query for online marketplaces")
        .build();

    let query = search_query.to_string();
    let overlay_opt = toast_overlay.cloned();
    copy_btn.connect_clicked(move |btn| {
        let display = btn.display();
        display.clipboard().set_text(&query);
        if let Some(ref overlay) = overlay_opt {
            let toast = adw::Toast::new("Search query copied to clipboard");
            toast.set_timeout(2);
            overlay.add_toast(toast);
        }
    });

    row.add_suffix(&copy_btn);
    row
}
