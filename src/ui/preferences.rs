use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Image, Orientation,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{ActionRow, ComboRow, PreferencesGroup, PreferencesPage, PreferencesWindow, SpinRow, SwitchRow};
use std::rc::Rc;
use crate::backend::graphics::GraphicsManager;
use crate::backend::settings::AppSettings;
use crate::ui::theme::{ThemeManager, GNOME_ACCENTS};

pub struct PreferencesDialog;

impl PreferencesDialog {
    pub fn build<F>(parent: &adw::ApplicationWindow, on_tab_visibility_changed: F) -> PreferencesWindow
    where
        F: FnMut() + 'static,
    {
        let win = PreferencesWindow::builder()
            .transient_for(parent)
            .modal(true)
            .title("Preferences")
            .default_width(680)
            .default_height(540)
            .build();

        let settings = AppSettings::get();
        let page = PreferencesPage::new();

        let on_tab_change_rc = Rc::new(std::cell::RefCell::new(on_tab_visibility_changed));

        // ---------------------------------------------------------
        // 1. Appearance & GNOME Accent Colors
        // ---------------------------------------------------------
        let appearance_group = PreferencesGroup::builder()
            .title("Appearance &amp; Accent Color")
            .description("Choose system accent color displayed like in GNOME Settings")
            .build();

        let accent_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .halign(Align::Center)
            .margin_top(8)
            .margin_bottom(8)
            .build();

        let current_accent = settings.accent_color.clone();
        let accent_btns: Rc<std::cell::RefCell<Vec<(String, Button, Image)>>> = Rc::new(std::cell::RefCell::new(Vec::new()));

        for accent in GNOME_ACCENTS {
            let is_selected = accent.id.eq_ignore_ascii_case(&current_accent);

            let check_icon = Image::from_icon_name("object-select-symbolic");
            check_icon.set_pixel_size(16);
            check_icon.set_visible(is_selected);

            let btn = Button::builder()
                .width_request(38)
                .height_request(38)
                .tooltip_text(accent.name)
                .css_classes(vec!["circular", "flat"])
                .child(&check_icon)
                .build();

            // Inject custom circular background color styling
            let css_provider = gtk4::CssProvider::new();
            let btn_css = format!(
                "button.circular {{ background-color: {}; border-radius: 9999px; min-width: 38px; min-height: 38px; padding: 0; }} button.circular:hover {{ filter: brightness(1.15); }}",
                accent.hex
            );
            css_provider.load_from_string(&btn_css);
            #[allow(deprecated)]
            btn.style_context().add_provider(&css_provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);

            let accent_id = accent.id.to_string();
            let all_btns = accent_btns.clone();

            btn.connect_clicked(move |_| {
                ThemeManager::apply_accent(&accent_id);
                AppSettings::update(|cfg| cfg.accent_color = accent_id.clone());

                for (id, _, icon) in all_btns.borrow().iter() {
                    icon.set_visible(id == &accent_id);
                }
            });

            accent_btns.borrow_mut().push((accent.id.to_string(), btn.clone(), check_icon));
            accent_box.append(&btn);
        }

        appearance_group.add(&ActionRow::builder().child(&accent_box).build());
        page.add(&appearance_group);

        // ---------------------------------------------------------
        // 2. Graphics & Hardware Acceleration
        // ---------------------------------------------------------
        let graphics_group = PreferencesGroup::builder()
            .title("Graphics &amp; Hardware Acceleration")
            .description("Vulkan and OpenGL hardware rendering acceleration for GTK4 scene graph")
            .build();

        let caps = GraphicsManager::detect_capabilities();

        let hw_accel_row = SwitchRow::builder()
            .title("Hardware Acceleration")
            .subtitle("Enable GPU hardware accelerated rendering via Vulkan or OpenGL")
            .active(settings.hardware_acceleration)
            .build();

        let renderer_model = gtk4::StringList::new(&[
            "Auto (Detect Vulkan / OpenGL)",
            "Vulkan",
            "OpenGL",
            "Software (CPU / Cairo)",
        ]);

        let initial_renderer_idx = match settings.graphics_renderer.as_str() {
            "vulkan" => 1,
            "gl" | "opengl" => 2,
            "cairo" | "software" => 3,
            _ => 0,
        };

        let renderer_row = ComboRow::builder()
            .title("Graphics Engine (GSK Renderer)")
            .subtitle("Select active rendering backend")
            .model(&renderer_model)
            .selected(initial_renderer_idx)
            .sensitive(settings.hardware_acceleration)
            .build();

        let renderer_row_clone = renderer_row.clone();
        hw_accel_row.connect_active_notify(move |row| {
            let active = row.is_active();
            renderer_row_clone.set_sensitive(active);
            AppSettings::update(|cfg| cfg.hardware_acceleration = active);
            let cur = AppSettings::get();
            GraphicsManager::apply_acceleration_settings(active, &cur.graphics_renderer);
        });
        graphics_group.add(&hw_accel_row);

        renderer_row.connect_selected_notify(move |combo| {
            let mode = match combo.selected() {
                1 => "vulkan",
                2 => "gl",
                3 => "cairo",
                _ => "auto",
            };
            AppSettings::update(|cfg| cfg.graphics_renderer = mode.to_string());
            let cur = AppSettings::get();
            GraphicsManager::apply_acceleration_settings(cur.hardware_acceleration, mode);
        });
        graphics_group.add(&renderer_row);

        let caps_row = ActionRow::builder()
            .title("Graphics Hardware Info")
            .subtitle(format!(
                "{}: Vulkan {} | OpenGL {}",
                caps.card_name,
                if caps.vulkan_available { "Available" } else { "Unavailable" },
                if caps.opengl_available { "Available" } else { "Unavailable" }
            ))
            .build();
        caps_row.add_prefix(&Image::from_icon_name("video-display-symbolic"));
        graphics_group.add(&caps_row);

        page.add(&graphics_group);

        // ---------------------------------------------------------
        // 3. Navigation Tabs Visibility
        // ---------------------------------------------------------
        let tabs_group = PreferencesGroup::builder()
            .title("Navigation Tabs Visibility")
            .description("Choose which modules are displayed or hidden in the sidebar navigation")
            .build();

        let optional_tabs = [
            ("kernel_power", "Kernel & CPU Schedulers", "speedometer-symbolic"),
            ("ai", "AI Optimizer", "starred-symbolic"),
            ("cleaner", "System Cleaner", "edit-clear-all-symbolic"),
            ("processes", "Process Manager", "view-list-bullet-symbolic"),
            ("services", "System Services", "emblem-system-symbolic"),
            ("autostart", "Startup Applications", "system-run-symbolic"),
            ("uninstaller", "Software Uninstaller", "system-software-install-symbolic"),
            ("btrfs", "BTRFS Snapshots", "drive-harddisk-symbolic"),
            ("grub", "GRUB Bootloader", "preferences-system-symbolic"),
        ];

        let disabled_tabs = settings.disabled_tabs.clone();

        for (tab_id, tab_label, tab_icon) in optional_tabs {
            let is_visible = !disabled_tabs.contains(&tab_id.to_string());

            let tab_switch = SwitchRow::builder()
                .title(tab_label)
                .subtitle(format!("Display {} module tab", tab_label))
                .active(is_visible)
                .build();
            tab_switch.add_prefix(&Image::from_icon_name(tab_icon));

            let id_str = tab_id.to_string();
            let on_change = on_tab_change_rc.clone();

            tab_switch.connect_active_notify(move |row| {
                let active = row.is_active();
                let tid = id_str.clone();

                AppSettings::update(|cfg| {
                    if active {
                        cfg.disabled_tabs.retain(|t| t != &tid);
                    } else if !cfg.disabled_tabs.contains(&tid) {
                        cfg.disabled_tabs.push(tid);
                    }
                });

                (on_change.borrow_mut())();
            });

            tabs_group.add(&tab_switch);
        }
        page.add(&tabs_group);

        // ---------------------------------------------------------
        // 4. Background Service & Desktop Integration
        // ---------------------------------------------------------
        let bg_group = PreferencesGroup::builder()
            .title("Background Service &amp; Integration")
            .description("Configure continuous system optimization and desktop integration")
            .build();

        let keep_bg_row = SwitchRow::builder()
            .title("Keep Running in Background")
            .subtitle("Hide window on close instead of exiting")
            .active(settings.keep_in_background)
            .build();
        keep_bg_row.connect_active_notify(|row| {
            let active = row.is_active();
            AppSettings::update(|cfg| cfg.keep_in_background = active);
        });
        bg_group.add(&keep_bg_row);

        let indicator_row = SwitchRow::builder()
            .title("Show Status Notifier / AppIndicator Tray")
            .subtitle("Display quick-boost indicator in status area")
            .active(settings.show_tray_icon)
            .build();
        indicator_row.connect_active_notify(|row| {
            let active = row.is_active();
            AppSettings::update(|cfg| cfg.show_tray_icon = active);
        });
        bg_group.add(&indicator_row);

        let notify_row = SwitchRow::builder()
            .title("Desktop Notifications")
            .subtitle("Notify when RAM is reclaimed or cache is cleaned")
            .active(settings.desktop_notifications)
            .build();
        notify_row.connect_active_notify(|row| {
            let active = row.is_active();
            AppSettings::update(|cfg| cfg.desktop_notifications = active);
        });
        bg_group.add(&notify_row);

        page.add(&bg_group);

        // ---------------------------------------------------------
        // 5. Smart Boost Automation
        // ---------------------------------------------------------
        let boost_group = PreferencesGroup::builder()
            .title("Smart Boost Automation")
            .description("Configure automatic standby cache trimming and RAM thresholds")
            .build();

        let auto_boost_row = SwitchRow::builder()
            .title("Automated Background Optimization")
            .subtitle("Periodically trim standby pagecache when RAM usage is high")
            .active(settings.auto_boost_enabled)
            .build();
        auto_boost_row.connect_active_notify(|row| {
            let active = row.is_active();
            AppSettings::update(|cfg| cfg.auto_boost_enabled = active);
        });
        boost_group.add(&auto_boost_row);

        let ram_thresh_row = SpinRow::builder()
            .title("RAM Threshold (%)")
            .subtitle("Trigger optimization when system memory crosses threshold")
            .adjustment(&gtk4::Adjustment::new(settings.ram_threshold, 50.0, 95.0, 1.0, 5.0, 0.0))
            .build();
        ram_thresh_row.connect_value_notify(|row| {
            let val = row.value();
            AppSettings::update(|cfg| cfg.ram_threshold = val);
        });
        boost_group.add(&ram_thresh_row);

        let cooldown_row = SpinRow::builder()
            .title("Cooldown Interval (minutes)")
            .subtitle("Minimum cooldown between automated boosts to prevent thrashing")
            .adjustment(&gtk4::Adjustment::new(settings.cooldown_minutes as f64, 1.0, 120.0, 1.0, 5.0, 0.0))
            .build();
        cooldown_row.connect_value_notify(|row| {
            let val = row.value().round() as u64;
            AppSettings::update(|cfg| cfg.cooldown_minutes = val);
        });
        boost_group.add(&cooldown_row);

        page.add(&boost_group);

        win.add(&page);
        win
    }
}
