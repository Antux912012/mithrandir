import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib
from ..backend.autostart import AutostartManager

class AutostartPage(Gtk.ScrolledWindow):
    """
    Startup applications management page inspired by Stacer.
    Lists and toggles startup applications in ~/.config/autostart and /etc/xdg/autostart.
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

        # 1. Mithrandir Autostart Group
        m_group = Adw.PreferencesGroup()
        m_group.set_title("Mithrandir Background Launch")
        m_group.set_description("Keep Mithrandir protecting and optimizing your system on boot")

        self.mithrandir_row = Adw.SwitchRow(title="Start Mithrandir Automatically on Login")
        self.mithrandir_row.set_subtitle("Launches in the background to monitor RAM, cache clutter, and sensors")
        self.mithrandir_row.set_active(AutostartManager.is_mithrandir_autostart_enabled())
        self.mithrandir_row.connect("notify::active", self._on_mithrandir_autostart_toggled)
        m_group.add(self.mithrandir_row)
        self.box.append(m_group)

        # 2. System and User Startup Apps
        apps_header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        apps_header.set_valign(Gtk.Align.CENTER)

        title_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        title_box.set_hexpand(True)
        lbl = Gtk.Label(label="Startup Applications")
        lbl.add_css_class("title-3")
        lbl.set_xalign(0)
        title_box.append(lbl)

        sub_lbl = Gtk.Label(label="Manage programs and services configured to launch automatically")
        sub_lbl.add_css_class("caption")
        sub_lbl.add_css_class("dim-label")
        sub_lbl.set_xalign(0)
        title_box.append(sub_lbl)
        apps_header.append(title_box)

        add_btn = Gtk.Button(label="Add Startup App")
        add_btn.set_icon_name("list-add-symbolic")
        add_btn.add_css_class("suggested-action")
        add_btn.connect("clicked", self._on_add_clicked)
        apps_header.append(add_btn)

        self.box.append(apps_header)

        self.apps_group = Adw.PreferencesGroup()
        self.box.append(self.apps_group)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(920)
        clamp.set_child(self.box)
        self.set_child(clamp)

        self.refresh_entries()

    def refresh_entries(self):
        # Clear existing rows
        # Remove and recreate group child
        self.box.remove(self.apps_group)
        self.apps_group = Adw.PreferencesGroup()
        self.box.append(self.apps_group)

        entries = AutostartManager.list_entries()
        if not entries:
            empty_row = Adw.ActionRow(title="No startup applications detected.")
            self.apps_group.add(empty_row)
            return

        for e in entries:
            row = Adw.ActionRow()
            row.set_title(e['name'])
            sub = e['command'] if len(e['command']) <= 60 else e['command'][:60] + "..."
            if e['comment']:
                sub = f"{e['comment']} ({sub})"
            row.set_subtitle(sub)

            # Icon
            icon = Gtk.Image.new_from_icon_name(e.get('icon') or "application-x-executable-symbolic")
            icon.set_pixel_size(24)
            row.add_prefix(icon)

            # Toggle switch
            switch = Gtk.Switch()
            switch.set_active(e['enabled'])
            switch.set_valign(Gtk.Align.CENTER)
            switch.connect("state-set", lambda _s, state, eid=e['id']: self._on_entry_state_set(eid, state))
            row.add_suffix(switch)

            # Delete button
            del_btn = Gtk.Button()
            del_btn.set_icon_name("user-trash-symbolic")
            del_btn.set_valign(Gtk.Align.CENTER)
            del_btn.add_css_class("flat")
            del_btn.set_tooltip_text("Remove startup application")
            del_btn.connect("clicked", lambda _b, eid=e['id'], name=e['name']: self._on_delete_clicked(eid, name))
            row.add_suffix(del_btn)

            self.apps_group.add(row)

    def _on_entry_state_set(self, entry_id: str, state: bool) -> bool:
        ok = AutostartManager.toggle_entry(entry_id, state)
        if ok and self.toast_overlay:
            status = "enabled" if state else "disabled"
            self.toast_overlay.add_toast(Adw.Toast.new(f"Startup application {status}."))
        return False

    def _on_mithrandir_autostart_toggled(self, row, _param):
        active = row.get_active()
        ok = AutostartManager.set_mithrandir_autostart(active)
        if ok and self.toast_overlay:
            msg = "Mithrandir autostart enabled." if active else "Mithrandir autostart disabled."
            self.toast_overlay.add_toast(Adw.Toast.new(msg))

    def _on_delete_clicked(self, entry_id: str, name: str):
        dialog = Adw.AlertDialog.new(
            "Remove Startup Application?",
            f"Are you sure you want to remove '{name}' from startup apps?"
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("delete", "Remove")
        dialog.set_response_appearance("delete", Adw.ResponseAppearance.DESTRUCTIVE)

        def on_response(_dialog, result):
            res = dialog.choose_finish(result)
            if res == "delete":
                ok = AutostartManager.remove_entry(entry_id)
                if ok and self.toast_overlay:
                    self.toast_overlay.add_toast(Adw.Toast.new(f"Removed '{name}'."))
                self.refresh_entries()

        dialog.choose(self.get_root(), None, on_response)

    def _on_add_clicked(self, _btn):
        dialog = Adw.PreferencesWindow()
        dialog.set_title("Add Startup Application")
        dialog.set_transient_for(self.get_root())
        dialog.set_modal(True)
        dialog.set_default_size(480, 360)

        page = Adw.PreferencesPage()
        group = Adw.PreferencesGroup()
        group.set_title("Application Details")

        name_row = Adw.EntryRow(title="Application Name")
        cmd_row = Adw.EntryRow(title="Command (Exec)")
        desc_row = Adw.EntryRow(title="Description / Comment (optional)")

        group.add(name_row)
        group.add(cmd_row)
        group.add(desc_row)

        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.END)
        btn_box.set_margin_top(16)

        save_btn = Gtk.Button(label="Save")
        save_btn.add_css_class("suggested-action")

        def on_save(_b):
            name = name_row.get_text().strip()
            cmd = cmd_row.get_text().strip()
            desc = desc_row.get_text().strip()
            if not name or not cmd:
                return
            ok = AutostartManager.add_entry(name, cmd, desc)
            dialog.close()
            if ok and self.toast_overlay:
                self.toast_overlay.add_toast(Adw.Toast.new(f"Added '{name}' to startup."))
            self.refresh_entries()

        save_btn.connect("clicked", on_save)
        btn_box.append(save_btn)

        group.add(btn_box)
        page.add(group)
        dialog.add(page)
        dialog.present()
