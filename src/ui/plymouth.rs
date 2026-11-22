use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, FileDialog, FileFilter, Image, Orientation, Picture,
    ScrolledWindow, SpinButton, TextView,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{
    ActionRow, Banner, ButtonContent, Clamp, ExpanderRow, PreferencesGroup, Toast, ToastOverlay,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::backend::indicator::NotificationManager;
use crate::backend::plymouth::{InitramfsBackend, PlymouthManager, PlymouthStatus};

pub struct PlymouthPage;

impl PlymouthPage {
    pub fn build(toast_overlay: Option<ToastOverlay>) -> ScrolledWindow {
        let scrolled = ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();

        let outer_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .build();

        let status = Rc::new(RefCell::new(PlymouthManager::detect()));
        let toast_overlay_rc = Rc::new(toast_overlay);

        // Top Readiness Banner (shown when GRUB flags or hooks need attention)
        let banner = Banner::new("");
        banner.set_button_label(Some("Enable quiet splash in GRUB"));
        update_readiness_banner(&banner, &status.borrow());
        outer_box.append(&banner);

        let root_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();

        // ---------------------------------------------------------
        // 1. Active Splash & Daemon Settings Group
        // ---------------------------------------------------------
        let daemon_group = PreferencesGroup::builder()
            .title("Plymouth Boot Splash &amp; Daemon Settings")
            .description("Manage active boot splash theme, daemon timing, and initramfs pipeline")
            .build();

        let active_row = ActionRow::builder()
            .title("Active Plymouth Theme")
            .subtitle(&format_active_subtitle(&status.borrow()))
            .use_markup(false)
            .build();
        active_row.add_prefix(&Image::from_icon_name("preferences-desktop-wallpaper-symbolic"));

        let rescan_btn = Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Rescan Plymouth Themes & Readiness")
            .css_classes(vec!["flat"])
            .valign(Align::Center)
            .build();
        active_row.add_suffix(&rescan_btn);
        daemon_group.add(&active_row);

        // ShowDelay SpinButton
        let show_delay_spin = SpinButton::with_range(0.0, 30.0, 1.0);
        show_delay_spin.set_value(status.borrow().config.show_delay_secs as f64);
        show_delay_spin.set_valign(Align::Center);
        let delay_row = ActionRow::builder()
            .title("Splash Show Delay (seconds)")
            .subtitle("Seconds to wait before displaying Plymouth (0 = immediate)")
            .use_markup(false)
            .build();
        delay_row.add_suffix(&show_delay_spin);
        daemon_group.add(&delay_row);

        // DeviceTimeout SpinButton
        let dev_timeout_spin = SpinButton::with_range(5.0, 60.0, 1.0);
        dev_timeout_spin.set_value(status.borrow().config.device_timeout_secs as f64);
        dev_timeout_spin.set_valign(Align::Center);
        let dev_timeout_row = ActionRow::builder()
            .title("Device Timeout (seconds)")
            .subtitle("Maximum time to wait for graphics/storage devices before fallback")
            .use_markup(false)
            .build();
        dev_timeout_row.add_suffix(&dev_timeout_spin);
        daemon_group.add(&dev_timeout_row);

        // Readiness details + Revert button
        let readiness_row = ActionRow::builder()
            .title("Boot Pipeline Readiness")
            .subtitle(&format_readiness_subtitle(&status.borrow()))
            .use_markup(false)
            .build();
        readiness_row.add_prefix(&Image::from_icon_name("emblem-default-symbolic"));

        let revert_content = ButtonContent::builder()
            .label("Revert Backup")
            .icon_name("edit-undo-symbolic")
            .build();
        let revert_btn = Button::builder()
            .child(&revert_content)
            .css_classes(vec!["destructive-action"])
            .sensitive(
                status.borrow().config.backup_exists
                    && status.borrow().initramfs_backend != InitramfsBackend::ImmutableReadOnly,
            )
            .valign(Align::Center)
            .build();
        readiness_row.add_suffix(&revert_btn);
        daemon_group.add(&readiness_row);

        root_box.append(&daemon_group);

        // ---------------------------------------------------------
        // 2. Live Initramfs Rebuild Console (Streaming Output)
        // ---------------------------------------------------------
        let console_group = PreferencesGroup::builder()
            .title("Initramfs Rebuild Console")
            .description("Real-time output from update-initramfs, dracut, or mkinitcpio")
            .build();

        let console_expander = ExpanderRow::builder()
            .title("Build Log Output")
            .subtitle("Idle — apply or revert a theme to stream initramfs rebuild progress")
            .use_markup(false)
            .build();
        console_expander.add_prefix(&Image::from_icon_name("utilities-terminal-symbolic"));

        let log_view = TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .monospace(true)
            .wrap_mode(gtk4::WrapMode::WordChar)
            .top_margin(10)
            .bottom_margin(10)
            .left_margin(12)
            .right_margin(12)
            .build();
        let log_scroll = ScrolledWindow::builder()
            .min_content_height(180)
            .max_content_height(280)
            .child(&log_view)
            .build();
        let log_row = gtk4::ListBoxRow::builder()
            .activatable(false)
            .selectable(false)
            .child(&log_scroll)
            .build();
        console_expander.add_row(&log_row);
        console_group.add(&console_expander);
        root_box.append(&console_group);

        // ---------------------------------------------------------
        // 3. Custom Theme Import Group
        // ---------------------------------------------------------
        let import_group = PreferencesGroup::builder()
            .title("Custom Theme Import")
            .description("Install a third-party Plymouth theme from .tar.gz, .tar.xz, or .zip")
            .build();

        let import_btn = Button::builder()
            .label("Import Archive...")
            .valign(Align::Center)
            .sensitive(status.borrow().initramfs_backend != InitramfsBackend::ImmutableReadOnly)
            .build();
        let import_row = ActionRow::builder()
            .title("Import Plymouth Theme Archive")
            .subtitle("Validates .plymouth manifest, rejects symlinks/traversal, and installs to /usr/share/plymouth/themes")
            .use_markup(false)
            .build();
        import_row.add_prefix(&Image::from_icon_name("folder-open-symbolic"));
        import_row.add_suffix(&import_btn);
        import_group.add(&import_row);
        root_box.append(&import_group);

        // ---------------------------------------------------------
        // 4. Installed Plymouth Themes Group
        // ---------------------------------------------------------
        let themes_group = PreferencesGroup::builder()
            .title("Installed Plymouth Themes")
            .description("Preview or apply a boot splash theme and regenerate initramfs")
            .build();
        root_box.append(&themes_group);

        let theme_rows_ref: Rc<RefCell<Vec<ActionRow>>> = Rc::new(RefCell::new(Vec::new()));
        let busy_flag = Rc::new(RefCell::new(false));

        // Shared log channel for streaming initramfs lines
        enum StreamMsg {
            Line(String),
            Done(Result<String, String>),
        }
        let (tx_stream, rx_stream) = async_channel::unbounded::<StreamMsg>();

        // Rebuild theme list helper
        let rebuild_themes_ui: Rc<dyn Fn()> = {
            let status_rc = status.clone();
            let themes_grp = themes_group.clone();
            let rows_vec = theme_rows_ref.clone();
            let active_r = active_row.clone();
            let ready_r = readiness_row.clone();
            let rev_b = revert_btn.clone();
            let bnr = banner.clone();
            let delay_s = show_delay_spin.clone();
            let to_s = dev_timeout_spin.clone();
            let tx_s = tx_stream.clone();
            let ov_rc = toast_overlay_rc.clone();
            let log_v = log_view.clone();
            let exp = console_expander.clone();
            let busy_r = busy_flag.clone();

            Rc::new(move || {
                let st = PlymouthManager::detect();
                *status_rc.borrow_mut() = st.clone();

                active_r.set_subtitle(&format_active_subtitle(&st));
                ready_r.set_subtitle(&format_readiness_subtitle(&st));
                rev_b.set_sensitive(
                    st.config.backup_exists
                        && st.initramfs_backend != InitramfsBackend::ImmutableReadOnly
                        && !*busy_r.borrow(),
                );
                update_readiness_banner(&bnr, &st);

                for old in rows_vec.borrow_mut().drain(..) {
                    themes_grp.remove(&old);
                }

                if st.themes.is_empty() {
                    let empty = ActionRow::builder()
                        .title("No Plymouth Themes Found")
                        .subtitle("Install plymouth-themes or import a custom theme archive above")
                        .use_markup(false)
                        .build();
                    empty.add_prefix(&Image::from_icon_name("dialog-information-symbolic"));
                    themes_grp.add(&empty);
                    rows_vec.borrow_mut().push(empty);
                    return;
                }

                let immutable = st.initramfs_backend == InitramfsBackend::ImmutableReadOnly;

                for theme in &st.themes {
                    let title = if theme.is_active {
                        format!("{}  [Active]", theme.name)
                    } else {
                        theme.name.clone()
                    };
                    let desc = if theme.description.is_empty() {
                        format!("ID: {} • Module: {}", theme.id, theme.module_name)
                    } else {
                        format!("{} ({} • {})", theme.description, theme.id, theme.module_name)
                    };

                    let row = ActionRow::builder()
                        .title(&title)
                        .subtitle(&desc)
                        .use_markup(false)
                        .build();

                    if let Some(ref img_path) = theme.preview_image {
                        let pic = Picture::for_filename(img_path);
                        pic.set_size_request(44, 32);
                        pic.set_can_shrink(true);
                        pic.set_valign(Align::Center);
                        row.add_prefix(&pic);
                    } else {
                        let icon = Image::from_icon_name("preferences-desktop-wallpaper-symbolic");
                        icon.set_pixel_size(24);
                        row.add_prefix(&icon);
                    }

                    let actions = GtkBox::builder()
                        .orientation(Orientation::Horizontal)
                        .spacing(8)
                        .valign(Align::Center)
                        .build();

                    let preview_btn = Button::builder()
                        .label("Preview (6s)")
                        .css_classes(vec!["flat"])
                        .sensitive(st.preview_supported && !*busy_r.borrow())
                        .valign(Align::Center)
                        .build();

                    let apply_btn = Button::builder()
                        .label(if theme.is_active { "Reapply" } else { "Apply" })
                        .css_classes(if theme.is_active {
                            vec!["flat"]
                        } else {
                            vec!["suggested-action"]
                        })
                        .sensitive(!immutable && !*busy_r.borrow())
                        .valign(Align::Center)
                        .build();

                    // Preview handler
                    {
                        let tid = theme.id.clone();
                        let ov_p = ov_rc.clone();
                        preview_btn.connect_clicked(move |btn| {
                            btn.set_sensitive(false);
                            btn.set_label("Previewing...");
                            let tid_inner = tid.clone();
                            let ov_inner = ov_p.clone();
                            let bw = btn.downgrade();
                            let (tx_p, rx_p) = async_channel::bounded::<Result<(), String>>(1);

                            glib::MainContext::default().spawn_local(async move {
                                if let Ok(res) = rx_p.recv().await {
                                    if let Some(b) = bw.upgrade() {
                                        b.set_sensitive(true);
                                        b.set_label("Preview (6s)");
                                    }
                                    if let Err(err) = res {
                                        if let Some(ref o) = ov_inner.as_ref() {
                                            let t = Toast::new(&format!("Preview failed: {}", err));
                                            t.set_timeout(4);
                                            o.add_toast(t);
                                        }
                                    }
                                }
                            });

                            std::thread::spawn(move || {
                                let res = PlymouthManager::preview_theme(&tid_inner, 6);
                                let _ = tx_p.send_blocking(res);
                            });
                        });
                    }

                    // Apply & Rebuild Initramfs handler
                    {
                        let tid = theme.id.clone();
                        let ds = delay_s.clone();
                        let ts = to_s.clone();
                        let tx = tx_s.clone();
                        let lv = log_v.clone();
                        let ex = exp.clone();
                        let bf = busy_r.clone();
                        let rb = rev_b.clone();

                        apply_btn.connect_clicked(move |btn| {
                            if *bf.borrow() {
                                return;
                            }
                            *bf.borrow_mut() = true;
                            btn.set_sensitive(false);
                            rb.set_sensitive(false);
                            lv.buffer().set_text("");
                            ex.set_expanded(true);
                            ex.set_subtitle(&format!(
                                "Applying '{}' and rebuilding initramfs...",
                                tid
                            ));

                            let delay_val = ds.value() as u32;
                            let to_val = ts.value() as u32;
                            let tid_thread = tid.clone();
                            let tx_thread = tx.clone();

                            std::thread::spawn(move || {
                                let tx_line = tx_thread.clone();
                                let res = PlymouthManager::apply_theme_streaming(
                                    &tid_thread,
                                    delay_val,
                                    to_val,
                                    |line| {
                                        let _ = tx_line.send_blocking(StreamMsg::Line(line));
                                    },
                                );
                                let done = res.map(|_| {
                                    format!("Plymouth theme '{}' applied and initramfs rebuilt.", tid_thread)
                                });
                                let _ = tx_thread.send_blocking(StreamMsg::Done(done));
                            });
                        });
                    }

                    actions.append(&preview_btn);
                    actions.append(&apply_btn);
                    row.add_suffix(&actions);
                    themes_grp.add(&row);
                    rows_vec.borrow_mut().push(row);
                }
            })
        };

        // Initial populate
        rebuild_themes_ui();

        // Stream receiver loop
        {
            let lv = log_view.clone();
            let ex = console_expander.clone();
            let bf = busy_flag.clone();
            let ov = toast_overlay_rc.clone();
            let rebuild = rebuild_themes_ui.clone();

            glib::MainContext::default().spawn_local(async move {
                while let Ok(msg) = rx_stream.recv().await {
                    match msg {
                        StreamMsg::Line(line) => {
                            let buf = lv.buffer();
                            let mut end = buf.end_iter();
                            buf.insert(&mut end, &format!("{}\n", line));
                        }
                        StreamMsg::Done(res) => {
                            *bf.borrow_mut() = false;
                            match res {
                                Ok(summary) => {
                                    ex.set_subtitle(&format!("✅ {}", summary));
                                    NotificationManager::send_notification("🎨 Plymouth Updated", &summary);
                                    if let Some(ref o) = ov.as_ref() {
                                        let t = Toast::new(&summary);
                                        t.set_timeout(4);
                                        o.add_toast(t);
                                    }
                                }
                                Err(err) => {
                                    ex.set_subtitle(&format!("⚠️ Failed: {}", err));
                                    NotificationManager::send_notification("⚠️ Plymouth Error", &err);
                                    if let Some(ref o) = ov.as_ref() {
                                        let t = Toast::new(&format!("Plymouth error: {}", err));
                                        t.set_timeout(5);
                                        o.add_toast(t);
                                    }
                                }
                            }
                            rebuild();
                        }
                    }
                }
            });
        }

        // Rescan button
        {
            let rebuild = rebuild_themes_ui.clone();
            let ov = toast_overlay_rc.clone();
            rescan_btn.connect_clicked(move |_| {
                rebuild();
                if let Some(ref o) = ov.as_ref() {
                    let t = Toast::new("Plymouth themes and readiness re-scanned");
                    t.set_timeout(2);
                    o.add_toast(t);
                }
            });
        }

        // Banner fix button ("Enable quiet splash in GRUB")
        {
            let rebuild = rebuild_themes_ui.clone();
            let ov = toast_overlay_rc.clone();
            let bnr = banner.clone();
            banner.connect_button_clicked(move |_| {
                bnr.set_revealed(false);
                let ov_i = ov.clone();
                let reb_i = rebuild.clone();
                let (tx_g, rx_g) = async_channel::bounded::<Result<(), String>>(1);

                glib::MainContext::default().spawn_local(async move {
                    if let Ok(res) = rx_g.recv().await {
                        match res {
                            Ok(_) => {
                                if let Some(ref o) = ov_i.as_ref() {
                                    let t = Toast::new("Enabled 'quiet splash' & gfxpayload=keep in GRUB");
                                    t.set_timeout(4);
                                    o.add_toast(t);
                                }
                            }
                            Err(err) => {
                                if let Some(ref o) = ov_i.as_ref() {
                                    let t = Toast::new(&format!("GRUB update failed: {}", err));
                                    t.set_timeout(4);
                                    o.add_toast(t);
                                }
                            }
                        }
                        reb_i();
                    }
                });

                std::thread::spawn(move || {
                    let _ = tx_g.send_blocking(PlymouthManager::enable_grub_splash_flags());
                });
            });
        }

        // Revert backup button
        {
            let tx = tx_stream.clone();
            let lv = log_view.clone();
            let ex = console_expander.clone();
            let bf = busy_flag.clone();
            revert_btn.connect_clicked(move |btn| {
                if *bf.borrow() {
                    return;
                }
                *bf.borrow_mut() = true;
                btn.set_sensitive(false);
                lv.buffer().set_text("");
                ex.set_expanded(true);
                ex.set_subtitle("Restoring previous Plymouth configuration and rebuilding initramfs...");

                let tx_t = tx.clone();
                std::thread::spawn(move || {
                    let tx_l = tx_t.clone();
                    let res = PlymouthManager::revert_to_backup_streaming(|line| {
                        let _ = tx_l.send_blocking(StreamMsg::Line(line));
                    });
                    let _ = tx_t.send_blocking(StreamMsg::Done(res));
                });
            });
        }

        // Import Archive button (GTK4 FileDialog)
        {
            let rebuild = rebuild_themes_ui.clone();
            let ov = toast_overlay_rc.clone();
            import_btn.connect_clicked(move |btn| {
                let filter = FileFilter::new();
                filter.set_name(Some("Plymouth Theme Archives (*.tar.gz, *.tar.xz, *.zip)"));
                filter.add_pattern("*.tar.gz");
                filter.add_pattern("*.tgz");
                filter.add_pattern("*.tar.xz");
                filter.add_pattern("*.tar.bz2");
                filter.add_pattern("*.zip");

                let filters = gtk4::gio::ListStore::new::<FileFilter>();
                filters.append(&filter);

                let dialog = FileDialog::builder()
                    .title("Import Plymouth Theme Archive")
                    .filters(&filters)
                    .build();

                let parent_win = btn.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
                let ov_c = ov.clone();
                let reb_c = rebuild.clone();

                dialog.open(parent_win.as_ref(), gtk4::gio::Cancellable::NONE, move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            let ov_t = ov_c.clone();
                            let reb_t = reb_c.clone();
                            let (tx_i, rx_i) = async_channel::bounded::<Result<String, String>>(1);

                            glib::MainContext::default().spawn_local(async move {
                                if let Ok(r) = rx_i.recv().await {
                                    match r {
                                        Ok(theme_id) => {
                                            if let Some(ref o) = ov_t.as_ref() {
                                                let t = Toast::new(&format!(
                                                    "Imported Plymouth theme '{}'",
                                                    theme_id
                                                ));
                                                t.set_timeout(4);
                                                o.add_toast(t);
                                            }
                                            reb_t();
                                        }
                                        Err(err) => {
                                            if let Some(ref o) = ov_t.as_ref() {
                                                let t = Toast::new(&format!("Import failed: {}", err));
                                                t.set_timeout(5);
                                                o.add_toast(t);
                                            }
                                        }
                                    }
                                }
                            });

                            std::thread::spawn(move || {
                                let _ = tx_i.send_blocking(PlymouthManager::import_theme_archive(&path));
                            });
                        }
                    }
                });
            });
        }

        let clamp = Clamp::builder()
            .maximum_size(880)
            .child(&root_box)
            .build();
        outer_box.append(&clamp);
        scrolled.set_child(Some(&outer_box));

        scrolled
    }
}

fn update_readiness_banner(banner: &Banner, st: &PlymouthStatus) {
    if st.initramfs_backend == InitramfsBackend::ImmutableReadOnly {
        banner.set_title("Immutable OS detected — configure Plymouth declaratively (e.g. rpm-ostree initramfs or NixOS boot.plymouth.theme).");
        banner.set_button_label(None);
        banner.set_revealed(true);
    } else if !st.readiness.grub_has_quiet || !st.readiness.grub_has_splash || !st.readiness.grub_gfxpayload_keep {
        banner.set_title("GRUB kernel arguments are missing 'quiet splash' or 'gfxpayload=keep' for seamless boot splash.");
        banner.set_button_label(Some("Enable quiet splash in GRUB"));
        banner.set_revealed(true);
    } else if !st.readiness.warnings.is_empty() {
        banner.set_title(&st.readiness.warnings[0]);
        banner.set_button_label(None);
        banner.set_revealed(true);
    } else {
        banner.set_revealed(false);
    }
}

fn format_active_subtitle(st: &PlymouthStatus) -> String {
    format!(
        "Theme: {} • Initramfs: {} • Config: {}",
        st.config.active_theme,
        st.initramfs_backend.label(),
        st.config.conf_path
    )
}

fn format_readiness_subtitle(st: &PlymouthStatus) -> String {
    let mut parts = Vec::new();
    parts.push(if st.readiness.plymouth_installed {
        "Plymouth: Installed"
    } else {
        "Plymouth: Missing"
    });
    parts.push(if st.readiness.grub_has_quiet && st.readiness.grub_has_splash {
        "Kernel: quiet splash OK"
    } else {
        "Kernel: quiet/splash missing"
    });
    parts.push(if st.readiness.grub_gfxpayload_keep {
        "Handoff: gfxpayload=keep"
    } else {
        "Handoff: default"
    });
    if let Some(hook) = st.readiness.mkinitcpio_has_plymouth_hook {
        parts.push(if hook {
            "mkinitcpio hook: OK"
        } else {
            "mkinitcpio hook: Missing"
        });
    }
    parts.join(" • ")
}
