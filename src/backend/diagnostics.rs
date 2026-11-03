use crate::ai::safety::SafetyGuard;
use crate::backend::host_bridge::HostBridge;
use crate::backend::monitor::SystemMonitor;

pub struct DiagnosticsCollector;

impl DiagnosticsCollector {
    /// Gathers a comprehensive, PII-redacted system diagnostics snapshot.
    pub fn gather_system_snapshot() -> String {
        let summary = SystemMonitor::get_system_summary();
        let mem = SystemMonitor::get_memory_info();
        let disks = SystemMonitor::get_disks_info();
        let root_disk = disks.iter().find(|d| d.mount_point == "/");

        let failed_units = HostBridge::run_command("systemctl", &["--failed", "--plain", "--no-legend", "--no-pager"])
            .map(|(_, out, _)| {
                let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).take(15).collect();
                if lines.is_empty() {
                    "None (all systemd services running healthy)".to_string()
                } else {
                    lines.join("\n")
                }
            })
            .unwrap_or_else(|_| "None".to_string());

        let journal_errors = HostBridge::run_command("journalctl", &["-p", "3", "-xb", "-n", "15", "--no-pager"])
            .map(|(_, out, _)| {
                let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).take(15).collect();
                if lines.is_empty() {
                    "No critical errors found in current boot journal.".to_string()
                } else {
                    lines.join("\n")
                }
            })
            .unwrap_or_else(|_| "Could not access system journal.".to_string());

        let root_summary = if let Some(d) = root_disk {
            format!("{:.1}GB free on root ({:.1}% used of {:.1}GB)", d.available_gb, d.percent, d.total_gb)
        } else {
            "Disk details unavailable".to_string()
        };

        let report = format!(
            "Mithrandir System Diagnostics Snapshot:\n\
            - OS: {}\n\
            - Kernel: {}\n\
            - Desktop: {}\n\
            - Memory: {:.0}MB used of {:.0}MB ({:.1}%, {:.0}MB available)\n\
            - Disk: {}\n\
            - Failed systemd units:\n{}\n\n\
            - Recent system errors:\n{}\n",
            summary.distro,
            summary.kernel,
            summary.desktop,
            mem.used_mb,
            mem.total_mb,
            mem.percent,
            mem.available_mb,
            root_summary,
            failed_units,
            journal_errors
        );

        SafetyGuard::redact_pii(&report)
    }
}
