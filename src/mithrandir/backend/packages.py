import json
import os
import shutil
from dataclasses import dataclass
from typing import List, Optional, Tuple
from .host_bridge import HostBridge

@dataclass
class PackageItem:
    name: str
    version: str
    kind: str  # 'native' (.deb / .rpm), 'flatpak', 'snap', 'python (pip)', 'python (uv)'
    summary: str = ""
    size_str: str = ""
    manager: str = ""  # 'pip', 'uv', 'uv-tool', etc.

class PackageManager:
    """
    Unified package uninstaller and manager supporting:
    - Native packages (.deb via apt/dpkg, .rpm via dnf/rpm)
    - Flatpak applications
    - Snap packages
    - Python packages and CLI tools (pip / uv)
    - App cache / data cleaner for specific applications
    """

    @classmethod
    def get_native_type(cls) -> str:
        code, _, _ = HostBridge.run_command(['which', 'dpkg'])
        if code == 0:
            return 'deb'
        code, _, _ = HostBridge.run_command(['which', 'rpm'])
        if code == 0:
            return 'rpm'
        return 'unknown'

    @classmethod
    def find_uv_executable(cls) -> Optional[str]:
        """Detects if uv is installed on the host."""
        code, out, _ = HostBridge.run_command(['which', 'uv'])
        if code == 0 and out.strip():
            return out.strip().splitlines()[0]

        for candidate in [
            os.path.expanduser('~/.cargo/bin/uv'),
            os.path.expanduser('~/.local/bin/uv'),
            '/usr/local/bin/uv',
            '/usr/bin/uv'
        ]:
            if HostBridge.is_flatpak():
                code, _, _ = HostBridge.run_command(['test', '-x', candidate])
                if code == 0:
                    return candidate
            elif os.path.exists(candidate) and os.access(candidate, os.X_OK):
                return candidate

        return None

    @classmethod
    def get_pip_cmd(cls) -> List[str]:
        """Detects the host pip command, preferring python3 -m pip."""
        code, _, _ = HostBridge.run_command(['python3', '-m', 'pip', '--version'], timeout=5)
        if code == 0:
            return ['python3', '-m', 'pip']
        code, _, _ = HostBridge.run_command(['pip3', '--version'], timeout=5)
        if code == 0:
            return ['pip3']
        code, _, _ = HostBridge.run_command(['pip', '--version'], timeout=5)
        if code == 0:
            return ['pip']
        return []

    @classmethod
    def list_python_packages(cls) -> List[PackageItem]:
        """
        Lists Python packages installed via pip or uv.
        Detects standalone uv tools as well as user/system environment pip packages.
        """
        items: List[PackageItem] = []
        seen_names = set()

        # 1. Check uv tools if uv is installed
        uv_bin = cls.find_uv_executable()
        if uv_bin:
            code, out, _ = HostBridge.run_command([uv_bin, 'tool', 'list'], timeout=8)
            if code == 0 and out:
                for line in out.splitlines():
                    line = line.strip()
                    if line and not line.startswith('-'):
                        parts = line.split()
                        if parts:
                            tool_name = parts[0]
                            tool_ver = parts[1].lstrip('v') if len(parts) > 1 else ""
                            seen_names.add(tool_name.lower())
                            items.append(PackageItem(
                                name=tool_name,
                                version=tool_ver,
                                kind='python (uv)',
                                summary='Installed CLI tool via uv',
                                manager='uv-tool'
                            ))

        # 2. Check standard pip packages (user and system)
        pip_cmd = cls.get_pip_cmd()
        if pip_cmd:
            code, out, _ = HostBridge.run_command(pip_cmd + ['list', '--format=json'], timeout=10)
            if code == 0 and out:
                try:
                    pkgs = json.loads(out)
                    # Query user-installed packages to distinguish ~/.local from system packages
                    user_set = set()
                    code_u, out_u, _ = HostBridge.run_command(pip_cmd + ['list', '--user', '--format=json'], timeout=6)
                    if code_u == 0 and out_u:
                        try:
                            user_set = {p.get('name', '').lower() for p in json.loads(out_u)}
                        except Exception:
                            pass

                    for p in pkgs:
                        name = p.get('name', '')
                        if not name:
                            continue
                        if name.lower() in seen_names:
                            continue
                        seen_names.add(name.lower())
                        version = p.get('version', '')
                        is_user = name.lower() in user_set
                        summary = "User package (~/.local)" if is_user else "System/environment package"

                        items.append(PackageItem(
                            name=name,
                            version=version,
                            kind='python (pip)',
                            summary=summary,
                            manager='pip'
                        ))
                except Exception:
                    pass

        return items

    @classmethod
    def list_packages(cls, kind_filter: Optional[str] = None) -> List[PackageItem]:
        items: List[PackageItem] = []

        # 1. Flatpaks
        if not kind_filter or kind_filter == 'flatpak':
            code, out, _ = HostBridge.run_command(
                ['flatpak', 'list', '--app', '--columns=application,version,name'],
                timeout=5
            )
            if code == 0 and out:
                for line in out.splitlines():
                    parts = line.split('\t') if '\t' in line else line.split()
                    if parts:
                        app_id = parts[0]
                        ver = parts[1] if len(parts) > 1 else ""
                        name = parts[2] if len(parts) > 2 else app_id
                        items.append(PackageItem(name=app_id, version=ver, kind='flatpak', summary=name))

        # 2. Snaps
        if not kind_filter or kind_filter == 'snap':
            code, out, _ = HostBridge.run_command(['snap', 'list'], timeout=5)
            if code == 0 and out:
                lines = out.splitlines()
                for line in lines[1:]:  # skip header
                    parts = line.split()
                    if len(parts) >= 2:
                        name = parts[0]
                        ver = parts[1]
                        items.append(PackageItem(name=name, version=ver, kind='snap', summary="Snap application"))

        # 3. Native (.deb or .rpm)
        if not kind_filter or kind_filter == 'native':
            nat_type = cls.get_native_type()
            if nat_type == 'deb':
                code, out, _ = HostBridge.run_command(
                    ['dpkg-query', '-W', '-f=${binary:Package}\t${Version}\t${binary:Summary}\n'],
                    timeout=10
                )
                if code == 0 and out:
                    for line in out.splitlines():
                        parts = line.split('\t')
                        if parts and parts[0]:
                            pkg_name = parts[0]
                            pkg_ver = parts[1] if len(parts) > 1 else ""
                            pkg_sum = parts[2] if len(parts) > 2 else ""
                            items.append(PackageItem(name=pkg_name, version=pkg_ver, kind='native (deb)', summary=pkg_sum))
            elif nat_type == 'rpm':
                code, out, _ = HostBridge.run_command(
                    ['rpm', '-qa', '--qf', '%{NAME}\t%{VERSION}\t%{SUMMARY}\n'],
                    timeout=10
                )
                if code == 0 and out:
                    for line in out.splitlines():
                        parts = line.split('\t')
                        if parts and parts[0]:
                            items.append(PackageItem(name=parts[0], version=parts[1] if len(parts) > 1 else "", kind='native (rpm)', summary=parts[2] if len(parts) > 2 else ""))

        # 4. Python Packages (pip / uv)
        if not kind_filter or kind_filter == 'python':
            items.extend(cls.list_python_packages())

        return items

    @classmethod
    def update_package(cls, item: PackageItem) -> Tuple[bool, str]:
        """Updates an existing package to the latest version."""
        if 'python' in item.kind:
            return cls.update_python_package(item)

        elif 'flatpak' in item.kind:
            code, out, err = HostBridge.run_command(['flatpak', 'update', '-y', item.name], timeout=60)
            return (code == 0), out if code == 0 else err

        elif 'snap' in item.kind:
            code, out, err = HostBridge.run_command(['snap', 'refresh', item.name], use_pkexec=True, timeout=60)
            return (code == 0), out if code == 0 else err

        elif 'deb' in item.kind:
            code, out, err = HostBridge.run_command(['apt-get', 'install', '--only-upgrade', '-y', item.name], use_pkexec=True, timeout=60)
            return (code == 0), out if code == 0 else err

        elif 'rpm' in item.kind:
            code, out, err = HostBridge.run_command(['dnf', 'upgrade', '-y', item.name], use_pkexec=True, timeout=60)
            return (code == 0), out if code == 0 else err

        return False, "Unsupported package format for update."

    @classmethod
    def update_python_package(cls, item: PackageItem) -> Tuple[bool, str]:
        """Upgrades a Python package via uv or pip."""
        uv_bin = cls.find_uv_executable()
        # uv tool update
        if getattr(item, 'manager', '') == 'uv-tool' and uv_bin:
            code, out, err = HostBridge.run_command([uv_bin, 'tool', 'upgrade', item.name], timeout=60)
            return (code == 0), out if code == 0 else err

        # uv pip update
        if getattr(item, 'manager', '') == 'uv' and uv_bin:
            code, out, err = HostBridge.run_command([uv_bin, 'pip', 'install', '--upgrade', item.name], timeout=60)
            if code != 0:
                code, out, err = HostBridge.run_command([uv_bin, 'pip', 'install', '--system', '--upgrade', item.name], timeout=60)
            return (code == 0), out if code == 0 else err

        # pip update
        pip_cmd = cls.get_pip_cmd()
        if not pip_cmd:
            return False, "Neither pip nor uv found on the system."

        cmd = pip_cmd + ['install', '--upgrade', '--break-system-packages', item.name]
        code, out, err = HostBridge.run_command(cmd, timeout=60)
        if code == 0:
            return True, f"Successfully updated {item.name}.\n{out}"

        if "permission" in (out + err).lower() or "access denied" in (out + err).lower():
            code, out, err = HostBridge.run_command(cmd, use_pkexec=True, timeout=60)
            return (code == 0), out if code == 0 else err

        return False, err or out or "Update failed."

    @classmethod
    def uninstall_package(cls, item: PackageItem) -> Tuple[bool, str]:
        if 'python' in item.kind:
            return cls.uninstall_python_package(item)

        elif 'flatpak' in item.kind:
            code, out, err = HostBridge.run_command(['flatpak', 'uninstall', '-y', item.name])
            return (code == 0), out if code == 0 else err

        elif 'snap' in item.kind:
            code, out, err = HostBridge.run_command(['snap', 'remove', item.name], use_pkexec=True)
            return (code == 0), out if code == 0 else err

        elif 'deb' in item.kind:
            code, out, err = HostBridge.run_command(['apt-get', 'remove', '-y', item.name], use_pkexec=True)
            return (code == 0), out if code == 0 else err

        elif 'rpm' in item.kind:
            code, out, err = HostBridge.run_command(['dnf', 'remove', '-y', item.name], use_pkexec=True)
            return (code == 0), out if code == 0 else err

        return False, "Unsupported package format."

    @classmethod
    def uninstall_python_package(cls, item: PackageItem) -> Tuple[bool, str]:
        """Removes a Python package via uv or pip."""
        uv_bin = cls.find_uv_executable()
        # uv tool uninstall
        if getattr(item, 'manager', '') == 'uv-tool' and uv_bin:
            code, out, err = HostBridge.run_command([uv_bin, 'tool', 'uninstall', item.name], timeout=30)
            return (code == 0), out if code == 0 else err

        # uv pip uninstall
        if getattr(item, 'manager', '') == 'uv' and uv_bin:
            code, out, err = HostBridge.run_command([uv_bin, 'pip', 'uninstall', item.name], timeout=30)
            if code != 0:
                code, out, err = HostBridge.run_command([uv_bin, 'pip', 'uninstall', '--system', item.name], timeout=30)
            return (code == 0), out if code == 0 else err

        # pip uninstall
        pip_cmd = cls.get_pip_cmd()
        if not pip_cmd:
            return False, "Neither pip nor uv found on the system."

        cmd = pip_cmd + ['uninstall', '-y', '--break-system-packages', item.name]
        code, out, err = HostBridge.run_command(cmd, timeout=30)
        if code == 0:
            return True, f"Successfully uninstalled {item.name}."

        if "permission" in (out + err).lower() or "access denied" in (out + err).lower() or "externally-managed" in (out + err).lower():
            code, out, err = HostBridge.run_command(cmd, use_pkexec=True, timeout=30)
            return (code == 0), out if code == 0 else err

        return False, err or out or "Uninstall failed."

    @classmethod
    def install_python_package(cls, name: str, manager: str = "pip") -> Tuple[bool, str]:
        """Installs a new Python package via pip or uv."""
        name = name.strip()
        if not name:
            return False, "Package name cannot be empty."

        uv_bin = cls.find_uv_executable()
        if manager == "uv" and uv_bin:
            code, out, err = HostBridge.run_command([uv_bin, 'pip', 'install', name], timeout=90)
            if code != 0:
                code, out, err = HostBridge.run_command([uv_bin, 'pip', 'install', '--system', name], timeout=90)
            return (code == 0), out if code == 0 else err

        pip_cmd = cls.get_pip_cmd()
        if not pip_cmd:
            return False, "Neither pip nor uv found on the system."

        cmd = pip_cmd + ['install', '--break-system-packages', name]
        code, out, err = HostBridge.run_command(cmd, timeout=90)
        if code == 0:
            return True, f"Successfully installed {name}."

        if "permission" in (out + err).lower() or "access denied" in (out + err).lower():
            code, out, err = HostBridge.run_command(cmd, use_pkexec=True, timeout=90)
            return (code == 0), out if code == 0 else err

        return False, err or out or "Installation failed."

    @classmethod
    def clean_app_data(cls, item: PackageItem) -> Tuple[bool, str]:
        """Purges leftover app config & cache directory for a specific package."""
        app_name = item.name

        # Python package cache
        if 'python' in item.kind:
            cleaned = 0
            for base in ['~/.cache/pip', '~/.cache/uv']:
                p = os.path.expanduser(base)
                if os.path.exists(p):
                    cleaned += 1
            return True, f"Cleaned cache references for Python package {app_name}."

        # For flatpaks, data lives in ~/.var/app/<app_id>
        if 'flatpak' in item.kind:
            path = os.path.expanduser(f"~/.var/app/{app_name}")
            if HostBridge.is_flatpak():
                code, _, err = HostBridge.run_command(['rm', '-rf', path])
                return (code == 0), f"Cleaned data directory for {app_name}" if code == 0 else err
            elif os.path.exists(path):
                shutil.rmtree(path, ignore_errors=True)
                return True, f"Cleaned data directory {path}"
            return True, "No local app data found for this Flatpak."

        # For snaps, data lives in ~/snap/<snap_name>
        if 'snap' in item.kind:
            path = os.path.expanduser(f"~/snap/{app_name}")
            if HostBridge.is_flatpak():
                code, _, err = HostBridge.run_command(['rm', '-rf', path])
                return (code == 0), f"Cleaned snap data directory for {app_name}" if code == 0 else err
            elif os.path.exists(path):
                shutil.rmtree(path, ignore_errors=True)
                return True, f"Cleaned snap data directory {path}"
            return True, "No local snap data found for this Snap."

        # For native apps, clean ~/.config/<app_name> and ~/.cache/<app_name>
        cleaned = 0
        for base in ['~/.config', '~/.cache']:
            p = os.path.expanduser(f"{base}/{app_name}")
            if HostBridge.is_flatpak():
                HostBridge.run_command(['rm', '-rf', p])
                cleaned += 1
            elif os.path.exists(p):
                shutil.rmtree(p, ignore_errors=True)
                cleaned += 1

        return True, f"Cleaned {cleaned} user profile directories for {app_name}."
