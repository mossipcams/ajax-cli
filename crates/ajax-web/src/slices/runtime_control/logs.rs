//! Redacted lifecycle log tail for runtime control.

use std::{fs, path::Path};

const MAX_LOG_LINES: usize = 50;
const MAX_LINE_BYTES: usize = 512;

const REDACT_PATTERNS: &[&str] = &[
    "authorization:",
    "cookie:",
    "token=",
    "bearer ",
    "-----begin",
    "private key",
];

pub fn recent_log_lines(runtime_dir: &Path) -> Vec<String> {
    let path = super::store::log_file_path(runtime_dir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut lines = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(redact_line)
        .collect::<Vec<_>>();
    if lines.len() > MAX_LOG_LINES {
        lines.drain(0..lines.len() - MAX_LOG_LINES);
    }
    lines
}

fn redact_line(line: &str) -> String {
    let mut trimmed = line.trim().to_string();
    if trimmed.len() > MAX_LINE_BYTES {
        trimmed.truncate(MAX_LINE_BYTES);
        trimmed.push('…');
    }
    let lower = trimmed.to_lowercase();
    for pattern in REDACT_PATTERNS {
        if lower.contains(pattern) {
            return "[redacted lifecycle line]".to_string();
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::redact_line;

    #[test]
    fn redact_line_scrubs_sensitive_tokens() {
        assert_eq!(
            redact_line("Authorization: Bearer secret-token"),
            "[redacted lifecycle line]"
        );
        assert_eq!(redact_line("building frontend"), "building frontend");
    }
}
