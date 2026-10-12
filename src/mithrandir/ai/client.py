import json
import urllib.request
import urllib.error
from typing import List, Dict, Any, Optional

class AIClient:
    """
    OpenAI-compatible client supporting both cloud providers (OpenAI)
    and local offline models (LM Studio, Ollama, LocalAI).
    """

    def __init__(self, base_url: str = "http://localhost:1234/v1", api_key: str = ""):
        self.base_url = base_url.rstrip('/')
        self.api_key = api_key or "lm-studio"

    def list_models(self) -> List[str]:
        url = f"{self.base_url}/models"
        req = urllib.request.Request(url, headers={
            'Authorization': f"Bearer {self.api_key}",
            'Content-Type': 'application/json'
        })
        try:
            with urllib.request.urlopen(req, timeout=5) as response:
                if response.status == 200:
                    data = json.loads(response.read().decode('utf-8'))
                    models = []
                    for item in data.get('data', []):
                        models.append(item.get('id', ''))
                    return [m for m in models if m]
        except Exception:
            pass
        return []

    def chat_completion(self, messages: List[Dict[str, str]], model: Optional[str] = None, temperature: float = 0.3) -> Dict[str, Any]:
        """
        Sends chat completion request to the OpenAI-compatible endpoint.
        """
        url = f"{self.base_url}/chat/completions"
        payload = {
            "model": model or "local-model",
            "messages": messages,
            "temperature": temperature,
            "stream": False
        }
        body = json.dumps(payload).encode('utf-8')
        req = urllib.request.Request(url, data=body, headers={
            'Authorization': f"Bearer {self.api_key}",
            'Content-Type': 'application/json'
        })

        try:
            with urllib.request.urlopen(req, timeout=45) as response:
                if response.status == 200:
                    data = json.loads(response.read().decode('utf-8'))
                    choices = data.get('choices', [])
                    if choices:
                        content = choices[0].get('message', {}).get('content', '')
                        return {'success': True, 'content': content}
            return {'success': False, 'error': f"HTTP {response.status}"}
        except urllib.error.HTTPError as e:
            return {'success': False, 'error': f"HTTP Error {e.code}: {e.reason}"}
        except urllib.error.URLError as e:
            return {'success': False, 'error': f"Connection Error: {e.reason}"}
        except Exception as e:
            return {'success': False, 'error': str(e)}
