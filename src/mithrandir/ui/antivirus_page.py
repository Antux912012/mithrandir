import os
import threading
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib, Gio, Gdk

from ..backend.antivirus_clamav import (
    ClamAVEngine,
    ThreatFinding,
    QuarantineVault,
    DefinitionsStatus,
    ScanReport,
)


class AntivirusPage(Gtk.ScrolledWindow):
    """
    ClamAV Anti-Malware and Ransomware scanning page with real-time detection,
    manual/automatic definition updates via FreshClam, and quarantine management.
    """

    def __init__(self, toast_overlay: Adw.ToastOverlay, settings: Gio.Settings):
        super().__init__()
        self.toast_overlay = toast_overlay
        self.settings = settings
        self.vault = QuarantineVault()

        self.set_vexpand(True)
        self.set_hexpand(True)
        self.set_kinetic_scrolling(True)

        self.is_scanning = False
        self.is_updating = False
        self.is_installing = False
        self.cancel_event: threading.Event | None = None
        self.pulse_timer = None
        self.detected_threats: list[ThreatFinding] = []

        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        self.box.set_margin_top(24)
        self.box.set_margin_bottom(24)
        self.box.set_margin_start(24)
        self.box.set_margin_end(24)

        # 1. Status / Action Banner
        self.banner = Adw.Banner.new("Virus definitions are outdated. Update to stay protected against new threats.")
        self.banner.set_button_label("Update Now")
        self.banner.connect("button-clicked", lambda _b: self._on_banner_clicked())
        self.banner.set_revealed(False)
        self.box.append(self.banner)

        # 2. Engine & Definitions Status Group
        self._build_status_group()

        # 3. Embedded Terminal Progress Box (Revealer)
        self._build_terminal_box()

        # 4. Scan Profiles Group
        self._build_scan_options_group()

        # 5. Live Scan Progress Box (Revealer)
        self._build_progress_box()

        # 6. Detected Threats Group
        self._build_findings_group()

        # 7. Quarantine Vault Group
        self._build_quarantine_group()

        self.set_child(self.box)

        # Initial background status query
        GLib.idle_add(self._refresh_status)

    def _build_status_group(self):
        self.status_group = Adw.PreferencesGroup()
        self.status_group.set_title("Engine &amp; Definition Status")
        self.status_group.set_description("ClamAV anti-malware engine and signature database configuration")
        self.box.append(self.status_group)

        # Engine Row
        self.engine_row = Adw.ActionRow()
        self.engine_row.set_title("ClamAV Scanning Engine")
        self.engine_row.set_subtitle("Checking binaries...")

        engine_suffix_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        engine_suffix_box.set_valign(Gtk.Align.CENTER)

        self.engine_badge = Gtk.Label(label="Checking")
        self.engine_badge.add_css_class("dim-label")
        self.engine_badge.set_valign(Gtk.Align.CENTER)
        engine_suffix_box.append(self.engine_badge)

        self.install_engine_btn = Gtk.Button(label="Install ClamAV")
        self.install_engine_btn.add_css_class("suggested-action")
        self.install_engine_btn.connect("clicked", lambda _b: self._on_install_clamav_clicked())
        engine_suffix_box.append(self.install_engine_btn)

        self.engine_row.add_suffix(engine_suffix_box)
        self.status_group.add(self.engine_row)

        # Definitions Row
        self.def_row = Adw.ActionRow()
        self.def_row.set_title("Virus &amp; Ransomware Definitions")
        self.def_row.set_subtitle("Inspecting signature database...")

        def_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        def_box.set_valign(Gtk.Align.CENTER)

        self.def_spinner = Gtk.Spinner()
        self.def_spinner.set_visible(False)
        def_box.append(self.def_spinner)

        self.update_btn = Gtk.Button(label="Update Definitions")
        self.update_btn.add_css_class("suggested-action")
        self.update_btn.connect("clicked", lambda _b: self._on_update_definitions_clicked())
        def_box.append(self.update_btn)

        self.def_row.add_suffix(def_box)
        self.status_group.add(self.def_row)

        # Auto-update switch row
        self.auto_update_switch = Adw.SwitchRow()
        self.auto_update_switch.set_title("Update Definitions Automatically Before Scans")
        self.auto_update_switch.set_subtitle("Verifies signature freshness and updates via FreshClam before starting any scan")
        try:
            self.settings.bind(
                "antivirus-auto-update-before-scan",
                self.auto_update_switch,
                "active",
                Gio.SettingsBindFlags.DEFAULT
            )
        except Exception:
            self.auto_update_switch.set_active(True)
        self.status_group.add(self.auto_update_switch)

        # Installation & FreshClam Instructions Expander Row
        self.instructions_expander = Adw.ExpanderRow()
        self.instructions_expander.set_title("How to Install &amp; Configure clamav-freshclam")
        self.instructions_expander.set_subtitle("Terminal commands and setup instructions for your Linux distribution")
        self.instructions_expander.set_icon_name("help-about-symbolic")

        distro_spec = ClamAVEngine.get_install_spec()

        # Distribution specific installation row
        row_distro = Adw.ActionRow()
        row_distro.set_title(f"1. Install ClamAV and FreshClam on {distro_spec.get('distro', 'your system')}")
        row_distro.set_subtitle(f"Command: sudo {distro_spec.get('display_command', 'apt install clamav clamav-freshclam')}")
        copy_distro_btn = Gtk.Button(icon_name="edit-copy-symbolic")
        copy_distro_btn.set_tooltip_text("Copy install command")
        copy_distro_btn.set_valign(Gtk.Align.CENTER)
        copy_distro_btn.add_css_class("flat")
        copy_distro_btn.connect(
            "clicked",
            lambda _b, cmd=f"sudo {distro_spec.get('display_command', 'apt install clamav clamav-freshclam')}":
                self._copy_to_clipboard(cmd, "Install command copied to clipboard")
        )
        row_distro.add_suffix(copy_distro_btn)
        self.instructions_expander.add_row(row_distro)

        # Debian / Ubuntu package instruction row
        row_deb = Adw.ActionRow()
        row_deb.set_title("Debian, Ubuntu &amp; Linux Mint package names")
        row_deb.set_subtitle("sudo apt-get install -y clamav clamav-daemon clamav-freshclam")
        copy_deb_btn = Gtk.Button(icon_name="edit-copy-symbolic")
        copy_deb_btn.set_tooltip_text("Copy Debian command")
        copy_deb_btn.set_valign(Gtk.Align.CENTER)
        copy_deb_btn.add_css_class("flat")
        copy_deb_btn.connect(
            "clicked",
            lambda _b: self._copy_to_clipboard(
                "sudo apt-get install -y clamav clamav-daemon clamav-freshclam",
                "Debian/Ubuntu command copied"
            )
        )
        row_deb.add_suffix(copy_deb_btn)
        self.instructions_expander.add_row(row_deb)

        # Fedora & RHEL row
        row_fedora = Adw.ActionRow()
        row_fedora.set_title("Fedora, RHEL &amp; Rocky Linux package names")
        row_fedora.set_subtitle("sudo dnf install -y clamav clamd clamav-update")
        copy_fedora_btn = Gtk.Button(icon_name="edit-copy-symbolic")
        copy_fedora_btn.set_tooltip_text("Copy Fedora command")
        copy_fedora_btn.set_valign(Gtk.Align.CENTER)
        copy_fedora_btn.add_css_class("flat")
        copy_fedora_btn.connect(
            "clicked",
            lambda _b: self._copy_to_clipboard(
                "sudo dnf install -y clamav clamd clamav-update",
                "Fedora command copied"
            )
        )
        row_fedora.add_suffix(copy_fedora_btn)
        self.instructions_expander.add_row(row_fedora)

        # Arch & openSUSE row
        row_arch = Adw.ActionRow()
        row_arch.set_title("Arch Linux &amp; openSUSE package names")
        row_arch.set_subtitle("Arch: sudo pacman -S clamav • openSUSE: sudo zypper install clamav")
        copy_arch_btn = Gtk.Button(icon_name="edit-copy-symbolic")
        copy_arch_btn.set_tooltip_text("Copy Arch command")
        copy_arch_btn.set_valign(Gtk.Align.CENTER)
        copy_arch_btn.add_css_class("flat")
        copy_arch_btn.connect(
            "clicked",
            lambda _b: self._copy_to_clipboard(
                "sudo pacman -Sy --noconfirm clamav",
                "Arch Linux command copied"
            )
        )
        row_arch.add_suffix(copy_arch_btn)
        self.instructions_expander.add_row(row_arch)

        # Daemon note row
        row_daemon = Adw.ActionRow()
        row_daemon.set_title("2. Automatic Background Signature Updates (systemd service)")
        row_daemon.set_subtitle(
            "clamav-freshclam runs as a system daemon. If manual update encounters a log lock, "
            "Mithrandir automatically pauses the service or updates using local fallback."
        )
        self.instructions_expander.add_row(row_daemon)

        self.status_group.add(self.instructions_expander)

    def _build_terminal_box(self):
        self.terminal_revealer = Gtk.Revealer()
        self.terminal_revealer.set_transition_type(Gtk.RevealerTransitionType.SLIDE_DOWN)
        self.terminal_revealer.set_reveal_child(False)

        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        card.add_css_class("terminal-card")

        # Terminal Header
        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        header.set_valign(Gtk.Align.CENTER)

        term_icon = Gtk.Image.new_from_icon_name("utilities-terminal-symbolic")
        term_icon.set_pixel_size(16)
        header.append(term_icon)

        self.terminal_title = Gtk.Label(label="<b>Terminal Console — Package Installer</b>")
        self.terminal_title.set_use_markup(True)
        self.terminal_title.set_halign(Gtk.Align.START)
        self.terminal_title.set_hexpand(True)
        header.append(self.terminal_title)

        self.terminal_spinner = Gtk.Spinner()
        self.terminal_spinner.set_visible(False)
        header.append(self.terminal_spinner)

        clear_btn = Gtk.Button(label="Clear")
        clear_btn.add_css_class("flat")
        clear_btn.connect("clicked", lambda _b: self._clear_terminal())
        header.append(clear_btn)

        hide_btn = Gtk.Button(label="Hide Console")
        hide_btn.add_css_class("flat")
        hide_btn.connect("clicked", lambda _b: self.terminal_revealer.set_reveal_child(False))
        header.append(hide_btn)

        card.append(header)

        # Monospace Text Scroller
        scroller = Gtk.ScrolledWindow()
        scroller.set_min_content_height(180)
        scroller.set_max_content_height(260)
        scroller.set_vexpand(True)
        scroller.set_kinetic_scrolling(True)

        self.terminal_view = Gtk.TextView()
        self.terminal_view.set_editable(False)
        self.terminal_view.set_cursor_visible(False)
        self.terminal_view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        self.terminal_view.add_css_class("terminal-view")
        self.terminal_buffer = self.terminal_view.get_buffer()

        scroller.set_child(self.terminal_view)
        card.append(scroller)

        self.terminal_revealer.set_child(card)
        self.box.append(self.terminal_revealer)

    def _build_scan_options_group(self):
        self.scan_group = Adw.PreferencesGroup()
        self.scan_group.set_title("Scan Profiles")
        self.scan_group.set_description("Select a scan scope to detect viruses, trojans, ransomware, and exploits")
        self.box.append(self.scan_group)

        card_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=16)
        card_box.set_homogeneous(True)

        # Quick Scan Button
        self.quick_btn = Gtk.Button()
        self.quick_btn.set_child(self._create_profile_card(
            "utilities-system-monitor-symbolic",
            "Quick Scan",
            "Downloads, Cache, Autostart & Temp"
        ))
        self.quick_btn.add_css_class("card")
        self.quick_btn.connect("clicked", lambda _b: self._start_scan("quick"))
        card_box.append(self.quick_btn)

        # Full Scan Button
        self.full_btn = Gtk.Button()
        self.full_btn.set_child(self._create_profile_card(
            "drive-harddisk-symbolic",
            "Full System Scan",
            "Entire filesystem (excluding virtual /proc, /sys)"
        ))
        self.full_btn.add_css_class("card")
        self.full_btn.connect("clicked", lambda _b: self._start_scan("full"))
        card_box.append(self.full_btn)

        # Custom Scan Button
        self.custom_btn = Gtk.Button()
        self.custom_btn.set_child(self._create_profile_card(
            "folder-symbolic",
            "Custom Target",
            "Select specific directory or external drive"
        ))
        self.custom_btn.add_css_class("card")
        self.custom_btn.connect("clicked", lambda _b: self._on_custom_scan_clicked())
        card_box.append(self.custom_btn)

        self.scan_group.add(card_box)

    def _create_profile_card(self, icon_name: str, title: str, subtitle: str) -> Gtk.Widget:
        vbox = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        vbox.set_margin_top(16)
        vbox.set_margin_bottom(16)
        vbox.set_margin_start(16)
        vbox.set_margin_end(16)

        icon = Gtk.Image.new_from_icon_name(icon_name)
        icon.set_pixel_size(32)
        vbox.append(icon)

        lbl_title = Gtk.Label(label=f"<b>{title}</b>")
        lbl_title.set_use_markup(True)
        vbox.append(lbl_title)

        lbl_sub = Gtk.Label(label=subtitle)
        lbl_sub.add_css_class("dim-label")
        lbl_sub.set_wrap(True)
        lbl_sub.set_justify(Gtk.Justification.CENTER)
        vbox.append(lbl_sub)

        return vbox

    def _build_progress_box(self):
        self.progress_revealer = Gtk.Revealer()
        self.progress_revealer.set_transition_type(Gtk.RevealerTransitionType.SLIDE_DOWN)
        self.progress_revealer.set_reveal_child(False)

        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        card.add_css_class("card")
        card.set_margin_top(8)
        card.set_margin_bottom(8)
        card.set_margin_start(4)
        card.set_margin_end(4)

        header_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)

        self.scan_status_label = Gtk.Label(label="Scanning files...")
        self.scan_status_label.set_halign(Gtk.Align.START)
        self.scan_status_label.set_hexpand(True)
        self.scan_status_label.set_ellipsize(3)  # PANGO_ELLIPSIZE_END
        header_box.append(self.scan_status_label)

        self.cancel_btn = Gtk.Button(label="Cancel Scan")
        self.cancel_btn.add_css_class("destructive-action")
        self.cancel_btn.connect("clicked", lambda _b: self._cancel_scan())
        header_box.append(self.cancel_btn)

        card.append(header_box)

        self.progress_bar = Gtk.ProgressBar()
        self.progress_bar.set_pulse_step(0.05)
        card.append(self.progress_bar)

        self.scan_counts_label = Gtk.Label(label="Scanned: 0 files | Threats found: 0")
        self.scan_counts_label.add_css_class("dim-label")
        self.scan_counts_label.set_halign(Gtk.Align.START)
        card.append(self.scan_counts_label)

        self.progress_revealer.set_child(card)
        self.box.append(self.progress_revealer)

    def _build_findings_group(self):
        self.findings_group = Adw.PreferencesGroup()
        self.findings_group.set_title("Detected Threats")
        self.findings_group.set_description("Malware and ransomware detections requiring remediation")
        self.findings_group.set_visible(False)
        self.box.append(self.findings_group)

    def _build_quarantine_group(self):
        self.quarantine_group = Adw.PreferencesGroup()
        self.quarantine_group.set_title("Quarantine Vault")
        self.quarantine_group.set_description("Neutralized files isolated with execution permissions revoked (0000)")
        self.box.append(self.quarantine_group)

        self.quarantine_expander = Adw.ExpanderRow()
        self.quarantine_expander.set_title("Quarantined Files")
        self.quarantine_expander.set_subtitle("0 files in vault")
        self.quarantine_group.add(self.quarantine_expander)

    def _refresh_status(self):
        """Checks ClamAV availability and definition freshness."""
        def worker():
            installed = ClamAVEngine.check_installed()
            spec = ClamAVEngine.get_install_spec()
            max_age = 24
            try:
                max_age = self.settings.get_int("antivirus-definitions-max-age-hours")
            except Exception:
                pass
            def_info = ClamAVEngine.get_definitions_info(max_age_hours=max_age)
            quarantined = self.vault.list_quarantined()

            def update_ui():
                has_engine = bool(installed.get("clamscan") or installed.get("clamdscan"))

                if has_engine:
                    self.engine_badge.set_text("Installed")
                    self.engine_badge.remove_css_class("dim-label")
                    self.engine_badge.remove_css_class("error")
                    self.engine_badge.add_css_class("success")
                    self.engine_row.set_subtitle("Engine ready (clamscan)")
                    self.install_engine_btn.set_label("Reinstall Engine")
                    self.install_engine_btn.remove_css_class("suggested-action")
                    self.quick_btn.set_sensitive(True)
                    self.full_btn.set_sensitive(True)
                else:
                    self.engine_badge.set_text("Missing")
                    self.engine_badge.remove_css_class("dim-label")
                    self.engine_badge.remove_css_class("success")
                    self.engine_badge.add_css_class("error")
                    self.engine_row.set_subtitle(
                        f"clamscan not found • Click 'Install ClamAV' to install via {spec['distro']} ({spec['manager']})"
                    )
                    self.install_engine_btn.set_label("Install ClamAV")
                    self.install_engine_btn.add_css_class("suggested-action")
                    self.quick_btn.set_sensitive(False)
                    self.full_btn.set_sensitive(False)

                if not has_engine:
                    self.banner.set_title(
                        f"ClamAV engine is not installed. Click to install via {spec['distro']} ({spec['manager']})."
                    )
                    self.banner.set_button_label("Install ClamAV")
                    self.banner.set_revealed(True)
                elif def_info.installed:
                    self.def_row.set_subtitle(
                        f"{def_info.status_label} • {def_info.total_signatures:,} signatures • Released {def_info.release_date or 'N/A'}"
                    )
                    if def_info.is_stale:
                        self.banner.set_title("Virus definitions are outdated. Update to stay protected against new threats.")
                        self.banner.set_button_label("Update Now")
                        self.banner.set_revealed(True)
                    else:
                        self.banner.set_revealed(False)
                else:
                    self.def_row.set_subtitle("Definitions missing. Click 'Update Definitions' to download.")
                    self.banner.set_title("Virus definitions missing. Update to enable protection.")
                    self.banner.set_button_label("Update Now")
                    self.banner.set_revealed(True)

                self._populate_quarantine_list(quarantined)

            GLib.idle_add(update_ui)

        threading.Thread(target=worker, daemon=True).start()

    def _populate_quarantine_list(self, quarantined: list[dict]):
        # Clear previous rows inside expander
        # In AdwExpanderRow, remove children
        self.quarantine_expander.set_subtitle(f"{len(quarantined)} files in vault")
        # Remove existing rows in expander
        # Using loop to clear
        while True:
            first_child = None
            # Walk rows
            try:
                # We can remove rows by keeping track
                pass
            except Exception:
                break
            break

        # Rebuild rows
        # Remove all existing child rows if any
        # To recreate cleanly, remove from parent and re-add expander
        self.quarantine_group.remove(self.quarantine_expander)
        self.quarantine_expander = Adw.ExpanderRow()
        self.quarantine_expander.set_title("Quarantined Files")
        self.quarantine_expander.set_subtitle(f"{len(quarantined)} files in vault")
        self.quarantine_group.add(self.quarantine_expander)

        if not quarantined:
            empty_row = Adw.ActionRow()
            empty_row.set_title("No quarantined files")
            empty_row.set_subtitle("Safe files or isolated threats will appear here")
            self.quarantine_expander.add_row(empty_row)
            return

        for item in quarantined:
            t_id = item.get("threat_id", "")
            t_name = item.get("threat_name", "Unknown Threat")
            orig_path = item.get("original_path", "")
            is_ransom = item.get("is_ransomware", False)

            row = Adw.ActionRow()
            badge_text = "RANSOMWARE" if is_ransom else "MALWARE"
            row.set_title(f"[{badge_text}] {t_name}")
            row.set_subtitle(orig_path)

            btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
            btn_box.set_valign(Gtk.Align.CENTER)

            restore_btn = Gtk.Button(label="Restore")
            restore_btn.add_css_class("suggested-action")
            restore_btn.connect("clicked", lambda _b, tid=t_id: self._restore_quarantine(tid))
            btn_box.append(restore_btn)

            delete_btn = Gtk.Button(label="Delete")
            delete_btn.add_css_class("destructive-action")
            delete_btn.connect("clicked", lambda _b, tid=t_id: self._delete_quarantine(tid))
            btn_box.append(delete_btn)

            row.add_suffix(btn_box)
            self.quarantine_expander.add_row(row)

    def _on_banner_clicked(self):
        installed = ClamAVEngine.check_installed()
        if not installed.get("clamscan") and not installed.get("clamdscan"):
            self._on_install_clamav_clicked()
        else:
            self._on_update_definitions_clicked()

    def _copy_to_clipboard(self, text: str, toast_message: str = "Copied to clipboard"):
        try:
            display = Gdk.Display.get_default()
            if display:
                clipboard = display.get_clipboard()
                clipboard.set(text)
                toast = Adw.Toast.new(toast_message)
                self.toast_overlay.add_toast(toast)
        except Exception:
            pass

    def _clear_terminal(self):
        if hasattr(self, "terminal_buffer"):
            self.terminal_buffer.set_text("")

    def _append_terminal_line(self, line: str):
        if not hasattr(self, "terminal_buffer"):
            return
        end_iter = self.terminal_buffer.get_end_iter()
        self.terminal_buffer.insert(end_iter, line + "\n")
        mark = self.terminal_buffer.create_mark(None, self.terminal_buffer.get_end_iter(), False)
        self.terminal_view.scroll_to_mark(mark, 0.05, True, 0.0, 1.0)

    def _on_install_clamav_clicked(self):
        """Installs ClamAV using the host package manager with real-time terminal output."""
        if getattr(self, "is_installing", False):
            return

        self.is_installing = True
        self.install_engine_btn.set_sensitive(False)
        self.terminal_revealer.set_reveal_child(True)
        self.terminal_spinner.set_visible(True)
        self.terminal_spinner.start()

        spec = ClamAVEngine.get_install_spec()
        self.terminal_title.set_markup(
            f"<b>Terminal Console — Installing ClamAV ({spec['distro']} / {spec['manager']})</b>"
        )
        self._clear_terminal()
        self._append_terminal_line("==================================================")
        self._append_terminal_line("       Mithrandir ClamAV Engine Installer         ")
        self._append_terminal_line("==================================================")
        self._append_terminal_line(f"Target Distribution : {spec['distro']}")
        self._append_terminal_line(f"Package Manager     : {spec['manager']}")
        self._append_terminal_line(f"Executing Command   : pkexec {spec['display_command']}")
        self._append_terminal_line("--------------------------------------------------\n")

        def worker():
            def line_cb(line: str):
                GLib.idle_add(lambda: self._append_terminal_line(line))

            ok, msg = ClamAVEngine.install_clamav(line_callback=line_cb)

            def on_done():
                self.is_installing = False
                self.terminal_spinner.stop()
                self.terminal_spinner.set_visible(False)
                self.install_engine_btn.set_sensitive(True)

                if ok:
                    self._append_terminal_line("\n--------------------------------------------------")
                    self._append_terminal_line("[Mithrandir] ✓ SUCCESS: ClamAV is installed and ready for scanning.")
                    toast = Adw.Toast.new("ClamAV successfully installed!")
                else:
                    self._append_terminal_line("\n--------------------------------------------------")
                    self._append_terminal_line(f"[Mithrandir] ✗ FAILED: {msg}")
                    toast = Adw.Toast.new(f"Installation failed: {msg}")

                self.toast_overlay.add_toast(toast)
                self._refresh_status()

            GLib.idle_add(on_done)

        threading.Thread(target=worker, daemon=True).start()

    def _on_update_definitions_clicked(self):
        """Triggers manual signature update via FreshClam."""
        if self.is_updating:
            return

        self.is_updating = True
        self.update_btn.set_sensitive(False)
        self.def_spinner.set_visible(True)
        self.def_spinner.start()
        self.def_row.set_subtitle("Synchronizing signature database with FreshClam...")

        def worker():
            def progress_cb(msg: str):
                GLib.idle_add(lambda: self.def_row.set_subtitle(msg))

            ok, msg = ClamAVEngine.update_definitions(callback_progress=progress_cb)

            def on_done():
                self.is_updating = False
                self.def_spinner.stop()
                self.def_spinner.set_visible(False)
                self.update_btn.set_sensitive(True)
                toast = Adw.Toast.new(msg)
                self.toast_overlay.add_toast(toast)
                self._refresh_status()

            GLib.idle_add(on_done)

        threading.Thread(target=worker, daemon=True).start()

    def _on_custom_scan_clicked(self):
        """Opens folder dialog to pick custom scan target."""
        dialog = Gtk.FileDialog()
        dialog.set_title("Select Directory to Scan")
        dialog.select_folder(self.get_root(), None, self._on_folder_selected)

    def _on_folder_selected(self, dialog, result):
        try:
            folder = dialog.select_folder_finish(result)
            if folder:
                path = folder.get_path()
                if path:
                    self._start_scan("custom", path)
        except Exception:
            pass

    def _start_scan(self, profile: str, custom_path: str | None = None):
        """Initiates a malware scan with real-time feedback."""
        if self.is_scanning:
            return

        self.is_scanning = True
        self.cancel_event = threading.Event()
        self.detected_threats = []

        # Update UI state
        self.quick_btn.set_sensitive(False)
        self.full_btn.set_sensitive(False)
        self.custom_btn.set_sensitive(False)
        self.update_btn.set_sensitive(False)

        self.scan_status_label.set_text(f"Starting {profile.capitalize()} Scan...")
        self.scan_counts_label.set_text("Scanned: 0 files | Threats: 0")
        self.progress_bar.set_fraction(0.0)
        self.progress_revealer.set_reveal_child(True)

        # Clear findings group
        self.findings_group.set_visible(False)

        # Pulse progress bar timer
        self.pulse_timer = GLib.timeout_add(100, self._on_pulse_tick)

        def worker():
            def progress_cb(current_file: str, count: int):
                GLib.idle_add(
                    lambda: self._update_scan_progress(current_file, count, len(self.detected_threats))
                )

            def threat_cb(finding: ThreatFinding):
                GLib.idle_add(lambda: self._add_threat_finding(finding))

            report = ClamAVEngine.scan(
                profile=profile,
                custom_path=custom_path,
                callback_progress=progress_cb,
                callback_threat=threat_cb,
                cancel_event=self.cancel_event,
                settings=self.settings,
            )

            GLib.idle_add(lambda: self._on_scan_finished(report))

        threading.Thread(target=worker, daemon=True).start()

    def _on_pulse_tick(self):
        if self.is_scanning:
            self.progress_bar.pulse()
            return True
        return False

    def _update_scan_progress(self, current_file: str, scanned_count: int, threat_count: int):
        self.scan_status_label.set_text(f"Scanning: {current_file}")
        self.scan_counts_label.set_text(f"Scanned: {scanned_count:,} files | Threats found: {threat_count}")

    def _add_threat_finding(self, finding: ThreatFinding):
        self.detected_threats.append(finding)
        self.findings_group.set_visible(True)

        row = Adw.ActionRow()
        badge = "RANSOMWARE" if finding.is_ransomware else "MALWARE"
        row.set_title(f"[{badge}] {finding.threat_name}")
        row.set_subtitle(finding.file_path)

        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        btn_box.set_valign(Gtk.Align.CENTER)

        q_btn = Gtk.Button(label="Quarantine")
        q_btn.add_css_class("suggested-action")
        q_btn.connect("clicked", lambda _b, f=finding, r=row: self._quarantine_threat(f, r))
        btn_box.append(q_btn)

        del_btn = Gtk.Button(label="Delete")
        del_btn.add_css_class("destructive-action")
        del_btn.connect("clicked", lambda _b, f=finding, r=row: self._delete_threat(f, r))
        btn_box.append(del_btn)

        row.add_suffix(btn_box)
        self.findings_group.add(row)

    def _cancel_scan(self):
        if self.cancel_event:
            self.cancel_event.set()
            self.scan_status_label.set_text("Cancelling scan...")

    def _on_scan_finished(self, report: ScanReport):
        self.is_scanning = False
        if self.pulse_timer:
            GLib.source_remove(self.pulse_timer)
            self.pulse_timer = None

        self.progress_bar.set_fraction(1.0)
        self.quick_btn.set_sensitive(True)
        self.full_btn.set_sensitive(True)
        self.custom_btn.set_sensitive(True)
        self.update_btn.set_sensitive(True)

        if report.was_cancelled:
            toast_text = f"Scan cancelled. {report.files_scanned} files inspected."
        elif report.error_message:
            toast_text = f"Scan error: {report.error_message}"
        else:
            threat_count = len(report.threats_found)
            if threat_count == 0:
                toast_text = f"Scan complete! Clean: 0 threats found across {report.files_scanned:,} files ({report.duration_seconds}s)."
            else:
                toast_text = f"Warning: {threat_count} threat(s) detected during scan!"

        toast = Adw.Toast.new(toast_text)
        self.toast_overlay.add_toast(toast)
        self.scan_status_label.set_text("Scan finished.")
        self._refresh_status()

    def _quarantine_threat(self, finding: ThreatFinding, row: Adw.ActionRow):
        ok, msg = self.vault.quarantine_file(finding)
        self.toast_overlay.add_toast(Adw.Toast.new(msg))
        if ok:
            self.findings_group.remove(row)
            self._refresh_status()

    def _delete_threat(self, finding: ThreatFinding, row: Adw.ActionRow):
        try:
            if os.path.exists(finding.file_path):
                os.remove(finding.file_path)
            self.toast_overlay.add_toast(Adw.Toast.new("Infected file permanently deleted."))
            self.findings_group.remove(row)
        except Exception as e:
            self.toast_overlay.add_toast(Adw.Toast.new(f"Failed to delete: {str(e)}"))

    def _restore_quarantine(self, threat_id: str):
        ok, msg = self.vault.restore_file(threat_id)
        self.toast_overlay.add_toast(Adw.Toast.new(msg))
        self._refresh_status()

    def _delete_quarantine(self, threat_id: str):
        ok, msg = self.vault.delete_permanently(threat_id)
        self.toast_overlay.add_toast(Adw.Toast.new(msg))
        self._refresh_status()
