import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib
from ..backend.grub import GrubManager

class GrubPage(Gtk.ScrolledWindow):
    """
    GRUB Bootloader Configuration and Customization Page.
    Allows personalizing boot timeout, quiet/splash kernel parameters,
    OS Prober, and restoring pristine default settings.
    """

    def __init__(self, toast_overlay: Adw.ToastOverlay = None):
        super().__init__()
        self.toast_overlay = toast_overlay
        self.set_vexpand(True)
        self.set_hexpand(True)
        self.set_kinetic_scrolling(True)

        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        self.box.set_margin_top(24)
        self.box.set_margin_bottom(24)
        self.box.set_margin_start(24)
        self.box.set_margin_end(24)

        # 1. Status overview
        self.status_group = Adw.PreferencesGroup()
        self.status_group.set_title("Bootloader Overview")
        self.status_group.set_description("Manage GRUB bootloader settings in /etc/default/grub safely")

        self.status_row = Adw.ActionRow(title="GRUB Bootloader Status")
        self.status_group.add(self.status_row)
        self.box.append(self.status_group)

        # 2. Personalisation settings
        self.options_group = Adw.PreferencesGroup()
        self.options_group.set_title("GRUB Personalisation")
        self.options_group.set_description("Customize boot menu appearance, delays, and kernel flags")

        # Timeout spin row
        self.timeout_row = Adw.SpinRow.new_with_range(0, 60, 1)
        self.timeout_row.set_title("Boot Menu Timeout (Seconds)")
        self.timeout_row.set_subtitle("Time GRUB waits before automatically launching default OS")
        self.options_group.add(self.timeout_row)

        # Default entry row
        self.default_row = Adw.EntryRow(title="Default Boot Entry")
        self.default_row.set_text("0")
        self.options_group.add(self.default_row)

        # Quiet boot switch
        self.quiet_row = Adw.SwitchRow(title="Quiet Boot Mode ('quiet')")
        self.quiet_row.set_subtitle("Hides kernel initialization messages during boot")
        self.options_group.add(self.quiet_row)

        # Splash screen switch
        self.splash_row = Adw.SwitchRow(title="Boot Splash Screen ('splash')")
        self.splash_row.set_subtitle("Displays graphical distribution boot animation")
        self.options_group.add(self.splash_row)

        # OS Prober switch
        self.os_prober_row = Adw.SwitchRow(title="Detect Other Operating Systems (OS Prober)")
        self.os_prober_row.set_subtitle("Automatically scan for Windows, macOS, or other Linux installs")
        self.options_group.add(self.os_prober_row)

        # Extra kernel parameters
        self.extra_args_row = Adw.EntryRow(title="Extra Kernel Parameters")
        self.options_group.add(self.extra_args_row)

        self.box.append(self.options_group)

        # 3. Action buttons
        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.END)

        self.revert_btn = Gtk.Button(label="Revert to Default")
        self.revert_btn.add_css_class("destructive-action")
        self.revert_btn.connect("clicked", self._on_revert_clicked)
        btn_box.append(self.revert_btn)

        self.save_btn = Gtk.Button(label="Save & Apply GRUB")
        self.save_btn.add_css_class("suggested-action")
        self.save_btn.connect("clicked", self._on_save_clicked)
        btn_box.append(self.save_btn)

        self.box.append(btn_box)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(920)
        clamp.set_child(self.box)
        self.set_child(clamp)

        self.load_grub_settings()

    def load_grub_settings(self):
        cfg = GrubManager.load_config()
        self.timeout_row.set_value(cfg.get('timeout', 5))
        self.default_row.set_text(str(cfg.get('default_entry', '0')))
        self.quiet_row.set_active(cfg.get('quiet', True))
        self.splash_row.set_active(cfg.get('splash', True))
        self.os_prober_row.set_active(cfg.get('os_prober', False))
        self.extra_args_row.set_text(cfg.get('custom_args', ''))

        backup_status = "Backup created" if cfg.get('has_backup') else "Automatic backup ready"
        self.status_row.set_subtitle(f"Config: /etc/default/grub • {backup_status}")

    def _on_save_clicked(self, _btn):
        options = {
            'timeout': int(self.timeout_row.get_value()),
            'default_entry': self.default_row.get_text().strip() or "0",
            'quiet': self.quiet_row.get_active(),
            'splash': self.splash_row.get_active(),
            'os_prober': self.os_prober_row.get_active(),
            'custom_args': self.extra_args_row.get_text().strip()
        }

        self.save_btn.set_sensitive(False)
        self.save_btn.set_label("Applying...")

        def worker():
            ok, msg = GrubManager.save_config(options)
            GLib.idle_add(self._on_operation_done, ok, msg)

        threading.Thread(target=worker, daemon=True).start()

    def _on_revert_clicked(self, _btn):
        dialog = Adw.AlertDialog.new(
            "Revert GRUB to Default?",
            "This will restore your original /etc/default/grub backup and re-run update-grub."
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("revert", "Revert")
        dialog.set_response_appearance("revert", Adw.ResponseAppearance.DESTRUCTIVE)

        def on_response(_dialog, result):
            res = dialog.choose_finish(result)
            if res == "revert":
                self.revert_btn.set_sensitive(False)

                def worker():
                    ok, msg = GrubManager.revert_to_default()
                    GLib.idle_add(self._on_operation_done, ok, msg)

                threading.Thread(target=worker, daemon=True).start()

        dialog.choose(self.get_root(), None, on_response)

    def _on_operation_done(self, ok: bool, msg: str):
        self.save_btn.set_sensitive(True)
        self.save_btn.set_label("Save & Apply GRUB")
        self.revert_btn.set_sensitive(True)
        if self.toast_overlay:
            self.toast_overlay.add_toast(Adw.Toast.new(msg))
        self.load_grub_settings()
