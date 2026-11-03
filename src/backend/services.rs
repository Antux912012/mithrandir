use serde::{Deserialize, Serialize};
use crate::backend::host_bridge::HostBridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub unit: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
    pub is_running: bool,
}

pub struct ServiceManager;

impl ServiceManager {
    pub fn list_services() -> Vec<ServiceInfo> {
        let output = HostBridge::run_command(
            "systemctl",
            &["list-units", "--type=service", "--all", "--plain", "--no-legend", "--no-pager"],
        );

        let mut services = Vec::new();
        if let Ok((code, stdout, _)) = output {
            if code == 0 {
                for line in stdout.lines() {
                    let cleaned = line.trim_start().trim_start_matches('●').trim_start();
                    let parts: Vec<&str> = cleaned.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let unit = parts[0].to_string();
                        if unit.is_empty() {
                            continue;
                        }
                        let load = parts[1].to_string();
                        let active = parts[2].to_string();
                        let sub = parts[3].to_string();
                        let desc = parts[4..].join(" ");
                        let is_running = active == "active" || sub == "running";

                        services.push(ServiceInfo {
                            unit,
                            load,
                            active,
                            sub,
                            description: desc,
                            is_running,
                        });
                    }
                }
            }
        }

        services.sort_by(|a, b| a.unit.cmp(&b.unit));
        services
    }

    pub fn manage_service(unit: &str, action: &str) -> Result<(), String> {
        // Try unprivileged host systemctl first, fall back to pkexec
        if let Ok((0, _, _)) = HostBridge::run_command("systemctl", &[action, unit]) {
            return Ok(());
        }

        let (code, _, stderr) = HostBridge::run_privileged(&["systemctl", action, unit])?;
        if code == 0 {
            Ok(())
        } else {
            Err(format!("Failed to {} {}: {}", action, unit, stderr.trim()))
        }
    }
}
