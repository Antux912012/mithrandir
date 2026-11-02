import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib
from ..backend.btrfs import BtrfsManager

class BtrfsPage(Gtk.ScrolledWindow):
    """
    BTRFS Snapshot Management and Cleaning Page.
    Handles listing subvolume snapshots, creating new snapshots,
    and automated cleanup rules for BTRFS filesystems.
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

        # 1. BTRFS Filesystem Status Banner
        self.status_group = Adw.PreferencesGroup()
        self.status_group.set_title("Filesystem Architecture and Snapshots")
        self.box.append(self.status_group)

        self.status_row = Adw.ActionRow(title="Filesystem Detection")
        self.status_group.add(self.status_row)

        # 2. Action buttons header
        self.actions_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.actions_box.set_halign(Gtk.Align.END)

        self.create_btn = Gtk.Button(label="Create Snapshot")
        self.create_btn.set_icon_name("drive-harddisk-symbolic")
        self.create_btn.add_css_class("suggested-action")
        self.create_btn.connect("clicked", self._on_create_clicked)
        self.actions_box.append(self.create_btn)

        self.clean_btn = Gtk.Button(label="Clean Old Snapshots")
        self.clean_btn.set_icon_name("user-trash-symbolic")
        self.clean_btn.add_css_class("destructive-action")
        self.clean_btn.connect("clicked", self._on_clean_clicked)
        self.actions_box.append(self.clean_btn)

        self.box.append(self.actions_box)

        # 3. Snapshot List Group
        self.list_group = Adw.PreferencesGroup()
        self.list_group.set_title("BTRFS Snapshots")
        self.box.append(self.list_group)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(920)
        clamp.set_child(self.box)
        self.set_child(clamp)

        self.refresh_snapshots()

    def refresh_snapshots(self):
        status = BtrfsManager.get_btrfs_status()

        if status['is_btrfs']:
            self.status_row.set_title("BTRFS Filesystem Active")
            self.status_row.set_subtitle(f"Root source: {status['root_source']} • Tools: {'btrfs-progs ' if status['has_btrfs_tool'] else ''}{'snapper' if status['has_snapper'] else ''}")
            self.create_btn.set_sensitive(True)
            self.clean_btn.set_sensitive(True)
            self._render_btrfs_snapshots()
        else:
            self.status_row.set_title(f"Root Filesystem: {status['root_fstype'].upper()}")
            self.status_row.set_subtitle(f"Active mount '{status['root_source']}' is formatted with {status['root_fstype']}.")
            self.create_btn.set_sensitive(False)
            self.clean_btn.set_sensitive(False)
            self._render_non_btrfs_guide(status)

    def _render_non_btrfs_guide(self, status):
        # Clear list group
        self.box.remove(self.list_group)
        self.list_group = Adw.PreferencesGroup()
        self.list_group.set_title("BTRFS Snapshot Information")
        self.list_group.set_description("Snapshots require copy-on-write partitions (BTRFS).")
        self.box.append(self.list_group)

        card = Adw.ActionRow(
            title="Instantaneous Subvolume Snapshots",
            subtitle="BTRFS subvolume snapshots provide atomic, zero-copy system restore points. Once a partition is formatted with BTRFS (common in Fedora, openSUSE, Arch, and custom Ubuntu setups), Mithrandir enables automated snapshot rollbacks, retention policies, and space reclamation."
        )
        card.add_prefix(Gtk.Image.new_from_icon_name("drive-harddisk-symbolic"))
        self.list_group.add(card)

    def _render_btrfs_snapshots(self):
        self.box.remove(self.list_group)
        self.list_group = Adw.PreferencesGroup()
        self.list_group.set_title("BTRFS Snapshots")
        self.box.append(self.list_group)

        def worker():
            snapshots = BtrfsManager.list_snapshots()
            GLib.idle_add(self._on_snapshots_loaded, snapshots)

        threading.Thread(target=worker, daemon=True).start()

    def _on_snapshots_loaded(self, snapshots):
        if not snapshots:
            empty_row = Adw.ActionRow(title="No BTRFS snapshots found.")
            empty_row.set_subtitle("Use 'Create Snapshot' above to create your first system restore point.")
            self.list_group.add(empty_row)
            return

        self.list_group.set_description(f"Found {len(snapshots)} snapshots")
        for s in snapshots:
            row = Adw.ActionRow()
            row.set_title(GLib.markup_escape_text(s['name']))
            sub = f"ID: {s['id']} • Path: {s['path']}"
            if 'date' in s:
                sub = f"{s['date']} • {sub}"
            row.set_subtitle(GLib.markup_escape_text(sub))

            icon = Gtk.Image.new_from_icon_name("drive-harddisk-symbolic")
            row.add_prefix(icon)

            del_btn = Gtk.Button()
            del_btn.set_icon_name("user-trash-symbolic")
            del_btn.add_css_class("destructive-action")
            del_btn.set_valign(Gtk.Align.CENTER)
            del_btn.set_tooltip_text("Delete snapshot")
            del_btn.connect("clicked", lambda _b, path=s['path'], name=s['name']: self._on_delete_snapshot(path, name))
            row.add_suffix(del_btn)

            self.list_group.add(row)

    def _on_create_clicked(self, _btn):
        dialog = Adw.PreferencesWindow()
        dialog.set_title("Create BTRFS Snapshot")
        dialog.set_transient_for(self.get_root())
        dialog.set_modal(True)
        dialog.set_default_size(440, 280)

        page = Adw.PreferencesPage()
        group = Adw.PreferencesGroup()
        group.set_title("Snapshot Options")

        name_row = Adw.EntryRow(title="Snapshot Name / Description")
        ro_row = Adw.SwitchRow(title="Read-Only Snapshot")
        ro_row.set_subtitle("Protects restore point against modification")
        ro_row.set_active(True)

        group.add(name_row)
        group.add(ro_row)

        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.END)
        btn_box.set_margin_top(16)

        save_btn = Gtk.Button(label="Create")
        save_btn.add_css_class("suggested-action")

        def on_create(_b):
            name = name_row.get_text().strip() or "system_snapshot"
            ro = ro_row.get_active()
            dialog.close()

            def worker():
                ok, msg = BtrfsManager.create_snapshot(name, read_only=ro)
                GLib.idle_add(self._on_operation_done, ok, msg)

            threading.Thread(target=worker, daemon=True).start()

        save_btn.connect("clicked", on_create)
        btn_box.append(save_btn)
        group.add(btn_box)

        page.add(group)
        dialog.add(page)
        dialog.present()

    def _on_clean_clicked(self, _btn):
        dialog = Adw.AlertDialog.new(
            "Clean Old Snapshots?",
            "This will keep the 5 most recent snapshots and delete older snapshot subvolumes."
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("clean", "Clean")
        dialog.set_response_appearance("clean", Adw.ResponseAppearance.DESTRUCTIVE)

        def on_response(_dialog, result):
            res = dialog.choose_finish(result)
            if res == "clean":
                def worker():
                    count, msg = BtrfsManager.clean_old_snapshots(keep_latest_count=5)
                    GLib.idle_add(self._on_operation_done, True, msg)

                threading.Thread(target=worker, daemon=True).start()

        dialog.choose(self.get_root(), None, on_response)

    def _on_delete_snapshot(self, path: str, name: str):
        dialog = Adw.AlertDialog.new(
            "Delete Snapshot?",
            f"Are you sure you want to permanently delete snapshot '{name}'?"
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("delete", "Delete")
        dialog.set_response_appearance("delete", Adw.ResponseAppearance.DESTRUCTIVE)

        def on_response(_dialog, result):
            res = dialog.choose_finish(result)
            if res == "delete":
                def worker():
                    ok, msg = BtrfsManager.delete_snapshot(path)
                    GLib.idle_add(self._on_operation_done, ok, msg)

                threading.Thread(target=worker, daemon=True).start()

        dialog.choose(self.get_root(), None, on_response)

    def _on_operation_done(self, ok: bool, msg: str):
        if self.toast_overlay:
            self.toast_overlay.add_toast(Adw.Toast.new(msg))
        self.refresh_snapshots()
