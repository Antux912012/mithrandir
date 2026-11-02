use serde::{Deserialize, Serialize};
use crate::backend::host_bridge::HostBridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtrfsSnapshot {
    pub id: String,
    pub path: String,
    pub timestamp: String,
}

pub struct BtrfsManager;

impl BtrfsManager {
    pub fn is_btrfs_available() -> bool {
        if let Ok((code, stdout, _)) = HostBridge::run_command("findmnt", &["-n", "-o", "FSTYPE", "/"]) {
            if code == 0 {
                return stdout.trim().to_lowercase() == "btrfs";
            }
        }
        false
    }

    pub fn list_snapshots() -> Vec<BtrfsSnapshot> {
        let mut list = Vec::new();
        if let Ok((code, stdout, _)) = HostBridge::run_command("btrfs", &["subvolume", "list", "/"]) {
            if code == 0 {
                for line in stdout.lines() {
                    // e.g. ID 256 gen 1234 top level 5 path @snapshots/2026-10-13
                    if line.contains("path") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        let id = parts.get(1).unwrap_or(&"?").to_string();
                        let path = parts.last().unwrap_or(&"").to_string();
                        list.push(BtrfsSnapshot {
                            id,
                            path: path.clone(),
                            timestamp: "Local Subvolume".to_string(),
                        });
                    }
                }
            }
        }
        list
    }

    pub fn create_snapshot(name: &str) -> Result<(), String> {
        let snap_path = format!("/.snapshots/{}", name);
        let (code, _, stderr) = HostBridge::run_privileged(&["btrfs", "subvolume", "snapshot", "/", &snap_path])?;
        if code == 0 {
            Ok(())
        } else {
            Err(format!("Snapshot creation failed with exit code {}: {}", code, stderr.trim()))
        }
    }
}
