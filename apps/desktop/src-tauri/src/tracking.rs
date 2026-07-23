use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use releasedock_core::manifest::ManifestStore;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackedRepo {
    pub repo_id: String,
}

#[derive(Debug, Default)]
pub struct TrackedRepoStore {
    path: PathBuf,
}

impl TrackedRepoStore {
    pub fn default() -> Result<Self> {
        Ok(Self {
            path: default_path()?,
        })
    }

    pub fn load(&self) -> Result<Vec<TrackedRepo>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read tracked repo store {}", self.path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse tracked repo store {}", self.path.display()))
    }

    pub fn seed_if_missing(&self, repo_ids: &[&str]) -> Result<()> {
        if self.path.exists() {
            return Ok(());
        }

        let repos = repo_ids
            .iter()
            .map(|repo_id| TrackedRepo {
                repo_id: (*repo_id).to_string(),
            })
            .collect::<Vec<_>>();
        if repos.is_empty() {
            return Ok(());
        }

        self.save(&repos)
    }

    pub fn upsert(&self, repo_id: &str) -> Result<()> {
        let mut repos = self.load()?;
        if repos.iter().any(|repo| repo.repo_id == repo_id) {
            return Ok(());
        }

        repos.push(TrackedRepo {
            repo_id: repo_id.to_string(),
        });
        self.save(&repos)
    }

    pub fn remove(&self, repo_id: &str) -> Result<bool> {
        let mut repos = self.load()?;
        let original_len = repos.len();
        repos.retain(|repo| repo.repo_id != repo_id);
        if repos.len() == original_len {
            return Ok(false);
        }

        self.save(&repos)?;
        Ok(true)
    }

    pub fn remove_many(&self, repo_ids: &[String]) -> Result<Vec<String>> {
        let wanted: HashSet<&str> = repo_ids.iter().map(|repo_id| repo_id.as_str()).collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }

        let mut repos = self.load()?;
        let mut removed = Vec::new();
        repos.retain(|repo| {
            if wanted.contains(repo.repo_id.as_str()) {
                removed.push(repo.repo_id.clone());
                false
            } else {
                true
            }
        });

        if !removed.is_empty() {
            self.save(&repos)?;
        }

        Ok(removed)
    }

    fn save(&self, repos: &[TrackedRepo]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create tracked repo directory {}", parent.display())
            })?;
        }

        let temp_path = self.path.with_extension("json.tmp");
        let content =
            serde_json::to_string_pretty(repos).context("failed to serialize tracked repos")?;
        fs::write(&temp_path, content).with_context(|| {
            format!("failed to write temporary tracked repo store {}", temp_path.display())
        })?;
        fs::rename(&temp_path, &self.path)
            .with_context(|| format!("failed to replace tracked repo store {}", self.path.display()))?;
        Ok(())
    }
}

fn default_path() -> Result<PathBuf> {
    let manifest_path = ManifestStore::default_path()?;
    Ok(manifest_path.with_file_name("tracked_repos.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_default_repos_only_when_missing() {
        let temp = tempfile::tempdir().unwrap();
        let store = TrackedRepoStore {
            path: temp.path().join("tracked.json"),
        };

        store.seed_if_missing(&["owner/project"]).unwrap();
        let repos = store.load().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].repo_id, "owner/project");

        store.seed_if_missing(&["other/project"]).unwrap();
        let repos = store.load().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].repo_id, "owner/project");
    }

    #[test]
    fn removes_many_tracked_repos_in_one_pass() {
        let temp = tempfile::tempdir().unwrap();
        let store = TrackedRepoStore {
            path: temp.path().join("tracked.json"),
        };

        store
            .seed_if_missing(&["owner/one", "owner/two", "owner/three"])
            .unwrap();
        let removed = store
            .remove_many(&["owner/one".to_string(), "owner/three".to_string()])
            .unwrap();

        assert_eq!(removed, vec!["owner/one".to_string(), "owner/three".to_string()]);
        let repos = store.load().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].repo_id, "owner/two");
    }
}
