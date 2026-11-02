use libadwaita as adw;
use adw::prelude::*;
use adw::{PreferencesGroup, PreferencesPage, PreferencesWindow, SpinRow, SwitchRow};
use crate::backend::settings::AppSettings;

pub struct PreferencesDialog;

impl PreferencesDialog {
    pub fn build(parent: &adw::ApplicationWindow) -> PreferencesWindow {
        let win = PreferencesWindow::builder()
            .transient_for(parent)
            .modal(true)
            .title("Preferences")
            .default_width(620)
            .default_height(480)
            .build();

        let settings = AppSettings::get();
        let page = PreferencesPage::new();

        // 1. Background Service & Integration
        let bg_group = PreferencesGroup::builder()
            .title("Background Service & Integration")
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
        page.add(&bg_group);

        // 2. Smart Boost Automation
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

        let notify_row = SwitchRow::builder()
            .title("Desktop Notifications")
            .subtitle("Notify when RAM is reclaimed or cache is cleaned")
            .active(settings.desktop_notifications)
            .build();
        notify_row.connect_active_notify(|row| {
            let active = row.is_active();
            AppSettings::update(|cfg| cfg.desktop_notifications = active);
        });
        boost_group.add(&notify_row);
        page.add(&boost_group);

        win.add(&page);
        win
    }
}
