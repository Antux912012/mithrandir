import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib
from ..backend.cleaner import CleanerEngine, CleanerItem

class CleanerPage(Gtk.ScrolledWindow):
    """
    BleachBit and Stacer-style system cleaning page with dry-run scan,
    package cache, Flatpaks, and Snaps cleaning.
    """

    CLEAN_ITEMS = [
        ('user_cache', 'User Application Cache', 'Temporary application cache files in ~/.cache'),
        ('flatpak_cache', 'Flatpak Applications Cache', 'Application caches stored in ~/.var/app'),
        ('snap_cache', 'Snap Packages Cache and Old Revisions', 'User caches in ~/snap and old disabled revisions'),
        ('stremio_cache', 'Stremio Torrent and Movie Cache', 'Cached torrent chunks and video streaming buffer'),
        ('thumbnails', 'Thumbnail Cache', 'Cached image and video preview thumbnails'),
        ('trash', 'Trash Bin', 'Deleted files currently in user trash bin'),
        ('systemd_journal', 'Systemd Journal Logs', 'Archived system logs vacuumed to 20 MB'),
        ('package_cache', 'Package Manager Cache (.deb / .rpm)', 'Downloaded APT, DNF, or Pacman package archives'),
        ('flatpak_unused', 'Unused Flatpak Runtimes', 'Old runtimes no longer required by any Flatpak')
    ]

    def __init__(self, toast_overlay: Adw.ToastOverlay):
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

        # Header action buttons
        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.END)

        self.scan_btn = Gtk.Button(label="Scan System")
        self.scan_btn.add_css_class("suggested-action")
        self.scan_btn.connect("clicked", self._on_scan_clicked)
        btn_box.append(self.scan_btn)

        self.clean_btn = Gtk.Button(label="Clean Selected")
        self.clean_btn.add_css_class("destructive-action")
        self.clean_btn.set_sensitive(False)
        self.clean_btn.connect("clicked", self._on_clean_clicked)
        btn_box.append(self.clean_btn)

        self.box.append(btn_box)

        # Cleaner Items Group
        self.group = Adw.PreferencesGroup()
        self.group.set_title("Cleaning Categories")
        self.group.set_description("Select items to scan and safely clean across System, Flatpaks, and Snaps")
        self.box.append(self.group)

        self.rows = {}
        for item_id, title, subtitle in self.CLEAN_ITEMS:
            row = Adw.ActionRow()
            row.set_title(GLib.markup_escape_text(title))
            row.set_subtitle(GLib.markup_escape_text(subtitle))
            check = Gtk.CheckButton()
            check.set_active(True)
            check.set_valign(Gtk.Align.CENTER)
            row.add_prefix(check)

            size_label = Gtk.Label(label="Not scanned")
            size_label.add_css_class("dim-label")
            size_label.set_valign(Gtk.Align.CENTER)
            row.add_suffix(size_label)

            self.rows[item_id] = {
                'row': row,
                'check': check,
                'label': size_label,
                'item_data': None
            }
            self.group.add(row)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(840)
        clamp.set_child(self.box)
        self.set_child(clamp)

    def _on_scan_clicked(self, _btn):
        self.scan_btn.set_sensitive(False)
        self.clean_btn.set_sensitive(False)
        toast = Adw.Toast.new("Scanning cleaning targets...")
        self.toast_overlay.add_toast(toast)

        def worker():
            results = {}
            for item in self.CLEAN_ITEMS:
                item_id = item[0]
                res_item = CleanerEngine.scan_item(item_id)
                results[item_id] = res_item
            GLib.idle_add(self._on_scan_complete, results)

        threading.Thread(target=worker, daemon=True).start()

    def _on_scan_complete(self, results):
        self.scan_btn.set_sensitive(True)
        self.clean_btn.set_sensitive(True)

        total_bytes = 0
        for item_id, item in results.items():
            self.rows[item_id]['item_data'] = item
            size_mb = item.size_bytes / (1024 * 1024)
            if size_mb >= 1024:
                size_str = f"{size_mb / 1024:.2f} GB"
            elif size_mb > 0:
                size_str = f"{size_mb:.1f} MB"
            else:
                size_str = "Clean"
            self.rows[item_id]['label'].set_text(size_str)
            if self.rows[item_id]['check'].get_active():
                total_bytes += item.size_bytes

        total_mb = total_bytes / (1024 * 1024)
        toast = Adw.Toast.new(f"Scan complete. Up to {total_mb:.1f} MB can be cleaned.")
        self.toast_overlay.add_toast(toast)

    def _on_clean_clicked(self, _btn):
        dialog = Adw.AlertDialog.new(
            "Clean Selected Items?",
            "This will permanently remove caches, logs, unused Flatpaks, or old Snap revisions according to your selections."
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("clean", "Clean")
        dialog.set_response_appearance("clean", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.choose(self.get_root(), None, self._on_dialog_response)

    def _on_dialog_response(self, dialog, result):
        response = dialog.choose_finish(result)
        if response == "clean":
            self._execute_clean()

    def _execute_clean(self):
        self.clean_btn.set_sensitive(False)
        selected = [item_id for item_id, d in self.rows.items() if d['check'].get_active()]

        def worker():
            cleaned_count = 0
            for item_id in selected:
                success, _ = CleanerEngine.clean_item(item_id)
                if success:
                    cleaned_count += 1
            GLib.idle_add(self._on_clean_complete, cleaned_count)

        threading.Thread(target=worker, daemon=True).start()

    def _on_clean_complete(self, cleaned_count):
        self.clean_btn.set_sensitive(True)
        toast = Adw.Toast.new(f"Cleaning complete! Processed {cleaned_count} targets.")
        self.toast_overlay.add_toast(toast)
        self._on_scan_clicked(None)
