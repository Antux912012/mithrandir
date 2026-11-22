# Building Plan: Plymouth Boot Splash Management & Theme Switcher for Mithrandir

**Target Version**: Mithrandir v1.4.0  
**Status**: Implementation Blueprint & Engineering Roadmap  
**Target Files**:
- `src/backend/plymouth.rs` (Plymouth detection, theme catalog, validation, preview, initramfs rebuild orchestration, rollback)
- `src/backend/mod.rs` & `src/lib.rs` (Re-exports)
- `src/backend/grub.rs` (Shared `quiet splash` & `GRUB_GFXPAYLOAD_LINUX=keep` readiness helpers)
- `src/ui/plymouth.rs` (Libadwaita Plymouth Manager page with theme gallery, streaming initramfs build console, live preview)
- `src/ui/window.rs` (Sidebar navigation integration under `SYSTEM & RECOVERY` or `Boot & Plymouth` sub-view)
- `src/main.rs` (`--plymouth-status` / `--plymouth-apply` CLI entrypoints)
- `data/io.github.antux912012.Mithrandir.policy` (Polkit action for Plymouth & initramfs updates)
- `tests/plymouth_tests.rs` (Unit tests for `.plymouth` parsing, distro initramfs planning, `mkinitcpio` hook verification, archive sanitization)

---

## 1. Executive Summary & Objective

The objective is to equip **Mithrandir** with a distro-aware, safe **Plymouth Boot Splash Manager** that complements the GRUB & Coreboot personalisation suite and delivers a seamless graphical boot pipeline from firmware (UEFI BGRT / Coreboot bootsplash) through GRUB2 into the Linux display manager.

### Core Capabilities
1. **Boot Splash Telemetry & Readiness Audit**:
   - Detect installed Plymouth binary (`plymouth`, `plymouthd`, `plymouth-set-default-theme`), active theme (from `/etc/plymouth/plymouthd.conf`, `/usr/share/plymouth/plymouthd.defaults`, and Debian `update-alternatives`), and initramfs generator.
   - Verify boot-pipeline prerequisites:
     - Kernel command line contains `quiet` and `splash` (cross-checked via `GrubManager::read_config()`).
     - `GRUB_GFXPAYLOAD_LINUX=keep` is active for flicker-free framebuffer handoff.
     - On Arch / CachyOS: verify `plymouth` hook exists in `/etc/mkinitcpio.conf` (`HOOKS=(...)`) or `booster` config.
     - On hybrid NVIDIA laptops: verify `nvidia-drm.modeset=1` is present so Plymouth renders at native KMS resolution instead of falling back to text dots.
2. **Theme Discovery & Switching**:
   - Scan `/usr/share/plymouth/themes/*/*.plymouth` (and `/run/host/usr/share/plymouth/themes` inside Flatpak).
   - Parse each `.plymouth` INI manifest (`Name`, `Description`, `ModuleName`: `bgrt`, `two-step`, `script`, `fade-throbber`, `tribar`, `text`, `details`, plus `ImageDir` and preview thumbnail candidate).
   - Switch active theme atomically and rebuild the initramfs using the host distribution's native tool, streaming live build logs into the Libadwaita UI via `HostBridge::spawn_streaming`.
3. **Safe Bounded Theme Preview**:
   - Preview any installed theme without rebooting, using a strict watchdog timeout (default 6 seconds) and automatic `plymouth quit` teardown guarantee so a preview can never leave the user stuck on a blank VT.
4. **Custom Theme Import & Validation**:
   - Import third-party Plymouth theme directories or `.tar.gz`/`.tar.xz`/`.zip` archives into `/usr/share/plymouth/themes/<name>/` after strict path-traversal and `.plymouth` manifest validation.
5. **Configuration Tuning & Disaster Recovery**:
   - Tune `/etc/plymouth/plymouthd.conf` (`Theme`, `ShowDelay`, `DeviceTimeout`) with automatic `.bak` snapshots and one-click rollback to the previously active theme.

---

## 2. Architectural Blueprint

```text
+---------------------------------------------------------------------------------------------------+
|                                  PLYMOUTH MANAGER PAGE (GTK4 / Adw)                               |
|  [Readiness Banner]  [Active Theme & Config Group]  [Installed Themes Grid/List]                  |
|  [Live Preview (6s)] [Import Theme Archive]  [Streaming Initramfs Log Console]  [Revert Backup]   |
+---------------------------------------------------------------------------------------------------+
                                                 |
                                                 v
+---------------------------------------------------------------------------------------------------+
|                                   PLYMOUTH BACKEND SUBSYSTEM                                      |
|                                    (src/backend/plymouth.rs)                                      |
+---------------------------------------------------------------------------------------------------+
| +--------------------------------------+    +---------------------------------------------------+ |
| |  DISCOVERY & MANIFEST PARSER         |    |  CROSS-DISTRO APPLY & INITRAMFS ORCHESTRATOR      | |
| |  - scan_themes()                     |    |  - detect_initramfs_strategy(DistroFamily)        | |
| |  - parse_plymouth_manifest()         |    |  - check_mkinitcpio_plymouth_hook()               | |
| |  - read_plymouthd_conf()             |    |  - check_grub_splash_readiness()                  | |
| |  - resolve_preview_thumbnail()       |    |  - apply_theme_streaming(theme, delay, on_line)   | |
| +--------------------------------------+    +---------------------------------------------------+ |
|                                                |                                                  |
|                      +-------------------------+-------------------------+                        |
|                      v                                                   v                        |
|   +------------------------------------+             +-----------------------------------------+  |
|   |  SAFE PREVIEW SANDBOX CONTROLLER   |             |  ARCHIVE VALIDATOR & ROLLBACK GUARD     |  |
|   |  - Prefer plymouth-x11 if present  |             |  - Reject "..", absolute paths, symlinks|  |
|   |  - Bounded 6s watchdog + trap quit |             |  - Verify .plymouth [Plymouth Theme]    |  |
|   |  - Never runs on headless/SSH      |             |  - Snapshot /etc/plymouth/plymouthd.bak |  |
|   +------------------------------------+             +-----------------------------------------+  |
+---------------------------------------------------------------------------------------------------+
                                                 |
                                                 v
+---------------------------------------------------------------------------------------------------+
|                                   HOST BRIDGE & POLKIT LAYER                                      |
|                      (src/backend/host_bridge.rs — single pkexec / flatpak-spawn)                 |
+---------------------------------------------------------------------------------------------------+
     |                      |                         |                          |
     v                      v                         v                          v
[Debian / Ubuntu]     [Fedora / RHEL / SUSE]    [Arch / CachyOS]          [Immutable OSTree/Nix]
update-alternatives   plymouth-set-default-     plymouth-set-default-     Read-only status +
+ update-initramfs    theme -R <theme>          theme <theme> +           declarative guidance
                      (dracut)                  mkinitcpio -P / booster   (rpm-ostree initramfs)
```

---

## 3. Cross-Distro Initramfs & Theme Switching Matrix

| Distro Family | Theme Setter | Initramfs Rebuild Command | Special Readiness Checks |
| :--- | :--- | :--- | :--- |
| **Debian / Ubuntu / Mint / Pop** | `plymouth-set-default-theme <name>` (fallback: `update-alternatives --set default.plymouth <path>`) | `update-initramfs -u` | Check `/etc/initramfs-tools/conf.d/splash` (`FRAMEBUFFER=y` if needed) and `plymouth-themes` package |
| **Fedora / RHEL / Rocky / Alma / OpenMandriva** | `plymouth-set-default-theme -R <name>` | Built into `-R` (invokes `dracut --regenerate-all --force`) | Verify BLS cmdline includes `rhgb quiet` or `quiet splash` |
| **Arch / CachyOS / EndeavourOS / Manjaro** | `plymouth-set-default-theme <name>` (or write `/etc/plymouth/plymouthd.conf`) | `mkinitcpio -P` (or `dracut-rebuild` / `booster build`) | Parse `/etc/mkinitcpio.conf` `HOOKS=(...)`: warn if `plymouth` hook is absent (and offer guided insertion after `base udev` / `systemd`) |
| **openSUSE** | `plymouth-set-default-theme -R <name>` | `dracut --force` | Verify `splash=silent quiet` in `GRUB_CMDLINE_LINUX_DEFAULT` |
| **Immutable (NixOS / OSTree / Kepler OS / Silverblue)** | **Disabled (Read-Only)** | Declarative only (`rpm-ostree initramfs --enable` or `boot.plymouth.theme`) | Present exact declarative snippet to copy instead of mutating `/usr` |

---

## 4. Core Rust Data Structures (`src/backend/plymouth.rs`)

```rust
use serde::{Deserialize, Serialize};
use crate::backend::distro::DistroFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InitramfsBackend {
    UpdateInitramfs,   // Debian / Ubuntu
    Dracut,            // Fedora / RHEL / openSUSE / OpenMandriva
    Mkinitcpio,        // Arch / CachyOS
    Booster,           // Arch / CachyOS alternative
    ImmutableReadOnly, // NixOS / OSTree / Kepler OS
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlymouthTheme {
    pub id: String,                  // Directory name, e.g. "bgrt", "spinner", "nord-arch"
    pub name: String,                // Display Name from [Plymouth Theme]
    pub description: String,         // Description from manifest
    pub module_name: String,         // "two-step", "script", "bgrt", "text", "details", etc.
    pub manifest_path: String,       // "/usr/share/plymouth/themes/<id>/<id>.plymouth"
    pub preview_image: Option<String>, // Optional logo/watermark PNG for card thumbnail
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlymouthReadiness {
    pub plymouth_installed: bool,
    pub grub_has_quiet: bool,
    pub grub_has_splash: bool,
    pub grub_gfxpayload_keep: bool,
    pub mkinitcpio_has_plymouth_hook: Option<bool>, // Some(...) on Arch/CachyOS only
    pub nvidia_kms_recommended: bool,               // True if NVIDIA dGPU present without nvidia-drm.modeset=1
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlymouthConfig {
    pub active_theme: String,
    pub show_delay_secs: u32,
    pub device_timeout_secs: u32,
    pub conf_path: String,
    pub backup_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlymouthStatus {
    pub family: DistroFamily,
    pub initramfs_backend: InitramfsBackend,
    pub config: PlymouthConfig,
    pub readiness: PlymouthReadiness,
    pub themes: Vec<PlymouthTheme>,
    pub preview_supported: bool,
}
```

### 4.1 Key Backend Functions
1. `PlymouthManager::parse_manifest(content: &str, id: &str, manifest_path: &str) -> Option<PlymouthTheme>`
   - Parses `[Plymouth Theme]` INI section (`Name=`, `Description=`, `ModuleName=`) and resolves `[two-step]` / `[script]` `ImageDir=` to locate a candidate preview PNG (e.g., `watermark.png`, `logo.png`, `header-image.png`, `animation-0001.png`).
2. `PlymouthManager::parse_plymouthd_conf(content: &str) -> (Option<String>, u32, u32)`
   - Reads `[Daemon]` keys `Theme`, `ShowDelay`, and `DeviceTimeout` cleanly.
3. `PlymouthManager::check_mkinitcpio_hook(conf_content: &str) -> bool`
   - Parses active `HOOKS=(...)` line in `/etc/mkinitcpio.conf` and verifies `plymouth` appears before `encrypt`/`sd-encrypt` and after `base udev`/`systemd`.
4. `PlymouthManager::apply_theme_streaming<F>(theme_id: &str, show_delay: u32, device_timeout: u32, on_line: F) -> Result<(), String>`
   - Validates `theme_id` (`[A-Za-z0-9._-]+`, exists in discovered themes).
   - Executes a **single** `pkexec sh -e -c ...` script via `HostBridge::spawn_streaming` that:
     1. Snapshots `/etc/plymouth/plymouthd.conf` to `/etc/plymouth/plymouthd.conf.bak` (if missing).
     2. Writes updated `/etc/plymouth/plymouthd.conf` atomically via temp file + `mv`.
     3. Invokes `plymouth-set-default-theme` / `update-alternatives` as appropriate for the distro.
     4. Rebuilds initramfs (`update-initramfs -u`, `dracut --regenerate-all --force`, `mkinitcpio -P`, or `booster build`) while streaming every output line live to the UI.
5. `PlymouthManager::preview_theme(theme_id: &str, duration_secs: u32) -> Result<(), String>`
   - Clamps `duration_secs` to `3..=15`.
   - Uses a `trap 'plymouth quit --retain-splash 2>/dev/null || plymouth quit 2>/dev/null || true' EXIT INT TERM` guard so termination is mathematically guaranteed even if cancelled or errored.
   - Prefers `plymouth-x11` windowed preview when available; otherwise displays a clear confirmation dialog explaining that fullscreen VT preview will briefly take over the display for N seconds and auto-return.
6. `PlymouthManager::import_theme_archive(archive_path: &Path) -> Result<String, String>`
   - Extracts into a temporary staging directory first (unprivileged), validates:
     - No `..` path components, no absolute paths, no symlinks escaping the theme root.
     - Contains exactly one valid `<name>.plymouth` file with `[Plymouth Theme]` and valid `ModuleName`.
     - Total uncompressed size cap (e.g., `<= 50 MiB`) to prevent zip/tar bombs.
   - Moves the verified theme folder into `/usr/share/plymouth/themes/<name>` via a single `pkexec` call (`chmod -R a+rX`).
7. `PlymouthManager::enable_grub_splash_flags() -> Result<(), String>`
   - Reuses `GrubManager::read_config()` and `GrubManager::save_and_update_with_personalisation(...)` to append `quiet splash` to `GRUB_CMDLINE_LINUX_DEFAULT` and set `GRUB_GFXPAYLOAD_LINUX=keep` without duplicating existing flags.

---

## 5. Libadwaita UI Design (`src/ui/plymouth.rs`)

### Layout Structure (inside `adw::Clamp`, max width `880px`)
1. **Readiness & Pipeline Banner** (`adw::Banner` / `PreferencesGroup`):
   - Shows green status when Plymouth + `quiet splash` + `gfxpayload=keep` + initramfs hooks are all aligned.
   - Shows actionable inline Fix button (e.g. **"Enable `quiet splash` in GRUB"**) if GRUB kernel flags are missing.
   - Displays read-only notice on immutable OS (NixOS / OSTree / Kepler OS) with copyable declarative config.
2. **Active Splash & Daemon Settings** (`PreferencesGroup`):
   - Active theme badge, initramfs backend label (`update-initramfs` / `dracut` / `mkinitcpio`), and config backup status.
   - `ShowDelay` `SpinButton` (0–30 s, default 0 for instant boot splash).
   - `DeviceTimeout` `SpinButton` (5–30 s, default 8).
   - **"Revert to Previous Theme"** destructive button (enabled when `/etc/plymouth/plymouthd.conf.bak` exists).
3. **Installed Plymouth Themes** (`PreferencesGroup`):
   - List/cards of all discovered themes with:
     - Thumbnail preview (loaded via `gtk4::Picture` if a PNG asset is found, fallback to `preferences-desktop-wallpaper-symbolic`).
     - Theme name, description, and module pill (`bgrt`, `two-step`, `script`, `text`).
     - Active indicator badge on current theme.
     - **"Preview (6s)"** flat button and **"Apply & Rebuild Initramfs"** suggested-action button.
4. **Custom Theme Import** (`ActionRow` with GTK4 `FileDialog`):
   - Accepts `.tar.gz`, `.tar.xz`, `.zip`, or theme folder; runs unprivileged staging validation before prompting `pkexec`.
5. **Live Initramfs Rebuild Console** (`ExpanderRow` + `gtk4::TextView` monospace):
   - Auto-expands while `update-initramfs` / `dracut` / `mkinitcpio` is running.
   - Streams real-time lines from `HostBridge::spawn_streaming` so the user sees live progress (initramfs rebuilds take 10–45 seconds, so visual feedback is essential).
   - Disables apply/revert buttons while an initramfs build is in flight to prevent concurrent initramfs corruption.

---

## 6. Security, Polkit & Flatpak Sandbox Considerations

1. **Single-Prompt Privilege Elevation**:
   - All privileged theme switches pass paths and theme IDs via positional shell arguments (`sh -c '...' -- "$theme" ...`) — never string interpolation.
2. **Polkit Policy (`data/io.github.antux912012.Mithrandir.policy`)**:
   - Add action `io.github.antux912012.Mithrandir.manage-plymouth` (`auth_admin_keep`) alongside existing kernel/power actions.
3. **Flatpak Host Bridge**:
   - Read themes via `/run/host/usr/share/plymouth/themes` and `/run/host/etc/plymouth/plymouthd.conf`.
   - Execute `plymouth-set-default-theme`, `update-initramfs`, `dracut`, and `mkinitcpio` through `HostBridge::spawn_streaming(..., privileged = true)` (`flatpak-spawn --host pkexec ...`).
4. **Preventing Unbootable Initramfs**:
   - Always validate that the chosen `.plymouth` manifest and its referenced `ImageDir` exist before regenerating initramfs.
   - Never invoke `mkinitcpio` or `dracut` with `--no-hostonly` overrides that break existing LUKS/BTRFS hooks.
   - Keep `/etc/plymouth/plymouthd.conf.bak` intact on first edit so one-click revert is always available.

---

## 7. Phased Implementation Roadmap

### Phase 1 — Backend Discovery & Pure Parsers (2–3 days)
- [ ] Create `src/backend/plymouth.rs` with `PlymouthTheme`, `PlymouthConfig`, `PlymouthReadiness`, `PlymouthStatus`, `InitramfsBackend`.
- [ ] Implement `.plymouth` INI parser, `plymouthd.conf` parser/updater, `/etc/mkinitcpio.conf` `HOOKS` parser, and GRUB `quiet splash` cross-check.
- [ ] Implement distro initramfs backend selector (`select_initramfs_backend`).
- [ ] Add comprehensive unit tests in `tests/plymouth_tests.rs` using `tempfile` mock directories.

### Phase 2 — Privileged Apply, Initramfs Streaming & Rollback (2 days)
- [ ] Implement single-prompt `apply_theme_streaming` using `HostBridge::spawn_streaming`.
- [ ] Implement `revert_to_backup` and `enable_grub_splash_flags` helper.
- [ ] Add CLI flags `--plymouth-status` and `--plymouth-apply <theme>` in `src/main.rs`.

### Phase 3 — Safe Bounded Preview & Archive Importer (2 days)
- [ ] Implement `preview_theme` with `plymouth-x11` preference + guaranteed `EXIT/INT/TERM` watchdog teardown.
- [ ] Implement `import_theme_archive` with unprivileged staging, symlink/traversal rejection, size cap, and manifest validation.

### Phase 4 — Libadwaita UI Integration (2–3 days)
- [ ] Create `src/ui/plymouth.rs` with Readiness Banner, Daemon Settings, Theme Gallery with thumbnail previews, Import dialog, and live streaming Initramfs Log Console.
- [ ] Register page in `src/ui/window.rs` (either as a dedicated `Plymouth Splash` entry directly below `GRUB & Coreboot` under `SYSTEM & RECOVERY`, or as an `adw::ViewSwitcher` tab inside a unified `Boot & Firmware` page alongside GRUB & Coreboot).
- [ ] Add tab visibility toggle to `PreferencesDialog` (`src/ui/preferences.rs`).

### Phase 5 — Verification & Packaging (1–2 days)
- [ ] Test initramfs rebuilds in Debian/Ubuntu, Fedora, and Arch/CachyOS VMs/containers.
- [ ] Test Flatpak host-bridge path translation (`/run/host/usr/share/plymouth/themes`).
- [ ] Update `data/io.github.antux912012.Mithrandir.policy`, `README.md`, and `ARCHITECTURE.md`.

---

## 8. Unit Test Plan (`tests/plymouth_tests.rs`)

| Test Case | What It Verifies |
| :--- | :--- |
| `test_parse_plymouth_manifest_bgrt_and_script` | Parses `Name`, `Description`, `ModuleName`, `ImageDir`, and preview asset discovery from mock `.plymouth` files |
| `test_parse_and_update_plymouthd_conf` | Reads and updates `Theme`, `ShowDelay`, and `DeviceTimeout` while preserving comments and other INI sections |
| `test_mkinitcpio_plymouth_hook_detection` | Detects `plymouth` in `HOOKS=(base udev plymouth ...)` and ignores commented `#HOOKS=(...)` lines |
| `test_initramfs_backend_selection_across_distros` | Verifies Debian→`UpdateInitramfs`, Fedora/SUSE→`Dracut`, Arch/CachyOS→`Mkinitcpio`/`Booster`, NixOS/OSTree→`ImmutableReadOnly` |
| `test_theme_id_and_archive_path_validation` | Rejects `../evil`, `/etc/passwd`, shell metacharacters, and missing `[Plymouth Theme]` headers |
| `test_grub_splash_readiness_audit` | Detects missing `quiet`, missing `splash`, and missing `GRUB_GFXPAYLOAD_LINUX=keep` accurately |
