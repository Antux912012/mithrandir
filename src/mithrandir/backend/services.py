from dataclasses import dataclass
from typing import List, Optional
from .host_bridge import HostBridge

@dataclass
class ServiceInfo:
    name: str
    load_state: str
    active_state: str
    sub_state: str
    description: str = ""
    is_critical: bool = False

class ServiceManager:
    """
    Manages systemd services: lists, starts, stops, restarts, enables, and disables them.
    Tags essential services (systemd, dbus, network, display managers) to prevent accidental bricking.
    """

    CRITICAL_SERVICES = [
        'systemd-', 'dbus', 'NetworkManager', 'systemd-resolved', 'sshd',
        'gdm', 'sddm', 'lightdm', 'pipewire', 'polkit', 'udisks2', 'login'
    ]

    @classmethod
    def is_critical(cls, service_name: str) -> bool:
        for crit in cls.CRITICAL_SERVICES:
            if crit in service_name:
                return True
        return False

    @classmethod
    def list_services(cls) -> List[ServiceInfo]:
        services = []
        code, out, _ = HostBridge.run_command(
            ['systemctl', 'list-unit-files', '--type=service', '--no-legend', '--plain', '--no-pager'],
            timeout=5
        )
        # Also query active status
        code_act, out_act, _ = HostBridge.run_command(
            ['systemctl', 'list-units', '--type=service', '--no-legend', '--plain', '--no-pager'],
            timeout=5
        )

        active_dict = {}
        if code_act == 0 and out_act:
            for line in out_act.splitlines():
                parts = line.split()
                if len(parts) >= 4:
                    active_dict[parts[0]] = (parts[2], parts[3]) # active_state, sub_state

        if code == 0 and out:
            for line in out.splitlines():
                parts = line.split()
                if len(parts) >= 2:
                    name = parts[0]
                    state = parts[1]
                    act_state, sub_state = active_dict.get(name, ('inactive', 'dead'))
                    services.append(ServiceInfo(
                        name=name,
                        load_state=state,
                        active_state=act_state,
                        sub_state=sub_state,
                        is_critical=cls.is_critical(name)
                    ))
        return services

    @classmethod
    def start_service(cls, name: str) -> tuple[bool, str]:
        code, out, err = HostBridge.run_command(['systemctl', 'start', name], use_pkexec=True)
        return (code == 0), out if code == 0 else err

    @classmethod
    def stop_service(cls, name: str) -> tuple[bool, str]:
        code, out, err = HostBridge.run_command(['systemctl', 'stop', name], use_pkexec=True)
        return (code == 0), out if code == 0 else err

    @classmethod
    def restart_service(cls, name: str) -> tuple[bool, str]:
        code, out, err = HostBridge.run_command(['systemctl', 'restart', name], use_pkexec=True)
        return (code == 0), out if code == 0 else err

    @classmethod
    def enable_service(cls, name: str) -> tuple[bool, str]:
        code, out, err = HostBridge.run_command(['systemctl', 'enable', name], use_pkexec=True)
        return (code == 0), out if code == 0 else err

    @classmethod
    def disable_service(cls, name: str) -> tuple[bool, str]:
        code, out, err = HostBridge.run_command(['systemctl', 'disable', name], use_pkexec=True)
        return (code == 0), out if code == 0 else err
