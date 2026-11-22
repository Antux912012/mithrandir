# Changelog

All notable changes to **Mithrandir** are documented in this file.

## [1.4.0] - 2026-11-22

### Added
- **Plymouth Boot Splash Management & Theme Switcher** (`src/backend/plymouth.rs`, `src/ui/plymouth.rs`):
  - Discovers installed Plymouth themes across `/usr/share/plymouth/themes` and `/run/host/usr/share/plymouth/themes`, parsing `[Plymouth Theme]` INI manifests and resolving PNG preview thumbnails.
  - Cross-distro initramfs rebuild orchestration with live streaming build log console (`update-initramfs` on Debian/Ubuntu, `dracut` / `plymouth-set-default-theme -R` on Fedora/RHEL/openSUSE/Mandriva, `mkinitcpio` / `booster` on Arch/CachyOS, and read-only declarative guidance on immutable OSTree/NixOS).
  - Boot pipeline readiness audit checking `quiet`, `splash`, `GRUB_GFXPAYLOAD_LINUX=keep`, Arch `/etc/mkinitcpio.conf` `plymouth` hook, and NVIDIA KMS (`nvidia-drm.modeset=1`), with one-click GRUB flag remediation.
  - Safe bounded 6-second theme preview with guaranteed `EXIT/INT/TERM` `plymouth quit` teardown.
  - Custom theme archive importer (`.tar.gz`, `.tar.xz`, `.zip`, or folder) with unprivileged staging validation, 50 MiB size cap, symlink/traversal rejection, and `.plymouth` manifest verification.
  - Automatic `/etc/plymouth/plymouthd.conf.bak` snapshots, `ShowDelay` / `DeviceTimeout` tuning, and one-click rollback.
  - Added `--plymouth-status` and `--plymouth-apply <theme>` CLI flags and `io.github.antux912012.Mithrandir.manage-plymouth` Polkit policy action.
- **GRUB2 & Coreboot Personalisation** (`src/backend/coreboot.rs`, `src/backend/grub.rs`, `src/ui/grub.rs`):
  - GRUB theme discovery (`theme.txt`), background wallpaper (`GRUB_BACKGROUND`), 16-color palette presets (`Nord Frost`, `Matrix Terminal`, `Solarized Dark`, `Gruvbox Warm`, `High Contrast`) + custom FG/BG pickers, `.pf2` font discovery + `grub-mkfont` TTF/OTF converter, `GRUB_GFXMODE`, `GRUB_GFXPAYLOAD_LINUX`, `GRUB_TERMINAL_OUTPUT`, and `GRUB_TIMEOUT_STYLE`.
  - Coreboot firmware detection via DMI and `/sys/firmware/coreboot` (Dasharo, MrChromebox, Libreboot, System76, Star Labs), live `nvramtool` CMOS/NVRAM option editing, baseline 4:2:0 JPEG CBFS bootsplash staging command generator, and Coreboot GRUB2 payload `etc/grub.cfg` snippet generator.
- **Comprehensive Battery Diagnostic Report** (`src/hardware/battery.rs`, `src/ui/hardware/battery_view.rs`, `src/hardware/system.rs`):
  - Reports charge/discharge cycle count with wear classification, capacity wear %, AC adapter status, ETA, live/nominal/max voltage rails, power (W) & current (mA), mAh capacities using real battery voltage, thermal telemetry, charge start/stop thresholds, cell count estimate, and EC/BIOS/ACPI firmware details.
  - Added inline expandable report preview, **Copy Report**, **Export (.md)** to `~/Documents`, and `--battery-report` CLI flag.

### Fixed
- **GRUB Submenu & BLS Entry Resolution**: Fixed `parse_structured_menu_entries` to emit exact `1>0` submenu indices instead of flat indices, and added BootLoaderSpec (`/boot/loader/entries/*.conf`) parsing for Fedora/RHEL.
- **GRUB Single-Prompt Atomic Writes & Flatpak Backup Fix**: Consolidated backup, restore, and save operations into single `pkexec` invocations with atomic temp-file writes, fixing Flatpak `.bak` overwrite issues and skipping `mkconfig` when restoring only `grub.cfg.bak`.
- **GRUB Parsing, Validation & UI Performance**: Fixed `GRUB_TIMEOUT=-1` UI clamping, `export` / inline-comment / `# KEY=` parsing, duplicate active key generation, Fedora `grubby --remove-args` diffing, and eliminated periodic `which` subprocesses from the GTK main thread.
- **Immutable OSTree Detection**: Updated `is_immutable_system()` to also detect `/run/ostree-booted`.
- **Battery UI Units**: Fixed power rate unit label (`W` instead of `Wh`) and dynamic cell count display.

---

## [1.3.2] - 2026-11-20
- Hardened Flatpak & `/tmp` cache cleaner with `symlink_metadata()` checks and active session file protection.
- Improved error propagation for kernel/power/systemd operations and BTRFS snapshot validation.
- Removed AI API keys from `curl` command-line arguments and reduced background polling overhead.

## [1.3.1] - 2026-11-13
- Bundled 58 Papirus symbolic SVG icons via GResource.

## [1.3.0] - 2026-11-13
- Added distro-aware GRUB strategy (Debian drop-ins, Fedora BLS + `grubby`, Arch/openSUSE/Gentoo direct atomic updates) and Flatpak `/run/host` path translation.
