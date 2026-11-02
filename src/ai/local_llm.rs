use std::fs;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::backend::host_bridge::HostBridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufModelInfo {
    pub id: String,
    pub name: String,
    pub size: String,
    pub ram_required: String,
    pub url: String,
    pub description: String,
    pub is_draft_model: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaCppStatus {
    pub engine_installed: bool,
    pub engine_path: String,
    pub running: bool,
    pub models: Vec<String>,
    pub has_nemotron_dflash: bool,
    pub has_nemotron_base: bool,
    pub disk_usage_mb: f64,
    pub port: u16,
    pub base_url: String,
    pub active_model: String,
}

pub fn get_recommended_nemotron_models() -> Vec<GgufModelInfo> {
    vec![
        GgufModelInfo {
            id: "Nemotron-3.5-Bolt-E4B-DFlash-BF16.gguf".to_string(),
            name: "NVIDIA Nemotron 3.5 Bolt E4B (DFlash Draft Model)".to_string(),
            size: "~737 MB".to_string(),
            ram_required: "2 GB RAM / VRAM".to_string(),
            url: "https://huggingface.co/lmstudio-community/NVIDIA-Nemotron-3.5-Bolt-E4B-DFlash-GGUF/resolve/main/Nemotron-3.5-Bolt-E4B-DFlash-BF16.gguf".to_string(),
            description: "Speculative decoding draft model (~2x inference acceleration) for NVIDIA Nemotron 3.5 Bolt E4B.".to_string(),
            is_draft_model: true,
        },
        GgufModelInfo {
            id: "NVIDIA-Nemotron-3.5-Bolt-E4B-Q4_K_M.gguf".to_string(),
            name: "NVIDIA Nemotron 3.5 Bolt E4B (Q4_K_M Target Base Model)".to_string(),
            size: "~4.5 GB".to_string(),
            ram_required: "6 GB RAM / VRAM".to_string(),
            url: "https://huggingface.co/lmstudio-community/NVIDIA-Nemotron-3.5-Bolt-E4B-GGUF/resolve/main/NVIDIA-Nemotron-3.5-Bolt-E4B-Q4_K_M.gguf".to_string(),
            description: "High-performance small language model optimized for Linux administration, shell execution, and hardware triage.".to_string(),
            is_draft_model: false,
        },
    ]
}

static MANAGED_CHILD: Mutex<Option<Child>> = Mutex::new(None);
static ACTIVE_MODEL: Mutex<Option<String>> = Mutex::new(None);

pub struct LocalLLMManager;

impl LocalLLMManager {
    pub const PORT: u16 = 18080;
    pub const BASE_URL: &'static str = "http://127.0.0.1:18080/v1";

    pub fn app_data_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let modern = PathBuf::from(&home).join(".local/share/io.github.antux912012.Mithrandir/llm");
        if modern.exists() {
            return modern;
        }
        let legacy = PathBuf::from(&home).join(".local/share/io.github.antonio.Mithrandir/llm");
        if legacy.exists() {
            return legacy;
        }
        modern
    }

    pub fn bin_dir() -> PathBuf {
        Self::app_data_dir().join("bin")
    }

    pub fn models_dir() -> PathBuf {
        Self::app_data_dir().join("models")
    }

    pub fn find_llama_server_binary() -> Option<PathBuf> {
        let internal = Self::bin_dir().join("llama-server");
        if internal.is_file() {
            return Some(internal);
        }
        if let Some(host_path) = HostBridge::which("llama-server") {
            return Some(PathBuf::from(host_path));
        }
        None
    }

    pub fn get_installed_gguf_models() -> Vec<String> {
        let mdir = Self::models_dir();
        if !mdir.is_dir() {
            return Vec::new();
        }

        let mut models = Vec::new();
        if let Ok(entries) = fs::read_dir(mdir) {
            for entry in entries.filter_map(Result::ok) {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension() {
                        if ext == "gguf" {
                            if let Some(fname) = p.file_name() {
                                models.push(fname.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
        models.sort();
        models
    }

    pub fn calculate_disk_usage_mb() -> f64 {
        let base = Self::app_data_dir();
        if !base.exists() {
            return 0.0;
        }
        let mut total_bytes = 0u64;
        for entry in walkdir::WalkDir::new(&base).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                total_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        (total_bytes as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0
    }

    pub fn is_port_open(port: u16) -> bool {
        let addr = format!("127.0.0.1:{}", port);
        TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_millis(250),
        )
        .is_ok()
    }

    pub fn check_llamacpp_status() -> LlamaCppStatus {
        let bin = Self::find_llama_server_binary();
        let engine_installed = bin.is_some();
        let engine_path = bin
            .map(|b| b.to_string_lossy().to_string())
            .unwrap_or_else(|| "Not Found".to_string());

        let running = Self::is_port_open(Self::PORT);
        let models = Self::get_installed_gguf_models();
        let has_nemotron_dflash = models.iter().any(|m| m.contains("Nemotron") && m.contains("DFlash"));
        let has_nemotron_base = models.iter().any(|m| m.contains("Nemotron") && !m.contains("DFlash"));
        let disk_usage_mb = Self::calculate_disk_usage_mb();
        let active_model = ACTIVE_MODEL
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "None".to_string());

        LlamaCppStatus {
            engine_installed,
            engine_path,
            running,
            models,
            has_nemotron_dflash,
            has_nemotron_base,
            disk_usage_mb,
            port: Self::PORT,
            base_url: Self::BASE_URL.to_string(),
            active_model,
        }
    }

    pub fn download_gguf_model<F>(
        model_id: &str,
        mut on_progress: F,
        custom_url: Option<&str>,
    ) -> Result<String, String>
    where
        F: FnMut(String) + Send + 'static,
    {
        let models = get_recommended_nemotron_models();
        let target_url = if let Some(u) = custom_url {
            u.to_string()
        } else if let Some(m) = models.iter().find(|m| m.id == model_id) {
            m.url.clone()
        } else {
            return Err(format!("Unknown GGUF model: {}", model_id));
        };

        let models_dir = Self::models_dir();
        fs::create_dir_all(&models_dir).map_err(|e| format!("Failed to create models dir: {}", e))?;

        let dest_file = models_dir.join(model_id);
        let part_file = models_dir.join(format!("{}.part", model_id));

        on_progress(format!("Starting download: {} -> {}", target_url, dest_file.display()));

        let status = HostBridge::spawn_streaming(
            "curl",
            &[
                "-L",
                "-C",
                "-",
                "-o",
                part_file.to_str().unwrap(),
                "--progress-bar",
                &target_url,
            ],
            false,
            move |line| {
                on_progress(line);
            },
        )?;

        if status != 0 {
            return Err(format!("Download failed with exit code: {}", status));
        }

        if part_file.exists() {
            fs::rename(&part_file, &dest_file)
                .map_err(|e| format!("Failed to finalize model file: {}", e))?;
        }

        Ok(format!("Successfully downloaded and ready: {}", model_id))
    }

    pub fn start_llama_server(preferred_model: Option<&str>) -> Result<(), String> {
        if Self::is_port_open(Self::PORT) {
            return Ok(());
        }

        let bin_path = Self::find_llama_server_binary()
            .ok_or_else(|| "llama-server binary not found. Please install llama.cpp.".to_string())?;

        let models = Self::get_installed_gguf_models();
        if models.is_empty() {
            return Err("No GGUF models installed in models folder.".to_string());
        }

        let dflash_model = models.iter().find(|m| m.contains("Nemotron") && m.contains("DFlash")).cloned();
        let base_model = models.iter().find(|m| m.contains("Nemotron") && !m.contains("DFlash")).cloned();

        let chosen_base = preferred_model
            .map(|s| s.to_string())
            .or(base_model)
            .or_else(|| models.first().cloned())
            .ok_or_else(|| "No base model found".to_string())?;

        let base_path = Self::models_dir().join(&chosen_base);

        let mut cmd = Command::new(&bin_path);
        cmd.arg("-m");
        cmd.arg(&base_path);

        // Enable DFlash speculative decoding if companion draft model exists
        if let Some(ref dflash) = dflash_model {
            if chosen_base != *dflash {
                let dflash_path = Self::models_dir().join(dflash);
                cmd.args(["-md", dflash_path.to_str().unwrap(), "--spec-type", "draft-dflash", "--spec-draft-n-max", "7"]);
            }
        }

        cmd.args([
            "--port",
            &Self::PORT.to_string(),
            "--host",
            "127.0.0.1",
            "-c",
            "4096",
            "-ngl",
            "99",
        ]);

        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to launch llama-server: {}", e))?;

        *MANAGED_CHILD.lock().unwrap() = Some(child);
        *ACTIVE_MODEL.lock().unwrap() = Some(chosen_base);

        // Wait up to 10 seconds for port to open
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(250));
            if Self::is_port_open(Self::PORT) {
                return Ok(());
            }
        }

        Err("llama-server was started but failed to bind port within timeout.".to_string())
    }

    pub fn stop_llama_server() -> Result<(), String> {
        let mut lock = MANAGED_CHILD.lock().unwrap();
        if let Some(mut child) = lock.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        if let Ok((code, _, _)) = HostBridge::run_command("pkill", &["-f", "llama-server.*18080"]) {
            if code == 0 {
                return Ok(());
            }
        }

        *ACTIVE_MODEL.lock().unwrap() = None;
        Ok(())
    }
}

/// Instant deterministic offline Linux assistant heuristics (zero-download fallback).
pub struct BuiltinLinuxAssistant;

impl BuiltinLinuxAssistant {
    pub fn respond(messages: &[ChatMessage]) -> String {
        let query = messages.last().map(|m| m.content.as_str()).unwrap_or("").to_lowercase();

        if query.contains("diagnose") || query.contains("anomal") || query.contains("triage") || query.contains("system state") {
            let mut findings = Vec::new();
            let mut proposed = "systemctl --failed --no-pager";

            if query.contains("memory:") {
                findings.push("• Memory telemetry scanned: Standby pagecache can be safely reclaimed via Smart Boost.");
            }
            if query.contains("failed systemd units:\nnone") {
                findings.push("• System Services: Healthy (0 failed systemd units detected).");
            } else if query.contains("failed systemd units:") {
                findings.push("• Alert: One or more failed systemd services detected on the host.");
                proposed = "systemctl reset-failed";
            }
            if query.contains("journal:") || query.contains("error") {
                findings.push("• Journal Log Triage: Prioritized system alerts recorded in current boot cycle.");
                proposed = "journalctl -p 3 -xb -n 30 --no-pager";
            }
            if query.contains("cachyos") || query.contains("bore") {
                findings.push("• Kernel & Scheduler: CachyOS BORE active for dynamic interactive response.");
            }

            return format!(
                "**MITHRANDIR NEMOTRON SYSTEM OPTIMIZATION TRIAGE**\n\n\
                **Status Summary:**\n{}\n\n\
                **Recommended Action:**\nExecute service status check and vacuum standby caches.\n\n\
                PROPOSED_COMMAND: {}",
                findings.join("\n"),
                proposed
            );
        }

        if query.contains("file") && (query.contains("large") || query.contains("100m")) {
            return "Scan home directory for large files exceeding 100MB:\n\nPROPOSED_COMMAND: find ~ -type f -size +100M -exec ls -lh {} + 2>/dev/null | sort -k5 -rh | head -n 20".to_string();
        }

        if query.contains("port") || query.contains("listen") || query.contains("network") {
            return "Display listening network ports and associated process sockets:\n\nPROPOSED_COMMAND: ss -tulnp".to_string();
        }

        if query.contains("scheduler") || query.contains("bore") || query.contains("scx") {
            return "Check active CPU scheduler and BORE status:\n\nPROPOSED_COMMAND: cat /proc/sys/kernel/sched_bore 2>/dev/null || uname -r".to_string();
        }

        if query.contains("memory") || query.contains("ram") || query.contains("cache") {
            return "Inspect detailed memory allocation and standby buffer status:\n\nPROPOSED_COMMAND: free -h && vmstat -s".to_string();
        }

        "I am Mithrandir's Nemotron AI Optimizer. You can ask system administration questions, request performance tuning, or click 'Run AI System Optimization Triage' to analyze host diagnostics.\n\nPROPOSED_COMMAND: systemctl --failed".to_string()
    }
}
