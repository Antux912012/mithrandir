# Architectural Proposal: CVE Vulnerability Scanner & ClamAV Anti-Malware Integration

**Target Version**: Mithrandir v0.2.0  
**Status**: Draft / RFC  
**Author**: Mithrandir Engineering  
**Components**: `src/mithrandir/backend/security_cve.py`, `src/mithrandir/backend/antivirus_clamav.py`, `src/mithrandir/ui/security_page.py`

---

## 1. Executive Summary

This proposal outlines the architecture, data flow, safety models, and implementation roadmap for introducing a unified **Security Subsystem** into Mithrandir. The subsystem encompasses two complementary modules:

1. **System Vulnerability & CVE Audit Engine**: Inspects installed system packages, running kernel versions, and shared libraries against public vulnerability databases (OSV, Debian Security Tracker, Red Hat OVAL) to detect known CVEs (Common Vulnerabilities and Exposures) and recommend immediate patch updates.
2. **ClamAV Anti-Malware & Antivirus Suite**: Integrates the open-source ClamAV scanning engine, supporting signature database synchronization (`freshclam`), local daemon interaction (`clamd`), quick/full/custom directory scans, real-time progress parsing, and a secure quarantine vault.

Both features will adhere to Mithrandir's core design values: **responsive Libadwaita UI**, **asynchronous non-blocking threading**, and **robust privilege isolation** via the `HostBridge` pattern.

---

## 2. Architecture & Data Flow

```
+--------------------------------------------------------------------------------------------------+
|                                        SECURITY PAGE (UI)                                        |
|                          AdwViewStack / AdwViewSwitcher: [Malware Scanner | CVE Audit]           |
+--------------------------------------------------------------------------------------------------+
                                        |                                  |
                                        v                                  v
+------------------------------------------------------+  +----------------------------------------+
|             CLAMAV ORCHESTRATION ENGINE              |  |         CVE VULNERABILITY AUDITOR      |
|           (backend/antivirus_clamav.py)              |  |          (backend/security_cve.py)     |
+------------------------------------------------------+  +----------------------------------------+
| - FreshClam signature freshness checker & updater    |  | - Distro package query (APT / DNF)     |
| - Scan execution (Quick, Full, Custom path)          |  | - OSV.dev REST API & local advisory DB|
| - clamd Unix socket / clamscan CLI streaming parser  |  | - CVSS v3 score parsing & prioritization|
| - Threat detection listener                          |  | - Remediating patch version resolver   |
| - Quarantine vault manager (AES / file renaming)     |  | - One-click package update orchestrator|
+------------------------------------------------------+  +----------------------------------------+
                                        \                                  /
                                         \                                /
                                          v                              v
+--------------------------------------------------------------------------------------------------+
|                                    HOST BRIDGE & PRIVILEGE LAYER                                 |
|                       (Flatpak: flatpak-spawn --host | Polkit: pkexec)                           |
+--------------------------------------------------------------------------------------------------+
                                        |                                  |
                                        v                                  v
+------------------------------------------------------+  +----------------------------------------+
|                   LOCAL HOST SYSTEM                  |  |         REMOTE SECURITY FEEDS          |
|  - clamscan / clamdscan / freshclam                  |  |  - api.osv.dev / Debian Security API   |
|  - Quarantine directory: ~/.local/share/.../vault    |  |  - ClamAV Signature Mirrors (database) |
|  - Package managers: dpkg-query / rpm / pacman       |  +----------------------------------------+
+------------------------------------------------------+
```

---

## 3. Subsystem 1: CVE Vulnerability Scanner

### 3.1 Data Acquisition Strategy
Rather than maintaining a proprietary vulnerability database, Mithrandir will leverage a hybrid local/remote approach:

1. **Package Inventory Extraction**:
   - Reuses `PackageManager.get_installed_packages()` from `packages.py` to extract installed package names, architectures, and exact version strings (e.g., `openssl 3.0.2-0ubuntu1.14`).
   - Queries running kernel via `uname -r`.

2. **Vulnerability Query Source**:
   - **Universal Primary: Google OSV (Open Source Vulnerabilities) API** (`https://api.osv.dev/v1/query`):
     - Free, high-speed, structured JSON format with CVSS scores, affected version ranges, fixed version tags, and upstream CVE references.
     - Supports Debian (`Debian:11`, `Debian:12`), Ubuntu, Alpine, Rocky Linux, Python, npm, etc.
   - **Native Distro Fallback**:
     - Debian/Ubuntu: Parsing `/var/lib/dpkg/status` against `debsecan` or Ubuntu Security Notices (USN).
     - Fedora/RHEL: `dnf updateinfo --security --list` through `HostBridge`.
     - Arch Linux: `arch-audit --json`.

### 3.2 Scanning Pipeline
```python
class CveScanner:
    @staticmethod
    def audit_installed_packages(callback_progress=None) -> list[CveFinding]:
        # 1. Fetch package list from host
        # 2. Batch packages into chunks of 100
        # 3. Query OSV batch endpoint / distro tracker
        # 4. Parse response: Filter only vulnerabilities affecting the current version
        # 5. Extract: CVE ID, Severity (CRITICAL, HIGH, MEDIUM, LOW), Summary, Fixed Version
        # 6. Return list of CveFinding dataclass items
```

### 3.3 Mitigation & Fix Action
- Each identified CVE item will display:
  - **CVE Badge**: e.g., `CVE-2024-21626` (hyperlinked to NIST NVD / OSV).
  - **Severity Pill**: Red (`Critical`), Orange (`High`), Yellow (`Medium`), Blue (`Low`).
  - **Vulnerable Package**: Current version vs Fixed version.
- **One-Click Remediation**:
  - Clicking "Upgrade Package" dispatches `pkexec apt-get install --only-upgrade <package>` or `pkexec dnf upgrade <package>` via `HostBridge`.

---

## 4. Subsystem 2: ClamAV Antivirus & Anti-Malware

### 4.1 ClamAV Integration Architecture
ClamAV operates through two modes depending on system configuration:
1. **Daemon Mode (`clamdscan`)**: If `clamav-daemon` is installed and running, scans communicate over `/run/clamav/clamd.ctl` socket. This provides near-instant scanning with pre-loaded virus definitions in RAM.
2. **Standalone Mode (`clamscan`)**: Fallback if the daemon is inactive. Loads definitions dynamically per scan.

### 4.2 Signature Freshness Management (`freshclam`)
- **Status Check**: Inspect `/var/lib/clamav/daily.cvd` and `main.cvd` timestamps.
- **One-Click Database Update**:
  - Executes `freshclam` through `HostBridge` (elevated via `pkexec freshclam` if required).
  - Emits real-time progress callbacks to the UI toast overlay.

### 4.3 Scan Profiles
1. **Quick Scan**:
   - High-risk volatile paths:
     - `~/.cache`
     - `~/Downloads`
     - `/tmp` and `/var/tmp`
     - `~/.config/autostart` and `/etc/xdg/autostart`
     - Running process executables: `/proc/[pid]/exe`
   - Estimated runtime: 30–90 seconds.
2. **Full System Scan**:
   - Root filesystem `/` with strict virtual filesystem exclusions:
     - `--exclude-dir="^/sys"`
     - `--exclude-dir="^/proc"`
     - `--exclude-dir="^/dev"`
     - `--exclude-dir="^/run"`
     - `--exclude-dir="^/media"` (unless explicitly requested)
3. **Custom Target Scan**:
   - Native GTK 4 `FileDialog` / `FileLauncher` for external USB flash drives, network shares, or specific directories.

### 4.4 Process Throttling & Output Parsing
To prevent the anti-malware scanner from freezing the user desktop:
- Run scanning processes with reduced process and I/O priority:
  ```bash
  nice -n 19 ionice -c 3 clamscan -r -i --no-summary <target_dir>
  ```
- Continuously read `stdout` line by line:
  - Line matches `^(.+): (.+) FOUND$`:
    - Emits malware finding: `threat_name = match.group(2)`, `infected_file = match.group(1)`
  - Periodically count scanned files to update the UI counter.

### 4.5 Quarantine Vault
- **Quarantine Directory**: `~/.local/share/io.github.antonio.Mithrandir/quarantine/` (permissions `0700`).
- **Safety Neutralization**:
  - Remove execute bits: `chmod 0000 <quarantined_file>`.
  - Append `.mithrandir-quarantine` extension to disallow MIME-type associations.
  - Store metadata JSON alongside the artifact (Original path, Timestamp, Threat signature name, SHA256 checksum).
- **User Actions**:
  - "Restore File" (reverses quarantine, restores original permissions).
  - "Delete Permanently" (`shred` or `unlink`).
  - "Report False Positive".

---

## 5. UI/UX Design (Libadwaita)

A new page: **`SecurityPage` (`AdwNavigationSplitView` page item)**.

### Layout Hierarchy
```
AdwToolbarView
├── Top Bar: AdwHeaderBar
│   └── Centered Title: "Security & Protection"
└── AdwViewStack
    ├── Tab 1: "Anti-Malware (ClamAV)"
    │   ├── AdwBanner: Database Outdated Warning (if definitions > 7 days old)
    │   ├── AdwPreferencesGroup: "Engine Status"
    │   │   ├── AdwActionRow: "Virus Definitions" (e.g., "Updated Today (Version 27142)")
    │   │   │   └── Suffix Button: "Update Definitions"
    │   │   └── AdwActionRow: "ClamAV Daemon" (Active / Inactive)
    │   ├── AdwPreferencesGroup: "Scan Options"
    │   │   ├── Hero Card: [ Quick Scan ]  [ Full Scan ]  [ Custom Folder ]
    │   ├── Active Scan Progress Box (visible during scan):
    │   │   ├── GtkProgressBar (pulsing or percent)
    │   │   ├── Label: "Scanned 14,250 files... Current: /usr/lib/..."
    │   │   └── Suffix Button: [ Cancel Scan ] (Destructive action)
    │   └── AdwPreferencesGroup: "Quarantine Vault & Detections"
    │       └── List of detected threats with remediation buttons
    └── Tab 2: "CVE Vulnerability Audit"
        ├── AdwPreferencesGroup: "System Audit Summary"
        │   ├── 4-Column Metric Grid: [Critical: 0] [High: 2] [Medium: 5] [Low: 8]
        │   └── Action Button: "Run Vulnerability Scan"
        └── AdwPreferencesGroup: "Vulnerable Packages"
            └── AdwExpanderRow: package name & version
                ├── Subtitle: CVE identifier & CVSS score
                ├── Description: Vulnerability summary from NVD/OSV
                └── Suffix Button: "Upgrade to [Fixed Version]"
```

---

## 6. Implementation Checklist & File Roadmap

| Phase | Target File | Responsibility |
| :--- | :--- | :--- |
| **1. CVE Backend** | `src/mithrandir/backend/security_cve.py` | Query package manager, format batch payload for OSV API, compute CVSS categorization, generate upgrade commands. |
| **2. ClamAV Backend**| `src/mithrandir/backend/antivirus_clamav.py` | Check ClamAV binary availability, execute `freshclam`, run `clamscan` with `nice`/`ionice`, stream progress, manage quarantine vault. |
| **3. Security UI** | `src/mithrandir/ui/security_page.py` | Build Libadwaita view stack with ClamAV and CVE tabs, live progress ticker, and remediation dialogues. |
| **4. Integration** | `src/mithrandir/window.py` | Add "Security" navigation item to main sidebar with shield icon (`security-high-symbolic`). |
| **5. Configuration**| `data/io.github.antonio.Mithrandir.gschema.xml`| Add keys for quarantine directory, last scan timestamp, auto-update virus DB, and default scan profile. |
| **6. Testing** | `tests/test_security.py` | Mock OSV API responses, mock `clamscan` stdout parser, test quarantine file isolation logic. |

---

## 7. Security & Sandboxing Considerations

- **Privilege Separation**: Scans run under unprivileged user permissions by default. Polkit elevation is requested **only** when scanning root-restricted directories or executing system package updates.
- **Quarantine Safety**: Files moved to the quarantine vault are stripped of execution flags (`chmod 0000`) to guarantee that quarantined binaries cannot be accidentally spawned by other user applications or background daemons.
- **Flatpak Compatibility**: When packaged as Flatpak, ClamAV and package inspection will leverage the existing `HostBridge` (`flatpak-spawn --host`) mechanism, avoiding the need to bundle the entire 200MB+ virus definitions database inside the Flatpak runtime image.
