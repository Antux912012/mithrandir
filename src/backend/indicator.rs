use crate::backend::host_bridge::HostBridge;
use crate::backend::settings::AppSettings;
use crate::backend::smart_boost::SmartBoostEngine;

pub struct NotificationManager;

impl NotificationManager {
    /// Dispatches a desktop notification with application icon if desktop notifications are enabled.
    pub fn send_notification(summary: &str, body: &str) {
        if !AppSettings::get().desktop_notifications {
            return;
        }
        let _ = HostBridge::run_command(
            "notify-send",
            &[
                "--app-name=Mithrandir",
                "--icon=io.github.antux912012.Mithrandir",
                summary,
                body,
            ],
        );
    }
}

pub struct BackgroundDaemon;

impl BackgroundDaemon {
    /// Checks system thresholds and executes automatic background boost if thresholds are crossed and cooldown expired.
    pub fn run_auto_boost_check(
        ram_threshold: f64,
        temp_threshold_mb: f64,
        cooldown_minutes: u64,
        notify: bool,
    ) -> bool {
        if let Some(res) = SmartBoostEngine::evaluate_auto_boost(ram_threshold, temp_threshold_mb, cooldown_minutes) {
            if notify && (res.ram_reclaimed_mb > 0.0 || res.disk_freed_mb > 0.0) {
                let msg = format!(
                    "Reclaimed {:.1} MB RAM and freed {:.1} MB disk cache.",
                    res.ram_reclaimed_mb, res.disk_freed_mb
                );
                NotificationManager::send_notification("⚡ Smart Boost Automated", &msg);
            }
            true
        } else {
            false
        }
    }
}
