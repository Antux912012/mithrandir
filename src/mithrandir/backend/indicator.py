import time
import logging
import gi
gi.require_version('Gio', '2.0')
gi.require_version('GLib', '2.0')
from gi.repository import Gio, GLib

logger = logging.getLogger(__name__)

SNI_XML = """
<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="Menu" type="o" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
    <method name="ContextMenu">
      <arg type="i" name="x" direction="in"/>
      <arg type="i" name="y" direction="in"/>
    </method>
    <method name="Activate">
      <arg type="i" name="x" direction="in"/>
      <arg type="i" name="y" direction="in"/>
    </method>
    <method name="SecondaryActivate">
      <arg type="i" name="x" direction="in"/>
      <arg type="i" name="y" direction="in"/>
    </method>
    <method name="Scroll">
      <arg type="i" name="delta" direction="in"/>
      <arg type="s" name="orientation" direction="in"/>
    </method>
    <signal name="NewTitle"/>
    <signal name="NewIcon"/>
    <signal name="NewAttentionIcon"/>
    <signal name="NewOverlayIcon"/>
    <signal name="NewToolTip"/>
    <signal name="NewStatus">
      <arg type="s" name="status"/>
    </signal>
  </interface>
</node>
"""

DBUSMENU_XML = """
<node>
  <interface name="com.canonical.dbusmenu">
    <property name="Version" type="u" access="read"/>
    <property name="TextDirection" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="IconThemePath" type="as" access="read"/>
    <method name="GetLayout">
      <arg type="i" name="parentId" direction="in"/>
      <arg type="i" name="recursionDepth" direction="in"/>
      <arg type="as" name="propertyNames" direction="in"/>
      <arg type="u" name="revision" direction="out"/>
      <arg type="(ia{sv}av)" name="layout" direction="out"/>
    </method>
    <method name="GetGroupProperties">
      <arg type="ai" name="ids" direction="in"/>
      <arg type="as" name="propertyNames" direction="in"/>
      <arg type="a(ia{sv})" name="properties" direction="out"/>
    </method>
    <method name="GetProperty">
      <arg type="i" name="id" direction="in"/>
      <arg type="s" name="name" direction="in"/>
      <arg type="v" name="value" direction="out"/>
    </method>
    <method name="Event">
      <arg type="i" name="id" direction="in"/>
      <arg type="s" name="eventId" direction="in"/>
      <arg type="v" name="data" direction="in"/>
      <arg type="u" name="timestamp" direction="in"/>
    </method>
    <method name="EventGroup">
      <arg type="a(isvu)" name="events" direction="in"/>
      <arg type="ai" name="idErrors" direction="out"/>
    </method>
    <method name="AboutToShow">
      <arg type="i" name="id" direction="in"/>
      <arg type="b" name="needUpdate" direction="out"/>
    </method>
    <method name="AboutToShowGroup">
      <arg type="ai" name="ids" direction="in"/>
      <arg type="ai" name="updatesNeeded" direction="out"/>
      <arg type="ai" name="idErrors" direction="out"/>
    </method>
    <signal name="LayoutUpdated">
      <arg type="u" name="revision"/>
      <arg type="i" name="parentId"/>
    </signal>
  </interface>
</node>
"""

class IndicatorService:
    """
    Native FreeDesktop StatusNotifierItem (SNI) and DBusMenu exporter for Mithrandir.
    Integrates directly into the GNOME Shell top panel via the AppIndicator extension,
    as well as KDE Plasma, XFCE, and other desktop panels, without requiring
    legacy GTK3 bindings or external C libraries.
    """

    def __init__(self, app=None):
        self.app = app
        self.bus = None
        self._sni_reg_id = 0
        self._menu_reg_id = 0
        self._watcher_watch_id = 0
        self._is_active = False

        self.tooltip_text = "System Optimizer & Diagnostics"
        self.icon_name = "io.github.antonio.Mithrandir"

        # Callbacks
        self.on_activate_callback = None
        self.on_smart_boost_callback = None
        self.on_navigate_callback = None
        self.on_preferences_callback = None
        self.on_quit_callback = None
        self._last_boost_click_time = 0.0

        # Introspection structures
        self.sni_node = Gio.DBusNodeInfo.new_for_xml(SNI_XML)
        self.menu_node = Gio.DBusNodeInfo.new_for_xml(DBUSMENU_XML)
        self.sni_iface = self.sni_node.interfaces[0]
        self.menu_iface = self.menu_node.interfaces[0]

        # Menu definition
        self.menu_items = {
            0: {"children-display": GLib.Variant("s", "submenu")},
            1: {
                "label": GLib.Variant("s", "Open Mithrandir"),
                "enabled": GLib.Variant("b", True),
                "icon-name": GLib.Variant("s", "io.github.antonio.Mithrandir")
            },
            2: {
                "label": GLib.Variant("s", "⚡ Smart Boost"),
                "enabled": GLib.Variant("b", True),
                "icon-name": GLib.Variant("s", "speedometer-symbolic")
            },
            3: {"type": GLib.Variant("s", "separator")},
            4: {
                "label": GLib.Variant("s", "🧹 Cleaner"),
                "enabled": GLib.Variant("b", True),
                "icon-name": GLib.Variant("s", "user-trash-symbolic")
            },
            5: {
                "label": GLib.Variant("s", "⚙️ Preferences..."),
                "enabled": GLib.Variant("b", True),
                "icon-name": GLib.Variant("s", "preferences-system-symbolic")
            },
            6: {"type": GLib.Variant("s", "separator")},
            7: {
                "label": GLib.Variant("s", "Quit Mithrandir"),
                "enabled": GLib.Variant("b", True),
                "icon-name": GLib.Variant("s", "application-exit-symbolic")
            }
        }
        self.children_order = [1, 2, 3, 4, 5, 6, 7]

    @property
    def is_active(self) -> bool:
        return self._is_active

    def start(self):
        """Starts the StatusNotifierItem service and registers it with the panel watcher."""
        if self._is_active:
            return

        try:
            self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            if not self.bus:
                logger.warning("Could not connect to D-Bus session bus.")
                return

            # Register SNI object at /StatusNotifierItem
            self._sni_reg_id = self.bus.register_object(
                "/StatusNotifierItem",
                self.sni_iface,
                self._handle_sni_method,
                self._handle_sni_get_prop,
                None
            )

            # Register DBusMenu at /MenuBar
            self._menu_reg_id = self.bus.register_object(
                "/MenuBar",
                self.menu_iface,
                self._handle_menu_method,
                self._handle_menu_get_prop,
                None
            )

            self._is_active = True

            # Watch StatusNotifierWatcher so we register immediately and re-register if GNOME Shell restarts
            self._watcher_watch_id = Gio.bus_watch_name_on_connection(
                self.bus,
                "org.kde.StatusNotifierWatcher",
                Gio.BusNameWatcherFlags.NONE,
                self._on_watcher_appeared,
                self._on_watcher_vanished
            )

            logger.info("Mithrandir StatusNotifierItem indicator service registered on D-Bus.")
        except Exception as e:
            logger.error(f"Failed to start IndicatorService: {e}")
            self._is_active = False

    def stop(self):
        """Unregisters the service and removes the applet from the GNOME panel."""
        if not self._is_active:
            return

        if self._watcher_watch_id and self.bus:
            Gio.bus_unwatch_name(self._watcher_watch_id)
            self._watcher_watch_id = 0

        if self._sni_reg_id and self.bus:
            self.bus.unregister_object(self._sni_reg_id)
            self._sni_reg_id = 0

        if self._menu_reg_id and self.bus:
            self.bus.unregister_object(self._menu_reg_id)
            self._menu_reg_id = 0

        self._is_active = False
        logger.info("Mithrandir StatusNotifierItem indicator service stopped.")

    def update_status(self, ram_percent: float, temp_str: str):
        """Updates the live status displayed in the tooltip."""
        self.tooltip_text = f"RAM: {ram_percent:.0f}% | Clutter: {temp_str}"
        if self._is_active and self.bus:
            try:
                # Emit NewToolTip signal
                self.bus.emit_signal(
                    None,
                    "/StatusNotifierItem",
                    "org.kde.StatusNotifierItem",
                    "NewToolTip",
                    None
                )
            except Exception:
                pass

    def _on_watcher_appeared(self, _conn, _name, _owner):
        """Called when org.kde.StatusNotifierWatcher is detected on the bus."""
        self._register_with_watcher()

    def _on_watcher_vanished(self, _conn, _name):
        pass

    def _register_with_watcher(self):
        if not self.bus or not self._is_active:
            return
        try:
            self.bus.call_sync(
                "org.kde.StatusNotifierWatcher",
                "/StatusNotifierWatcher",
                "org.kde.StatusNotifierWatcher",
                "RegisterStatusNotifierItem",
                GLib.Variant("(s)", ("/StatusNotifierItem",)),
                None,
                Gio.DBusCallFlags.NONE,
                1500,
                None
            )
            logger.info("Successfully registered with org.kde.StatusNotifierWatcher.")
        except Exception as e:
            logger.warning(f"RegisterStatusNotifierItem call failed: {e}")

    # SNI Callbacks
    def _handle_sni_get_prop(self, conn, sender, path, iface, prop_name):
        props = {
            "Category": GLib.Variant("s", "ApplicationStatus"),
            "Id": GLib.Variant("s", "io.github.antonio.Mithrandir"),
            "Title": GLib.Variant("s", "Mithrandir"),
            "Status": GLib.Variant("s", "Active"),
            "IconName": GLib.Variant("s", self.icon_name),
            "Menu": GLib.Variant("o", "/MenuBar"),
            "ItemIsMenu": GLib.Variant("b", False),
            "ToolTip": GLib.Variant("(sa(iiay)ss)", ("", [], "Mithrandir", self.tooltip_text))
        }
        return props.get(prop_name)

    @staticmethod
    def _run_once(callback, *args) -> bool:
        """Helper to ensure a callback scheduled via GLib.idle_add runs strictly once."""
        try:
            callback(*args)
        except Exception as e:
            logger.error(f"Indicator callback error: {e}")
        return GLib.SOURCE_REMOVE

    def _handle_sni_method(self, conn, sender, path, iface, method, params, invocation):
        if method in ("Activate", "SecondaryActivate"):
            if self.on_activate_callback:
                GLib.idle_add(self._run_once, self.on_activate_callback)
        invocation.return_value(None)

    # DBusMenu Callbacks
    def _handle_menu_get_prop(self, conn, sender, path, iface, prop_name):
        props = {
            "Version": GLib.Variant("u", 4),
            "TextDirection": GLib.Variant("s", "ltr"),
            "Status": GLib.Variant("s", "normal"),
            "IconThemePath": GLib.Variant("as", [])
        }
        return props.get(prop_name)

    def _handle_menu_method(self, conn, sender, path, iface, method, params, invocation):
        if method == "GetLayout":
            children_variants = []
            for cid in self.children_order:
                c_struct = (cid, self.menu_items[cid], [])
                children_variants.append(GLib.Variant("(ia{sv}av)", c_struct))
            
            root = (0, self.menu_items[0], children_variants)
            invocation.return_value(GLib.Variant("(u(ia{sv}av))", (1, root)))

        elif method == "GetGroupProperties":
            ids, prop_names = params.unpack()
            result = []
            for item_id in ids:
                if item_id in self.menu_items:
                    result.append((item_id, self.menu_items[item_id]))
            invocation.return_value(GLib.Variant("(a(ia{sv}))", (result,)))

        elif method == "GetProperty":
            item_id, prop_name = params.unpack()
            item = self.menu_items.get(item_id, {})
            val = item.get(prop_name, GLib.Variant("s", ""))
            invocation.return_value(GLib.Variant("(v)", (val,)))

        elif method == "AboutToShow":
            invocation.return_value(GLib.Variant("(b)", (False,)))

        elif method == "Event":
            item_id, event_type, data, timestamp = params.unpack()
            if event_type == "clicked":
                self._dispatch_menu_action(item_id)
            invocation.return_value(None)

        elif method == "EventGroup":
            (events,) = params.unpack()
            for item_id, event_type, _data, _timestamp in events:
                if event_type == "clicked":
                    self._dispatch_menu_action(item_id)
            invocation.return_value(GLib.Variant("(ai)", ([],)))

        else:
            invocation.return_value(None)

    def _dispatch_menu_action(self, item_id: int):
        now = time.monotonic()
        if item_id == 1:  # Open Mithrandir (Dashboard)
            if self.on_navigate_callback:
                GLib.idle_add(self._run_once, self.on_navigate_callback, "dashboard")
            elif self.on_activate_callback:
                GLib.idle_add(self._run_once, self.on_activate_callback)
        elif item_id == 2:  # Smart Boost
            if (now - self._last_boost_click_time) < 1.0:
                logger.debug("Debouncing duplicate Smart Boost click event from DBusMenu.")
                return
            self._last_boost_click_time = now
            if self.on_smart_boost_callback:
                GLib.idle_add(self._run_once, self.on_smart_boost_callback)
        elif item_id == 4:  # Cleaner
            if self.on_navigate_callback:
                GLib.idle_add(self._run_once, self.on_navigate_callback, "cleaner")
        elif item_id == 5:  # Preferences
            if self.on_preferences_callback:
                GLib.idle_add(self._run_once, self.on_preferences_callback)
        elif item_id == 7:  # Quit
            if self.on_quit_callback:
                GLib.idle_add(self._run_once, self.on_quit_callback)
