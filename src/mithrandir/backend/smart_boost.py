import os
import time
import threading
from typing import Dict, Any
from .host_bridge import HostBridge
from .monitor import SystemMonitor
from .cleaner import CleanerEngine


class SmartBoostEngine:
    """
    Microsoft PC Manager-inspired Smart Boost Engine.
    Monitors RAM usage and temporary/cache clutter, offering one-click
    and automated performance boost with customizable thresholds.
    """

    _temp_cache_time: float = 0.0
    _temp_cache_val: tuple[int, int] = (0, 0)
    _boost_lock: threading.Lock = threading.Lock()
    _is_boosting: bool = False
    _last_auto_boost_time: float = 0.0

    @classmethod
    def get_temporary_files_size(cls, force_refresh: bool = False) -> tuple[int, int]:
        """
        Calculates total size (bytes) and file count of cleanable temporary clutter:
        - Thumbnail cache
        - /tmp user temporary files
        Caches result for 15 seconds to avoid excessive disk I/O during 3s UI polling.
        """
        now = time.time()
        if not force_refresh and (now - cls._temp_cache_time) < 15.0:
            return cls._temp_cache_val

        total_size = 0
        total_count = 0

        # Thumbnails
        thumb_item = CleanerEngine.scan_item('thumbnails')
        total_size += thumb_item.size_bytes
        total_count += thumb_item.file_count

        # /tmp user files
        tmp_dir = '/tmp'
        try:
            if os.path.exists(tmp_dir):
                uid = os.getuid()
                for root, _dirs, files in os.walk(tmp_dir):
                    for f in files:
                        fp = os.path.join(root, f)
                        try:
                            st = os.lstat(fp)
                            if st.st_uid == uid and not os.path.islink(fp):
                                total_size += st.st_size
                                total_count += 1
                        except OSError:
                            pass
        except Exception:
            pass

        cls._temp_cache_time = now
        cls._temp_cache_val = (total_size, total_count)
        return total_size, total_count

    @classmethod
    def get_boost_status(cls) -> Dict[str, Any]:
        """
        Returns real-time status for the Smart Boost dashboard widget:
        - ram_percent: float
        - ram_used_mb: float
        - ram_total_mb: float
        - temp_size_mb: float
        - temp_count: int
        - temp_size_str: str
        """
        mem = SystemMonitor.get_memory_info()
        temp_bytes, temp_count = cls.get_temporary_files_size()
        temp_mb = round(temp_bytes / (1024 * 1024), 1)

        if temp_mb >= 1024:
            temp_str = f"{temp_mb / 1024:.2f} GB"
        elif temp_mb > 0:
            temp_str = f"{temp_mb:.1f} MB"
        else:
            temp_str = "0 MB"

        return {
            'ram_percent': mem.get('percent', 0.0),
            'ram_used_mb': mem.get('used_mb', 0.0),
            'ram_total_mb': mem.get('total_mb', 0.0),
            'temp_bytes': temp_bytes,
            'temp_size_mb': temp_mb,
            'temp_count': temp_count,
            'temp_size_str': temp_str
        }

    @classmethod
    def execute_boost(cls, clean_temp: bool = True, drop_caches: bool = True) -> Dict[str, Any]:
        """
        Performs PC Manager-style boost:
        1. Empties temporary files & thumbnails
        2. Flushes filesystem buffers and drops kernel file caches/buffers if permitted
        """
        with cls._boost_lock:
            if cls._is_boosting:
                mem_cur = SystemMonitor.get_memory_info()
                return {
                    'success': False,
                    'already_running': True,
                    'ram_reclaimed_mb': 0.0,
                    'disk_freed_mb': 0.0,
                    'current_ram_percent': mem_cur.get('percent', 0.0)
                }
            cls._is_boosting = True

        try:
            mem_before = SystemMonitor.get_memory_info()
            used_before = mem_before.get('used_mb', 0.0)

            temp_bytes_before, _ = cls.get_temporary_files_size(force_refresh=True)

            # 1. Clean temp & thumbnails
            if clean_temp:
                CleanerEngine.clean_item('thumbnails')
                try:
                    uid = os.getuid()
                    for root, _dirs, files in os.walk('/tmp'):
                        for f in files:
                            fp = os.path.join(root, f)
                            try:
                                st = os.lstat(fp)
                                if st.st_uid == uid and not os.path.islink(fp):
                                    os.remove(fp)
                            except OSError:
                                pass
                except Exception:
                    pass

            # 2. Release inactive memory cache & buffers
            if drop_caches:
                HostBridge.run_command(['sync'], timeout=5)
                # Attempt non-interactive sudo -n first, or direct write if running with privileges
                HostBridge.run_command(
                    ['sh', '-c', 'sudo -n sh -c "echo 3 > /proc/sys/vm/drop_caches && echo 1 > /proc/sys/vm/compact_memory" 2>/dev/null || (echo 3 > /proc/sys/vm/drop_caches 2>/dev/null || true)'],
                    use_pkexec=False,
                    timeout=5
                )

            time.sleep(0.2)
            mem_after = SystemMonitor.get_memory_info()
            used_after = mem_after.get('used_mb', 0.0)

            temp_bytes_after, _ = cls.get_temporary_files_size(force_refresh=True)

            ram_reclaimed = max(0.0, round(used_before - used_after, 1))
            disk_freed_mb = max(0.0, round((temp_bytes_before - temp_bytes_after) / (1024 * 1024), 1))

            return {
                'success': True,
                'already_running': False,
                'ram_reclaimed_mb': ram_reclaimed,
                'disk_freed_mb': disk_freed_mb,
                'current_ram_percent': mem_after.get('percent', 0.0)
            }
        finally:
            with cls._boost_lock:
                cls._is_boosting = False

    @classmethod
    def evaluate_auto_boost(cls, settings) -> tuple[bool, Dict[str, Any]]:
        """
        Checks if auto boost should trigger based on GSettings rules:
        - smart-boost-enabled (bool)
        - smart-boost-ram-threshold (int %)
        - smart-boost-temp-threshold-mb (int MB)
        - smart-boost-interval-min (int minutes, cooldown)
        """
        try:
            enabled = settings.get_boolean('smart-boost-enabled')
        except Exception:
            enabled = True

        if not enabled:
            return False, {}

        try:
            interval_min = max(5, settings.get_int('smart-boost-interval-min'))
        except Exception:
            interval_min = 15

        now = time.time()
        if (now - cls._last_auto_boost_time) < (interval_min * 60):
            return False, {}

        try:
            ram_threshold = settings.get_int('smart-boost-ram-threshold')
        except Exception:
            ram_threshold = 80

        try:
            temp_threshold = settings.get_int('smart-boost-temp-threshold-mb')
        except Exception:
            temp_threshold = 1024

        status = cls.get_boost_status()
        should_boost = (status['ram_percent'] >= ram_threshold) or (status['temp_size_mb'] >= temp_threshold)

        if should_boost:
            cls._last_auto_boost_time = now
            result = cls.execute_boost()
            if not result.get('success', False):
                return False, {}
            return True, {
                'ram_reclaimed_mb': result['ram_reclaimed_mb'],
                'disk_freed_mb': result['disk_freed_mb'],
                'reason': f"RAM at {status['ram_percent']}% (threshold {ram_threshold}%) or Temp files at {status['temp_size_mb']} MB (threshold {temp_threshold} MB)"
            }

        return False, {}
