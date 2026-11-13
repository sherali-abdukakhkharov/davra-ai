//! `@handle` parsing.

/// Handles that address a group rather than one member.
pub const GROUP_HANDLES: &[&str] = &["all", "agents"];

const MAX_HANDLE: usize = 64;

fn is_handle_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Normalises a handle: trims a leading `@`, lowercases, and rejects
/// anything that is not a valid handle.
pub fn normalize_handle(raw: &str) -> Option<String> {
    let h = raw.trim().trim_start_matches('@');
    let h = h.trim_end_matches('.');
    if h.is_empty() || h.chars().count() > MAX_HANDLE {
        return None;
    }
    let first = h.chars().next()?;
    if !first.is_ascii_alphanumeric() || !h.chars().all(is_handle_char) {
        return None;
    }
    Some(h.to_ascii_lowercase())
}

/// Extracts unique, lowercased `@handles` from text, in order of appearance.
///
/// An `@` preceded by a word character (as in an e-mail address) is ignored.
pub fn parse(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '@' {
            let prev_ok = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
            let mut j = i + 1;
            while j < chars.len() && is_handle_char(chars[j]) {
                j += 1;
            }
            if prev_ok && j > i + 1 {
                let raw: String = chars[i + 1..j].iter().collect();
                if let Some(h) = normalize_handle(&raw) {
                    if !out.contains(&h) {
                        out.push(h);
                    }
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_handles_in_order() {
        assert_eq!(
            parse("@backend-ai bugun qilib ber, @Frontend ko'rib chiq. @backend-ai"),
            vec!["backend-ai", "frontend"]
        );
    }

    #[test]
    fn ignores_emails_and_bare_at() {
        assert!(parse("mail me at a@b.uz or just @ here").is_empty());
    }

    #[test]
    fn strips_trailing_dot() {
        assert_eq!(parse("rahmat @frontend-ai."), vec!["frontend-ai"]);
    }

    #[test]
    fn normalizes() {
        assert_eq!(normalize_handle("@Backend_AI"), Some("backend_ai".into()));
        assert_eq!(normalize_handle("-bad"), None);
        assert_eq!(normalize_handle(""), None);
        assert_eq!(normalize_handle("has space"), None);
    }
}
