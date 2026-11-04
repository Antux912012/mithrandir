use serde::{Deserialize, Serialize};
use crate::ai::client::{AIClient, ChatMessage};
use crate::ai::safety::SafetyGuard;
use crate::backend::cleaner::CleanerEngine;
use crate::backend::host_bridge::HostBridge;
use crate::backend::monitor::SystemMonitor;
use crate::kernel::detect_kernel;
use crate::power::{get_current_epp, get_current_governor, is_turbo_boost_enabled};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    pub category: String,
    pub title: String,
    pub description: String,
    pub severity: String, // "info", "recommendation", "warning"
    pub proposed_command: Option<String>,
    pub is_safe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationReport {
    pub timestamp: String,
    pub cpu_summary: String,
    pub memory_summary: String,
    pub kernel_summary: String,
    pub storage_summary: String,
    pub services_summary: String,
    pub ai_insights: String,
    pub recommendations: Vec<OptimizationRecommendation>,
}

pub struct AIOptimizerEngine;

impl AIOptimizerEngine {
    /// Gathers host diagnostic metrics into an anonymized optimization prompt.
    pub fn gather_optimization_context() -> String {
        let cpu = SystemMonitor::get_cpu_info();
        let kinfo = detect_kernel();
        let gov = get_current_governor().unwrap_or_else(|| "unknown".to_string());
        let epp = get_current_epp().unwrap_or_else(|| "unknown".to_string());
        let turbo = is_turbo_boost_enabled().map(|b| if b { "enabled" } else { "disabled" }).unwrap_or("n/a");

        let (tmp_bytes, tmp_count) = CleanerEngine::scan_user_tmp();
        let tmp_mb = (tmp_bytes as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0;

        let base_snapshot = crate::backend::diagnostics::DiagnosticsCollector::gather_system_snapshot();

        let context = format!(
            "{}\n\
            Hardware & Scheduler Telemetry:\n\
            - CPU: {} cores, {} (Usage: {:.1}%)\n\
            - Governor: {} | EPP: {} | Turbo: {}\n\
            - Scheduler: CachyOS={}, BORE={}\n\
            - User Temp Files: {:.1}MB ({} files in /tmp)\n",
            base_snapshot,
            cpu.core_count,
            cpu.brand,
            cpu.usage_percent,
            gov,
            epp,
            turbo,
            kinfo.is_cachyos,
            kinfo.bore_enabled,
            tmp_mb,
            tmp_count,
        );

        SafetyGuard::redact_pii(&context)
    }

    /// Instant heuristic system evaluation providing instant fallback and baseline recommendations.
    pub fn run_heuristic_optimization() -> OptimizationReport {
        let cpu = SystemMonitor::get_cpu_info();
        let mem = SystemMonitor::get_memory_info();
        let kinfo = detect_kernel();
        let gov = get_current_governor().unwrap_or_else(|| "unknown".to_string());
        let disks = SystemMonitor::get_disks_info();
        let root_disk = disks.iter().find(|d| d.mount_point == "/");

        let mut recommendations = Vec::new();

        // 1. Kernel & Scheduler Optimization
        if kinfo.bore_supported && !kinfo.bore_enabled {
            recommendations.push(OptimizationRecommendation {
                category: "Kernel Scheduler".to_string(),
                title: "Enable CachyOS BORE Scheduler".to_string(),
                description: "BORE dynamically prioritizes interactive GUI processes to reduce desktop latency and micro-stutter.".to_string(),
                severity: "recommendation".to_string(),
                proposed_command: Some("echo 1 | sudo tee /proc/sys/kernel/sched_bore".to_string()),
                is_safe: true,
            });
        }

        if gov == "powersave" && cpu.usage_percent > 60.0 {
            recommendations.push(OptimizationRecommendation {
                category: "CPU Performance".to_string(),
                title: "Elevate CPU Frequency Governor".to_string(),
                description: "CPU load exceeds 60% while governor is set to powersave. Switching to performance or schedutil improves throughput.".to_string(),
                severity: "info".to_string(),
                proposed_command: Some("echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor".to_string()),
                is_safe: true,
            });
        }

        // 2. Memory / Standby Cache Optimization
        if mem.percent >= 75.0 {
            recommendations.push(OptimizationRecommendation {
                category: "Memory Management".to_string(),
                title: "Trim Standby Memory Buffers".to_string(),
                description: format!("RAM utilization is at {:.1}%. Reclaiming pagecache will prevent background swap thrashing.", mem.percent),
                severity: "warning".to_string(),
                proposed_command: Some("sync && echo 3 | sudo tee /proc/sys/vm/drop_caches".to_string()),
                is_safe: true,
            });
        }

        // 3. Failed Services Inspection
        let failed_out = HostBridge::run_command("systemctl", &["--failed", "--no-legend"]);
        if let Ok((0, stdout, _)) = failed_out {
            let failed_count = stdout.lines().filter(|l| !l.trim().is_empty()).count();
            if failed_count > 0 {
                recommendations.push(OptimizationRecommendation {
                    category: "System Services".to_string(),
                    title: format!("Resolve {} Failed Systemd Service(s)", failed_count),
                    description: "Degraded services can consume unnecessary CPU cycles and log error buffers.".to_string(),
                    severity: "warning".to_string(),
                    proposed_command: Some("systemctl reset-failed".to_string()),
                    is_safe: true,
                });
            }
        }

        // 4. Storage & Temporary Files
        let (tmp_bytes, _) = CleanerEngine::scan_user_tmp();
        let tmp_mb = (tmp_bytes as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0;
        if tmp_mb > 500.0 {
            recommendations.push(OptimizationRecommendation {
                category: "Storage & Cache".to_string(),
                title: "Clean Volatile User Temp Files".to_string(),
                description: format!("Over {:.0}MB of temporary volatile files detected in /tmp.", tmp_mb),
                severity: "info".to_string(),
                proposed_command: Some("rm -rf /tmp/stremio-cache /tmp/tracker-* 2>/dev/null || true".to_string()),
                is_safe: true,
            });
        }

        let disk_summary = if let Some(d) = root_disk {
            format!("{:.1}GB free ({:.1}% used)", d.available_gb, d.percent)
        } else {
            "Disk available".to_string()
        };

        OptimizationReport {
            timestamp: chrono_lite_timestamp(),
            cpu_summary: format!("{} cores ({:.1}% load)", cpu.core_count, cpu.usage_percent),
            memory_summary: format!("{:.0}MB used / {:.0}MB ({:.1}%)", mem.used_mb, mem.total_mb, mem.percent),
            kernel_summary: format!("{} (BORE: {})", kinfo.kernel_release, if kinfo.bore_enabled { "Active" } else { "Off" }),
            storage_summary: disk_summary,
            services_summary: "System health check completed".to_string(),
            ai_insights: "Heuristic baseline analysis completed. Connect NVIDIA Nemotron 3.5 Bolt E4B for deep LLM reasoning.".to_string(),
            recommendations,
        }
    }

    /// Queries the configured Nemotron / LLM client with system diagnostics to produce advanced optimization insights.
    pub fn run_ai_optimization(client: &AIClient) -> Result<OptimizationReport, String> {
        let mut report = Self::run_heuristic_optimization();
        let context = Self::gather_optimization_context();

        let prompt = format!(
            "You are Mithrandir's NVIDIA Nemotron 3.5 Bolt E4B AI Linux Optimizer.\n\
            Analyze the following GNU/Linux diagnostic state and provide actionable system tuning insights:\n\n\
            {}\n\n\
            Provide a concise diagnosis summary and, if applicable, specify a safe terminal command fix formatted as:\n\
            PROPOSED_COMMAND: <single line shell command>",
            context
        );

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are an expert GNU/Linux performance tuning engineer and kernel specialist. Always prioritize system stability and safety. Keep answers concise.".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: prompt,
            },
        ];

        match client.chat(&messages) {
            Ok(resp) => {
                report.ai_insights = resp.content;
                if let Some(cmd) = resp.proposed_command {
                    let (safe, _) = SafetyGuard::is_command_safe(&cmd);
                    if safe {
                        report.recommendations.insert(
                            0,
                            OptimizationRecommendation {
                                category: "Nemotron AI Recommendation".to_string(),
                                title: "AI Proposed Performance Fix".to_string(),
                                description: "Derived from real-time system diagnostics and telemetry analysis.".to_string(),
                                severity: "recommendation".to_string(),
                                proposed_command: Some(cmd),
                                is_safe: true,
                            },
                        );
                    }
                }
                Ok(report)
            }
            Err(e) => {
                // If AI call failed, return the heuristic report with note of the error
                report.ai_insights = format!("Could not connect to AI provider ({}); displaying heuristic findings.", e);
                Ok(report)
            }
        }
    }
}

fn chrono_lite_timestamp() -> String {
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02} UTC", hours, mins, s)
}
