import os
import signal
import pwd
from typing import List, Dict, Any, Optional
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
        Uses psutil when available; falls back to parsing /proc directly.
        """
        procs = []
        try:
            import psutil
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
            return procs
        except ImportError:
            pass

        # Fallback: /proc parsing
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

        return procs

    @classmethod
    def kill_process(cls, pid: int, force: bool = False) -> tuple[bool, str]:
        """
        Terminates the specified process using SIGTERM (graceful) or SIGKILL (force).
        Uses pkexec if the process belongs to another user/root.
        """
        sig = signal.SIGKILL if force else signal.SIGTERM
        try:
            os.kill(pid, sig)
            return True, f"Sent {'SIGKILL' if force else 'SIGTERM'} to process {pid}."
        except PermissionError:
            # Requires root
            flag = "-9" if force else "-15"
            code, out, err = HostBridge.run_command(['kill', flag, str(pid)], use_pkexec=True)
            if code == 0:
                return True, f"Killed process {pid} with elevated privileges."
            return False, f"Failed to terminate process {pid}: {err or out}"
        except ProcessLookupError:
            return True, f"Process {pid} has already ended."
        except Exception as e:
            return False, str(e)
