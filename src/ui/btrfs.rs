use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, ListBox, Orientation, ScrolledWindow,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{ActionRow, ButtonContent, Clamp, PreferencesGroup};
use crate::backend::btrfs::BtrfsManager;
use crate::backend::indicator::NotificationManager;

pub struct BtrfsPage;

impl BtrfsPage {
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

        let is_btrfs = BtrfsManager::is_btrfs_available();

        let status_group = PreferencesGroup::builder()
            .title("BTRFS Subvolumes &amp; Snapshots")
            .description("Manage system restore points and subvolumes")
            .build();

        let status_row = ActionRow::builder()
            .title("Filesystem Status")
            .subtitle(if is_btrfs {
                "Root partition is formatted as BTRFS. Instant snapshots supported."
            } else {
                "Root partition is ext4/other. BTRFS snapshotting is inactive."
            })
            .use_markup(false)
            .build();

        let create_content = ButtonContent::builder()
            .label("Create Snapshot")
            .icon_name("list-add-symbolic")
            .build();
        let create_btn = Button::builder()
            .child(&create_content)
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .sensitive(is_btrfs)
            .build();
        status_row.add_suffix(&create_btn);
        status_group.add(&status_row);
        root_box.append(&status_group);

        // List Group
        let list_group = PreferencesGroup::builder()
            .title("Existing Snapshots")
            .build();

        let list_box = ListBox::builder()
            .css_classes(vec!["boxed-list"])
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        list_group.add(&list_box);
        root_box.append(&list_group);

        let populate_snapshots = {
            let list_box = list_box.clone();
            move || {
                while let Some(child) = list_box.first_child() {
                    list_box.remove(&child);
                }
                let snapshots = BtrfsManager::list_snapshots();
                if snapshots.is_empty() {
                    let empty_row = ActionRow::builder()
                        .title("No BTRFS snapshots detected")
                        .subtitle("Create a snapshot before upgrading packages or editing system files")
                        .use_markup(false)
                        .build();
                    list_box.append(&empty_row);
                } else {
                    for s in snapshots {
                        let row = ActionRow::builder()
                            .title(&s.path)
                            .subtitle(format!("Subvolume ID: {}", s.id))
                            .use_markup(false)
                            .build();
                        list_box.append(&row);
                    }
                }
            }
        };

        populate_snapshots();

        let btn_clone = create_btn.clone();
        let content_clone = create_content.clone();
        let pop_clone = populate_snapshots.clone();
        create_btn.connect_clicked(move |_| {
            btn_clone.set_sensitive(false);
            content_clone.set_label("Creating Snapshot...");
            let (tx, rx) = async_channel::unbounded();
            std::thread::spawn(move || {
                let res = BtrfsManager::create_snapshot("/");
                let _ = tx.send_blocking(res);
            });

            let b_ref = btn_clone.clone();
            let c_ref = content_clone.clone();
            let p_ref = pop_clone.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(res) = rx.recv().await {
                    b_ref.set_sensitive(true);
                    c_ref.set_label("Create Snapshot");
                    match res {
                        Ok(()) => {
                            NotificationManager::send_notification(
                                "📸 BTRFS Snapshot Created",
                                "System restore snapshot created successfully.",
                            );
                            p_ref();
                        }
                        Err(e) => {
                            NotificationManager::send_notification(
                                "⚠️ BTRFS Snapshot Failed",
                                &e,
                            );
                        }
                    }
                }
            });
        });

        let clamp = Clamp::builder()
            .maximum_size(880)
            .child(&root_box)
            .build();
        scrolled.set_child(Some(&clamp));

        scrolled
    }
}
