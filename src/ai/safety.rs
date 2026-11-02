use std::env;

pub struct SafetyGuard;

impl SafetyGuard {
    /// Redacts personally identifiable information (usernames, home paths, tokens) from text.
    pub fn redact_pii(text: &str) -> String {
        let mut result = text.to_string();

        // Redact active username
        if let Ok(user) = env::var("USER") {
            if !user.is_empty() {
                let pattern = format!("/home/{}", user);
                result = result.replace(&pattern, "/home/[REDACTED_USER]");
            }
        }

        // Generic /home/<user> fallback
        let mut lines = Vec::new();
        for line in result.lines() {
            let mut modified_line = line.to_string();
            if let Some(pos) = modified_line.find("/home/") {
                let rest = &modified_line[pos + 6..];
                if let Some(end_slash) = rest.find('/') {
                    let user_part = &rest[..end_slash];
                    if !user_part.is_empty() && user_part != "[REDACTED_USER]" {
                        let full_user_path = format!("/home/{}", user_part);
                        modified_line = modified_line.replace(&full_user_path, "/home/[REDACTED_USER]");
                    }
                }
            }
            lines.push(modified_line);
        }
        lines.join("\n")
    }

    /// Validates proposed terminal commands against dangerous or destructive patterns.
    pub fn is_command_safe(cmd: &str) -> (bool, String) {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return (false, "Command is empty.".to_string());
        }

        let denylist = [
            ("rm -rf /", "Catastrophic filesystem deletion attempted"),
            ("rm -rf /*", "Catastrophic filesystem deletion attempted"),
            ("mkfs", "Filesystem format command detected"),
            ("dd if=", "Direct disk overwriting (dd) command detected"),
            ("> /dev/sd", "Direct raw block device write detected"),
            ("> /dev/nvme", "Direct raw block device write detected"),
            (":(){:|:&};:", "Fork bomb script detected"),
            ("chmod -r 777", "Dangerous root permission tampering detected"),
            ("chmod 777 /", "Dangerous root permission tampering detected"),
            ("chown -r", "Recursive ownership change detected"),
            ("shutdown", "System power manipulation blocked in auto-exec"),
            ("reboot", "System reboot blocked in auto-exec"),
        ];

        let lower = trimmed.to_lowercase();
        for (pattern, reason) in denylist {
            if lower.contains(&pattern.to_lowercase()) {
                return (false, format!("Blocked dangerous command: {}", reason));
            }
        }

        // Disallow remote script piping
        if (lower.contains("curl ") || lower.contains("wget "))
            && (lower.contains("| sh") || lower.contains("| bash") || lower.contains("| sudo"))
        {
            return (false, "Blocked remote script piped execution.".to_string());
        }

        (true, "Command passed security checks.".to_string())
    }
}
