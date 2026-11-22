use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DropDown, Entry, Image, Label, Orientation, ScrolledWindow,
    SpinButton, StringList,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{
    ActionRow, ButtonContent, Clamp, ComboRow, ExpanderRow, PreferencesGroup, Toast, ToastOverlay,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::backend::coreboot::{CorebootManager, CorebootPersonalisationConfig};
use crate::backend::grub::{
    GrubLiveStatus, GrubManager, GrubPersonalisation, GRUB_COLOR_PALETTE, GRUB_COLOR_PRESETS,
    GRUB_GFXMODE_PRESETS, GRUB_GFXPAYLOAD_OPTIONS, GRUB_TERMINAL_OUTPUTS, GRUB_TIMEOUT_STYLES,
};
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
        let cb_status = CorebootManager::detect();
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
            .sensitive(has_restorable_state(&cfg.live_status))
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

        // Timeout row (-1 = wait indefinitely, 0 = immediate, up to 3600s)
        let timeout_row = ActionRow::builder()
            .title("Boot Menu Timeout (seconds)")
            .subtitle("Seconds before booting default (-1 = wait indefinitely, 0 = immediate)")
            .use_markup(false)
            .build();

        let timeout_spin = SpinButton::with_range(-1.0, 3600.0, 1.0);
        timeout_spin.set_value(cfg.timeout.clamp(-1, 3600) as f64);
        timeout_spin.set_valign(Align::Center);
        timeout_row.add_suffix(&timeout_spin);
        params_group.add(&timeout_row);

        // Timeout Style row (menu / countdown / hidden)
        let mut timeout_style_opts: Vec<&str> = vec!["Default / Unchanged"];
        timeout_style_opts.extend_from_slice(GRUB_TIMEOUT_STYLES);
        let timeout_style_model = StringList::new(&timeout_style_opts);
        let timeout_style_row = ComboRow::builder()
            .title("Boot Menu Visibility Style")
            .subtitle("GRUB_TIMEOUT_STYLE (set 'menu' to always display the boot menu)")
            .model(&timeout_style_model)
            .use_markup(false)
            .build();
        timeout_style_row.add_prefix(&Image::from_icon_name("view-reveal-symbolic"));
        timeout_style_row.set_selected(find_opt_index(
            GRUB_TIMEOUT_STYLES,
            cfg.personalisation.timeout_style.as_deref(),
        ));
        params_group.add(&timeout_style_row);

        // Default entry row
        let default_row = ActionRow::builder()
            .title("Default Boot Entry")
            .subtitle("Index (0, 1>0 for submenu), entry name, or 'saved' to remember last choice")
            .use_markup(false)
            .build();

        let default_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(6)
            .valign(Align::Center)
            .build();
        let default_entry = Entry::builder()
            .text(&cfg.default_entry)
            .valign(Align::Center)
            .width_request(210)
            .build();
        let saved_btn = Button::builder()
            .label("saved")
            .tooltip_text("Remember last booted kernel (GRUB_DEFAULT=saved)")
            .css_classes(vec!["flat"])
            .valign(Align::Center)
            .build();
        {
            let def_clone = default_entry.clone();
            saved_btn.connect_clicked(move |_| {
                def_clone.set_text("saved");
            });
        }
        default_box.append(&default_entry);
        default_box.append(&saved_btn);
        default_row.add_suffix(&default_box);
        params_group.add(&default_row);

        // Detected Boot Menu Entries Expander (with exact submenu-aware grub_id!)
        let entries_subtitle = if cfg.structured_entries.is_empty() {
            "No boot menu entries detected in grub.cfg, BLS entries, or /boot".to_string()
        } else {
            format!(
                "{} boot entry/kernel image(s) detected — expand to select exact GRUB ID",
                cfg.structured_entries.len()
            )
        };
        let entries_expander = ExpanderRow::builder()
            .title("Detected Boot Menu Entries")
            .subtitle(&entries_subtitle)
            .use_markup(false)
            .enable_expansion(!cfg.structured_entries.is_empty())
            .build();
        entries_expander.add_prefix(&Image::from_icon_name("view-list-symbolic"));

        for item in &cfg.structured_entries {
            let display_title = if let Some(ref sub) = item.submenu_title {
                format!("[{}] {} › {}", item.grub_id, sub, item.title)
            } else {
                format!("[{}] {}", item.grub_id, item.title)
            };
            let entry_row = ActionRow::builder()
                .title(&display_title)
                .use_markup(false)
                .build();
            let select_btn = Button::builder()
                .label("Use as Default")
                .valign(Align::Center)
                .css_classes(vec!["flat"])
                .build();
            let def_entry_clone = default_entry.clone();
            let grub_id = item.grub_id.clone();
            select_btn.connect_clicked(move |_| {
                def_entry_clone.set_text(&grub_id);
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
            .width_request(300)
            .build();
        cmdline_row.add_suffix(&cmdline_entry);
        params_group.add(&cmdline_row);

        root_box.append(&params_group);

        // ---------------------------------------------------------
        // 4. GRUB Personalisation (Theming, Colors, Fonts, Resolution)
        // ---------------------------------------------------------
        let pers_group = PreferencesGroup::builder()
            .title("GRUB Visual Personalisation")
            .description("Customize GRUB2 themes, background wallpaper, menu colors, .pf2 fonts, and framebuffer resolution")
            .build();

        // 4a. Theme Selector + Custom Path
        let mut theme_labels: Vec<String> = vec!["None (Classic Menu)".to_string()];
        let mut theme_paths: Vec<String> = vec![String::new()];
        for th in &cfg.available_themes {
            theme_labels.push(format!("{} ({})", th.name, th.path));
            theme_paths.push(th.path.clone());
        }
        let theme_label_refs: Vec<&str> = theme_labels.iter().map(|s| s.as_str()).collect();
        let theme_model = StringList::new(&theme_label_refs);
        let theme_combo = ComboRow::builder()
            .title("Installed GRUB Theme")
            .subtitle("Discovered themes in /boot/grub/themes and /usr/share/grub/themes")
            .model(&theme_model)
            .use_markup(false)
            .build();
        theme_combo.add_prefix(&Image::from_icon_name("preferences-desktop-theme-symbolic"));

        let theme_path_entry = Entry::builder()
            .text(cfg.personalisation.theme.as_deref().unwrap_or(""))
            .placeholder_text("/boot/grub/themes/<name>/theme.txt")
            .valign(Align::Center)
            .width_request(280)
            .build();

        if let Some(ref active_theme) = cfg.personalisation.theme {
            if let Some(pos) = theme_paths.iter().position(|p| p == active_theme) {
                theme_combo.set_selected(pos as u32);
            }
        }

        let theme_paths_rc = Rc::new(theme_paths);
        {
            let entry_ref = theme_path_entry.clone();
            let paths_ref = theme_paths_rc.clone();
            theme_combo.connect_selected_notify(move |combo| {
                let idx = combo.selected() as usize;
                if let Some(p) = paths_ref.get(idx) {
                    entry_ref.set_text(p);
                }
            });
        }
        pers_group.add(&theme_combo);

        let theme_custom_row = ActionRow::builder()
            .title("GRUB Theme File (GRUB_THEME)")
            .subtitle("Path to active theme.txt (leave blank to disable theme)")
            .use_markup(false)
            .build();
        theme_custom_row.add_suffix(&theme_path_entry);
        pers_group.add(&theme_custom_row);

        // 4b. Background Image
        let bg_entry = Entry::builder()
            .text(cfg.personalisation.background.as_deref().unwrap_or(""))
            .placeholder_text("/boot/grub/background.png (PNG, JPG, TGA)")
            .valign(Align::Center)
            .width_request(280)
            .build();
        let bg_row = ActionRow::builder()
            .title("Background Wallpaper (GRUB_BACKGROUND)")
            .subtitle("Fallback image used when no theme.txt is active (black bg = transparent)")
            .use_markup(false)
            .build();
        bg_row.add_prefix(&Image::from_icon_name("image-x-generic-symbolic"));
        bg_row.add_suffix(&bg_entry);
        pers_group.add(&bg_row);

        // 4c. Color Palette Presets & Foreground/Background Pickers
        let mut color_preset_names: Vec<&str> = GRUB_COLOR_PRESETS.iter().map(|p| p.name).collect();
        color_preset_names.push("Custom");
        let color_preset_model = StringList::new(&color_preset_names);
        let color_preset_row = ComboRow::builder()
            .title("Color Scheme Preset")
            .subtitle("Quickly apply a curated GRUB2 16-color palette")
            .model(&color_preset_model)
            .use_markup(false)
            .build();
        color_preset_row.add_prefix(&Image::from_icon_name("preferences-color-symbolic"));

        let mut palette_items: Vec<&str> = vec!["default"];
        palette_items.extend_from_slice(GRUB_COLOR_PALETTE);

        let norm_fg_dd = DropDown::new(Some(StringList::new(&palette_items)), gtk4::Expression::NONE);
        let norm_bg_dd = DropDown::new(Some(StringList::new(&palette_items)), gtk4::Expression::NONE);
        norm_fg_dd.set_valign(Align::Center);
        norm_bg_dd.set_valign(Align::Center);

        let hi_fg_dd = DropDown::new(Some(StringList::new(&palette_items)), gtk4::Expression::NONE);
        let hi_bg_dd = DropDown::new(Some(StringList::new(&palette_items)), gtk4::Expression::NONE);
        hi_fg_dd.set_valign(Align::Center);
        hi_bg_dd.set_valign(Align::Center);

        apply_color_pair_to_dropdowns(
            cfg.personalisation.color_normal.as_deref(),
            &norm_fg_dd,
            &norm_bg_dd,
        );
        apply_color_pair_to_dropdowns(
            cfg.personalisation.color_highlight.as_deref(),
            &hi_fg_dd,
            &hi_bg_dd,
        );

        let initial_preset_idx = detect_color_preset_index(
            cfg.personalisation.color_normal.as_deref(),
            cfg.personalisation.color_highlight.as_deref(),
        );
        color_preset_row.set_selected(initial_preset_idx);

        let syncing_colors = Rc::new(Cell::new(false));
        {
            let sync_flag = syncing_colors.clone();
            let nfg = norm_fg_dd.clone();
            let nbg = norm_bg_dd.clone();
            let hfg = hi_fg_dd.clone();
            let hbg = hi_bg_dd.clone();
            color_preset_row.connect_selected_notify(move |combo| {
                if sync_flag.get() {
                    return;
                }
                let idx = combo.selected() as usize;
                if let Some(preset) = GRUB_COLOR_PRESETS.get(idx) {
                    sync_flag.set(true);
                    let n_opt = if preset.normal.is_empty() { None } else { Some(preset.normal) };
                    let h_opt = if preset.highlight.is_empty() { None } else { Some(preset.highlight) };
                    apply_color_pair_to_dropdowns(n_opt, &nfg, &nbg);
                    apply_color_pair_to_dropdowns(h_opt, &hfg, &hbg);
                    sync_flag.set(false);
                }
            });
        }
        pers_group.add(&color_preset_row);

        let norm_color_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(6)
            .valign(Align::Center)
            .build();
        norm_color_box.append(&Label::new(Some("FG:")));
        norm_color_box.append(&norm_fg_dd);
        norm_color_box.append(&Label::new(Some("BG:")));
        norm_color_box.append(&norm_bg_dd);

        let norm_color_row = ActionRow::builder()
            .title("Normal Menu Colors (GRUB_COLOR_NORMAL)")
            .subtitle("Foreground / background for unselected menu entries")
            .use_markup(false)
            .build();
        norm_color_row.add_suffix(&norm_color_box);
        pers_group.add(&norm_color_row);

        let hi_color_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(6)
            .valign(Align::Center)
            .build();
        hi_color_box.append(&Label::new(Some("FG:")));
        hi_color_box.append(&hi_fg_dd);
        hi_color_box.append(&Label::new(Some("BG:")));
        hi_color_box.append(&hi_bg_dd);

        let hi_color_row = ActionRow::builder()
            .title("Highlight Colors (GRUB_COLOR_HIGHLIGHT)")
            .subtitle("Foreground / background for the selected menu entry")
            .use_markup(false)
            .build();
        hi_color_row.add_suffix(&hi_color_box);
        pers_group.add(&hi_color_row);

        // 4d. Font Selector + TTF/OTF -> .pf2 Converter
        let mut font_labels: Vec<String> = vec!["System Default".to_string()];
        let mut font_paths: Vec<String> = vec![String::new()];
        for f in &cfg.available_fonts {
            font_labels.push(format!("{} ({})", f.name, f.path));
            font_paths.push(f.path.clone());
        }
        let font_label_refs: Vec<&str> = font_labels.iter().map(|s| s.as_str()).collect();
        let font_model = StringList::new(&font_label_refs);
        let font_combo = ComboRow::builder()
            .title("Installed GRUB Font (.pf2)")
            .subtitle("Bitmap fonts discovered in /boot/grub/fonts and /usr/share/grub")
            .model(&font_model)
            .use_markup(false)
            .build();
        font_combo.add_prefix(&Image::from_icon_name("font-x-generic-symbolic"));

        let font_path_entry = Entry::builder()
            .text(cfg.personalisation.font.as_deref().unwrap_or(""))
            .placeholder_text("/boot/grub/fonts/unicode.pf2")
            .valign(Align::Center)
            .width_request(280)
            .build();

        if let Some(ref active_font) = cfg.personalisation.font {
            if let Some(pos) = font_paths.iter().position(|p| p == active_font) {
                font_combo.set_selected(pos as u32);
            }
        }

        let font_paths_rc = Rc::new(font_paths);
        {
            let entry_ref = font_path_entry.clone();
            let paths_ref = font_paths_rc.clone();
            font_combo.connect_selected_notify(move |combo| {
                let idx = combo.selected() as usize;
                if let Some(p) = paths_ref.get(idx) {
                    entry_ref.set_text(p);
                }
            });
        }
        pers_group.add(&font_combo);

        let font_custom_row = ActionRow::builder()
            .title("GRUB Font Path (GRUB_FONT)")
            .subtitle("Path to .pf2 font file (requires gfxterm output)")
            .use_markup(false)
            .build();
        font_custom_row.add_suffix(&font_path_entry);
        pers_group.add(&font_custom_row);

        // TTF/OTF -> PF2 Converter Expander
        let mkfont_expander = ExpanderRow::builder()
            .title("Convert System TTF/OTF Font to GRUB .pf2")
            .subtitle("Use grub-mkfont to generate a HiDPI-friendly .pf2 font in /boot/grub/fonts")
            .use_markup(false)
            .build();
        mkfont_expander.add_prefix(&Image::from_icon_name("preferences-desktop-font-symbolic"));

        let src_font_entry = Entry::builder()
            .placeholder_text("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")
            .valign(Align::Center)
            .width_request(300)
            .build();
        let src_font_row = ActionRow::builder()
            .title("Source TTF/OTF Path")
            .use_markup(false)
            .build();
        src_font_row.add_suffix(&src_font_entry);
        mkfont_expander.add_row(&src_font_row);

        let font_size_spin = SpinButton::with_range(8.0, 64.0, 2.0);
        font_size_spin.set_value(20.0);
        font_size_spin.set_valign(Align::Center);
        let font_size_row = ActionRow::builder()
            .title("Font Size (px)")
            .subtitle("Use 18–28 px for 1080p/HiDPI screens")
            .use_markup(false)
            .build();
        font_size_row.add_suffix(&font_size_spin);
        mkfont_expander.add_row(&font_size_row);

        let out_font_name_entry = Entry::builder()
            .text("mithrandir-custom")
            .valign(Align::Center)
            .width_request(180)
            .build();
        let gen_font_btn = Button::builder()
            .label("Generate .pf2")
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .build();
        let out_font_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .valign(Align::Center)
            .build();
        out_font_box.append(&out_font_name_entry);
        out_font_box.append(&gen_font_btn);

        let out_font_row = ActionRow::builder()
            .title("Output Font Name")
            .subtitle("Saved as /boot/grub/fonts/<name>-<size>.pf2 and auto-filled above")
            .use_markup(false)
            .build();
        out_font_row.add_suffix(&out_font_box);
        mkfont_expander.add_row(&out_font_row);
        pers_group.add(&mkfont_expander);

        // Async handler for grub-mkfont conversion
        {
            let (tx_font, rx_font) = async_channel::unbounded::<Result<String, String>>();
            let btn_ref = gen_font_btn.clone();
            let font_target_entry = font_path_entry.clone();
            let ov_font = toast_overlay_rc.clone();

            glib::MainContext::default().spawn_local(async move {
                while let Ok(res) = rx_font.recv().await {
                    btn_ref.set_sensitive(true);
                    btn_ref.set_label("Generate .pf2");
                    match res {
                        Ok(out_path) => {
                            font_target_entry.set_text(&out_path);
                            if let Some(ref o) = ov_font.as_ref() {
                                let toast = Toast::new(&format!("Generated GRUB font: {}", out_path));
                                toast.set_timeout(4);
                                o.add_toast(toast);
                            }
                        }
                        Err(err) => {
                            if let Some(ref o) = ov_font.as_ref() {
                                let toast = Toast::new(&format!("Font conversion failed: {}", err));
                                toast.set_timeout(4);
                                o.add_toast(toast);
                            }
                        }
                    }
                }
            });

            let btn_click = gen_font_btn.clone();
            let src_ref = src_font_entry.clone();
            let size_ref = font_size_spin.clone();
            let name_ref = out_font_name_entry.clone();
            gen_font_btn.connect_clicked(move |_| {
                btn_click.set_sensitive(false);
                btn_click.set_label("Generating...");
                let src = src_ref.text().to_string();
                let sz = size_ref.value() as u32;
                let name = name_ref.text().to_string();
                let tx = tx_font.clone();
                std::thread::spawn(move || {
                    let res = GrubManager::convert_font_to_pf2(&src, sz, &name);
                    let _ = tx.send_blocking(res);
                });
            });
        }

        // 4e. Resolution & Graphics Output (GRUB_GFXMODE, GRUB_GFXPAYLOAD_LINUX, GRUB_TERMINAL_OUTPUT)
        let mut gfxmode_opts: Vec<&str> = vec!["Default / Unchanged"];
        gfxmode_opts.extend_from_slice(GRUB_GFXMODE_PRESETS);
        let gfxmode_model = StringList::new(&gfxmode_opts);
        let gfxmode_row = ComboRow::builder()
            .title("Boot Menu Resolution (GRUB_GFXMODE)")
            .subtitle("Framebuffer resolution used by the GRUB graphical menu")
            .model(&gfxmode_model)
            .use_markup(false)
            .build();
        gfxmode_row.add_prefix(&Image::from_icon_name("video-display-symbolic"));
        gfxmode_row.set_selected(find_opt_index(
            GRUB_GFXMODE_PRESETS,
            cfg.personalisation.gfxmode.as_deref(),
        ));
        pers_group.add(&gfxmode_row);

        let mut gfxpayload_opts: Vec<&str> = vec!["Default / Unchanged"];
        gfxpayload_opts.extend_from_slice(GRUB_GFXPAYLOAD_OPTIONS);
        let gfxpayload_model = StringList::new(&gfxpayload_opts);
        let gfxpayload_row = ComboRow::builder()
            .title("Kernel Graphics Handoff (GRUB_GFXPAYLOAD_LINUX)")
            .subtitle("Set 'keep' for seamless transition into Plymouth boot splash")
            .model(&gfxpayload_model)
            .use_markup(false)
            .build();
        gfxpayload_row.add_prefix(&Image::from_icon_name("preferences-desktop-display-symbolic"));
        gfxpayload_row.set_selected(find_opt_index(
            GRUB_GFXPAYLOAD_OPTIONS,
            cfg.personalisation.gfxpayload_linux.as_deref(),
        ));
        pers_group.add(&gfxpayload_row);

        let mut term_opts: Vec<&str> = vec!["Default / Unchanged"];
        term_opts.extend_from_slice(GRUB_TERMINAL_OUTPUTS);
        let term_model = StringList::new(&term_opts);
        let term_row = ComboRow::builder()
            .title("Terminal Output Mode (GRUB_TERMINAL_OUTPUT)")
            .subtitle("Use 'gfxterm' to enable themes, fonts, and custom resolutions")
            .model(&term_model)
            .use_markup(false)
            .build();
        term_row.add_prefix(&Image::from_icon_name("utilities-terminal-symbolic"));
        term_row.set_selected(find_opt_index(
            GRUB_TERMINAL_OUTPUTS,
            cfg.personalisation.terminal_output.as_deref(),
        ));
        pers_group.add(&term_row);

        root_box.append(&pers_group);

        // ---------------------------------------------------------
        // 5. Save & Update Action Bar
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
        // 6. Coreboot Firmware & Payload Personalisation Group
        // ---------------------------------------------------------
        let cb_group = PreferencesGroup::builder()
            .title("Coreboot Firmware &amp; Payload Personalisation")
            .description("Detect Coreboot firmware, manage NVRAM/CMOS options, and stage bootsplash or GRUB2 payload themes")
            .build();

        let cb_tools_list: Vec<&str> = [
            cb_status.has_nvramtool.then_some("nvramtool"),
            cb_status.has_cbfstool.then_some("cbfstool"),
            cb_status.has_cbmem.then_some("cbmem"),
            cb_status.has_flashrom.then_some("flashrom"),
        ]
        .into_iter()
        .flatten()
        .collect();
        let tools_str = if cb_tools_list.is_empty() {
            "No Coreboot CLI tools installed (nvramtool / cbfstool / cbmem)".to_string()
        } else {
            format!("Tools: {}", cb_tools_list.join(", "))
        };
        let cb_status_subtitle = format!(
            "{} • {} {} ({}) • Payload: {} • {}",
            if cb_status.detected { "🟢 Coreboot Active" } else { "⚪ Standard Firmware" },
            cb_status.bios_vendor,
            cb_status.bios_version,
            cb_status.bios_date,
            cb_status.payload_hint,
            tools_str
        );
        let cb_status_row = ActionRow::builder()
            .title(&cb_status.variant)
            .subtitle(&cb_status_subtitle)
            .use_markup(false)
            .build();
        cb_status_row.add_prefix(&Image::from_icon_name("application-x-firmware-symbolic"));
        cb_group.add(&cb_status_row);

        // Coreboot live CMOS options (only when Coreboot + nvramtool return entries)
        if cb_status.detected && !cb_status.cmos_options.is_empty() {
            let cmos_expander = ExpanderRow::builder()
                .title("Coreboot CMOS / NVRAM Options (nvramtool)")
                .subtitle(&format!(
                    "{} live firmware CMOS option(s) detected — edit and apply via nvramtool",
                    cb_status.cmos_options.len()
                ))
                .use_markup(false)
                .build();
            cmos_expander.add_prefix(&Image::from_icon_name("preferences-system-symbolic"));

            for opt in &cb_status.cmos_options {
                let row = ActionRow::builder()
                    .title(&opt.key)
                    .subtitle(&format!("Current value: {}", opt.value))
                    .use_markup(false)
                    .build();
                let val_entry = Entry::builder()
                    .text(&opt.value)
                    .width_request(130)
                    .valign(Align::Center)
                    .build();
                let apply_btn = Button::builder()
                    .label("Apply")
                    .css_classes(vec!["flat"])
                    .valign(Align::Center)
                    .build();

                let key_clone = opt.key.clone();
                let entry_clone = val_entry.clone();
                let ov_cmos = toast_overlay_rc.clone();
                let row_weak = row.downgrade();

                apply_btn.connect_clicked(move |btn| {
                    let new_val = entry_clone.text().to_string();
                    let new_val_ui = new_val.clone();
                    let k = key_clone.clone();
                    let ov = ov_cmos.clone();
                    let rw = row_weak.clone();
                    btn.set_sensitive(false);
                    let btn_weak = btn.downgrade();
                    let (tx, rx) = async_channel::bounded::<Result<String, String>>(1);

                    glib::MainContext::default().spawn_local(async move {
                        if let Ok(res) = rx.recv().await {
                            if let Some(b) = btn_weak.upgrade() {
                                b.set_sensitive(true);
                            }
                            match res {
                                Ok(msg) => {
                                    if let Some(r) = rw.upgrade() {
                                        r.set_subtitle(&format!("Current value: {}", new_val_ui));
                                    }
                                    if let Some(ref o) = ov.as_ref() {
                                        let t = Toast::new(&msg);
                                        t.set_timeout(3);
                                        o.add_toast(t);
                                    }
                                }
                                Err(err) => {
                                    if let Some(ref o) = ov.as_ref() {
                                        let t = Toast::new(&format!("CMOS update failed: {}", err));
                                        t.set_timeout(4);
                                        o.add_toast(t);
                                    }
                                }
                            }
                        }
                    });

                    std::thread::spawn(move || {
                        let res = CorebootManager::set_cmos_option(&k, &new_val);
                        let _ = tx.send_blocking(res);
                    });
                });

                let ctrl_box = GtkBox::builder()
                    .orientation(Orientation::Horizontal)
                    .spacing(6)
                    .valign(Align::Center)
                    .build();
                ctrl_box.append(&val_entry);
                ctrl_box.append(&apply_btn);
                row.add_suffix(&ctrl_box);
                cmos_expander.add_row(&row);
            }
            cb_group.add(&cmos_expander);
        }

        // Coreboot Bootsplash & GRUB2 Payload Generator Expander
        let cb_pers_expander = ExpanderRow::builder()
            .title("Coreboot Bootsplash & GRUB2 Payload Personalisation")
            .subtitle("Stage baseline 4:2:0 JPEG bootsplash CBFS commands or generate Coreboot etc/grub.cfg theming")
            .use_markup(false)
            .build();
        cb_pers_expander.add_prefix(&Image::from_icon_name("preferences-desktop-appearance-symbolic"));

        let cb_splash_entry = Entry::builder()
            .placeholder_text("/path/to/bootsplash.png (converted to baseline JPEG 4:2:0)")
            .valign(Align::Center)
            .width_request(280)
            .build();
        let cb_splash_row = ActionRow::builder()
            .title("Bootsplash Image Source")
            .subtitle("Image used for Coreboot CBFS bootsplash.jpg or GRUB2 payload background")
            .use_markup(false)
            .build();
        cb_splash_row.add_suffix(&cb_splash_entry);
        cb_pers_expander.add_row(&cb_splash_row);

        let cb_rom_entry = Entry::builder()
            .text("coreboot-working.rom")
            .valign(Align::Center)
            .width_request(220)
            .build();
        let cb_rom_row = ActionRow::builder()
            .title("Target Coreboot ROM Filename")
            .subtitle("Safety: commands always read a flashrom backup first and never auto-flash SPI")
            .use_markup(false)
            .build();
        cb_rom_row.add_suffix(&cb_rom_entry);
        cb_pers_expander.add_row(&cb_rom_row);

        let cb_gen_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .valign(Align::Center)
            .build();
        let copy_cbfs_btn = Button::builder()
            .label("Copy CBFS Bootsplash Commands")
            .valign(Align::Center)
            .build();
        let copy_cb_grub_btn = Button::builder()
            .label("Copy Coreboot grub.cfg Snippet")
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .build();
        cb_gen_box.append(&copy_cbfs_btn);
        cb_gen_box.append(&copy_cb_grub_btn);

        let cb_actions_row = ActionRow::builder()
            .title("Generate Coreboot Personalisation Artifacts")
            .subtitle("Synced with your selected colors, font, resolution, and timeout above")
            .use_markup(false)
            .build();
        cb_actions_row.add_suffix(&cb_gen_box);
        cb_pers_expander.add_row(&cb_actions_row);

        {
            let splash_ref = cb_splash_entry.clone();
            let rom_ref = cb_rom_entry.clone();
            let gfx_ref = gfxmode_row.clone();
            let ov_cb = toast_overlay_rc.clone();
            copy_cbfs_btn.connect_clicked(move |btn| {
                let res = selected_opt_value(GRUB_GFXMODE_PRESETS, gfx_ref.selected())
                    .unwrap_or_else(|| "1920x1080".to_string());
                let cmds = CorebootManager::generate_cbfs_bootsplash_commands(
                    &rom_ref.text(),
                    &splash_ref.text(),
                    &res,
                );
                btn.display().clipboard().set_text(&cmds.join("\n"));
                if let Some(ref o) = ov_cb.as_ref() {
                    let t = Toast::new("Coreboot CBFS bootsplash staging commands copied to clipboard");
                    t.set_timeout(3);
                    o.add_toast(t);
                }
            });
        }

        {
            let splash_ref = cb_splash_entry.clone();
            let gfx_ref = gfxmode_row.clone();
            let nfg = norm_fg_dd.clone();
            let nbg = norm_bg_dd.clone();
            let hfg = hi_fg_dd.clone();
            let hbg = hi_bg_dd.clone();
            let font_ref = font_path_entry.clone();
            let spin_ref = timeout_spin.clone();
            let ov_cb = toast_overlay_rc.clone();

            copy_cb_grub_btn.connect_clicked(move |btn| {
                let splash_txt = splash_ref.text().trim().to_string();
                let font_txt = font_ref.text().trim().to_string();
                let cb_cfg = CorebootPersonalisationConfig {
                    bootsplash_image: if splash_txt.is_empty() { None } else { Some(splash_txt) },
                    resolution: selected_opt_value(GRUB_GFXMODE_PRESETS, gfx_ref.selected())
                        .unwrap_or_else(|| "1920x1080".to_string()),
                    color_normal: read_color_pair_from_dropdowns(&nfg, &nbg)
                        .unwrap_or_else(|| "light-gray/black".to_string()),
                    color_highlight: read_color_pair_from_dropdowns(&hfg, &hbg)
                        .unwrap_or_else(|| "white/blue".to_string()),
                    font_path: if font_txt.is_empty() { None } else { Some(font_txt) },
                    timeout: spin_ref.value() as i32,
                };
                let snippet = CorebootManager::generate_coreboot_grub_payload_cfg(&cb_cfg);
                btn.display().clipboard().set_text(&snippet);
                if let Some(ref o) = ov_cb.as_ref() {
                    let t = Toast::new("Coreboot GRUB2 payload configuration snippet copied to clipboard");
                    t.set_timeout(3);
                    o.add_toast(t);
                }
            });
        }

        cb_group.add(&cb_pers_expander);
        root_box.append(&cb_group);

        // ---------------------------------------------------------
        // 7. Real-Time File Monitoring Controller (Weak-Ref, Cached)
        // ---------------------------------------------------------
        let last_status = Rc::new(RefCell::new(cfg.live_status.clone()));

        let update_live_status = {
            let last_status = last_status.clone();
            let default_row = default_path_row.downgrade();
            let cfg_row = cfg_path_row.downgrade();
            let default_bak_row = default_backup_row.downgrade();
            let cfg_bak_row = cfg_backup_row.downgrade();
            let restore_btn = restore_bak_btn.downgrade();
            Rc::new(move || -> bool {
                let (
                    Some(default_row),
                    Some(cfg_row),
                    Some(default_bak_row),
                    Some(cfg_bak_row),
                    Some(restore_btn),
                ) = (
                    default_row.upgrade(),
                    cfg_row.upgrade(),
                    default_bak_row.upgrade(),
                    cfg_bak_row.upgrade(),
                    restore_btn.upgrade(),
                )
                else {
                    return false;
                };

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
                    restore_btn.set_sensitive(has_restorable_state(&cur));
                    *last_status.borrow_mut() = cur;
                }
                true
            })
        };

        // Periodic polling every 3 seconds (fast fs::metadata only, zero subprocesses)
        let timer_update = update_live_status.clone();
        glib::timeout_add_local(Duration::from_secs(3), move || {
            if timer_update() {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });

        // Manual refresh button
        let manual_update = update_live_status.clone();
        refresh_btn.connect_clicked(move |_| {
            let _ = manual_update();
        });

        // ---------------------------------------------------------
        // 8. Connect Backup Button (Async Channel)
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
                let _ = update_live_bak();

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
                            toast.set_timeout(4);
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
        // 9. Connect Restore Button (Async Channel)
        // ---------------------------------------------------------
        let (tx_rst, rx_rst) = async_channel::unbounded::<Result<String, String>>();
        let btn_rst_ref = restore_bak_btn.clone();
        let content_rst_ref = restore_bak_content.clone();
        let update_live_rst = update_live_status.clone();
        let overlay_rst = toast_overlay_rc.clone();
        let spin_rst = timeout_spin.clone();
        let entry_rst = default_entry.clone();
        let cmdline_rst = cmdline_entry.clone();
        let theme_rst = theme_path_entry.clone();
        let bg_rst = bg_entry.clone();
        let font_rst = font_path_entry.clone();
        let gfx_rst = gfxmode_row.clone();
        let pay_rst = gfxpayload_row.clone();
        let term_rst = term_row.clone();
        let style_rst = timeout_style_row.clone();
        let nfg_rst = norm_fg_dd.clone();
        let nbg_rst = norm_bg_dd.clone();
        let hfg_rst = hi_fg_dd.clone();
        let hbg_rst = hi_bg_dd.clone();

        glib::MainContext::default().spawn_local(async move {
            while let Ok(res) = rx_rst.recv().await {
                content_rst_ref.set_label("Restore Configuration");
                let _ = update_live_rst();
                let fresh_status = GrubManager::get_live_status();
                btn_rst_ref.set_sensitive(has_restorable_state(&fresh_status));

                match res {
                    Ok(msg) => {
                        let fresh = GrubManager::read_config();
                        spin_rst.set_value(fresh.timeout.clamp(-1, 3600) as f64);
                        entry_rst.set_text(&fresh.default_entry);
                        cmdline_rst.set_text(&fresh.cmdline);
                        theme_rst.set_text(fresh.personalisation.theme.as_deref().unwrap_or(""));
                        bg_rst.set_text(fresh.personalisation.background.as_deref().unwrap_or(""));
                        font_rst.set_text(fresh.personalisation.font.as_deref().unwrap_or(""));
                        gfx_rst.set_selected(find_opt_index(
                            GRUB_GFXMODE_PRESETS,
                            fresh.personalisation.gfxmode.as_deref(),
                        ));
                        pay_rst.set_selected(find_opt_index(
                            GRUB_GFXPAYLOAD_OPTIONS,
                            fresh.personalisation.gfxpayload_linux.as_deref(),
                        ));
                        term_rst.set_selected(find_opt_index(
                            GRUB_TERMINAL_OUTPUTS,
                            fresh.personalisation.terminal_output.as_deref(),
                        ));
                        style_rst.set_selected(find_opt_index(
                            GRUB_TIMEOUT_STYLES,
                            fresh.personalisation.timeout_style.as_deref(),
                        ));
                        apply_color_pair_to_dropdowns(
                            fresh.personalisation.color_normal.as_deref(),
                            &nfg_rst,
                            &nbg_rst,
                        );
                        apply_color_pair_to_dropdowns(
                            fresh.personalisation.color_highlight.as_deref(),
                            &hfg_rst,
                            &hbg_rst,
                        );

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
                            toast.set_timeout(4);
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
        // 10. Connect Save & Update GRUB Button (Async Channel)
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
                let _ = update_live_save();

                match res {
                    Ok(_) => {
                        NotificationManager::send_notification(
                            "⚙️ GRUB Updated",
                            "Bootloader parameters and personalisation saved and updated successfully.",
                        );
                        if let Some(ref o) = overlay_save.as_ref() {
                            let toast = Toast::new("Bootloader configuration and personalisation updated.");
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
                            toast.set_timeout(5);
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
        let theme_save = theme_path_entry.clone();
        let bg_save = bg_entry.clone();
        let font_save = font_path_entry.clone();
        let gfx_save = gfxmode_row.clone();
        let pay_save = gfxpayload_row.clone();
        let term_save = term_row.clone();
        let style_save = timeout_style_row.clone();
        let nfg_save = norm_fg_dd.clone();
        let nbg_save = norm_bg_dd.clone();
        let hfg_save = hi_fg_dd.clone();
        let hbg_save = hi_bg_dd.clone();

        save_btn.connect_clicked(move |_| {
            btn_save_click.set_sensitive(false);
            content_save_click.set_label("Updating GRUB...");
            let timeout = spin_save.value() as i32;
            let def = entry_save.text().to_string();
            let cmd = cmdline_save.text().to_string();

            let non_empty = |s: String| {
                let t = s.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            };

            let pers = GrubPersonalisation {
                theme: non_empty(theme_save.text().to_string()),
                background: non_empty(bg_save.text().to_string()),
                color_normal: read_color_pair_from_dropdowns(&nfg_save, &nbg_save),
                color_highlight: read_color_pair_from_dropdowns(&hfg_save, &hbg_save),
                font: non_empty(font_save.text().to_string()),
                gfxmode: selected_opt_value(GRUB_GFXMODE_PRESETS, gfx_save.selected()),
                gfxpayload_linux: selected_opt_value(GRUB_GFXPAYLOAD_OPTIONS, pay_save.selected()),
                terminal_output: selected_opt_value(GRUB_TERMINAL_OUTPUTS, term_save.selected()),
                timeout_style: selected_opt_value(GRUB_TIMEOUT_STYLES, style_save.selected()),
            };

            let tx = tx_save.clone();
            std::thread::spawn(move || {
                let res = GrubManager::save_and_update_with_personalisation(
                    timeout,
                    &def,
                    Some(&cmd),
                    Some(&pers),
                );
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

fn has_restorable_state(status: &GrubLiveStatus) -> bool {
    status.backup_exists || status.default_backup_exists || status.mithrandir_dropin_exists
}

fn find_opt_index(options: &[&str], current: Option<&str>) -> u32 {
    if let Some(val) = current {
        if let Some(pos) = options.iter().position(|o| *o == val.trim()) {
            return (pos + 1) as u32;
        }
    }
    0
}

fn selected_opt_value(options: &[&str], selected: u32) -> Option<String> {
    if selected == 0 {
        None
    } else {
        options.get((selected - 1) as usize).map(|s| (*s).to_string())
    }
}

fn apply_color_pair_to_dropdowns(pair: Option<&str>, fg_dd: &DropDown, bg_dd: &DropDown) {
    if let Some(p) = pair {
        if let Some((fg, bg)) = p.trim().split_once('/') {
            let fg_idx = GRUB_COLOR_PALETTE
                .iter()
                .position(|c| *c == fg)
                .map(|i| (i + 1) as u32)
                .unwrap_or(0);
            let bg_idx = GRUB_COLOR_PALETTE
                .iter()
                .position(|c| *c == bg)
                .map(|i| (i + 1) as u32)
                .unwrap_or(0);
            fg_dd.set_selected(fg_idx);
            bg_dd.set_selected(bg_idx);
            return;
        }
    }
    fg_dd.set_selected(0);
    bg_dd.set_selected(0);
}

fn read_color_pair_from_dropdowns(fg_dd: &DropDown, bg_dd: &DropDown) -> Option<String> {
    let fg_i = fg_dd.selected();
    let bg_i = bg_dd.selected();
    if fg_i == 0 || bg_i == 0 {
        return None;
    }
    let fg = GRUB_COLOR_PALETTE.get((fg_i - 1) as usize)?;
    let bg = GRUB_COLOR_PALETTE.get((bg_i - 1) as usize)?;
    Some(format!("{}/{}", fg, bg))
}

fn detect_color_preset_index(normal: Option<&str>, highlight: Option<&str>) -> u32 {
    let n = normal.unwrap_or("").trim();
    let h = highlight.unwrap_or("").trim();
    for (idx, preset) in GRUB_COLOR_PRESETS.iter().enumerate() {
        if preset.normal == n && preset.highlight == h {
            return idx as u32;
        }
    }
    GRUB_COLOR_PRESETS.len() as u32
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

fn format_file_backup_subtitle(
    path: &str,
    exists: bool,
    size_bytes: u64,
    modified_time: &str,
) -> String {
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
