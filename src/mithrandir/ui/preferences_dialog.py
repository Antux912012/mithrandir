import os
import json
import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, Gio, GLib
from ..backend.autostart import AutostartManager
from ..ai.client import AIClient
from ..ai.local_llm import LocalLLMManager


class AISettingsStore:
    """
    Safe wrapper around Gio.Settings for AI configuration keys.
    Uses GSettings natively and falls back to ~/.config/io.github.antonio.Mithrandir/ai.json
    if a newly added schema key (such as ai-api-key) is not yet compiled in the system schema dir.
    """

    FALLBACK_PATH = os.path.expanduser("~/.config/io.github.antonio.Mithrandir/ai.json")

    @classmethod
    def _has_schema_key(cls, settings: Gio.Settings, key: str) -> bool:
        if not settings:
            return False
        try:
            schema = settings.props.settings_schema
            return bool(schema and schema.has_key(key))
        except Exception:
            return False

    @classmethod
    def _read_fallback(cls) -> dict:
        try:
            if os.path.isfile(cls.FALLBACK_PATH):
                with open(cls.FALLBACK_PATH, "r", encoding="utf-8") as f:
                    return json.load(f)
        except Exception:
            pass
        return {}

    @classmethod
    def _write_fallback(cls, key: str, value: str):
        try:
            os.makedirs(os.path.dirname(cls.FALLBACK_PATH), mode=0o700, exist_ok=True)
            data = cls._read_fallback()
            data[key] = value
            with open(cls.FALLBACK_PATH, "w", encoding="utf-8") as f:
                json.dump(data, f, indent=2)
        except Exception:
            pass

    @classmethod
    def get_str(cls, settings: Gio.Settings, key: str, default: str = "") -> str:
        if cls._has_schema_key(settings, key):
            try:
                val = settings.get_string(key)
                if val:
                    return val
            except Exception:
                pass
        fb = cls._read_fallback()
        return fb.get(key, default)

    @classmethod
    def set_str(cls, settings: Gio.Settings, key: str, value: str):
        cls._write_fallback(key, value)
        if cls._has_schema_key(settings, key):
            try:
                settings.set_string(key, value)
            except Exception:
                pass


class PreferencesDialog(Adw.PreferencesDialog):
    """
    Preferences dialog for Mithrandir.
    Configures General behavior (background mode, autostart, smart boost)
    and AI Diagnostics endpoint, API key, and model selection settings.
    """

    PROVIDER_KEYS = ["builtin", "llamacpp", "ollama", "lmstudio", "nim", "openai"]

    def __init__(self, settings: Gio.Settings):
        super().__init__()
        self.settings = settings
        self.set_title("Preferences")
        self._updating_ui = False
        self._available_models: list[str] = []

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

        # Show GNOME Panel Applet
        self.indicator_row = Adw.SwitchRow()
        self.indicator_row.set_title("Show GNOME Panel Applet")
        self.indicator_row.set_subtitle("Displays Mithrandir icon in the top panel with one-click Smart Boost and quick navigation")
        try:
            self.indicator_row.set_active(self.settings.get_boolean("show-panel-indicator"))
        except Exception:
            self.indicator_row.set_active(True)
        self.indicator_row.connect("notify::active", lambda w, _p: self.settings.set_boolean("show-panel-indicator", w.get_active()))
        bg_group.add(self.indicator_row)

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
        ai_group.set_title("Provider &amp; Model Settings")
        ai_group.set_description("Configure OpenAI-compatible services (NVIDIA NIM, OpenAI) or Offline Local LLMs (Ollama, LM Studio, Built-in)")

        # Provider Preset ComboRow
        provider_labels = [LocalLLMManager.PROVIDER_PRESETS[k]["label"] for k in self.PROVIDER_KEYS]
        self.provider_combo = Adw.ComboRow(title="AI Provider Preset")
        self.provider_combo.set_model(Gtk.StringList.new(provider_labels))
        current_provider = AISettingsStore.get_str(self.settings, "ai-provider", "builtin")
        if current_provider in self.PROVIDER_KEYS:
            self.provider_combo.set_selected(self.PROVIDER_KEYS.index(current_provider))
        else:
            self.provider_combo.set_selected(0)
        self.provider_combo.connect("notify::selected", self._on_provider_changed)
        ai_group.add(self.provider_combo)

        # Base URL Row
        self.url_row = Adw.EntryRow()
        self.url_row.set_title("API Base URL")
        self.url_row.set_text(AISettingsStore.get_str(self.settings, "ai-base-url", "builtin://linux-mini"))
        self.url_row.connect("changed", self._on_url_changed)
        ai_group.add(self.url_row)

        # API Key Row (for NVIDIA NIM, OpenAI, OpenRouter, etc.)
        self.key_row = Adw.PasswordEntryRow()
        self.key_row.set_title("API Key (NVIDIA NIM / OpenAI / Optional for Local)")
        self.key_row.set_text(AISettingsStore.get_str(self.settings, "ai-api-key", ""))
        self.key_row.connect("changed", self._on_key_changed)
        ai_group.add(self.key_row)

        # Model Selection Dropdown Row + Fetch Button
        self.model_combo = Adw.ComboRow(title="Available Models")
        self.model_combo.set_subtitle("Click 'Fetch Models' to load models from endpoint")
        self.fetch_models_btn = Gtk.Button(label="Fetch Models")
        self.fetch_models_btn.set_valign(Gtk.Align.CENTER)
        self.fetch_models_btn.add_css_class("suggested-action")
        self.fetch_models_btn.connect("clicked", self._on_fetch_models_clicked)
        self.model_combo.add_suffix(self.fetch_models_btn)
        self.model_combo.connect("notify::selected", self._on_model_combo_selected)
        ai_group.add(self.model_combo)

        # Active / Custom Model Entry Row
        self.model_entry_row = Adw.EntryRow()
        self.model_entry_row.set_title("Selected Model ID (or enter custom model)")
        saved_model = AISettingsStore.get_str(self.settings, "ai-model", "")
        self.model_entry_row.set_text(saved_model)
        self.model_entry_row.connect("changed", self._on_model_entry_changed)
        ai_group.add(self.model_entry_row)

        # Populate initial model list
        self._populate_initial_models(saved_model)

        # Privacy Row
        self.privacy_row = Adw.SwitchRow()
        self.privacy_row.set_title("Redact Sensitive Data (PII)")
        self.privacy_row.set_subtitle("Automatically mask IP addresses, usernames, and host paths before sending to AI")
        self.privacy_row.set_active(self.settings.get_boolean("ai-privacy-mask-pii"))
        self.privacy_row.connect("notify::active", self._on_privacy_changed)
        ai_group.add(self.privacy_row)

        ai_page.add(ai_group)
        self.add(ai_page)

    def _populate_initial_models(self, saved_model: str):
        base_url = self.url_row.get_text().strip()
        models = []
        if base_url.startswith("builtin://"):
            models = ["builtin-linux-mini"]
        elif "18080" in base_url:
            models = LocalLLMManager.get_installed_gguf_models() or ["qwen2.5-1.5b-instruct-q4_k_m.gguf"]
        elif "integrate.api.nvidia.com" in base_url:
            models = list(LocalLLMManager.NIM_POPULAR_MODELS)
        elif "11434" in base_url:
            models = ["qwen2.5:1.5b", "qwen2.5-coder:1.5b", "llama3.2:1b", "smollm2:1.7b"]
        if saved_model and saved_model not in models:
            models.insert(0, saved_model)
        if not models:
            models = ["builtin-linux-mini"]
        self._set_model_combo_items(models, saved_model or models[0])

    def _set_model_combo_items(self, models: list[str], selected_model: str):
        self._updating_ui = True
        self._available_models = list(models)
        self.model_combo.set_model(Gtk.StringList.new(self._available_models))
        if selected_model in self._available_models:
            self.model_combo.set_selected(self._available_models.index(selected_model))
        elif self._available_models:
            self.model_combo.set_selected(0)
        self._updating_ui = False

    def _on_provider_changed(self, combo, _pspec):
        if self._updating_ui:
            return
        idx = combo.get_selected()
        if 0 <= idx < len(self.PROVIDER_KEYS):
            pkey = self.PROVIDER_KEYS[idx]
            preset = LocalLLMManager.PROVIDER_PRESETS[pkey]
            AISettingsStore.set_str(self.settings, "ai-provider", pkey)
            self._updating_ui = True
            self.url_row.set_text(preset["base_url"])
            AISettingsStore.set_str(self.settings, "ai-base-url", preset["base_url"])
            if preset["default_model"]:
                self.model_entry_row.set_text(preset["default_model"])
                AISettingsStore.set_str(self.settings, "ai-model", preset["default_model"])
            self._updating_ui = False
            self._populate_initial_models(preset["default_model"])

    def _on_url_changed(self, entry):
        if self._updating_ui:
            return
        AISettingsStore.set_str(self.settings, "ai-base-url", entry.get_text().strip())

    def _on_key_changed(self, entry):
        if self._updating_ui:
            return
        AISettingsStore.set_str(self.settings, "ai-api-key", entry.get_text().strip())

    def _on_model_combo_selected(self, combo, _pspec):
        if self._updating_ui:
            return
        idx = combo.get_selected()
        if 0 <= idx < len(self._available_models):
            chosen = self._available_models[idx]
            self._updating_ui = True
            self.model_entry_row.set_text(chosen)
            self._updating_ui = False
            AISettingsStore.set_str(self.settings, "ai-model", chosen)

    def _on_model_entry_changed(self, entry):
        if self._updating_ui:
            return
        AISettingsStore.set_str(self.settings, "ai-model", entry.get_text().strip())

    def _on_fetch_models_clicked(self, _btn):
        base_url = self.url_row.get_text().strip()
        api_key = self.key_row.get_text().strip()
        self.fetch_models_btn.set_sensitive(False)
        self.fetch_models_btn.set_label("Fetching...")

        def worker():
            client = AIClient(base_url=base_url, api_key=api_key)
            res = client.test_connection()
            GLib.idle_add(self._on_fetch_models_done, res)

        threading.Thread(target=worker, daemon=True).start()

    def _on_fetch_models_done(self, res: dict):
        self.fetch_models_btn.set_sensitive(True)
        self.fetch_models_btn.set_label("Fetch Models")
        models = res.get("models", [])
        if models:
            cur = self.model_entry_row.get_text().strip()
            target = cur if cur in models else models[0]
            self._set_model_combo_items(models, target)
            self._updating_ui = True
            self.model_entry_row.set_text(target)
            self._updating_ui = False
            AISettingsStore.set_str(self.settings, "ai-model", target)
            self.model_combo.set_subtitle(f"Loaded {len(models)} models from endpoint")
        else:
            err = res.get("error", "No models returned")
            self.model_combo.set_subtitle(f"Error: {err[:60]}")

    def _on_privacy_changed(self, switch, _pspec):
        self.settings.set_boolean("ai-privacy-mask-pii", switch.get_active())
