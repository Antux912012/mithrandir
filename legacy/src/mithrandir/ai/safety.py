import re
from typing import Tuple, List

class SafetyGuard:
    """
    Enforces strict safety verification on fix proposals and commands.
    Prevents destructive operations like accidental disk formatting or recursion on root.
    """

    DENYLIST_PATTERNS = [
        r'rm\s+-(?:[rf]{1,2})\s+/(?:\s|$|\*)',     # rm -rf / or rm -rf /*
        r'mkfs',                                    # mkfs formatting
        r'dd\s+if=.*of=/dev/(?:sd[a-z]|nvme\d+n\d+|vd[a-z])', # raw disk overwriting
        r':\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:',         # forkbomb
        r'>\s*/dev/(?:sd[a-z]|nvme\d+n\d+|vd[a-z])',         # direct disk redirect
        r'chmod\s+-R\s+777\s+/',                    # destructive recursive perm modification
    ]

    @classmethod
    def evaluate_command(cls, command: str) -> Tuple[bool, str, str]:
        """
        Evaluates a proposed command.
        Returns (is_allowed, risk_level, reason).
        risk_level can be 'safe', 'moderate', 'dangerous', or 'blocked'.
        """
        cleaned = command.strip()
        for pattern in cls.DENYLIST_PATTERNS:
            if re.search(pattern, cleaned):
                return False, 'blocked', "Command contains dangerous or destructive patterns that could damage the system."

        # High-risk operations needing explicit user review
        if 'pkexec' in cleaned or 'sudo' in cleaned or 'systemctl restart' in cleaned:
            return True, 'moderate', "Requires elevated root privileges."

        if cleaned.startswith('systemctl stop') or cleaned.startswith('systemctl disable'):
            return True, 'moderate', "Stops or disables system service."

        return True, 'safe', "Standard diagnostic/maintenance command."
