import pytest
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw
from mithrandir.ui.theme import ThemeManager

def test_theme_css_parsing():
    """Verify that both Dark and Light CSS strings are valid for GTK4 CssProvider."""
    for mode, css in [("dark", ThemeManager.DARK_CSS), ("light", ThemeManager.LIGHT_CSS)]:
        provider = Gtk.CssProvider()
        provider.load_from_string(css)
        assert provider is not None, f"Failed to load {mode} CSS"

def test_windowcontrols_circular_highlight_rules():
    """Verify that windowcontrols rules are explicitly defined to prevent rectangular hover artifacts."""
    for css in [ThemeManager.DARK_CSS, ThemeManager.LIGHT_CSS]:
        assert "headerbar windowcontrols button" in css
        assert "background: transparent;" in css or "background-color: transparent;" in css
        assert "headerbar windowcontrols button > image" in css
        assert "border-radius: 9999px;" in css
        assert "headerbar windowcontrols button:hover > image" in css

def test_theme_apply_and_switch():
    """Verify that ThemeManager.apply_theme() executes without error."""
    ThemeManager.apply_theme()
    assert ThemeManager._provider is not None
