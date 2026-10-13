import sys
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, Gio, GLib
from .window import MithrandirWindow
from .backend.smart_boost import SmartBoostEngine

class MithrandirApplication(Adw.Application):
    """
    Main application instance for Mithrandir with background lifecycle,
    automated Smart Boost loop, and desktop notification support.
    """

    def __init__(self, version='0.3.5'):
        super().__init__(
            application_id='io.github.antonio.Mithrandir',
            flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE
        )
        self.version = version
        self.settings = None
        self.window = None

    def do_command_line(self, command_line):
        args = command_line.get_arguments()
        is_background = '--background' in args

        if not self.window:
            self.window = MithrandirWindow(application=self)

        if not is_background:
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

        # Preferences action
        pref_action = Gio.SimpleAction.new('preferences', None)
        pref_action.connect('activate', self._on_preferences)
        self.add_action(pref_action)

        # About action
        about_action = Gio.SimpleAction.new('about', None)
        about_action.connect('activate', self._on_about)
        self.add_action(about_action)

        # Quit action
        quit_action = Gio.SimpleAction.new('quit', None)
        quit_action.connect('activate', self._on_quit)
        self.add_action(quit_action)

        # Shortcuts
        self.set_accels_for_action('app.preferences', ['<primary>comma'])
        self.set_accels_for_action('app.quit', ['<primary>q'])

        # Background Smart Boost interval loop (checks every 60 seconds)
        GLib.timeout_add_seconds(60, self._on_smart_boost_interval)

    def _on_smart_boost_interval(self) -> bool:
        if not self.settings:
            return True
        try:
            triggered, info = SmartBoostEngine.evaluate_auto_boost(self.settings)
            if triggered and self.settings.get_boolean("smart-boost-notify"):
                notification = Gio.Notification.new("Mithrandir Smart Boost")
                notification.set_body(
                    f"Performance optimized: Freed {info.get('ram_reclaimed_mb', 0):.1f} MB RAM and {info.get('disk_freed_mb', 0):.1f} MB clutter."
                )
                self.send_notification("smart-boost-trigger", notification)
        except Exception:
            pass
        return True

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

def main(version='0.3.5'):
    app = MithrandirApplication(version=version)
    return app.run(sys.argv)
