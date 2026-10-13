import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib
from ..backend.packages import PackageManager, PackageItem

class UninstallerPage(Gtk.ScrolledWindow):
    """
    Uninstaller and application cleaner page supporting:
    - Native (.deb / .rpm)
    - Flatpaks
    - Snaps
    - Python packages and CLI tools (pip / uv)
    Features dedicated filter buttons and per-package update/uninstall controls.
    """

    def __init__(self, toast_overlay: Adw.ToastOverlay):
        super().__init__()
        self.toast_overlay = toast_overlay
        self.set_vexpand(True)
        self.set_hexpand(True)
        self.set_kinetic_scrolling(True)

        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=20)
        self.box.set_margin_top(24)
        self.box.set_margin_bottom(24)
        self.box.set_margin_start(24)
        self.box.set_margin_end(24)

        # Filter Section: 4 Toggle Buttons for Native, Flatpak, Snap, Python (pip / uv)
        filter_section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)

        filter_bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        filter_bar.set_halign(Gtk.Align.CENTER)

        # Native Button
        self.btn_native = Gtk.ToggleButton(label="Native (.deb / .rpm)")
        self.btn_native.set_active(True)
        self.btn_native.add_css_class("pill")
        self.btn_native.connect("toggled", self._on_filter_toggled, "native")
        filter_bar.append(self.btn_native)

        # Flatpak Button
        self.btn_flatpak = Gtk.ToggleButton(label="Flatpak")
        self.btn_flatpak.add_css_class("pill")
        self.btn_flatpak.connect("toggled", self._on_filter_toggled, "flatpak")
        filter_bar.append(self.btn_flatpak)

        # Snap Button
        self.btn_snap = Gtk.ToggleButton(label="Snap")
        self.btn_snap.add_css_class("pill")
        self.btn_snap.connect("toggled", self._on_filter_toggled, "snap")
        filter_bar.append(self.btn_snap)

        # Python Button (pip / uv)
        self.btn_python = Gtk.ToggleButton(label="Python (pip / uv)")
        self.btn_python.add_css_class("pill")
        self.btn_python.connect("toggled", self._on_filter_toggled, "python")
        filter_bar.append(self.btn_python)

        filter_section.append(filter_bar)

        # Python Sub-Filter Bar (All / pip / uv)
        self.subfilter_bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.subfilter_bar.set_halign(Gtk.Align.CENTER)
        self.subfilter_bar.set_visible(False)

        self.btn_sub_all = Gtk.ToggleButton(label="All")
        self.btn_sub_all.set_active(True)
        self.btn_sub_all.add_css_class("pill")
        self.btn_sub_all.connect("toggled", self._on_subfilter_toggled, "all")
        self.subfilter_bar.append(self.btn_sub_all)

        self.btn_sub_pip = Gtk.ToggleButton(label="pip")
        self.btn_sub_pip.add_css_class("pill")
        self.btn_sub_pip.connect("toggled", self._on_subfilter_toggled, "pip")
        self.subfilter_bar.append(self.btn_sub_pip)

        self.btn_sub_uv = Gtk.ToggleButton(label="uv")
        self.btn_sub_uv.add_css_class("pill")
        self.btn_sub_uv.connect("toggled", self._on_subfilter_toggled, "uv")
        self.subfilter_bar.append(self.btn_sub_uv)

        filter_section.append(self.subfilter_bar)
        self.box.append(filter_section)

        # Search & Actions Row
        search_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        search_box.set_halign(Gtk.Align.FILL)

        self.search_entry = Gtk.SearchEntry()
        self.search_entry.set_hexpand(True)
        self.search_entry.set_placeholder_text("Search packages in selected category...")
        self.search_entry.connect("search-changed", self._on_search_changed)
        search_box.append(self.search_entry)

        # Install Python Package button (revealed when Python tab active)
        self.install_btn = Gtk.Button(label="Install Package")
        self.install_btn.set_icon_name("list-add-symbolic")
        self.install_btn.set_tooltip_text("Install a Python package via pip or uv")
        self.install_btn.connect("clicked", self._on_install_clicked)
        self.install_btn.set_visible(False)
        search_box.append(self.install_btn)

        self.refresh_btn = Gtk.Button(label="Refresh")
        self.refresh_btn.set_icon_name("view-refresh-symbolic")
        self.refresh_btn.connect("clicked", self._on_refresh_clicked)
        search_box.append(self.refresh_btn)
        self.box.append(search_box)

        # Package List Group
        self.group = Adw.PreferencesGroup()
        self.group.set_title("Installed Software")
        self.group.set_description("Select an application to uninstall, update, or purge cached data")
        self.box.append(self.group)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(880)
        clamp.set_child(self.box)
        self.set_child(clamp)

        self.active_filter = "native"
        self.active_subfilter = "all"
        self.all_packages = []
        self._load_packages()

    def _on_filter_toggled(self, button, kind: str):
        if button.get_active():
            # Radio behavior among main filters
            if kind == "native":
                self.btn_flatpak.set_active(False)
                self.btn_snap.set_active(False)
                self.btn_python.set_active(False)
            elif kind == "flatpak":
                self.btn_native.set_active(False)
                self.btn_snap.set_active(False)
                self.btn_python.set_active(False)
            elif kind == "snap":
                self.btn_native.set_active(False)
                self.btn_flatpak.set_active(False)
                self.btn_python.set_active(False)
            elif kind == "python":
                self.btn_native.set_active(False)
                self.btn_flatpak.set_active(False)
                self.btn_snap.set_active(False)

            self.active_filter = kind
            is_py = (kind == "python")
            self.subfilter_bar.set_visible(is_py)
            self.install_btn.set_visible(is_py)
            self._render_rows(self.search_entry.get_text().strip().lower())
        else:
            # Prevent all 4 being unselected
            if (not self.btn_native.get_active() and
                not self.btn_flatpak.get_active() and
                not self.btn_snap.get_active() and
                not self.btn_python.get_active()):
                button.set_active(True)

    def _on_subfilter_toggled(self, button, sub_kind: str):
        if button.get_active():
            if sub_kind == "all":
                self.btn_sub_pip.set_active(False)
                self.btn_sub_uv.set_active(False)
            elif sub_kind == "pip":
                self.btn_sub_all.set_active(False)
                self.btn_sub_uv.set_active(False)
            elif sub_kind == "uv":
                self.btn_sub_all.set_active(False)
                self.btn_sub_pip.set_active(False)
            self.active_subfilter = sub_kind
            self._render_rows(self.search_entry.get_text().strip().lower())
        else:
            if not self.btn_sub_all.get_active() and not self.btn_sub_pip.get_active() and not self.btn_sub_uv.get_active():
                button.set_active(True)

    def _on_search_changed(self, entry):
        self._render_rows(entry.get_text().strip().lower())

    def _on_refresh_clicked(self, _btn):
        self._load_packages()
        toast = Adw.Toast.new("Refreshing installed packages...")
        self.toast_overlay.add_toast(toast)

    def _load_packages(self):
        self.refresh_btn.set_sensitive(False)

        def worker():
            pkgs = PackageManager.list_packages()
            GLib.idle_add(self._on_load_complete, pkgs)

        threading.Thread(target=worker, daemon=True).start()

    def _on_load_complete(self, pkgs):
        self.refresh_btn.set_sensitive(True)
        self.all_packages = pkgs
        self._render_rows(self.search_entry.get_text().strip().lower())

    def _render_rows(self, search_text: str):
        self.box.remove(self.group)
        self.group = Adw.PreferencesGroup()

        filter_titles = {
            "native": "Native Packages (.deb / .rpm)",
            "flatpak": "Flatpak Applications",
            "snap": "Snap Packages",
            "python": "Python Packages (pip / uv)"
        }
        self.group.set_title(filter_titles.get(self.active_filter, "Installed Software"))

        if self.active_filter == "python":
            self.group.set_description("Manage, update, and remove Python libraries and tools")
        else:
            self.group.set_description(f"Manage applications and dependencies for {self.active_filter.capitalize()}")

        self.box.append(self.group)

        # Filter by kind and subfilter
        matched = []
        for p in self.all_packages:
            if self.active_filter == "native" and "native" in p.kind:
                matched.append(p)
            elif self.active_filter == "flatpak" and "flatpak" in p.kind:
                matched.append(p)
            elif self.active_filter == "snap" and "snap" in p.kind:
                matched.append(p)
            elif self.active_filter == "python" and "python" in p.kind:
                if self.active_subfilter == "pip":
                    if "pip" not in p.kind and getattr(p, 'manager', '') != 'pip':
                        continue
                elif self.active_subfilter == "uv":
                    if "uv" not in p.kind and getattr(p, 'manager', '') not in ['uv', 'uv-tool']:
                        continue
                matched.append(p)

        # Filter by search
        filtered = [
            p for p in matched
            if not search_text or search_text in p.name.lower() or search_text in p.summary.lower()
        ]

        if not filtered:
            if self.active_filter == "python" and self.active_subfilter == "uv" and not PackageManager.find_uv_executable():
                row = Adw.ActionRow(title="uv package manager not detected on system")
                row.set_subtitle("Install uv with Astral's installer or manage packages via pip")
                row.add_prefix(Gtk.Image.new_from_icon_name("dialog-information-symbolic"))
                self.group.add(row)
                return

            row = Adw.ActionRow(title=f"No {self.active_filter.capitalize()} packages found")
            self.group.add(row)
            return

        display_limit = 100
        display_list = filtered[:display_limit]

        for pkg in display_list:
            row = Adw.ActionRow(title=GLib.markup_escape_text(pkg.name))
            sub = f"v{pkg.version}" if pkg.version else ""
            if 'python' in pkg.kind:
                tag = getattr(pkg, 'manager', '') or pkg.kind.replace('python', '').strip(' ()')
                tag_str = f" • {tag}" if tag else ""
                sub_detail = f"{sub}{tag_str}" if sub else tag_str.lstrip(" • ")
                if pkg.summary:
                    sub = f"{sub_detail} — {pkg.summary}" if sub_detail else pkg.summary
                else:
                    sub = sub_detail
            else:
                if pkg.summary:
                    sub = f"{sub} — {pkg.summary[:45]}" if sub else pkg.summary[:45]

            row.set_subtitle(GLib.markup_escape_text(sub or "Installed package"))

            # Determine icon
            if 'python' in pkg.kind:
                icon_name = "python3"
            elif 'flatpak' in pkg.kind:
                icon_name = "package-x-generic-symbolic"
            elif 'snap' in pkg.kind:
                icon_name = "application-x-addon-symbolic"
            else:
                icon_name = "system-software-install-symbolic"

            row.add_prefix(Gtk.Image.new_from_icon_name(icon_name))

            btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
            btn_box.set_valign(Gtk.Align.CENTER)

            if 'python' in pkg.kind:
                # Update button
                update_btn = Gtk.Button(label="Update")
                update_btn.set_tooltip_text(f"Update {pkg.name} to the latest available version")
                update_btn.connect("clicked", self._create_update_handler(pkg))
                btn_box.append(update_btn)

                # Uninstall button
                uninst_btn = Gtk.Button(label="Uninstall")
                uninst_btn.add_css_class("destructive-action")
                uninst_btn.set_tooltip_text(f"Uninstall {pkg.name}")
                uninst_btn.connect("clicked", self._create_uninstall_handler(pkg))
                btn_box.append(uninst_btn)
            else:
                clean_btn = Gtk.Button(label="Clean Files")
                clean_btn.set_tooltip_text("Clear cache and user configurations without uninstalling")
                clean_btn.connect("clicked", self._create_clean_handler(pkg))
                btn_box.append(clean_btn)

                uninst_btn = Gtk.Button(label="Uninstall")
                uninst_btn.add_css_class("destructive-action")
                uninst_btn.connect("clicked", self._create_uninstall_handler(pkg))
                btn_box.append(uninst_btn)

            row.add_suffix(btn_box)
            self.group.add(row)

        if len(filtered) > display_limit:
            info_row = Adw.ActionRow(
                title=f"Showing {display_limit} of {len(filtered)} packages",
                subtitle="Use the search bar above to locate specific packages"
            )
            info_row.add_prefix(Gtk.Image.new_from_icon_name("system-search-symbolic"))
            self.group.add(info_row)

    def _create_update_handler(self, pkg: PackageItem):
        def handler(_btn):
            mgr = getattr(pkg, 'manager', 'pip')
            dialog = Adw.AlertDialog.new(
                f"Update {pkg.name}?",
                f"Attempt to update {pkg.name} (currently v{pkg.version}) to the latest version via {mgr}?"
            )
            dialog.add_response("cancel", "Cancel")
            dialog.add_response("update", "Update")
            dialog.set_response_appearance("update", Adw.ResponseAppearance.SUGGESTED)
            dialog.choose(self.get_root(), None, lambda d, r: self._on_update_confirm(d, r, pkg))
        return handler

    def _on_update_confirm(self, dialog, result, pkg: PackageItem):
        res = dialog.choose_finish(result)
        if res == "update":
            toast = Adw.Toast.new(f"Updating {pkg.name}...")
            self.toast_overlay.add_toast(toast)

            def worker():
                ok, err = PackageManager.update_package(pkg)
                GLib.idle_add(self._on_update_complete, pkg.name, ok, err)

            threading.Thread(target=worker, daemon=True).start()

    def _on_update_complete(self, name, ok, err):
        msg = f"Update {name}: {'Completed' if ok else 'Failed'}"
        toast = Adw.Toast.new(msg)
        self.toast_overlay.add_toast(toast)
        self._load_packages()

    def _on_install_clicked(self, _btn):
        dialog = Adw.AlertDialog.new(
            "Install Python Package",
            "Enter the package name from PyPI to install on the system."
        )
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        box.set_margin_top(8)
        box.set_margin_bottom(8)
        box.set_margin_start(16)
        box.set_margin_end(16)

        entry = Gtk.Entry()
        entry.set_placeholder_text("e.g. requests, ruff, rich")
        box.append(entry)

        has_uv = bool(PackageManager.find_uv_executable())
        mgr_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        mgr_box.set_halign(Gtk.Align.CENTER)
        lbl = Gtk.Label(label="Installer:")
        mgr_box.append(lbl)

        btn_pip = Gtk.CheckButton(label="pip")
        btn_pip.set_active(True)
        mgr_box.append(btn_pip)

        btn_uv = Gtk.CheckButton(label="uv")
        btn_uv.set_group(btn_pip)
        btn_uv.set_sensitive(has_uv)
        if not has_uv:
            btn_uv.set_tooltip_text("uv is not installed on this system")
        mgr_box.append(btn_uv)

        box.append(mgr_box)
        dialog.set_extra_child(box)

        dialog.add_response("cancel", "Cancel")
        dialog.add_response("install", "Install")
        dialog.set_response_appearance("install", Adw.ResponseAppearance.SUGGESTED)

        def on_choose(d, r):
            res = d.choose_finish(r)
            if res == "install":
                pkg_name = entry.get_text().strip()
                if not pkg_name:
                    return
                manager = "uv" if btn_uv.get_active() else "pip"
                toast = Adw.Toast.new(f"Installing {pkg_name} via {manager}...")
                self.toast_overlay.add_toast(toast)

                def worker():
                    ok, msg = PackageManager.install_python_package(pkg_name, manager)
                    GLib.idle_add(self._on_install_complete, pkg_name, ok, msg)

                threading.Thread(target=worker, daemon=True).start()

        dialog.choose(self.get_root(), None, on_choose)

    def _on_install_complete(self, name, ok, msg):
        toast_msg = f"Install {name}: {'Completed' if ok else 'Failed'}"
        toast = Adw.Toast.new(toast_msg)
        self.toast_overlay.add_toast(toast)
        self._load_packages()

    def _create_clean_handler(self, pkg: PackageItem):
        def handler(_btn):
            dialog = Adw.AlertDialog.new(
                f"Clean Files for {pkg.name}?",
                "This will clear user configuration and application cache without uninstalling the package."
            )
            dialog.add_response("cancel", "Cancel")
            dialog.add_response("clean", "Clean Files")
            dialog.set_response_appearance("clean", Adw.ResponseAppearance.DESTRUCTIVE)
            dialog.choose(self.get_root(), None, lambda d, r: self._on_clean_confirm(d, r, pkg))
        return handler

    def _on_clean_confirm(self, dialog, result, pkg: PackageItem):
        res = dialog.choose_finish(result)
        if res == "clean":
            ok, msg = PackageManager.clean_app_data(pkg)
            toast = Adw.Toast.new(msg)
            self.toast_overlay.add_toast(toast)

    def _create_uninstall_handler(self, pkg: PackageItem):
        def handler(_btn):
            dialog = Adw.AlertDialog.new(
                f"Uninstall {pkg.name}?",
                f"Are you sure you want to remove this {pkg.kind} package from the system?"
            )
            dialog.add_response("cancel", "Cancel")
            dialog.add_response("uninstall", "Uninstall")
            dialog.set_response_appearance("uninstall", Adw.ResponseAppearance.DESTRUCTIVE)
            dialog.choose(self.get_root(), None, lambda d, r: self._on_uninstall_confirm(d, r, pkg))
        return handler

    def _on_uninstall_confirm(self, dialog, result, pkg: PackageItem):
        res = dialog.choose_finish(result)
        if res == "uninstall":
            toast = Adw.Toast.new(f"Uninstalling {pkg.name}...")
            self.toast_overlay.add_toast(toast)

            def worker():
                ok, err = PackageManager.uninstall_package(pkg)
                GLib.idle_add(self._on_uninstall_complete, pkg.name, ok, err)

            threading.Thread(target=worker, daemon=True).start()

    def _on_uninstall_complete(self, name, ok, err):
        msg = f"Uninstall {name}: {'Completed' if ok else 'Failed'}"
        toast = Adw.Toast.new(msg)
        self.toast_overlay.add_toast(toast)
        self._load_packages()
