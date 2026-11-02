use gtk4::gdk::Display;
use gtk4::CssProvider;
use libadwaita as adw;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GnomeAccent {
    pub id: &'static str,
    pub name: &'static str,
    pub hex: &'static str,
    pub dark_header: &'static str,
    pub dark_sidebar: &'static str,
}

pub static GNOME_ACCENTS: &[GnomeAccent] = &[
    GnomeAccent {
        id: "blue",
        name: "Blue",
        hex: "#2b7bdd",
        dark_header: "#174272",
        dark_sidebar: "#102a49",
    },
    GnomeAccent {
        id: "teal",
        name: "Teal",
        hex: "#2190a4",
        dark_header: "#114c57",
        dark_sidebar: "#0c3239",
    },
    GnomeAccent {
        id: "green",
        name: "Green",
        hex: "#3a944a",
        dark_header: "#1c4e25",
        dark_sidebar: "#13361a",
    },
    GnomeAccent {
        id: "yellow",
        name: "Yellow",
        hex: "#c88800",
        dark_header: "#634400",
        dark_sidebar: "#442f00",
    },
    GnomeAccent {
        id: "orange",
        name: "Orange",
        hex: "#ed5b00",
        dark_header: "#762e00",
        dark_sidebar: "#522000",
    },
    GnomeAccent {
        id: "red",
        name: "Red",
        hex: "#e62d42",
        dark_header: "#741721",
        dark_sidebar: "#511017",
    },
    GnomeAccent {
        id: "pink",
        name: "Pink",
        hex: "#d56199",
        dark_header: "#6b314d",
        dark_sidebar: "#492235",
    },
    GnomeAccent {
        id: "purple",
        name: "Purple",
        hex: "#9141ac",
        dark_header: "#492156",
        dark_sidebar: "#33173c",
    },
    GnomeAccent {
        id: "slate",
        name: "Slate",
        hex: "#637889",
        dark_header: "#323c45",
        dark_sidebar: "#22292f",
    },
];

thread_local! {
    static CSS_PROVIDER: CssProvider = CssProvider::new();
}

pub struct ThemeManager;

impl ThemeManager {
    pub fn get_accents() -> &'static [GnomeAccent] {
        GNOME_ACCENTS
    }

    pub fn get_accent_by_id(id: &str) -> GnomeAccent {
        GNOME_ACCENTS
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(id))
            .copied()
            .unwrap_or(GNOME_ACCENTS[0])
    }

    pub fn generate_css(accent: &GnomeAccent, is_dark: bool) -> String {
        if is_dark {
            format!(
                r#"
                /* Mithrandir GNOME Accent - Dark Mode ({name}) */
                @define-color accent_color {hex};
                @define-color accent_bg_color {hex};

                headerbar {{
                    background: linear-gradient(180deg, {dark_header} 0%, rgba(10, 20, 35, 0.95) 100%);
                    color: #ffffff;
                    border-bottom: 1px solid rgba(255, 255, 255, 0.12);
                    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
                }}

                headerbar windowtitle, headerbar .title, headerbar .subtitle {{
                    color: #ffffff;
                }}

                headerbar button {{
                    color: #ffffff;
                }}

                headerbar button:hover {{
                    background: rgba(255, 255, 255, 0.15);
                    color: #ffffff;
                }}

                headerbar button.sidebar-toggle-btn {{
                    min-width: 36px;
                    min-height: 36px;
                    padding: 4px;
                    border-radius: 8px;
                }}

                .sidebar {{
                    background: linear-gradient(180deg, {dark_sidebar} 0%, rgba(8, 16, 26, 0.98) 100%);
                    border-right: 1px solid rgba(255, 255, 255, 0.08);
                }}

                .navigation-sidebar {{
                    background: transparent;
                }}

                .navigation-sidebar row {{
                    color: #f1f5f9;
                    border-radius: 8px;
                    margin: 2px 6px;
                    transition: all 150ms ease;
                }}

                .navigation-sidebar row:hover {{
                    background: rgba(255, 255, 255, 0.08);
                    color: #ffffff;
                }}

                .navigation-sidebar row:selected {{
                    background: {hex};
                    color: #ffffff;
                    font-weight: 600;
                    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
                }}

                .navigation-sidebar row:selected label,
                .navigation-sidebar row:selected image {{
                    color: #ffffff;
                }}

                .smart-boost-card {{
                    background: linear-gradient(135deg, rgba(255, 255, 255, 0.06) 0%, rgba(0, 0, 0, 0.2) 100%);
                    border: 1px solid {hex};
                    border-radius: 12px;
                    padding: 16px;
                }}

                button.suggested-action {{
                    background-color: {hex};
                    color: #ffffff;
                    font-weight: bold;
                }}

                button.suggested-action:hover {{
                    filter: brightness(1.15);
                }}

                progressbar > trough > progress {{
                    background-color: {hex};
                    border-radius: 4px;
                }}

                .metric-card {{
                    background: rgba(255, 255, 255, 0.04);
                    border: 1px solid rgba(255, 255, 255, 0.1);
                    border-radius: 12px;
                    padding: 14px;
                }}
                "#,
                name = accent.name,
                hex = accent.hex,
                dark_header = accent.dark_header,
                dark_sidebar = accent.dark_sidebar,
            )
        } else {
            format!(
                r#"
                /* Mithrandir GNOME Accent - Light Mode ({name}) */
                @define-color accent_color {hex};
                @define-color accent_bg_color {hex};

                headerbar {{
                    background: linear-gradient(180deg, #f8fafc 0%, #edf2f7 100%);
                    color: #1e293b;
                    border-bottom: 1px solid rgba(0, 0, 0, 0.1);
                    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
                }}

                headerbar windowtitle, headerbar .title, headerbar .subtitle {{
                    color: #1e293b;
                }}

                headerbar button {{
                    color: #334155;
                }}

                headerbar button:hover {{
                    background: rgba(0, 0, 0, 0.06);
                    color: #0f172a;
                }}

                headerbar button.sidebar-toggle-btn {{
                    min-width: 36px;
                    min-height: 36px;
                    padding: 4px;
                    border-radius: 8px;
                }}

                .sidebar {{
                    background: #f1f5f9;
                    border-right: 1px solid rgba(0, 0, 0, 0.08);
                }}

                .navigation-sidebar {{
                    background: transparent;
                }}

                .navigation-sidebar row {{
                    color: #334155;
                    border-radius: 8px;
                    margin: 2px 6px;
                    transition: all 150ms ease;
                }}

                .navigation-sidebar row:hover {{
                    background: rgba(0, 0, 0, 0.05);
                    color: #0f172a;
                }}

                .navigation-sidebar row:selected {{
                    background: {hex};
                    color: #ffffff;
                    font-weight: 600;
                    box-shadow: 0 2px 5px rgba(0, 0, 0, 0.15);
                }}

                .navigation-sidebar row:selected label,
                .navigation-sidebar row:selected image {{
                    color: #ffffff;
                }}

                .smart-boost-card {{
                    background: linear-gradient(135deg, rgba(0, 0, 0, 0.02) 0%, rgba(0, 0, 0, 0.06) 100%);
                    border: 1px solid {hex};
                    border-radius: 12px;
                    padding: 16px;
                }}

                button.suggested-action {{
                    background-color: {hex};
                    color: #ffffff;
                    font-weight: bold;
                }}

                button.suggested-action:hover {{
                    filter: brightness(0.9);
                }}

                progressbar > trough > progress {{
                    background-color: {hex};
                    border-radius: 4px;
                }}

                .metric-card {{
                    background: rgba(0, 0, 0, 0.02);
                    border: 1px solid rgba(0, 0, 0, 0.08);
                    border-radius: 12px;
                    padding: 14px;
                }}
                "#,
                name = accent.name,
                hex = accent.hex,
            )
        }
    }

    pub fn apply_theme() {
        let cfg = crate::backend::AppSettings::get();
        Self::apply_accent(&cfg.accent_color);
    }

    pub fn apply_accent(accent_id: &str) {
        if let Some(display) = Display::default() {
            CSS_PROVIDER.with(|provider| {
                let style_manager = adw::StyleManager::default();
                let is_dark = style_manager.is_dark();

                let accent = Self::get_accent_by_id(accent_id);
                let css = Self::generate_css(&accent, is_dark);
                provider.load_from_string(&css);

                gtk4::style_context_add_provider_for_display(
                    &display,
                    provider,
                    gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );

                let provider_clone = provider.clone();
                let accent_id_owned = accent_id.to_string();
                style_manager.connect_dark_notify(move |sm| {
                    let dark = sm.is_dark();
                    let acc = Self::get_accent_by_id(&accent_id_owned);
                    provider_clone.load_from_string(&Self::generate_css(&acc, dark));
                });
            });
        }
    }
}
