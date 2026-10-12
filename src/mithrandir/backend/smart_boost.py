import os
import shutil
import glob
import time
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

    @classmethod
    def get_temporary_files_size(cls) -> tuple[int, int]:
        """
        Calculates total size (bytes) and file count of cleanable temporary clutter:
        - Thumbnail cache
        - User cache fragments
        - Stremio torrent cache (if exists)
        - /tmp user temporary files
        """
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
                for root, dirs, files in os.walk(tmp_dir):
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
        2. Drops kernel file caches/buffers to release standby RAM
        3. Compacts memory
        """
        mem_before = SystemMonitor.get_memory_info()
        used_before = mem_before.get('used_mb', 0.0)

        temp_bytes_before, _ = cls.get_temporary_files_size()

        # 1. Clean temp & thumbnails
        if clean_temp:
            CleanerEngine.clean_item('thumbnails')
            # Clean user temporary files in /tmp
            try:
                uid = os.getuid()
                for root, dirs, files in os.walk('/tmp'):
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
            # Sync filesystem before dropping cache
            HostBridge.run_command(['sync'])
            # Drop clean caches via pkexec
            HostBridge.run_command(['sh', '-c', 'echo 3 > /proc/sys/vm/drop_caches 2>/dev/null || true'], use_pkexec=False)
            HostBridge.run_command(['sh', '-c', 'echo 1 > /proc/sys/vm/compact_memory 2>/dev/null || true'], use_pkexec=False)

        time.sleep(0.3)
        mem_after = SystemMonitor.get_memory_info()
        used_after = mem_after.get('used_mb', 0.0)

        temp_bytes_after, _ = cls.get_temporary_files_size()

        ram_reclaimed = max(0.0, round(used_before - used_after, 1))
        disk_freed_mb = max(0.0, round((temp_bytes_before - temp_bytes_after) / (1024 * 1024), 1))

        return {
            'success': True,
            'ram_reclaimed_mb': ram_reclaimed,
            'disk_freed_mb': disk_freed_mb,
            'current_ram_percent': mem_after.get('percent', 0.0)
        }

    @classmethod
    def evaluate_auto_boost(cls, settings) -> tuple[bool, Dict[str, Any]]:
        """
        Checks if auto boost should trigger based on GSettings rules:
        - smart-boost-enabled (bool)
        - smart-boost-ram-threshold (int %)
        - smart-boost-temp-threshold-mb (int MB)
        """
        try:
            enabled = settings.get_boolean('smart-boost-enabled')
        except Exception:
            enabled = True

        if not enabled:
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
            result = cls.execute_boost()
            return True, {
                'ram_reclaimed_mb': result['ram_reclaimed_mb'],
                'disk_freed_mb': result['disk_freed_mb'],
                'reason': f"RAM at {status['ram_percent']}% (threshold {ram_threshold}%) or Temp files at {status['temp_size_mb']} MB (threshold {temp_threshold} MB)"
            }

        return False, {}
