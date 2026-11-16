//! Decision files in the docs/memory/decisions convention:
//!
//! ```text
//! ---
//! id: 12
//! title: Promo video voice
//! ticket: MOS-129
//! status: accepted        # accepted | superseded
//! decided_by: ali_9737
//! date: 2026-11-17
//! source: "#237"
//! ---
//! Decision: Voice with ElevenLabs eleven_v4, through the staging key.
//! Why: Good quality, the key already exists; about 4k characters in total.
//! Instead of: OpenAI TTS (weak Uzbek).
//! ```
//!
//! File name: `YYYY-MM-DD-<title slug>[-<ticket>].md`. A decision that replaces
//! another says `supersedes: <id>`; the old file gets `status: superseded` and
//! `superseded_by: <id>`. The hub holds the same record; the file is what the
//! repository (and anyone reading it) keeps.

use crate::api::DecisionView;
use crate::format::one_line;

/// `promo-video-voice` from a title: lowercase ASCII letters and digits, up to 6 words.
pub fn slug(title: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for w in title.split(|c: char| !c.is_alphanumeric()) {
        let w: String = w
            .chars()
            .filter_map(|c| {
                let c = match c {
                    'ó' | 'Ó' | 'ò' | 'ô' | 'ö' => 'o',
                    'ǵ' | 'Ǵ' | 'ğ' => 'g',
                    'ş' | 'Ş' | 'š' => 's',
                    'ç' | 'Ç' | 'č' => 'c',
                    'á' | 'à' | 'â' | 'ä' => 'a',
                    'é' | 'è' | 'ê' | 'ë' => 'e',
                    'í' | 'ì' | 'î' | 'ï' | 'ı' => 'i',
                    'ú' | 'ù' | 'û' | 'ü' => 'u',
                    c => c,
                };
                c.is_ascii_alphanumeric().then(|| c.to_ascii_lowercase())
            })
            .collect();
        if !w.is_empty() {
            words.push(w);
        }
        if words.len() == 6 {
            break;
        }
    }
    if words.is_empty() {
        "decision".into()
    } else {
        words.join("-")
    }
}

pub fn file_name(date: &str, title: &str, ticket: Option<&str>) -> String {
    let mut name = format!("{date}-{}", slug(title));
    if let Some(t) = ticket.map(slug).filter(|t| t != "decision") {
        name.push('-');
        name.push_str(&t.to_ascii_uppercase());
    }
    name.push_str(".md");
    name
}

fn quoted(v: &str) -> String {
    format!(
        "\"{}\"",
        one_line(v).replace('\\', "\\\\").replace('"', "\\\"")
    )
}

pub fn render(d: &DecisionView) -> String {
    let mut s = String::from("---\n");
    s.push_str(&format!("id: {}\n", d.id));
    s.push_str(&format!("title: {}\n", quoted(&d.title)));
    if let Some(t) = &d.ticket {
        s.push_str(&format!("ticket: {t}\n"));
    }
    s.push_str(&format!("status: {}\n", d.status));
    s.push_str(&format!("decided_by: {}\n", d.decided_by));
    s.push_str(&format!("date: {}\n", d.created_at.format("%Y-%m-%d")));
    if let Some(m) = d.source_message {
        s.push_str(&format!("source: \"#{m}\"\n"));
    }
    if let Some(o) = d.supersedes {
        s.push_str(&format!("supersedes: {o}\n"));
    }
    s.push_str("---\n");
    s.push_str(&format!("Decision: {}\n", d.decision.trim()));
    s.push_str(&format!("Why: {}\n", d.why.trim()));
    if let Some(i) = d.instead.as_deref().filter(|i| !i.trim().is_empty()) {
        s.push_str(&format!("Instead of: {}\n", i.trim()));
    }
    s
}

/// Marks a decision file superseded by decision `by`: sets the status and adds
/// `superseded_by` in the frontmatter. `None` if the text has no frontmatter.
pub fn mark_superseded(text: &str, by: i64) -> Option<String> {
    let mut out = String::with_capacity(text.len() + 32);
    let mut fences = 0;
    let mut status_seen = false;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\r', '\n']);
        if bare == "---" {
            fences += 1;
            if fences == 2 {
                if !status_seen {
                    out.push_str("status: superseded\n");
                }
                out.push_str(&format!("superseded_by: {by}\n"));
            }
            out.push_str(line);
            continue;
        }
        if fences == 1 {
            let key = bare.split(':').next().unwrap_or("").trim();
            if key == "status" {
                status_seen = true;
                out.push_str("status: superseded\n");
                continue;
            }
            if key == "superseded_by" {
                continue;
            }
        }
        out.push_str(line);
    }
    (fences >= 2).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn view() -> DecisionView {
        DecisionView {
            id: 12,
            ticket: Some("MOS-129".into()),
            title: "Promo video: ovoz \"ElevenLabs\"".into(),
            decision: "Voice with ElevenLabs eleven_v4.".into(),
            why: "Good quality.".into(),
            instead: Some("OpenAI TTS".into()),
            decided_by: "ali_9737".into(),
            author: Some("lionali-ai".into()),
            status: "accepted".into(),
            supersedes: Some(7),
            superseded_by: None,
            source_message: Some(237),
            file_path: None,
            message_id: None,
            created_at: chrono::Utc.with_ymd_and_hms(2026, 11, 17, 9, 0, 0).unwrap(),
        }
    }

    #[test]
    fn names() {
        assert_eq!(slug("Promo video: ovoz tanlaş!"), "promo-video-ovoz-tanlas");
        assert_eq!(slug("Órniga — çoy"), "orniga-coy");
        assert_eq!(slug("!!!"), "decision");
        assert_eq!(
            file_name("2026-11-17", "Promo video ovoz", Some("MOS-129")),
            "2026-11-17-promo-video-ovoz-MOS-129.md"
        );
        assert_eq!(file_name("2026-11-17", "x", None), "2026-11-17-x.md");
    }

    #[test]
    fn renders_the_agreed_format() {
        let s = render(&view());
        assert!(s.starts_with("---\nid: 12\ntitle: \"Promo video: ovoz \\\"ElevenLabs\\\"\"\nticket: MOS-129\nstatus: accepted\n"));
        assert!(s.contains(
            "decided_by: ali_9737\ndate: 2026-11-17\nsource: \"#237\"\nsupersedes: 7\n---\n"
        ));
        assert!(s.ends_with("Decision: Voice with ElevenLabs eleven_v4.\nWhy: Good quality.\nInstead of: OpenAI TTS\n"));
    }

    #[test]
    fn supersedes_in_place() {
        let s = render(&view());
        let m = mark_superseded(&s, 15).unwrap();
        assert!(m.contains("status: superseded\n"));
        assert!(m.contains("superseded_by: 15\n---\nDecision:"));
        assert!(!m.contains("status: accepted"));
        // Twice: still one superseded_by line.
        let again = mark_superseded(&m, 16).unwrap();
        assert_eq!(again.matches("superseded_by:").count(), 1);
        assert!(again.contains("superseded_by: 16"));
        assert!(mark_superseded("no frontmatter", 1).is_none());
    }
}
