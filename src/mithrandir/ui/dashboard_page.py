import threading
import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib, Gio
from ..backend.monitor import SystemMonitor
from ..backend.distro import DistroDetector
from ..backend.smart_boost import SmartBoostEngine

class MetricTile(Gtk.Box):
    """Square tile widget for realtime sensor monitoring."""
    def __init__(self, title: str, icon_name: str):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.add_css_class("card")
        self.set_size_request(160, 160)
        self.set_margin_top(4)
        self.set_margin_bottom(4)
        self.set_margin_start(4)
        self.set_margin_end(4)

        # Header with icon and title
        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        header.set_margin_top(12)
        header.set_margin_start(12)
        header.set_margin_end(12)

        icon = Gtk.Image.new_from_icon_name(icon_name)
        icon.set_icon_size(Gtk.IconSize.NORMAL)
        header.append(icon)

        title_lbl = Gtk.Label(label=title)
        title_lbl.add_css_class("heading")
        header.append(title_lbl)
        self.append(header)

        # Main value label
        self.val_label = Gtk.Label(label="--")
        self.val_label.add_css_class("title-1")
        self.val_label.set_vexpand(True)
        self.append(self.val_label)

        # Progress bar
        self.progress = Gtk.ProgressBar()
        self.progress.set_margin_start(12)
        self.progress.set_margin_end(12)
        self.progress.set_margin_bottom(4)
        self.append(self.progress)

        # Subtitle label
        self.sub_label = Gtk.Label(label="--")
        self.sub_label.add_css_class("caption")
        self.sub_label.add_css_class("dim-label")
        self.sub_label.set_margin_bottom(12)
        self.append(self.sub_label)

    def set_data(self, val_text: str, fraction: float, sub_text: str = ""):
        self.val_label.set_text(val_text)
        self.progress.set_fraction(max(0.0, min(1.0, fraction)))
        self.sub_label.set_text(sub_text)

class DashboardPage(Gtk.ScrolledWindow):
    """
    Overview page with:
    - Microsoft PC Manager-inspired Smart Boost hero banner with one-click boost
    - Real-time sensor tiles (CPU, RAM, Disk, Temp, Battery, GPU0, GPU1, Network DL/UL)
    - Host system overview
    - Kinetic scrolling enabled
    """

    def __init__(self, toast_overlay: Adw.ToastOverlay = None, settings: Gio.Settings = None):
        super().__init__()
        self.toast_overlay = toast_overlay
        self.settings = settings or Gio.Settings.new("io.github.antonio.Mithrandir")
        self.set_vexpand(True)
        self.set_hexpand(True)
        self.set_kinetic_scrolling(True)

        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        box.set_margin_top(24)
        box.set_margin_bottom(24)
        box.set_margin_start(24)
        box.set_margin_end(24)

        # 1. Microsoft PC Manager Style Smart Boost Banner
        self._build_smart_boost_banner(box)

        # 2. Real-time System Monitor - Square Tiles (every 5 seconds)
        tile_group = Adw.PreferencesGroup()
        tile_group.set_title("Real-Time System Sensors")
        tile_group.set_description("Live telemetry updating every 3 seconds (Stacer-grade sensor suite)")

        self.flow_box = Gtk.FlowBox()
        self.flow_box.set_valign(Gtk.Align.START)
        self.flow_box.set_max_children_per_line(5)
        self.flow_box.set_min_children_per_line(2)
        self.flow_box.set_selection_mode(Gtk.SelectionMode.NONE)
        self.flow_box.set_column_spacing(16)
        self.flow_box.set_row_spacing(16)

        self.tile_cpu = MetricTile("CPU Load", "utilities-system-monitor-symbolic")
        self.tile_mem = MetricTile("Memory", "drive-harddisk-symbolic")
        self.tile_disk = MetricTile("Root Disk", "drive-multidisk-symbolic")
        self.tile_temp = MetricTile("Temperature", "weather-clear-symbolic")
        self.tile_battery = MetricTile("Battery", "battery-symbolic")
        self.tile_net_dl = MetricTile("Download", "network-receive-symbolic")
        self.tile_net_ul = MetricTile("Upload", "network-transmit-symbolic")
        self.tile_gpu0 = MetricTile("GPU 0", "video-display-symbolic")
        self.tile_gpu1 = MetricTile("GPU 1", "video-display-symbolic")

        self.flow_box.append(self.tile_cpu)
        self.flow_box.append(self.tile_mem)
        self.flow_box.append(self.tile_disk)
        self.flow_box.append(self.tile_temp)
        self.flow_box.append(self.tile_battery)
        self.flow_box.append(self.tile_net_dl)
        self.flow_box.append(self.tile_net_ul)
        self.flow_box.append(self.tile_gpu0)
        self.flow_box.append(self.tile_gpu1)

        tile_group.add(self.flow_box)
        box.append(tile_group)

        # 3. System Information on Two Columns
        sys_group = Adw.PreferencesGroup()
        sys_group.set_title("System Information")
        sys_group.set_description("Host configuration and hardware overview")

        info = DistroDetector.get_full_system_info()

        two_col_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=16)
        two_col_box.set_homogeneous(True)

        col1_list = Gtk.ListBox()
        col1_list.add_css_class("boxed-list")
        col1_list.set_selection_mode(Gtk.SelectionMode.NONE)
        col1_list.append(self._create_info_row("Distribution", info['os'], "computer-symbolic"))
        col1_list.append(self._create_info_row("Kernel", info['kernel'], "emblem-system-symbolic"))
        col1_list.append(self._create_info_row("Architecture", info['arch'], "cpu-symbolic"))
        col1_list.append(self._create_info_row("Desktop", info['desktop'], "preferences-desktop-display-symbolic"))
        two_col_box.append(col1_list)

        col2_list = Gtk.ListBox()
        col2_list.add_css_class("boxed-list")
        col2_list.set_selection_mode(Gtk.SelectionMode.NONE)
        cpu_display = info['cpu_model'] if len(info['cpu_model']) <= 32 else info['cpu_model'][:32] + "..."
        col2_list.append(self._create_info_row("Processor", cpu_display, "cpu-symbolic"))
        col2_list.append(self._create_info_row("System Uptime", info['uptime'], "alarm-symbolic"))
        col2_list.append(self._create_info_row("Session", DistroDetector.get_desktop_environment(), "dialog-information-symbolic"))
        two_col_box.append(col2_list)

        sys_group.add(two_col_box)
        box.append(sys_group)

        clamp = Adw.Clamp()
        clamp.set_maximum_size(960)
        clamp.set_child(box)
        self.set_child(clamp)

        # Refresh immediately and every 3 seconds
        self._update_metrics()
        GLib.timeout_add_seconds(3, self._update_metrics)

    def _build_smart_boost_banner(self, parent_box: Gtk.Box):
        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        card.add_css_class("smart-boost-card")

        # Top row: Title and Personalise button
        top_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        
        icon = Gtk.Image.new_from_icon_name("media-flash-symbolic")
        icon.set_pixel_size(24)
        top_row.append(icon)

        title_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        title_box.set_hexpand(True)
        lbl_title = Gtk.Label(label="Smart Boost")
        lbl_title.add_css_class("title-3")
        lbl_title.set_xalign(0)
        title_box.append(lbl_title)

        lbl_desc = Gtk.Label(label="Personalized one-click system performance booster and standby memory trimmer")
        lbl_desc.add_css_class("caption")
        lbl_desc.add_css_class("dim-label")
        lbl_desc.set_xalign(0)
        title_box.append(lbl_desc)

        top_row.append(title_box)

        # Personalise button
        pers_btn = Gtk.Button()
        pers_btn.set_icon_name("emblem-system-symbolic")
        pers_btn.set_tooltip_text("Personalise Smart Boost Rules")
        pers_btn.add_css_class("flat")
        pers_btn.connect("clicked", self._on_personalise_clicked)
        top_row.append(pers_btn)

        card.append(top_row)

        # Middle row: Stat displays + Boost button
        stats_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=24)
        stats_row.set_margin_top(8)

        # RAM stat
        ram_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        ram_box.set_hexpand(True)
        self.lbl_boost_ram = Gtk.Label(label="RAM Usage: --%")
        self.lbl_boost_ram.add_css_class("heading")
        self.lbl_boost_ram.set_xalign(0)
        ram_box.append(self.lbl_boost_ram)

        self.boost_ram_bar = Gtk.ProgressBar()
        ram_box.append(self.boost_ram_bar)
        stats_row.append(ram_box)

        # Temp files stat
        temp_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        temp_box.set_hexpand(True)
        self.lbl_boost_temp = Gtk.Label(label="Temporary Files: -- MB")
        self.lbl_boost_temp.add_css_class("heading")
        self.lbl_boost_temp.set_xalign(0)
        temp_box.append(self.lbl_boost_temp)

        self.lbl_boost_temp_sub = Gtk.Label(label="Standby cache and clutter cleanable")
        self.lbl_boost_temp_sub.add_css_class("caption")
        self.lbl_boost_temp_sub.add_css_class("dim-label")
        self.lbl_boost_temp_sub.set_xalign(0)
        temp_box.append(self.lbl_boost_temp_sub)
        stats_row.append(temp_box)

        # Boost action button
        self.boost_btn = Gtk.Button(label="🚀 Boost")
        self.boost_btn.add_css_class("suggested-action")
        self.boost_btn.add_css_class("pill")
        self.boost_btn.set_valign(Gtk.Align.CENTER)
        self.boost_btn.connect("clicked", self._on_boost_clicked)
        stats_row.append(self.boost_btn)

        card.append(stats_row)
        parent_box.append(card)

    def _on_boost_clicked(self, _btn):
        self.boost_btn.set_sensitive(False)
        self.boost_btn.set_label("Boosting...")

        def worker():
            res = SmartBoostEngine.execute_boost()
            GLib.idle_add(self._on_boost_complete, res)

        threading.Thread(target=worker, daemon=True).start()

    def _on_boost_complete(self, res):
        self.boost_btn.set_sensitive(True)
        self.boost_btn.set_label("🚀 Boost")
        self._update_metrics()

        reclaimed_ram = res.get('ram_reclaimed_mb', 0.0)
        freed_disk = res.get('disk_freed_mb', 0.0)
        msg = f"Boost Complete! Freed {reclaimed_ram:.1f} MB RAM and {freed_disk:.1f} MB temporary files."
        if self.toast_overlay:
            self.toast_overlay.add_toast(Adw.Toast.new(msg))

    def _on_personalise_clicked(self, _btn):
        # Open personalisation dialog
        dialog = Adw.PreferencesWindow()
        dialog.set_title("Personalise Smart Boost")
        dialog.set_transient_for(self.get_root())
        dialog.set_modal(True)
        dialog.set_default_size(500, 420)

        page = Adw.PreferencesPage()
        group = Adw.PreferencesGroup()
        group.set_title("Automatic Smart Boost Conditions")
        group.set_description("Mithrandir will automatically boost performance when thresholds are met")

        # Auto boost switch
        auto_row = Adw.SwitchRow(title="Enable Automatic Smart Boost")
        auto_row.set_subtitle("Continuously monitors RAM and cache clutter in the background")
        auto_row.set_active(self.settings.get_boolean("smart-boost-enabled"))
        auto_row.connect("notify::active", lambda w, _p: self.settings.set_boolean("smart-boost-enabled", w.get_active()))
        group.add(auto_row)

        # RAM threshold
        ram_row = Adw.SpinRow.new_with_range(50, 95, 5)
        ram_row.set_title("Auto-Boost RAM Threshold (%)")
        ram_row.set_subtitle("Trigger boost when system memory usage exceeds this percentage")
        ram_row.set_value(self.settings.get_int("smart-boost-ram-threshold"))
        ram_row.connect("notify::value", lambda w, _p: self.settings.set_int("smart-boost-ram-threshold", int(w.get_value())))
        group.add(ram_row)

        # Temp files threshold
        temp_row = Adw.SpinRow.new_with_range(250, 5000, 250)
        temp_row.set_title("Auto-Boost Temp Clutter Threshold (MB)")
        temp_row.set_subtitle("Trigger boost when cleanable temporary files exceed this size")
        temp_row.set_value(self.settings.get_int("smart-boost-temp-threshold-mb"))
        temp_row.connect("notify::value", lambda w, _p: self.settings.set_int("smart-boost-temp-threshold-mb", int(w.get_value())))
        group.add(temp_row)

        # Notification switch
        notify_row = Adw.SwitchRow(title="Show Notification on Boost")
        notify_row.set_subtitle("Display desktop toast when Smart Boost reclaims memory and files")
        notify_row.set_active(self.settings.get_boolean("smart-boost-notify"))
        notify_row.connect("notify::active", lambda w, _p: self.settings.set_boolean("smart-boost-notify", w.get_active()))
        group.add(notify_row)

        page.add(group)
        dialog.add(page)
        dialog.present()

    def _create_info_row(self, title: str, subtitle: str, icon_name: str) -> Adw.ActionRow:
        row = Adw.ActionRow(title=title)
        row.set_subtitle(subtitle)
        row.add_prefix(Gtk.Image.new_from_icon_name(icon_name))
        return row

    def _update_metrics(self) -> bool:
        if getattr(self, '_metrics_busy', False):
            return True
        self._metrics_busy = True

        def worker():
            try:
                boost_stat = SmartBoostEngine.get_boost_status()
                cpu_pct = SystemMonitor.get_cpu_percent()
                mem = SystemMonitor.get_memory_info()
                disk = SystemMonitor.get_disk_usage('/')
                temps = SystemMonitor.get_temperatures()
                bat = SystemMonitor.get_battery_info()
                net_rates = SystemMonitor.get_network_rates()
                gpus = SystemMonitor.get_gpu_info()
                GLib.idle_add(
                    self._apply_metrics_to_ui,
                    boost_stat, cpu_pct, mem, disk, temps, bat, net_rates, gpus
                )
            except Exception:
                self._metrics_busy = False

        threading.Thread(target=worker, daemon=True).start()
        return True

    def _apply_metrics_to_ui(self, boost_stat, cpu_pct, mem, disk, temps, bat, net_rates, gpus):
        self._metrics_busy = False

        # Update Smart Boost Banner
        self.lbl_boost_ram.set_text(f"RAM Usage: {boost_stat['ram_percent']:.1f}%")
        self.boost_ram_bar.set_fraction(boost_stat['ram_percent'] / 100.0)
        self.lbl_boost_temp.set_text(f"Temporary Files: {boost_stat['temp_size_str']}")

        # CPU
        self.tile_cpu.set_data(f"{cpu_pct}%", cpu_pct / 100.0, "Total Load")

        # Memory
        mem_pct = mem.get('percent', 0.0)
        self.tile_mem.set_data(
            f"{mem_pct}%",
            mem_pct / 100.0,
            f"{mem.get('used_mb', 0):.0f} / {mem.get('total_mb', 0):.0f} MB"
        )

        # Disk
        disk_pct = disk.get('percent', 0.0)
        self.tile_disk.set_data(
            f"{disk_pct}%",
            disk_pct / 100.0,
            f"{disk.get('free_gb', 0):.1f} GB Free"
        )

        # Temperature
        cpu_temp = temps.get('cpu_temp', 0.0)
        temp_fraction = cpu_temp / 100.0 if cpu_temp > 0 else 0.0
        self.tile_temp.set_data(f"{cpu_temp}°C", temp_fraction, "CPU Package")

        # Battery (Real Capacity vs Maximum Design Capacity in Wh)
        if bat.get('present', False) and bat.get('design_capacity_wh', 0.0) > 0:
            health_pct = bat.get('capacity_percent', 0.0)
            real_wh = bat.get('real_capacity_wh', 0.0)
            design_wh = bat.get('design_capacity_wh', 0.0)
            frac = real_wh / design_wh if design_wh > 0 else 0.0
            self.tile_battery.set_data(
                f"{health_pct:.1f}%",
                frac,
                f"{real_wh:.1f} / {design_wh:.1f} Wh"
            )
            status_text = f" · {bat['status']}" if bat.get('status') and bat['status'] != 'Unknown' else ""
            self.tile_battery.set_tooltip_text(
                f"Battery Health: {health_pct:.1f}%\n"
                f"Real Capacity: {real_wh:.2f} Wh\n"
                f"Design Capacity: {design_wh:.2f} Wh"
                f"{status_text}"
            )
        else:
            self.tile_battery.set_data("N/A", 0.0, "No Battery")
            self.tile_battery.set_tooltip_text("No battery detected on this system")

        # Network Telemetry (Stacer feature)
        dl_frac = min(1.0, net_rates['rx_bytes_sec'] / (20 * 1024 * 1024))
        ul_frac = min(1.0, net_rates['tx_bytes_sec'] / (10 * 1024 * 1024))
        self.tile_net_dl.set_data(net_rates['rx_str'], dl_frac, "Incoming Rate")
        self.tile_net_ul.set_data(net_rates['tx_str'], ul_frac, "Outgoing Rate")

        # GPU0 & GPU1
        if len(gpus) > 0:
            gpu0 = gpus[0]
            self.tile_gpu0.set_data(
                f"{gpu0.get('util_percent', 0):.0f}%",
                gpu0.get('util_percent', 0) / 100.0,
                gpu0.get('name', 'GPU 0')[:16]
            )
        else:
            self.tile_gpu0.set_data("N/A", 0.0, "No GPU Detected")

        if len(gpus) > 1:
            gpu1 = gpus[1]
            self.tile_gpu1.set_data(
                f"{gpu1.get('util_percent', 0):.0f}%",
                gpu1.get('util_percent', 0) / 100.0,
                gpu1.get('name', 'GPU 1')[:16]
            )
        else:
            self.tile_gpu1.set_data("N/A", 0.0, "Integrated / None")
        return False
