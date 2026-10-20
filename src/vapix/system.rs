//! System and audit logs.

use crate::vapix::client::VapixClient;

/// The camera's system log, or its last `lines` lines.
pub fn system_log(client: &VapixClient, lines: Option<usize>) -> anyhow::Result<String> {
    Ok(tail(&client.get_text("/axis-cgi/admin/systemlog.cgi", &[])?, lines))
}

/// The security audit log (logins, configuration changes), or its last `lines` lines.
pub fn audit_log(client: &VapixClient, lines: Option<usize>) -> anyhow::Result<String> {
    Ok(tail(&client.get_text("/axis-cgi/auditlog.cgi", &[])?, lines))
}

/// The last `lines` lines of `text`; everything when `lines` is absent or 0.
pub fn tail(text: &str, lines: Option<usize>) -> String {
    match lines {
        Some(n) if n > 0 => {
            let all: Vec<&str> = text.lines().collect();
            all[all.len().saturating_sub(n)..].join("\n")
        }
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::tail;

    #[test]
    fn tail_keeps_the_last_lines() {
        assert_eq!(tail("a\nb\nc\nd", Some(2)), "c\nd");
        assert_eq!(tail("a\nb", Some(5)), "a\nb");
    }

    #[test]
    fn tail_without_a_limit_returns_everything() {
        assert_eq!(tail("a\nb\n", None), "a\nb\n");
        assert_eq!(tail("a\nb\n", Some(0)), "a\nb\n");
    }
}
