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

    def test_monitor_battery_live(self):
        """Check system monitor battery reading structure and types."""
        bat = SystemMonitor.get_battery_info()
        self.assertIn('present', bat)
        self.assertIn('real_capacity_wh', bat)
        self.assertIn('design_capacity_wh', bat)
        self.assertIn('capacity_percent', bat)
        self.assertIn('status', bat)
        self.assertIsInstance(bat['present'], bool)
        self.assertIsInstance(bat['real_capacity_wh'], (int, float))
        self.assertIsInstance(bat['design_capacity_wh'], (int, float))
        self.assertIsInstance(bat['capacity_percent'], (int, float))
        if bat['present']:
            self.assertGreater(bat['design_capacity_wh'], 0.0)
            self.assertGreaterEqual(bat['real_capacity_wh'], 0.0)
            self.assertGreaterEqual(bat['capacity_percent'], 0.0)

    def test_monitor_battery_energy_mock(self):
        """Test battery calculation when energy_full and energy_full_design are reported."""
        from unittest.mock import patch
        mock_files = {
            '/sys/class/power_supply/BAT0/type': 'Battery\n',
            '/sys/class/power_supply/BAT0/present': '1\n',
            '/sys/class/power_supply/BAT0/energy_full': '40000000\n',  # 40 Wh
            '/sys/class/power_supply/BAT0/energy_full_design': '50000000\n',  # 50 Wh
            '/sys/class/power_supply/BAT0/energy_now': '30000000\n',
            '/sys/class/power_supply/BAT0/status': 'Discharging\n',
            '/sys/class/power_supply/BAT0/model_name': 'TestModel\n',
            '/sys/class/power_supply/BAT0/manufacturer': 'TestVendor\n'
        }
        with patch('os.path.isdir', return_value=True), \
             patch('os.listdir', return_value=['BAT0']), \
             patch('mithrandir.backend.host_bridge.HostBridge.read_host_file', side_effect=lambda p: mock_files.get(p)):
            res = SystemMonitor.get_battery_info()
            self.assertTrue(res['present'])
            self.assertEqual(res['real_capacity_wh'], 40.0)
            self.assertEqual(res['design_capacity_wh'], 50.0)
            self.assertEqual(res['capacity_percent'], 80.0)
            self.assertEqual(res['status'], 'Discharging')
            self.assertEqual(res['model'], 'TestModel')
            self.assertEqual(res['vendor'], 'TestVendor')

    def test_monitor_battery_charge_mock(self):
        """Test battery calculation when charge_full and voltage are reported."""
        from unittest.mock import patch
        # 4,000,000 µAh * 12,000,000 µV / 10^12 = 48.0 Wh
        # 5,000,000 µAh * 12,000,000 µV / 10^12 = 60.0 Wh
        mock_files = {
            '/sys/class/power_supply/BAT1/type': 'Battery\n',
            '/sys/class/power_supply/BAT1/present': '1\n',
            '/sys/class/power_supply/BAT1/energy_full': None,
            '/sys/class/power_supply/BAT1/charge_full': '4000000\n',
            '/sys/class/power_supply/BAT1/charge_full_design': '5000000\n',
            '/sys/class/power_supply/BAT1/voltage_min_design': '12000000\n',
            '/sys/class/power_supply/BAT1/status': 'Full\n',
        }
        with patch('os.path.isdir', return_value=True), \
             patch('os.listdir', return_value=['BAT1']), \
             patch('mithrandir.backend.host_bridge.HostBridge.read_host_file', side_effect=lambda p: mock_files.get(p)):
            res = SystemMonitor.get_battery_info()
            self.assertTrue(res['present'])
            self.assertEqual(res['real_capacity_wh'], 48.0)
            self.assertEqual(res['design_capacity_wh'], 60.0)
            self.assertEqual(res['capacity_percent'], 80.0)
            self.assertEqual(res['status'], 'Full')

    def test_monitor_battery_absent_mock(self):
        """Test battery response when no battery is present."""
        from unittest.mock import patch
        with patch('os.path.isdir', return_value=True), \
             patch('os.listdir', return_value=[]), \
             patch('mithrandir.backend.host_bridge.HostBridge.which', return_value=None):
            res = SystemMonitor.get_battery_info()
            self.assertFalse(res['present'])
            self.assertEqual(res['real_capacity_wh'], 0.0)
            self.assertEqual(res['design_capacity_wh'], 0.0)
            self.assertEqual(res['capacity_percent'], 0.0)
            self.assertEqual(res['status'], 'N/A')

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

    def test_host_bridge_fs_queries(self):
        """Check HostBridge host file existence, dir existence, and mtime helpers."""
        from mithrandir.backend.host_bridge import HostBridge
        self.assertTrue(HostBridge.dir_exists('/etc'))
        self.assertTrue(HostBridge.file_exists('/etc/passwd') or HostBridge.file_exists('/etc/os-release'))
        self.assertFalse(HostBridge.file_exists('/nonexistent_path_mithrandir_test'))
        self.assertFalse(HostBridge.dir_exists('/nonexistent_path_mithrandir_test_dir'))
        if HostBridge.file_exists('/etc/passwd'):
            self.assertGreater(HostBridge.get_file_mtime('/etc/passwd'), 0.0)

    def test_process_manager_host_ps_parsing(self):
        """Check ProcessManager host ps stdout parser."""
        from mithrandir.backend.processes import ProcessManager
        sample_ps = (
            "         1 root                   0.0      0.1        12288 Ss       systemd                        /sbin/init splash\n"
            "       100 antonio                2.5      5.0       102400 Sl       mithrandir                     python3 -m mithrandir.main\n"
        )
        procs = ProcessManager._parse_host_ps(sample_ps)
        self.assertEqual(len(procs), 2)
        self.assertEqual(procs[0]['pid'], 1)
        self.assertEqual(procs[0]['user'], 'root')
        self.assertEqual(procs[0]['name'], 'systemd')
        self.assertEqual(procs[1]['pid'], 100)
        self.assertEqual(procs[1]['user'], 'antonio')
        self.assertEqual(procs[1]['name'], 'mithrandir')
        self.assertEqual(procs[1]['cpu_percent'], 2.5)
        self.assertEqual(procs[1]['mem_percent'], 5.0)

if __name__ == '__main__':
    unittest.main()

