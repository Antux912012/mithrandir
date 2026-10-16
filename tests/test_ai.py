import unittest
from unittest.mock import patch, MagicMock
import json
from mithrandir.ai.client import AIClient
from mithrandir.ai.local_llm import LocalLLMManager, BuiltinLinuxAssistant, RECOMMENDED_MINI_MODELS
from mithrandir.ui.ai_page import AIPage


class TestAIIntegration(unittest.TestCase):

    def test_url_normalization(self):
        """Ensure OpenAI-compatible and local URLs normalize cleanly."""
        c1 = AIClient(base_url="https://integrate.api.nvidia.com")
        self.assertEqual(c1.base_url, "https://integrate.api.nvidia.com/v1")

        c2 = AIClient(base_url="https://integrate.api.nvidia.com/v1/chat/completions")
        self.assertEqual(c2.base_url, "https://integrate.api.nvidia.com/v1")

        c3 = AIClient(base_url="http://localhost:11434")
        self.assertEqual(c3.base_url, "http://localhost:11434/v1")

        c4 = AIClient(base_url="builtin://linux-mini")
        self.assertTrue(c4.is_builtin())

    def test_nim_model_selection_and_headers(self):
        """Ensure NVIDIA NIM API key header and selected model are sent in chat_completion."""
        client = AIClient(
            base_url="https://integrate.api.nvidia.com/v1",
            api_key="nvapi-test-key-123",
            model="meta/llama-3.1-70b-instruct"
        )
        headers = client._build_headers()
        self.assertEqual(headers["Authorization"], "Bearer nvapi-test-key-123")
        self.assertIn("Mithrandir", headers["User-Agent"])

        captured_req = {}

        def fake_urlopen(req, timeout=60):
            captured_req["url"] = req.full_url
            captured_req["headers"] = dict(req.header_items())
            captured_req["body"] = json.loads(req.data.decode("utf-8"))
            resp = MagicMock()
            resp.status = 200
            resp.read.return_value = json.dumps({
                "choices": [{"message": {"content": "System looks healthy.\nPROPOSED_COMMAND: df -h /"}}]
            }).encode("utf-8")
            resp.__enter__.return_value = resp
            return resp

        with patch("urllib.request.urlopen", side_effect=fake_urlopen):
            res = client.chat_completion(
                [{"role": "user", "content": "Diagnose"}],
                model="qwen/qwen2.5-coder-32b-instruct"
            )
            self.assertTrue(res["success"])
            self.assertEqual(res["model"], "qwen/qwen2.5-coder-32b-instruct")
            self.assertEqual(captured_req["body"]["model"], "qwen/qwen2.5-coder-32b-instruct")
            self.assertEqual(captured_req["url"], "https://integrate.api.nvidia.com/v1/chat/completions")

    def test_list_models_extraction(self):
        """Ensure list_models parses OpenAI /v1/models and Ollama formats."""
        payload_openai = {"data": [{"id": "model-alpha"}, {"id": "model-beta"}]}
        extracted = AIClient._extract_models_from_payload(payload_openai)
        self.assertEqual(extracted, ["model-alpha", "model-beta"])

        payload_ollama = {"models": [{"name": "qwen2.5:1.5b"}, {"name": "llama3.2:1b"}]}
        extracted_ollama = AIClient._extract_models_from_payload(payload_ollama)
        self.assertEqual(extracted_ollama, ["qwen2.5:1.5b", "llama3.2:1b"])

    def test_builtin_offline_diagnostics(self):
        """Ensure BuiltinLinuxAssistant analyzes system state and proposes a command offline."""
        client = AIClient(base_url="builtin://linux-mini", model="builtin-linux-mini")
        prompt = (
            "You are a GNU/Linux system diagnostics assistant. Analyze this system state:\n"
            "- OS: Ubuntu 24.04 LTS\n"
            "- Kernel: 6.8.0-generic\n"
            "- Desktop: GNOME\n"
            "- Memory: 4200MB used of 16000MB (26.2%)\n"
            "- Disk: 40GB free on root (50.0% used)\n"
            "- Failed systemd units:\nNone\n"
            "- Recent system errors:\nNo critical errors found.\n\n"
            "Provide a brief diagnosis of any anomalies"
        )
        res = client.chat_completion([{"role": "user", "content": prompt}])
        self.assertTrue(res["success"])
        self.assertIn("MITHRANDIR BUILT-IN OFFLINE LINUX DIAGNOSTICS", res["content"])
        self.assertIn("PROPOSED_COMMAND:", res["content"])

    def test_builtin_offline_generic_tasks(self):
        """Ensure BuiltinLinuxAssistant handles generic Linux tasks offline."""
        res_files = BuiltinLinuxAssistant.respond([{"role": "user", "content": "Find large files over 100MB"}])
        self.assertIn("PROPOSED_COMMAND:", res_files)
        self.assertIn("find ~ -type f -size +100M", res_files)

        res_ports = BuiltinLinuxAssistant.respond([{"role": "user", "content": "Check open network ports"}])
        self.assertIn("ss -tulnp", res_ports)

        res_pkg = BuiltinLinuxAssistant.respond([{"role": "user", "content": "Fix broken apt packages"}])
        self.assertIn("dpkg --configure -a", res_pkg)

    def test_extract_proposed_command_variants(self):
        """Ensure _extract_proposed_command handles inline, bold, and code block formatting."""
        cmd1 = AIPage._extract_proposed_command("Diagnosis complete.\nPROPOSED_COMMAND: `systemctl status sshd`")
        self.assertEqual(cmd1, "systemctl status sshd")

        cmd2 = AIPage._extract_proposed_command("**PROPOSED_COMMAND:** journalctl -p 3 -xb")
        self.assertEqual(cmd2, "journalctl -p 3 -xb")

        cmd3 = AIPage._extract_proposed_command("PROPOSED_COMMAND:\n```bash\ndf -h /\n```")
        self.assertEqual(cmd3, "df -h /")

    def test_recommended_mini_models_catalog(self):
        """Ensure curated offline mini-LLMs are present and hardware recommendation works."""
        self.assertGreaterEqual(len(RECOMMENDED_MINI_MODELS), 4)
        hw = LocalLLMManager.get_hardware_recommendation()
        self.assertIn("recommended_id", hw)
        self.assertIn("summary", hw)


if __name__ == "__main__":
    unittest.main()
