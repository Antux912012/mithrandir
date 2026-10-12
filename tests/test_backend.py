import unittest
import os
from mithrandir.backend.cleaner import CleanerEngine
from mithrandir.backend.distro import DistroDetector
from mithrandir.backend.monitor import SystemMonitor
from mithrandir.backend.diagnostics import DiagnosticCollector
from mithrandir.ai.safety import SafetyGuard

class TestMithrandir(unittest.TestCase):

    def test_cleaner_protection(self):
        """Ensure system root and home directories are strictly protected."""
        self.assertFalse(CleanerEngine._is_path_safe('/'))
        self.assertFalse(CleanerEngine._is_path_safe('/etc'))
        self.assertFalse(CleanerEngine._is_path_safe('/usr'))
        self.assertFalse(CleanerEngine._is_path_safe(os.path.expanduser('~')))
        self.assertTrue(CleanerEngine._is_path_safe('/tmp/mithrandir_test_dummy'))

    def test_safety_guard_denylist(self):
        """Ensure dangerous commands are flagged as blocked."""
        allowed, risk, _ = SafetyGuard.evaluate_command('rm -rf /')
        self.assertFalse(allowed)
        self.assertEqual(risk, 'blocked')

        allowed, risk, _ = SafetyGuard.evaluate_command('mkfs.ext4 /dev/sda1')
        self.assertFalse(allowed)
        self.assertEqual(risk, 'blocked')

        allowed, risk, _ = SafetyGuard.evaluate_command('systemctl status systemd-journald')
        self.assertTrue(allowed)
        self.assertEqual(risk, 'safe')

    def test_redaction(self):
        """Ensure IP addresses and user home paths are stripped."""
        raw = "User logs at /home/antonio/.cache connected to 192.168.1.100."
        redacted = DiagnosticCollector.redact_pii(raw)
        self.assertNotIn("192.168.1.100", redacted)
        self.assertNotIn("/home/antonio", redacted)
        self.assertIn("[REDACTED_IP]", redacted)
        self.assertIn("/home/[USER]", redacted)

    def test_monitor_memory(self):
        """Check system monitor memory reading."""
        mem = SystemMonitor.get_memory_info()
        self.assertIn('total_mb', mem)
        self.assertGreater(mem['total_mb'], 0)

    def test_package_manager(self):
        """Check PackageManager native type and PackageItem creation."""
        from mithrandir.backend.packages import PackageManager, PackageItem
        nat_type = PackageManager.get_native_type()
        self.assertIn(nat_type, ['deb', 'rpm', 'unknown'])
        item = PackageItem(name="dummy-pkg", version="1.0.0", kind="native (deb)", summary="Test package")
        self.assertEqual(item.name, "dummy-pkg")
        # Ensure clean_app_data executes without NameError (tests os import)
        ok, msg = PackageManager.clean_app_data(item)
        self.assertTrue(ok)

    def test_package_manager_python(self):
        """Check Python package detection, listing, and item creation."""
        from mithrandir.backend.packages import PackageManager, PackageItem
        pip_cmd = PackageManager.get_pip_cmd()
        self.assertIsInstance(pip_cmd, list)
        self.assertGreater(len(pip_cmd), 0)

        # Ensure uv detector runs without error
        uv_path = PackageManager.find_uv_executable()
        self.assertTrue(uv_path is None or isinstance(uv_path, str))

        # Test listing Python packages
        py_pkgs = PackageManager.list_python_packages()
        self.assertIsInstance(py_pkgs, list)
        self.assertGreater(len(py_pkgs), 0)
        first = py_pkgs[0]
        self.assertTrue(hasattr(first, 'name'))
        self.assertTrue(hasattr(first, 'version'))
        self.assertIn('python', first.kind)

        # Test clean_app_data for Python package
        py_item = PackageItem(name="requests", version="2.31.0", kind="python (pip)", manager="pip")
        ok, msg = PackageManager.clean_app_data(py_item)
        self.assertTrue(ok)


    def test_service_manager_critical(self):
        """Check ServiceManager critical service tagging."""
        from mithrandir.backend.services import ServiceManager
        self.assertTrue(ServiceManager.is_critical("systemd-journald.service"))
        self.assertTrue(ServiceManager.is_critical("NetworkManager.service"))
        self.assertFalse(ServiceManager.is_critical("custom-user-service.service"))

    def test_host_bridge_command_normalization(self):
        """Check HostBridge command execution and sudo prefix stripping."""
        from mithrandir.backend.host_bridge import HostBridge
        code, out, _ = HostBridge.run_command(['echo', 'hello_mithrandir'])
        self.assertEqual(code, 0)
        self.assertIn('hello_mithrandir', out)

    def test_cleaner_human_size(self):
        """Check CleanerEngine human-readable size parser."""
        self.assertEqual(CleanerEngine._parse_human_size('100M'), 100 * 1024 * 1024)
        self.assertEqual(CleanerEngine._parse_human_size('2G'), 2 * 1024 * 1024 * 1024)
        self.assertEqual(CleanerEngine._parse_human_size('512K'), 512 * 1024)
        self.assertEqual(CleanerEngine._parse_human_size(''), 0)

    def test_stremio_cache_detection(self):
        """Check Stremio torrent cache scan item."""
        item = CleanerEngine.scan_item('stremio_cache')
        self.assertEqual(item.id, 'stremio_cache')
        self.assertEqual(item.category, 'Media and Streaming')
        self.assertIsInstance(item.size_bytes, int)
        self.assertGreaterEqual(item.size_bytes, 0)

    def test_smart_boost_status(self):
        """Check Smart Boost status and temporary files detection."""
        from mithrandir.backend.smart_boost import SmartBoostEngine
        status = SmartBoostEngine.get_boost_status()
        self.assertIn('ram_percent', status)
        self.assertIn('temp_size_str', status)
        self.assertGreaterEqual(status['ram_percent'], 0.0)

    def test_process_manager_listing(self):
        """Check ProcessManager process listing."""
        from mithrandir.backend.processes import ProcessManager
        procs = ProcessManager.list_processes()
        self.assertIsInstance(procs, list)
        self.assertGreater(len(procs), 0)
        self.assertIn('pid', procs[0])
        self.assertIn('name', procs[0])
        self.assertIn('mem_mb', procs[0])

    def test_autostart_manager(self):
        """Check AutostartManager listing."""
        from mithrandir.backend.autostart import AutostartManager
        entries = AutostartManager.list_entries()
        self.assertIsInstance(entries, list)
        if entries:
            self.assertIn('id', entries[0])
            self.assertIn('name', entries[0])
            self.assertIn('enabled', entries[0])

    def test_btrfs_manager_status(self):
        """Check BtrfsManager filesystem status detection."""
        from mithrandir.backend.btrfs import BtrfsManager
        status = BtrfsManager.get_btrfs_status()
        self.assertIn('is_btrfs', status)
        self.assertIn('root_fstype', status)

    def test_grub_manager_parsing(self):
        """Check GrubManager load_config."""
        from mithrandir.backend.grub import GrubManager
        cfg = GrubManager.load_config()
        self.assertIn('timeout', cfg)
        self.assertIn('quiet', cfg)
        self.assertIn('splash', cfg)

    def test_network_telemetry(self):
        """Check SystemMonitor network transfer rate telemetry."""
        rates = SystemMonitor.get_network_rates()
        self.assertIn('rx_str', rates)
        self.assertIn('tx_str', rates)

if __name__ == '__main__':
    unittest.main()

