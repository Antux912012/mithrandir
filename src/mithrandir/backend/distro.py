import os
from typing import Dict, Optional
from .host_bridge import HostBridge

class DistroDetector:
    """
    Detects Linux distribution, desktop environment, kernel, architecture, and package managers.
    """

    @classmethod
    def get_os_release(cls) -> Dict[str, str]:
        content = HostBridge.read_host_file('/etc/os-release')
        info = {}
        if content:
            for line in content.splitlines():
                if '=' in line:
                    k, v = line.split('=', 1)
                    info[k.strip()] = v.strip().strip('"').strip("'")
        return info

    @classmethod
    def get_distro_name(cls) -> str:
        data = cls.get_os_release()
        return data.get('PRETTY_NAME', data.get('NAME', 'Linux'))

    @classmethod
    def get_full_system_info(cls) -> Dict[str, str]:
        os_data = cls.get_os_release()

        # Hostname
        hostname = HostBridge.read_host_file('/etc/hostname')
        hostname = hostname.strip() if hostname else 'localhost'

        # Kernel
        kernel = HostBridge.read_host_file('/proc/sys/kernel/osrelease')
        kernel = kernel.strip() if kernel else 'Unknown'

        # CPU model
        cpuinfo = HostBridge.read_host_file('/proc/cpuinfo')
        cpu_model = 'Unknown'
        if cpuinfo:
            for line in cpuinfo.splitlines():
                if 'model name' in line:
                    cpu_model = line.split(':', 1)[1].strip()
                    break

        # Architecture
        code, arch, _ = HostBridge.run_command(['uname', '-m'], timeout=2)
        arch = arch.strip() if code == 0 and arch else 'x86_64'

        # Uptime
        uptime_str = 'Unknown'
        uptime_raw = HostBridge.read_host_file('/proc/uptime')
        if uptime_raw:
            try:
                seconds = int(float(uptime_raw.split()[0]))
                hours, remainder = divmod(seconds, 3600)
                minutes, _ = divmod(remainder, 60)
                uptime_str = f"{hours}h {minutes}m"
            except Exception:
                pass

        return {
            'os': os_data.get('PRETTY_NAME', 'GNU/Linux'),
            'distro_id': os_data.get('ID', 'linux'),
            'kernel': kernel,
            'arch': arch,
            'hostname': hostname,
            'cpu_model': cpu_model,
            'desktop': cls.get_desktop_environment(),
            'uptime': uptime_str
        }

    @classmethod
    def detect_package_managers(cls) -> list[str]:
        pms = []
        checks = [
            ('apt', ['which', 'apt-get']),
            ('dpkg', ['which', 'dpkg']),
            ('dnf', ['which', 'dnf']),
            ('rpm', ['which', 'rpm']),
            ('pacman', ['which', 'pacman']),
            ('zypper', ['which', 'zypper']),
            ('flatpak', ['which', 'flatpak']),
            ('snap', ['which', 'snap'])
        ]
        for name, cmd in checks:
            code, _, _ = HostBridge.run_command(cmd, timeout=3)
            if code == 0:
                pms.append(name)
        return pms

    @classmethod
    def get_desktop_environment(cls) -> str:
        de = os.environ.get('XDG_CURRENT_DESKTOP', '')
        if not de:
            de = os.environ.get('DESKTOP_SESSION', 'Unknown')
        return de
