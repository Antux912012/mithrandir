import gi
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, Gio, GLib
from .ui.dashboard_page import DashboardPage
from .ui.cleaner_page import CleanerPage
from .ui.processes_page import ProcessesPage
from .ui.autostart_page import AutostartPage
from .ui.services_page import ServicesPage
from .ui.uninstaller_page import UninstallerPage
from .ui.btrfs_page import BtrfsPage
from .ui.grub_page import GrubPage
from .ui.ai_page import AIPage
from .ui.antivirus_page import AntivirusPage
from .ui.preferences_dialog import PreferencesDialog
from .ui.theme import ThemeManager

class MithrandirWindow(Adw.ApplicationWindow):
    """
    Main application window following GNOME HIG and Libadwaita design.
    Features:
    - Pure 3-horizontal-lines hamburger icon button on the top-left
    - Perfect vertical alignment between the hamburger button and the sidebar icons
    - Color #2b7bdd shaded dynamically for light and dark themes on sidebar and headerbar
    - Papirus icon theme prioritized with symbolic fallbacks
    - Kinetic scrolling across all scrollable areas
    - Adaptive layout handling small screen devices and large displays via AdwBreakpoint
    - Fluid animated easing for expanding/collapsing the navigation sidebar
    - Collapsed icon-rail mode with high-contrast monochrome icons
    - Expanded mode displaying full titles
    - Keep running in background support
    """

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.set_default_size(1080, 720)
        self.set_title("Mithrandir")

        self.settings = Gio.Settings.new("io.github.antonio.Mithrandir")

        self._configure_icon_theme()

        # Apply #2b7bdd color theme dynamically adapted to system dark/light mode
        ThemeManager.apply_theme()

        # Toast overlay as top-level notification layer
        self.toast_overlay = Adw.ToastOverlay()

        # Window root toolbar view for a seamless, unified headerbar
        self.toolbar_view = Adw.ToolbarView()
        self.header_bar = Adw.HeaderBar()

        # TOP-LEFT: Clean 3-horizontal-lines hamburger menu button
        self.hamburger_btn = Gtk.Button()
        self.hamburger_btn.set_icon_name("mithrandir-hamburger-symbolic")
        self.hamburger_btn.set_tooltip_text("Toggle Navigation Menu")
        self.hamburger_btn.set_margin_start(4)
        self.hamburger_btn.connect("clicked", self._on_toggle_menu_clicked)
        self.header_bar.pack_start(self.hamburger_btn)

        # Center Title Widget
        self.title_widget = Adw.WindowTitle.new("Mithrandir", "Dashboard")
        self.header_bar.set_title_widget(self.title_widget)

        # TOP-RIGHT: Circled 'i' info button
        self.info_btn = Gtk.Button()
        self.info_btn.set_icon_name("dialog-information-symbolic")
        self.info_btn.set_tooltip_text("About Mithrandir and System Info")
        self.info_btn.connect("clicked", lambda _b: self.show_about())
        self.header_bar.pack_end(self.info_btn)

        # TOP-RIGHT: Preferences & Menu
        menu_button = Gtk.MenuButton()
        menu_button.set_icon_name("view-more-symbolic")
        menu = Gio.Menu()
        menu.append("Preferences", "app.preferences")
        menu.append("About Mithrandir", "app.about")
        menu.append("Quit Mithrandir", "app.quit")
        menu_button.set_menu_model(menu)
        self.header_bar.pack_end(menu_button)

        self.toolbar_view.add_top_bar(self.header_bar)

        # Main horizontal layout: Sidebar + Separator + Content Stack
        self.main_h_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)

        # Content setup
        self.stack = Gtk.Stack()
        self.stack.set_transition_type(Gtk.StackTransitionType.CROSSFADE)
        self.stack.set_hexpand(True)
        self.stack.set_vexpand(True)

        # Subpages
        self.dashboard_page = DashboardPage(self.toast_overlay, self.settings)
        self.cleaner_page = CleanerPage(self.toast_overlay)
        self.processes_page = ProcessesPage(self.toast_overlay)
        self.autostart_page = AutostartPage(self.toast_overlay)
        self.services_page = ServicesPage(self.toast_overlay)
        self.uninstaller_page = UninstallerPage(self.toast_overlay)
        self.btrfs_page = BtrfsPage(self.toast_overlay)
        self.grub_page = GrubPage(self.toast_overlay)
        self.ai_page = AIPage(self.toast_overlay, self.settings)
        self.antivirus_page = AntivirusPage(self.toast_overlay, self.settings)

        self.stack.add_titled(self.dashboard_page, "dashboard", "Dashboard")
        self.stack.add_titled(self.cleaner_page, "cleaner", "Cleaner")
        self.stack.add_titled(self.processes_page, "processes", "Processes")
        self.stack.add_titled(self.autostart_page, "autostart", "Startup Apps")
        self.stack.add_titled(self.services_page, "services", "Services")
        self.stack.add_titled(self.uninstaller_page, "uninstaller", "Uninstaller")
        self.stack.add_titled(self.btrfs_page, "btrfs", "BTRFS Snapshots")
        self.stack.add_titled(self.grub_page, "grub", "GRUB Bootloader")
        self.stack.add_titled(self.ai_page, "ai", "AI Diagnostics")
        self.stack.add_titled(self.antivirus_page, "antivirus", "Anti-Malware")

        # Sidebar setup
        self.sidebar_expanded = True
        self.target_width = 200
        self.current_width = 200
        self.animation_timer = None

        self.sidebar_widget = self._create_sidebar()
        self.main_h_box.append(self.sidebar_widget)

        self.separator = Gtk.Separator(orientation=Gtk.Orientation.VERTICAL)
        self.main_h_box.append(self.separator)
        self.main_h_box.append(self.stack)

        self.toolbar_view.set_content(self.main_h_box)
        self.toast_overlay.set_child(self.toolbar_view)
        self.set_content(self.toast_overlay)

        # Setup Breakpoint for Adaptive layouts (Small screens <= 720px)
        self._setup_adaptive_breakpoints()

        # Handle window close request (keep running in background)
        self.connect("close-request", self._on_close_request)

    def _on_close_request(self, _win) -> bool:
        """
        Keeps Mithrandir running in background if enabled in settings.
        """
        try:
            keep_in_bg = self.settings.get_boolean("keep-in-background")
        except Exception:
            keep_in_bg = True

        if keep_in_bg:
            self.set_visible(False)
            return True  # Inhibit window destruction
        return False  # Allow destruction

    def _configure_icon_theme(self):
        """Ensure Papirus icons are prioritized if installed on host or inside runtime."""
        theme = Gtk.IconTheme.get_for_display(Gtk.Widget.get_display(self))
        for p in ['/usr/share/icons/Papirus', '/usr/share/icons/Papirus-Dark', '/usr/share/icons/Papirus-Light', '/run/host/usr/share/icons/Papirus']:
            theme.add_search_path(p)

    def _setup_adaptive_breakpoints(self):
        """Enables responsive adaptation on small screen devices and window resizing."""
        breakpoint = Adw.Breakpoint.new(Adw.BreakpointCondition.parse("max-width: 720px"))
        breakpoint.connect("apply", self._on_small_screen_apply)
        breakpoint.connect("unapply", self._on_large_screen_apply)
        self.add_breakpoint(breakpoint)

    def _on_small_screen_apply(self, _bp):
        self.sidebar_expanded = False
        self.target_width = 48
        self.current_width = 48
        self.sidebar_box.set_size_request(48, -1)
        for tag, data in self.sidebar_items.items():
            data['label'].set_visible(False)

    def _on_large_screen_apply(self, _bp):
        self.sidebar_expanded = True
        self.target_width = 200
        self.current_width = 200
        self.sidebar_box.set_size_request(200, -1)
        for tag, data in self.sidebar_items.items():
            data['label'].set_visible(True)

    def _on_toggle_menu_clicked(self, _btn):
        self.sidebar_expanded = not self.sidebar_expanded
        self.target_width = 200 if self.sidebar_expanded else 48
        self._start_fluid_animation()

    def _start_fluid_animation(self):
        if self.animation_timer is not None:
            GLib.source_remove(self.animation_timer)
            self.animation_timer = None

        if self.sidebar_expanded:
            for tag, data in self.sidebar_items.items():
                data['label'].set_visible(True)

        self.animation_timer = GLib.timeout_add(16, self._animate_sidebar_step)

    def _animate_sidebar_step(self):
        diff = self.target_width - self.current_width
        if abs(diff) <= 3:
            self.current_width = self.target_width
            self.sidebar_box.set_size_request(int(self.current_width), -1)
            if not self.sidebar_expanded:
                for tag, data in self.sidebar_items.items():
                    data['label'].set_visible(False)
            self.animation_timer = None
            return False

        # Fluid easing step
        step = diff * 0.38
        if abs(step) < 2:
            step = 2 if diff > 0 else -2
        self.current_width += step
        self.sidebar_box.set_size_request(int(self.current_width), -1)
        return True

    def _create_sidebar(self) -> Gtk.Widget:
        self.sidebar_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.sidebar_box.set_size_request(200, -1)
        self.sidebar_box.add_css_class("sidebar")

        # Navigation list with items
        self.list_box = Gtk.ListBox()
        self.list_box.add_css_class("navigation-sidebar")
        self.list_box.set_selection_mode(Gtk.SelectionMode.SINGLE)

        # High-contrast Papirus / FreeDesktop symbolic icons
        items = [
            ("dashboard", "Dashboard", "utilities-system-monitor-symbolic"),
            ("cleaner", "Cleaner", "user-trash-symbolic"),
            ("processes", "Processes", "utilities-terminal-symbolic"),
            ("autostart", "Startup Apps", "system-run-symbolic"),
            ("services", "Services", "emblem-system-symbolic"),
            ("uninstaller", "Uninstaller", "system-software-install-symbolic"),
            ("btrfs", "BTRFS Snapshots", "drive-harddisk-symbolic"),
            ("grub", "GRUB Bootloader", "computer-symbolic"),
            ("ai", "AI Diagnostics", "system-search-symbolic"),
            ("antivirus", "Anti-Malware", "security-high-symbolic")
        ]

        self.sidebar_items = {}
        for tag, title, icon_name in items:
            row = Gtk.ListBoxRow()
            row_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
            row_box.set_margin_top(8)
            row_box.set_margin_bottom(8)
            row_box.set_margin_start(6)
            row_box.set_margin_end(6)

            icon = Gtk.Image.new_from_icon_name(icon_name)
            icon.set_pixel_size(18)
            icon.set_tooltip_text(title)
            row_box.append(icon)

            lbl = Gtk.Label(label=title)
            lbl.set_xalign(0)
            lbl.set_hexpand(True)
            row_box.append(lbl)

            row.set_child(row_box)
            row.tag = tag
            row.page_title = title
            self.list_box.append(row)

            self.sidebar_items[tag] = {
                'row': row,
                'icon': icon,
                'label': lbl
            }

        self.list_box.connect("row-selected", self._on_sidebar_row_selected)
        self.list_box.select_row(self.sidebar_items["dashboard"]['row'])

        scroller = Gtk.ScrolledWindow()
        scroller.set_vexpand(True)
        scroller.set_kinetic_scrolling(True)
        scroller.set_child(self.list_box)
        self.sidebar_box.append(scroller)

        return self.sidebar_box

    def _on_sidebar_row_selected(self, _box, row):
        if row and hasattr(row, 'tag') and hasattr(self, 'stack'):
            self.stack.set_visible_child_name(row.tag)
            if hasattr(self, 'title_widget'):
                self.title_widget.set_subtitle(getattr(row, 'page_title', 'Dashboard'))

    def show_preferences(self):
        dialog = PreferencesDialog(self.settings)
        dialog.present(self)

    def show_about(self):
        about = Adw.AboutDialog.new()
        about.set_application_name("Mithrandir")
        about.set_application_icon("io.github.antonio.Mithrandir")
        about.set_developer_name("Antonio")
        app = self.get_application()
        version = getattr(app, 'version', '0.3.5') if app else '0.3.5'
        about.set_version(version)
        about.set_comments("Modern GNU/Linux Cleaner, Maintenance, Optimizer, and AI Diagnostics")
        about.set_website("https://github.com/Antux912012/mithrandir")
        about.set_issue_url("https://github.com/Antux912012/mithrandir/issues")
        about.set_license_type(Gtk.License.GPL_3_0)
        about.present(self)
