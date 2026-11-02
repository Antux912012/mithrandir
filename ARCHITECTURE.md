# ARCHITECTURE.md — Mithrandir Unified System Optimization & Kernel Utility

## 1. Project Overview & Target Environments
**Mithrandir** (consolidating `mithrandir` and `mithrandir-rs`) is a native Linux system optimization, cache management, CachyOS scheduler controller, and hybrid GPU power utility written in **Rust**, **GTK4**, **Libadwaita**, and **`ksni`** (StatusNotifierItem system tray).

### Primary Target Environments
1. **Debian Testing / Unstable** with the **CachyOS Kernel** (`linux-cachyos`, BORE, EEVDF, `sched-ext`).
2. **Kepler OS**: Immutable rolling-release Linux distribution based on Debian Testing (`OSTree` / `bootc` container delivery, GNOME 50+, CachyOS kernel).
3. **Hardware Optimization Targets**: Intel H-series / Core / Xeon laptops and workstations with hybrid Intel Integrated + NVIDIA Optimus discrete GPUs (spanning legacy Maxwell/Pascal GPUs such as NVIDIA Quadro M1200 up to modern Turing/Ampere/Ada architectures).

### Design & UX Standards
- **UI Framework**: GTK4 + Libadwaita adhering strictly to **GNOME Human Interface Guidelines (HIG)**.
- **Visual Theme**: Nord color palette integration and Papirus icon naming compatibility.
- **Privilege Separation**: The GTK4 GUI and `ksni` tray run strictly unprivileged in userspace. All `/sys`, `/proc/sys`, and kernel module mutations execute via D-Bus (`zbus`) or an isolated Polkit helper subcommand (`pkexec mithrandir-helper` / `mithrandir --apply-privileged`).

---

## 2. Repository Directory Structure

```text
mithrandir/
├── Cargo.toml
├── ARCHITECTURE.md
├── data/
│   ├── io.github.antux912012.Mithrandir.desktop
│   ├── io.github.antux912012.Mithrandir.metainfo.xml
│   ├── io.github.antux912012.Mithrandir.policy       # Polkit action definitions
│   ├── 90-mithrandir.rules                           # Polkit wheel/sudo passwordless rules
│   └── systemd/
│       └── mithrandir-gpu-boot.service               # Pre-GDM PCIe dGPU detachment unit
├── src/
│   ├── main.rs                                       # GUI & CLI --apply-privileged entrypoint
│   ├── app.rs                                        # AdwApplication & lifecycle
│   ├── tray.rs                                       # ksni StatusNotifierItem system tray
│   ├── cleaner/
│   │   ├── mod.rs
│   │   ├── flatpak.rs                                # Hardened XDG-compliant Flatpak cleaner
│   │   └── system.rs                                 # Package & user cache cleaner
│   ├── kernel/
│   │   ├── mod.rs
│   │   ├── bore.rs                                   # CachyOS BORE / EEVDF sysctl controller
│   │   └── scx.rs                                    # sched-ext BPF loader (org.scx.Loader D-Bus)
│   ├── power/
│   │   ├── mod.rs
│   │   ├── profiles.rs                               # CPU governor, EPP, Turbo Boost, UPower D-Bus
│   │   ├── gpu.rs                                    # NVIDIA dGPU Runtime PM & PCIe bus detachment
│   │   └── coordinator.rs                            # Unified 4-stage Mithrandir Preset synchronization
│   └── ui/
│       ├── window.rs                                 # Main AdwApplicationWindow
│       ├── cleaner_page.rs                           # Cache & Flatpak maintenance view
│       └── kernel_power_page.rs                      # CachyOS Scheduler, CPU & Hybrid GPU view
└── tests/
    ├── flatpak_cleaner_test.rs                       # Data-loss prevention unit tests
    ├── kernel_sysfs_test.rs                          # Mock procfs/sysfs scheduler & EPP tests
    └── gpu_power_test.rs                             # Mock PCIe topology discovery & guard tests
```

---

## 3. Module 1: Crate Consolidation & Rebranding (`mithrandir-rs` -> `mithrandir`)

1. **`Cargo.toml` Standardization**:
   - Set `[package] name = "mithrandir"` and `[[bin]] name = "mithrandir"`.
   - Set `repository = "https://github.com/Antux912012/mithrandir"`.
2. **Application Identifiers**:
   - Unify `application_id` to `io.github.antux912012.Mithrandir`.
   - Update all `.desktop`, `.metainfo.xml`, Polkit `.policy`, and `ksni` tray tooltips from `Mithrandir-RS` to `Mithrandir`.

---

## 4. Module 2: Hardened Flatpak Cache Cleaner (`src/cleaner/flatpak.rs`)

### 4.1 Root Cause & Strict XDG Sandbox Rules
Flatpak stores per-application files under `$HOME/.var/app/<app-id>/`. Wiping the `<app-id>` root folder or calling `flatpak uninstall --delete-data` destroys user configurations. Mithrandir MUST enforce strict XDG boundaries:

| Path | XDG Role | Cleaner Action |
| :--- | :--- | :--- |
| `~/.var/app/<app-id>/cache/` | `XDG_CACHE_HOME` | **SAFE TO CLEAN** (inner contents only) |
| `~/.var/app/<app-id>/config/` | `XDG_CONFIG_HOME` | **PROTECTED — NEVER TOUCH** |
| `~/.var/app/<app-id>/data/` | `XDG_DATA_HOME` | **PROTECTED — NEVER TOUCH** |
| `~/.var/app/<app-id>/.local/state/` | `XDG_STATE_HOME` | **PROTECTED — NEVER TOUCH** |
| `~/.cache/flatpak/` | System/User Repo Cache | **SAFE TO CLEAN** |
| `/var/tmp/flatpak-cache-*` | Temporary OSTree Staging | **SAFE TO CLEAN** |

### 4.2 Path Validation & Deletion Guardrails
1. **Path Validator (`is_safe_flatpak_cache_dir`)**:
   - Verify the path is not a symbolic link (`fs::symlink_metadata(path)?.file_type().is_symlink() == false`) to prevent sandbox symlink escapes.
   - Verify `path.file_name() == Some(OsStr::new("cache"))` and its parent directory resides inside `.var/app/<app-id>`.
   - Explicitly reject any path containing `/config`, `/data`, or `/.local/state`.
2. **Inner Content Deletion (`clean_flatpak_app_cache`)**:
   - Never remove the `~/.var/app/<app-id>/cache` directory inode itself (running apps may hold an open directory descriptor). Iterate `std::fs::read_dir(&cache_dir)` and delete only its children:
   ```rust
   pub fn clean_flatpak_app_cache(app_dir: &Path, active_apps: &HashSet<String>) -> Result<u64, CleanerError> {
       if let Some(app_id) = app_dir.file_name().and_then(|n| n.to_str()) {
           if active_apps.contains(app_id) {
               return Ok(0); // Skip currently running Flatpak applications
           }
       }
       let cache_dir = app_dir.join("cache");
       let meta = match std::fs::symlink_metadata(&cache_dir) {
           Ok(m) => m,
           Err(_) => return Ok(0),
       };
       if !meta.is_dir() || meta.file_type().is_symlink() {
           return Ok(0);
       }
       assert_eq!(cache_dir.file_name().and_then(|n| n.to_str()), Some("cache"));

       let mut freed_bytes = 0u64;
       for entry in std::fs::read_dir(&cache_dir)? {
           let entry = entry?;
           let path = entry.path();
           let entry_meta = std::fs::symlink_metadata(&path)?;
           freed_bytes += calculate_size(&path);
           if entry_meta.is_dir() && !entry_meta.file_type().is_symlink() {
               std::fs::remove_dir_all(&path)?;
           } else {
               std::fs::remove_file(&path)?;
           }
       }
       Ok(freed_bytes)
   }
   ```
3. **Running Application Detection**:
   - Query `flatpak ps --columns=application` before cleaning and populate `active_apps: HashSet<String>` to skip running sandboxes.
4. **Separation of CLI Operations**:
   - **Cache Clean**: Cleans `~/.var/app/*/cache/*` and `~/.cache/flatpak/*`. **NEVER** passes `--delete-data`.
   - **Unused Runtimes**: Runs `flatpak uninstall --unused -y --noninteractive`.
   - **Orphaned Data (Opt-In Destructive Action)**: Lists directories in `~/.var/app/` that do not appear in `flatpak list --app --columns=application`. Requires an explicit Libadwaita destructive confirmation dialog (`AdwAlertDialog`) before removal.

---

## 5. Module 3: CachyOS Kernel Scheduler & CPU Power Controller (`src/kernel/` & `src/power/profiles.rs`)

### 5.1 BORE / EEVDF Kernel Tunables (`src/kernel/bore.rs`)
1. **Capability Detection**:
   - Inspect `/proc/version` and verify `/proc/sys/kernel/sched_bore` exists.
   - If absent (e.g., standard Debian kernel), mark BORE toggle as `Unsupported` and make the UI switch row insensitive with an explanatory subtitle.
2. **State Management**:
   - Read `/proc/sys/kernel/sched_bore`: `1` = **BORE** active, `0` = **EEVDF** active.
   - Apply runtime state via privileged helper: write `1` or `0` to `/proc/sys/kernel/sched_bore`.
   - Apply persistent state across reboots (compatible with Debian & Kepler OS `/etc` overlay): write `kernel.sched_bore = 1` (or `0`) to `/etc/sysctl.d/99-mithrandir-scheduler.conf`.

### 5.2 `sched-ext` (SCX) BPF Pluggable Schedulers (`src/kernel/scx.rs`)
1. **Sysfs Telemetry**:
   - Read `/sys/kernel/sched_ext/state` (`enabled` | `disabled`) and `/sys/kernel/sched_ext/root/ops` (e.g., `bpfland`, `lavd`, `rusty`, `flash`, `cosmos`).
2. **Primary Control via System D-Bus (`zbus` -> `org.scx.Loader`)**:
   - Service: `org.scx.Loader`, Path: `/org/scx/Loader`, Interface: `org.scx.Loader`.
   - Read Properties:
     - `CurrentScheduler` (`String`)
     - `SchedulerMode` (`u32`: `0` = Auto, `1` = Gaming, `2` = PowerSave, `3` = LowLatency, `4` = Server)
     - `SupportedSchedulers` (`Vec<String>`: e.g., `scx_bpfland`, `scx_lavd`, `scx_rusty`, `scx_flash`, `scx_cosmos`)
   - Invoke Methods:
     - `SwitchScheduler(scx_name: &str, mode: u32)`
     - `StopScheduler()` (detaches BPF scheduler and returns control to in-kernel BORE/EEVDF).
3. **CLI Fallback (`scxctl`)**:
   - If `org.scx.Loader` D-Bus is inactive, check `scxctl` binary availability (`scxctl switch --sched <name> --mode <mode>` and `scxctl stop`).

### 5.3 CPU Governors, EPP & Turbo Boost (`src/power/profiles.rs`)
1. **System Power Profiles (`zbus`)**:
   - Interface with `org.freedesktop.UPower.PowerProfiles` (fallback `net.hadess.PowerProfiles`) to read/write `ActiveProfile` (`power-saver`, `balanced`, `performance`).
2. **CPUFreq Governor & P-State EPP**:
   - Read `/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver` (`intel_pstate`, `intel_cpufreq`, `amd-pstate-epp`, `acpi-cpufreq`).
   - Enumerate `/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors` and write selected governor to `/sys/devices/system/cpu/cpu*/cpufreq/scaling_governor`.
   - Enumerate `/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_available_preferences` and write selected preference (`performance`, `balance_performance`, `balance_power`, `power`) to `/sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference`.
3. **CPU Turbo Boost Toggle**:
   - Intel (`intel_pstate`): `/sys/devices/system/cpu/intel_pstate/no_turbo` (`0` = Boost ON, `1` = Boost OFF).
   - AMD / Generic (`cpufreq`): `/sys/devices/system/cpu/cpufreq/boost` (`1` = Boost ON, `0` = Boost OFF).

---

## 6. Module 4: Hybrid NVIDIA dGPU Runtime PM & PCIe Bus Detachment (`src/power/gpu.rs`)

### 6.1 Hardware Topology Discovery
1. Scan `/sys/bus/pci/devices/*` for devices matching:
   - `vendor == 0x10de` (NVIDIA)
   - `class` prefix `0x0300` (VGA controller) or `0x0302` (3D controller).
2. Detect companion NVIDIA HD Audio function on the same PCI slot (e.g., if GPU is `0000:01:00.0`, check `0000:01:00.1` with `vendor == 0x10de` and `class == 0x040300`).
3. Cache discovered BDF addresses in `/run/mithrandir/dgpu_topology.json` (and `/var/lib/mithrandir/dgpu_topology.json`) so Mithrandir remembers the hardware identity even while the PCIe device is physically removed from the bus.
4. **Primary Display Safety Lock**:
   - Verify a separate integrated GPU (e.g., Intel `0x8086` at `0000:00:02.0`) is present and bound before enabling the PCIe detachment controls.

### 6.2 Three-Tier GPU Power States
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DgpuPowerMode {
    /// Kernel modules unloaded, PCIe device & audio sibling removed from /sys/bus/pci (0W draw)
    IntegratedOnlyPcieRemoved,
    /// PCIe device present, power/control = "auto" (RTD3 / D3cold dynamic sleep)
    HybridRuntimeAuto,
    /// PCIe device present, power/control = "on" (Forced D0 awake)
    PerformanceForcedOn,
}
```

### 6.3 Pre-Flight Safety Gate & Hard PCIe Detachment Pipeline
When transitioning to `IntegratedOnlyPcieRemoved`, the privileged helper (`mithrandir --dgpu-detach`) MUST execute these steps in strict order:
1. **Open File Descriptor Check**:
   - Scan `/proc/[0-9]*/fd/` for links pointing to `/dev/nvidia*` or `/dev/dri/by-path/pci-<gpu_bdf>-*` (or `/dev/dri/renderD129`).
   - If any process holds an open FD, abort immediately and return `GpuError::DeviceBusy(Vec<ProcessInfo>)` containing PID and process command name.
2. **Kernel Module Unload (Reverse Dependency Order)**:
   - Execute `modprobe -r nvidia_uvm nvidia_drm nvidia_modeset nvidia` (or `modprobe -r nouveau`).
   - Verify `/proc/modules` contains no loaded `nvidia` or `nouveau` modules.
3. **PCIe Function Removal**:
   - Write `"auto"` to `/sys/bus/pci/devices/<gpu_bdf>/power/control`.
   - If `<audio_bdf>` exists, write `"1"` to `/sys/bus/pci/devices/<audio_bdf>/remove`.
   - Write `"1"` to `/sys/bus/pci/devices/<gpu_bdf>/remove`.
4. **Re-Attachment Pipeline (`mithrandir --dgpu-attach`)**:
   - Write `"1"` to `/sys/bus/pci/rescan`.
   - Poll up to 1500ms for `/sys/bus/pci/devices/<gpu_bdf>` to appear.
   - Write `"auto"` to `power/control` on both `<gpu_bdf>` and `<audio_bdf>`.
   - Trigger `udevadm settle` and `modprobe nvidia_drm` (or `nouveau`).

### 6.4 Boot-Time Persistence Service (`mithrandir-gpu-boot.service`)
To prevent GNOME Wayland (`gnome-shell`) from opening `/dev/dri/renderD129` on boot when the user wants Integrated-Only battery mode:
- Store persistent configuration in `/etc/mithrandir/power.toml`.
- Ship `mithrandir-gpu-boot.service` configured with `DefaultDependencies=no`, `After=systemd-modules-load.service`, and `Before=display-manager.service gdm.service`.
- When `boot_dgpu_mode = "detached"`, detach the NVIDIA PCIe device before GDM starts.

---

## 7. Module 5: Unified "Mithrandir Presets" Coordinator (`src/power/coordinator.rs`)

Synchronize all subsystems into 4 one-click profiles accessible from both the GTK4 window and the `ksni` System Tray:

| Mithrandir Preset | UPower Profile | Governor / EPP | Scheduler (`sched-ext` / BORE) | CPU Turbo Boost | NVIDIA dGPU State |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Battery / Longevity** | `power-saver` | `powersave` / `power` | `scx_bpfland` (Mode 2: PowerSave) or BORE (`1`) | **OFF** (`no_turbo=1`) | `IntegratedOnlyPcieRemoved` (or `HybridRuntimeAuto`) |
| **Balanced (Daily Dev)** | `balanced` | `powersave` / `balance_performance` | In-Kernel **BORE** (`sched_bore=1`, SCX stopped) | **ON** | `HybridRuntimeAuto` |
| **Low-Latency / IDE** | `performance` | `performance` / `balance_performance` | `scx_lavd` or `scx_bpfland` (Mode 3: LowLatency) | **ON** | `HybridRuntimeAuto` |
| **Max Performance** | `performance` | `performance` / `performance` | `scx_bpfland` or `scx_lavd` (Mode 1: Gaming) | **ON** | `PerformanceForcedOn` (PCIe attached) |

---

## 8. GTK4 / Libadwaita UI & `ksni` System Tray Layout

### 8.1 Main Window (`AdwApplicationWindow` + `AdwViewStack`)
1. **View 1: System & Flatpak Cleaner (`src/ui/cleaner_page.rs`)**
   - `AdwPreferencesGroup` ("Flatpak Sandbox Cache"):
     - Displays calculated size exclusively from `~/.var/app/*/cache` and `~/.cache/flatpak`.
     - Badge showing number of skipped running Flatpak apps.
     - Action button: **Clean Safe Caches** (`suggested-action` style).
   - `AdwPreferencesGroup` ("Advanced Flatpak Maintenance"):
     - Action row: **Remove Unused Runtimes** (`flatpak uninstall --unused`).
     - Action row: **Scan Orphaned App Data** (`destructive-action` confirmation dialog).
2. **View 2: Kernel, Scheduler & Power (`src/ui/kernel_power_page.rs`)**
   - **Status Banner**: Displays kernel release (`uname -r`), active scheduler badge (`BORE`, `EEVDF`, or `scx_*`), CPU scaling driver, and dGPU D-state / PCIe status.
   - **Group 1 — Mithrandir Unified Presets**: 4-button segmented selector (`Battery`, `Balanced`, `Low-Latency`, `Max Performance`).
   - **Group 2 — CachyOS Scheduler**:
     - `AdwSwitchRow`: BORE Burst-Oriented Scheduler (`kernel.sched_bore`).
     - `AdwComboRow`: `sched-ext` BPF Scheduler (`Disabled (Use BORE/EEVDF)`, `scx_bpfland`, `scx_lavd`, `scx_rusty`, `scx_flash`, `scx_cosmos`).
     - `AdwComboRow`: `sched-ext` Profile Mode (`Auto`, `Gaming`, `PowerSave`, `LowLatency`, `Server`).
   - **Group 3 — CPU Frequency & Thermals**:
     - `AdwComboRow`: Scaling Governor.
     - `AdwComboRow`: Energy Performance Preference (EPP).
     - `AdwSwitchRow`: CPU Turbo Boost.
   - **Group 4 — Hybrid NVIDIA dGPU & PCIe Power**:
     - `AdwActionRow`: Detected GPU model and live power state (`PCIe Detached (0W)`, `Suspended (D3cold)`, `Active (D0)`).
     - `AdwComboRow`: dGPU Mode (`Integrated Only (PCIe Removed)`, `Hybrid Dynamic (Auto)`, `Forced On`).
     - `AdwSwitchRow`: Detach dGPU at boot before GNOME starts.

### 8.2 `ksni` System Tray Menu (`src/tray.rs`)
- **Header Label**: `Mithrandir — <Active Preset> | <Scheduler> | dGPU: <State>`
- **Submenu: Power & Scheduler Preset** (Radio buttons for the 4 Unified Presets)
- **Submenu: Hybrid GPU Mode** (`Integrated Only (PCIe Off)`, `Hybrid Dynamic`, `Forced On`)
- **Quick Action**: `Clean Safe Caches Now`
- **Standard Actions**: `Open Mithrandir`, `Quit`

---

## 9. Verification & Automated Test Suite Requirements

All filesystem/sysfs interactions must accept a configurable base root path (`&Path`) so `cargo test` runs completely unprivileged using `tempfile::TempDir`:

1. **`tests/flatpak_cleaner_test.rs`**:
   - Construct mock `~/.var/app/org.gnome.Builder/{cache,config,data,.local/state}` populated with dummy files.
   - Run `clean_flatpak_app_cache`.
   - Assert `cache/` is empty while `config/`, `data/`, and `.local/state/` files retain identical SHA-256 checksums.
2. **`tests/kernel_sysfs_test.rs`**:
   - Construct mock `/proc/sys/kernel/sched_bore` and `/sys/devices/system/cpu/cpu0/cpufreq/`.
   - Verify parser reads and writes BORE (`0`/`1`), EPP strings, and `intel_pstate/no_turbo` transitions accurately, and returns `Ok(Capability::Unsupported)` when files are absent.
3. **`tests/gpu_power_test.rs`**:
   - Construct mock `/sys/bus/pci/devices/0000:00:02.0` (`0x8086` Intel iGPU) and `0000:01:00.0` + `0000:01:00.1` (`0x10de` NVIDIA dGPU + Audio).
   - Assert topology scanner targets `0000:01:00.0` and `0000:01:00.1` together and rejects any attempt to detach `0000:00:02.0`.

---

## 10. Implementation Execution Order (Phase Gates for AI Coding Agent)
- **Phase 1**: Update `Cargo.toml`, application IDs, and Desktop/Polkit metadata to `Mithrandir`.
- **Phase 2**: Implement `src/cleaner/flatpak.rs` and pass `cargo test --test flatpak_cleaner_test`.
- **Phase 3**: Implement `src/kernel/bore.rs`, `src/kernel/scx.rs`, and `src/power/profiles.rs` and pass `cargo test --test kernel_sysfs_test`.
- **Phase 4**: Implement `src/power/gpu.rs`, `src/power/coordinator.rs`, and the privileged CLI entrypoint (`--apply-privileged`), then pass `cargo test --test gpu_power_test`.
- **Phase 5**: Wire the GTK4/Libadwaita UI pages and `ksni` tray menu to the backend controllers and verify `cargo build --release` compiles with zero warnings.
