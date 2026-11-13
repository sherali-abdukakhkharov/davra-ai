//! Keeps a repository's handoff files and the hub in step.
//!
//! The files in git are the source of truth. `sync` sends only files whose
//! content changed since the last sync (hashes are cached in `~/.davra/sync`),
//! so running it on every session start is cheap.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use davra_core::api::{HandoffSync, HandoffSyncItem, HandoffSyncResult};
use davra_core::handoff_file::{self, HandoffFile};
use davra_core::project_file;
use sha2::{Digest, Sha256};

use crate::client::Client;
use crate::config;

/// A repository that keeps handoff files (its `.davra.toml` has `handoffs = "..."`).
pub struct Repo {
    pub root: PathBuf,
    pub dir: PathBuf,
    /// The handoffs directory relative to the root, forward slashes.
    pub rel_dir: String,
    pub project: String,
}

pub fn repo(cwd: &Path) -> Option<Repo> {
    let (file, pf) = project_file::find(cwd)?;
    let rel = pf.handoffs?.trim().trim_matches('/').replace('\\', "/");
    if rel.is_empty() || rel.contains("..") {
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

fn hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn cache_path(hub: &str, project: &str) -> Result<PathBuf> {
    let host: String = hub
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    Ok(config::dir()?
        .join("sync")
        .join(format!("{host}__{project}.json")))
}

/// Parsed handoff files: (relative path, hash, parsed).
pub fn scan(repo: &Repo) -> Result<Vec<(String, String, HandoffFile)>> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&repo.dir) {
        Ok(e) => e,
        Err(_) => return Ok(out),
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if !name.ends_with(".md") || name.eq_ignore_ascii_case("README.md") || !path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(h) = handoff_file::parse(&text) {
            out.push((format!("{}/{}", repo.rel_dir, name), hash(&text), h));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Sends changed handoff files. `announce = None` means: announce unless this
/// is the first sync for this hub + project (a quiet initial import).
pub async fn sync(
    client: &Client,
    hub: &str,
    repo: &Repo,
    announce: Option<bool>,
) -> Result<HandoffSyncResult> {
    let cache_file = cache_path(hub, &repo.project)?;
    let first = !cache_file.exists();
    let mut cache: BTreeMap<String, String> = if first {
        BTreeMap::new()
    } else {
        serde_json::from_str(&std::fs::read_to_string(&cache_file)?).unwrap_or_default()
    };
    let announce = announce.unwrap_or(!first);

    let changed: Vec<HandoffSyncItem> = scan(repo)?
        .into_iter()
        .filter(|(p, h, _)| cache.get(p) != Some(h))
        .map(|(source_path, source_hash, f)| HandoffSyncItem {
            source_path,
            source_hash,
            from: f.from,
            to: f.to,
            ticket: f.ticket,
            status: f.status,
            created: f.created,
            body: f.body,
        })
        .collect();

    let mut total = HandoffSyncResult::default();
    for chunk in changed.chunks(25) {
        let r = client
            .handoff_sync(
                &repo.project,
                &HandoffSync {
                    items: chunk.to_vec(),
                    announce,
                },
            )
            .await?;
        for item in chunk {
            cache.insert(item.source_path.clone(), item.source_hash.clone());
        }
        total.created += r.created;
        total.updated += r.updated;
        total.closed += r.closed;
        total.reopened += r.reopened;
        total.unchanged += r.unchanged;
        total.touched.extend(r.touched);
    }
    std::fs::create_dir_all(cache_file.parent().expect("cache has a parent"))?;
    std::fs::write(&cache_file, serde_json::to_string(&cache)?)?;
    Ok(total)
}

/// Writes a new handoff file and returns its path relative to the root.
pub fn write_new(repo: &Repo, h: &HandoffFile) -> Result<String> {
    let date = h
        .created
        .clone()
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let base = handoff_file::file_name(&date, &h.from, &h.to, h.ticket.as_deref());
    std::fs::create_dir_all(&repo.dir)
        .with_context(|| format!("creating {}", repo.dir.display()))?;
    let mut name = base.clone();
    let mut n = 2;
    while repo.dir.join(&name).exists() {
        name = base.replace(".md", &format!("-{n}.md"));
        n += 1;
    }
    std::fs::write(repo.dir.join(&name), handoff_file::render(h))?;
    Ok(format!("{}/{}", repo.rel_dir, name))
}

/// Marks the file done in place. The path must be inside the handoffs dir.
pub fn mark_file_done(repo: &Repo, rel_path: &str, note: &str) -> Result<()> {
    let Some(name) = rel_path.strip_prefix(&format!("{}/", repo.rel_dir)) else {
        bail!("{rel_path} is not in {}", repo.rel_dir);
    };
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        bail!("unexpected handoff path {rel_path}");
    }
    let path = repo.dir.join(name);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let done = handoff_file::mark_done(&text, note)
        .with_context(|| format!("{rel_path} is not a handoff file"))?;
    std::fs::write(&path, done)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo() -> Repo {
        let root =
            std::env::temp_dir().join(format!("davra-ho-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(root.join("docs/memory/handoffs")).unwrap();
        std::fs::write(
            root.join(".davra.toml"),
            "project = \"atlas\"\nhandoffs = \"docs/memory/handoffs\"\n",
        )
        .unwrap();
        repo(&root).unwrap()
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    #[test]
    fn write_scan_and_close() {
        let r = temp_repo();
        let h = HandoffFile {
            from: "ui-kit".into(),
            to: vec!["architect".into()],
            ticket: Some("ATL-032".into()),
            status: "open".into(),
            created: Some("2026-11-02".into()),
            body: "What changed: x\nDone when: y".into(),
        };
        let p = write_new(&r, &h).unwrap();
        assert_eq!(
            p,
            "docs/memory/handoffs/2026-11-02-ui-kit-to-architect-ATL-032.md"
        );
        // Same name again gets a suffix instead of overwriting.
        let p2 = write_new(&r, &h).unwrap();
        assert!(p2.ends_with("ATL-032-2.md"));
        std::fs::write(r.dir.join("README.md"), "# not a handoff").unwrap();
        let found = scan(&r).unwrap();
        assert_eq!(found.len(), 2);
        mark_file_done(&r, &p, "done it").unwrap();
        let found = scan(&r).unwrap();
        let closed = found.iter().find(|f| f.0 == p).unwrap();
        assert_eq!(closed.2.status, "done");
        assert!(mark_file_done(&r, "docs/memory/handoffs/../../x.md", "n").is_err());
        std::fs::remove_dir_all(&r.root).ok();
    }
}
