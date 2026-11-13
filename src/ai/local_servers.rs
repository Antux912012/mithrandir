use std::net::TcpStream;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::backend::host_bridge::HostBridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalServerStatus {
    pub id: String,
    pub name: String,
    pub port: u16,
    pub is_online: bool,
    pub default_base_url: String,
    pub detected_models: Vec<String>,
}

pub struct LocalServerDetector;

impl LocalServerDetector {
    /// Detects if a TCP port on localhost is accepting connections.
    pub fn is_port_open(port: u16) -> bool {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        TcpStream::connect_timeout(&addr, Duration::from_millis(150)).is_ok()
    }

    /// Queries models from a local OpenAI-compatible endpoint.
    pub fn fetch_models(base_url: &str) -> Vec<String> {
        let endpoint = format!("{}/models", base_url.trim_end_matches('/'));
        let res = HostBridge::run_command("curl", &["-s", "--max-time", "2", &endpoint]);

        let mut models = Vec::new();
        if let Ok((0, stdout, _)) = res {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(data) = val.get("data").and_then(|d| d.as_array()) {
                    for item in data {
                        if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                            models.push(id.to_string());
                        }
                    }
                }
            }
        }
        models
    }

    /// Scans the 3 standard local runners (LM Studio, Ollama, llama.cpp).
    pub fn scan_local_servers() -> Vec<LocalServerStatus> {
        vec![
            Self::check_lm_studio(),
            Self::check_ollama(),
            Self::check_llama_cpp(),
        ]
    }

    pub fn check_lm_studio() -> LocalServerStatus {
        let port = 1234;
        let online = Self::is_port_open(port);
        let base_url = "http://localhost:1234/v1".to_string();
        let models = if online {
            Self::fetch_models(&base_url)
        } else {
            Vec::new()
        };

        LocalServerStatus {
            id: "lmstudio".to_string(),
            name: "LM Studio".to_string(),
            port,
            is_online: online,
            default_base_url: base_url,
            detected_models: models,
        }
    }

    pub fn check_ollama() -> LocalServerStatus {
        let port = 11434;
        let online = Self::is_port_open(port);
        let base_url = "http://localhost:11434/v1".to_string();
        let models = if online {
            Self::fetch_models(&base_url)
        } else {
            Vec::new()
        };

        LocalServerStatus {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            port,
            is_online: online,
            default_base_url: base_url,
            detected_models: models,
        }
    }

    pub fn check_llama_cpp() -> LocalServerStatus {
        let port = 8080;
        let online = Self::is_port_open(port);
        let base_url = "http://localhost:8080/v1".to_string();
        let models = if online {
            Self::fetch_models(&base_url)
        } else {
            Vec::new()
        };

        LocalServerStatus {
            id: "llamacpp".to_string(),
            name: "llama.cpp Server".to_string(),
            port,
            is_online: online,
            default_base_url: base_url,
            detected_models: models,
        }
    }
}
