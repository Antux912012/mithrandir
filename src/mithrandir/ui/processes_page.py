import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib
from ..backend.processes import ProcessManager

class ProcessesPage(Gtk.ScrolledWindow):
    """
    Task manager / processes monitoring page inspired by Stacer.
    Lists active system processes, allows searching, sorting by CPU/RAM,
    and terminating or force-killing processes.
    """

    def __init__(self, toast_overlay: Adw.ToastOverlay = None):
        super().__init__()
        self.toast_overlay = toast_overlay
        self.set_vexpand(True)
        self.set_hexpand(True)
        self.set_kinetic_scrolling(True)

        self.all_processes = []
        self.filter_text = ""
        self.sort_key = "mem_mb"
        self.sort_reverse = True
        self.is_refreshing = False

        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        self.box.set_margin_top(24)
        self.box.set_margin_bottom(24)
        self.box.set_margin_start(24)
        self.box.set_margin_end(24)

        # Top controls header
        controls_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)

        # Search entry
        self.search_entry = Gtk.SearchEntry()
        self.search_entry.set_placeholder_text("Search by process name or PID...")
        self.search_entry.set_hexpand(True)
        self.search_entry.connect("search-changed", self._on_search_changed)
        controls_box.append(self.search_entry)

        # Sort dropdown
        sort_model = Gtk.StringList.new(["Sort: Memory", "Sort: CPU %", "Sort: Name", "Sort: PID"])
        self.sort_dropdown = Gtk.DropDown.new(sort_model, None)
        self.sort_dropdown.set_selected(0)
        self.sort_dropdown.connect("notify::selected", self._on_sort_changed)
        controls_box.append(self.sort_dropdown)

        # Refresh button
        self.refresh_btn = Gtk.Button()
        self.refresh_btn.set_icon_name("view-refresh-symbolic")
        self.refresh_btn.set_tooltip_text("Refresh process list")
        self.refresh_btn.connect("clicked", lambda _b: self.refresh_processes())
        controls_box.append(self.refresh_btn)

        self.box.append(controls_box)

        # Preferences Group / Container
        self.group = Adw.PreferencesGroup()
        self.group.set_title("Active Processes")
        self.group.set_description("Loading system processes...")
        self.box.append(self.group)

        self.list_box = Gtk.ListBox()
        self.list_box.add_css_class("boxed-list")
        self.list_box.set_selection_mode(Gtk.SelectionMode.NONE)
        self.group.add(self.list_box)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(960)
        clamp.set_child(self.box)
        self.set_child(clamp)

        # Initial load and 3-second auto-refresh
        self.refresh_processes()
        GLib.timeout_add_seconds(3, self._auto_refresh_tick)

    def _auto_refresh_tick(self) -> bool:
        if self.get_mapped():
            self.refresh_processes()
        return True

    def refresh_processes(self):
        if self.is_refreshing:
            return
        self.is_refreshing = True

        def worker():
            procs = ProcessManager.list_processes()
            GLib.idle_add(self._on_processes_loaded, procs)

        threading.Thread(target=worker, daemon=True).start()

    def _on_processes_loaded(self, procs):
        self.is_refreshing = False
        self.all_processes = procs
        self._apply_filter_and_render()

    def _on_search_changed(self, entry):
        self.filter_text = entry.get_text().strip().lower()
        self._apply_filter_and_render()

    def _on_sort_changed(self, dropdown, _param):
        idx = dropdown.get_selected()
        if idx == 0:
            self.sort_key = "mem_mb"
            self.sort_reverse = True
        elif idx == 1:
            self.sort_key = "cpu_percent"
            self.sort_reverse = True
        elif idx == 2:
            self.sort_key = "name"
            self.sort_reverse = False
        elif idx == 3:
            self.sort_key = "pid"
            self.sort_reverse = False
        self._apply_filter_and_render()

    def _apply_filter_and_render(self):
        # Filter
        filtered = []
        for p in self.all_processes:
            if not self.filter_text:
                filtered.append(p)
            elif (self.filter_text in p['name'].lower() or
                  str(p['pid']).startswith(self.filter_text) or
                  self.filter_text in p['user'].lower()):
                filtered.append(p)

        # Sort
        filtered.sort(key=lambda x: x.get(self.sort_key, 0), reverse=self.sort_reverse)

        self.group.set_description(f"Displaying {len(filtered)} of {len(self.all_processes)} processes")

        # Clear existing list items
        while True:
            row = self.list_box.get_row_at_index(0)
            if not row:
                break
            self.list_box.remove(row)

        # Show top 50 processes for ultra-smooth UI performance
        for p in filtered[:50]:
            row = Adw.ActionRow()
            row.set_title(GLib.markup_escape_text(f"{p['name']} (PID: {p['pid']})"))
            row.set_subtitle(GLib.markup_escape_text(f"User: {p['user']} • Status: {p['status']}"))

            # Right badges & buttons
            info_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
            info_box.set_valign(Gtk.Align.CENTER)

            # CPU pill
            cpu_badge = Gtk.Label(label=f"CPU: {p['cpu_percent']:.1f}%")
            cpu_badge.add_css_class("dim-label")
            info_box.append(cpu_badge)

            # RAM pill
            ram_badge = Gtk.Label(label=f"RAM: {p['mem_mb']:.1f} MB ({p['mem_percent']:.1f}%)")
            ram_badge.add_css_class("dim-label")
            info_box.append(ram_badge)

            # Terminate button
            term_btn = Gtk.Button(label="End")
            term_btn.add_css_class("flat")
            term_btn.set_tooltip_text(f"Terminate process {p['name']} (SIGTERM)")
            term_btn.connect("clicked", lambda _b, pid=p['pid'], name=p['name']: self._on_kill_clicked(pid, name, force=False))
            info_box.append(term_btn)

            # Force Kill button
            kill_btn = Gtk.Button()
            kill_btn.set_icon_name("process-stop-symbolic")
            kill_btn.add_css_class("destructive-action")
            kill_btn.set_tooltip_text(f"Force Kill {p['name']} (SIGKILL)")
            kill_btn.connect("clicked", lambda _b, pid=p['pid'], name=p['name']: self._on_kill_clicked(pid, name, force=True))
            info_box.append(kill_btn)

            row.add_suffix(info_box)
            self.list_box.append(row)

    def _on_kill_clicked(self, pid: int, name: str, force: bool):
        action_name = "Force Kill" if force else "End"
        dialog = Adw.AlertDialog.new(
            f"{action_name} Process?",
            f"Are you sure you want to terminate '{name}' (PID: {pid})?"
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("confirm", action_name)
        if force:
            dialog.set_response_appearance("confirm", Adw.ResponseAppearance.DESTRUCTIVE)

        def on_response(_dialog, result):
            res = dialog.choose_finish(result)
            if res == "confirm":
                ok, msg = ProcessManager.kill_process(pid, force=force)
                if self.toast_overlay:
                    self.toast_overlay.add_toast(Adw.Toast.new(msg))
                self.refresh_processes()

        dialog.choose(self.get_root(), None, on_response)
