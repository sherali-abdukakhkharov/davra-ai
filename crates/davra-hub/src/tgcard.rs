//! How room messages look in the project's Telegram group.
//!
//! Every message is a small card sent with `sendRichMessage` (HTML, Bot API
//! 10.1+): the first line says who or what, then the body, then a footer for
//! outside sources. GitHub pushes, PRs and passing CI runs collect in one
//! digest card that is edited as events arrive; failures get their own card.
//!
//! Entity detection is off, so Telegram never turns `@gulom` out of
//! `@gulom-ai` into a link to a stranger: people with a Telegram identity are
//! explicit `tg://user` mentions (they get notified), agents and unknown
//! handles are code. Each card also carries the old plain text, sent with
//! `sendMessage` when a project is set to plain or a rich send fails.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::github::GhEvent;
use crate::telegram::{render_mentions, TgPerson};

pub type People = HashMap<String, TgPerson>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Uz,
}

impl Lang {
    /// `DAVRA_LANG`: "uz" for Uzbek labels, anything else English.
    pub fn from_env() -> Lang {
        match std::env::var("DAVRA_LANG").as_deref() {
            Ok("uz") => Lang::Uz,
            _ => Lang::En,
        }
    }

    fn pick(self, en: &'static str, uz: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Uz => uz,
        }
    }
}

/// A button under the card (opens a URL).
#[derive(Clone, Debug)]
pub struct Button {
    pub label: String,
    pub url: String,
    pub style: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct Card {
    pub html: String,
    /// Plain-text fallback for sendMessage, with its mention entities.
    pub plain: String,
    pub entities: Vec<serde_json::Value>,
    /// Arrives without a sound (GitHub chatter).
    pub silent: bool,
}

impl Card {
    fn new(html: String, plain: &str, people: &People, silent: bool) -> Card {
        let (plain, entities) = render_mentions(plain, people);
        Card {
            html,
            plain,
            entities,
            silent,
        }
    }
}

/// Escapes text for Telegram's rich HTML.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn attr_url(url: &str) -> Option<String> {
    (url.starts_with("https://") || url.starts_with("http://")).then(|| esc(url))
}

fn buttons(bs: &[Button]) -> String {
    if bs.is_empty() {
        return String::new();
    }
    let mut out = String::from("<tg-button-row align=\"left\">");
    for b in bs {
        let Some(url) = attr_url(&b.url) else {
            continue;
        };
        let style = b
            .style
            .map(|s| format!(" style=\"{s}\""))
            .unwrap_or_default();
        out.push_str(&format!(
            "<tg-button type=\"url\"{style} url=\"{url}\">{}</tg-button>",
            esc(&b.label)
        ));
    }
    out.push_str("</tg-button-row>");
    out
}

/// One @handle: a real mention for people Telegram knows, code otherwise.
pub fn handle(h: &str, people: &People) -> String {
    match people.get(&h.to_ascii_lowercase()) {
        Some(p) => {
            let shown = p
                .username
                .as_deref()
                .map(|u| format!("@{u}"))
                .unwrap_or_else(|| p.name.clone());
            format!("<a href=\"tg://user?id={}\">{}</a>", p.user_id, esc(&shown))
        }
        None => format!("<code>@{}</code>", esc(h)),
    }
}

static TOKENS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"`([^`\n]+)`|(https?://[^\s<>`]+)|@([A-Za-z0-9._-]+)|\b([A-Z][A-Z0-9]{1,9}-[0-9]{1,6})\b",
    )
    .expect("valid regex")
});

/// Inline text: escaped, with `code`, links, @mentions and ticket ids.
pub fn inline(text: &str, people: &People) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut last = 0;
    for c in TOKENS.captures_iter(text) {
        let m = c.get(0).expect("whole match");
        if let Some(code) = c.get(1) {
            out.push_str(&esc(&text[last..m.start()]));
            out.push_str(&format!("<code>{}</code>", esc(code.as_str())));
        } else if let Some(url) = c.get(2) {
            // Sentence punctuation after a URL is not part of it.
            let u = url
                .as_str()
                .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '"']);
            out.push_str(&esc(&text[last..m.start()]));
            out.push_str(&format!("<a href=\"{}\">{}</a>", esc(u), esc(u)));
            out.push_str(&esc(&url.as_str()[u.len()..]));
        } else if let Some(h) = c.get(3) {
            // Only at a word start (not e-mail addresses); a trailing dot ends the sentence.
            let before = text[..m.start()].chars().next_back();
            if before.is_some_and(|b| b.is_alphanumeric() || b == '_') {
                out.push_str(&esc(&text[last..m.end()]));
                last = m.end();
                continue;
            }
            let name = h.as_str().trim_end_matches('.');
            out.push_str(&esc(&text[last..m.start()]));
            out.push_str(&handle(name, people));
            out.push_str(&esc(&h.as_str()[name.len()..]));
        } else if let Some(t) = c.get(4) {
            out.push_str(&esc(&text[last..m.start()]));
            out.push_str(&format!("<b>{}</b>", esc(t.as_str())));
        }
        last = m.end();
    }
    out.push_str(&esc(&text[last..]));
    out
}

static BULLET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[-*•]\s+(.*)$").expect("valid regex"));
static NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\d{1,2}[.)]\s+(.*)$").expect("valid regex"));
static INLINE_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)(\d{1,2})\)\s+").expect("valid regex"));

/// "Intro: 1) a 2) b 3) c" on one line: the intro and the items, if numbered 1..n.
fn inline_list(line: &str) -> Option<(&str, Vec<&str>)> {
    let marks: Vec<(usize, usize, u32)> = INLINE_ITEM
        .captures_iter(line)
        .filter_map(|c| {
            let m = c.get(0)?;
            Some((m.start(), m.end(), c.get(1)?.as_str().parse().ok()?))
        })
        .collect();
    if marks.len() < 2 || marks.iter().enumerate().any(|(i, m)| m.2 != i as u32 + 1) {
        return None;
    }
    let intro = line[..marks[0].0].trim();
    let items = marks
        .iter()
        .enumerate()
        .map(|(i, m)| line[m.1..marks.get(i + 1).map_or(line.len(), |n| n.0)].trim())
        .collect();
    Some((intro, items))
}

/// A message body: paragraphs, line breaks, bullet and numbered lists.
pub fn body(text: &str, people: &People) -> String {
    let mut out = String::new();
    for para in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        let lines: Vec<&str> = para.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let kind = |l: &str| {
                if BULLET.is_match(l) {
                    1
                } else if NUMBERED.is_match(l) {
                    2
                } else {
                    0
                }
            };
            let k = kind(lines[i]);
            let mut j = i;
            while j < lines.len() && kind(lines[j]) == k {
                j += 1;
            }
            let group = &lines[i..j];
            match k {
                1 | 2 if group.len() >= 2 => {
                    let re = if k == 1 { &*BULLET } else { &*NUMBERED };
                    let tag = if k == 1 { "ul" } else { "ol" };
                    out.push_str(&format!("<{tag}>"));
                    for l in group {
                        let item = re
                            .captures(l)
                            .and_then(|c| c.get(1))
                            .map_or(*l, |m| m.as_str());
                        out.push_str(&format!("<li>{}</li>", inline(item, people)));
                    }
                    out.push_str(&format!("</{tag}>"));
                }
                _ => {
                    if let [one] = group {
                        if let Some((intro, items)) = inline_list(one) {
                            if !intro.is_empty() {
                                out.push_str(&format!("<p>{}</p>", inline(intro, people)));
                            }
                            out.push_str("<ol>");
                            for it in items {
                                out.push_str(&format!("<li>{}</li>", inline(it, people)));
                            }
                            out.push_str("</ol>");
                            i = j;
                            continue;
                        }
                    }
                    let joined: Vec<String> = group.iter().map(|l| inline(l, people)).collect();
                    out.push_str(&format!("<p>{}</p>", joined.join("<br>")));
                }
            }
            i = j;
        }
    }
    out
}

/// Who wrote a room message.
pub struct Author<'a> {
    pub handle: &'a str,
    pub is_agent: bool,
    pub display_name: Option<&'a str>,
    /// The agent's owner (handle).
    pub owner: Option<&'a str>,
}

/// A chat message from a person or an agent.
pub fn post(lang: Lang, a: &Author, text: &str, people: &People) -> Card {
    let head = if a.is_agent {
        let owner = a
            .owner
            .map(|o| {
                format!(
                    " · <i>{}</i>",
                    esc(&format!("{o} {}", lang.pick("agent", "agenti")))
                )
            })
            .unwrap_or_default();
        format!("<p>🤖 <b>{}</b>{owner}</p>", esc(a.handle))
    } else {
        let name = a
            .display_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(a.handle);
        format!("<p>👤 <b>{}</b></p>", esc(name))
    };
    let icon = if a.is_agent { "🤖" } else { "💬" };
    Card::new(
        format!("{head}{}", body(text, people)),
        &format!("{icon} {}\n{text}", a.handle),
        people,
        false,
    )
}

fn labels(ls: &[String]) -> String {
    ls.iter()
        .map(|l| format!("<code>{}</code>", esc(l)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A new handoff.
#[allow(clippy::too_many_arguments)]
pub fn handoff_new(
    lang: Lang,
    id: i64,
    ticket: Option<&str>,
    from: &str,
    to: &[String],
    text: &str,
    people: &People,
) -> Card {
    let ticket_html = ticket
        .map(|t| format!(" · <b>{}</b>", esc(t)))
        .unwrap_or_default();
    let ticket_plain = ticket.map(|t| format!(" · {t}")).unwrap_or_default();
    let shown = davra_core::format::truncate_chars(text, 3000);
    let _ = lang;
    Card::new(
        format!(
            "<p>📋 <b>Handoff #{id}</b>{ticket_html}</p><p><code>{}</code> → {}</p>{}",
            esc(from),
            labels(to),
            body(&shown, people)
        ),
        &format!(
            "📋 Handoff #{id}{ticket_plain} · {from} → {}\n{}",
            to.join(", "),
            davra_core::format::truncate_chars(text.lines().next().unwrap_or(""), 600)
        ),
        people,
        false,
    )
}

/// A handoff closed by `by` with a note.
pub fn handoff_done(
    lang: Lang,
    id: i64,
    ticket: Option<&str>,
    by: &str,
    note: &str,
    people: &People,
) -> Card {
    let ticket_html = ticket
        .map(|t| format!(" · <b>{}</b>", esc(t)))
        .unwrap_or_default();
    let ticket_plain = ticket.map(|t| format!(" · {t}")).unwrap_or_default();
    Card::new(
        format!(
            "<p>✅ <b>Handoff #{id} {}</b>{ticket_html}</p><p><code>{}</code>: {}</p>",
            lang.pick("done", "bajarildi"),
            esc(by),
            inline(note, people)
        ),
        &format!("✅ Done · handoff #{id}{ticket_plain} · {note}"),
        people,
        false,
    )
}

/// Someone joined the project.
pub fn joined(lang: Lang, who: &str, role: &str, people: &People) -> Card {
    Card::new(
        format!(
            "<p>👋 {} {} · <b>{}</b></p>",
            handle(who, people),
            lang.pick("joined the project", "loyihaga qóşildi"),
            esc(role)
        ),
        &format!("👋 @{who} joined the project as {role}."),
        people,
        false,
    )
}

/// An agent asks its owner to decide (answer buttons go in reply_markup).
pub fn question(
    lang: Lang,
    qid: i64,
    agent: &str,
    owner: &str,
    text: &str,
    options: &[String],
    people: &People,
) -> Card {
    let mut plain = format!("❓ {agent}\nQuestion #{qid} for @{owner}: {text}");
    for (i, o) in options.iter().enumerate() {
        plain.push_str(&format!("\n{}. {}", i + 1, o));
    }
    let only = match lang {
        Lang::En => format!("Only @{owner} can answer"),
        Lang::Uz => format!("Faqat @{owner} javob bera oladi"),
    };
    Card::new(
        format!(
            "<p>❓ <b>{} #{qid}</b> · <code>{}</code> → {}</p>{}<footer>{}</footer>",
            lang.pick("Question", "Savol"),
            esc(agent),
            handle(owner, people),
            body(text, people),
            esc(&only)
        ),
        &plain,
        people,
        false,
    )
}

/// The answer to a question, as a reply to it.
pub fn answer(who: &str, text: &str, people: &People) -> Card {
    Card::new(
        format!(
            "<p>✅ <b>{}</b> · {}</p>",
            inline(text, people),
            handle(who, people)
        ),
        &format!("✅ {who}: {text}"),
        people,
        false,
    )
}

/// A failed (or otherwise not passing) CI run: its own card, with sound.
pub fn ci_failed(lang: Lang, ev: &GhEvent) -> Option<Card> {
    let GhEvent::Ci {
        name,
        mark,
        branch,
        url,
        run_number,
        sha,
        actor,
        head,
    } = ev
    else {
        return None;
    };
    let run = run_number.map(|n| format!(" #{n}")).unwrap_or_default();
    let title = if mark == "FAILED" {
        lang.pick("CI failed", "CI yiqildi").to_string()
    } else {
        format!("CI: {mark}")
    };
    let mut html = format!(
        "<p>🔴 <b>{}</b> · <code>{}</code>{}</p><p>Branch: <code>{}</code>",
        esc(&title),
        esc(name),
        esc(&run),
        esc(branch)
    );
    if !sha.is_empty() {
        html.push_str(&format!("<br>Commit: <code>{}</code>", esc(sha)));
        if !actor.is_empty() {
            html.push_str(&format!(" · {}", esc(actor)));
        }
    }
    html.push_str("</p>");
    if !head.is_empty() {
        html.push_str(&format!("<blockquote>{}</blockquote>", esc(head)));
    }
    html.push_str(&buttons(&[Button {
        label: lang.pick("Open the log", "Logni oçiş").into(),
        url: url.clone(),
        style: Some("danger"),
    }]));
    html.push_str("<footer>🐙 GitHub Actions</footer>");
    Some(Card::new(
        html,
        &format!("🐙 {}", ev.text()),
        &People::new(),
        false,
    ))
}

/// The daily digest card. Every string comes from the model and is escaped.
pub fn daily(
    lang: Lang,
    project: &str,
    day: &str,
    s: &crate::digest::Summary,
    messages: i64,
    events: i64,
    people: &People,
) -> Card {
    let mut html = format!(
        "<p>🗓 <b>{}</b> · {} · {}</p><p><i>{}</i></p>",
        lang.pick("Daily digest", "Kun xulosasi"),
        esc(project),
        esc(day),
        inline(&s.headline, people)
    );
    for (icon, title, items) in [
        ("✅", lang.pick("Done", "Bajarildi"), &s.done),
        ("🔄", lang.pick("In progress", "Jarayonda"), &s.in_progress),
        ("⛔", lang.pick("Blockers", "To'siqlar"), &s.blockers),
        ("➡️", lang.pick("Next", "Keyingi qadamlar"), &s.next),
    ] {
        if items.is_empty() {
            continue;
        }
        html.push_str(&format!("<p>{icon} <b>{title}</b></p><ul>"));
        for i in items {
            html.push_str(&format!("<li>{}</li>", inline(i, people)));
        }
        html.push_str("</ul>");
    }
    let footer = match lang {
        Lang::En => format!("{messages} messages · {events} GitHub events · written by Claude"),
        Lang::Uz => format!("{messages} ta xabar · {events} ta GitHub hodisasi · Claude yozdi"),
    };
    html.push_str(&format!("<footer>{}</footer>", esc(&footer)));
    Card::new(html, &s.text(lang, project, day), people, false)
}

/// A one-line notice as a card (used where a project wants plain messages).
pub fn notice(text: &str, silent: bool) -> Card {
    Card::new(
        format!("<p>{}</p>", esc(text)),
        text,
        &People::new(),
        silent,
    )
}

/// One digest line for a push, PR or passing CI run (HTML).
pub fn digest_line(lang: Lang, ev: &GhEvent) -> String {
    match ev {
        GhEvent::Push {
            branch,
            who,
            commits,
            compare,
            head,
        } => {
            let n = format!("{commits} commit");
            let n = match attr_url(compare) {
                Some(u) => format!("<a href=\"{u}\">{n}</a>"),
                None => n,
            };
            format!(
                "⬆️ <code>{}</code> · {n} · {}<br><i>{}</i>",
                esc(branch),
                esc(who),
                esc(head)
            )
        }
        GhEvent::Pr {
            number,
            verb,
            who,
            title,
            url,
        } => {
            let (icon, word) = match *verb {
                "opened" => ("🟢", lang.pick("opened", "oçildi")),
                "reopened" => ("🟢", lang.pick("reopened", "qayta oçildi")),
                "ready for review" => ("🔵", lang.pick("ready for review", "review'ga tayyor")),
                "merged" => ("🟣", lang.pick("merged", "merge qilindi")),
                _ => ("⚪", lang.pick("closed", "yopildi")),
            };
            let num = match attr_url(url) {
                Some(u) => format!("<a href=\"{u}\">#{number}</a>"),
                None => format!("#{number}"),
            };
            format!(
                "{icon} PR {num} {word} · {}<br><i>{}</i>",
                esc(who),
                esc(title)
            )
        }
        GhEvent::Ci {
            name, branch, url, ..
        } => {
            let n = match attr_url(url) {
                Some(u) => format!("<a href=\"{u}\">{}</a>", esc(name)),
                None => esc(name),
            };
            format!("✅ CI {n} · <code>{}</code>", esc(branch))
        }
    }
}

/// The digest card: a table of recent GitHub events.
/// `lines` are (unix time, HTML) pairs, oldest first.
pub fn digest(lang: Lang, project: &str, lines: &[(i64, String)]) -> Card {
    let mut html = format!(
        "<p>🐙 <b>GitHub</b> · {}</p><table compact striped><tr><th>{}</th><th>{}</th></tr>",
        esc(project),
        lang.pick("Time", "Vaqt"),
        lang.pick("Event", "Hodisa")
    );
    for (unix, line) in lines {
        let hhmm = chrono::DateTime::from_timestamp(*unix, 0)
            .map(|t| t.format("%H:%M UTC").to_string())
            .unwrap_or_default();
        html.push_str(&format!(
            "<tr><td><tg-time unix=\"{unix}\" format=\"t\">{hhmm}</tg-time></td><td>{line}</td></tr>"
        ));
    }
    html.push_str(&format!(
        "</table><footer>{}</footer>",
        lang.pick(
            "New GitHub events within 15 minutes join this message",
            "15 daqiqa içidagi yangi GitHub hodisalari şu xabarga qóşiladi"
        )
    ));
    Card {
        html,
        plain: String::new(),
        entities: vec![],
        silent: true,
    }
}

/// Sends one of each card to `to` (a private chat), so an operator can see
/// how the group will look before switching a project to rich cards.
pub async fn preview(tg: &crate::telegram::Telegram, to: i64) -> anyhow::Result<()> {
    let lang = tg.lang;
    let mut people = People::new();
    people.insert(
        "you".into(),
        TgPerson {
            user_id: to,
            username: None,
            name: "you".into(),
        },
    );
    let agent = Author {
        handle: "lionali-ai",
        is_agent: true,
        display_name: None,
        owner: Some("you"),
    };
    let human = Author {
        handle: "you",
        is_agent: false,
        display_name: Some("You"),
        owner: None,
    };
    let push = GhEvent::Push {
        branch: "MOS-113-i18n-check-windows-paths".into(),
        who: "sherali".into(),
        head: "MOS-113: i18n-lang-check normalises paths to /".into(),
        commits: 3,
        compare: "https://github.com/example/repo/compare/56da94b...a3f9c21".into(),
    };
    let pr = GhEvent::Pr {
        number: 40,
        verb: "opened",
        who: "sherali".into(),
        title: "MOS-113: i18n-lang-check on Windows".into(),
        url: "https://github.com/example/repo/pull/40".into(),
    };
    let ci = GhEvent::Ci {
        name: "ci".into(),
        mark: "FAILED".into(),
        branch: "MOS-113-i18n-check-windows-paths".into(),
        url: "https://github.com/example/repo/actions/runs/1".into(),
        run_number: Some(214),
        sha: "a3f9c21".into(),
        actor: "sherali".into(),
        head: "MOS-113: i18n-lang-check normalises paths to /".into(),
    };
    let now = chrono::Utc::now().timestamp();
    let cards = [
        post(
            lang,
            &agent,
            "Yangiliklar: 1) PR #37 (MOS-052) merge bo'ldi, CI 20/20. `DATA_MASTER_KEYS` qo'shildi, @gulom-ai rahmat. 2) PR #39 ochildi: MOS-052 ni done qilish. 3) MOS-113 ni olaman, raqamni band qilmanglar.",
            &people,
        ),
        post(lang, &human, "@lionali-ai rahmat 👍 MOS-113 seniki. @you, kalitni 1Password'ga ham qo'sh.", &people),
        digest(lang, "mosla", &[(now - 120, digest_line(lang, &push)), (now - 60, digest_line(lang, &pr))]),
        ci_failed(lang, &ci).expect("a CI event"),
        handoff_new(
            lang,
            12,
            Some("MOS-113"),
            "lionali-ai",
            &["gulom-ai".to_string()],
            "Windows yo'l tuzatishini review qilib, staging'da i18n-lang-check'ni bir marta ishga tushir.\nPR #40. Natija 0 bo'lishi kerak.",
            &people,
        ),
        handoff_done(lang, 12, Some("MOS-113"), "gulom-ai", "staging'da 0 ta topilma, PR #40 approve qilindi.", &people),
        joined(lang, "you", "maintainer", &people),
        question(
            lang,
            7,
            "lionali-ai",
            "you",
            "PR #39 ni hozir merge qilaymi yoki staging smoke'ni kutaymi?",
            &["Hozir merge qil".into(), "Smoke'ni kut".into()],
            &people,
        ),
    ];
    for card in &cards {
        tg.send_card(to, card, true, None, None, None).await?;
    }
    println!("sent {} sample cards", cards.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> People {
        let mut p = People::new();
        p.insert(
            "imbakirov".into(),
            TgPerson {
                user_id: 953176420,
                username: Some("imbakirov".into()),
                name: "G'ulom".into(),
            },
        );
        p
    }

    #[test]
    fn escapes_and_formats_inline() {
        let p = people();
        assert_eq!(esc("a<b>&\"c\""), "a&lt;b&gt;&amp;&quot;c&quot;");
        let s = inline(
            "@gulom-ai and @imbakirov: MOS-113 in `a<b>`, see https://x.io/p?a=1&b=2.",
            &p,
        );
        assert!(s.contains("<code>@gulom-ai</code>"), "{s}");
        assert!(
            s.contains("<a href=\"tg://user?id=953176420\">@imbakirov</a>"),
            "{s}"
        );
        assert!(s.contains("<b>MOS-113</b>"), "{s}");
        assert!(s.contains("<code>a&lt;b&gt;</code>"), "{s}");
        assert!(s.contains("<a href=\"https://x.io/p?a=1&amp;b=2\">"), "{s}");
        assert!(s.ends_with("</a>."), "trailing dot stays text: {s}");
        assert_eq!(inline("mail a@b.uz", &p), "mail a@b.uz");
        assert_eq!(
            inline("thanks @gulom-ai.", &p),
            "thanks <code>@gulom-ai</code>."
        );
        assert_eq!(inline("<script>", &p), "&lt;script&gt;");
    }

    #[test]
    fn bodies_get_lists_and_paragraphs() {
        let p = People::new();
        let b = body(
            "Yangiliklar: 1) PR #37 merge 2) PR #39 ochildi 3) MOS-113 ni olaman",
            &p,
        );
        assert_eq!(
            b,
            "<p>Yangiliklar:</p><ol><li>PR #37 merge</li><li>PR #39 ochildi</li><li><b>MOS-113</b> ni olaman</li></ol>"
        );
        assert_eq!(body("a\nb\n\nc", &p), "<p>a<br>b</p><p>c</p>");
        assert_eq!(
            body("Plan:\n- one\n- two", &p),
            "<p>Plan:</p><ul><li>one</li><li>two</li></ul>"
        );
        // Numbers that are not a 1..n list stay text.
        assert_eq!(
            body("version 2) and 5) later", &p),
            "<p>version 2) and 5) later</p>"
        );
    }

    #[test]
    fn cards_have_html_and_plain() {
        let p = people();
        let a = Author {
            handle: "lionali-ai",
            is_agent: true,
            display_name: None,
            owner: Some("ali_9737"),
        };
        let c = post(Lang::Uz, &a, "salom @imbakirov", &p);
        assert!(c
            .html
            .starts_with("<p>🤖 <b>lionali-ai</b> · <i>ali_9737 agenti</i></p>"));
        assert_eq!(c.plain, "🤖 lionali-ai\nsalom @imbakirov");
        let ci = GhEvent::Ci {
            name: "ci".into(),
            mark: "FAILED".into(),
            branch: "main".into(),
            url: "https://github.com/x/y/actions/runs/1".into(),
            run_number: Some(214),
            sha: "a3f9c21".into(),
            actor: "sherali".into(),
            head: "fix <paths>".into(),
        };
        let card = ci_failed(Lang::Uz, &ci).unwrap();
        assert!(card.html.contains("CI yiqildi") && card.html.contains("fix &lt;paths&gt;"));
        assert!(card
            .html
            .contains("style=\"danger\" url=\"https://github.com/x/y/actions/runs/1\""));
        assert!(!card.silent);
        let d = digest(
            Lang::En,
            "mosla",
            &[(1794555900, digest_line(Lang::En, &ci))],
        );
        assert!(d.silent && d.html.contains("<tg-time unix=\"1794555900\""));
    }
}
