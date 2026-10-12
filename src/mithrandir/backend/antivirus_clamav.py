import datetime
import hashlib
import json
import os
import re
import shutil
import threading
import time
import uuid
from dataclasses import dataclass, field, asdict
from typing import Callable, Dict, List, Optional, Tuple

from .host_bridge import HostBridge

RANSOMWARE_REGEX = re.compile(
    r"(ransom|cryptor|wannacry|locky|cerber|ryuk|petya|conti|stop|sodinokibi|blackcat|filecoder|revil|maze|dharma|phobos|hive|babuk)",
    re.IGNORECASE
)

FOUND_REGEX = re.compile(r"^(.+):\s+([^\s]+)\s+FOUND$")


@dataclass
class ThreatFinding:
    threat_id: str
    threat_name: str
    file_path: str
    file_size_bytes: int
    detected_at: str
    is_ransomware: bool
    status: str = "detected"  # 'detected', 'quarantined', 'deleted', 'ignored'


@dataclass
class DefinitionsStatus:
    installed: bool
    version: str
    release_date: Optional[str]
    age_hours: float
    total_signatures: int
    status_label: str  # 'Up to Date', 'Outdated', 'Missing', 'Unknown'
    is_stale: bool


@dataclass
class ScanReport:
    profile: str
    files_scanned: int
    threats_found: List[ThreatFinding] = field(default_factory=list)
    started_at: str = ""
    finished_at: str = ""
    duration_seconds: float = 0.0
    was_cancelled: bool = False
    error_message: str = ""


class QuarantineVault:
    """
    Manages isolated and neutralized files detected as malware or ransomware.
    Files are moved into an isolated directory (mode 0700), stripped of permissions
    (mode 0000), renamed to <id>.quarantined, and tracked with a JSON manifest.
    """

    def __init__(self, custom_path: Optional[str] = None):
        if custom_path and custom_path.strip():
            self.vault_dir = os.path.expanduser(custom_path.strip())
        else:
            self.vault_dir = os.path.expanduser("~/.local/share/io.github.antonio.Mithrandir/quarantine")
        self._ensure_vault()

    def _ensure_vault(self):
        try:
            if not os.path.exists(self.vault_dir):
                os.makedirs(self.vault_dir, mode=0o700, exist_ok=True)
            else:
                os.chmod(self.vault_dir, 0o700)
        except Exception:
            pass

    def quarantine_file(self, threat: ThreatFinding) -> Tuple[bool, str]:
        """
        Quarantines a file: calculates SHA256, moves it to vault, strips permissions,
        and saves metadata.
        """
        self._ensure_vault()
        source_path = threat.file_path

        if not os.path.exists(source_path):
            return False, f"File no longer exists: {source_path}"

        try:
            sha256_hash = hashlib.sha256()
            try:
                with open(source_path, "rb") as f:
                    for chunk in iter(lambda: f.read(65536), b""):
                        sha256_hash.update(chunk)
                file_hash = sha256_hash.hexdigest()
            except Exception:
                file_hash = "unknown"

            dest_filename = f"{threat.threat_id}.quarantined"
            dest_meta = f"{threat.threat_id}.json"
            dest_path = os.path.join(self.vault_dir, dest_filename)
            meta_path = os.path.join(self.vault_dir, dest_meta)

            shutil.move(source_path, dest_path)
            # Remove all read, write, and execute permissions
            os.chmod(dest_path, 0o000)

            metadata = {
                "threat_id": threat.threat_id,
                "threat_name": threat.threat_name,
                "original_path": source_path,
                "file_size_bytes": threat.file_size_bytes,
                "detected_at": threat.detected_at,
                "quarantined_at": datetime.datetime.now().isoformat(),
                "is_ransomware": threat.is_ransomware,
                "sha256": file_hash,
                "vault_file": dest_filename,
            }

            with open(meta_path, "w", encoding="utf-8") as f:
                json.dump(metadata, f, indent=2)

            threat.status = "quarantined"
            return True, "File safely quarantined."
        except Exception as e:
            return False, f"Quarantine failed: {str(e)}"

    def restore_file(self, threat_id: str) -> Tuple[bool, str]:
        """Restores a quarantined file back to its original path."""
        meta_path = os.path.join(self.vault_dir, f"{threat_id}.json")
        vault_file = os.path.join(self.vault_dir, f"{threat_id}.quarantined")

        if not os.path.exists(meta_path) or not os.path.exists(vault_file):
            return False, "Quarantined record not found."

        try:
            with open(meta_path, "r", encoding="utf-8") as f:
                metadata = json.load(f)

            orig_path = metadata.get("original_path")
            if not orig_path:
                return False, "Original path missing from metadata."

            orig_dir = os.path.dirname(orig_path)
            if not os.path.exists(orig_dir):
                os.makedirs(orig_dir, exist_ok=True)

            # Restore read/write permissions so file can be moved
            os.chmod(vault_file, 0o644)
            shutil.move(vault_file, orig_path)

            os.remove(meta_path)
            return True, f"File restored to {orig_path}"
        except Exception as e:
            return False, f"Restore failed: {str(e)}"

    def delete_permanently(self, threat_id: str) -> Tuple[bool, str]:
        """Permanently deletes a quarantined file and its metadata."""
        meta_path = os.path.join(self.vault_dir, f"{threat_id}.json")
        vault_file = os.path.join(self.vault_dir, f"{threat_id}.quarantined")

        deleted = False
        if os.path.exists(vault_file):
            try:
                os.chmod(vault_file, 0o600)
                os.remove(vault_file)
                deleted = True
            except Exception as e:
                return False, f"Failed to delete quarantined file: {str(e)}"

        if os.path.exists(meta_path):
            try:
                os.remove(meta_path)
            except Exception:
                pass

        return True, "Quarantined file permanently deleted."

    def list_quarantined(self) -> List[dict]:
        """Returns all quarantined items recorded in the vault."""
        self._ensure_vault()
        items = []
        if not os.path.exists(self.vault_dir):
            return items

        for fname in os.listdir(self.vault_dir):
            if fname.endswith(".json"):
                try:
                    fpath = os.path.join(self.vault_dir, fname)
                    with open(fpath, "r", encoding="utf-8") as f:
                        items.append(json.load(f))
                except Exception:
                    continue

        items.sort(key=lambda x: x.get("quarantined_at", ""), reverse=True)
        return items


class ClamAVEngine:
    """
    Orchestrates ClamAV scanning, signature updates via freshclam, and threat detection.
    Supports manual updates and automatic pre-scan update hooks.
    """

    CVD_DIRS = [
        "/var/lib/clamav",
        "/var/clamav",
        os.path.expanduser("~/.local/share/clamav"),
        os.path.expanduser("~/.clamav"),
    ]

    @classmethod
    def check_installed(cls) -> Dict[str, bool]:
        """Checks availability of ClamAV binaries."""
        return {
            "clamscan": HostBridge.which("clamscan") is not None,
            "clamdscan": HostBridge.which("clamdscan") is not None,
            "freshclam": HostBridge.which("freshclam") is not None,
            "sigtool": HostBridge.which("sigtool") is not None,
        }

    @classmethod
    def is_daemon_running(cls) -> bool:
        """Checks if clamav-daemon is active."""
        code, out, _ = HostBridge.run_command(["systemctl", "is-active", "clamav-daemon"], timeout=5)
        return code == 0 and out.strip() == "active"

    @classmethod
    def parse_cvd_header(cls, file_path: str) -> Optional[Dict]:
        """
        Parses ClamAV Virus Database (.cvd / .cld) header.
        First 512 bytes contains: ClamAV-VDB:build_time:version:sigs:type:md5:dsig:...
        """
        try:
            if not os.path.exists(file_path):
                return None
            with open(file_path, "rb") as f:
                header = f.read(512).decode("latin-1", errors="ignore")
            if header.startswith("ClamAV-VDB:"):
                parts = header.split(":")
                if len(parts) >= 4:
                    build_time_str = parts[1]
                    version_str = parts[2]
                    sigs_str = parts[3]
                    return {
                        "build_time": build_time_str,
                        "version": version_str,
                        "signatures": int(sigs_str) if sigs_str.isdigit() else 0,
                    }
        except Exception:
            pass
        return None

    @classmethod
    def get_definitions_info(cls, max_age_hours: int = 24) -> DefinitionsStatus:
        """
        Inspects ClamAV database files to determine freshness, signature count, and age.
        """
        candidate_files = []
        for d in cls.CVD_DIRS:
            for fname in ["daily.cld", "daily.cvd", "main.cld", "main.cvd"]:
                p = os.path.join(d, fname)
                if os.path.exists(p):
                    candidate_files.append(p)

        if not candidate_files:
            return DefinitionsStatus(
                installed=False,
                version="None",
                release_date=None,
                age_hours=999999.0,
                total_signatures=0,
                status_label="Missing",
                is_stale=True,
            )

        # Use the most recently modified database file
        candidate_files.sort(key=lambda p: os.path.getmtime(p), reverse=True)
        latest_file = candidate_files[0]
        mtime = os.path.getmtime(latest_file)
        age_seconds = max(0, time.time() - mtime)
        age_hours = age_seconds / 3600.0

        header_data = cls.parse_cvd_header(latest_file)
        version = header_data.get("version", "Installed") if header_data else "Installed"
        total_sigs = header_data.get("signatures", 0) if header_data else 0

        # Try to sum signatures across main and daily if available
        if header_data:
            for extra in candidate_files[1:]:
                eh = cls.parse_cvd_header(extra)
                if eh and eh.get("signatures"):
                    total_sigs += eh["signatures"]

        rel_date = datetime.datetime.fromtimestamp(mtime).strftime("%Y-%m-%d %H:%M")
        is_stale = age_hours > max_age_hours

        if is_stale:
            status_label = f"Outdated ({int(age_hours / 24)} days old)" if age_hours >= 48 else f"Outdated ({int(age_hours)}h old)"
        else:
            status_label = "Up to Date"

        return DefinitionsStatus(
            installed=True,
            version=version,
            release_date=rel_date,
            age_hours=age_hours,
            total_signatures=total_sigs,
            status_label=status_label,
            is_stale=is_stale,
        )

    @classmethod
    def update_definitions(
        cls,
        callback_progress: Optional[Callable[[str], None]] = None,
        cancel_event: Optional[threading.Event] = None
    ) -> Tuple[bool, str]:
        """
        Executes `freshclam` to synchronize the virus & ransomware signature definitions.
        Handles daemon lock conflicts and extracts download progress.
        """
        installed = cls.check_installed()
        if not installed.get("freshclam"):
            return False, "freshclam binary not found. Please install clamav-freshclam."

        if callback_progress:
            callback_progress("Contacting ClamAV update mirrors...")

        # If freshclam systemd service is active, freshclam command may fail with lock error.
        # Check if clamav-freshclam service is running
        code, out, _ = HostBridge.run_command(["systemctl", "is-active", "clamav-freshclam"], timeout=3)
        if code == 0 and out.strip() == "active":
            # We can trigger an update by restarting clamav-freshclam or using freshclam directly
            if callback_progress:
                callback_progress("Synchronizing with clamav-freshclam daemon...")

        output_log = []

        def line_cb(line: str):
            output_log.append(line)
            if callback_progress:
                if "Downloading" in line:
                    callback_progress(line.strip())
                elif "Database updated" in line:
                    callback_progress(line.strip())
                elif "is up to date" in line:
                    callback_progress(line.strip())

        code, out = HostBridge.run_streaming_command(
            ["freshclam"],
            line_callback=line_cb,
            cancel_event=cancel_event,
            use_pkexec=False,  # If run as user, freshclam can update user DB or use system fallback
        )

        # If permission denied to /var/log/clamav or /var/lib/clamav, retry with pkexec if needed
        if code != 0 and ("Permission denied" in out or "locked" in out.lower() and "try again" in out.lower()):
            if "Permission denied" in out:
                if callback_progress:
                    callback_progress("Requesting elevated privileges to update system definitions...")
                code, out = HostBridge.run_streaming_command(
                    ["freshclam"],
                    line_callback=line_cb,
                    cancel_event=cancel_event,
                    use_pkexec=True,
                )

        full_output = "\n".join(output_log) if output_log else out

        # Check for success indicators
        if code == 0 or "is up to date" in full_output or "Database updated" in full_output:
            return True, "Virus definitions successfully updated."

        if "Can't connect to port" in full_output or "Network is unreachable" in full_output:
            return False, "Network unreachable: unable to contact ClamAV database mirrors."

        return False, f"freshclam exited with code {code}: {full_output.strip()[-200:]}"

    @classmethod
    def ensure_definitions_updated(
        cls,
        settings=None,
        callback_status: Optional[Callable[[str], None]] = None,
        cancel_event: Optional[threading.Event] = None
    ) -> Tuple[bool, str]:
        """
        Pre-scan hook: If auto-update is enabled in settings and definitions are stale or missing,
        triggers an automatic freshclam update before proceeding.
        Gracefully falls back to existing definitions if offline.
        """
        auto_update = True
        max_age_hours = 24

        if settings:
            try:
                auto_update = settings.get_boolean("antivirus-auto-update-before-scan")
                max_age_hours = settings.get_int("antivirus-definitions-max-age-hours")
            except Exception:
                pass

        if not auto_update:
            return True, "Auto-update disabled by user settings; continuing with existing definitions."

        def_info = cls.get_definitions_info(max_age_hours=max_age_hours)
        if not def_info.is_stale and def_info.installed:
            return True, "Virus definitions are current."

        if callback_status:
            callback_status("Definitions are outdated. Automatically updating before scan...")

        ok, msg = cls.update_definitions(callback_progress=callback_status, cancel_event=cancel_event)
        if ok and settings:
            try:
                settings.set_int64("antivirus-last-update-timestamp", int(time.time()))
            except Exception:
                pass
            return True, msg

        # If update failed due to network / offline, allow scan with cached definitions if available
        if def_info.installed:
            if callback_status:
                callback_status("Offline or update error; proceeding with cached definitions.")
            return True, f"Proceeding with existing cached definitions ({msg})"

        return False, f"Definitions missing and update failed: {msg}"

    @classmethod
    def get_quick_scan_targets(cls) -> List[str]:
        """Returns standard high-risk volatile paths for Quick Scan."""
        candidates = [
            os.path.expanduser("~/Downloads"),
            os.path.expanduser("~/.cache"),
            "/tmp",
            "/var/tmp",
            os.path.expanduser("~/.config/autostart"),
            "/etc/xdg/autostart",
        ]
        return [p for p in candidates if os.path.exists(p)]

    @classmethod
    def scan(
        cls,
        profile: str = "quick",
        custom_path: Optional[str] = None,
        callback_progress: Optional[Callable[[str, int], None]] = None,
        callback_threat: Optional[Callable[[ThreatFinding], None]] = None,
        cancel_event: Optional[threading.Event] = None,
        settings=None
    ) -> ScanReport:
        """
        Executes a scan according to profile ('quick', 'full', 'custom').
        If auto-update is enabled, checks and updates definitions first.
        Parses output in real time and classifies malware / ransomware threats.
        """
        start_time = time.time()
        start_iso = datetime.datetime.now().isoformat()
        report = ScanReport(
            profile=profile,
            files_scanned=0,
            threats_found=[],
            started_at=start_iso,
        )

        installed = cls.check_installed()
        if not installed.get("clamscan") and not installed.get("clamdscan"):
            report.error_message = "clamscan binary not found. Please install clamav."
            report.finished_at = datetime.datetime.now().isoformat()
            return report

        # Pre-scan definition check & auto-update hook
        def status_cb(msg: str):
            if callback_progress:
                callback_progress(msg, 0)

        ok, update_msg = cls.ensure_definitions_updated(
            settings=settings,
            callback_status=status_cb,
            cancel_event=cancel_event
        )

        if cancel_event and cancel_event.is_set():
            report.was_cancelled = True
            report.finished_at = datetime.datetime.now().isoformat()
            return report

        # Build scan targets and arguments
        scan_args = ["clamscan", "-r", "--no-summary", "-i"]

        # Heuristic inspection options
        enable_heuristics = True
        nice_pri = 19
        if settings:
            try:
                enable_heuristics = settings.get_boolean("antivirus-heuristic-alerts")
                nice_pri = settings.get_int("antivirus-process-priority-nice")
            except Exception:
                pass

        if enable_heuristics:
            scan_args.extend(["--alert-encrypted=yes", "--alert-broken-media=yes"])

        target_paths = []
        if profile == "quick":
            target_paths = cls.get_quick_scan_targets()
        elif profile == "full":
            target_paths = ["/"]
            # Exclude virtual filesystems to prevent loops and kernel crashes
            scan_args.extend([
                '--exclude-dir=^/sys',
                '--exclude-dir=^/proc',
                '--exclude-dir=^/dev',
                '--exclude-dir=^/run',
                '--exclude-dir=^/sys/fs/cgroup',
                '--exclude-dir=^/media',
                '--exclude-dir=^/mnt',
            ])
        elif profile == "custom":
            if custom_path and os.path.exists(custom_path):
                target_paths = [custom_path]
            else:
                report.error_message = f"Invalid custom path: {custom_path}"
                report.finished_at = datetime.datetime.now().isoformat()
                return report

        if not target_paths:
            report.error_message = "No scan targets available."
            report.finished_at = datetime.datetime.now().isoformat()
            return report

        scan_cmd = scan_args + target_paths
        files_scanned_count = 0

        def scan_line_cb(line: str):
            nonlocal files_scanned_count
            files_scanned_count += 1

            # Match infected file pattern: "/path/to/file: ThreatName FOUND"
            match = FOUND_REGEX.match(line)
            if match:
                fpath = match.group(1).strip()
                tname = match.group(2).strip()
                is_ransom = bool(RANSOMWARE_REGEX.search(tname))

                fsize = 0
                try:
                    if os.path.exists(fpath):
                        fsize = os.path.getsize(fpath)
                except Exception:
                    pass

                finding = ThreatFinding(
                    threat_id=str(uuid.uuid4())[:8],
                    threat_name=tname,
                    file_path=fpath,
                    file_size_bytes=fsize,
                    detected_at=datetime.datetime.now().isoformat(),
                    is_ransomware=is_ransom,
                    status="detected",
                )
                report.threats_found.append(finding)
                if callback_threat:
                    callback_threat(finding)

            if callback_progress:
                # Provide real-time feedback
                current_display = line if len(line) < 80 else line[:40] + "..." + line[-35:]
                callback_progress(current_display, files_scanned_count)

        code, out = HostBridge.run_streaming_command(
            scan_cmd,
            line_callback=scan_line_cb,
            cancel_event=cancel_event,
            nice_priority=nice_pri,
            io_idle=True
        )

        report.files_scanned = files_scanned_count
        report.duration_seconds = round(time.time() - start_time, 2)
        report.finished_at = datetime.datetime.now().isoformat()

        if cancel_event and cancel_event.is_set():
            report.was_cancelled = True

        # Clamscan exit codes:
        # 0 = No virus found
        # 1 = Virus(es) found
        # 2 = Some error(s) occurred
        if code == 2 and not report.threats_found and not report.was_cancelled:
            report.error_message = f"clamscan completed with warnings or minor errors: {out[-150:]}"

        return report
