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
    static DARK_SIGNAL_CONNECTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
                /* Mithrandir GNOME / Libadwaita HIG - Dark Mode ({name}) */
                @define-color accent_color {hex};
                @define-color accent_bg_color {hex};

                .navigation-sidebar {{
                    background: transparent;
                }}

                .navigation-sidebar row {{
                    border-radius: 8px;
                    margin: 2px 8px;
                    transition: background 150ms ease;
                }}

                .navigation-sidebar row:selected {{
                    background: alpha({hex}, 0.88);
                    color: #ffffff;
                    font-weight: 600;
                }}

                .navigation-sidebar row:selected label,
                .navigation-sidebar row:selected image {{
                    color: #ffffff;
                }}

                .sidebar-section-header {{
                    font-size: 11px;
                    font-weight: 700;
                    letter-spacing: 0.04em;
                    opacity: 0.62;
                    margin: 12px 14px 4px 14px;
                }}

                .smart-boost-card {{
                    background: alpha({hex}, 0.10);
                    border: 1px solid alpha({hex}, 0.35);
                    border-radius: 12px;
                    padding: 16px;
                }}

                .metric-tile {{
                    border-radius: 12px;
                }}

                .chip-button {{
                    font-size: 12px;
                    padding: 4px 12px;
                }}

                progressbar > trough > progress {{
                    background-color: {hex};
                    border-radius: 4px;
                }}
                "#,
                name = accent.name,
                hex = accent.hex,
            )
        } else {
            format!(
                r#"
                /* Mithrandir GNOME / Libadwaita HIG - Light Mode ({name}) */
                @define-color accent_color {hex};
                @define-color accent_bg_color {hex};

                .navigation-sidebar {{
                    background: transparent;
                }}

                .navigation-sidebar row {{
                    border-radius: 8px;
                    margin: 2px 8px;
                    transition: background 150ms ease;
                }}

                .navigation-sidebar row:selected {{
                    background: {hex};
                    color: #ffffff;
                    font-weight: 600;
                }}

                .navigation-sidebar row:selected label,
                .navigation-sidebar row:selected image {{
                    color: #ffffff;
                }}

                .sidebar-section-header {{
                    font-size: 11px;
                    font-weight: 700;
                    letter-spacing: 0.04em;
                    opacity: 0.62;
                    margin: 12px 14px 4px 14px;
                }}

                .smart-boost-card {{
                    background: alpha({hex}, 0.06);
                    border: 1px solid alpha({hex}, 0.28);
                    border-radius: 12px;
                    padding: 16px;
                }}

                .metric-tile {{
                    border-radius: 12px;
                }}

                .chip-button {{
                    font-size: 12px;
                    padding: 4px 12px;
                }}

                progressbar > trough > progress {{
                    background-color: {hex};
                    border-radius: 4px;
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

                DARK_SIGNAL_CONNECTED.with(|connected| {
                    if !connected.get() {
                        connected.set(true);
                        let provider_clone = provider.clone();
                        style_manager.connect_dark_notify(move |sm| {
                            let dark = sm.is_dark();
                            let cur_cfg = crate::backend::AppSettings::get();
                            let acc = Self::get_accent_by_id(&cur_cfg.accent_color);
                            provider_clone.load_from_string(&Self::generate_css(&acc, dark));
                        });
                    }
                });
            });
        }
    }
}
