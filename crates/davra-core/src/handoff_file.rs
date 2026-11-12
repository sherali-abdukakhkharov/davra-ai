//! Handoff files in the Mosla convention:
//!
//! ```text
//! ---
//! from: core
//! to: business
//! ticket: MOS-020
//! status: open        # open | done
//! created: 2026-10-31
//! ---
//! What changed: ...
//! What you need to do: ...
//! Done when: ...
//! ```
//!
//! File name: `YYYY-MM-DD-<from>-to-<to>-<ticket>.md`. The receiving agent sets
//! `status: done` and adds one line under the frontmatter with what it did.

use crate::mentions::normalize_handle;

#[derive(Debug, Clone, PartialEq)]
pub struct HandoffFile {
    pub from: String,
    pub to: Vec<String>,
    pub ticket: Option<String>,
    pub status: String,
    pub created: Option<String>,
    pub body: String,
}

/// Splits `to:` values like `core, auth` or `core auth business`.
pub fn split_labels(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(|c: char| c == ',' || c == '&' || c == '/' || c.is_whitespace()) {
        if let Some(h) = normalize_handle(part) {
            if !out.contains(&h) {
                out.push(h);
            }
        }
    }
    out
}

fn strip_comment(v: &str) -> &str {
    match v.find(" #") {
        Some(i) => v[..i].trim(),
        None => v.trim(),
    }
}

pub fn parse(text: &str) -> Option<HandoffFile> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = text.strip_prefix("---")?;
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;
    let end = rest.find("\n---")?;
    let (front, after) = rest.split_at(end);
    let body = after
        .trim_start_matches("\n---")
        .trim_start_matches(['\r', '\n']);

    let (mut from, mut to, mut ticket, mut status, mut created) =
        (None, Vec::new(), None, None, None);
    for line in front.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = strip_comment(v).trim_matches(['"', '\'']);
        match k.trim() {
            "from" => from = normalize_handle(v),
            "to" => to = split_labels(v),
            "ticket" if !v.is_empty() => ticket = Some(v.to_string()),
            "status" => status = Some(v.to_ascii_lowercase()),
            "created" if !v.is_empty() => created = Some(v.to_string()),
            _ => {}
        }
    }
    let status = match status.as_deref() {
        Some("done") | Some("closed") | Some("resolved") => "done".to_string(),
        _ => "open".to_string(),
    };
    Some(HandoffFile {
        from: from?,
        to,
        ticket,
        status,
        created,
        body: body.trim_end().to_string(),
    })
}

pub fn file_name(date: &str, from: &str, to: &[String], ticket: Option<&str>) -> String {
    let mut name = format!("{date}-{from}-to-{}", to.join("-"));
    if let Some(t) = ticket {
        name.push('-');
        name.push_str(t);
    }
    name.push_str(".md");
    name
}

pub fn render(h: &HandoffFile) -> String {
    let mut s = String::from("---\n");
    s.push_str(&format!("from: {}\n", h.from));
    s.push_str(&format!("to: {}\n", h.to.join(", ")));
    if let Some(t) = &h.ticket {
        s.push_str(&format!("ticket: {t}\n"));
    }
    s.push_str(&format!("status: {}\n", h.status));
    if let Some(c) = &h.created {
        s.push_str(&format!("created: {c}\n"));
    }
    s.push_str("---\n");
    s.push_str(h.body.trim_end());
    s.push('\n');
    s
}

/// Marks a handoff file done: sets `status: done` and adds the note as one
/// line right under the frontmatter. Returns `None` if the text is not a
/// handoff file.
pub fn mark_done(text: &str, note: &str) -> Option<String> {
    parse(text)?;
    let mut out = String::with_capacity(text.len() + note.len() + 16);
    let mut fences = 0;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\r', '\n']);
        if bare == "---" {
            fences += 1;
            out.push_str(line);
            if fences == 2 {
                out.push_str(&format!("Done: {}\n", crate::format::one_line(note)));
            }
            continue;
        }
        if fences == 1 && bare.trim_start().starts_with("status:") {
            out.push_str("status: done\n");
            continue;
        }
        out.push_str(line);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\nfrom: ui-kit\nto: architect\nticket: MOS-032\nstatus: open        # open | done\ncreated: 2026-11-02\n---\nWhat changed: tokens v2.\n\nDone when: tests pass.\n";

    #[test]
    fn parses_mosla_file() {
        let h = parse(SAMPLE).unwrap();
        assert_eq!(h.from, "ui-kit");
        assert_eq!(h.to, vec!["architect"]);
        assert_eq!(h.ticket.as_deref(), Some("MOS-032"));
        assert_eq!(h.status, "open");
        assert_eq!(h.created.as_deref(), Some("2026-11-02"));
        assert!(h.body.starts_with("What changed"));
    }

    #[test]
    fn splits_multiple_targets() {
        assert_eq!(
            split_labels("core, auth business"),
            vec!["core", "auth", "business"]
        );
        assert_eq!(split_labels("db/designer"), vec!["db", "designer"]);
    }

    #[test]
    fn roundtrip_and_done() {
        let h = parse(SAMPLE).unwrap();
        let again = parse(&render(&h)).unwrap();
        assert_eq!(h, again);
        let done = mark_done(SAMPLE, "counter added, tests green").unwrap();
        let d = parse(&done).unwrap();
        assert_eq!(d.status, "done");
        assert!(d
            .body
            .starts_with("Done: counter added, tests green\nWhat changed"));
    }

    #[test]
    fn windows_newlines_and_bad_files() {
        let crlf = SAMPLE.replace('\n', "\r\n");
        assert_eq!(parse(&crlf).unwrap().ticket.as_deref(), Some("MOS-032"));
        assert!(parse("# just markdown").is_none());
        assert!(parse("---\nto: x\n---\nno from").is_none());
    }

    #[test]
    fn names_files() {
        assert_eq!(
            file_name(
                "2026-11-12",
                "db",
                &["core".into(), "auth".into()],
                Some("MOS-096")
            ),
            "2026-11-12-db-to-core-auth-MOS-096.md"
        );
    }
}
