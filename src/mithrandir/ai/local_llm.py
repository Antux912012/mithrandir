import os
import json
import re
import threading
import subprocess
import platform
import zipfile
import shutil
import time
import urllib.request
import urllib.error
from typing import Any, Callable, Dict, List, Optional, Tuple
from ..backend.host_bridge import HostBridge
from ..backend.monitor import SystemMonitor


RECOMMENDED_QWEN_GGUF_MODELS: List[Dict[str, str]] = [
    {
        "id": "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        "name": "Qwen 2.5 (1.5B Instruct Q4_K_M) — Recommended In-App Mini-LLM",
        "size": "~986 MB",
        "ram_required": "2 GB RAM",
        "url": "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
        "description": "Fast CPU inference (~30 tok/s on laptops). Excellent at Linux system commands, bash, and diagnostics.",
    },
    {
        "id": "qwen2.5-coder-1.5b-instruct-q4_k_m.gguf",
        "name": "Qwen 2.5 Coder (1.5B Instruct Q4_K_M) — Shell & Script Specialist",
        "size": "~986 MB",
        "ram_required": "2 GB RAM",
        "url": "https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF/resolve/main/qwen2.5-coder-1.5b-instruct-q4_k_m.gguf",
        "description": "Specialized in shell scripting, configuration files, and terminal CLI command generation.",
    },
    {
        "id": "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        "name": "Qwen 2.5 (0.5B Instruct Q4_K_M) — Ultra-Lightweight Edge Model",
        "size": "~398 MB",
        "ram_required": "1 GB RAM",
        "url": "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf",
        "description": "Ultra-compact model for minimal CPU/RAM footprint on older or resource-constrained PCs.",
    },
]

RECOMMENDED_MINI_MODELS: List[Dict[str, str]] = RECOMMENDED_QWEN_GGUF_MODELS + [
    {
        "id": "qwen2.5:1.5b",
        "name": "Qwen 2.5 (1.5B) — Ollama Version",
        "size": "~986 MB",
        "ram_required": "2 GB RAM",
        "url": "",
        "description": "Fast CPU inference via external Ollama service.",
    },
    {
        "id": "llama3.2:1b",
        "name": "Llama 3.2 (1B) — Ultra-Light Edge Model",
        "size": "~1.3 GB",
        "ram_required": "2 GB RAM",
        "url": "",
        "description": "Meta's ultra-compact instruction model for low-resource Linux systems.",
    },
    {
        "id": "smollm2:1.7b",
        "name": "SmolLM2 (1.7B) — Compact On-Device Assistant",
        "size": "~1.0 GB",
        "ram_required": "2.5 GB RAM",
        "url": "",
        "description": "HuggingFace's compact model designed specifically for local CPU execution.",
    },
]


class LocalLLMManager:
    """
    Manages local offline AI runtimes (in-app llama.cpp engine, Ollama, LM Studio) and provides
    a built-in zero-dependency Linux Mini-Assistant for offline diagnostics and generic Linux tasks.
    """

    APP_DATA_DIR = os.path.expanduser("~/.local/share/io.github.antonio.Mithrandir/llm")
    BIN_DIR = os.path.join(APP_DATA_DIR, "bin")
    MODELS_DIR = os.path.join(APP_DATA_DIR, "models")
    LLAMACPP_PORT = 18080
    LLAMACPP_BASE_URL = f"http://127.0.0.1:{LLAMACPP_PORT}/v1"

    _llama_proc: Optional[subprocess.Popen] = None
    _llama_proc_lock: threading.Lock = threading.Lock()
    _active_model: Optional[str] = None

    PROVIDER_PRESETS: Dict[str, Dict[str, str]] = {
        "llamacpp": {
            "label": "Mithrandir In-App Mini-LLM (Qwen 2.5 + llama.cpp — Offline)",
            "base_url": "http://127.0.0.1:18080/v1",
            "default_model": "qwen2.5-1.5b-instruct-q4_k_m.gguf",
            "needs_key": "false",
        },
        "builtin": {
            "label": "Built-in Linux Mini-Assistant (Offline, Zero-Download)",
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

    # --- Managed llama.cpp + Qwen GGUF In-App Engine ---

    @classmethod
    def find_llama_server_binary(cls) -> Optional[str]:
        """Locates the llama-server binary in Mithrandir app storage or host PATH."""
        internal_bin = os.path.join(cls.BIN_DIR, "llama-server")
        if os.path.isfile(internal_bin) and os.access(internal_bin, os.X_OK):
            return internal_bin
        system_bin = HostBridge.which("llama-server") or HostBridge.which("llama-cli")
        if system_bin:
            return system_bin
        return None

    @classmethod
    def get_installed_gguf_models(cls) -> List[str]:
        """Returns filenames of downloaded .gguf models in Mithrandir storage."""
        if not os.path.isdir(cls.MODELS_DIR):
            return []
        try:
            files = []
            for f in sorted(os.listdir(cls.MODELS_DIR)):
                if f.endswith(".gguf") and not f.endswith(".part"):
                    full_p = os.path.join(cls.MODELS_DIR, f)
                    if os.path.isfile(full_p) and os.path.getsize(full_p) > 10 * 1024 * 1024:
                        files.append(f)
            return files
        except Exception:
            return []

    @classmethod
    def is_llamacpp_server_running(cls) -> bool:
        """Checks if the local llama-server instance is responsive on the configured port."""
        try:
            req = urllib.request.Request(
                f"http://127.0.0.1:{cls.LLAMACPP_PORT}/health",
                headers={"User-Agent": "Mithrandir/0.4.0"}
            )
            with urllib.request.urlopen(req, timeout=1.5) as resp:
                if resp.status in (200, 503):
                    return True
        except Exception:
            pass

        try:
            req = urllib.request.Request(
                f"http://127.0.0.1:{cls.LLAMACPP_PORT}/v1/models",
                headers={"User-Agent": "Mithrandir/0.4.0"}
            )
            with urllib.request.urlopen(req, timeout=1.5) as resp:
                if resp.status == 200:
                    return True
        except Exception:
            pass

        return False

    @classmethod
    def check_llamacpp_status(cls) -> Dict[str, Any]:
        """Returns full status of the internal llama.cpp engine and downloaded GGUF weights."""
        engine_bin = cls.find_llama_server_binary()
        engine_installed = engine_bin is not None
        installed_models = cls.get_installed_gguf_models()
        is_running = cls.is_llamacpp_server_running()

        total_bytes = 0
        if os.path.isdir(cls.APP_DATA_DIR):
            for root, _dirs, files in os.walk(cls.APP_DATA_DIR):
                for f in files:
                    try:
                        total_bytes += os.path.getsize(os.path.join(root, f))
                    except Exception:
                        pass

        default_model = "qwen2.5-1.5b-instruct-q4_k_m.gguf"
        return {
            "engine_installed": engine_installed,
            "engine_path": engine_bin or "",
            "running": is_running,
            "models": installed_models,
            "default_model_installed": default_model in installed_models,
            "disk_usage_mb": round(total_bytes / (1024 * 1024), 1),
            "port": cls.LLAMACPP_PORT,
            "base_url": cls.LLAMACPP_BASE_URL,
            "active_model": cls._active_model or (installed_models[0] if installed_models else default_model),
        }

    @classmethod
    def install_llamacpp_engine(
        cls,
        line_callback: Optional[Callable[[str], None]] = None,
        cancel_event: Optional[threading.Event] = None
    ) -> Tuple[bool, str]:
        """Downloads and unpacks the official prebuilt llama.cpp binaries for Linux."""
        os.makedirs(cls.BIN_DIR, mode=0o755, exist_ok=True)
        if cls.find_llama_server_binary() and os.path.exists(os.path.join(cls.BIN_DIR, "llama-server")):
            return True, "llama.cpp runtime is already installed."

        if line_callback:
            line_callback("[Mithrandir] Resolving official llama.cpp Linux release from GitHub...")

        arch = platform.machine().lower()
        if arch in ("x86_64", "amd64"):
            asset_suffix = "bin-ubuntu-x64.zip"
        elif "arm" in arch or "aarch64" in arch:
            asset_suffix = "bin-ubuntu-arm64.zip"
        else:
            asset_suffix = "bin-ubuntu-x64.zip"

        download_url = None
        try:
            req = urllib.request.Request(
                "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest",
                headers={"User-Agent": "Mithrandir-Linux/0.4.0"}
            )
            with urllib.request.urlopen(req, timeout=10) as resp:
                if resp.status == 200:
                    data = json.loads(resp.read().decode("utf-8"))
                    for asset in data.get("assets", []):
                        aname = asset.get("name", "")
                        if aname.endswith(asset_suffix) and "noavx" not in aname:
                            download_url = asset.get("browser_download_url")
                            break
        except Exception as e:
            if line_callback:
                line_callback(f"[Mithrandir] Notice: {e}")

        if not download_url:
            download_url = f"https://github.com/ggml-org/llama.cpp/releases/latest/download/llama-b4000-{asset_suffix}"

        if line_callback:
            line_callback(f"[Mithrandir] Downloading llama.cpp runtime: {download_url}")

        tmp_zip = os.path.join(cls.APP_DATA_DIR, "llama_tmp.zip")
        try:
            req = urllib.request.Request(download_url, headers={"User-Agent": "Mithrandir-Linux/0.4.0"})
            with urllib.request.urlopen(req, timeout=60) as response:
                total_size = int(response.headers.get("Content-Length", 0))
                downloaded = 0
                while True:
                    if cancel_event and cancel_event.is_set():
                        if line_callback:
                            line_callback("[Mithrandir] Installation cancelled by user.")
                        if os.path.exists(tmp_zip):
                            os.remove(tmp_zip)
                        return False, "Installation cancelled."
                    chunk = response.read(256 * 1024)
                    if not chunk:
                        break
                    downloaded += len(chunk)
                    if line_callback and total_size > 0:
                        pct = (downloaded / total_size) * 100
                        mb_down = downloaded / (1024 * 1024)
                        mb_tot = total_size / (1024 * 1024)
                        line_callback(f"[Mithrandir] Fetching llama.cpp: {mb_down:.1f} MB / {mb_tot:.1f} MB ({pct:.1f}%)")

            if line_callback:
                line_callback("[Mithrandir] Extracting binaries and libraries to internal storage...")

            with zipfile.ZipFile(tmp_zip, "r") as zf:
                zf.extractall(cls.BIN_DIR)

            for root, _dirs, files in os.walk(cls.BIN_DIR):
                for f in files:
                    fp = os.path.join(root, f)
                    try:
                        st = os.stat(fp)
                        os.chmod(fp, st.st_mode | 0o755)
                    except Exception:
                        pass

            if os.path.exists(tmp_zip):
                os.remove(tmp_zip)

            found_server = cls.find_llama_server_binary()
            if found_server:
                if line_callback:
                    line_callback(f"[Mithrandir] ✓ llama.cpp installed successfully at {found_server}")
                return True, "llama.cpp installed successfully."
            return False, "Extraction completed but llama-server binary was not found."

        except Exception as e:
            if os.path.exists(tmp_zip):
                try:
                    os.remove(tmp_zip)
                except Exception:
                    pass
            return False, f"Failed to install llama.cpp: {e}"

    @classmethod
    def download_gguf_model(
        cls,
        model_id: str,
        line_callback: Optional[Callable[[str], None]] = None,
        cancel_event: Optional[threading.Event] = None
    ) -> Tuple[bool, str]:
        """Downloads a Qwen GGUF model directly from Hugging Face into internal storage."""
        os.makedirs(cls.MODELS_DIR, mode=0o755, exist_ok=True)
        dest_file = os.path.join(cls.MODELS_DIR, model_id)
        if os.path.isfile(dest_file) and os.path.getsize(dest_file) > 10 * 1024 * 1024:
            return True, f"Model '{model_id}' is already downloaded."

        target_meta = None
        for m in RECOMMENDED_QWEN_GGUF_MODELS:
            if m["id"] == model_id:
                target_meta = m
                break

        if not target_meta:
            target_meta = RECOMMENDED_QWEN_GGUF_MODELS[0]
            model_id = target_meta["id"]
            dest_file = os.path.join(cls.MODELS_DIR, model_id)

        model_url = target_meta["url"]
        part_file = dest_file + ".part"

        if line_callback:
            line_callback(f"[Mithrandir] Connecting to Hugging Face for {target_meta['name']}...")
            line_callback(f"[Mithrandir] URL: {model_url}")

        try:
            req = urllib.request.Request(model_url, headers={"User-Agent": "Mithrandir-Linux/0.4.0"})
            with urllib.request.urlopen(req, timeout=60) as response:
                total_bytes = int(response.headers.get("Content-Length", 0))
                downloaded = 0
                last_report_pct = -1

                with open(part_file, "wb") as f_out:
                    while True:
                        if cancel_event and cancel_event.is_set():
                            if line_callback:
                                line_callback("[Mithrandir] Download cancelled by user.")
                            if os.path.exists(part_file):
                                os.remove(part_file)
                            return False, "Download cancelled."

                        chunk = response.read(1024 * 1024)
                        if not chunk:
                            break
                        f_out.write(chunk)
                        downloaded += len(chunk)

                        if total_bytes > 0:
                            pct = int((downloaded / total_bytes) * 100)
                            if pct != last_report_pct and (pct % 5 == 0 or downloaded == total_bytes):
                                last_report_pct = pct
                                mb_down = downloaded / (1024 * 1024)
                                mb_tot = total_bytes / (1024 * 1024)
                                if line_callback:
                                    line_callback(f"[Mithrandir] Downloading {model_id}: {mb_down:.1f} MB / {mb_tot:.1f} MB ({pct}%)")

            if os.path.exists(dest_file):
                os.remove(dest_file)
            os.rename(part_file, dest_file)

            if line_callback:
                line_callback(f"[Mithrandir] ✓ Model '{model_id}' downloaded and ready!")
            return True, f"Model '{model_id}' is ready for offline use."

        except Exception as e:
            if os.path.exists(part_file):
                try:
                    os.remove(part_file)
                except Exception:
                    pass
            return False, f"Failed to download model '{model_id}': {e}"

    @classmethod
    def install_qwen_with_llamacpp(
        cls,
        model_id: str = "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        line_callback: Optional[Callable[[str], None]] = None,
        cancel_event: Optional[threading.Event] = None
    ) -> Tuple[bool, str]:
        """All-in-one setup: installs llama.cpp and downloads Qwen model weights."""
        if not cls.find_llama_server_binary():
            if line_callback:
                line_callback("[Mithrandir] Step 1 of 2: Installing llama.cpp inference engine...")
            ok_engine, msg_engine = cls.install_llamacpp_engine(line_callback, cancel_event)
            if not ok_engine:
                return False, msg_engine

        if line_callback:
            line_callback(f"[Mithrandir] Step 2 of 2: Downloading Qwen Mini-LLM weights ({model_id})...")
        ok_model, msg_model = cls.download_gguf_model(model_id, line_callback, cancel_event)
        if not ok_model:
            return False, msg_model

        if line_callback:
            line_callback("[Mithrandir] Starting embedded llama.cpp server with Qwen 2.5...")
        ok_start, _ = cls.start_llamacpp_server(model_id)
        if ok_start and line_callback:
            line_callback("[Mithrandir] ✓ Qwen 2.5 Mini-LLM is active and ready inside Mithrandir!")
        return True, "Qwen 2.5 Mini-LLM and llama.cpp runtime installed successfully."

    @classmethod
    def start_llamacpp_server(cls, model_id: Optional[str] = None) -> Tuple[bool, str]:
        """Starts the managed in-app llama-server process serving the selected GGUF model."""
        with cls._llama_proc_lock:
            if cls.is_llamacpp_server_running():
                return True, f"llama.cpp is already running on {cls.LLAMACPP_BASE_URL}."

            server_bin = cls.find_llama_server_binary()
            if not server_bin:
                return False, "llama-server binary is not installed yet. Click 'Install Qwen + llama.cpp' first."

            installed = cls.get_installed_gguf_models()
            if not installed:
                return False, "No GGUF models downloaded yet. Click 'Install Qwen + llama.cpp' first."

            target_model = model_id if (model_id and model_id in installed) else installed[0]
            model_path = os.path.join(cls.MODELS_DIR, target_model)
            if not os.path.isfile(model_path):
                return False, f"Model file not found at {model_path}."

            if cls._llama_proc:
                try:
                    cls._llama_proc.terminate()
                    cls._llama_proc.wait(timeout=2)
                except Exception:
                    pass
                cls._llama_proc = None

            env = os.environ.copy()
            ld_path = cls.BIN_DIR
            if "LD_LIBRARY_PATH" in env:
                ld_path = f"{ld_path}:{env['LD_LIBRARY_PATH']}"
            env["LD_LIBRARY_PATH"] = ld_path

            cmd = [
                server_bin,
                "-m", model_path,
                "--host", "127.0.0.1",
                "--port", str(cls.LLAMACPP_PORT),
                "-c", "2048",
            ]

            try:
                cls._llama_proc = subprocess.Popen(
                    cmd,
                    env=env,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    start_new_session=True
                )
                cls._active_model = target_model
            except Exception as e:
                return False, f"Failed to start llama-server process: {e}"

        for _ in range(20):
            time.sleep(0.5)
            if cls.is_llamacpp_server_running():
                return True, f"llama.cpp server started successfully on http://127.0.0.1:{cls.LLAMACPP_PORT}/v1."

        return False, "llama-server started but did not respond on port 18080 within 10s."

    @classmethod
    def stop_llamacpp_server(cls) -> Tuple[bool, str]:
        """Gracefully terminates the in-app llama-server process."""
        with cls._llama_proc_lock:
            if cls._llama_proc:
                try:
                    cls._llama_proc.terminate()
                    cls._llama_proc.wait(timeout=3)
                except Exception:
                    try:
                        cls._llama_proc.kill()
                    except Exception:
                        pass
                cls._llama_proc = None
            cls._active_model = None

        try:
            HostBridge.run_command(["fuser", "-k", f"{cls.LLAMACPP_PORT}/tcp"], timeout=2)
        except Exception:
            pass

        return True, "llama.cpp server stopped."

    @classmethod
    def uninstall_llamacpp_and_models(cls, remove_engine: bool = True, remove_models: bool = True) -> Tuple[bool, str]:
        """Removes downloaded model weights and/or llama.cpp binaries from internal storage."""
        cls.stop_llamacpp_server()

        freed_msg = []
        if remove_models and os.path.isdir(cls.MODELS_DIR):
            try:
                shutil.rmtree(cls.MODELS_DIR)
                freed_msg.append("model weights removed")
            except Exception as e:
                return False, f"Could not remove models: {e}"

        if remove_engine and os.path.isdir(cls.BIN_DIR):
            try:
                shutil.rmtree(cls.BIN_DIR)
                freed_msg.append("llama.cpp engine removed")
            except Exception as e:
                return False, f"Could not remove engine: {e}"

        return True, f"Uninstalled successfully ({', '.join(freed_msg) if freed_msg else 'storage cleaned'})."

    # --- Ollama Support (Optional External Runtime) ---

    @classmethod
    def check_ollama_status(cls) -> Dict[str, Any]:
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
