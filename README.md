# Mithrandir

<p align="center">
  <img src="data/icons/hicolor/scalable/apps/io.github.antonio.Mithrandir.svg" width="128" height="128" alt="Mithrandir Application Icon" />
</p>

<p align="center">
  <strong>Next-Generation GNU/Linux System Optimizer, Deep Cleaner, and AI-Assisted Diagnostics Engine</strong>
</p>

<p align="center">
  <a href="https://github.com/Antux912012/mithrandir/releases"><img src="https://img.shields.io/badge/version-0.1.0--experimental-blue.svg" alt="Version" /></a>
  <a href="https://www.gnu.org/licenses/gpl-3.0"><img src="https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg" alt="License: GPL-3.0-or-later" /></a>
  <a href="https://gtk.org/"><img src="https://img.shields.io/badge/GUI-GTK%204%20%7C%20Libadwaita-brightgreen.svg" alt="GTK4 & Libadwaita" /></a>
  <a href="https://www.python.org/"><img src="https://img.shields.io/badge/Python-3.10%2B-informational.svg" alt="Python 3.10+" /></a>
  <a href="https://github.com/Antux912012/mithrandir/actions"><img src="https://img.shields.io/badge/Platform-GNU%2FLinux-orange.svg" alt="Platform" /></a>
</p>

---

## 1. Executive Overview

**Mithrandir** is an experimental, enterprise-grade system maintenance, optimization, and diagnostic utility designed natively for the modern GNU/Linux desktop. Built with **Python 3**, **GTK 4**, and **Libadwaita**, Mithrandir synthesizes the best paradigms of desktop maintenance utilities:

- The proactive, heuristic memory optimization and threshold automation of **Microsoft PC Manager**
- The deep storage recovery and granular cleaning rules of **BleachBit**
- The high-fidelity telemetry, service controls, and process monitoring of **Stacer**
- A sandboxed **AI Diagnostics Assistant** connecting local LLMs (Ollama, LM Studio) or cloud endpoints (OpenAI-compatible) to triage complex systemd logs, kernel alerts, and system issues with automated PII redaction and deterministic command safety guards.

Mithrandir adheres strictly to GNOME Human Interface Guidelines (HIG), featuring adaptive responsive breakpoints (`AdwBreakpoint`), kinetic scrolling, and dynamic branding (`#2b7bdd`) with luminance-adaptive contrast scaling for dark and light desktop palettes.

---

## 2. System Architecture & Engineering Design

```
+---------------------------------------------------------------------------------------+
|                                    PRESENTATION LAYER                                 |
|         GTK 4 + Libadwaita (AdwApplicationWindow, AdwNavigationSplitView, Breakpoints)|
|             Dynamic Accent Branding (#2b7bdd) | Kinetic Scrolling | GNOME HIG         |
+---------------------------------------------------------------------------------------+
        |                                       |                                   |
        v                                       v                                   v
+-----------------------+       +-------------------------------+       +-----------------------+
|   DASHBOARD & SENSORS |       |   SYSTEM MANAGEMENT MODULES   |       |   AI DIAGNOSTICS &    |
| - Host Telemetry      |       | - Deep Cleaner (BleachBit)    |       |   SAFETY BOUNDARY     |
| - Multi-Core CPU/RAM  |       | - Task/Process Manager        |       | - Ollama / LM Studio  |
| - Disk / Network I/O  |       | - Systemd Units & Autostart   |       | - OpenAI API Client   |
| - Dual GPU Load       |       | - BTRFS Snapshots Manager     |       | - PII Redaction Layer |
| - Smart Boost Banner  |       | - GRUB Configuration Engine   |       | - Command Denylist    |
|                       |       | - Multi-Format Uninstaller    |       |                       |
+-----------------------+       +-------------------------------+       +-----------------------+
        \                                       |                                   /
         \                                      |                                  /
          +-------------------------------------+---------------------------------+
                                                |
                                                v
+---------------------------------------------------------------------------------------+
|                                    BACKEND ENGINE                                     |
|  - Asynchronous Worker Threads & GLib.idle_add Dispatchers                            |
|  - GSettings (io.github.antonio.Mithrandir) State & Threshold Persistence             |
|  - Background Daemon Lifecycle (SmartBoostEngine & Minimize-to-Tray/Background)       |
+---------------------------------------------------------------------------------------+
                                                |
                                                v
+---------------------------------------------------------------------------------------+
|                          HOST BRIDGE & PRIVILEGE ELEVATION                            |
|  - HostBridge Subsystem: Flatpak Sandbox Detection (flatpak-spawn --host)             |
|  - Polkit (pkexec) Elevated Action Handlers                                           |
|  - Native Host Fallbacks (/proc, /sys, systemctl, btrfs, update-grub)                 |
+---------------------------------------------------------------------------------------+
```

### 2.1 Host Bridge & Sandboxing Isolation (`host_bridge.py`)
Mithrandir operates transparently across both native host environments (`.deb`, RPM, local user builds) and containerized sandboxes (**Flatpak**). The `HostBridge` abstraction dynamically determines execution context:
- Inside Flatpak (`/.flatpak-info` presence), host operations are marshaled via `flatpak-spawn --host`.
- Privileged operations (kernel cache flushing, BTRFS subvolume deletions, `update-grub`, `systemctl` unit manipulation) invoke authenticated privilege escalators (`pkexec`) with structured arguments.
- Standard read-only inspections parse virtual filesystems directly (`/proc/stat`, `/proc/meminfo`, `/sys/class/thermal`, `/proc/net/dev`).

### 2.2 Concurrency & UI Responsiveness
Long-running scanning and I/O-intensive cleaning operations run in isolated background daemon threads (`threading.Thread`). State transitions, progress counters, and telemetry metrics communicate back to the GTK main loop through thread-safe `GLib.idle_add()` callbacks, preventing UI thread starvation and maintaining 60 FPS transitions.

### 2.3 Background Lifecycle & Smart Boost Daemon (`smart_boost.py`)
When enabled, Mithrandir can minimize to background on window close (`hide_on_close`). An internal optimization loop monitors memory consumption and disk clutter at periodic intervals. When memory utilization or temporary cache volume crosses user-configured thresholds in `Gio.Settings`, the Smart Boost engine automatically triggers standby cache eviction and temp purging.

---

## 3. Comprehensive Feature Breakdown

### 🚀 Smart Boost & Real-Time Memory Compression
- **One-Click Boost**: Instantaneous eviction of pagecache, dentries, and inodes (`echo 3 > /proc/sys/vm/drop_caches`) paired with volatile temporary file reclamation (`/tmp`, user thumbnail caches).
- **Automated Threshold Triggers**: Configurable memory threshold (e.g., auto-boost when RAM exceeds 80%) and disk clutter threshold (e.g., auto-clean when temp files exceed 1.0 GB).
- **Safe Mode Guards**: Prevents continuous or thrashing cache drops with adaptive cooldown intervals.

### 📊 System Information & Stacer-Grade Telemetry
- **Hardware Architecture Summary**: Hostname, Distribution, Kernel version, CPU model, Physical Cores, Total RAM, and Active Display Server (Wayland/X11).
- **High-Frequency Sensor Gauges**:
  - Multi-Core CPU Aggregate Utilization
  - Physical Memory (Used, Available, Standby/Cached)
  - Root Filesystem Mount Capacity & Inode Ratios
  - Hardware Thermal Zones (`/sys/class/thermal` & `coretemp`)
  - Real-time Network Throughput (Download Rx / Upload Tx in KB/s and MB/s)
  - Dual GPU Detection & Metric Scraping (NVIDIA NVML & AMD/Intel DRM sysfs nodes)

### 🧹 Multi-Tier Deep Cleaner Engine
- **Stremio Streaming Cache**: Cleans video chunk caches and torrent part buffers in `~/.stremio-server`, Flatpak (`~/.var/app/com.stremio.Stremio`), and Snap directories that often silently consume tens of gigabytes.
- **Application & Desktop Caches**: Scans `~/.cache`, thumbnail databases (`~/.cache/thumbnails`), and stale crash dumps.
- **Flatpak App Caches**: Granular scanning across `~/.var/app/*/cache` with safety whitelisting to protect active application states and self-referential paths.
- **Snap Caches & Revisions**: Reclaims disabled snap revisions (`snap set system refresh.retain=2`), `/var/lib/snapd/cache`, and per-user snap caches.
- **System Logs & Journals**: Scans and vacuums `systemd-journald` logs older than specified retention horizons.
- **Package Manager Caches**: Purges unneeded `.deb` archives in `/var/cache/apt/archives` and RPM equivalents.
- **Trash Bin**: Safely empties XDG compliant trash bins across connected mount points.
- **Dry-Run Inspection Mode**: Complete non-destructive preview displaying exact byte counts, item counts, and paths prior to committing deletion.

### ⚙️ Task & Process Manager
- **Live Process Inspection**: Continuous refresh of running processes with PID, Process Name, User, CPU %, and Resident Memory (RSS).
- **Real-Time Filter & Search**: Instant substring search across process names and command lines.
- **Process Termination Signals**:
  - Graceful termination via standard `SIGTERM` (15)
  - Elevated forceful kill via `SIGKILL` (9) through Polkit

### 🛡️ Systemd Services Inspector
- **Unit Management**: Live query of active, inactive, enabled, and disabled systemd services (`systemctl list-units`).
- **Critical Service Safety Guards**: Built-in warning and confirmation prompts for essential core units (e.g., `dbus`, `systemd-logind`, `NetworkManager`, `sshd`, display managers) to prevent accidental bricking of user sessions.
- **Service Control**: Start, stop, and restart services directly from the graphical interface.

### 🔄 Startup & Autostart Manager
- **Dual Scope Support**: Inspects and toggles both User Autostart entries (`~/.config/autostart/*.desktop`) and System-wide entries (`/etc/xdg/autostart/*.desktop`).
- **Custom Startup Command Entry**: Add custom executables or shell scripts to boot sequences with friendly display names and descriptions.
- **Mithrandir Self-Boot Configuration**: Toggle Mithrandir background launch directly on user login.

### 💾 BTRFS Subvolume & Snapshot Management
- **Automatic Filesystem Detection**: Identifies whether root or data mounts utilize BTRFS.
- **Snapshot Integration**: Discovers native BTRFS subvolume snapshots, Snapper snapshots (`/.snapshots`), and Timeshift subvolumes (`/run/timeshift/backup`).
- **Snapshot Creation**: Create instantaneous read-only or read-write snapshots with custom naming conventions.
- **Retention & Purge Policies**: Safely purge obsolete snapshots to free underlying extent allocation.

### 🥾 GRUB Bootloader Personalization
- **Configuration Parsing**: Safely parses and updates `/etc/default/grub`.
- **Customizable Boot Parameters**:
  - Boot Menu Timeout (`GRUB_TIMEOUT`)
  - Default Kernel Entry (`GRUB_DEFAULT`)
  - Quiet Splash toggling (`GRUB_CMDLINE_LINUX_DEFAULT`)
  - OS Prober Activation (`GRUB_DISABLE_OS_PROBER`)
- **Transactional Backup & Rollback**: Automatic generation of `/etc/default/grub.mithrandir.bak` prior to edits, with one-click "Revert to Default" restoration.
- **Kernel Boot Generation**: Invokes `update-grub` / `grub2-mkconfig` through the HostBridge with polkit elevation.

### 📦 Unified Software Uninstaller
- **Cross-Format Support**: Queries and uninstalls packages across Native APT/RPM, Flatpak applications (`flatpak list --app`), and Canonical Snap packages (`snap list`).
- **App Data Cleaner**: Prompts to optionally remove lingering configuration and data folders upon uninstalling.

### 🧠 AI Diagnostics Assistant
- **Endpoint Agnostic**:
  - Local LLMs via **Ollama** (`http://localhost:11434/v1`)
  - Local LLMs via **LM Studio** (`http://localhost:1234/v1`)
  - Cloud Providers via **OpenAI-Compatible** API specifications
- **PII Redaction Guardrail**: Sensitive user information (usernames, IP addresses, MAC addresses, home directories, hostnames) is redacted before payload transmission.
- **Deterministic Command Denylist**: Remediation shell commands generated by AI models are evaluated against an immutable denylist blocking destructive commands (e.g., `rm -rf /`, `mkfs`, fork bombs, raw block device writes, unauthorized curl-pipe-to-bash).

---

## 4. Installation & Build Instructions

### 4.1 Debian / Ubuntu (.deb Package)

Install the pre-compiled `.deb` package:
```bash
sudo apt install ./mithrandir_0.1.0_all.deb
```

Or build the package directly from source:
```bash
# Clean previous build artifacts
rm -rf _build_deb pkg-deb

# Setup meson build with /usr prefix
meson setup _build_deb --prefix=/usr

# Install to packaging staging root
DESTDIR=$(pwd)/pkg-deb ninja -C _build_deb install

# Prepare Debian control files
mkdir -p pkg-deb/DEBIAN
cp packaging/debian/control packaging/debian/postinst packaging/debian/postrm pkg-deb/DEBIAN/
chmod 755 pkg-deb/DEBIAN/postinst pkg-deb/DEBIAN/postrm

# Clean bytecode artifacts and assemble package
find pkg-deb -type d -name "__pycache__" -exec rm -rf {} +
dpkg-deb --build --root-owner-group pkg-deb mithrandir_0.1.0_all.deb
```

### 4.2 Flatpak (Sandboxed)

Build and install locally using `flatpak-builder`:
```bash
# Build and install for current user
flatpak-builder --user --install --force-clean build-dir io.github.antonio.Mithrandir.json

# Execute the application
flatpak run io.github.antonio.Mithrandir
```

### 4.3 Local User Installation (Meson / Ninja)

Install to `$HOME/.local` without system-wide root modifications:
```bash
# Setup build directory
meson setup _build_user --prefix=$HOME/.local

# Compile schemas, locales, and install
ninja -C _build_user install

# Run application
~/.local/bin/mithrandir
```

---

## 5. Verification & Test Suite

Mithrandir includes a comprehensive test suite covering cleaner logic, host bridge isolation, AI client boundaries, safety denylists, and system monitor telemetry scrapers.

Execute the test suite:
```bash
PYTHONPATH=src python3 -m unittest discover -s tests
```

Expected output:
```text
................
----------------------------------------------------------------------
Ran 16 tests in 1.129s

OK
```

---

## 6. Security, Safety, & Permissions

| Subsystem | Execution Boundary | Escalation Mechanism | Safety Controls |
| :--- | :--- | :--- | :--- |
| **Cleaner Engine** | User Space | None (user cache) / Polkit (system logs) | Dry-run preview, Path Whitelisting, Self-Preservation rules |
| **Smart Boost** | User Space & Host Bridge | Polkit (`pkexec`) | Cache drop only (`drop_caches`), No kill of running processes |
| **Systemd Services** | Host Bridge / D-Bus | Polkit (`systemctl`) | Core system unit blacklist with modal confirmation |
| **GRUB Editor** | Host Bridge | Polkit (`pkexec tee /etc/default/grub`) | Automated backup creation, Syntax verification, Revert button |
| **BTRFS Snapshots**| Host Bridge | Polkit (`btrfs subvolume`) | Read-only options, Confirmation dialogs before purge |
| **AI Diagnostics** | Isolated Sandbox | None | Automatic PII sanitization, Immutable command execution denylist |

---

## 7. Configuration & GSettings

Application settings are managed through `io.github.antonio.Mithrandir` GSettings schema:

```bash
# Read smart boost threshold
gsettings get io.github.antonio.Mithrandir smart-boost-ram-threshold

# Enable auto boost
gsettings set io.github.antonio.Mithrandir auto-boost-enabled true

# Configure AI Endpoint
gsettings set io.github.antonio.Mithrandir ai-api-base "http://localhost:11434/v1"
gsettings set io.github.antonio.Mithrandir ai-model "llama3.2"
```

---

## 8. Repository Information & Contributing

- **Repository**: [https://github.com/Antux912012/mithrandir](https://github.com/Antux912012/mithrandir)
- **Issue Tracker**: [https://github.com/Antux912012/mithrandir/issues](https://github.com/Antux912012/mithrandir/issues)
- **License**: GNU General Public License v3.0 or later ([GPL-3.0-or-later](LICENSE))

Contributions, feature suggestions, and bug reports are welcome. Please open an issue or pull request on GitHub.

---

## 9. Acknowledgments & Inspirations

Mithrandir stands on the shoulders of remarkable open-source projects and developer utilities. Sincere gratitude and appreciation are extended to:

- **[Stacer](https://github.com/oguzhaninan/Stacer)** — For pioneering modern, intuitive all-in-one Linux system monitoring, task management, and service control.
- **[Microsoft PC Manager](https://pcmanager.microsoft.com/)** — For inspiring the proactive, low-overhead "Smart Boost" mechanism, automating standby RAM purging and temporary clutter threshold monitoring.
- **[BleachBit](https://www.bleachbit.org/)** — For establishing the gold standard in deep system cleaning algorithms, granular path scanning, dry-run previews, and safe cache vacuuming.
- **[Snapper](https://github.com/openSUSE/snapper)** & **[Timeshift](https://github.com/linuxmint/timeshift)** — For their foundational work on BTRFS copy-on-write (COW) snapshot creation, layout conventions, and filesystem restore points.
- **[Stremio](https://www.stremio.com/)** — For the streaming media platform that inspired Mithrandir's specialized torrent chunk and video part cache cleaner.
- **[GNOME](https://www.gnome.org/) & [Libadwaita](https://gitlab.gnome.org/GNOME/libadwaita)** — For the design philosophy, adaptive widgets, and modern UI foundation that make Linux applications a pleasure to use.
- **[Ollama](https://ollama.com/) & [LM Studio](https://lmstudio.ai/)** — For empowering local, privacy-first AI model inference and enabling offline intelligent system log triage.

