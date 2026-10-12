import os
import re
import datetime
from typing import List, Dict, Any, Optional
from .host_bridge import HostBridge

class BtrfsManager:
    """
    Manages BTRFS subvolume snapshots, listings, creation, and automated cleanup.
    Works natively with `btrfs-progs`, Snapper, and Timeshift.
    """

    @classmethod
    def get_btrfs_status(cls) -> Dict[str, Any]:
        """
        Detects if BTRFS is present on any mountpoint (e.g. /, /home).
        Returns filesystem details or info about the active root filesystem.
        """
        code, out, _ = HostBridge.run_command(['findmnt', '-t', 'btrfs', '-o', 'TARGET,SOURCE,FSTYPE', '-n'])
        btrfs_mounts = []
        if code == 0 and out.strip():
            for line in out.strip().splitlines():
                parts = line.split()
                if len(parts) >= 2:
                    btrfs_mounts.append({
                        'target': parts[0],
                        'source': parts[1],
                        'fstype': 'btrfs'
                    })

        # Also check root filesystem type
        code_root, out_root, _ = HostBridge.run_command(['findmnt', '-n', '-o', 'FSTYPE,SOURCE', '/'])
        root_fstype = 'unknown'
        root_source = ''
        if code_root == 0 and out_root.strip():
            parts = out_root.strip().split()
            if len(parts) >= 2:
                root_fstype = parts[0]
                root_source = parts[1]

        has_btrfs_cmd = (HostBridge.run_command(['which', 'btrfs'])[0] == 0)
        has_snapper = (HostBridge.run_command(['which', 'snapper'])[0] == 0)

        return {
            'is_btrfs': len(btrfs_mounts) > 0 or (root_fstype == 'btrfs'),
            'btrfs_mounts': btrfs_mounts,
            'root_fstype': root_fstype,
            'root_source': root_source,
            'has_btrfs_tool': has_btrfs_cmd,
            'has_snapper': has_snapper
        }

    @classmethod
    def list_snapshots(cls, mountpoint: str = '/') -> List[Dict[str, Any]]:
        """
        Lists snapshots on the BTRFS filesystem.
        Parses `btrfs subvolume list -s <mountpoint>` output:
        ID <id> gen <gen> top level <top> path <path>
        """
        status = cls.get_btrfs_status()
        if not status['is_btrfs']:
            return []

        snapshots = []
        code, out, _ = HostBridge.run_command(['btrfs', 'subvolume', 'list', '-s', mountpoint], use_pkexec=True)
        if code == 0 and out.strip():
            for line in out.strip().splitlines():
                # Format: ID 256 gen 42 top level 5 path .snapshots/1/snapshot
                match = re.search(r'ID\s+(\d+)\s+gen\s+(\d+)\s+top\s+level\s+(\d+)\s+path\s+(.+)', line)
                if match:
                    subvol_id = match.group(1)
                    gen = match.group(2)
                    top_level = match.group(3)
                    path = match.group(4)
                    name = os.path.basename(path)
                    if name == 'snapshot' or not name:
                        name = path

                    snapshots.append({
                        'id': subvol_id,
                        'generation': gen,
                        'top_level': top_level,
                        'path': path,
                        'name': name,
                        'read_only': True
                    })

        # Also check snapper if empty
        if not snapshots and status['has_snapper']:
            code, out, _ = HostBridge.run_command(['snapper', '-c', 'root', 'list', '--columns', 'number,type,date,description'], use_pkexec=True)
            if code == 0 and out.strip():
                lines = out.strip().splitlines()
                for line in lines[2:]:
                    parts = [p.strip() for p in line.split('|')]
                    if len(parts) >= 4 and parts[0].isdigit():
                        snapshots.append({
                            'id': parts[0],
                            'generation': 'snapper',
                            'top_level': '0',
                            'path': f"/.snapshots/{parts[0]}/snapshot",
                            'name': parts[3] or f"Snapshot #{parts[0]}",
                            'date': parts[2],
                            'read_only': True
                        })

        return snapshots

    @classmethod
    def create_snapshot(cls, name: str, source_path: str = '/', read_only: bool = True) -> tuple[bool, str]:
        """
        Creates a BTRFS snapshot.
        """
        status = cls.get_btrfs_status()
        if not status['is_btrfs']:
            return False, f"Root filesystem is {status['root_fstype']}, not BTRFS. Snapshots are only available on BTRFS."

        safe_name = re.sub(r'[^a-zA-Z0-9_\-\.]', '_', name.strip()) or f"snapshot_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}"
        dest_dir = "/.snapshots" if os.path.isdir('/.snapshots') else os.path.join(source_path, ".snapshots")
        dest_path = os.path.join(dest_dir, safe_name)

        flags = ['-r'] if read_only else []
        cmd = ['btrfs', 'subvolume', 'snapshot'] + flags + [source_path, dest_path]

        code, out, err = HostBridge.run_command(cmd, use_pkexec=True)
        if code == 0:
            return True, f"Snapshot '{safe_name}' created successfully at {dest_path}."
        return False, err or out or "Failed to create snapshot."

    @classmethod
    def delete_snapshot(cls, snapshot_path: str) -> tuple[bool, str]:
        """
        Deletes a BTRFS snapshot subvolume.
        """
        status = cls.get_btrfs_status()
        if not status['is_btrfs']:
            return False, "Not a BTRFS filesystem."

        code, out, err = HostBridge.run_command(['btrfs', 'subvolume', 'delete', snapshot_path], use_pkexec=True)
        if code == 0:
            return True, f"Snapshot deleted: {snapshot_path}"
        return False, err or out or "Failed to delete snapshot."

    @classmethod
    def clean_old_snapshots(cls, keep_latest_count: int = 5) -> tuple[int, str]:
        """
        Keeps the most recent N snapshots and deletes the older ones.
        """
        snapshots = cls.list_snapshots()
        if len(snapshots) <= keep_latest_count:
            return 0, f"Found {len(snapshots)} snapshots; nothing to clean (keeping latest {keep_latest_count})."

        to_delete = snapshots[:-keep_latest_count]
        deleted_count = 0
        for s in to_delete:
            ok, _ = cls.delete_snapshot(s['path'])
            if ok:
                deleted_count += 1

        return deleted_count, f"Cleaned {deleted_count} old BTRFS snapshots."
