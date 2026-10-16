import unittest
import gi
gi.require_version('Gio', '2.0')
gi.require_version('GLib', '2.0')
from gi.repository import Gio, GLib
from mithrandir.backend.indicator import IndicatorService

class TestIndicatorService(unittest.TestCase):

    def setUp(self):
        self.service = IndicatorService()

    def test_initial_state(self):
        self.assertFalse(self.service.is_active)
        self.assertEqual(self.service.icon_name, "io.github.antonio.Mithrandir")
        self.assertIn(1, self.service.menu_items)
        self.assertIn(2, self.service.menu_items)
        self.assertEqual(self.service.children_order, [1, 2, 3, 4, 5, 6, 7])

    def test_sni_properties(self):
        prop_id = self.service._handle_sni_get_prop(None, None, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Id")
        self.assertEqual(prop_id.unpack(), "io.github.antonio.Mithrandir")

        prop_title = self.service._handle_sni_get_prop(None, None, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Title")
        self.assertEqual(prop_title.unpack(), "Mithrandir")

        prop_status = self.service._handle_sni_get_prop(None, None, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Status")
        self.assertEqual(prop_status.unpack(), "Active")

        prop_menu = self.service._handle_sni_get_prop(None, None, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Menu")
        self.assertEqual(prop_menu.unpack(), "/MenuBar")

    def test_menu_properties(self):
        prop_version = self.service._handle_menu_get_prop(None, None, "/MenuBar", "com.canonical.dbusmenu", "Version")
        self.assertEqual(prop_version.unpack(), 4)

        prop_status = self.service._handle_menu_get_prop(None, None, "/MenuBar", "com.canonical.dbusmenu", "Status")
        self.assertEqual(prop_status.unpack(), "normal")

    def test_action_dispatching(self):
        events = []

        self.service.on_navigate_callback = lambda page: events.append(f"navigate:{page}")
        self.service.on_smart_boost_callback = lambda: events.append("boost")
        self.service.on_preferences_callback = lambda: events.append("preferences")
        self.service.on_quit_callback = lambda: events.append("quit")

        # Test item 1 (Dashboard)
        self.service._dispatch_menu_action(1)
        # Test item 2 (Smart Boost)
        self.service._dispatch_menu_action(2)
        # Test item 4 (Cleaner)
        self.service._dispatch_menu_action(4)
        # Test item 5 (Preferences)
        self.service._dispatch_menu_action(5)
        # Test item 7 (Quit)
        self.service._dispatch_menu_action(7)

        # Run main loop briefly to allow idle_add to fire
        loop = GLib.MainLoop()
        GLib.timeout_add(50, loop.quit)
        loop.run()

        self.assertIn("navigate:dashboard", events)
        self.assertIn("boost", events)
        self.assertIn("navigate:cleaner", events)
        self.assertIn("preferences", events)
        self.assertIn("quit", events)

    def test_update_status(self):
        self.service.update_status(65.4, "350 MB")
        self.assertEqual(self.service.tooltip_text, "RAM: 65% | Clutter: 350 MB")

    def test_start_and_stop_lifecycle(self):
        # Starting the service should connect to session bus and register objects
        self.service.start()
        self.assertTrue(self.service.is_active)

        # Stopping should unregister and clean up
        self.service.stop()
        self.assertFalse(self.service.is_active)

if __name__ == '__main__':
    unittest.main()
