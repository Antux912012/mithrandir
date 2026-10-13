import os
import shutil
import glob
from dataclasses import dataclass
from typing import List, Callable, Optional
from .host_bridge import HostBridge

@dataclass
class CleanerItem:
    id: str
    name: str
    description: str
    category: str
    size_bytes: int = 0
    file_count: int = 0
    requires_root: bool = False
    is_safe: bool = True

class CleanerEngine:
    """
    Cleaner Engine inspired by BleachBit and Stacer.
    Supports dry-run scanning, path protection, Flatpaks, and Snaps.
    """

    PROTECTED_PATHS = [
        '/', '/home', '/etc', '/usr', '/bin', '/sbin', '/lib', '/lib64',
        '/var', '/root', '/boot', '/dev', '/sys', '/proc'
    ]

    @classmethod
    def get_user_cache_dir(cls) -> str:
        # In Flatpak sandbox, XDG_CACHE_HOME points to ~/.var/app/<app-id>/cache.
        # For a system cleaner, the target user cache on the host is ~/.cache.
        if HostBridge.is_flatpak():
            return os.path.expanduser('~/.cache')
        return os.path.expanduser(os.environ.get('XDG_CACHE_HOME', '~/.cache'))

    @classmethod
    def get_trash_dir(cls) -> str:
        return os.path.expanduser('~/.local/share/Trash')

    @classmethod
    def get_stremio_cache_dirs(cls) -> List[str]:
        """
        Locates Stremio torrent and streaming cache directories
        across native, Flatpak, and Snap installations.
        """
        dirs = []
        candidates = [
            os.path.expanduser('~/.stremio-server/stremio-cache'),
            os.path.expanduser('~/.local/share/Smart Code ltd/Stremio/stremio-cache'),
            os.path.expanduser('~/.local/share/stremio/stremio-cache'),
            os.path.expanduser('~/.var/app/com.stremio.Stremio/.stremio-server/stremio-cache'),
            os.path.expanduser('~/.var/app/com.stremio.Stremio/data/Smart Code ltd/Stremio/stremio-cache'),
            os.path.expanduser('~/.var/app/com.stremio.Stremio/cache'),
            os.path.expanduser('~/snap/stremio/common/.stremio-server/stremio-cache'),
        ]
        # Check custom cacheRoot from settings.json if present
        for settings_path in [
            os.path.expanduser('~/.stremio-server/settings.json'),
            os.path.expanduser('~/.var/app/com.stremio.Stremio/.stremio-server/settings.json')
        ]:
            if os.path.isfile(settings_path):
                try:
                    import json
                    with open(settings_path, 'r', encoding='utf-8') as f:
                        data = json.load(f)
                        custom_root = data.get('cacheRoot')
                        if custom_root and os.path.isdir(custom_root) and custom_root not in candidates:
                            candidates.append(custom_root)
                except Exception:
                    pass

        for p in candidates:
            if os.path.exists(p) and p not in dirs:
                dirs.append(p)
        return dirs

    @classmethod
    def _is_path_safe(cls, path: str) -> bool:
        normalized = os.path.realpath(os.path.abspath(path))
        home = os.path.realpath(os.path.expanduser('~'))
        if normalized in cls.PROTECTED_PATHS or normalized == home:
            return False
        for p in cls.PROTECTED_PATHS:
            if normalized == p:
                return False
        return True

    @classmethod
    def _calc_dir_size(cls, path: str) -> tuple[int, int]:
        total_size = 0
        total_count = 0
        if not os.path.exists(path):
            return 0, 0
        try:
            for root, dirs, files in os.walk(path):
                for f in files:
                    fp = os.path.join(root, f)
                    try:
                        if not os.path.islink(fp):
                            total_size += os.path.getsize(fp)
                        total_count += 1
                    except OSError:
                        pass
        except OSError:
            pass
        return total_size, total_count

    @classmethod
    def scan_item(cls, item_id: str) -> CleanerItem:
        if item_id == 'user_cache':
            cache_dir = cls.get_user_cache_dir()
            total_size = 0
            total_count = 0
            if os.path.exists(cache_dir):
                for entry in os.listdir(cache_dir):
                    if entry == 'thumbnails':
                        continue
                    ep = os.path.join(cache_dir, entry)
                    if os.path.isdir(ep):
                        s, c = cls._calc_dir_size(ep)
                    else:
                        try:
                            s = os.path.getsize(ep) if not os.path.islink(ep) else 0
                        except OSError:
                            s = 0
                        c = 1
                    total_size += s
                    total_count += c
            return CleanerItem(
                id='user_cache',
                name='User Application Cache',
                description=f'Temporary files in {cache_dir}',
                category='User',
                size_bytes=total_size,
                file_count=total_count,
                requires_root=False
            )
        elif item_id == 'flatpak_cache':
            total_size = 0
            total_count = 0
            var_app = os.path.expanduser('~/.var/app')
            apps_found = []
            if os.path.exists(var_app):
                try:
                    apps_found = [
                        a for a in os.listdir(var_app)
                        if a not in ['io.github.antonio.Mitrandil', 'io.github.antonio.Mithrandir']
                    ]
                except OSError:
                    apps_found = []

            if apps_found:
                for app in apps_found:
                    c_dir = os.path.join(var_app, app, 'cache')
                    if os.path.exists(c_dir):
                        s, c = cls._calc_dir_size(c_dir)
                        total_size += s
                        total_count += c
            else:
                # Fallback to host command if ~/.var/app is isolated in sandbox
                code, out, _ = HostBridge.run_command(
                    ['sh', '-c', 'du -sk ~/.var/app/*/cache 2>/dev/null'],
                    timeout=6
                )
                if code == 0 and out.strip():
                    for line in out.strip().splitlines():
                        parts = line.split()
                        if len(parts) >= 2 and not any(m in parts[1] for m in ['Mithrandir', 'Mitrandil']):
                            try:
                                total_size += int(parts[0]) * 1024
                                total_count += 1
                            except ValueError:
                                pass
            return CleanerItem(
                id='flatpak_cache',
                name='Flatpak Applications Cache',
                description='Temporary cache data stored by installed Flatpaks (~/.var/app)',
                category='Applications',
                size_bytes=total_size,
                file_count=total_count,
                requires_root=False
            )
        elif item_id == 'thumbnails':
            thumb_dir = os.path.join(cls.get_user_cache_dir(), 'thumbnails')
            size, count = cls._calc_dir_size(thumb_dir)
            return CleanerItem(
                id='thumbnails',
                name='Thumbnail Cache',
                description='Cached image and video preview thumbnails',
                category='User',
                size_bytes=size,
                file_count=count,
                requires_root=False
            )
        elif item_id == 'trash':
            trash_dir = cls.get_trash_dir()
            size, count = cls._calc_dir_size(trash_dir)
            return CleanerItem(
                id='trash',
                name='Trash Bin',
                description='Deleted files in user trash',
                category='User',
                size_bytes=size,
                file_count=count,
                requires_root=False
            )
        elif item_id == 'systemd_journal':
            code, out, _ = HostBridge.run_command(['journalctl', '--disk-usage'], timeout=5)
            size = 0
            if code == 0 and 'take up' in out:
                parts = out.split('take up')
                if len(parts) > 1:
                    size_str = parts[1].strip().split()[0]
                    size = cls._parse_human_size(size_str)
            return CleanerItem(
                id='systemd_journal',
                name='Systemd Journal Logs',
                description='Archived logs from systemd journal',
                category='System',
                size_bytes=size,
                file_count=1,
                requires_root=True
            )
        elif item_id == 'package_cache':
            size = 0
            code, out, _ = HostBridge.run_command(['sh', '-c', 'du -sb /var/cache/apt/archives 2>/dev/null || true'], timeout=5)
            if out.strip():
                try:
                    size = int(out.split()[0])
                except (ValueError, IndexError):
                    pass
            if size == 0:
                code, out, _ = HostBridge.run_command(['sh', '-c', 'du -sb /var/cache/pacman/pkg 2>/dev/null || true'], timeout=5)
                if out.strip():
                    try:
                        size = int(out.split()[0])
                    except (ValueError, IndexError):
                        pass
            if size == 0:
                code, out, _ = HostBridge.run_command(['sh', '-c', 'du -sb /var/cache/dnf 2>/dev/null || true'], timeout=5)
                if out.strip():
                    try:
                        size = int(out.split()[0])
                    except (ValueError, IndexError):
                        pass
            return CleanerItem(
                id='package_cache',
                name='Package Manager Cache',
                description='Downloaded .deb / .rpm / pacman package archives',
                category='System',
                size_bytes=size,
                file_count=1 if size > 0 else 0,
                requires_root=True
            )
        elif item_id == 'flatpak_unused':
            code, out, _ = HostBridge.run_command(['flatpak', 'uninstall', '--unused'], timeout=5)
            count = 0
            if code == 0 and out:
                lines = [l.strip() for l in out.splitlines() if l.strip() and not l.strip().startswith('These runtimes') and not l.strip().startswith('Nothing unused') and not l.strip().startswith('runtime/')]
                count = len(lines)
            return CleanerItem(
                id='flatpak_unused',
                name='Unused Flatpak Runtimes',
                description='Runtimes and extensions no longer required by any Flatpak',
                category='Applications',
                size_bytes=0,
                file_count=count,
                requires_root=False
            )
        elif item_id == 'snap_cache':
            size = 0
            count = 0
            # User snap caches
            snap_user = os.path.expanduser('~/snap')
            if os.path.isdir(snap_user):
                for pattern in [os.path.join(snap_user, '*', 'common', '.cache'), os.path.join(snap_user, '*', '*', '.cache')]:
                    for p in glob.glob(pattern):
                        s, c = cls._calc_dir_size(p)
                        size += s
                        count += c

            # System snap cache
            code, out, _ = HostBridge.run_command(['sh', '-c', 'du -sb /var/lib/snapd/cache 2>/dev/null || true'], timeout=5)
            if out.strip():
                try:
                    size += int(out.split()[0])
                    count += 1
                except (ValueError, IndexError):
                    pass
            return CleanerItem(
                id='snap_cache',
                name='Snap Packages Cache',
                description='Snap user cache (~/snap), archives, and disabled revisions',
                category='Applications',
                size_bytes=size,
                file_count=count if count > 0 else (1 if size > 0 else 0),
                requires_root=False
            )
        elif item_id == 'stremio_cache':
            total_size = 0
            total_count = 0
            for d in cls.get_stremio_cache_dirs():
                s, c = cls._calc_dir_size(d)
                total_size += s
                total_count += c
            return CleanerItem(
                id='stremio_cache',
                name='Stremio Torrent and Movie Cache',
                description='Cached torrent chunks and video streaming buffers',
                category='Media and Streaming',
                size_bytes=total_size,
                file_count=total_count,
                requires_root=False
            )
        return CleanerItem(id=item_id, name=item_id, description="", category="Other")

    @classmethod
    def clean_item(cls, item_id: str) -> tuple[bool, str]:
        if item_id == 'thumbnails':
            thumb_dir = os.path.join(cls.get_user_cache_dir(), 'thumbnails')
            if cls._is_path_safe(thumb_dir) and os.path.exists(thumb_dir):
                shutil.rmtree(thumb_dir, ignore_errors=True)
                os.makedirs(thumb_dir, exist_ok=True)
            return True, "Thumbnail cache cleared."
        elif item_id == 'trash':
            trash_dir = cls.get_trash_dir()
            files_dir = os.path.join(trash_dir, 'files')
            info_dir = os.path.join(trash_dir, 'info')
            if cls._is_path_safe(trash_dir):
                shutil.rmtree(files_dir, ignore_errors=True)
                shutil.rmtree(info_dir, ignore_errors=True)
                os.makedirs(files_dir, exist_ok=True)
                os.makedirs(info_dir, exist_ok=True)
                HostBridge.run_command(['gio', 'trash', '--empty'])
                return True, "Trash emptied."
        elif item_id == 'user_cache':
            cache_dir = cls.get_user_cache_dir()
            count = 0
            if cls._is_path_safe(cache_dir) and os.path.exists(cache_dir):
                for entry in os.listdir(cache_dir):
                    if entry == 'thumbnails':
                        continue
                    ep = os.path.join(cache_dir, entry)
                    try:
                        if os.path.isdir(ep):
                            shutil.rmtree(ep, ignore_errors=True)
                        else:
                            os.remove(ep)
                        count += 1
                    except OSError:
                        pass
                if HostBridge.is_flatpak():
                    HostBridge.run_command(['sh', '-c', f'find "{cache_dir}" -mindepth 1 -maxdepth 1 ! -name thumbnails -exec rm -rf {{}} + 2>/dev/null || true'])
                return True, f"Cleaned {count} cache folders."
        elif item_id == 'flatpak_cache':
            var_app = os.path.expanduser('~/.var/app')
            apps_found = []
            if os.path.exists(var_app):
                try:
                    apps_found = [
                        a for a in os.listdir(var_app)
                        if a not in ['io.github.antonio.Mitrandil', 'io.github.antonio.Mithrandir']
                    ]
                except OSError:
                    apps_found = []

            if apps_found:
                cleaned_apps = 0
                for app in apps_found:
                    c_dir = os.path.join(var_app, app, 'cache')
                    if os.path.exists(c_dir) and cls._is_path_safe(c_dir):
                        try:
                            for entry in os.listdir(c_dir):
                                ep = os.path.join(c_dir, entry)
                                try:
                                    if os.path.isdir(ep):
                                        shutil.rmtree(ep, ignore_errors=True)
                                    else:
                                        os.remove(ep)
                                except OSError:
                                    pass
                            cleaned_apps += 1
                        except OSError:
                            pass
                return True, f"Cleaned cache for {cleaned_apps} Flatpak applications."
            else:
                clean_cmd = [
                    'sh', '-c',
                    'for d in ~/.var/app/*/cache; do case "$d" in *io.github.antonio.Mithrandir*|*io.github.antonio.Mitrandil*) ;; *) rm -rf "$d"/* ;; esac; done'
                ]
                code, out, err = HostBridge.run_command(clean_cmd, timeout=15)
                return (code == 0), "Flatpak applications cache cleaned." if code == 0 else err
        elif item_id == 'systemd_journal':
            code, out, err = HostBridge.run_command(['journalctl', '--vacuum-time=2d', '--vacuum-size=20M'], use_pkexec=True)
            return (code == 0), out if code == 0 else err
        elif item_id == 'package_cache':
            code, _, _ = HostBridge.run_command(['which', 'apt-get'])
            if code == 0:
                c, out, err = HostBridge.run_command(['apt-get', 'clean'], use_pkexec=True)
                return (c == 0), "APT cache cleaned." if c == 0 else err
            code, _, _ = HostBridge.run_command(['which', 'dnf'])
            if code == 0:
                c, out, err = HostBridge.run_command(['dnf', 'clean', 'all'], use_pkexec=True)
                return (c == 0), "DNF cache cleaned." if c == 0 else err
            code, _, _ = HostBridge.run_command(['which', 'pacman'])
            if code == 0:
                c, out, err = HostBridge.run_command(['pacman', '-Sc', '--noconfirm'], use_pkexec=True)
                return (c == 0), "Pacman cache cleaned." if c == 0 else err
            return False, "No package cache found."
        elif item_id == 'flatpak_unused':
            code, out, err = HostBridge.run_command(['flatpak', 'uninstall', '--unused', '-y'])
            return (code == 0), "Unused Flatpaks removed." if code == 0 else err
        elif item_id == 'snap_cache':
            # Clean user snap caches first (no root needed)
            snap_user = os.path.expanduser('~/snap')
            user_cleaned = 0
            if os.path.isdir(snap_user):
                for pattern in [os.path.join(snap_user, '*', 'common', '.cache'), os.path.join(snap_user, '*', '*', '.cache')]:
                    for p in glob.glob(pattern):
                        if cls._is_path_safe(p) and os.path.isdir(p):
                            shutil.rmtree(p, ignore_errors=True)
                            os.makedirs(p, exist_ok=True)
                            user_cleaned += 1

            # System snap cache and disabled revisions
            clean_cmd = [
                'sh', '-c',
                'rm -rf /var/lib/snapd/cache/*; snap list --all 2>/dev/null | awk \'/disabled/{print $1, $3}\' | while read name rev; do [ -n "$name" ] && snap remove "$name" --revision="$rev" 2>/dev/null; done || true'
            ]
            code, out, err = HostBridge.run_command(clean_cmd, use_pkexec=True)
            msg = f"Snap cache cleaned ({user_cleaned} app caches cleared)."
            return True, msg
        elif item_id == 'stremio_cache':
            cleaned_dirs = 0
            for d in cls.get_stremio_cache_dirs():
                if cls._is_path_safe(d) and os.path.isdir(d):
                    for entry in os.listdir(d):
                        ep = os.path.join(d, entry)
                        try:
                            if os.path.isdir(ep):
                                shutil.rmtree(ep, ignore_errors=True)
                            else:
                                os.remove(ep)
                        except OSError:
                            pass
                    cleaned_dirs += 1
            return True, f"Cleaned Stremio torrent & movie cache in {cleaned_dirs} locations."

        return False, "Unknown item."

    @staticmethod
    def _parse_human_size(text: str) -> int:
        units = {'B': 1, 'K': 1024, 'M': 1024**2, 'G': 1024**3, 'T': 1024**4}
        text = text.strip().upper()
        if not text:
            return 0
        try:
            num = float(''.join([c for c in text if c.isdigit() or c == '.']))
            unit = text[-1]
            return int(num * units.get(unit, 1))
        except Exception:
            return 0
