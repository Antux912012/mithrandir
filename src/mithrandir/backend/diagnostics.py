import re
from typing import Dict, Any
from .host_bridge import HostBridge
from .distro import DistroDetector
from .monitor import SystemMonitor

class DiagnosticCollector:
    """
    Collects diagnostic telemetry safely and provides redaction
    to sanitize sensitive PII before forwarding to AI models.
    """

    @classmethod
    def collect_system_summary(cls) -> Dict[str, Any]:
        os_info = DistroDetector.get_os_release()
        mem = SystemMonitor.get_memory_info()
        disk = SystemMonitor.get_disk_usage('/')

        # Fetch recent journal errors (priority 3 = err)
        code, journal_errs, _ = HostBridge.run_command(['journalctl', '-p', '3', '-b', '-n', '25', '--no-pager'], timeout=6)
        if code != 0:
            journal_errs = "Unable to read journal (requires permissions)."

        # Fetch failed systemd units
        code, failed_units, _ = HostBridge.run_command(['systemctl', '--failed', '--no-legend', '--plain'], timeout=5)
        if code != 0:
            failed_units = "None or unable to query."

        return {
            'os': os_info.get('PRETTY_NAME', 'GNU/Linux'),
            'kernel': HostBridge.read_host_file('/proc/sys/kernel/osrelease') or 'Unknown',
            'desktop': DistroDetector.get_desktop_environment(),
            'memory_used_mb': mem.get('used_mb', 0),
            'memory_total_mb': mem.get('total_mb', 0),
            'memory_percent': mem.get('percent', 0),
            'disk_free_gb': disk.get('free_gb', 0),
            'disk_percent': disk.get('percent', 0),
            'failed_units': failed_units.strip() if failed_units else "None",
            'recent_errors': journal_errs.strip() if journal_errs else "No critical errors found."
        }

    @classmethod
    def redact_pii(cls, text: str) -> str:
        """
        Redacts IP addresses, user home directories, MAC addresses, and usernames.
        """
        # Redact IPv4
        text = re.sub(r'\b(?:\d{1,3}\.){3}\d{1,3}\b', '[REDACTED_IP]', text)
        # Redact MAC addresses
        text = re.sub(r'\b([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})\b', '[REDACTED_MAC]', text)
        # Redact User directories /home/<user>
        text = re.sub(r'/home/[a-zA-Z0-9_\-]+', '/home/[USER]', text)
        return text
