use std::io::Write;
use std::process::Stdio;
use serde::{Deserialize, Serialize};
use crate::ai::local_llm::{BuiltinLinuxAssistant, ChatMessage};
use crate::ai::safety::SafetyGuard;
use crate::backend::host_bridge::HostBridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub success: bool,
    pub content: String,
    pub model: String,
    pub proposed_command: Option<String>,
}

pub struct AIClient {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl AIClient {
    pub fn new(base_url: &str, api_key: &str, model: &str) -> Self {
        Self {
            base_url: Self::normalize_base_url(base_url),
            api_key: api_key.trim().to_string(),
            model: model.trim().to_string(),
        }
    }

    pub fn normalize_base_url(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with("builtin://") {
            return "builtin://linux-mini".to_string();
        }

        let mut url = trimmed.trim_end_matches('/').to_string();

        if url.contains(":generateContent") {
            if let Some(idx) = url.find("/models/") {
                url = url[..idx].trim_end_matches('/').to_string();
            }
        }

        for suffix in ["/chat/completions", "/completions", "/messages", "/models"] {
            if url.ends_with(suffix) {
                url = url.trim_end_matches(suffix).trim_end_matches('/').to_string();
            }
        }

        if url == "https://integrate.api.nvidia.com" {
            return "https://integrate.api.nvidia.com/v1".to_string();
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
        if url == "http://localhost:18080" || url == "http://127.0.0.1:18080" {
            return "http://127.0.0.1:18080/v1".to_string();
        }

        url
    }

    pub fn is_builtin(&self) -> bool {
        self.base_url.starts_with("builtin://")
    }

    pub fn is_gemini(&self) -> bool {
        self.base_url.contains("generativelanguage.googleapis.com")
    }

    pub fn is_claude(&self) -> bool {
        self.base_url.contains("api.anthropic.com")
    }

    pub fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse, String> {
        if self.is_builtin() {
            let reply = BuiltinLinuxAssistant::respond(messages);
            let cmd = Self::extract_proposed_command(&reply);
            return Ok(ChatResponse {
                success: true,
                content: reply,
                model: "builtin-linux-mini".to_string(),
                proposed_command: cmd,
            });
        }

        if self.is_gemini() {
            return self.call_gemini(messages);
        }

        if self.is_claude() {
            return self.call_claude(messages);
        }

        self.call_openai_compatible(messages)
    }

    fn call_openai_compatible(&self, messages: &[ChatMessage]) -> Result<ChatResponse, String> {
        let endpoint = format!("{}/chat/completions", self.base_url);

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
            "max_tokens": 1024
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

        let mut child = cmd.spawn().map_err(|e| format!("Failed to invoke curl: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }

        let output = child.wait_with_output().map_err(|e| format!("HTTP request failed: {}", e))?;
        if !output.status.success() {
            return Err(format!("curl failed with status: {:?}", output.status.code()));
        }

        let resp_text = String::from_utf8_lossy(&output.stdout);
        let val: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|_| format!("Invalid JSON response: {}", resp_text.trim()))?;

        if let Some(err) = val.get("error") {
            let msg = err["message"].as_str().unwrap_or("Unknown provider error");
            return Err(format!("AI Provider Error: {}", msg));
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
            self.base_url, self.model, self.api_key
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
                "maxOutputTokens": 1024
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

        let mut child = cmd.spawn().map_err(|e| format!("Failed to invoke curl: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }

        let output = child.wait_with_output().map_err(|e| format!("HTTP request failed: {}", e))?;
        let resp_text = String::from_utf8_lossy(&output.stdout);
        let val: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|_| format!("Invalid JSON response from Gemini: {}", resp_text.trim()))?;

        if let Some(err) = val.get("error") {
            let msg = err["message"].as_str().unwrap_or("Unknown Gemini error");
            return Err(format!("Gemini API Error: {}", msg));
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
        let endpoint = format!("{}/messages", self.base_url);

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
            "max_tokens": 1024
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

        let mut child = cmd.spawn().map_err(|e| format!("Failed to invoke curl: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }

        let output = child.wait_with_output().map_err(|e| format!("HTTP request failed: {}", e))?;
        let resp_text = String::from_utf8_lossy(&output.stdout);
        let val: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|_| format!("Invalid JSON response from Claude: {}", resp_text.trim()))?;

        if let Some(err) = val.get("error") {
            let msg = err["message"].as_str().unwrap_or("Unknown Claude error");
            return Err(format!("Claude API Error: {}", msg));
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
        // 1. Look for explicit prefix: PROPOSED_COMMAND: <cmd>
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

        // 2. Look for code fences: ```bash ... ``` or ```sh ... ```
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
