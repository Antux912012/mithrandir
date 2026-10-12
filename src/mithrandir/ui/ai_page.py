import threading
import json
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib, Gio
from ..backend.diagnostics import DiagnosticCollector
from ..backend.host_bridge import HostBridge
from ..ai.client import AIClient
from ..ai.safety import SafetyGuard

class AIPage(Gtk.ScrolledWindow):
    """
    AI Diagnosis & Fix page.
    Connects to OpenAI-compatible endpoints or local LLMs (LM Studio, Ollama).
    """

    def __init__(self, toast_overlay: Adw.ToastOverlay, settings: Gio.Settings):
        super().__init__()
        self.toast_overlay = toast_overlay
        self.settings = settings
        self.set_vexpand(True)
        self.set_hexpand(True)
        self.set_kinetic_scrolling(True)

        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        self.box.set_margin_top(24)
        self.box.set_margin_bottom(24)
        self.box.set_margin_start(24)
        self.box.set_margin_end(24)

        # Connection Header Card
        conn_group = Adw.PreferencesGroup()
        conn_group.set_title("AI Diagnostics Assistant")
        conn_group.set_description("Connect to a local model via LM Studio or an OpenAI-compatible service to analyze system anomalies.")

        self.status_row = Adw.ActionRow(title="Provider Connection")
        self.status_row.set_subtitle("Configured via Preferences (Local / OpenAI)")
        self.status_row.add_prefix(Gtk.Image.new_from_icon_name("network-server-symbolic"))

        self.test_btn = Gtk.Button(label="Test Connection")
        self.test_btn.set_valign(Gtk.Align.CENTER)
        self.test_btn.connect("clicked", self._on_test_conn_clicked)
        self.status_row.add_suffix(self.test_btn)
        conn_group.add(self.status_row)
        self.box.append(conn_group)

        # Diagnose System Action Card
        diag_group = Adw.PreferencesGroup()
        diag_group.set_title("System Scan &amp; Analysis")

        self.diag_btn = Gtk.Button(label="Run Diagnostics &amp; Consult AI")
        self.diag_btn.add_css_class("suggested-action")
        self.diag_btn.set_margin_top(12)
        self.diag_btn.set_margin_bottom(12)
        self.diag_btn.connect("clicked", self._on_diagnose_clicked)
        diag_group.add(self.diag_btn)
        self.box.append(diag_group)

        # AI Response Display
        resp_group = Adw.PreferencesGroup()
        resp_group.set_title("AI Findings &amp; Recommended Fixes")

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

        # Interactive Proposed Fix Card
        self.fix_group = Adw.PreferencesGroup()
        self.fix_group.set_title("Interactive Fix Execution")
        self.fix_group.set_description("Commands proposed by AI require explicit confirmation and safety review.")

        self.fix_row = Adw.ActionRow(title="Proposed Command")
        self.fix_row.set_subtitle("No command proposed yet.")
        self.fix_btn = Gtk.Button(label="Run Fix")
        self.fix_btn.add_css_class("destructive-action")
        self.fix_btn.set_valign(Gtk.Align.CENTER)
        self.fix_btn.set_sensitive(False)
        self.fix_btn.connect("clicked", self._on_run_fix_clicked)
        self.fix_row.add_suffix(self.fix_btn)
        self.fix_group.add(self.fix_row)

        self.box.append(self.fix_group)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(800)
        clamp.set_child(self.box)
        self.set_child(clamp)

        self.proposed_cmd = None

    def _get_ai_client(self) -> AIClient:
        base_url = self.settings.get_string("ai-base-url")
        return AIClient(base_url=base_url)

    def _on_test_conn_clicked(self, _btn):
        client = self._get_ai_client()
        self.test_btn.set_sensitive(False)

        def worker():
            models = client.list_models()
            GLib.idle_add(self._on_test_conn_result, models)

        threading.Thread(target=worker, daemon=True).start()

    def _on_test_conn_result(self, models):
        self.test_btn.set_sensitive(True)
        if models:
            msg = f"Connected! Found models: {', '.join(models[:3])}"
            toast = Adw.Toast.new(msg)
            self.toast_overlay.add_toast(toast)
            self.status_row.set_subtitle(f"Connected to endpoint ({len(models)} models)")
        else:
            toast = Adw.Toast.new("Failed to connect to AI server. Ensure LM Studio or API is running.")
            self.toast_overlay.add_toast(toast)
            self.status_row.set_subtitle("Connection failed.")

    def _on_diagnose_clicked(self, _btn):
        self.diag_btn.set_sensitive(False)
        buf = self.response_view.get_buffer()
        buf.set_text("Collecting system diagnostics and consulting AI assistant...\n")

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
            res = client.chat_completion([{"role": "user", "content": prompt}])
            GLib.idle_add(self._on_diagnose_complete, res)

        threading.Thread(target=worker, daemon=True).start()

    def _on_diagnose_complete(self, res):
        self.diag_btn.set_sensitive(True)
        buf = self.response_view.get_buffer()

        if not res.get('success'):
            buf.set_text(f"Error querying AI assistant: {res.get('error')}")
            return

        content = res.get('content', '')
        buf.set_text(content)

        # Look for PROPOSED_COMMAND:
        self.proposed_cmd = None
        for line in content.splitlines():
            if 'PROPOSED_COMMAND:' in line:
                cmd = line.split('PROPOSED_COMMAND:')[1].strip()
                # strip backticks or quotes
                cmd = cmd.strip('`').strip('"').strip("'")
                self.proposed_cmd = cmd
                break

        if self.proposed_cmd:
            is_allowed, risk, reason = SafetyGuard.evaluate_command(self.proposed_cmd)
            self.fix_row.set_title(f"Command: {self.proposed_cmd}")
            self.fix_row.set_subtitle(f"Risk: {risk.upper()} — {reason}")
            self.fix_btn.set_sensitive(is_allowed)
        else:
            self.fix_row.set_title("Proposed Command")
            self.fix_row.set_subtitle("No specific fix command proposed.")
            self.fix_btn.set_sensitive(False)

    def _on_run_fix_clicked(self, _btn):
        if not self.proposed_cmd:
            return

        dialog = Adw.AlertDialog.new(
            "Execute Proposed Fix?",
            f"Are you sure you want to run:\n\n{self.proposed_cmd}\n\nMake sure you understand the effect before confirming."
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("run", "Run Command")
        dialog.set_response_appearance("run", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.choose(self.get_root(), None, self._on_run_dialog_response)

    def _on_run_dialog_response(self, dialog, result):
        response = dialog.choose_finish(result)
        if response == "run":
            toast = Adw.Toast.new("Executing fix in background...")
            self.toast_overlay.add_toast(toast)
            self.fix_btn.set_sensitive(False)

            def worker():
                cmd_parts = self.proposed_cmd.split()
                code, out, err = HostBridge.run_command(cmd_parts)
                GLib.idle_add(self._on_run_fix_complete, code, out, err)

            threading.Thread(target=worker, daemon=True).start()

    def _on_run_fix_complete(self, code: int, out: str, err: str):
        self.fix_btn.set_sensitive(True)
        msg = f"Finished with code {code}."
        if out:
            msg += f"\nStdout: {out.strip()}"
        if err:
            msg += f"\nStderr: {err.strip()}"
        toast = Adw.Toast.new(f"Fix executed (Exit code: {code})")
        self.toast_overlay.add_toast(toast)
        buf = self.response_view.get_buffer()
        buf.insert_at_cursor(f"\n\n--- EXECUTION RESULT ---\n{msg}\n")
