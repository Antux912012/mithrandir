import os
import configparser
from typing import List, Dict, Any, Optional
from .host_bridge import HostBridge

class AutostartManager:
    """
    Manages user and system startup applications according to the
    XDG Desktop Application Autostart Specification.
    """

    USER_AUTOSTART_DIR = os.path.expanduser('~/.config/autostart')
    SYSTEM_AUTOSTART_DIR = '/etc/xdg/autostart'
    MITHRANDIR_DESKTOP_ID = 'io.github.antonio.Mithrandir.desktop'

    @classmethod
    def ensure_user_dir(cls):
        os.makedirs(cls.USER_AUTOSTART_DIR, exist_ok=True)

    @classmethod
    def list_entries(cls) -> List[Dict[str, Any]]:
        """
        Lists all autostart applications merged across ~/.config/autostart
        and /etc/xdg/autostart, respecting user overrides.
        """
        cls.ensure_user_dir()
        entries_dict = {}

        # 1. Parse system entries
        if os.path.isdir(cls.SYSTEM_AUTOSTART_DIR):
            for fname in sorted(os.listdir(cls.SYSTEM_AUTOSTART_DIR)):
                if not fname.endswith('.desktop'):
                    continue
                path = os.path.join(cls.SYSTEM_AUTOSTART_DIR, fname)
                entry = cls._parse_desktop_file(path, is_system=True)
                if entry:
                    entries_dict[fname] = entry

        # 2. Parse user entries (overrides system entries)
        if os.path.isdir(cls.USER_AUTOSTART_DIR):
            for fname in sorted(os.listdir(cls.USER_AUTOSTART_DIR)):
                if not fname.endswith('.desktop'):
                    continue
                path = os.path.join(cls.USER_AUTOSTART_DIR, fname)
                entry = cls._parse_desktop_file(path, is_system=False)
                if entry:
                    # User entry takes precedence over system entry
                    entries_dict[fname] = entry

        return list(entries_dict.values())

    @classmethod
    def _parse_desktop_file(cls, path: str, is_system: bool) -> Optional[Dict[str, Any]]:
        try:
            parser = configparser.ConfigParser(interpolation=None, strict=False)
            parser.read(path, encoding='utf-8')
            if not parser.has_section('Desktop Entry'):
                return None

            section = parser['Desktop Entry']
            fname = os.path.basename(path)
            name = section.get('Name', fname.replace('.desktop', ''))
            cmd = section.get('Exec', '')
            comment = section.get('Comment', '')
            icon = section.get('Icon', 'application-x-executable-symbolic')
            hidden = section.get('Hidden', 'false').lower() == 'true'
            gnome_enabled = section.get('X-GNOME-Autostart-enabled', 'true').lower() != 'false'

            enabled = (not hidden) and gnome_enabled

            return {
                'id': fname,
                'name': name,
                'command': cmd,
                'comment': comment,
                'icon': icon,
                'is_system': is_system,
                'enabled': enabled,
                'path': path
            }
        except Exception:
            return None

    @classmethod
    def toggle_entry(cls, entry_id: str, enable: bool) -> bool:
        """
        Toggles autostart status. If modifying a system entry, creates a user override
        desktop file in ~/.config/autostart/.
        """
        cls.ensure_user_dir()
        user_path = os.path.join(cls.USER_AUTOSTART_DIR, entry_id)

        if not os.path.exists(user_path):
            # Must be a system entry; copy to user autostart dir
            system_path = os.path.join(cls.SYSTEM_AUTOSTART_DIR, entry_id)
            if os.path.exists(system_path):
                try:
                    with open(system_path, 'r', encoding='utf-8') as f:
                        content = f.read()
                    with open(user_path, 'w', encoding='utf-8') as f:
                        f.write(content)
                except Exception:
                    return False

        if os.path.exists(user_path):
            try:
                parser = configparser.ConfigParser(interpolation=None, strict=False)
                parser.read(user_path, encoding='utf-8')
                if not parser.has_section('Desktop Entry'):
                    parser.add_section('Desktop Entry')

                parser.set('Desktop Entry', 'Hidden', 'false' if enable else 'true')
                parser.set('Desktop Entry', 'X-GNOME-Autostart-enabled', 'true' if enable else 'false')

                with open(user_path, 'w', encoding='utf-8') as f:
                    parser.write(f, space_around_delimiters=False)
                return True
            except Exception:
                return False

        return False

    @classmethod
    def add_entry(cls, name: str, command: str, comment: str = "", icon: str = "application-x-executable-symbolic") -> bool:
        """
        Creates a new user autostart entry in ~/.config/autostart/.
        """
        cls.ensure_user_dir()
        safe_name = "".join(c for c in name if c.isalnum() or c in ('-', '_')).lower() or "app"
        filename = f"{safe_name}.desktop"
        user_path = os.path.join(cls.USER_AUTOSTART_DIR, filename)

        counter = 1
        while os.path.exists(user_path):
            filename = f"{safe_name}_{counter}.desktop"
            user_path = os.path.join(cls.USER_AUTOSTART_DIR, filename)
            counter += 1

        content = f"""[Desktop Entry]
Type=Application
Name={name}
Comment={comment}
Exec={command}
Icon={icon}
Hidden=false
X-GNOME-Autostart-enabled=true
Terminal=false
"""
        try:
            with open(user_path, 'w', encoding='utf-8') as f:
                f.write(content)
            return True
        except Exception:
            return False

    @classmethod
    def remove_entry(cls, entry_id: str) -> bool:
        """
        Removes a user autostart file or hides a system entry.
        """
        cls.ensure_user_dir()
        user_path = os.path.join(cls.USER_AUTOSTART_DIR, entry_id)
        if os.path.exists(user_path):
            system_path = os.path.join(cls.SYSTEM_AUTOSTART_DIR, entry_id)
            if os.path.exists(system_path):
                # Mask it
                return cls.toggle_entry(entry_id, False)
            else:
                try:
                    os.remove(user_path)
                    return True
                except Exception:
                    return False
        else:
            return cls.toggle_entry(entry_id, False)

    @classmethod
    def is_mithrandir_autostart_enabled(cls) -> bool:
        user_path = os.path.join(cls.USER_AUTOSTART_DIR, cls.MITHRANDIR_DESKTOP_ID)
        if not os.path.exists(user_path):
            return False
        entry = cls._parse_desktop_file(user_path, is_system=False)
        return entry['enabled'] if entry else False

    @classmethod
    def set_mithrandir_autostart(cls, enable: bool, start_in_background: bool = True) -> bool:
        cls.ensure_user_dir()
        user_path = os.path.join(cls.USER_AUTOSTART_DIR, cls.MITHRANDIR_DESKTOP_ID)

        if not enable:
            if os.path.exists(user_path):
                try:
                    os.remove(user_path)
                except Exception:
                    return cls.toggle_entry(cls.MITHRANDIR_DESKTOP_ID, False)
            return True

        if HostBridge.is_flatpak():
            exec_cmd = "flatpak run io.github.antonio.Mithrandir"
        else:
            exec_cmd = "mithrandir"

        if start_in_background:
            exec_cmd += " --background"

        content = f"""[Desktop Entry]
Type=Application
Name=Mithrandir
Comment=GNU/Linux System Cleaner, Optimizer and Diagnostics
Exec={exec_cmd}
Icon=io.github.antonio.Mithrandir
Hidden=false
X-GNOME-Autostart-enabled=true
Terminal=false
"""
        try:
            with open(user_path, 'w', encoding='utf-8') as f:
                f.write(content)
            return True
        except Exception:
            return False
