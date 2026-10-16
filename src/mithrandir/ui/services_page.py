import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib
from ..backend.services import ServiceManager, ServiceInfo

class ServicesPage(Gtk.ScrolledWindow):
    """
    Lists systemd services, indicators for critical vs non-critical services,
    and actions to start, stop, and restart services with pkexec elevation.
    """

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

        header_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        header_box.set_halign(Gtk.Align.END)

        self.search_entry = Gtk.SearchEntry()
        self.search_entry.set_placeholder_text("Search services...")
        self.search_entry.connect("search-changed", self._on_search_changed)
        header_box.append(self.search_entry)

        self.refresh_btn = Gtk.Button(label="Refresh Services")
        self.refresh_btn.connect("clicked", self._on_refresh_clicked)
        header_box.append(self.refresh_btn)
        self.box.append(header_box)

        self.group = Adw.PreferencesGroup()
        self.group.set_title("System Services")
        self.group.set_description("Monitor and manage systemd services on the host system")
        self.box.append(self.group)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(880)
        clamp.set_child(self.box)
        self.set_child(clamp)

        self.all_services = []
        self._load_services()

    def _on_search_changed(self, entry):
        text = entry.get_text().strip().lower()
        self._render_rows(text)

    def _on_refresh_clicked(self, _btn):
        self._load_services()
        toast = Adw.Toast.new("Services refreshed")
        self.toast_overlay.add_toast(toast)

    def _load_services(self):
        self.refresh_btn.set_sensitive(False)

        def worker():
            services = ServiceManager.list_services()
            GLib.idle_add(self._on_load_complete, services)

        threading.Thread(target=worker, daemon=True).start()

    def _on_load_complete(self, services):
        self.refresh_btn.set_sensitive(True)
        self.all_services = services
        self._render_rows(self.search_entry.get_text().strip().lower())

    def _render_rows(self, filter_text: str):
        # Re-create preferences group for clean list update
        self.box.remove(self.group)
        self.group = Adw.PreferencesGroup()
        self.group.set_title("System Services")
        self.group.set_description("Monitor and manage systemd services on the host system")
        self.box.append(self.group)

        filtered = [
            s for s in self.all_services
            if not filter_text or filter_text in s.name.lower()
        ][:60]  # Display top 60 matches for snappy rendering

        if not filtered:
            row = Adw.ActionRow(title="No matching services found")
            self.group.add(row)
            return

        for svc in filtered:
            expander = Adw.ExpanderRow()
            expander.set_title(GLib.markup_escape_text(svc.name))

            # Subtitle with status & critical badge
            crit_badge = "[Essential System Service]" if svc.is_critical else "[Optional / User Service]"
            expander.set_subtitle(GLib.markup_escape_text(f"Status: {svc.active_state} ({svc.sub_state}) • {crit_badge}"))

            # Icon
            is_active = (svc.active_state == "active")
            icon_name = "emblem-default-symbolic" if is_active else "media-playback-stop-symbolic"
            icon = Gtk.Image.new_from_icon_name(icon_name)
            expander.add_prefix(icon)

            # Action buttons row inside expander
            btn_row = Adw.ActionRow(title="Service Actions")

            actions_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
            actions_box.set_valign(Gtk.Align.CENTER)

            if is_active:
                stop_btn = Gtk.Button(label="Stop")
                stop_btn.add_css_class("destructive-action")
                stop_btn.connect("clicked", self._create_service_action(svc.name, "stop"))
                actions_box.append(stop_btn)

                restart_btn = Gtk.Button(label="Restart")
                restart_btn.connect("clicked", self._create_service_action(svc.name, "restart"))
                actions_box.append(restart_btn)
            else:
                start_btn = Gtk.Button(label="Start")
                start_btn.add_css_class("suggested-action")
                start_btn.connect("clicked", self._create_service_action(svc.name, "start"))
                actions_box.append(start_btn)

            btn_row.add_suffix(actions_box)
            expander.add_row(btn_row)

            self.group.add(expander)

    def _create_service_action(self, service_name: str, action: str):
        def handler(_btn):
            if ServiceManager.is_critical(service_name) and action == "stop":
                dialog = Adw.AlertDialog.new(
                    "Warning: Critical Service",
                    f"{service_name} is marked as an essential system service. Stopping it might disrupt network or system stability. Proceed?"
                )
                dialog.add_response("cancel", "Cancel")
                dialog.add_response("stop", "Stop Anyway")
                dialog.set_response_appearance("stop", Adw.ResponseAppearance.DESTRUCTIVE)
                dialog.choose(self.get_root(), None, lambda d, r: self._on_critical_confirm(d, r, service_name, action))
            else:
                self._execute_service_action(service_name, action)
        return handler

    def _on_critical_confirm(self, dialog, result, service_name, action):
        res = dialog.choose_finish(result)
        if res == "stop":
            self._execute_service_action(service_name, action)

    def _execute_service_action(self, service_name: str, action: str):
        toast = Adw.Toast.new(f"Performing '{action}' on {service_name}...")
        self.toast_overlay.add_toast(toast)

        def worker():
            if action == "start":
                ok, err = ServiceManager.start_service(service_name)
            elif action == "stop":
                ok, err = ServiceManager.stop_service(service_name)
            elif action == "restart":
                ok, err = ServiceManager.restart_service(service_name)
            else:
                ok, err = False, "Unknown action"

            GLib.idle_add(self._on_action_complete, service_name, action, ok, err)

        threading.Thread(target=worker, daemon=True).start()

    def _on_action_complete(self, service_name, action, ok, err):
        msg = f"{action.capitalize()} {service_name}: {'Success' if ok else 'Failed'}"
        toast = Adw.Toast.new(msg)
        self.toast_overlay.add_toast(toast)
        self._load_services()
