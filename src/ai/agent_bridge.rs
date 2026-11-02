use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::backend::host_bridge::HostBridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAgentStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub binary_path: Option<String>,
    pub session_detected: bool,
    pub session_info: String,
    pub description: String,
}

pub struct LocalAgentBridge;

impl LocalAgentBridge {
    /// Detects all supported local coding agents and their active sessions.
    pub fn detect_agents() -> Vec<LocalAgentStatus> {
        vec![
            Self::detect_antigravity(),
            Self::detect_openclaude(),
            Self::detect_pi_agent(),
            Self::detect_codex(),
            Self::detect_cursor(),
        ]
    }

    /// Detects Google DeepMind Antigravity CLI and active brain sessions.
    pub fn detect_antigravity() -> LocalAgentStatus {
        let home = std::env::var("HOME").unwrap_or_default();
        let home_p = PathBuf::from(&home);

        let bin_candidate = home_p.join(".local/bin/agy");
        let bin_path = if bin_candidate.exists() {
            Some(bin_candidate.to_string_lossy().to_string())
        } else {
            HostBridge::which("agy").or_else(|| HostBridge::which("antigravity"))
        };

        let installed = bin_path.is_some() || home_p.join(".gemini/antigravity").exists();

        // Check for active conversation directories in .gemini/antigravity-cli/brain
        let brain_dir = home_p.join(".gemini/antigravity-cli/brain");
        let mut session_detected = false;
        let mut session_info = "No active session".to_string();

        if brain_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&brain_dir) {
                let count = entries.filter_map(Result::ok).filter(|e| e.path().is_dir()).count();
                if count > 0 {
                    session_detected = true;
                    session_info = format!("{} session(s) in ~/.gemini/antigravity-cli", count);
                }
            }
        }

        LocalAgentStatus {
            id: "antigravity".to_string(),
            name: "Antigravity (Google DeepMind)".to_string(),
            installed,
            binary_path: bin_path,
            session_detected,
            session_info,
            description: "Agentic AI coding assistant with local workspace inspection and execution tools.".to_string(),
        }
    }

    /// Detects OpenClaude session / CLI.
    pub fn detect_openclaude() -> LocalAgentStatus {
        let home = std::env::var("HOME").unwrap_or_default();
        let config_dir = Path::new(&home).join(".config/openclaude");
        let bin = HostBridge::which("openclaude");

        let installed = bin.is_some() || config_dir.exists();
        let session_detected = config_dir.join("session.json").exists() || config_dir.join("history").exists();
        let session_info = if session_detected {
            "Active session detected in ~/.config/openclaude".to_string()
        } else {
            "No active session".to_string()
        };

        LocalAgentStatus {
            id: "openclaude".to_string(),
            name: "OpenClaude".to_string(),
            installed,
            binary_path: bin,
            session_detected,
            session_info,
            description: "Open-source Claude command-line agent and terminal session controller.".to_string(),
        }
    }

    /// Detects Pi-Agent local session / daemon.
    pub fn detect_pi_agent() -> LocalAgentStatus {
        let home = std::env::var("HOME").unwrap_or_default();
        let config_dir = Path::new(&home).join(".config/pi-agent");
        let bin = HostBridge::which("pi-agent").or_else(|| HostBridge::which("pi"));

        let installed = bin.is_some() || config_dir.exists();
        let session_detected = config_dir.exists();
        let session_info = if session_detected {
            "Config & session workspace present in ~/.config/pi-agent".to_string()
        } else {
            "No active session".to_string()
        };

        LocalAgentStatus {
            id: "pi-agent".to_string(),
            name: "Pi-Agent".to_string(),
            installed,
            binary_path: bin,
            session_detected,
            session_info,
            description: "Autonomous reasoning and personal terminal agent.".to_string(),
        }
    }

    /// Detects OpenAI Codex CLI / daemon.
    pub fn detect_codex() -> LocalAgentStatus {
        let home = std::env::var("HOME").unwrap_or_default();
        let config_dir = Path::new(&home).join(".config/codex");
        let bin = HostBridge::which("codex");

        let installed = bin.is_some() || config_dir.exists();
        let session_detected = config_dir.join("session").exists() || config_dir.join("auth.json").exists();
        let session_info = if session_detected {
            "Active Codex credentials / session detected".to_string()
        } else {
            "No active session".to_string()
        };

        LocalAgentStatus {
            id: "codex".to_string(),
            name: "Codex CLI".to_string(),
            installed,
            binary_path: bin,
            session_detected,
            session_info,
            description: "Command-line shell code generation and task executor.".to_string(),
        }
    }

    /// Detects Cursor editor / agent session.
    pub fn detect_cursor() -> LocalAgentStatus {
        let home = std::env::var("HOME").unwrap_or_default();
        let cursor_dir = Path::new(&home).join(".cursor");
        let config_dir = Path::new(&home).join(".config/Cursor");
        let bin = HostBridge::which("cursor");

        let installed = bin.is_some() || cursor_dir.exists() || config_dir.exists();
        let session_detected = cursor_dir.exists() || config_dir.exists();
        let session_info = if session_detected {
            "Cursor workspace & agent state present".to_string()
        } else {
            "No active workspace".to_string()
        };

        LocalAgentStatus {
            id: "cursor".to_string(),
            name: "Cursor IDE & Agent".to_string(),
            installed,
            binary_path: bin,
            session_detected,
            session_info,
            description: "AI-first code editor and background agent service.".to_string(),
        }
    }

    /// Dispatches prompt to an active local agent CLI.
    pub fn query_agent(agent_id: &str, prompt: &str) -> Result<String, String> {
        let sanitized = crate::ai::SafetyGuard::redact_pii(prompt);

        match agent_id {
            "antigravity" => {
                let home = std::env::var("HOME").unwrap_or_default();
                let bin = Path::new(&home).join(".local/bin/agy");
                let cmd_str = if bin.exists() {
                    bin.to_string_lossy().to_string()
                } else {
                    "agy".to_string()
                };

                let res = HostBridge::run_command(&cmd_str, &["prompt", &sanitized]);
                match res {
                    Ok((0, stdout, _)) if !stdout.trim().is_empty() => Ok(stdout),
                    Ok((code, stdout, stderr)) => {
                        if !stdout.trim().is_empty() {
                            Ok(stdout)
                        } else {
                            Err(format!("Antigravity exited with code {}: {}", code, stderr.trim()))
                        }
                    }
                    Err(e) => Err(format!("Failed to invoke Antigravity CLI: {}", e)),
                }
            }
            "openclaude" => {
                let res = HostBridge::run_command("openclaude", &["--prompt", &sanitized]);
                match res {
                    Ok((0, stdout, _)) => Ok(stdout),
                    Ok((code, stdout, stderr)) => Err(format!("OpenClaude returned {}: {} {}", code, stdout, stderr)),
                    Err(e) => Err(format!("Failed to call OpenClaude: {}", e)),
                }
            }
            "pi-agent" => {
                let res = HostBridge::run_command("pi", &["--prompt", &sanitized]);
                match res {
                    Ok((0, stdout, _)) => Ok(stdout),
                    Ok((code, stdout, stderr)) => Err(format!("Pi-Agent returned {}: {} {}", code, stdout, stderr)),
                    Err(e) => Err(format!("Failed to call Pi-Agent: {}", e)),
                }
            }
            "codex" => {
                let res = HostBridge::run_command("codex", &["exec", &sanitized]);
                match res {
                    Ok((0, stdout, _)) => Ok(stdout),
                    Ok((code, stdout, stderr)) => Err(format!("Codex returned {}: {} {}", code, stdout, stderr)),
                    Err(e) => Err(format!("Failed to call Codex: {}", e)),
                }
            }
            "cursor" => {
                let res = HostBridge::run_command("cursor", &["--ai", &sanitized]);
                match res {
                    Ok((0, stdout, _)) => Ok(stdout),
                    Ok((code, stdout, stderr)) => Err(format!("Cursor returned {}: {} {}", code, stdout, stderr)),
                    Err(e) => Err(format!("Failed to call Cursor: {}", e)),
                }
            }
            _ => Err(format!("Unknown local agent: {}", agent_id)),
        }
    }
}
