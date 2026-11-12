//! `.davra.toml`: committed to a repository so agents know which project
//! they are working in. It never contains secrets.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = ".davra.toml";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectFile {
    /// Hub base URL, e.g. `https://davra.example.uz`.
    pub hub: Option<String>,
    pub org: Option<String>,
    pub project: String,
    /// Where the repository keeps handoff files (Mosla convention).
    pub handoffs: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
}

impl ProjectFile {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}

/// Walks up from `start` looking for `.davra.toml`.
pub fn find(start: &Path) -> Option<(PathBuf, ProjectFile)> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(FILE_NAME);
        if candidate.is_file() {
            let text = std::fs::read_to_string(&candidate).ok()?;
            return ProjectFile::parse(&text).ok().map(|pf| (candidate, pf));
        }
        dir = d.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample() {
        let pf = ProjectFile::parse(
            r#"
hub      = "https://davra.example.uz"
org      = "shaxsiy"
project  = "mosla"
handoffs = "docs/memory/handoffs"
topics   = ["general", "handoffs"]
"#,
        )
        .unwrap();
        assert_eq!(pf.project, "mosla");
        assert_eq!(pf.topics.len(), 2);
    }

    #[test]
    fn finds_upwards() {
        let root = std::env::temp_dir().join(format!("davra-pf-{}", std::process::id()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join(FILE_NAME), "project = \"mosla\"\n").unwrap();
        let (path, pf) = find(&nested).unwrap();
        assert_eq!(pf.project, "mosla");
        assert_eq!(path, root.join(FILE_NAME));
        std::fs::remove_dir_all(&root).ok();
    }
}
