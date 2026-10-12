import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, Gio
from ..backend.autostart import AutostartManager

class PreferencesDialog(Adw.PreferencesDialog):
    """
    Preferences dialog for Mithrandir.
    Configures General behavior (background mode, autostart, smart boost)
    and AI Diagnostics endpoint settings.
    """

    def __init__(self, settings: Gio.Settings):
        super().__init__()
        self.settings = settings
        self.set_title("Preferences")

        # 1. General & System Page
        gen_page = Adw.PreferencesPage()
        gen_page.set_title("General")
        gen_page.set_icon_name("emblem-system-symbolic")

        # Background & Startup Group
        bg_group = Adw.PreferencesGroup()
        bg_group.set_title("Background and System Startup")
        bg_group.set_description("Control Mithrandir lifecycle and boot behavior")

        # Keep running in background
        self.bg_row = Adw.SwitchRow()
        self.bg_row.set_title("Keep Running in Background")
        self.bg_row.set_subtitle("Closing the window minimizes Mithrandir to the background for continuous telemetry and auto-boost")
        try:
            self.bg_row.set_active(self.settings.get_boolean("keep-in-background"))
        except Exception:
            self.bg_row.set_active(True)
        self.bg_row.connect("notify::active", lambda w, _p: self.settings.set_boolean("keep-in-background", w.get_active()))
        bg_group.add(self.bg_row)

        # Autostart on boot
        self.autostart_row = Adw.SwitchRow()
        self.autostart_row.set_title("Start on System Login")
        self.autostart_row.set_subtitle("Automatically launch Mithrandir in the background on system boot")
        self.autostart_row.set_active(AutostartManager.is_mithrandir_autostart_enabled())
        self.autostart_row.connect("notify::active", lambda w, _p: AutostartManager.set_mithrandir_autostart(w.get_active()))
        bg_group.add(self.autostart_row)

        gen_page.add(bg_group)

        # Smart Boost Group
        boost_group = Adw.PreferencesGroup()
        boost_group.set_title("Smart Boost (Microsoft PC Manager Style)")
        boost_group.set_description("Configure automated performance boost and standby cache reclaiming")

        self.boost_enable_row = Adw.SwitchRow()
        self.boost_enable_row.set_title("Enable Automated Smart Boost")
        self.boost_enable_row.set_subtitle("Trims memory buffers and cache when thresholds are reached")
        try:
            self.boost_enable_row.set_active(self.settings.get_boolean("smart-boost-enabled"))
        except Exception:
            self.boost_enable_row.set_active(True)
        self.boost_enable_row.connect("notify::active", lambda w, _p: self.settings.set_boolean("smart-boost-enabled", w.get_active()))
        boost_group.add(self.boost_enable_row)

        # RAM threshold
        ram_row = Adw.SpinRow.new_with_range(50, 95, 5)
        ram_row.set_title("RAM Auto-Boost Threshold (%)")
        try:
            ram_row.set_value(self.settings.get_int("smart-boost-ram-threshold"))
        except Exception:
            ram_row.set_value(80)
        ram_row.connect("notify::value", lambda w, _p: self.settings.set_int("smart-boost-ram-threshold", int(w.get_value())))
        boost_group.add(ram_row)

        # Temp files threshold
        temp_row = Adw.SpinRow.new_with_range(250, 5000, 250)
        temp_row.set_title("Temp Files Clutter Threshold (MB)")
        try:
            temp_row.set_value(self.settings.get_int("smart-boost-temp-threshold-mb"))
        except Exception:
            temp_row.set_value(1024)
        temp_row.connect("notify::value", lambda w, _p: self.settings.set_int("smart-boost-temp-threshold-mb", int(w.get_value())))
        boost_group.add(temp_row)

        gen_page.add(boost_group)
        self.add(gen_page)

        # 2. AI Diagnostics Page
        ai_page = Adw.PreferencesPage()
        ai_page.set_title("AI Diagnostics")
        ai_page.set_icon_name("network-server-symbolic")

        ai_group = Adw.PreferencesGroup()
        ai_group.set_title("Provider Settings")
        ai_group.set_description("Configure OpenAI-compatible API or Local LLM (LM Studio, Ollama)")

        # Base URL Row
        self.url_row = Adw.EntryRow()
        self.url_row.set_title("Base URL")
        self.url_row.set_text(self.settings.get_string("ai-base-url"))
        self.url_row.connect("changed", self._on_url_changed)
        ai_group.add(self.url_row)

        # Privacy Row
        self.privacy_row = Adw.SwitchRow()
        self.privacy_row.set_title("Redact Sensitive Data (PII)")
        self.privacy_row.set_subtitle("Automatically mask IP addresses, usernames, and host paths before sending to AI")
        self.privacy_row.set_active(self.settings.get_boolean("ai-privacy-mask-pii"))
        self.privacy_row.connect("notify::active", self._on_privacy_changed)
        ai_group.add(self.privacy_row)

        ai_page.add(ai_group)
        self.add(ai_page)

    def _on_url_changed(self, entry):
        self.settings.set_string("ai-base-url", entry.get_text().strip())

    def _on_privacy_changed(self, switch, _pspec):
        self.settings.set_boolean("ai-privacy-mask-pii", switch.get_active())
