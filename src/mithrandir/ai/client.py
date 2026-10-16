import json
import urllib.request
import urllib.error
from typing import List, Dict, Any, Optional
from .local_llm import LocalLLMManager, BuiltinLinuxAssistant


class AIClient:
    """
    OpenAI-compatible client supporting cloud providers (NVIDIA NIM, OpenAI, OpenRouter),
    local offline models (Ollama, LM Studio, llama.cpp, LocalAI), and the built-in
    zero-dependency Linux Mini-Assistant.
    """

    def __init__(self, base_url: str = "http://localhost:11434/v1", api_key: str = "", model: str = ""):
        self.raw_url = (base_url or "").strip()
        self.base_url = self._normalize_base_url(self.raw_url)
        self.api_key = (api_key or "").strip()
        self.default_model = (model or "").strip()

    @staticmethod
    def _normalize_base_url(url: str) -> str:
        if not url:
            return "http://localhost:11434/v1"
        if url.startswith("builtin://"):
            return url
        cleaned = url.rstrip("/")
        # Strip accidental endpoint suffixes if user pasted full completions/models URL
        for suffix in ("/chat/completions", "/completions", "/models"):
            if cleaned.endswith(suffix):
                cleaned = cleaned[: -len(suffix)].rstrip("/")

        # Auto-append /v1 for known roots when omitted
        if cleaned in (
            "http://localhost:18080",
            "http://127.0.0.1:18080",
            "http://localhost:11434",
            "http://127.0.0.1:11434",
            "http://localhost:1234",
            "http://127.0.0.1:1234",
            "http://localhost:8080",
            "http://127.0.0.1:8080",
            "https://integrate.api.nvidia.com",
            "https://api.openai.com",
        ):
            cleaned = f"{cleaned}/v1"
        return cleaned

    def _build_headers(self) -> Dict[str, str]:
        token = self.api_key or "lm-studio"
        return {
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "Mithrandir/0.3.5 (Linux; GTK4)",
        }

    def is_builtin(self) -> bool:
        return self.base_url.startswith("builtin://") or self.default_model == "builtin-linux-mini"

    def test_connection(self) -> Dict[str, Any]:
        """
        Tests connectivity to the configured endpoint and returns available models
        along with detailed status or error messages.
        """
        if self.base_url.startswith("builtin://"):
            return {
                "success": True,
                "models": ["builtin-linux-mini"],
                "message": "Built-in Offline Linux Mini-Assistant is active.",
            }

        if "18080" in self.base_url:
            st = LocalLLMManager.check_llamacpp_status()
            if not st["engine_installed"]:
                return {
                    "success": False,
                    "models": [],
                    "error": "Mithrandir In-App llama.cpp engine is not installed yet. Click 'Install Qwen + llama.cpp' in AI Diagnostics.",
                }
            if not st["models"]:
                return {
                    "success": False,
                    "models": [],
                    "error": "No Qwen GGUF models downloaded yet. Click 'Install Qwen + llama.cpp' to download weights (~986 MB).",
                }
            if not st["running"]:
                ok_start, _ = LocalLLMManager.start_llamacpp_server(self.default_model)
                if not ok_start:
                    return {
                        "success": False,
                        "models": st["models"],
                        "error": "Failed to auto-start in-app llama.cpp server. Click 'Start Engine' in AI Diagnostics.",
                    }

        url = f"{self.base_url}/models"
        req = urllib.request.Request(url, headers=self._build_headers())
        try:
            with urllib.request.urlopen(req, timeout=8) as response:
                if response.status == 200:
                    raw = response.read().decode("utf-8", errors="ignore")
                    data = json.loads(raw)
                    models = self._extract_models_from_payload(data)
                    if not models and ("18080" in self.base_url):
                        models = LocalLLMManager.get_installed_gguf_models()
                    elif not models and ("11434" in self.base_url):
                        models = LocalLLMManager.check_ollama_status().get("models", [])
                    if "integrate.api.nvidia.com" in self.base_url:
                        models = self._prioritize_nim_models(models)
                    return {
                        "success": True,
                        "models": models,
                        "message": f"Connected ({len(models)} models available)",
                    }
                return {"success": False, "models": [], "error": f"HTTP {response.status}"}
        except urllib.error.HTTPError as e:
            err_detail = self._parse_http_error(e)
            # If NVIDIA NIM /models endpoint is restricted or needs key, still offer curated NIM models if key is present
            if "integrate.api.nvidia.com" in self.base_url and e.code in (401, 403):
                return {
                    "success": False,
                    "models": list(LocalLLMManager.NIM_POPULAR_MODELS),
                    "error": f"HTTP {e.code} (Check your NVIDIA NIM API Key): {err_detail}",
                }
            return {"success": False, "models": [], "error": f"HTTP {e.code}: {err_detail}"}
        except urllib.error.URLError as e:
            if "18080" in self.base_url:
                st = LocalLLMManager.check_llamacpp_status()
                if not st["engine_installed"]:
                    return {
                        "success": False,
                        "models": [],
                        "error": "In-app llama.cpp is not installed yet. Click 'Install Qwen + llama.cpp'.",
                    }
                if not st["models"]:
                    return {
                        "success": False,
                        "models": [],
                        "error": "No Qwen GGUF models found. Click 'Install Qwen + llama.cpp'.",
                    }
                return {
                    "success": False,
                    "models": st["models"],
                    "error": "In-app llama-server is not running. Click 'Start Engine' to launch.",
                }
            # Check Ollama fallback if port 11434
            if "11434" in self.base_url:
                st = LocalLLMManager.check_ollama_status()
                if st["installed"] and not st["running"]:
                    return {
                        "success": False,
                        "models": st["models"],
                        "error": "Ollama is installed but not running. Click 'Start Local Server' or run 'ollama serve'.",
                    }
            return {"success": False, "models": [], "error": f"Connection Error: {e.reason}"}
        except Exception as e:
            return {"success": False, "models": [], "error": str(e)}

    def list_models(self) -> List[str]:
        res = self.test_connection()
        models = res.get("models", [])
        if models:
            return models
        if "integrate.api.nvidia.com" in self.base_url:
            return list(LocalLLMManager.NIM_POPULAR_MODELS)
        return []

    @staticmethod
    def _extract_models_from_payload(data: Any) -> List[str]:
        models: List[str] = []
        if isinstance(data, dict):
            items = data.get("data") or data.get("models") or []
            if isinstance(items, list):
                for item in items:
                    if isinstance(item, dict):
                        mid = item.get("id") or item.get("name") or item.get("model") or ""
                        if mid:
                            models.append(str(mid))
                    elif isinstance(item, str):
                        models.append(item)
        elif isinstance(data, list):
            for item in data:
                if isinstance(item, dict):
                    mid = item.get("id") or item.get("name") or ""
                    if mid:
                        models.append(str(mid))
                elif isinstance(item, str):
                    models.append(item)
        # Deduplicate while preserving order
        seen = set()
        unique = []
        for m in models:
            if m and m not in seen:
                seen.add(m)
                unique.append(m)
        return unique

    @staticmethod
    def _prioritize_nim_models(models: List[str]) -> List[str]:
        popular = LocalLLMManager.NIM_POPULAR_MODELS
        top = [m for m in popular if m in models]
        for m in popular:
            if m not in top:
                top.append(m)
        rest = sorted([m for m in models if m not in top])
        return top + rest

    @staticmethod
    def _parse_http_error(e: urllib.error.HTTPError) -> str:
        try:
            body = e.read().decode("utf-8", errors="ignore").strip()
            if body:
                try:
                    j = json.loads(body)
                    if isinstance(j, dict):
                        err_obj = j.get("error")
                        if isinstance(err_obj, dict) and err_obj.get("message"):
                            return str(err_obj["message"])
                        if isinstance(err_obj, str):
                            return err_obj
                        if j.get("detail"):
                            return str(j["detail"])
                        if j.get("message"):
                            return str(j["message"])
                except Exception:
                    pass
                return body[:200]
        except Exception:
            pass
        return str(e.reason)

    def resolve_model(self, explicit_model: Optional[str] = None) -> str:
        chosen = (explicit_model or self.default_model or "").strip()
        if self.base_url.startswith("builtin://") or chosen == "builtin-linux-mini":
            return "builtin-linux-mini"
        if chosen:
            return chosen

        if "18080" in self.base_url:
            installed = LocalLLMManager.get_installed_gguf_models()
            if installed:
                return installed[0]
            return "qwen2.5-1.5b-instruct-q4_k_m.gguf"

        # Try discovering available models from endpoint
        discovered = self.list_models()
        if discovered:
            return discovered[0]

        if "integrate.api.nvidia.com" in self.base_url:
            return "meta/llama-3.1-70b-instruct"
        if "api.openai.com" in self.base_url:
            return "gpt-4o-mini"
        if "11434" in self.base_url:
            return "qwen2.5:1.5b"
        return "local-model"

    def chat_completion(
        self,
        messages: List[Dict[str, str]],
        model: Optional[str] = None,
        temperature: float = 0.3
    ) -> Dict[str, Any]:
        """
        Sends chat completion request to the OpenAI-compatible endpoint or built-in offline assistant.
        """
        target_model = self.resolve_model(model)

        if self.base_url.startswith("builtin://") or target_model == "builtin-linux-mini":
            content = BuiltinLinuxAssistant.respond(messages)
            return {"success": True, "content": content, "model": "builtin-linux-mini"}

        if "18080" in self.base_url and not LocalLLMManager.is_llamacpp_server_running():
            ok_start, start_msg = LocalLLMManager.start_llamacpp_server(target_model)
            if not ok_start:
                return {"success": False, "error": f"Failed to start in-app llama.cpp: {start_msg}"}

        url = f"{self.base_url}/chat/completions"
        payload = {
            "model": target_model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": 1024,
            "stream": False,
        }
        body = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(url, data=body, headers=self._build_headers())

        try:
            with urllib.request.urlopen(req, timeout=60) as response:
                if response.status == 200:
                    data = json.loads(response.read().decode("utf-8", errors="ignore"))
                    choices = data.get("choices", [])
                    if choices:
                        msg_obj = choices[0].get("message", {})
                        content = msg_obj.get("content") or msg_obj.get("reasoning_content") or ""
                        return {"success": True, "content": content, "model": target_model}
                return {"success": False, "error": f"HTTP {response.status}"}
        except urllib.error.HTTPError as e:
            detail = self._parse_http_error(e)
            return {"success": False, "error": f"HTTP Error {e.code} (model '{target_model}'): {detail}"}
        except urllib.error.URLError as e:
            return {"success": False, "error": f"Connection Error: {e.reason}"}
        except Exception as e:
            return {"success": False, "error": str(e)}
