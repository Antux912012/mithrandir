import os
import shutil
import subprocess
from typing import List, Optional, Tuple

class HostBridge:
    """
    Host bridge responsible for executing commands on the host OS.
    When running inside Flatpak, it uses `flatpak-spawn --host`.
    Outside Flatpak, it invokes commands directly on the host system.
    """

    @staticmethod
    def is_flatpak() -> bool:
        return os.path.exists('/.flatpak-info')

    @classmethod
    def run_command(cls, cmd: List[str], use_pkexec: bool = False, timeout: Optional[int] = 30) -> Tuple[int, str, str]:
        """
        Executes an allowlisted command on the host.
        Returns (returncode, stdout, stderr).
        """
        full_cmd = []
        if cls.is_flatpak():
            full_cmd = ['flatpak-spawn', '--host']

        cmd_parts = list(cmd)
        if cmd_parts:
            if cmd_parts[0] in ['sudo', 'pkexec']:
                use_pkexec = True
                cmd_parts = cmd_parts[1:]

        if use_pkexec:
            full_cmd.append('pkexec')

        full_cmd.extend(cmd_parts)

        try:
            res = subprocess.run(
                full_cmd,
                capture_output=True,
                text=True,
                timeout=timeout
            )
            return res.returncode, res.stdout, res.stderr
        except subprocess.TimeoutExpired:
            return -1, "", "Command timed out."
        except Exception as e:
            return -1, "", str(e)

    @classmethod
    def read_host_file(cls, path: str) -> Optional[str]:
        """Reads a file from the host filesystem."""
        if cls.is_flatpak():
            code, out, _ = cls.run_command(['cat', path], timeout=5)
            if code == 0:
                return out
            return None
        else:
            try:
                with open(path, 'r', encoding='utf-8', errors='ignore') as f:
                    return f.read()
            except Exception:
                return None
