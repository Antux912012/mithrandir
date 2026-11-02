import os
import signal
import pwd
from typing import List, Dict, Any
from .host_bridge import HostBridge


class ProcessManager:
    """
    Task manager / process manager backend inspired by Stacer.
    Lists active system processes with PID, user, CPU %, Memory, and allows termination.
    """

    @classmethod
    def list_processes(cls) -> List[Dict[str, Any]]:
        """
        Lists all system processes with details.
        Queries host processes via `ps` for accurate instantaneous CPU % and host PID namespace,
        with `psutil` and direct `/proc` inspection as fallbacks.
        """
        # 1. Fast host process listing via ps (provides accurate %cpu and works across Flatpak & native)
        code, out, _ = HostBridge.run_command(
            ['ps', '-eo', 'pid:10,user:20,%cpu:8,%mem:8,rss:12,stat:8,comm:30,args', '--no-headers'],
            timeout=8
        )
        if code == 0 and out.strip():
            procs = cls._parse_host_ps(out)
            if procs:
                return procs

        # 2. Fallback to psutil outside Flatpak
        if not HostBridge.is_flatpak():
            try:
                import psutil
                procs = []
                for p in psutil.process_iter(['pid', 'name', 'username', 'cpu_percent', 'memory_info', 'memory_percent', 'status', 'cmdline']):
                    try:
                        info = p.info
                        rss = info['memory_info'].rss if info.get('memory_info') else 0
                        mem_mb = round(rss / (1024 * 1024), 1)
                        procs.append({
                            'pid': info['pid'],
                            'name': info['name'] or f"proc-{info['pid']}",
                            'user': info['username'] or 'system',
                            'cpu_percent': round(info.get('cpu_percent') or 0.0, 1),
                            'mem_mb': mem_mb,
                            'mem_percent': round(info.get('memory_percent') or 0.0, 1),
                            'status': (info.get('status') or 'running').capitalize(),
                            'cmdline': " ".join(info.get('cmdline') or [])
                        })
                    except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
                        pass
                if procs:
                    return procs
            except ImportError:
                pass

        # 3. Final fallback: direct /proc parsing
        return cls._parse_proc_fallback()

    @classmethod
    def _parse_host_ps(cls, out: str) -> List[Dict[str, Any]]:
        procs = []
        if not out or not out.strip():
            return procs
        for line in out.strip().splitlines():
            parts = line.split(maxsplit=7)
            if len(parts) >= 7 and parts[0].isdigit():
                try:
                    pid = int(parts[0])
                    user = parts[1]
                    try:
                        cpu = float(parts[2])
                    except ValueError:
                        cpu = 0.0
                    try:
                        mem_pct = float(parts[3])
                    except ValueError:
                        mem_pct = 0.0
                    try:
                        rss_kb = int(parts[4])
                        mem_mb = round(rss_kb / 1024.0, 1)
                    except ValueError:
                        mem_mb = 0.0
                    stat = parts[5]
                    comm = parts[6]
                    cmdline = parts[7] if len(parts) > 7 else comm

                    procs.append({
                        'pid': pid,
                        'name': comm,
                        'user': user,
                        'cpu_percent': cpu,
                        'mem_mb': mem_mb,
                        'mem_percent': mem_pct,
                        'status': stat,
                        'cmdline': cmdline
                    })
                except Exception:
                    continue
        return procs

    @classmethod
    def _parse_proc_fallback(cls) -> List[Dict[str, Any]]:
        total_mem_kb = 1
        meminfo = HostBridge.read_host_file('/proc/meminfo')
        if meminfo:
            for line in meminfo.splitlines():
                if line.startswith('MemTotal:'):
                    try:
                        total_mem_kb = int(line.split()[1])
                    except (ValueError, IndexError):
                        pass
                    break

        procs = []
        try:
            for pid_dir in os.listdir('/proc'):
                if not pid_dir.isdigit():
                    continue
                pid = int(pid_dir)
                status_file = f"/proc/{pid}/status"
                content = HostBridge.read_host_file(status_file)
                if not content:
                    continue

                name = f"proc-{pid}"
                user = "system"
                rss_kb = 0
                state = "Running"

                for line in content.splitlines():
                    if line.startswith('Name:'):
                        name = line.split(':', 1)[1].strip()
                    elif line.startswith('Uid:'):
                        try:
                            uid = int(line.split()[1])
                            user = pwd.getpwuid(uid).pw_name
                        except Exception:
                            pass
                    elif line.startswith('VmRSS:'):
                        try:
                            rss_kb = int(line.split()[1])
                        except (ValueError, IndexError):
                            pass
                    elif line.startswith('State:'):
                        state = line.split(':', 1)[1].strip().split()[0]

                mem_mb = round(rss_kb / 1024.0, 1)
                mem_pct = round((rss_kb / max(1, total_mem_kb)) * 100.0, 1)

                procs.append({
                    'pid': pid,
                    'name': name,
                    'user': user,
                    'cpu_percent': 0.0,
                    'mem_mb': mem_mb,
                    'mem_percent': mem_pct,
                    'status': state,
                    'cmdline': name
                })
        except OSError:
            pass

        return procs

    @classmethod
    def kill_process(cls, pid: int, force: bool = False) -> tuple[bool, str]:
        """
        Terminates the specified process using SIGTERM (graceful) or SIGKILL (force).
        Uses HostBridge in Flatpak to ensure the host process is targeted.
        """
        flag = "-9" if force else "-15"
        sig_name = 'SIGKILL' if force else 'SIGTERM'

        if HostBridge.is_flatpak():
            code, out, err = HostBridge.run_command(['kill', flag, str(pid)], timeout=5)
            if code == 0:
                return True, f"Sent {sig_name} to process {pid} on host."
            if 'not permitted' in (out + err).lower() or 'permission denied' in (out + err).lower():
                code_p, out_p, err_p = HostBridge.run_command(['kill', flag, str(pid)], use_pkexec=True, timeout=10)
                if code_p == 0:
                    return True, f"Killed process {pid} with elevated privileges."
                return False, f"Failed to terminate process {pid}: {err_p or out_p}"
            if 'no such process' in (out + err).lower():
                return True, f"Process {pid} has already ended."
            return False, f"Failed to terminate process {pid}: {err or out}"

        # Native environment outside Flatpak
        sig = signal.SIGKILL if force else signal.SIGTERM
        try:
            os.kill(pid, sig)
            return True, f"Sent {sig_name} to process {pid}."
        except PermissionError:
            code, out, err = HostBridge.run_command(['kill', flag, str(pid)], use_pkexec=True)
            if code == 0:
                return True, f"Killed process {pid} with elevated privileges."
            return False, f"Failed to terminate process {pid}: {err or out}"
        except ProcessLookupError:
            return True, f"Process {pid} has already ended."
        except Exception as e:
            return False, str(e)
