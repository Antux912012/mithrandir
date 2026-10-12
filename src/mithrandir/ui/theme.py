import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, Gdk

class ThemeManager:
    """
    Manages custom color theming for Mithrandir.
    Applies custom branding based on #2b7bdd shaded for light and dark system themes.
    Dynamically listens to Adw.StyleManager 'notify::dark' events.
    """

    DARK_CSS = """
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
    """

    LIGHT_CSS = """
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
    """

    _provider = None

    @classmethod
    def apply_theme(cls):
        style_manager = Adw.StyleManager.get_default()
        if cls._provider is None:
            cls._provider = Gtk.CssProvider()
            display = Gdk.Display.get_default()
            if display:
                Gtk.StyleContext.add_provider_for_display(
                    display,
                    cls._provider,
                    Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
                )
            style_manager.connect('notify::dark', cls._on_dark_changed)

        cls._update_css(style_manager.get_dark())

    @classmethod
    def _on_dark_changed(cls, style_manager, _pspec):
        cls._update_css(style_manager.get_dark())

    @classmethod
    def _update_css(cls, is_dark: bool):
        css = cls.DARK_CSS if is_dark else cls.LIGHT_CSS
        if cls._provider:
            cls._provider.load_from_string(css)
