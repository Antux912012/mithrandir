import sys
import threading
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, Gio, GLib
from .window import MithrandirWindow
from .backend.smart_boost import SmartBoostEngine
from .backend.indicator import IndicatorService

class MithrandirApplication(Adw.Application):
    """
    Main application instance for Mithrandir with background lifecycle,
    StatusNotifierItem panel applet, automated Smart Boost loop, and desktop notification support.
    """

    def __init__(self, version='0.4.1'):
        super().__init__(
            application_id='io.github.antonio.Mithrandir',
            flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE
        )
        self.version = version
        self.settings = None
        self.window = None
        self.indicator = None
        self._is_held = False
        self._boost_in_progress = False

    def _sync_hold_state(self, keep_bg: bool):
        """Keeps the application process alive when minimized to background."""
        if keep_bg and not self._is_held:
            self.hold()
            self._is_held = True
        elif not keep_bg and self._is_held:
            self.release()
            self._is_held = False

    def do_command_line(self, command_line):
        args = command_line.get_arguments()
        is_background = '--background' in args
        trigger_boost = '--smart-boost' in args

        target_page = None
        for i, arg in enumerate(args):
            if arg == '--open-page' and i + 1 < len(args):
                target_page = args[i + 1]

        if not self.window:
            self.window = MithrandirWindow(application=self)

        if trigger_boost:
            self.perform_smart_boost_with_notification()

        if target_page:
            self.window.navigate_to_page(target_page)
        elif not is_background and not trigger_boost:
            self.window.set_visible(True)
            self.window.present()

        return 0

    def do_activate(self):
        if not self.window:
            self.window = MithrandirWindow(application=self)
        self.window.set_visible(True)
        self.window.present()

    def do_startup(self):
        Adw.Application.do_startup(self)
        self.settings = Gio.Settings.new('io.github.antonio.Mithrandir')

        # Keep alive in background if configured
        try:
            keep_bg = self.settings.get_boolean('keep-in-background')
        except Exception:
            keep_bg = True
        self._sync_hold_state(keep_bg)

        # Actions
        pref_action = Gio.SimpleAction.new('preferences', None)
        pref_action.connect('activate', self._on_preferences)
        self.add_action(pref_action)

        about_action = Gio.SimpleAction.new('about', None)
        about_action.connect('activate', self._on_about)
        self.add_action(about_action)

        boost_action = Gio.SimpleAction.new('smart-boost', None)
        boost_action.connect('activate', lambda _a, _p: self.perform_smart_boost_with_notification())
        self.add_action(boost_action)

        quit_action = Gio.SimpleAction.new('quit', None)
        quit_action.connect('activate', self._on_quit)
        self.add_action(quit_action)

        # Shortcuts
        self.set_accels_for_action('app.preferences', ['<primary>comma'])
        self.set_accels_for_action('app.quit', ['<primary>q'])

        # Initialize GNOME panel applet / StatusNotifierItem service
        self.indicator = IndicatorService(app=self)
        self.indicator.on_activate_callback = self._on_indicator_activate
        self.indicator.on_navigate_callback = self._on_indicator_navigate
        self.indicator.on_smart_boost_callback = self.perform_smart_boost_with_notification
        self.indicator.on_preferences_callback = lambda: self._on_preferences(None, None)
        self.indicator.on_quit_callback = lambda: self.quit()

        # Listen for setting changes
        self.settings.connect('changed::show-panel-indicator', self._on_show_indicator_changed)
        self.settings.connect('changed::keep-in-background', self._on_keep_in_bg_changed)

        try:
            show_indicator = self.settings.get_boolean('show-panel-indicator')
        except Exception:
            show_indicator = True

        if show_indicator:
            self.indicator.start()
            self._update_indicator_status()

        # Background Smart Boost interval loop (checks every 60 seconds)
        GLib.timeout_add_seconds(60, self._on_smart_boost_interval)

    def do_shutdown(self):
        if self.indicator:
            self.indicator.stop()
        try:
            from .ai.local_llm import LocalLLMManager
            LocalLLMManager.stop_llamacpp_server()
        except Exception:
            pass
        if self._is_held:
            self.release()
            self._is_held = False
        Adw.Application.do_shutdown(self)

    def _on_show_indicator_changed(self, _settings, _key):
        show = self.settings.get_boolean('show-panel-indicator')
        if self.indicator:
            if show and not self.indicator.is_active:
                self.indicator.start()
                self._update_indicator_status()
            elif not show and self.indicator.is_active:
                self.indicator.stop()

    def _on_keep_in_bg_changed(self, _settings, _key):
        keep_bg = self.settings.get_boolean('keep-in-background')
        self._sync_hold_state(keep_bg)

    def _on_indicator_activate(self):
        if not self.window:
            self.window = MithrandirWindow(application=self)
        self.window.set_visible(True)
        self.window.present()

    def _on_indicator_navigate(self, page_tag: str):
        if not self.window:
            self.window = MithrandirWindow(application=self)
        self.window.navigate_to_page(page_tag)

    def perform_smart_boost_with_notification(self) -> bool:
        """
        Executes Smart Boost asynchronously in a background thread and dispatches
        a native GNOME notification upon completion. Explicitly returns False
        so that when called via GLib.idle_add, GLib removes the source immediately.
        """
        if getattr(self, '_boost_in_progress', False):
            return False
        self._boost_in_progress = True

        def worker():
            try:
                res = SmartBoostEngine.execute_boost(clean_temp=True, drop_caches=True)
                GLib.idle_add(self._on_smart_boost_finished, res)
            except Exception:
                self._boost_in_progress = False

        threading.Thread(target=worker, daemon=True).start()
        return False

    def _on_smart_boost_finished(self, res: dict) -> bool:
        self._boost_in_progress = False
        if res.get('already_running', False):
            return False

        ram_freed = res.get('ram_reclaimed_mb', 0.0)
        disk_freed = res.get('disk_freed_mb', 0.0)

        try:
            notify = self.settings.get_boolean('smart-boost-notify')
        except Exception:
            notify = True

        if notify:
            notification = Gio.Notification.new('Mithrandir Smart Boost')
            notification.set_body(
                f"Performance optimized: Freed {ram_freed:.1f} MB RAM and {disk_freed:.1f} MB clutter."
            )
            notification.set_icon(Gio.ThemedIcon.new('io.github.antonio.Mithrandir'))
            self.send_notification('smart-boost-trigger', notification)

        self._update_indicator_status()

        # Update open dashboard metrics if window is active
        if self.window and hasattr(self.window, 'dashboard_page') and hasattr(self.window.dashboard_page, '_update_metrics'):
            self.window.dashboard_page._update_metrics()

        return False

    def _update_indicator_status(self):
        if self.indicator and self.indicator.is_active:
            try:
                st = SmartBoostEngine.get_boost_status()
                self.indicator.update_status(st['ram_percent'], st['temp_size_str'])
            except Exception:
                pass

    def _on_smart_boost_interval(self) -> bool:
        if not self.settings:
            return True

        def worker():
            try:
                triggered, info = SmartBoostEngine.evaluate_auto_boost(self.settings)
                if triggered:
                    GLib.idle_add(self._on_auto_boost_triggered, info)
                else:
                    GLib.idle_add(self._update_indicator_status)
            except Exception:
                pass

        threading.Thread(target=worker, daemon=True).start()
        return True

    def _on_auto_boost_triggered(self, info: dict) -> bool:
        try:
            if self.settings and self.settings.get_boolean('smart-boost-notify'):
                notification = Gio.Notification.new('Mithrandir Smart Boost')
                notification.set_body(
                    f"Performance optimized: Freed {info.get('ram_reclaimed_mb', 0):.1f} MB RAM and {info.get('disk_freed_mb', 0):.1f} MB clutter."
                )
                notification.set_icon(Gio.ThemedIcon.new('io.github.antonio.Mithrandir'))
                self.send_notification('smart-boost-trigger', notification)
            self._update_indicator_status()
            if self.window and hasattr(self.window, 'dashboard_page') and hasattr(self.window.dashboard_page, '_update_metrics'):
                self.window.dashboard_page._update_metrics()
        except Exception:
            pass
        return False

    def _on_preferences(self, _action, _param):
        win = self.window or self.props.active_window
        if win and hasattr(win, 'show_preferences'):
            win.show_preferences()

    def _on_about(self, _action, _param):
        win = self.window or self.props.active_window
        if win and hasattr(win, 'show_about'):
            win.show_about()

    def _on_quit(self, _action, _param):
        self.quit()

def main(version='0.4.1'):
    app = MithrandirApplication(version=version)
    return app.run(sys.argv)
