import os
import shutil
import tempfile
import unittest
from unittest.mock import MagicMock, patch

from mithrandir.backend.antivirus_clamav import (
    ClamAVEngine,
    QuarantineVault,
    ThreatFinding,
    DefinitionsStatus,
    ScanReport,
    RANSOMWARE_REGEX,
    FOUND_REGEX,
)


class TestAntivirus(unittest.TestCase):

    def setUp(self):
        self.test_dir = tempfile.mkdtemp(prefix="mithrandir_av_test_")
        self.vault_dir = os.path.join(self.test_dir, "vault")
        self.vault = QuarantineVault(custom_path=self.vault_dir)

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_ransomware_regex(self):
        """Verify ransomware regex identifies ransomware signatures accurately."""
        ransomware_signatures = [
            "Win.Ransomware.Locky-1234",
            "Win.Ransomware.WannaCry-M",
            "Linux.Cryptor.Agent",
            "Osx.Ransom.Petya",
            "Win.Ransomware.Conti-1",
            "Win.Trojan.BlackCat",
            "Win.Malware.FileCoder-99",
            "Win.Ransomware.Revil-A",
            "Win.Ransomware.Sodinokibi-4",
        ]
        for sig in ransomware_signatures:
            self.assertTrue(
                bool(RANSOMWARE_REGEX.search(sig)),
                f"Expected signature {sig} to be flagged as ransomware"
            )

        standard_malware = [
            "Eicar-Test-Signature",
            "Unix.Trojan.Mirai-1",
            "Win.Spyware.Keylogger-2",
            "Html.Exploit.CVE_2021_1234",
        ]
        for sig in standard_malware:
            self.assertFalse(
                bool(RANSOMWARE_REGEX.search(sig)),
                f"Did not expect signature {sig} to be flagged as ransomware"
            )

    def test_found_regex(self):
        """Verify clamscan streaming line regex correctly parses infected files."""
        line1 = "/home/antonio/Downloads/suspicious_invoice.exe: Win.Ransomware.Locky-123 FOUND"
        match1 = FOUND_REGEX.match(line1)
        self.assertIsNotNone(match1)
        self.assertEqual(match1.group(1), "/home/antonio/Downloads/suspicious_invoice.exe")
        self.assertEqual(match1.group(2), "Win.Ransomware.Locky-123")

        line2 = "/tmp/eicar.com: Eicar-Test-Signature FOUND"
        match2 = FOUND_REGEX.match(line2)
        self.assertIsNotNone(match2)
        self.assertEqual(match2.group(1), "/tmp/eicar.com")
        self.assertEqual(match2.group(2), "Eicar-Test-Signature")

        # Non-infected lines shouldn't match
        line_clean = "/usr/bin/python3: OK"
        self.assertIsNone(FOUND_REGEX.match(line_clean))

    def test_cvd_header_parsing(self):
        """Verify parsing ClamAV CVD database header."""
        test_cvd = os.path.join(self.test_dir, "test.cvd")
        header_data = b"ClamAV-VDB:12 Oct 2026 14-22 +0000:27142:8650230:1:abcdef:dsig" + b" " * 400
        with open(test_cvd, "wb") as f:
            f.write(header_data)

        parsed = ClamAVEngine.parse_cvd_header(test_cvd)
        self.assertIsNotNone(parsed)
        self.assertEqual(parsed["version"], "27142")
        self.assertEqual(parsed["signatures"], 8650230)

    def test_quarantine_neutralization_and_restore(self):
        """Verify file quarantine: execution revocation (0000) and restoration."""
        test_file = os.path.join(self.test_dir, "malware_sample.sh")
        with open(test_file, "w") as f:
            f.write("#!/bin/bash\necho 'malicious payload'\n")
        os.chmod(test_file, 0o755)

        finding = ThreatFinding(
            threat_id="test-threat-01",
            threat_name="Unix.Ransomware.Test-1",
            file_path=test_file,
            file_size_bytes=os.path.getsize(test_file),
            detected_at="2026-10-13T00:00:00",
            is_ransomware=True,
            status="detected",
        )

        # Quarantine file
        ok, msg = self.vault.quarantine_file(finding)
        self.assertTrue(ok)
        self.assertFalse(os.path.exists(test_file), "Original file should no longer exist")

        vault_file = os.path.join(self.vault_dir, "test-threat-01.quarantined")
        meta_file = os.path.join(self.vault_dir, "test-threat-01.json")
        self.assertTrue(os.path.exists(vault_file))
        self.assertTrue(os.path.exists(meta_file))

        # Check permissions stripped to 0000
        stat_mode = os.stat(vault_file).st_mode & 0o777
        self.assertEqual(stat_mode, 0o000, "Quarantined file must have 0000 permissions")

        # Test listing quarantined files
        items = self.vault.list_quarantined()
        self.assertEqual(len(items), 1)
        self.assertEqual(items[0]["threat_id"], "test-threat-01")
        self.assertTrue(items[0]["is_ransomware"])

        # Test restoration
        ok_res, msg_res = self.vault.restore_file("test-threat-01")
        self.assertTrue(ok_res)
        self.assertTrue(os.path.exists(test_file), "Restored file must exist at original path")
        self.assertFalse(os.path.exists(vault_file), "Vault file should be moved back")
        self.assertFalse(os.path.exists(meta_file), "Metadata should be cleaned up")

    def test_quarantine_delete_permanently(self):
        """Verify permanent deletion from quarantine vault."""
        test_file = os.path.join(self.test_dir, "trojan.exe")
        with open(test_file, "w") as f:
            f.write("binary data")

        finding = ThreatFinding(
            threat_id="test-threat-02",
            threat_name="Win.Trojan.Agent",
            file_path=test_file,
            file_size_bytes=os.path.getsize(test_file),
            detected_at="2026-10-13T00:00:00",
            is_ransomware=False,
            status="detected",
        )

        ok, _ = self.vault.quarantine_file(finding)
        self.assertTrue(ok)

        # Permanently delete
        del_ok, _ = self.vault.delete_permanently("test-threat-02")
        self.assertTrue(del_ok)
        self.assertEqual(len(self.vault.list_quarantined()), 0)

    def test_pre_scan_auto_update_disabled(self):
        """When auto-update is disabled, pre-scan hook should not trigger freshclam."""
        mock_settings = MagicMock()
        mock_settings.get_boolean.return_value = False

        with patch.object(ClamAVEngine, 'update_definitions') as mock_update:
            ok, msg = ClamAVEngine.ensure_definitions_updated(settings=mock_settings)
            self.assertTrue(ok)
            mock_update.assert_not_called()

    def test_pre_scan_auto_update_fresh_definitions(self):
        """When definitions are up-to-date, pre-scan hook should not re-trigger freshclam."""
        mock_settings = MagicMock()
        mock_settings.get_boolean.return_value = True
        mock_settings.get_int.return_value = 24

        fresh_status = DefinitionsStatus(
            installed=True,
            version="27142",
            release_date="2026-10-13",
            age_hours=2.0,
            total_signatures=8650230,
            status_label="Up to Date",
            is_stale=False,
        )

        with patch.object(ClamAVEngine, 'get_definitions_info', return_value=fresh_status):
            with patch.object(ClamAVEngine, 'update_definitions') as mock_update:
                ok, msg = ClamAVEngine.ensure_definitions_updated(settings=mock_settings)
                self.assertTrue(ok)
                mock_update.assert_not_called()

    def test_pre_scan_auto_update_stale_triggers_update(self):
        """When definitions are stale and auto-update is on, freshclam is invoked."""
        mock_settings = MagicMock()
        mock_settings.get_boolean.return_value = True
        mock_settings.get_int.return_value = 24

        stale_status = DefinitionsStatus(
            installed=True,
            version="26000",
            release_date="2026-09-01",
            age_hours=72.0,
            total_signatures=8000000,
            status_label="Outdated",
            is_stale=True,
        )

        with patch.object(ClamAVEngine, 'get_definitions_info', return_value=stale_status):
            with patch.object(ClamAVEngine, 'update_definitions', return_value=(True, "Updated")) as mock_update:
                ok, msg = ClamAVEngine.ensure_definitions_updated(settings=mock_settings)
                self.assertTrue(ok)
                mock_update.assert_called_once()

    def test_quick_scan_targets(self):
        """Verify quick scan targets returns existing host paths."""
        targets = ClamAVEngine.get_quick_scan_targets()
        self.assertIsInstance(targets, list)
        self.assertTrue(len(targets) > 0)
        for t in targets:
            self.assertTrue(os.path.exists(t))


if __name__ == "__main__":
    unittest.main()
