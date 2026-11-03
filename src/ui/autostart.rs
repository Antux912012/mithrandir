use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Image, Orientation, ScrolledWindow, Switch, Window,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{ActionRow, ButtonContent, Clamp, EntryRow, PreferencesGroup, PreferencesPage, PreferencesWindow, SwitchRow};
use std::cell::RefCell;
use std::rc::Rc;
use crate::backend::autostart::AutostartManager;

pub struct AutostartPage;

impl AutostartPage {
    pub fn build() -> ScrolledWindow {
        let scrolled = ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();

        let root_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();

        // 1. Mithrandir Autostart Switch
        let m_group = PreferencesGroup::builder()
            .title("Mithrandir Background Launch")
            .description("Keep Mithrandir protecting and optimizing your system on boot")
            .build();

        let m_switch_row = SwitchRow::builder()
            .title("Start Mithrandir Automatically on Login")
            .subtitle("Launches in the background to monitor RAM, cache clutter, and sensors")
            .active(AutostartManager::is_mithrandir_autostart_enabled())
            .build();

        m_switch_row.connect_active_notify(|row| {
            let active = row.is_active();
            let _ = AutostartManager::set_mithrandir_autostart(active);
        });

        m_group.add(&m_switch_row);
        root_box.append(&m_group);

        // 2. Startup Applications Group (HIG-compliant PreferencesGroup with header suffix)
        let add_btn_content = ButtonContent::builder()
            .label("Add Startup App")
            .icon_name("list-add-symbolic")
            .build();
        let add_btn = Button::builder()
            .child(&add_btn_content)
            .valign(Align::Center)
            .css_classes(vec!["suggested-action"])
            .build();

        let apps_group = PreferencesGroup::builder()
            .title("Startup Applications")
            .description("Manage programs configured to launch automatically on login")
            .header_suffix(&add_btn)
            .build();
        root_box.append(&apps_group);

        let added_rows: Rc<RefCell<Vec<ActionRow>>> = Rc::new(RefCell::new(Vec::new()));

        let reload_entries: Rc<dyn Fn()> = {
            let group = apps_group.clone();
            let added_rows = added_rows.clone();

            Rc::new(move || {
                for old_row in added_rows.borrow_mut().drain(..) {
                    group.remove(&old_row);
                }

                let entries = AutostartManager::list_entries();
                if entries.is_empty() {
                    let empty_row = ActionRow::builder()
                        .title("No startup applications configured")
                        .subtitle("Click 'Add Startup App' to register a command or application on login")
                        .use_markup(false)
                        .build();
                    group.add(&empty_row);
                    added_rows.borrow_mut().push(empty_row);
                    return;
                }

                for e in entries {
                    let subtitle = if !e.comment.is_empty() && e.comment != e.name {
                        format!("{} — {}", e.comment, e.exec)
                    } else {
                        e.exec.clone()
                    };

                    let row = ActionRow::builder()
                        .title(&e.name)
                        .subtitle(&subtitle)
                        .use_markup(false)
                        .build();

                    let icon_name = if !e.icon.is_empty() {
                        e.icon.clone()
                    } else {
                        "application-x-executable-symbolic".to_string()
                    };
                    let img = Image::from_icon_name(&icon_name);
                    img.set_pixel_size(24);
                    row.add_prefix(&img);

                    let sw = Switch::builder()
                        .active(e.enabled)
                        .valign(Align::Center)
                        .build();

                    let e_clone = e.clone();
                    sw.connect_active_notify(move |s| {
                        let _ = AutostartManager::toggle_entry(&e_clone, s.is_active());
                    });
                    row.add_suffix(&sw);

                    if e.is_user {
                        let del_btn = Button::builder()
                            .icon_name("user-trash-symbolic")
                            .valign(Align::Center)
                            .css_classes(vec!["flat"])
                            .tooltip_text("Remove entry")
                            .build();

                        let e_del = e.clone();
                        let group_del = group.clone();
                        let row_del = row.clone();
                        let rows_del = added_rows.clone();

                        del_btn.connect_clicked(move |_| {
                            let _ = AutostartManager::remove_entry(&e_del);
                            rows_del.borrow_mut().retain(|r| r != &row_del);
                            group_del.remove(&row_del);
                        });
                        row.add_suffix(&del_btn);
                    }

                    group.add(&row);
                    added_rows.borrow_mut().push(row);
                }
            })
        };

        let reload_for_add = reload_entries.clone();
        add_btn.connect_clicked(move |btn| {
            let dlg = PreferencesWindow::builder()
                .title("Add Startup Application")
                .modal(true)
                .default_width(460)
                .default_height(320)
                .build();
            if let Some(root) = btn.root().and_then(|r| r.downcast::<Window>().ok()) {
                dlg.set_transient_for(Some(&root));
            }

            let page = PreferencesPage::new();
            let form_grp = PreferencesGroup::builder()
                .title("New Autostart Entry")
                .description("Creates a desktop launcher in ~/.config/autostart/")
                .build();

            let name_row = EntryRow::builder().title("Application Name").build();
            let exec_row = EntryRow::builder().title("Command / Executable").build();
            let comment_row = EntryRow::builder().title("Description (Optional)").build();

            form_grp.add(&name_row);
            form_grp.add(&exec_row);
            form_grp.add(&comment_row);

            let save_btn = Button::builder()
                .label("Save Startup Entry")
                .css_classes(vec!["suggested-action", "pill"])
                .halign(Align::End)
                .margin_top(12)
                .build();
            form_grp.add(&save_btn);

            let dlg_clone = dlg.clone();
            let reload_cb = reload_for_add.clone();
            save_btn.connect_clicked(move |_| {
                let name = name_row.text().trim().to_string();
                let exec = exec_row.text().trim().to_string();
                let comment = comment_row.text().trim().to_string();
                if !name.is_empty() && !exec.is_empty() {
                    if AutostartManager::add_entry(&name, &exec, &comment).is_ok() {
                        reload_cb();
                        dlg_clone.close();
                    }
                }
            });

            page.add(&form_grp);
            dlg.add(&page);
            dlg.present();
        });

        reload_entries();

        let clamp = Clamp::builder()
            .maximum_size(920)
            .child(&root_box)
            .build();
        scrolled.set_child(Some(&clamp));

        scrolled
    }
}
