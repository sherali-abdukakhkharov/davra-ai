//! Decision files in the repository (docs/memory/decisions by default).
//!
//! The hub records a decision first; the client then writes its file, so the
//! file carries the hub's id, and reports the path back. A decision that
//! replaces another marks the old file superseded.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use davra_core::api::DecisionView;
use davra_core::{decision_file, project_file};

pub const DEFAULT_DIR: &str = "docs/memory/decisions";

pub struct Repo {
    pub root: PathBuf,
    pub dir: PathBuf,
    /// The decisions directory relative to the root, forward slashes.
    pub rel_dir: String,
    pub project: String,
}

/// The repository's decisions folder: `decisions` in `.davra.toml`, else next
/// to the handoffs folder, else docs/memory/decisions.
pub fn repo(cwd: &Path) -> Option<Repo> {
    let (file, pf) = project_file::find(cwd)?;
    let rel = match (pf.decisions.as_deref(), pf.handoffs.as_deref()) {
        (Some(d), _) => d.to_string(),
        (None, Some(h)) => {
            let h = h.trim().trim_end_matches(['/', '\\']).replace('\\', "/");
            match h.rsplit_once('/') {
                Some((parent, _)) => format!("{parent}/decisions"),
                None => "decisions".to_string(),
            }
        }
        (None, None) => DEFAULT_DIR.to_string(),
    };
    let rel = rel.trim().trim_matches('/').replace('\\', "/");
    if rel.is_empty() || rel.split('/').any(|s| s == "..") {
        return None;
    }
    let root = file.parent()?.to_path_buf();
    Some(Repo {
        dir: root.join(&rel),
        root,
        rel_dir: rel,
        project: pf.project,
    })
}

/// Writes the decision's file and returns its path relative to the root.
pub fn write(repo: &Repo, d: &DecisionView) -> Result<String> {
    let date = d
        .created_at
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string();
    let base = decision_file::file_name(&date, &d.title, d.ticket.as_deref());
    std::fs::create_dir_all(&repo.dir)
        .with_context(|| format!("creating {}", repo.dir.display()))?;
    let mut name = base.clone();
    let mut n = 2;
    while repo.dir.join(&name).exists() {
        name = base.replace(".md", &format!("-{n}.md"));
        n += 1;
    }
    std::fs::write(repo.dir.join(&name), decision_file::render(d))?;
    Ok(format!("{}/{}", repo.rel_dir, name))
}

/// Marks an existing decision file superseded by decision `by`. Returns false
/// when the file isn't in this checkout.
pub fn mark_superseded(repo: &Repo, rel_path: &str, by: i64) -> Result<bool> {
    let rel = rel_path.replace('\\', "/");
    if rel.starts_with('/') || rel.split('/').any(|s| s == ".." || s.is_empty()) {
        bail!("unexpected decision path {rel_path}");
    }
    let path = repo.root.join(&rel);
    if !path.is_file() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(&path)?;
    let Some(updated) = decision_file::mark_superseded(&text, by) else {
        bail!("{rel_path} is not a decision file");
    };
    std::fs::write(&path, updated)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn decision(id: i64, supersedes: Option<i64>) -> DecisionView {
        DecisionView {
            id,
            ticket: Some("ATL-040".into()),
            title: "Cursor paging for contacts".into(),
            decision: "Contacts page with a cursor.".into(),
            why: "Offsets skip rows when contacts are added.".into(),
            instead: None,
            decided_by: "frontend".into(),
            author: Some("frontend-ai".into()),
            status: "accepted".into(),
            supersedes,
            superseded_by: None,
            source_message: None,
            file_path: None,
            message_id: None,
            created_at: chrono::Utc.with_ymd_and_hms(2026, 11, 17, 9, 0, 0).unwrap(),
        }
    }

    struct Checkout(PathBuf);

    impl Checkout {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Checkout {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn checkout(toml: &str) -> Checkout {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "davra-dec-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".davra.toml"), toml).unwrap();
        Checkout(root)
    }

    #[test]
    fn folder_defaults() {
        let t = checkout("project = \"atlas\"\nhandoffs = \"docs/memory/handoffs\"\n");
        assert_eq!(repo(t.path()).unwrap().rel_dir, "docs/memory/decisions");
        let t = checkout("project = \"atlas\"\n");
        assert_eq!(repo(t.path()).unwrap().rel_dir, DEFAULT_DIR);
        let t = checkout("project = \"atlas\"\ndecisions = \"adr\"\n");
        assert_eq!(repo(t.path()).unwrap().rel_dir, "adr");
        let t = checkout("project = \"atlas\"\ndecisions = \"../x\"\n");
        assert!(repo(t.path()).is_none());
    }

    #[test]
    fn write_then_supersede() {
        let t = checkout("project = \"atlas\"\n");
        let r = repo(t.path()).unwrap();
        let p1 = write(&r, &decision(3, None)).unwrap();
        assert!(
            p1.starts_with("docs/memory/decisions/")
                && p1.ends_with("-cursor-paging-for-contacts-ATL-040.md")
        );
        // Same day, same title: a second file, not an overwrite.
        let p2 = write(&r, &decision(5, Some(3))).unwrap();
        assert_ne!(p1, p2);
        assert!(mark_superseded(&r, &p1, 5).unwrap());
        let old = std::fs::read_to_string(t.path().join(&p1)).unwrap();
        assert!(old.contains("status: superseded\n") && old.contains("superseded_by: 5\n"));
        assert!(!mark_superseded(&r, "docs/memory/decisions/missing.md", 5).unwrap());
        assert!(mark_superseded(&r, "../etc/passwd.md", 5).is_err());
    }
}
