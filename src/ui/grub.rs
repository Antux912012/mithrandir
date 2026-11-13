use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Entry, Image, Orientation, ScrolledWindow, SpinButton,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{ActionRow, ButtonContent, Clamp, ExpanderRow, PreferencesGroup, Toast, ToastOverlay};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::backend::grub::{GrubLiveStatus, GrubManager};
use crate::backend::indicator::NotificationManager;

pub struct GrubPage;

impl GrubPage {
    pub fn build(toast_overlay: Option<ToastOverlay>) -> ScrolledWindow {
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

        let cfg = GrubManager::read_config();
        let toast_overlay_rc = Rc::new(toast_overlay);

        // ---------------------------------------------------------
        // 1. Real-Time Monitoring & Telemetry Group
        // ---------------------------------------------------------
        let live_group = PreferencesGroup::builder()
            .title("Live GRUB File Telemetry")
            .description("Real-time monitoring of host bootloader state and configuration (/etc/default/grub &amp; grub.cfg)")
            .build();

        let default_path_row = ActionRow::builder()
            .title("Master Configuration (/etc/default/grub)")
            .subtitle(format_default_subtitle(&cfg.live_status))
            .use_markup(false)
            .build();
        default_path_row.add_prefix(&Image::from_icon_name("document-properties-symbolic"));

        let refresh_btn = Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Check Live Status Now")
            .valign(Align::Center)
            .css_classes(vec!["flat"])
            .build();
        default_path_row.add_suffix(&refresh_btn);
        live_group.add(&default_path_row);

        let cfg_path_row = ActionRow::builder()
            .title("Bootloader Runtime Output (grub.cfg)")
            .subtitle(format_cfg_subtitle(&cfg.live_status))
            .use_markup(false)
            .build();
        cfg_path_row.add_prefix(&Image::from_icon_name("application-x-firmware-symbolic"));
        live_group.add(&cfg_path_row);

        root_box.append(&live_group);

        // ---------------------------------------------------------
        // 2. Backup & Disaster Recovery Group
        // ---------------------------------------------------------
        let backup_group = PreferencesGroup::builder()
            .title("Configuration Backup &amp; Disaster Recovery")
            .description("Create, monitor, and restore original boot configurations")
            .build();

        let default_backup_row = ActionRow::builder()
            .title("Master Config Backup (/etc/default/grub.bak)")
            .subtitle(format_file_backup_subtitle(
                &cfg.live_status.default_backup_path,
                cfg.live_status.default_backup_exists,
                cfg.live_status.default_backup_size_bytes,
                &cfg.live_status.default_backup_modified_time,
            ))
            .use_markup(false)
            .build();
        default_backup_row.add_prefix(&Image::from_icon_name("document-save-symbolic"));
        backup_group.add(&default_backup_row);

        let cfg_backup_row = ActionRow::builder()
            .title("Bootloader Config Backup (grub.cfg.bak)")
            .subtitle(format_file_backup_subtitle(
                &cfg.live_status.backup_path,
                cfg.live_status.backup_exists,
                cfg.live_status.backup_size_bytes,
                &cfg.live_status.backup_modified_time,
            ))
            .use_markup(false)
            .build();
        cfg_backup_row.add_prefix(&Image::from_icon_name("document-save-symbolic"));
        backup_group.add(&cfg_backup_row);

        let backup_actions_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .valign(Align::Center)
            .build();

        let create_bak_content = ButtonContent::builder()
            .label("Create Backups (.bak)")
            .icon_name("document-save-symbolic")
            .build();
        let create_bak_btn = Button::builder()
            .child(&create_bak_content)
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .build();

        let restore_bak_content = ButtonContent::builder()
            .label("Restore Configuration")
            .icon_name("edit-undo-symbolic")
            .build();
        let restore_bak_btn = Button::builder()
            .child(&restore_bak_content)
            .css_classes(vec!["destructive-action"])
            .sensitive(cfg.live_status.backup_exists || cfg.live_status.default_backup_exists)
            .valign(Align::Center)
            .build();

        backup_actions_box.append(&create_bak_btn);
        backup_actions_box.append(&restore_bak_btn);

        let backup_actions_row = ActionRow::builder()
            .title("Disaster Recovery Actions")
            .subtitle("Generate an instant backup or recover the original bootloader state")
            .use_markup(false)
            .build();
        backup_actions_row.add_prefix(&Image::from_icon_name("security-high-symbolic"));
        backup_actions_row.add_suffix(&backup_actions_box);
        backup_group.add(&backup_actions_row);

        root_box.append(&backup_group);

        // ---------------------------------------------------------
        // 3. Bootloader Parameters Group
        // ---------------------------------------------------------
        let params_group = PreferencesGroup::builder()
            .title("GRUB Bootloader Parameters")
            .description("Customize system boot menu delay, default kernel, and boot arguments")
            .build();

        // Timeout row
        let timeout_row = ActionRow::builder()
            .title("Boot Menu Timeout (seconds)")
            .subtitle("Seconds before automatically booting default selection (0 = immediate)")
            .use_markup(false)
            .build();

        let timeout_spin = SpinButton::with_range(0.0, 60.0, 1.0);
        timeout_spin.set_value(cfg.timeout as f64);
        timeout_spin.set_valign(Align::Center);
        timeout_row.add_suffix(&timeout_spin);
        params_group.add(&timeout_row);

        // Default entry row
        let default_row = ActionRow::builder()
            .title("Default Boot Entry")
            .subtitle("Index (0, 1, ...) or name of the kernel to boot by default")
            .use_markup(false)
            .build();

        let default_entry = Entry::builder()
            .text(&cfg.default_entry)
            .valign(Align::Center)
            .width_request(240)
            .build();
        default_row.add_suffix(&default_entry);
        params_group.add(&default_row);

        // Detected Boot Menu Entries Expander
        let entries_subtitle = if cfg.menu_entries.is_empty() {
            "No boot menu entries detected in grub.cfg or /boot".to_string()
        } else {
            format!("{} boot entry/kernel image(s) detected — expand to select default", cfg.menu_entries.len())
        };
        let entries_expander = ExpanderRow::builder()
            .title("Detected Boot Menu Entries")
            .subtitle(&entries_subtitle)
            .use_markup(false)
            .enable_expansion(!cfg.menu_entries.is_empty())
            .build();
        entries_expander.add_prefix(&Image::from_icon_name("view-list-symbolic"));

        for (idx, entry_title) in cfg.menu_entries.iter().enumerate() {
            let entry_row = ActionRow::builder()
                .title(&format!("[{}] {}", idx, entry_title))
                .use_markup(false)
                .build();
            let select_btn = Button::builder()
                .label("Use as Default")
                .valign(Align::Center)
                .css_classes(vec!["flat"])
                .build();
            let def_entry_clone = default_entry.clone();
            let idx_str = idx.to_string();
            select_btn.connect_clicked(move |_| {
                def_entry_clone.set_text(&idx_str);
            });
            entry_row.add_suffix(&select_btn);
            entries_expander.add_row(&entry_row);
        }
        params_group.add(&entries_expander);

        // Kernel Boot Parameters row
        let cmdline_row = ActionRow::builder()
            .title("Kernel Boot Parameters")
            .subtitle("Command-line parameters passed to the Linux kernel (GRUB_CMDLINE_LINUX_DEFAULT)")
            .use_markup(false)
            .build();

        let cmdline_entry = Entry::builder()
            .text(&cfg.cmdline)
            .valign(Align::Center)
            .hexpand(true)
            .width_request(320)
            .build();
        cmdline_row.add_suffix(&cmdline_entry);
        params_group.add(&cmdline_row);

        root_box.append(&params_group);

        // ---------------------------------------------------------
        // 4. Save & Update Action Bar
        // ---------------------------------------------------------
        let btn_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .halign(Align::End)
            .spacing(12)
            .build();

        let save_content = ButtonContent::builder()
            .label("Save & Update GRUB")
            .icon_name("emblem-default-symbolic")
            .build();
        let save_btn = Button::builder()
            .child(&save_content)
            .css_classes(vec!["suggested-action"])
            .build();
        btn_box.append(&save_btn);
        root_box.append(&btn_box);

        // ---------------------------------------------------------
        // 5. Real-Time File Monitoring Controller
        // ---------------------------------------------------------
        let default_row_clone = default_path_row.clone();
        let cfg_row_clone = cfg_path_row.clone();
        let default_bak_row_clone = default_backup_row.clone();
        let cfg_bak_row_clone = cfg_backup_row.clone();
        let restore_btn_clone = restore_bak_btn.clone();
        let last_status = Rc::new(RefCell::new(cfg.live_status.clone()));

        let update_live_status = {
            let last_status = last_status.clone();
            let default_row = default_row_clone.clone();
            let cfg_row = cfg_row_clone.clone();
            let default_bak_row = default_bak_row_clone.clone();
            let cfg_bak_row = cfg_bak_row_clone.clone();
            let restore_btn = restore_btn_clone.clone();
            Rc::new(move || {
                let cur = GrubManager::get_live_status();
                if cur != *last_status.borrow() {
                    default_row.set_subtitle(&format_default_subtitle(&cur));
                    cfg_row.set_subtitle(&format_cfg_subtitle(&cur));
                    default_bak_row.set_subtitle(&format_file_backup_subtitle(
                        &cur.default_backup_path,
                        cur.default_backup_exists,
                        cur.default_backup_size_bytes,
                        &cur.default_backup_modified_time,
                    ));
                    cfg_bak_row.set_subtitle(&format_file_backup_subtitle(
                        &cur.backup_path,
                        cur.backup_exists,
                        cur.backup_size_bytes,
                        &cur.backup_modified_time,
                    ));
                    restore_btn.set_sensitive(cur.backup_exists || cur.default_backup_exists);
                    *last_status.borrow_mut() = cur;
                }
            })
        };

        // Real-time interval polling every 2 seconds
        let timer_update = update_live_status.clone();
        glib::timeout_add_local(Duration::from_secs(2), move || {
            timer_update();
            glib::ControlFlow::Continue
        });

        // Manual refresh button
        let manual_update = update_live_status.clone();
        refresh_btn.connect_clicked(move |_| {
            manual_update();
        });

        // ---------------------------------------------------------
        // 6. Connect Backup Button (Async Channel)
        // ---------------------------------------------------------
        let (tx_bak, rx_bak) = async_channel::unbounded::<Result<String, String>>();
        let btn_bak_ref = create_bak_btn.clone();
        let content_bak_ref = create_bak_content.clone();
        let update_live_bak = update_live_status.clone();
        let overlay_bak = toast_overlay_rc.clone();

        glib::MainContext::default().spawn_local(async move {
            while let Ok(res) = rx_bak.recv().await {
                btn_bak_ref.set_sensitive(true);
                content_bak_ref.set_label("Create Backups (.bak)");
                update_live_bak();

                match res {
                    Ok(msg) => {
                        NotificationManager::send_notification("💾 GRUB Backup Created", &msg);
                        if let Some(ref o) = overlay_bak.as_ref() {
                            let toast = Toast::new(&msg);
                            toast.set_timeout(3);
                            o.add_toast(toast);
                        }
                    }
                    Err(err) => {
                        NotificationManager::send_notification("⚠️ GRUB Backup Failed", &err);
                        if let Some(ref o) = overlay_bak.as_ref() {
                            let toast = Toast::new(&format!("Backup failed: {}", err));
                            toast.set_timeout(3);
                            o.add_toast(toast);
                        }
                    }
                }
            }
        });

        let btn_bak_click = create_bak_btn.clone();
        let content_bak_click = create_bak_content.clone();
        create_bak_btn.connect_clicked(move |_| {
            btn_bak_click.set_sensitive(false);
            content_bak_click.set_label("Backing up...");
            let tx = tx_bak.clone();
            std::thread::spawn(move || {
                let res = GrubManager::create_backup();
                let _ = tx.send_blocking(res);
            });
        });

        // ---------------------------------------------------------
        // 7. Connect Restore Button (Async Channel)
        // ---------------------------------------------------------
        let (tx_rst, rx_rst) = async_channel::unbounded::<Result<String, String>>();
        let btn_rst_ref = restore_bak_btn.clone();
        let content_rst_ref = restore_bak_content.clone();
        let update_live_rst = update_live_status.clone();
        let overlay_rst = toast_overlay_rc.clone();
        let spin_rst = timeout_spin.clone();
        let entry_rst = default_entry.clone();
        let cmdline_rst = cmdline_entry.clone();

        glib::MainContext::default().spawn_local(async move {
            while let Ok(res) = rx_rst.recv().await {
                btn_rst_ref.set_sensitive(true);
                content_rst_ref.set_label("Restore Configuration");
                update_live_rst();

                match res {
                    Ok(msg) => {
                        let fresh = GrubManager::read_config();
                        spin_rst.set_value(fresh.timeout as f64);
                        entry_rst.set_text(&fresh.default_entry);
                        cmdline_rst.set_text(&fresh.cmdline);

                        NotificationManager::send_notification("🔄 GRUB Restored", &msg);
                        if let Some(ref o) = overlay_rst.as_ref() {
                            let toast = Toast::new(&msg);
                            toast.set_timeout(3);
                            o.add_toast(toast);
                        }
                    }
                    Err(err) => {
                        NotificationManager::send_notification("⚠️ GRUB Restore Failed", &err);
                        if let Some(ref o) = overlay_rst.as_ref() {
                            let toast = Toast::new(&format!("Restore failed: {}", err));
                            toast.set_timeout(3);
                            o.add_toast(toast);
                        }
                    }
                }
            }
        });

        let btn_rst_click = restore_bak_btn.clone();
        let content_rst_click = restore_bak_content.clone();
        restore_bak_btn.connect_clicked(move |_| {
            btn_rst_click.set_sensitive(false);
            content_rst_click.set_label("Restoring...");
            let tx = tx_rst.clone();
            std::thread::spawn(move || {
                let res = GrubManager::restore_backup();
                let _ = tx.send_blocking(res);
            });
        });

        // ---------------------------------------------------------
        // 8. Connect Save & Update GRUB Button (Async Channel)
        // ---------------------------------------------------------
        let (tx_save, rx_save) = async_channel::unbounded::<Result<(), String>>();
        let btn_save_ref = save_btn.clone();
        let content_save_ref = save_content.clone();
        let update_live_save = update_live_status.clone();
        let overlay_save = toast_overlay_rc.clone();

        glib::MainContext::default().spawn_local(async move {
            while let Ok(res) = rx_save.recv().await {
                btn_save_ref.set_sensitive(true);
                content_save_ref.set_label("Save & Update GRUB");
                update_live_save();

                match res {
                    Ok(_) => {
                        NotificationManager::send_notification(
                            "⚙️ GRUB Updated",
                            "Bootloader configuration saved and updated successfully.",
                        );
                        if let Some(ref o) = overlay_save.as_ref() {
                            let toast = Toast::new("Bootloader configuration updated.");
                            toast.set_timeout(3);
                            o.add_toast(toast);
                        }
                    }
                    Err(err) => {
                        NotificationManager::send_notification(
                            "⚠️ GRUB Update Failed",
                            &format!("Could not update GRUB: {}", err),
                        );
                        if let Some(ref o) = overlay_save.as_ref() {
                            let toast = Toast::new(&format!("Update error: {}", err));
                            toast.set_timeout(3);
                            o.add_toast(toast);
                        }
                    }
                }
            }
        });

        let btn_save_click = save_btn.clone();
        let content_save_click = save_content.clone();
        let spin_save = timeout_spin.clone();
        let entry_save = default_entry.clone();
        let cmdline_save = cmdline_entry.clone();

        save_btn.connect_clicked(move |_| {
            btn_save_click.set_sensitive(false);
            content_save_click.set_label("Updating GRUB...");
            let timeout = spin_save.value() as i32;
            let def = entry_save.text().to_string();
            let cmd = cmdline_save.text().to_string();
            let tx = tx_save.clone();

            std::thread::spawn(move || {
                let res = GrubManager::save_and_update(timeout, &def, Some(&cmd));
                let _ = tx.send_blocking(res);
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

fn format_default_subtitle(status: &GrubLiveStatus) -> String {
    if status.default_exists {
        let size_str = if status.default_size_bytes < 1024 {
            format!("{} B", status.default_size_bytes)
        } else {
            format!("{:.1} KB", (status.default_size_bytes as f64) / 1024.0)
        };
        let dropins_info = if status.default_dropins_count > 0 {
            format!(
                " (+{} drop-in file{} in /etc/default/grub.d)",
                status.default_dropins_count,
                if status.default_dropins_count == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        };
        let strategy_info = if status.strategy_label.is_empty() {
            String::new()
        } else {
            format!(" • Strategy: {}", status.strategy_label)
        };
        format!(
            "🟢 {} ({}) • Modified: {}{}{}",
            status.default_path, size_str, status.default_modified_time, dropins_info, strategy_info
        )
    } else {
        format!("⚠️ {} (Not detected on filesystem)", status.default_path)
    }
}

fn format_cfg_subtitle(status: &GrubLiveStatus) -> String {
    if status.cfg_exists {
        let kb = (status.cfg_size_bytes as f64) / 1024.0;
        format!(
            "🟢 {} ({:.1} KB) • Modified: {}",
            status.cfg_path, kb, status.cfg_modified_time
        )
    } else {
        format!("⚠️ {} (Not detected on filesystem)", status.cfg_path)
    }
}

fn format_file_backup_subtitle(path: &str, exists: bool, size_bytes: u64, modified_time: &str) -> String {
    if exists {
        let size_str = if size_bytes < 1024 {
            format!("{} B", size_bytes)
        } else {
            format!("{:.1} KB", (size_bytes as f64) / 1024.0)
        };
        format!("✅ {} ({}) • Modified: {}", path, size_str, modified_time)
    } else {
        format!("No backup file created yet ({})", path)
    }
}
