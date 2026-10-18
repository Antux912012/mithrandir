import json
import urllib.request
import urllib.error
from typing import List, Dict, Any, Optional
from .local_llm import LocalLLMManager, BuiltinLinuxAssistant


class AIClient:
    """
    AI client supporting Google Gemini (Google AI Studio), Anthropic Claude,
    OpenAI-compatible cloud providers (NVIDIA NIM, OpenAI, OpenRouter),
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

        # Strip accidental Gemini :generateContent suffix if user pasted full endpoint
        if ":generateContent" in cleaned:
            idx = cleaned.find("/models/")
            if idx != -1:
                cleaned = cleaned[:idx].rstrip("/")

        # Strip accidental endpoint suffixes if user pasted full completions/models/messages URL
        for suffix in ("/chat/completions", "/completions", "/messages", "/models"):
            if cleaned.endswith(suffix):
                cleaned = cleaned[: -len(suffix)].rstrip("/")

        if cleaned == "https://generativelanguage.googleapis.com":
            return "https://generativelanguage.googleapis.com/v1beta"

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
            "https://api.anthropic.com",
        ):
            cleaned = f"{cleaned}/v1"
        return cleaned

    def is_builtin(self) -> bool:
        return self.base_url.startswith("builtin://") or self.default_model == "builtin-linux-mini"

    def is_gemini_native(self) -> bool:
        return (
            "generativelanguage.googleapis.com" in self.base_url
            and not self.base_url.endswith("/openai")
        )

    def is_gemini(self) -> bool:
        return "generativelanguage.googleapis.com" in self.base_url

    def is_claude(self) -> bool:
        return "api.anthropic.com" in self.base_url

    def _build_headers(self) -> Dict[str, str]:
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "Mithrandir/0.4.1 (Linux; GTK4)",
        }
        if self.is_gemini_native():
            if self.api_key:
                headers["x-goog-api-key"] = self.api_key
            return headers
        if self.is_claude():
            if self.api_key:
                headers["x-api-key"] = self.api_key
            headers["anthropic-version"] = "2023-06-01"
            return headers
        token = self.api_key or "lm-studio"
        headers["Authorization"] = f"Bearer {token}"
        return headers

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

        if self.is_gemini_native() and not self.api_key:
            return {
                "success": False,
                "models": list(LocalLLMManager.GEMINI_POPULAR_MODELS),
                "error": "Google AI Studio API Key is required. Get one at aistudio.google.com.",
            }

        if self.is_claude() and not self.api_key:
            return {
                "success": False,
                "models": list(LocalLLMManager.CLAUDE_POPULAR_MODELS),
                "error": "Anthropic Claude API Key is required.",
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
                    elif self.is_gemini():
                        models = self._prioritize_gemini_models(models)
                    elif self.is_claude():
                        models = self._prioritize_claude_models(models)
                    return {
                        "success": True,
                        "models": models,
                        "message": f"Connected ({len(models)} models available)",
                    }
                return {"success": False, "models": [], "error": f"HTTP {response.status}"}
        except urllib.error.HTTPError as e:
            err_detail = self._parse_http_error(e)
            if "integrate.api.nvidia.com" in self.base_url and e.code in (401, 403):
                return {
                    "success": False,
                    "models": list(LocalLLMManager.NIM_POPULAR_MODELS),
                    "error": f"HTTP {e.code} (Check your NVIDIA NIM API Key): {err_detail}",
                }
            if self.is_gemini() and e.code in (400, 401, 403):
                return {
                    "success": False,
                    "models": list(LocalLLMManager.GEMINI_POPULAR_MODELS),
                    "error": f"HTTP {e.code} (Check your Google AI Studio API Key): {err_detail}",
                }
            if self.is_claude() and e.code in (400, 401, 403):
                return {
                    "success": False,
                    "models": list(LocalLLMManager.CLAUDE_POPULAR_MODELS),
                    "error": f"HTTP {e.code} (Check your Anthropic API Key): {err_detail}",
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
        if self.is_gemini():
            return list(LocalLLMManager.GEMINI_POPULAR_MODELS)
        if self.is_claude():
            return list(LocalLLMManager.CLAUDE_POPULAR_MODELS)
        return []

    @staticmethod
    def _extract_models_from_payload(data: Any) -> List[str]:
        models: List[str] = []
        if isinstance(data, dict):
            items = data.get("data") or data.get("models") or []
            if isinstance(items, list):
                for item in items:
                    if isinstance(item, dict):
                        # Filter out non-generative Gemini embedding/aqa models when supportedGenerationMethods is present
                        methods = item.get("supportedGenerationMethods")
                        if isinstance(methods, list) and "generateContent" not in methods:
                            continue
                        mid = item.get("id") or item.get("name") or item.get("model") or ""
                        if mid:
                            mid_str = str(mid)
                            if mid_str.startswith("models/"):
                                mid_str = mid_str[len("models/"):]
                            models.append(mid_str)
                    elif isinstance(item, str):
                        m_str = item[len("models/"):] if item.startswith("models/") else item
                        models.append(m_str)
        elif isinstance(data, list):
            for item in data:
                if isinstance(item, dict):
                    mid = item.get("id") or item.get("name") or ""
                    if mid:
                        mid_str = str(mid)
                        if mid_str.startswith("models/"):
                            mid_str = mid_str[len("models/"):]
                        models.append(mid_str)
                elif isinstance(item, str):
                    m_str = item[len("models/"):] if item.startswith("models/") else item
                    models.append(m_str)
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
    def _prioritize_gemini_models(models: List[str]) -> List[str]:
        popular = LocalLLMManager.GEMINI_POPULAR_MODELS
        top = [m for m in popular if m in models]
        for m in popular:
            if m not in top:
                top.append(m)
        rest = sorted([m for m in models if m not in top])
        return top + rest

    @staticmethod
    def _prioritize_claude_models(models: List[str]) -> List[str]:
        popular = LocalLLMManager.CLAUDE_POPULAR_MODELS
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
            return chosen[len("models/"):] if chosen.startswith("models/") else chosen

        if "18080" in self.base_url:
            installed = LocalLLMManager.get_installed_gguf_models()
            if installed:
                return installed[0]
            return "qwen2.5-1.5b-instruct-q4_k_m.gguf"

        if self.is_gemini():
            return "gemini-2.5-flash"
        if self.is_claude():
            return "claude-3-5-sonnet-latest"

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

    def _chat_completion_gemini(
        self,
        messages: List[Dict[str, str]],
        target_model: str,
        temperature: float = 0.3,
    ) -> Dict[str, Any]:
        clean_model = target_model[len("models/"):] if target_model.startswith("models/") else target_model
        url = f"{self.base_url}/models/{clean_model}:generateContent"

        system_parts = []
        contents = []
        for msg in messages:
            role = msg.get("role", "user")
            text = msg.get("content", "")
            if role == "system":
                if text:
                    system_parts.append({"text": text})
            elif role in ("assistant", "model"):
                contents.append({"role": "model", "parts": [{"text": text}]})
            else:
                contents.append({"role": "user", "parts": [{"text": text}]})

        if not contents and system_parts:
            contents.append({"role": "user", "parts": system_parts})
            system_parts = []

        payload: Dict[str, Any] = {
            "contents": contents,
            "generationConfig": {
                "temperature": temperature,
                "maxOutputTokens": 1024,
            },
        }
        if system_parts:
            payload["systemInstruction"] = {"parts": system_parts}

        body = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(url, data=body, headers=self._build_headers())

        try:
            with urllib.request.urlopen(req, timeout=60) as response:
                if response.status == 200:
                    data = json.loads(response.read().decode("utf-8", errors="ignore"))
                    candidates = data.get("candidates", [])
                    if candidates:
                        parts = candidates[0].get("content", {}).get("parts", [])
                        chunks = [p.get("text", "") for p in parts if isinstance(p, dict) and p.get("text")]
                        content = "".join(chunks).strip()
                        return {"success": True, "content": content, "model": clean_model}
                return {"success": False, "error": f"HTTP {response.status}"}
        except urllib.error.HTTPError as e:
            detail = self._parse_http_error(e)
            return {"success": False, "error": f"HTTP Error {e.code} (model '{clean_model}'): {detail}"}
        except urllib.error.URLError as e:
            return {"success": False, "error": f"Connection Error: {e.reason}"}
        except Exception as e:
            return {"success": False, "error": str(e)}

    def _chat_completion_claude(
        self,
        messages: List[Dict[str, str]],
        target_model: str,
        temperature: float = 0.3,
    ) -> Dict[str, Any]:
        url = f"{self.base_url}/messages"
        system_chunks = []
        chat_msgs = []
        for msg in messages:
            role = msg.get("role", "user")
            text = msg.get("content", "")
            if role == "system":
                if text:
                    system_chunks.append(text)
            else:
                chat_msgs.append({"role": "assistant" if role == "assistant" else "user", "content": text})

        if not chat_msgs and system_chunks:
            chat_msgs.append({"role": "user", "content": "\n\n".join(system_chunks)})
            system_chunks = []

        payload: Dict[str, Any] = {
            "model": target_model,
            "messages": chat_msgs,
            "max_tokens": 1024,
            "temperature": temperature,
        }
        if system_chunks:
            payload["system"] = "\n\n".join(system_chunks)

        body = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(url, data=body, headers=self._build_headers())

        try:
            with urllib.request.urlopen(req, timeout=60) as response:
                if response.status == 200:
                    data = json.loads(response.read().decode("utf-8", errors="ignore"))
                    content_blocks = data.get("content", [])
                    chunks = [b.get("text", "") for b in content_blocks if isinstance(b, dict) and b.get("text")]
                    content = "".join(chunks).strip()
                    return {"success": True, "content": content, "model": target_model}
                return {"success": False, "error": f"HTTP {response.status}"}
        except urllib.error.HTTPError as e:
            detail = self._parse_http_error(e)
            return {"success": False, "error": f"HTTP Error {e.code} (model '{target_model}'): {detail}"}
        except urllib.error.URLError as e:
            return {"success": False, "error": f"Connection Error: {e.reason}"}
        except Exception as e:
            return {"success": False, "error": str(e)}

    def chat_completion(
        self,
        messages: List[Dict[str, str]],
        model: Optional[str] = None,
        temperature: float = 0.3
    ) -> Dict[str, Any]:
        """
        Sends chat completion request to Google Gemini (Google AI Studio), Anthropic Claude,
        OpenAI-compatible endpoint, or built-in offline assistant.
        """
        target_model = self.resolve_model(model)

        if self.base_url.startswith("builtin://") or target_model == "builtin-linux-mini":
            content = BuiltinLinuxAssistant.respond(messages)
            return {"success": True, "content": content, "model": "builtin-linux-mini"}

        if self.is_gemini_native():
            return self._chat_completion_gemini(messages, target_model, temperature)

        if self.is_claude():
            return self._chat_completion_claude(messages, target_model, temperature)

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
