import os
import shutil
import tempfile
from typing import Dict, Any, Optional
from .host_bridge import HostBridge

class GrubManager:
    """
    Manages GRUB bootloader configuration (/etc/default/grub).
    Supports personalizing boot timeout, default entry, quiet/splash modes,
    OS prober, and reverting to original default configuration.
    """

    GRUB_CONFIG_PATH = '/etc/default/grub'
    GRUB_BACKUP_PATH = '/etc/default/grub.mithrandir.bak'

    @classmethod
    def is_available(cls) -> bool:
        return HostBridge.file_exists(cls.GRUB_CONFIG_PATH)

    @classmethod
    def has_backup(cls) -> bool:
        return HostBridge.file_exists(cls.GRUB_BACKUP_PATH)

    @classmethod
    def ensure_backup(cls) -> bool:
        """
        Creates a pristine backup of /etc/default/grub if one doesn't exist yet.
        """
        if not cls.has_backup() and cls.is_available():
            code, _, _ = HostBridge.run_command(['cp', cls.GRUB_CONFIG_PATH, cls.GRUB_BACKUP_PATH], use_pkexec=True)
            return code == 0
        return True

    @classmethod
    def load_config(cls) -> Dict[str, Any]:
        """
        Reads and parses current /etc/default/grub settings.
        """
        content = HostBridge.read_host_file(cls.GRUB_CONFIG_PATH)
        if not content:
            return {
                'timeout': 5,
                'default_entry': '0',
                'quiet': True,
                'splash': True,
                'os_prober': False,
                'custom_args': '',
                'has_backup': cls.has_backup()
            }

        cfg = {}
        for line in content.splitlines():
            s = line.strip()
            if s and not s.startswith('#') and '=' in s:
                k, v = s.split('=', 1)
                cfg[k.strip()] = v.strip().strip('"\'')

        # Timeout
        try:
            timeout = int(cfg.get('GRUB_TIMEOUT', '5'))
        except ValueError:
            timeout = 5

        default_entry = cfg.get('GRUB_DEFAULT', '0')
        cmdline = cfg.get('GRUB_CMDLINE_LINUX_DEFAULT', 'quiet splash')

        cmd_parts = cmdline.split()
        quiet = 'quiet' in cmd_parts
        splash = 'splash' in cmd_parts
        custom_parts = [p for p in cmd_parts if p not in ('quiet', 'splash')]
        custom_args = " ".join(custom_parts)

        os_prober_disabled = cfg.get('GRUB_DISABLE_OS_PROBER', 'true').lower() == 'true'

        return {
            'timeout': timeout,
            'default_entry': default_entry,
            'quiet': quiet,
            'splash': splash,
            'os_prober': not os_prober_disabled,
            'custom_args': custom_args,
            'has_backup': cls.has_backup()
        }

    @classmethod
    def save_config(cls, options: Dict[str, Any]) -> tuple[bool, str]:
        """
        Updates /etc/default/grub with user preferences, preserving other lines and comments,
        then updates grub configuration with update-grub or grub-mkconfig.
        """
        if not cls.is_available():
            return False, "GRUB configuration file /etc/default/grub not found."

        # Ensure backup exists first
        cls.ensure_backup()

        content = HostBridge.read_host_file(cls.GRUB_CONFIG_PATH)
        if not content:
            return False, "Could not read /etc/default/grub."

        # Assemble cmdline
        cmd_tokens = []
        if options.get('quiet', True):
            cmd_tokens.append('quiet')
        if options.get('splash', True):
            cmd_tokens.append('splash')
        if options.get('custom_args', '').strip():
            cmd_tokens.append(options['custom_args'].strip())
        new_cmdline = " ".join(cmd_tokens)

        new_timeout = str(options.get('timeout', 5))
        new_default = str(options.get('default_entry', '0'))
        os_prober = "false" if options.get('os_prober', False) else "true"

        keys_to_update = {
            'GRUB_TIMEOUT': new_timeout,
            'GRUB_DEFAULT': f'"{new_default}"' if ' ' in new_default else new_default,
            'GRUB_CMDLINE_LINUX_DEFAULT': f'"{new_cmdline}"',
            'GRUB_DISABLE_OS_PROBER': os_prober
        }

        updated_lines = []
        found_keys = set()

        for line in content.splitlines():
            s = line.strip()
            if s and not s.startswith('#') and '=' in s:
                k = s.split('=', 1)[0].strip()
                if k in keys_to_update:
                    updated_lines.append(f"{k}={keys_to_update[k]}")
                    found_keys.add(k)
                    continue
            updated_lines.append(line)

        # Append any missing keys
        for k, v in keys_to_update.items():
            if k not in found_keys:
                updated_lines.append(f"{k}={v}")

        new_text = "\n".join(updated_lines) + "\n"

        # Use ~/.cache/mithrandir_grub.tmp so the file is accessible to both the Flatpak sandbox and the host
        cache_dir = os.path.expanduser('~/.cache')
        os.makedirs(cache_dir, exist_ok=True)
        temp_path = os.path.join(cache_dir, f'mithrandir_grub_{os.getpid()}.tmp')

        try:
            with open(temp_path, 'w', encoding='utf-8') as tf:
                tf.write(new_text)

            copy_cmd = ['cp', temp_path, cls.GRUB_CONFIG_PATH]
            code, out, err = HostBridge.run_command(copy_cmd, use_pkexec=True)
        finally:
            try:
                if os.path.exists(temp_path):
                    os.remove(temp_path)
            except OSError:
                pass

        if code != 0:
            return False, f"Failed to update /etc/default/grub: {err or out}"

        # Run update-grub or grub-mkconfig
        return cls._run_grub_update()

    @classmethod
    def revert_to_default(cls) -> tuple[bool, str]:
        """
        Reverts /etc/default/grub from the backup file and updates GRUB.
        """
        if not cls.has_backup():
            return False, "No backup found to restore."

        code, out, err = HostBridge.run_command(['cp', cls.GRUB_BACKUP_PATH, cls.GRUB_CONFIG_PATH], use_pkexec=True)
        if code != 0:
            return False, f"Failed to restore backup: {err or out}"

        return cls._run_grub_update()

    @classmethod
    def _run_grub_update(cls) -> tuple[bool, str]:
        """
        Executes update-grub or grub-mkconfig on the host.
        """
        if HostBridge.which('update-grub'):
            code, out, err = HostBridge.run_command(['update-grub'], use_pkexec=True)
            if code == 0:
                return True, "GRUB configuration saved and updated successfully."
            return False, f"update-grub error: {err or out}"

        # Fallback to grub-mkconfig / grub2-mkconfig
        for grub_tool in ['grub-mkconfig', 'grub2-mkconfig']:
            if HostBridge.which(grub_tool):
                for cfg_target in ['/boot/grub/grub.cfg', '/boot/grub2/grub.cfg']:
                    if HostBridge.dir_exists(os.path.dirname(cfg_target)):
                        code, out, err = HostBridge.run_command([grub_tool, '-o', cfg_target], use_pkexec=True)
                        if code == 0:
                            return True, f"GRUB updated with {grub_tool} on {cfg_target}."

        return True, "GRUB configuration saved to /etc/default/grub."
