import re
import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib, Gio
from ..backend.diagnostics import DiagnosticCollector
from ..backend.host_bridge import HostBridge
from ..ai.client import AIClient
from ..ai.safety import SafetyGuard
from ..ai.local_llm import LocalLLMManager, RECOMMENDED_MINI_MODELS, RECOMMENDED_QWEN_GGUF_MODELS
from .preferences_dialog import AISettingsStore


class AIPage(Gtk.ScrolledWindow):
    """
    AI Diagnosis & Generic Linux Task Assistant page.
    Supports:
    - Built-in Zero-Dependency Offline Linux Mini-Assistant (zero download)
    - Optional In-App Mini-LLM (Qwen 2.5 GGUF + llama.cpp running directly inside Mithrandir)
    - External Offline LLMs (Ollama, LM Studio)
    - OpenAI-compatible Cloud APIs (NVIDIA NIM, OpenAI, OpenRouter)
    - Interactive Generic Linux Task prompt + Safe Fix Execution
    """

    PROVIDER_KEYS = ["builtin", "llamacpp", "ollama", "lmstudio", "nim", "openai"]

    def __init__(self, toast_overlay: Adw.ToastOverlay, settings: Gio.Settings):
        super().__init__()
        self.toast_overlay = toast_overlay
        self.settings = settings
        self.set_vexpand(True)
        self.set_hexpand(True)
        self.set_kinetic_scrolling(True)

        self._updating_ui = False
        self._available_models: list[str] = []
        self.proposed_cmd = None
        self.is_busy_terminal = False

        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        self.box.set_margin_top(24)
        self.box.set_margin_bottom(24)
        self.box.set_margin_start(24)
        self.box.set_margin_end(24)

        # 1. Connection & Model Selection Card
        self._build_connection_group()

        # 2. Local Offline Mini-LLM Manager Card
        self._build_offline_llm_group()

        # 3. Embedded Terminal Console for Downloads & Engine Logs
        self._build_terminal_revealer()

        # 4. System Diagnostics & Generic Linux Task Card
        self._build_tasks_and_diagnostics_group()

        # 5. AI Response Display
        self._build_response_group()

        # 6. Interactive Proposed Fix Card
        self._build_fix_group()

        clamp = Adw.Clamp()
        clamp.set_maximum_size(860)
        clamp.set_child(self.box)
        self.set_child(clamp)

        # Listen to settings changes from PreferencesDialog
        try:
            self.settings.connect("changed::ai-base-url", self._on_external_settings_changed)
            self.settings.connect("changed::ai-model", self._on_external_settings_changed)
            self.settings.connect("changed::ai-provider", self._on_external_settings_changed)
        except Exception:
            pass

        GLib.idle_add(self._refresh_offline_status)

    def _build_connection_group(self):
        conn_group = Adw.PreferencesGroup()
        conn_group.set_title("AI Provider &amp; Model Selection")
        conn_group.set_description(
            "Select the Built-in Zero-Download Assistant, In-App Qwen Mini-LLM, External Ollama/LM Studio, or NVIDIA NIM / OpenAI."
        )

        # Provider Preset Dropdown
        provider_labels = [LocalLLMManager.PROVIDER_PRESETS[k]["label"] for k in self.PROVIDER_KEYS]
        self.provider_combo = Adw.ComboRow(title="AI Service / Mode")
        self.provider_combo.set_model(Gtk.StringList.new(provider_labels))
        cur_prov = AISettingsStore.get_str(self.settings, "ai-provider", "builtin")
        if cur_prov in self.PROVIDER_KEYS:
            self.provider_combo.set_selected(self.PROVIDER_KEYS.index(cur_prov))
        else:
            self.provider_combo.set_selected(0)
        self.provider_combo.connect("notify::selected", self._on_provider_combo_changed)
        conn_group.add(self.provider_combo)

        # Endpoint URL EntryRow + Test/Fetch Button
        self.url_row = Adw.EntryRow(title="Endpoint Base URL")
        self.url_row.set_text(AISettingsStore.get_str(self.settings, "ai-base-url", "builtin://linux-mini"))
        self.url_row.connect("changed", self._on_url_entry_changed)

        self.test_btn = Gtk.Button(label="Fetch Models")
        self.test_btn.add_css_class("suggested-action")
        self.test_btn.set_valign(Gtk.Align.CENTER)
        self.test_btn.set_tooltip_text("Test connection and load available models from endpoint")
        self.test_btn.connect("clicked", self._on_test_conn_clicked)
        self.url_row.add_suffix(self.test_btn)
        conn_group.add(self.url_row)

        # API Key EntryRow
        self.key_row = Adw.PasswordEntryRow(title="API Key (Required for NVIDIA NIM / OpenAI, leave empty for Local)")
        self.key_row.set_text(AISettingsStore.get_str(self.settings, "ai-api-key", ""))
        self.key_row.connect("changed", self._on_key_entry_changed)
        conn_group.add(self.key_row)

        # Model Selector Dropdown
        self.model_combo = Adw.ComboRow(title="Select Model")
        self.model_combo.set_subtitle("Choose from fetched models or enter a model ID below")
        self.model_combo.connect("notify::selected", self._on_model_combo_changed)
        conn_group.add(self.model_combo)

        # Custom / Active Model EntryRow
        self.model_entry = Adw.EntryRow(title="Active Model ID")
        saved_model = AISettingsStore.get_str(self.settings, "ai-model", "")
        self.model_entry.set_text(saved_model)
        self.model_entry.connect("changed", self._on_model_entry_changed)
        conn_group.add(self.model_entry)

        self._populate_initial_models(saved_model)
        self.box.append(conn_group)

    def _build_offline_llm_group(self):
        self.offline_group = Adw.PreferencesGroup()
        self.offline_group.set_title("Optional In-App Mini-LLM (Qwen 2.5 + llama.cpp)")
        hw_rec = LocalLLMManager.get_hardware_recommendation()
        self.offline_group.set_description(
            "Mithrandir ships without bundled neural weights to stay lightweight. "
            "You can optionally install Qwen 2.5 (1.5B GGUF) and llama.cpp to run directly inside Mithrandir, or remove it anytime."
        )

        # 1. Main In-App llama.cpp Engine ActionRow
        self.llama_status_row = Adw.ActionRow(title="Mithrandir In-App Mini-LLM (llama.cpp)")
        self.llama_status_row.set_subtitle("Checking internal runtime status...")
        self.llama_status_row.add_prefix(Gtk.Image.new_from_icon_name("system-run-symbolic"))

        llama_btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        llama_btn_box.set_valign(Gtk.Align.CENTER)

        self.install_qwen_btn = Gtk.Button(label="Install Qwen + llama.cpp")
        self.install_qwen_btn.add_css_class("suggested-action")
        self.install_qwen_btn.connect("clicked", lambda _b: self._on_install_qwen_llamacpp_clicked())
        llama_btn_box.append(self.install_qwen_btn)

        self.start_llama_btn = Gtk.Button(label="Start Engine")
        self.start_llama_btn.connect("clicked", self._on_start_llama_clicked)
        llama_btn_box.append(self.start_llama_btn)

        self.stop_llama_btn = Gtk.Button(label="Stop Engine")
        self.stop_llama_btn.connect("clicked", self._on_stop_llama_clicked)
        llama_btn_box.append(self.stop_llama_btn)

        self.uninstall_llama_btn = Gtk.Button(label="Remove / Free Space")
        self.uninstall_llama_btn.add_css_class("flat")
        self.uninstall_llama_btn.connect("clicked", self._on_uninstall_llama_clicked)
        llama_btn_box.append(self.uninstall_llama_btn)

        self.llama_status_row.add_suffix(llama_btn_box)
        self.offline_group.add(self.llama_status_row)

        # 2. Curated Qwen GGUF Models Expander
        self.mini_models_expander = Adw.ExpanderRow()
        self.mini_models_expander.set_title("In-App Qwen 2.5 GGUF Model Options (0.5B – 1.5B)")
        self.mini_models_expander.set_subtitle("Lightweight quantized models executed directly on CPU inside Mithrandir")
        self.mini_models_expander.add_prefix(Gtk.Image.new_from_icon_name("folder-download-symbolic"))

        self.qwen_model_rows = {}
        for m_info in RECOMMENDED_QWEN_GGUF_MODELS:
            m_row = Adw.ActionRow()
            m_row.set_title(GLib.markup_escape_text(f"{m_info['name']}"))
            m_row.set_subtitle(
                GLib.markup_escape_text(f"File: {m_info['id']} • Size: {m_info['size']} • Min: {m_info['ram_required']} — {m_info['description']}")
            )

            btn = Gtk.Button(label="Download & Use")
            btn.set_valign(Gtk.Align.CENTER)
            btn.connect("clicked", lambda _b, mid=m_info["id"]: self._on_download_gguf_model_clicked(mid))
            m_row.add_suffix(btn)
            self.qwen_model_rows[m_info["id"]] = (m_row, btn)
            self.mini_models_expander.add_row(m_row)

        self.offline_group.add(self.mini_models_expander)

        # 3. External Ollama Runtime (Optional Expander)
        self.ollama_expander = Adw.ExpanderRow()
        self.ollama_expander.set_title("External Ollama Runtime (Optional)")
        self.ollama_expander.set_subtitle("Connect to a system-wide Ollama daemon on localhost:11434 if installed")
        self.ollama_expander.add_prefix(Gtk.Image.new_from_icon_name("computer-symbolic"))

        self.ollama_status_row = Adw.ActionRow(title="Local Ollama AI Engine")
        self.ollama_status_row.set_subtitle("Checking system status...")

        ollama_btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        ollama_btn_box.set_valign(Gtk.Align.CENTER)

        self.start_ollama_btn = Gtk.Button(label="Start Server")
        self.start_ollama_btn.connect("clicked", self._on_start_ollama_clicked)
        ollama_btn_box.append(self.start_ollama_btn)

        self.install_ollama_btn = Gtk.Button(label="Install Ollama")
        self.install_ollama_btn.connect("clicked", self._on_install_ollama_clicked)
        ollama_btn_box.append(self.install_ollama_btn)

        self.ollama_status_row.add_suffix(ollama_btn_box)
        self.ollama_expander.add_row(self.ollama_status_row)
        self.offline_group.add(self.ollama_expander)

        self.box.append(self.offline_group)

    def _build_terminal_revealer(self):
        self.terminal_revealer = Gtk.Revealer()
        self.terminal_revealer.set_transition_type(Gtk.RevealerTransitionType.SLIDE_DOWN)
        self.terminal_revealer.set_reveal_child(False)

        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        card.add_css_class("terminal-card")

        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        header.append(Gtk.Image.new_from_icon_name("utilities-terminal-symbolic"))

        self.terminal_title = Gtk.Label(label="<b>Offline AI Runtime &amp; Model Downloader</b>")
        self.terminal_title.set_use_markup(True)
        self.terminal_title.set_halign(Gtk.Align.START)
        self.terminal_title.set_hexpand(True)
        header.append(self.terminal_title)

        self.terminal_spinner = Gtk.Spinner()
        self.terminal_spinner.set_visible(False)
        header.append(self.terminal_spinner)

        hide_btn = Gtk.Button(label="Hide")
        hide_btn.add_css_class("flat")
        hide_btn.connect("clicked", lambda _b: self.terminal_revealer.set_reveal_child(False))
        header.append(hide_btn)
        card.append(header)

        scroller = Gtk.ScrolledWindow()
        scroller.set_min_content_height(150)
        scroller.set_max_content_height(220)

        self.terminal_view = Gtk.TextView()
        self.terminal_view.set_editable(False)
        self.terminal_view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        self.terminal_view.add_css_class("terminal-view")
        self.terminal_buffer = self.terminal_view.get_buffer()

        scroller.set_child(self.terminal_view)
        card.append(scroller)

        self.terminal_revealer.set_child(card)
        self.box.append(self.terminal_revealer)

    def _build_tasks_and_diagnostics_group(self):
        diag_group = Adw.PreferencesGroup()
        diag_group.set_title("System Diagnostics &amp; Generic Linux Task Assistant")
        diag_group.set_description(
            "Run a full automated telemetry diagnosis or ask the AI to handle any generic Linux task."
        )

        # Full System Scan Button (Note: Gtk.Button uses plain text, not XML entities)
        self.diag_btn = Gtk.Button(label="Run Full System Diagnostics & Consult AI")
        self.diag_btn.add_css_class("suggested-action")
        self.diag_btn.set_margin_top(6)
        self.diag_btn.set_margin_bottom(8)
        self.diag_btn.connect("clicked", self._on_diagnose_clicked)
        diag_group.add(self.diag_btn)

        # Generic Linux Task Input Row
        task_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        task_box.set_margin_top(4)

        self.task_entry = Gtk.Entry()
        self.task_entry.set_hexpand(True)
        self.task_entry.set_placeholder_text(
            "Ask a Linux question or task (e.g., 'Find files over 100MB', 'Check open ports', 'Fix broken apt packages')..."
        )
        self.task_entry.connect("activate", self._on_ask_task_clicked)
        task_box.append(self.task_entry)

        self.ask_btn = Gtk.Button(label="Ask AI")
        self.ask_btn.add_css_class("suggested-action")
        self.ask_btn.connect("clicked", self._on_ask_task_clicked)
        task_box.append(self.ask_btn)

        diag_group.add(task_box)

        # Quick Task Chips
        chips_flow = Gtk.FlowBox()
        chips_flow.set_selection_mode(Gtk.SelectionMode.NONE)
        chips_flow.set_max_children_per_line(6)
        chips_flow.set_column_spacing(6)
        chips_flow.set_row_spacing(6)
        chips_flow.set_margin_top(8)

        quick_tasks = [
            ("Find Large Files", "Find the largest files over 100MB in my home directory"),
            ("Open Network Ports", "Check all open listening network ports and processes"),
            ("Top Memory / CPU", "Show top processes consuming the most RAM and CPU"),
            ("Fix Broken Packages", "Repair broken packages and unmet dependencies"),
            ("Analyze Boot Time", "Check system boot speed and slowest startup services"),
            ("Restart Audio Stack", "Restart PipeWire and WirePlumber audio services"),
        ]
        for label_text, prompt_text in quick_tasks:
            chip_btn = Gtk.Button(label=label_text)
            chip_btn.add_css_class("pill")
            chip_btn.connect("clicked", lambda _b, pt=prompt_text: self._run_quick_task(pt))
            chips_flow.append(chip_btn)

        diag_group.add(chips_flow)
        self.box.append(diag_group)

    def _build_response_group(self):
        resp_group = Adw.PreferencesGroup()
        resp_group.set_title("AI Findings &amp; Recommended Actions")

        self.response_view = Gtk.TextView()
        self.response_view.set_editable(False)
        self.response_view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        self.response_view.set_monospace(True)
        self.response_view.set_margin_top(12)
        self.response_view.set_margin_bottom(12)
        self.response_view.set_margin_start(12)
        self.response_view.set_margin_end(12)

        resp_scroller = Gtk.ScrolledWindow()
        resp_scroller.set_min_content_height(240)
        resp_scroller.set_child(self.response_view)

        frame = Gtk.Frame()
        frame.set_child(resp_scroller)
        resp_group.add(frame)
        self.box.append(resp_group)

    def _build_fix_group(self):
        self.fix_group = Adw.PreferencesGroup()
        self.fix_group.set_title("Interactive Command &amp; Fix Execution")
        self.fix_group.set_description("Commands proposed by AI are verified by SafetyGuard and require confirmation.")

        self.fix_row = Adw.ActionRow(title="Proposed Command")
        self.fix_row.set_subtitle("No command proposed yet.")
        self.fix_btn = Gtk.Button(label="Run Command")
        self.fix_btn.add_css_class("destructive-action")
        self.fix_btn.set_valign(Gtk.Align.CENTER)
        self.fix_btn.set_sensitive(False)
        self.fix_btn.connect("clicked", self._on_run_fix_clicked)
        self.fix_row.add_suffix(self.fix_btn)
        self.fix_group.add(self.fix_row)
        self.box.append(self.fix_group)

    # --- Settings & Model Sync ---

    def _populate_initial_models(self, saved_model: str):
        base_url = self.url_row.get_text().strip()
        models = []
        if base_url.startswith("builtin://"):
            models = ["builtin-linux-mini"]
        elif "18080" in base_url:
            models = LocalLLMManager.get_installed_gguf_models() or [m["id"] for m in RECOMMENDED_QWEN_GGUF_MODELS]
        elif "integrate.api.nvidia.com" in base_url:
            models = list(LocalLLMManager.NIM_POPULAR_MODELS)
        elif "11434" in base_url:
            models = ["qwen2.5:1.5b", "qwen2.5-coder:1.5b", "llama3.2:1b", "smollm2:1.7b"]
        if saved_model and saved_model not in models:
            models.insert(0, saved_model)
        if not models:
            models = ["qwen2.5-1.5b-instruct-q4_k_m.gguf"]

        target = saved_model if saved_model else models[0]
        self._set_model_combo_items(models, target)
        if not saved_model:
            self._updating_ui = True
            self.model_entry.set_text(target)
            self._updating_ui = False

    def _set_model_combo_items(self, models: list[str], selected_model: str):
        self._updating_ui = True
        self._available_models = list(models)
        self.model_combo.set_model(Gtk.StringList.new(self._available_models))
        if selected_model in self._available_models:
            self.model_combo.set_selected(self._available_models.index(selected_model))
        elif self._available_models:
            self.model_combo.set_selected(0)
        self._updating_ui = False

    def _on_external_settings_changed(self, _settings, _key):
        if self._updating_ui:
            return
        self._updating_ui = True
        url = AISettingsStore.get_str(self.settings, "ai-base-url", "builtin://linux-mini")
        model = AISettingsStore.get_str(self.settings, "ai-model", "")
        key = AISettingsStore.get_str(self.settings, "ai-api-key", "")
        self.url_row.set_text(url)
        self.key_row.set_text(key)
        if model:
            self.model_entry.set_text(model)
        self._updating_ui = False

    def _on_provider_combo_changed(self, combo, _pspec):
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
                self.model_entry.set_text(preset["default_model"])
                AISettingsStore.set_str(self.settings, "ai-model", preset["default_model"])
            self._updating_ui = False
            self._populate_initial_models(preset["default_model"])

    def _on_url_entry_changed(self, entry):
        if self._updating_ui:
            return
        url = entry.get_text().strip()
        AISettingsStore.set_str(self.settings, "ai-base-url", url)
        if "18080" in url:
            gguf_models = LocalLLMManager.get_installed_gguf_models() or [m["id"] for m in RECOMMENDED_QWEN_GGUF_MODELS]
            cur = self.model_entry.get_text().strip() or gguf_models[0]
            self._set_model_combo_items(gguf_models, cur)
            self._updating_ui = True
            self.model_entry.set_text(cur)
            self._updating_ui = False
            AISettingsStore.set_str(self.settings, "ai-model", cur)
        elif "integrate.api.nvidia.com" in url and "meta/llama-3.1-70b-instruct" not in self._available_models:
            cur = self.model_entry.get_text().strip() or "meta/llama-3.1-70b-instruct"
            self._set_model_combo_items(list(LocalLLMManager.NIM_POPULAR_MODELS), cur)
            self._updating_ui = True
            self.model_entry.set_text(cur)
            self._updating_ui = False
            AISettingsStore.set_str(self.settings, "ai-model", cur)

    def _on_key_entry_changed(self, entry):
        if self._updating_ui:
            return
        AISettingsStore.set_str(self.settings, "ai-api-key", entry.get_text().strip())

    def _on_model_combo_changed(self, combo, _pspec):
        if self._updating_ui:
            return
        idx = combo.get_selected()
        if 0 <= idx < len(self._available_models):
            chosen = self._available_models[idx]
            self._updating_ui = True
            self.model_entry.set_text(chosen)
            self._updating_ui = False
            AISettingsStore.set_str(self.settings, "ai-model", chosen)

    def _on_model_entry_changed(self, entry):
        if self._updating_ui:
            return
        AISettingsStore.set_str(self.settings, "ai-model", entry.get_text().strip())

    def _get_ai_client(self) -> AIClient:
        base_url = self.url_row.get_text().strip() or AISettingsStore.get_str(self.settings, "ai-base-url", "http://localhost:11434/v1")
        api_key = self.key_row.get_text().strip() or AISettingsStore.get_str(self.settings, "ai-api-key", "")
        model = self.model_entry.get_text().strip() or AISettingsStore.get_str(self.settings, "ai-model", "")
        return AIClient(base_url=base_url, api_key=api_key, model=model)

    def _get_selected_model(self) -> str:
        m = self.model_entry.get_text().strip()
        if m:
            return m
        idx = self.model_combo.get_selected()
        if 0 <= idx < len(self._available_models):
            return self._available_models[idx]
        return AISettingsStore.get_str(self.settings, "ai-model", "")

    # --- Connection Test & Model Fetching ---

    def _on_test_conn_clicked(self, _btn):
        client = self._get_ai_client()
        self.test_btn.set_sensitive(False)
        self.test_btn.set_label("Fetching...")

        def worker():
            res = client.test_connection()
            GLib.idle_add(self._on_test_conn_result, res)

        threading.Thread(target=worker, daemon=True).start()

    def _on_test_conn_result(self, res: dict):
        self.test_btn.set_sensitive(True)
        self.test_btn.set_label("Fetch Models")
        models = res.get("models", [])

        if models:
            cur = self.model_entry.get_text().strip()
            target = cur if cur in models else models[0]
            self._set_model_combo_items(models, target)
            self._updating_ui = True
            self.model_entry.set_text(target)
            self._updating_ui = False
            AISettingsStore.set_str(self.settings, "ai-model", target)

        if res.get("success") and models:
            msg = f"Connected! Loaded {len(models)} models (Active: {self.model_entry.get_text().strip()})"
            self.toast_overlay.add_toast(Adw.Toast.new(msg))
            self.model_combo.set_subtitle(GLib.markup_escape_text(f"Loaded {len(models)} models from endpoint"))
        else:
            err = res.get("error", "Could not fetch models from server.")
            self.toast_overlay.add_toast(Adw.Toast.new(f"AI Endpoint: {err[:80]}"))
            self.model_combo.set_subtitle(GLib.markup_escape_text(f"Status: {err[:80]}"))

    # --- Offline Mini-LLM Management ---

    # --- Offline Mini-LLM Management ---

    def _refresh_offline_status(self):
        def worker():
            llama_st = LocalLLMManager.check_llamacpp_status()
            ollama_st = LocalLLMManager.check_ollama_status()
            GLib.idle_add(self._update_offline_ui, llama_st, ollama_st)

        threading.Thread(target=worker, daemon=True).start()

    def _update_offline_ui(self, llama_st: dict, ollama_st: dict) -> bool:
        # 1. Update in-app llama.cpp engine status
        engine_installed = llama_st.get("engine_installed", False)
        default_model_installed = llama_st.get("default_model_installed", False)
        llama_running = llama_st.get("running", False)
        llama_models = llama_st.get("models", [])
        disk_mb = llama_st.get("disk_usage_mb", 0.0)

        if llama_running:
            self.llama_status_row.set_subtitle(
                GLib.markup_escape_text(f"Running inside Mithrandir on localhost:18080 • Model: {llama_st.get('active_model')} ({disk_mb} MB disk)")
            )
            self.install_qwen_btn.set_visible(False)
            self.start_llama_btn.set_visible(False)
            self.stop_llama_btn.set_visible(True)
            self.uninstall_llama_btn.set_visible(True)
        elif engine_installed and default_model_installed:
            self.llama_status_row.set_subtitle(
                GLib.markup_escape_text(f"Installed & Ready ({disk_mb} MB disk). Click 'Start Engine' or ask a query to run.")
            )
            self.install_qwen_btn.set_visible(False)
            self.start_llama_btn.set_visible(True)
            self.stop_llama_btn.set_visible(False)
            self.uninstall_llama_btn.set_visible(True)
        elif engine_installed and not default_model_installed:
            self.llama_status_row.set_subtitle(
                "llama.cpp is installed, but Qwen model weights are missing. Click 'Install Qwen + llama.cpp'."
            )
            self.install_qwen_btn.set_visible(True)
            self.start_llama_btn.set_visible(False)
            self.stop_llama_btn.set_visible(False)
            self.uninstall_llama_btn.set_visible(True)
        else:
            self.llama_status_row.set_subtitle(
                "Not installed (App ships without LLM weights). Click 'Install Qwen + llama.cpp' (~986 MB) to enable in-app offline AI."
            )
            self.install_qwen_btn.set_visible(True)
            self.start_llama_btn.set_visible(False)
            self.stop_llama_btn.set_visible(False)
            self.uninstall_llama_btn.set_visible(disk_mb > 0)

        # Update Qwen GGUF model items in expander
        for mid, (row, btn) in getattr(self, "qwen_model_rows", {}).items():
            if mid in llama_models:
                if llama_running and llama_st.get("active_model") == mid:
                    btn.set_label("Active ✓")
                    btn.set_sensitive(False)
                else:
                    btn.set_label("Switch to Model")
                    btn.set_sensitive(True)
            else:
                btn.set_label("Download & Use")
                btn.set_sensitive(True)

        if llama_models and "18080" in self.url_row.get_text():
            cur = self.model_entry.get_text().strip()
            target = cur if cur in llama_models else llama_models[0]
            self._set_model_combo_items(llama_models, target)

        # 2. Update external Ollama status
        o_installed = ollama_st.get("installed", False)
        o_running = ollama_st.get("running", False)
        o_models = ollama_st.get("models", [])

        if o_installed and o_running:
            models_str = f"Models: {', '.join(o_models[:3])}" if o_models else "No models pulled"
            self.ollama_status_row.set_subtitle(GLib.markup_escape_text(f"Running on localhost:11434 • {models_str}"))
            self.start_ollama_btn.set_visible(False)
            self.install_ollama_btn.set_visible(False)
        elif o_installed and not o_running:
            self.ollama_status_row.set_subtitle("Ollama daemon is stopped. Click 'Start Server' to launch.")
            self.start_ollama_btn.set_visible(True)
            self.install_ollama_btn.set_visible(False)
        else:
            self.ollama_status_row.set_subtitle("Ollama is not installed on this system.")
            self.start_ollama_btn.set_visible(False)
            self.install_ollama_btn.set_visible(True)

        return False

    def _append_terminal_line(self, line: str):
        end_iter = self.terminal_buffer.get_end_iter()
        self.terminal_buffer.insert(end_iter, line + "\n")
        mark = self.terminal_buffer.create_mark(None, self.terminal_buffer.get_end_iter(), False)
        self.terminal_view.scroll_to_mark(mark, 0.05, True, 0.0, 1.0)
        return False

    def _on_install_qwen_llamacpp_clicked(self):
        if self.is_busy_terminal:
            return
        self.is_busy_terminal = True
        self.install_qwen_btn.set_sensitive(False)
        self.terminal_buffer.set_text("")
        self.terminal_revealer.set_reveal_child(True)
        self.terminal_spinner.set_visible(True)
        self.terminal_spinner.start()

        def worker():
            def line_cb(line: str):
                GLib.idle_add(self._append_terminal_line, line)

            ok, msg = LocalLLMManager.install_qwen_with_llamacpp(line_callback=line_cb)

            def done():
                self.is_busy_terminal = False
                self.install_qwen_btn.set_sensitive(True)
                self.terminal_spinner.stop()
                self.terminal_spinner.set_visible(False)
                self.toast_overlay.add_toast(Adw.Toast.new(msg))
                if ok:
                    self._updating_ui = True
                    self.provider_combo.set_selected(self.PROVIDER_KEYS.index("llamacpp"))
                    self.url_row.set_text("http://127.0.0.1:18080/v1")
                    self.model_entry.set_text("qwen2.5-1.5b-instruct-q4_k_m.gguf")
                    self._updating_ui = False
                    AISettingsStore.set_str(self.settings, "ai-provider", "llamacpp")
                    AISettingsStore.set_str(self.settings, "ai-base-url", "http://127.0.0.1:18080/v1")
                    AISettingsStore.set_str(self.settings, "ai-model", "qwen2.5-1.5b-instruct-q4_k_m.gguf")
                self._refresh_offline_status()
                return False

            GLib.idle_add(done)

        threading.Thread(target=worker, daemon=True).start()

    def _on_start_llama_clicked(self, _btn):
        self.start_llama_btn.set_sensitive(False)

        def worker():
            target_model = self.model_entry.get_text().strip() or "qwen2.5-1.5b-instruct-q4_k_m.gguf"
            ok, msg = LocalLLMManager.start_llamacpp_server(target_model)
            def done():
                self.start_llama_btn.set_sensitive(True)
                self.toast_overlay.add_toast(Adw.Toast.new(msg))
                self._refresh_offline_status()
                return False
            GLib.idle_add(done)

        threading.Thread(target=worker, daemon=True).start()

    def _on_stop_llama_clicked(self, _btn):
        self.stop_llama_btn.set_sensitive(False)

        def worker():
            ok, msg = LocalLLMManager.stop_llamacpp_server()
            def done():
                self.stop_llama_btn.set_sensitive(True)
                self.toast_overlay.add_toast(Adw.Toast.new(msg))
                self._refresh_offline_status()
                return False
            GLib.idle_add(done)

        threading.Thread(target=worker, daemon=True).start()

    def _on_uninstall_llama_clicked(self, _btn):
        dialog = Adw.AlertDialog.new(
            "Uninstall Qwen Mini-LLM & llama.cpp?",
            "This will delete the in-app llama.cpp inference engine and all downloaded Qwen GGUF model weights from your system, freeing disk space."
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("uninstall", "Uninstall & Free Space")
        dialog.set_response_appearance("uninstall", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.choose(self.get_root(), None, self._on_uninstall_llama_dialog_response)

    def _on_uninstall_llama_dialog_response(self, dialog, result):
        response = dialog.choose_finish(result)
        if response == "uninstall":
            def worker():
                ok, msg = LocalLLMManager.uninstall_llamacpp_and_models(remove_engine=True, remove_models=True)
                def done():
                    self.toast_overlay.add_toast(Adw.Toast.new(msg))
                    self._updating_ui = True
                    self.provider_combo.set_selected(self.PROVIDER_KEYS.index("builtin"))
                    self.url_row.set_text("builtin://linux-mini")
                    self.model_entry.set_text("builtin-linux-mini")
                    self._updating_ui = False
                    AISettingsStore.set_str(self.settings, "ai-provider", "builtin")
                    AISettingsStore.set_str(self.settings, "ai-base-url", "builtin://linux-mini")
                    AISettingsStore.set_str(self.settings, "ai-model", "builtin-linux-mini")
                    self._refresh_offline_status()
                    return False
                GLib.idle_add(done)

            threading.Thread(target=worker, daemon=True).start()

    def _on_download_gguf_model_clicked(self, model_id: str):
        if self.is_busy_terminal:
            return
        self.is_busy_terminal = True
        self.terminal_buffer.set_text("")
        self.terminal_revealer.set_reveal_child(True)
        self.terminal_spinner.set_visible(True)
        self.terminal_spinner.start()

        def worker():
            def line_cb(line: str):
                GLib.idle_add(self._append_terminal_line, line)

            # Ensure llama.cpp engine is installed first
            if not LocalLLMManager.find_llama_server_binary():
                LocalLLMManager.install_llamacpp_engine(line_callback=line_cb)

            ok, msg = LocalLLMManager.download_gguf_model(model_id, line_callback=line_cb)
            if ok:
                LocalLLMManager.stop_llamacpp_server()
                LocalLLMManager.start_llamacpp_server(model_id)

            def done():
                self.is_busy_terminal = False
                self.terminal_spinner.stop()
                self.terminal_spinner.set_visible(False)
                self.toast_overlay.add_toast(Adw.Toast.new(msg))
                if ok:
                    self._updating_ui = True
                    self.provider_combo.set_selected(self.PROVIDER_KEYS.index("llamacpp"))
                    self.url_row.set_text("http://127.0.0.1:18080/v1")
                    self.model_entry.set_text(model_id)
                    self._updating_ui = False
                    AISettingsStore.set_str(self.settings, "ai-provider", "llamacpp")
                    AISettingsStore.set_str(self.settings, "ai-base-url", "http://127.0.0.1:18080/v1")
                    AISettingsStore.set_str(self.settings, "ai-model", model_id)
                self._refresh_offline_status()
                return False

            GLib.idle_add(done)

        threading.Thread(target=worker, daemon=True).start()

    def _on_start_ollama_clicked(self, _btn):
        self.start_ollama_btn.set_sensitive(False)

        def worker():
            ok, msg = LocalLLMManager.start_ollama_server()
            def done():
                self.start_ollama_btn.set_sensitive(True)
                self.toast_overlay.add_toast(Adw.Toast.new(msg))
                self._refresh_offline_status()
                return False
            GLib.idle_add(done)

        threading.Thread(target=worker, daemon=True).start()

    def _on_install_ollama_clicked(self, _btn):
        if self.is_busy_terminal:
            return
        self.is_busy_terminal = True
        self.install_ollama_btn.set_sensitive(False)
        self.terminal_buffer.set_text("")
        self.terminal_revealer.set_reveal_child(True)
        self.terminal_spinner.set_visible(True)
        self.terminal_spinner.start()

        def worker():
            def line_cb(line: str):
                GLib.idle_add(self._append_terminal_line, line)

            ok, msg = LocalLLMManager.install_ollama(line_callback=line_cb)

            def done():
                self.is_busy_terminal = False
                self.install_ollama_btn.set_sensitive(True)
                self.terminal_spinner.stop()
                self.terminal_spinner.set_visible(False)
                self.toast_overlay.add_toast(Adw.Toast.new(msg))
                self._refresh_offline_status()
                return False

            GLib.idle_add(done)

        threading.Thread(target=worker, daemon=True).start()

    def _on_pull_mini_model_clicked(self, model_id: str):
        if self.is_busy_terminal:
            return
        self.is_busy_terminal = True
        self.terminal_buffer.set_text("")
        self.terminal_revealer.set_reveal_child(True)
        self.terminal_spinner.set_visible(True)
        self.terminal_spinner.start()

        def worker():
            def line_cb(line: str):
                GLib.idle_add(self._append_terminal_line, line)

            ok, msg = LocalLLMManager.pull_model(model_id, line_callback=line_cb)

            def done():
                self.is_busy_terminal = False
                self.terminal_spinner.stop()
                self.terminal_spinner.set_visible(False)
                self.toast_overlay.add_toast(Adw.Toast.new(msg))
                if ok:
                    self._updating_ui = True
                    self.provider_combo.set_selected(self.PROVIDER_KEYS.index("ollama"))
                    self.url_row.set_text("http://localhost:11434/v1")
                    self.model_entry.set_text(model_id)
                    self._updating_ui = False
                    AISettingsStore.set_str(self.settings, "ai-provider", "ollama")
                    AISettingsStore.set_str(self.settings, "ai-base-url", "http://localhost:11434/v1")
                    AISettingsStore.set_str(self.settings, "ai-model", model_id)
                self._refresh_offline_status()
                return False

            GLib.idle_add(done)

        threading.Thread(target=worker, daemon=True).start()

    # --- Diagnostics & Generic Linux Tasks ---

    def _on_diagnose_clicked(self, _btn):
        self.diag_btn.set_sensitive(False)
        self.ask_btn.set_sensitive(False)
        selected_model = self._get_selected_model()
        buf = self.response_view.get_buffer()
        buf.set_text(f"Collecting system diagnostics and consulting AI ({selected_model or 'auto'})...\n")

        def worker():
            summary = DiagnosticCollector.collect_system_summary()
            mask_pii = self.settings.get_boolean("ai-privacy-mask-pii")
            recent_errors = DiagnosticCollector.redact_pii(summary['recent_errors']) if mask_pii else summary['recent_errors']
            failed_units = DiagnosticCollector.redact_pii(summary['failed_units']) if mask_pii else summary['failed_units']

            prompt = (
                f"You are a GNU/Linux system diagnostics assistant. Analyze this system state:\n"
                f"- OS: {summary['os']}\n"
                f"- Kernel: {summary['kernel']}\n"
                f"- Desktop: {summary['desktop']}\n"
                f"- Memory: {summary['memory_used_mb']}MB used of {summary['memory_total_mb']}MB ({summary['memory_percent']}%)\n"
                f"- Disk: {summary['disk_free_gb']}GB free on root ({summary['disk_percent']}% used)\n"
                f"- Failed systemd units:\n{failed_units}\n"
                f"- Recent system errors:\n{recent_errors}\n\n"
                f"Provide a brief diagnosis of any anomalies, and if an action is needed, suggest ONE single Linux command under the header PROPOSED_COMMAND: <command>"
            )

            client = self._get_ai_client()
            res = client.chat_completion(
                [{"role": "user", "content": prompt}],
                model=selected_model or None
            )
            # If external endpoint failed and user is offline, offer seamless fallback notice
            if not res.get("success") and not client.is_builtin():
                fallback_res = AIClient(base_url="builtin://linux-mini").chat_completion(
                    [{"role": "user", "content": prompt}],
                    model="builtin-linux-mini"
                )
                res = {
                    "success": True,
                    "content": (
                        f"[Notice: Remote/Local endpoint '{client.base_url}' returned: {res.get('error')}]\n"
                        f"[Automatically falling back to Mithrandir Built-in Offline Linux Mini-Assistant]\n\n"
                        f"{fallback_res.get('content', '')}"
                    ),
                }
            GLib.idle_add(self._on_diagnose_complete, res)

        threading.Thread(target=worker, daemon=True).start()

    def _run_quick_task(self, task_text: str):
        self.task_entry.set_text(task_text)
        self._on_ask_task_clicked(None)

    def _on_ask_task_clicked(self, _widget):
        query = self.task_entry.get_text().strip()
        if not query:
            return

        self.diag_btn.set_sensitive(False)
        self.ask_btn.set_sensitive(False)
        selected_model = self._get_selected_model()
        buf = self.response_view.get_buffer()
        buf.set_text(f"Consulting AI ({selected_model or 'auto'}) for task: {query}...\n")

        def worker():
            summary = DiagnosticCollector.collect_system_summary()
            system_msg = (
                f"You are Mithrandir, an expert GNU/Linux assistant running on {summary['os']} "
                f"(Kernel {summary['kernel']}, Desktop: {summary['desktop']}). "
                f"Answer the user's Linux task concisely and provide ONE safe executable command on its own line "
                f"starting with exact prefix: PROPOSED_COMMAND: <command>"
            )
            client = self._get_ai_client()
            if client.is_builtin():
                res = client.chat_completion([{"role": "user", "content": query}], model="builtin-linux-mini")
            else:
                res = client.chat_completion(
                    [
                        {"role": "system", "content": system_msg},
                        {"role": "user", "content": query},
                    ],
                    model=selected_model or None,
                )
                if not res.get("success"):
                    fallback_res = AIClient(base_url="builtin://linux-mini").chat_completion(
                        [{"role": "user", "content": query}],
                        model="builtin-linux-mini"
                    )
                    res = {
                        "success": True,
                        "content": (
                            f"[Notice: Endpoint '{client.base_url}' returned: {res.get('error')}]\n"
                            f"[Automatically falling back to Mithrandir Built-in Offline Linux Mini-Assistant]\n\n"
                            f"{fallback_res.get('content', '')}"
                        ),
                    }
            GLib.idle_add(self._on_diagnose_complete, res)

        threading.Thread(target=worker, daemon=True).start()

    @staticmethod
    def _extract_proposed_command(content: str) -> str | None:
        lines = content.splitlines()
        for idx, line in enumerate(lines):
            if "PROPOSED_COMMAND:" in line:
                after = line.split("PROPOSED_COMMAND:", 1)[1].strip()
                after = re.sub(r"^\*+\s*", "", after).strip()
                after = after.strip("`").strip('"').strip("'").strip()
                if after:
                    return after
                # Check next non-empty line if command was placed in a code block below header
                for next_line in lines[idx + 1 : idx + 4]:
                    nl = next_line.strip()
                    if not nl or nl.startswith("```"):
                        continue
                    return nl.strip("`").strip()
        return None

    def _on_diagnose_complete(self, res):
        self.diag_btn.set_sensitive(True)
        self.ask_btn.set_sensitive(True)
        buf = self.response_view.get_buffer()

        if not res.get('success'):
            buf.set_text(f"Error querying AI assistant: {res.get('error')}")
            return

        content = res.get('content', '')
        buf.set_text(content)

        self.proposed_cmd = self._extract_proposed_command(content)

        if self.proposed_cmd:
            is_allowed, risk, reason = SafetyGuard.evaluate_command(self.proposed_cmd)
            self.fix_row.set_title(GLib.markup_escape_text(f"Command: {self.proposed_cmd}"))
            self.fix_row.set_subtitle(GLib.markup_escape_text(f"Risk: {risk.upper()} — {reason}"))
            self.fix_btn.set_sensitive(is_allowed)
        else:
            self.fix_row.set_title("Proposed Command")
            self.fix_row.set_subtitle("No specific fix command proposed.")
            self.fix_btn.set_sensitive(False)

    def _on_run_fix_clicked(self, _btn):
        if not self.proposed_cmd:
            return

        dialog = Adw.AlertDialog.new(
            "Execute Proposed Command?",
            f"Are you sure you want to run:\n\n{self.proposed_cmd}\n\nMake sure you understand the effect before confirming."
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("run", "Run Command")
        dialog.set_response_appearance("run", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.choose(self.get_root(), None, self._on_run_dialog_response)

    def _on_run_dialog_response(self, dialog, result):
        response = dialog.choose_finish(result)
        if response == "run":
            toast = Adw.Toast.new("Executing command in background...")
            self.toast_overlay.add_toast(toast)
            self.fix_btn.set_sensitive(False)

            def worker():
                raw_cmd = self.proposed_cmd.strip()
                use_pkexec = False
                if raw_cmd.startswith("pkexec "):
                    use_pkexec = True
                    raw_cmd = raw_cmd[len("pkexec "):].strip()
                elif raw_cmd.startswith("sudo "):
                    use_pkexec = True
                    raw_cmd = raw_cmd[len("sudo "):].strip()

                code, out, err = HostBridge.run_command(
                    ["sh", "-c", raw_cmd],
                    use_pkexec=use_pkexec,
                    timeout=60
                )
                GLib.idle_add(self._on_run_fix_complete, code, out, err)

            threading.Thread(target=worker, daemon=True).start()

    def _on_run_fix_complete(self, code: int, out: str, err: str):
        self.fix_btn.set_sensitive(True)
        msg = f"Finished with exit code {code}."
        if out:
            msg += f"\nStdout:\n{out.strip()}"
        if err:
            msg += f"\nStderr:\n{err.strip()}"
        toast = Adw.Toast.new(f"Command executed (Exit code: {code})")
        self.toast_overlay.add_toast(toast)
        buf = self.response_view.get_buffer()
        end_iter = buf.get_end_iter()
        buf.insert(end_iter, f"\n\n--- EXECUTION RESULT ---\n{msg}\n")
