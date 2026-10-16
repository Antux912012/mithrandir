import json
import re
import threading
import urllib.request
import urllib.error
from typing import Callable, Dict, List, Optional, Tuple
from ..backend.host_bridge import HostBridge
from ..backend.monitor import SystemMonitor


RECOMMENDED_MINI_MODELS: List[Dict[str, str]] = [
    {
        "id": "qwen2.5:1.5b",
        "name": "Qwen 2.5 (1.5B) — Recommended for Linux Tasks",
        "size": "~986 MB",
        "ram_required": "2 GB RAM",
        "description": "Fast CPU inference (~30 tok/s on laptop CPUs). Excellent at Linux commands, bash, and systemd diagnostics.",
    },
    {
        "id": "qwen2.5-coder:1.5b",
        "name": "Qwen 2.5 Coder (1.5B) — Shell & Config Specialist",
        "size": "~986 MB",
        "ram_required": "2 GB RAM",
        "description": "Tuned for shell scripting, system configuration files, and CLI command generation.",
    },
    {
        "id": "llama3.2:1b",
        "name": "Llama 3.2 (1B) — Ultra-Light Edge Model",
        "size": "~1.3 GB",
        "ram_required": "2 GB RAM",
        "description": "Meta's ultra-compact instruction model for low-resource Linux laptops and desktops.",
    },
    {
        "id": "smollm2:1.7b",
        "name": "SmolLM2 (1.7B) — Compact On-Device Assistant",
        "size": "~1.0 GB",
        "ram_required": "2.5 GB RAM",
        "description": "HuggingFace's compact model designed specifically for local CPU execution.",
    },
    {
        "id": "llama3.2:3b",
        "name": "Llama 3.2 (3B) — Balanced Reasoning",
        "size": "~2.0 GB",
        "ram_required": "4 GB RAM",
        "description": "Deeper multi-step troubleshooting and log analysis for systems with 8GB+ RAM.",
    },
]


class LocalLLMManager:
    """
    Manages local offline AI runtimes (Ollama, LM Studio, llama.cpp) and provides
    a built-in zero-dependency Linux Mini-Assistant for offline diagnostics and generic Linux tasks.
    """

    PROVIDER_PRESETS: Dict[str, Dict[str, str]] = {
        "builtin": {
            "label": "Built-in Linux Mini-Assistant (Offline, Zero-Config)",
            "base_url": "builtin://linux-mini",
            "default_model": "builtin-linux-mini",
            "needs_key": "false",
        },
        "ollama": {
            "label": "Ollama (Local Offline — localhost:11434)",
            "base_url": "http://localhost:11434/v1",
            "default_model": "qwen2.5:1.5b",
            "needs_key": "false",
        },
        "lmstudio": {
            "label": "LM Studio (Local Offline — localhost:1234)",
            "base_url": "http://localhost:1234/v1",
            "default_model": "",
            "needs_key": "false",
        },
        "llamacpp": {
            "label": "llama.cpp / LocalAI (Local Offline — localhost:8080)",
            "base_url": "http://localhost:8080/v1",
            "default_model": "",
            "needs_key": "false",
        },
        "nim": {
            "label": "NVIDIA NIM (OpenAI-Compatible Cloud API)",
            "base_url": "https://integrate.api.nvidia.com/v1",
            "default_model": "meta/llama-3.1-70b-instruct",
            "needs_key": "true",
        },
        "openai": {
            "label": "OpenAI / Custom OpenAI-Compatible Endpoint",
            "base_url": "https://api.openai.com/v1",
            "default_model": "gpt-4o-mini",
            "needs_key": "true",
        },
    }

    NIM_POPULAR_MODELS: List[str] = [
        "meta/llama-3.1-70b-instruct",
        "meta/llama-3.1-8b-instruct",
        "meta/llama-3.3-70b-instruct",
        "nvidia/llama-3.1-nemotron-70b-instruct",
        "qwen/qwen2.5-coder-32b-instruct",
        "mistralai/mistral-nemotron",
        "deepseek-ai/deepseek-r1",
        "google/gemma-2-27b-it",
    ]

    @classmethod
    def check_ollama_status(cls) -> Dict[str, any]:
        """Checks if Ollama is installed and whether its local server is running."""
        has_binary = HostBridge.which("ollama") is not None
        is_running = False
        installed_models: List[str] = []

        try:
            req = urllib.request.Request(
                "http://localhost:11434/api/tags",
                headers={"User-Agent": "Mithrandir/0.3.5"}
            )
            with urllib.request.urlopen(req, timeout=2) as resp:
                if resp.status == 200:
                    is_running = True
                    data = json.loads(resp.read().decode("utf-8"))
                    for m in data.get("models", []):
                        name = m.get("name", "")
                        if name:
                            installed_models.append(name)
        except Exception:
            pass

        if has_binary and not installed_models:
            code, out, _ = HostBridge.run_command(["ollama", "list"], timeout=4)
            if code == 0 and out.strip():
                lines = out.strip().splitlines()
                for line in lines[1:]:
                    parts = line.split()
                    if parts:
                        installed_models.append(parts[0])

        return {
            "installed": has_binary,
            "running": is_running,
            "models": installed_models,
        }

    @classmethod
    def get_hardware_recommendation(cls) -> Dict[str, str]:
        """Analyzes system RAM to recommend the best offline mini-LLM."""
        mem = SystemMonitor.get_memory_info()
        total_mb = mem.get("total_mb", 4096)
        if total_mb >= 12000:
            return {
                "recommended_id": "qwen2.5:1.5b",
                "alternative_id": "llama3.2:3b",
                "summary": f"Detected {total_mb / 1024:.1f} GB RAM — Qwen 2.5 (1.5B) or Llama 3.2 (3B) will run smoothly on CPU.",
            }
        return {
            "recommended_id": "qwen2.5:1.5b",
            "alternative_id": "llama3.2:1b",
            "summary": f"Detected {total_mb / 1024:.1f} GB RAM — Qwen 2.5 (1.5B) or Llama 3.2 (1B) recommended for low memory footprint.",
        }

    @classmethod
    def start_ollama_server(cls) -> Tuple[bool, str]:
        """Attempts to start the local Ollama server in the background."""
        status = cls.check_ollama_status()
        if status["running"]:
            return True, "Ollama server is already running on localhost:11434."
        if not status["installed"]:
            return False, "Ollama binary is not installed."

        # Try systemd service first, then background daemon
        code, _, _ = HostBridge.run_command(["systemctl", "is-enabled", "ollama"], timeout=3)
        if code == 0:
            HostBridge.run_command(["systemctl", "start", "ollama"], use_pkexec=True, timeout=10)
        else:
            HostBridge.run_command(["sh", "-c", "nohup ollama serve >/dev/null 2>&1 &"], timeout=4)

        import time
        time.sleep(1.5)
        after = cls.check_ollama_status()
        if after["running"]:
            return True, "Ollama local AI server started on http://localhost:11434/v1."
        return False, "Could not verify Ollama server startup on port 11434."

    @classmethod
    def install_ollama(
        cls,
        line_callback: Optional[Callable[[str], None]] = None,
        cancel_event: Optional[threading.Event] = None
    ) -> Tuple[bool, str]:
        """Installs Ollama on Linux using the official install script with Polkit elevation."""
        if line_callback:
            line_callback("[Mithrandir] Installing Ollama local AI runtime...")
            line_callback("[Mithrandir] Executing: curl -fsSL https://ollama.com/install.sh | sh")

        if not HostBridge.which("curl"):
            return False, "curl is required to download the Ollama installer. Please install curl first."

        code, out = HostBridge.run_streaming_command(
            ["sh", "-c", "curl -fsSL https://ollama.com/install.sh | sh"],
            line_callback=line_callback,
            cancel_event=cancel_event,
            use_pkexec=True,
        )
        status = cls.check_ollama_status()
        if code == 0 or status["installed"]:
            cls.start_ollama_server()
            return True, "Ollama installed and started successfully."
        return False, f"Ollama installation exited with code {code}."

    @classmethod
    def pull_model(
        cls,
        model_name: str,
        line_callback: Optional[Callable[[str], None]] = None,
        cancel_event: Optional[threading.Event] = None
    ) -> Tuple[bool, str]:
        """Downloads a mini-LLM via Ollama (`ollama pull <model>`)."""
        model_name = model_name.strip()
        if not model_name:
            return False, "Model name cannot be empty."

        status = cls.check_ollama_status()
        if not status["installed"]:
            return False, "Ollama is not installed yet. Click 'Install Ollama' first."
        if not status["running"]:
            cls.start_ollama_server()

        if line_callback:
            line_callback(f"[Mithrandir] Pulling offline mini-LLM '{model_name}'...")

        code, out = HostBridge.run_streaming_command(
            ["ollama", "pull", model_name],
            line_callback=line_callback,
            cancel_event=cancel_event,
            use_pkexec=False,
        )
        if code == 0:
            if line_callback:
                line_callback(f"[Mithrandir] ✓ Model '{model_name}' is ready for offline use!")
            return True, f"Model '{model_name}' downloaded and ready."
        return False, f"Failed to pull model '{model_name}' (exit code {code})."


class BuiltinLinuxAssistant:
    """
    Zero-dependency built-in offline Linux mini-assistant.
    Provides instant system diagnostics and natural-language generic Linux task
    assistance without requiring external cloud APIs or heavy model downloads.
    """

    TASK_KNOWLEDGE_BASE = [
        {
            "keywords": ["large file", "big file", "disk space", "full disk", "largest", "100mb", "500mb", "space"],
            "title": "Find Largest Files & Directories",
            "explanation": (
                "To locate files consuming the most disk space in your home directory or `/var`, "
                "you can scan for files larger than 100 MB sorted by size."
            ),
            "command": "find ~ -type f -size +100M -exec ls -lh {} + 2>/dev/null | sort -k5 -hr | head -n 25",
        },
        {
            "keywords": ["port", "listen", "socket", "network connection", "open port", "8080", "ss ", "netstat"],
            "title": "Inspect Open Network Ports & Listening Processes",
            "explanation": (
                "Using `ss -tulnp` shows all active TCP and UDP listening sockets along with the process "
                "and PID bound to each port."
            ),
            "command": "ss -tulnp",
        },
        {
            "keywords": ["broken package", "dpkg", "apt", "fix package", "unmet dependencies", "lock", "apt-get"],
            "title": "Repair Broken Packages & Dependencies",
            "explanation": (
                "If APT or dpkg was interrupted or has unmet dependencies, configuring pending packages "
                "and running `apt-get install -f` resolves dependency issues safely."
            ),
            "command": "pkexec sh -c 'dpkg --configure -a && apt-get install -f -y'",
        },
        {
            "keywords": ["update", "upgrade", "patch", "system update"],
            "title": "Update System Packages",
            "explanation": (
                "Refreshes package repositories and upgrades installed packages to their latest security versions."
            ),
            "command": "pkexec sh -c 'apt-get update && apt-get upgrade -y'",
        },
        {
            "keywords": ["journal", "log", "syslog", "dmesg", "boot error", "system log"],
            "title": "Inspect Recent Critical System Logs",
            "explanation": (
                "Queries the systemd journal for priority 0–3 (emergency, alert, critical, error) messages from the current boot."
            ),
            "command": "journalctl -p 3 -xb --no-pager -n 40",
        },
        {
            "keywords": ["clean journal", "vacuum", "clear log"],
            "title": "Vacuum Systemd Journal Logs",
            "explanation": (
                "Trims archived systemd journal logs down to 50 MB to reclaim `/var/log/journal` space."
            ),
            "command": "pkexec journalctl --vacuum-size=50M",
        },
        {
            "keywords": ["memory", "ram", "high memory", "swap", "oom", "leak"],
            "title": "Top Memory-Consuming Processes",
            "explanation": (
                "Lists the top 15 processes sorted by Resident Set Size (RSS) memory consumption."
            ),
            "command": "ps -eo pid,user,%mem,rss,comm --sort=-rss | head -n 16",
        },
        {
            "keywords": ["cpu", "high cpu", "fan", "slow", "load", "overheat", "lag"],
            "title": "Top CPU-Consuming Processes",
            "explanation": (
                "Lists the top 15 active processes sorted by instantaneous CPU utilization."
            ),
            "command": "ps -eo pid,user,%cpu,%mem,stat,comm --sort=-%cpu | head -n 16",
        },
        {
            "keywords": ["audio", "sound", "pipewire", "pulseaudio", "microphone", "speaker", "no sound"],
            "title": "Restart PipeWire & WirePlumber Audio Stack",
            "explanation": (
                "Restarting the user-level PipeWire and WirePlumber services resolves most stuck audio output/input issues on modern Linux desktops."
            ),
            "command": "systemctl --user restart pipewire pipewire-pulse wireplumber",
        },
        {
            "keywords": ["wifi", "wi-fi", "network", "internet", "dns", "connection", "ip address", "nmcli"],
            "title": "Network Interfaces, IP & Wi-Fi Diagnostics",
            "explanation": (
                "Checks NetworkManager device states, active connections, and IP routing."
            ),
            "command": "nmcli device status && ip -brief address",
        },
        {
            "keywords": ["bluetooth", "bluez"],
            "title": "Check & Restart Bluetooth Service",
            "explanation": (
                "Checks the status of the `bluetooth` systemd service and controller state."
            ),
            "command": "systemctl status bluetooth --no-pager",
        },
        {
            "keywords": ["boot time", "slow boot", "startup time", "blame", "systemd-analyze"],
            "title": "Analyze System Boot Performance",
            "explanation": (
                "Uses `systemd-analyze` to display total boot duration and the top 15 slowest services during startup."
            ),
            "command": "systemd-analyze time && systemd-analyze blame --no-pager | head -n 15",
        },
        {
            "keywords": ["failed service", "failed unit", "systemctl", "degraded"],
            "title": "List Failed Systemd Services",
            "explanation": (
                "Displays all systemd units currently in a failed state."
            ),
            "command": "systemctl --failed --no-pager",
        },
        {
            "keywords": ["disk health", "smart", "ssd", "nvme", "hard drive", "block", "partition", "lsblk"],
            "title": "Inspect Block Devices, Partitions & Mounts",
            "explanation": (
                "Displays block devices, filesystems, mount points, and usage percentages."
            ),
            "command": "lsblk -f && df -hT -x tmpfs -x devtmpfs",
        },
        {
            "keywords": ["zombie", "defunct"],
            "title": "Find Zombie / Defunct Processes",
            "explanation": (
                "Checks the process table for any zombie (`Z`) processes and their parent PIDs."
            ),
            "command": "ps -eo pid,ppid,stat,user,comm | awk '$3 ~ /^Z/'",
        },
        {
            "keywords": ["battery", "power", "upower", "energy", "laptop"],
            "title": "Detailed Battery & Power Telemetry",
            "explanation": (
                "Queries UPower for battery health, cycle status, voltage, and discharge rate."
            ),
            "command": "upower -i $(upower -e | grep BAT | head -n 1)",
        },
        {
            "keywords": ["flatpak", "unused flatpak"],
            "title": "Clean Unused Flatpak Runtimes",
            "explanation": (
                "Removes orphaned Flatpak runtimes and extensions that are no longer needed."
            ),
            "command": "flatpak uninstall --unused -y",
        },
        {
            "keywords": ["docker", "container", "podman"],
            "title": "Check Running Containers & Reclaimable Space",
            "explanation": (
                "Shows active Docker/Podman containers and disk usage."
            ),
            "command": "docker system df 2>/dev/null || podman system df 2>/dev/null || echo 'No container runtime active'",
        },
    ]

    @classmethod
    def respond(cls, messages: List[Dict[str, str]]) -> str:
        """
        Generates a response from the built-in offline Linux mini-assistant.
        Detects whether the prompt is a full system diagnostics scan or a generic user task.
        """
        user_text = ""
        for m in reversed(messages):
            if m.get("role") == "user":
                user_text = m.get("content", "")
                break

        if "Analyze this system state:" in user_text:
            return cls._analyze_diagnostics_prompt(user_text)

        return cls._handle_generic_task(user_text)

    @classmethod
    def _analyze_diagnostics_prompt(cls, prompt: str) -> str:
        lines = prompt.splitlines()
        os_name = "GNU/Linux"
        kernel = "Unknown"
        mem_line = ""
        disk_line = ""
        failed_section = []
        errors_section = []
        mode = None

        for line in lines:
            if line.startswith("- OS:"):
                os_name = line.split(":", 1)[1].strip()
            elif line.startswith("- Kernel:"):
                kernel = line.split(":", 1)[1].strip()
            elif line.startswith("- Memory:"):
                mem_line = line.split(":", 1)[1].strip()
            elif line.startswith("- Disk:"):
                disk_line = line.split(":", 1)[1].strip()
            elif line.startswith("- Failed systemd units:"):
                mode = "failed"
            elif line.startswith("- Recent system errors:"):
                mode = "errors"
            elif line.startswith("Provide a brief diagnosis"):
                mode = None
            else:
                if mode == "failed" and line.strip():
                    failed_section.append(line.strip())
                elif mode == "errors" and line.strip():
                    errors_section.append(line.strip())

        mem_pct = 0.0
        m_match = re.search(r"\(([\d.]+)%\)", mem_line)
        if m_match:
            try:
                mem_pct = float(m_match.group(1))
            except ValueError:
                pass

        disk_pct = 0.0
        d_match = re.search(r"\(([\d.]+)%\s*used\)", disk_line)
        if d_match:
            try:
                disk_pct = float(d_match.group(1))
            except ValueError:
                pass

        failed_text = "\n".join(failed_section).strip()
        has_failed_units = bool(
            failed_text
            and failed_text.lower() != "none"
            and "0 loaded units listed" not in failed_text.lower()
            and "unable to query" not in failed_text.lower()
        )

        report = [
            "=== MITHRANDIR BUILT-IN OFFLINE LINUX DIAGNOSTICS ===",
            f"System: {os_name} (Kernel {kernel})",
            "",
            "1. RESOURCE HEALTH:",
        ]

        proposed_cmd = None

        if mem_pct >= 85.0:
            report.append(f"   • [WARNING] High memory utilization detected ({mem_line}).")
            proposed_cmd = "ps -eo pid,user,%mem,rss,comm --sort=-rss | head -n 15"
        else:
            report.append(f"   • [OK] Memory utilization is healthy ({mem_line}).")

        if disk_pct >= 85.0:
            report.append(f"   • [WARNING] Root filesystem usage is high ({disk_line}).")
            if not proposed_cmd:
                proposed_cmd = "pkexec journalctl --vacuum-size=50M"
        else:
            report.append(f"   • [OK] Disk free space is adequate ({disk_line}).")

        report.append("")
        report.append("2. SYSTEMD SERVICES:")
        if has_failed_units:
            first_unit = failed_section[0].split()[0] if failed_section[0].split() else ""
            if first_unit == "●":
                parts = failed_section[0].split()
                first_unit = parts[1] if len(parts) > 1 else ""
            report.append(f"   • [ALERT] Failed systemd units detected:\n     {failed_text}")
            if first_unit and not proposed_cmd:
                proposed_cmd = f"systemctl status {first_unit} --no-pager"
        else:
            report.append("   • [OK] All active systemd services are running normally (0 failed units).")

        report.append("")
        report.append("3. KERNEL & JOURNAL ERROR ANALYSIS:")
        err_text = "\n".join(errors_section).strip()
        if not err_text or "no critical errors" in err_text.lower() or "no entries" in err_text.lower():
            report.append("   • [OK] No critical priority-3 journal errors found in current boot.")
        elif "unable to read journal" in err_text.lower():
            report.append("   • [INFO] Journal requires elevated permissions or systemd-journal group membership.")
            if not proposed_cmd:
                proposed_cmd = "pkexec journalctl -p 3 -xb --no-pager -n 25"
        else:
            err_count = len(errors_section)
            report.append(f"   • [NOTICE] Found {err_count} recent priority-3 error log lines in current boot.")
            sample = errors_section[-3:]
            for s in sample:
                report.append(f"     - {s[:110]}")
            if not proposed_cmd:
                proposed_cmd = "journalctl -p 3 -xb --no-pager -n 25"

        if not proposed_cmd:
            proposed_cmd = "systemd-analyze time && df -h /"

        report.append("")
        report.append("4. RECOMMENDED ACTION:")
        report.append(f"PROPOSED_COMMAND: {proposed_cmd}")
        return "\n".join(report)

    @classmethod
    def _handle_generic_task(cls, query: str) -> str:
        q_lower = query.lower().strip()

        # Match against knowledge base
        best_match = None
        best_score = 0
        for item in cls.TASK_KNOWLEDGE_BASE:
            score = sum(1 for kw in item["keywords"] if kw in q_lower)
            if score > best_score:
                best_score = score
                best_match = item

        if best_match and best_score > 0:
            return (
                f"=== {best_match['title']} (Built-in Offline Assistant) ===\n\n"
                f"{best_match['explanation']}\n\n"
                f"You can review and execute the recommended command below:\n"
                f"PROPOSED_COMMAND: {best_match['command']}"
            )

        # Fallback generic Linux helper
        return (
            "=== Mithrandir Built-in Offline Linux Assistant ===\n\n"
            f"Query: \"{query}\"\n\n"
            "I am running in Built-in Zero-Config Offline Mode. Here are tasks I can immediately help with:\n"
            "• Disk & Files: 'find large files', 'check disk health'\n"
            "• Performance: 'check high memory', 'check high cpu', 'analyze slow boot'\n"
            "• Packages: 'fix broken packages', 'update system packages', 'clean flatpak'\n"
            "• Networking & Audio: 'check open ports', 'wifi diagnostics', 'restart audio / pipewire'\n"
            "• Logs & Services: 'list failed services', 'inspect journal errors', 'clean journal logs'\n\n"
            "Tip: For open-ended natural language reasoning offline, expand 'Local Offline Mini-LLM Manager' above "
            "to download Qwen 2.5 (1.5B) via Ollama, or connect to NVIDIA NIM / OpenAI.\n\n"
            "PROPOSED_COMMAND: uname -a && uptime && free -h && df -h /"
        )
