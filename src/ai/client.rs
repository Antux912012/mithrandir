use std::io::Write;
use std::process::Stdio;
use serde::{Deserialize, Serialize};
use crate::ai::agent_bridge::LocalAgentBridge;
use crate::ai::safety::SafetyGuard;
use crate::backend::host_bridge::HostBridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub success: bool,
    pub content: String,
    pub model: String,
    pub proposed_command: Option<String>,
}

pub struct AIClient {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl AIClient {
    pub fn new(provider: &str, base_url: &str, api_key: &str, model: &str) -> Self {
        Self {
            provider: provider.trim().to_lowercase(),
            base_url: Self::normalize_base_url(base_url),
            api_key: api_key.trim().to_string(),
            model: model.trim().to_string(),
        }
    }

    pub fn normalize_base_url(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with("agent://") {
            return trimmed.to_string();
        }

        let mut url = trimmed.trim_end_matches('/').to_string();

        for suffix in ["/chat/completions", "/completions", "/messages", "/models"] {
            if url.ends_with(suffix) {
                url = url.trim_end_matches(suffix).trim_end_matches('/').to_string();
            }
        }

        if url.contains(":generateContent") {
            if let Some(idx) = url.find("/models/") {
                url = url[..idx].trim_end_matches('/').to_string();
            }
        }

        if url == "https://api.openai.com" {
            return "https://api.openai.com/v1".to_string();
        }
        if url == "https://api.anthropic.com" {
            return "https://api.anthropic.com/v1".to_string();
        }
        if url == "https://generativelanguage.googleapis.com" {
            return "https://generativelanguage.googleapis.com/v1beta".to_string();
        }
        if url == "http://localhost:1234" || url == "http://127.0.0.1:1234" {
            return "http://localhost:1234/v1".to_string();
        }
        if url == "http://localhost:11434" || url == "http://127.0.0.1:11434" {
            return "http://localhost:11434/v1".to_string();
        }
        if url == "http://localhost:8080" || url == "http://127.0.0.1:8080" {
            return "http://localhost:8080/v1".to_string();
        }

        url
    }

    pub fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse, String> {
        // 1. Check for Local Coding Agent Sessions
        if self.provider.starts_with("agent://") || self.is_local_agent() {
            let agent_id = self.provider.trim_start_matches("agent://");
            let prompt = messages.last().map(|m| m.content.as_str()).unwrap_or("");
            return match LocalAgentBridge::query_agent(agent_id, prompt) {
                Ok(reply) => {
                    let cmd = Self::extract_proposed_command(&reply);
                    Ok(ChatResponse {
                        success: true,
                        content: reply,
                        model: format!("local-agent:{}", agent_id),
                        proposed_command: cmd,
                    })
                }
                Err(e) => Err(format!("Local agent session query failed: {}", e)),
            };
        }

        // 2. Google Gemini
        if self.provider == "gemini" || self.base_url.contains("generativelanguage.googleapis.com") {
            return self.call_gemini(messages);
        }

        // 3. Anthropic Claude
        if self.provider == "claude" || self.base_url.contains("api.anthropic.com") {
            return self.call_claude(messages);
        }

        // 4. OpenAI / Cursor / LM Studio / Ollama / llama.cpp / OpenAI-Compatible
        self.call_openai_compatible(messages)
    }

    fn is_local_agent(&self) -> bool {
        matches!(
            self.provider.as_str(),
            "antigravity" | "openclaude" | "pi-agent" | "codex"
        )
    }

    fn call_openai_compatible(&self, messages: &[ChatMessage]) -> Result<ChatResponse, String> {
        let endpoint = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let sanitized_msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": SafetyGuard::redact_pii(&m.content)
                })
            })
            .collect();

        let payload = serde_json::json!({
            "model": self.model,
            "messages": sanitized_msgs,
            "temperature": 0.2,
            "max_tokens": 1200
        });

        let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

        let mut headers = vec![
            ("Content-Type", "application/json"),
            ("User-Agent", "Mithrandir/1.0.1"),
        ];

        let auth_hdr = if !self.api_key.is_empty() {
            format!("Bearer {}", self.api_key)
        } else {
            String::new()
        };

        if !auth_hdr.is_empty() {
            headers.push(("Authorization", &auth_hdr));
        }

        let mut cmd = HostBridge::wrap_command("curl", &[
            "-s",
            "-X",
            "POST",
            "--max-time",
            "45",
            &endpoint,
            "--data-binary",
            "@-",
        ]);

        for (k, v) in headers {
            cmd.args(["-H", &format!("{}: {}", k, v)]);
        }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn curl: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }

        let output = child.wait_with_output().map_err(|e| format!("HTTP request failed: {}", e))?;
        if !output.status.success() {
            return Err(format!("curl error status: {:?}", output.status.code()));
        }

        let resp_text = String::from_utf8_lossy(&output.stdout);
        let val: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|_| format!("Invalid JSON response: {}", resp_text.trim()))?;

        if let Some(err) = val.get("error") {
            let msg = err["message"].as_str().unwrap_or("API error returned");
            return Err(format!("API Error: {}", msg));
        }

        let content = val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let proposed_command = Self::extract_proposed_command(&content);

        Ok(ChatResponse {
            success: true,
            content,
            model: self.model.clone(),
            proposed_command,
        })
    }

    fn call_gemini(&self, messages: &[ChatMessage]) -> Result<ChatResponse, String> {
        let endpoint = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url.trim_end_matches('/'),
            self.model,
            self.api_key
        );

        let contents: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                let role = if m.role == "assistant" { "model" } else { "user" };
                serde_json::json!({
                    "role": role,
                    "parts": [{ "text": SafetyGuard::redact_pii(&m.content) }]
                })
            })
            .collect();

        let payload = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "temperature": 0.2,
                "maxOutputTokens": 1200
            }
        });

        let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

        let mut cmd = HostBridge::wrap_command("curl", &[
            "-s",
            "-X",
            "POST",
            "--max-time",
            "45",
            "-H",
            "Content-Type: application/json",
            &endpoint,
            "--data-binary",
            "@-",
        ]);

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to call curl: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }

        let output = child.wait_with_output().map_err(|e| format!("HTTP request failed: {}", e))?;
        let resp_text = String::from_utf8_lossy(&output.stdout);
        let val: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|_| format!("Invalid Gemini JSON: {}", resp_text.trim()))?;

        if let Some(err) = val.get("error") {
            let msg = err["message"].as_str().unwrap_or("Gemini error");
            return Err(format!("Gemini Error: {}", msg));
        }

        let content = val["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let proposed_command = Self::extract_proposed_command(&content);

        Ok(ChatResponse {
            success: true,
            content,
            model: self.model.clone(),
            proposed_command,
        })
    }

    fn call_claude(&self, messages: &[ChatMessage]) -> Result<ChatResponse, String> {
        let endpoint = format!("{}/messages", self.base_url.trim_end_matches('/'));

        let sanitized_msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": SafetyGuard::redact_pii(&m.content)
                })
            })
            .collect();

        let payload = serde_json::json!({
            "model": self.model,
            "messages": sanitized_msgs,
            "max_tokens": 1200
        });

        let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

        let mut cmd = HostBridge::wrap_command("curl", &[
            "-s",
            "-X",
            "POST",
            "--max-time",
            "45",
            "-H",
            "Content-Type: application/json",
            "-H",
            &format!("x-api-key: {}", self.api_key),
            "-H",
            "anthropic-version: 2023-06-01",
            &endpoint,
            "--data-binary",
            "@-",
        ]);

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to call curl: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }

        let output = child.wait_with_output().map_err(|e| format!("HTTP request failed: {}", e))?;
        let resp_text = String::from_utf8_lossy(&output.stdout);
        let val: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|_| format!("Invalid Claude response: {}", resp_text.trim()))?;

        if let Some(err) = val.get("error") {
            let msg = err["message"].as_str().unwrap_or("Claude API error");
            return Err(format!("Claude Error: {}", msg));
        }

        let content = val["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let proposed_command = Self::extract_proposed_command(&content);

        Ok(ChatResponse {
            success: true,
            content,
            model: self.model.clone(),
            proposed_command,
        })
    }

    /// Extracts and validates terminal commands from AI generated text.
    pub fn extract_proposed_command(text: &str) -> Option<String> {
        // Look for explicit prefix: PROPOSED_COMMAND: <cmd>
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(cmd) = trimmed.strip_prefix("PROPOSED_COMMAND:") {
                let clean = cmd.trim().trim_matches('`').trim();
                let (safe, _) = SafetyGuard::is_command_safe(clean);
                if safe && !clean.is_empty() {
                    return Some(clean.to_string());
                }
            }
        }

        // Look for markdown code fence blocks: ```bash or ```sh
        if let Some(start) = text.find("```") {
            let after_start = &text[start + 3..];
            let code_block = if let Some(first_newline) = after_start.find('\n') {
                let rest = &after_start[first_newline + 1..];
                if let Some(end) = rest.find("```") {
                    &rest[..end]
                } else {
                    rest
                }
            } else {
                ""
            };

            for line in code_block.lines() {
                let clean = line.trim().trim_start_matches('$').trim();
                if !clean.is_empty() && !clean.starts_with('#') {
                    let (safe, _) = SafetyGuard::is_command_safe(clean);
                    if safe {
                        return Some(clean.to_string());
                    }
                }
            }
        }

        None
    }
}
