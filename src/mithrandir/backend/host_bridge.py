import os
import shutil
import signal
import subprocess
import threading
from typing import Callable, List, Optional, Tuple

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
    def which(cls, binary: str) -> Optional[str]:
        """Checks if a binary is available on the host."""
        if cls.is_flatpak():
            code, out, _ = cls.run_command(['which', binary], timeout=5)
            if code == 0 and out.strip():
                return out.strip().splitlines()[0]
            return None
        return shutil.which(binary)

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
    def run_streaming_command(
        cls,
        cmd: List[str],
        line_callback: Optional[Callable[[str], None]] = None,
        cancel_event: Optional[threading.Event] = None,
        use_pkexec: bool = False,
        nice_priority: Optional[int] = None,
        io_idle: bool = False
    ) -> Tuple[int, str]:
        """
        Executes a host command streaming stdout line by line.
        Supports cancellation and low CPU/IO scheduling.
        Returns (exit_code, full_output).
        """
        prefix = []
        if cls.is_flatpak():
            prefix = ['flatpak-spawn', '--host']

        if nice_priority is not None and cls.which('nice'):
            prefix.extend(['nice', '-n', str(nice_priority)])
        if io_idle and cls.which('ionice'):
            prefix.extend(['ionice', '-c', '3'])

        cmd_parts = list(cmd)
        if cmd_parts and cmd_parts[0] in ['sudo', 'pkexec']:
            use_pkexec = True
            cmd_parts = cmd_parts[1:]

        if use_pkexec:
            prefix.append('pkexec')

        full_cmd = prefix + cmd_parts

        try:
            process = subprocess.Popen(
                full_cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                preexec_fn=os.setsid if not cls.is_flatpak() and hasattr(os, 'setsid') else None
            )
        except Exception as e:
            return -1, str(e)

        output_lines: List[str] = []

        def terminate_process():
            try:
                if not cls.is_flatpak() and hasattr(os, 'killpg'):
                    os.killpg(os.getpgid(process.pid), signal.SIGTERM)
                else:
                    process.terminate()
            except Exception:
                pass

        while True:
            if cancel_event is not None and cancel_event.is_set():
                terminate_process()
                break

            line = process.stdout.readline() if process.stdout else ""
            if not line and process.poll() is not None:
                break
            if line:
                cleaned = line.rstrip('\r\n')
                output_lines.append(cleaned)
                if line_callback:
                    line_callback(cleaned)

        if process.stdout:
            process.stdout.close()
        returncode = process.wait()
        return returncode, "\n".join(output_lines)

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

