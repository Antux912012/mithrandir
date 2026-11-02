use gtk4::gdk::Display;
use gtk4::CssProvider;
use libadwaita as adw;

pub struct ThemeManager;

impl ThemeManager {
    pub const ACCENT_COLOR: &'static str = "#2b7bdd";

    pub const DARK_CSS: &'static str = r#"
    /* Mithrandir Theme - Dark Mode Shaded from #2b7bdd */
    headerbar {
        background: linear-gradient(180deg, #174272 0%, #12345a 100%);
        color: #ffffff;
        border-bottom: 1px solid rgba(43, 123, 221, 0.35);
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    }

    headerbar windowtitle, headerbar .title, headerbar .subtitle {
        color: #ffffff;
    }

    headerbar button {
        color: #ffffff;
    }

    headerbar button:hover {
        background: rgba(255, 255, 255, 0.15);
        color: #ffffff;
    }

    headerbar button.sidebar-toggle-btn {
        min-width: 36px;
        min-height: 36px;
        padding: 4px;
        border-radius: 8px;
    }

    .sidebar {
        background: linear-gradient(180deg, #102a49 0%, #0c2037 100%);
        border-right: 1px solid rgba(43, 123, 221, 0.25);
    }

    .navigation-sidebar {
        background: transparent;
    }

    .navigation-sidebar row {
        color: #dbeafe;
        border-radius: 8px;
        margin: 2px 6px;
        transition: all 150ms ease;
    }

    .navigation-sidebar row:hover {
        background: rgba(43, 123, 221, 0.25);
        color: #ffffff;
    }

    .navigation-sidebar row:selected {
        background: #2b7bdd;
        color: #ffffff;
        font-weight: 600;
        box-shadow: 0 2px 6px rgba(43, 123, 221, 0.4);
    }

    .navigation-sidebar row:selected label,
    .navigation-sidebar row:selected image {
        color: #ffffff;
    }

    .smart-boost-card {
        background: linear-gradient(135deg, rgba(43, 123, 221, 0.22) 0%, rgba(20, 60, 110, 0.28) 100%);
        border: 1px solid rgba(43, 123, 221, 0.35);
        border-radius: 12px;
        padding: 16px;
    }

    button.suggested-action {
        background-color: #2b7bdd;
        color: #ffffff;
        font-weight: bold;
    }

    button.suggested-action:hover {
        background-color: #3b8cf0;
    }

    progressbar > trough > progress {
        background-color: #2b7bdd;
        border-radius: 4px;
    }

    .metric-card {
        background: rgba(255, 255, 255, 0.04);
        border: 1px solid rgba(43, 123, 221, 0.25);
        border-radius: 12px;
        padding: 14px;
    }
    "#;

    pub const LIGHT_CSS: &'static str = r#"
    /* Mithrandir Theme - Light Mode Shaded from #2b7bdd */
    headerbar {
        background: linear-gradient(180deg, #2f84e8 0%, #2571cb 100%);
        color: #ffffff;
        border-bottom: 1px solid rgba(34, 103, 186, 0.4);
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
    }

    headerbar windowtitle, headerbar .title, headerbar .subtitle {
        color: #ffffff;
    }

    headerbar button {
        color: #ffffff;
    }

    headerbar button:hover {
        background: rgba(255, 255, 255, 0.22);
        color: #ffffff;
    }

    headerbar button.sidebar-toggle-btn {
        min-width: 36px;
        min-height: 36px;
        padding: 4px;
        border-radius: 8px;
    }

    .sidebar {
        background: linear-gradient(180deg, #edf4fc 0%, #e2eefb 100%);
        border-right: 1px solid rgba(43, 123, 221, 0.25);
    }

    .navigation-sidebar {
        background: transparent;
    }

    .navigation-sidebar row {
        color: #173e6b;
        border-radius: 8px;
        margin: 2px 6px;
        transition: all 150ms ease;
    }

    .navigation-sidebar row:hover {
        background: rgba(43, 123, 221, 0.15);
        color: #0e2a4a;
    }

    .navigation-sidebar row:selected {
        background: #2b7bdd;
        color: #ffffff;
        font-weight: 600;
        box-shadow: 0 2px 6px rgba(43, 123, 221, 0.35);
    }

    .navigation-sidebar row:selected label,
    .navigation-sidebar row:selected image {
        color: #ffffff;
    }

    .smart-boost-card {
        background: linear-gradient(135deg, rgba(43, 123, 221, 0.12) 0%, rgba(220, 235, 252, 0.8) 100%);
        border: 1px solid rgba(43, 123, 221, 0.3);
        border-radius: 12px;
        padding: 16px;
    }

    button.suggested-action {
        background-color: #2b7bdd;
        color: #ffffff;
        font-weight: bold;
    }

    button.suggested-action:hover {
        background-color: #3b8cf0;
    }

    progressbar > trough > progress {
        background-color: #2b7bdd;
        border-radius: 4px;
    }

    .metric-card {
        background: rgba(0, 0, 0, 0.02);
        border: 1px solid rgba(43, 123, 221, 0.2);
        border-radius: 12px;
        padding: 14px;
    }
    "#;

    pub fn apply_theme() {
        if let Some(display) = Display::default() {
            let provider = CssProvider::new();
            let style_manager = adw::StyleManager::default();
            let is_dark = style_manager.is_dark();
            provider.load_from_string(if is_dark { Self::DARK_CSS } else { Self::LIGHT_CSS });

            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );

            let provider_clone = provider.clone();
            style_manager.connect_dark_notify(move |sm| {
                let dark = sm.is_dark();
                provider_clone.load_from_string(if dark { Self::DARK_CSS } else { Self::LIGHT_CSS });
            });
        }
    }
}
