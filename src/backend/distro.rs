use serde::{Deserialize, Serialize};
use crate::os_info::{detect_os, LinuxDistro};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistroFamily {
    Debian,
    Fedora,
    Arch,
    Suse,
    Alpine,
    Unknown,
}

impl From<LinuxDistro> for DistroFamily {
    fn from(distro: LinuxDistro) -> Self {
        match distro {
            LinuxDistro::Debian | LinuxDistro::Ubuntu => DistroFamily::Debian,
            LinuxDistro::Fedora => DistroFamily::Fedora,
            LinuxDistro::Arch | LinuxDistro::CachyOS => DistroFamily::Arch,
            LinuxDistro::OpenSuse => DistroFamily::Suse,
            LinuxDistro::Alpine => DistroFamily::Alpine,
            LinuxDistro::NixOS | LinuxDistro::Unknown => DistroFamily::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroInfo {
    pub id: String,
    pub pretty_name: String,
    pub family: DistroFamily,
}

pub struct DistroDetector;

impl DistroDetector {
    /// Detects current GNU/Linux distribution by querying centralized os_info.
    pub fn detect() -> DistroInfo {
        let details = detect_os();
        DistroInfo {
            id: details.id,
            pretty_name: details.pretty_name,
            family: DistroFamily::from(details.distro),
        }
    }
}
