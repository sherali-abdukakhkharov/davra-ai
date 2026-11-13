//! Best-effort secret redaction, applied before anything is stored.
//!
//! This is a safety net, not a guarantee: it catches the common token
//! formats and `key = value` assignments that tend to leak into chat.

use regex::Regex;
use std::sync::LazyLock;

pub const MARK: &str = "[REDACTED]";

struct Rule {
    re: Regex,
    /// Capture group to keep in front of the mark (for `key=value` rules).
    keep_prefix: bool,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let whole = |p: &str| Rule {
        re: Regex::new(p).expect("valid redaction regex"),
        keep_prefix: false,
    };
    let keyed = |p: &str| Rule {
        re: Regex::new(p).expect("valid redaction regex"),
        keep_prefix: true,
    };
    vec![
        whole(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----"),
        whole(r"\bsk-ant-[A-Za-z0-9_\-]{20,}"),
        whole(r"\bsk-(?:proj-)?[A-Za-z0-9_\-]{20,}"),
        whole(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}"),
        whole(r"\bgithub_pat_[A-Za-z0-9_]{30,}"),
        whole(r"\bglpat-[A-Za-z0-9_\-]{20,}"),
        whole(r"\bxox[abprs]-[A-Za-z0-9\-]{10,}"),
        whole(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
        whole(r"\bAIza[0-9A-Za-z_\-]{35}\b"),
        whole(r"\b[0-9]{8,10}:AA[A-Za-z0-9_\-]{33}\b"),
        whole(r"\bdvr_[a-f0-9]{32,}\b"),
        whole(r"\beyJ[A-Za-z0-9_\-]{10,}\.eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}"),
        // scheme://user:password@host
        keyed(r"([a-zA-Z][a-zA-Z0-9+.\-]*://[^\s:/@]+:)[^\s@/]+(@)"),
        // password = ..., api_key: ..., TOKEN="..."
        keyed(
            r#"(?i)(\b[A-Za-z0-9_\-]*?(?:password|passwd|pwd|secret|api[_\-]?key|access[_\-]?key|auth[_\-]?token|token|client[_\-]?secret)\b["']?\s*[:=]\s*)["']?[^\s"',;]{6,}["']?"#,
        ),
    ]
});

/// Returns the text with secrets replaced, and whether anything changed.
pub fn redact(text: &str) -> (String, bool) {
    let mut out = text.to_string();
    let mut changed = false;
    for rule in RULES.iter() {
        if !rule.re.is_match(&out) {
            continue;
        }
        changed = true;
        out = if rule.keep_prefix {
            rule.re
                .replace_all(&out, |c: &regex::Captures| {
                    let suffix = c.get(2).map(|m| m.as_str()).unwrap_or("");
                    format!("{}{}{}", &c[1], MARK, suffix)
                })
                .into_owned()
        } else {
            rule.re.replace_all(&out, MARK).into_owned()
        };
    }
    (out, changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_tokens() {
        let (t, c) = redact("token: ghp_abcdefghijklmnopqrstuvwxyz0123456789 ok");
        assert!(c);
        assert!(!t.contains("ghp_"));
        let (t, _) = redact("bot 123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsawQ end");
        assert_eq!(t, "bot [REDACTED] end");
        let (t, _) = redact("key sk-ant-api03-abcdefghijklmnopqrstuvwxyz here");
        assert_eq!(t, "key [REDACTED] here");
    }

    #[test]
    fn keeps_key_names() {
        let (t, _) = redact("DB_PASSWORD=hunter2hunter2 and api_key: \"abc123456\"");
        assert!(t.contains("DB_PASSWORD=[REDACTED]"), "{t}");
        assert!(t.contains("api_key: [REDACTED]"), "{t}");
    }

    #[test]
    fn redacts_url_passwords() {
        let (t, _) = redact("postgres://davra:s3cretpass@db:5432/davra");
        assert_eq!(t, "postgres://davra:[REDACTED]@db:5432/davra");
    }

    #[test]
    fn leaves_normal_text() {
        let s = "Textarea maxLength endi soft limit, IndustryStep.tsx:111 ga counter qo'shing";
        assert_eq!(redact(s), (s.to_string(), false));
    }

    #[test]
    fn redacts_private_keys() {
        let s = "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\ndef\n-----END OPENSSH PRIVATE KEY-----";
        assert_eq!(redact(s).0, MARK);
    }
}
