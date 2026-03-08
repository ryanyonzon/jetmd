// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Cache-backed draft autosave support.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::xdg::{AppDirectories, save_json};

static DRAFT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[derive(Debug)]
pub enum AutosaveError {
    Io(io::Error),
}

impl std::fmt::Display for AutosaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutosaveError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for AutosaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AutosaveError::Io(error) => Some(error),
        }
    }
}

impl From<io::Error> for AutosaveError {
    fn from(value: io::Error) -> Self {
        AutosaveError::Io(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DraftManifest {
    drafts: Vec<DraftEntry>,
}

impl Default for DraftManifest {
    fn default() -> Self {
        Self { drafts: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DraftEntry {
    draft_id: String,
    original_path: Option<PathBuf>,
    draft_path: PathBuf,
    updated_unix_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredDraft {
    pub draft_id: String,
    pub file_path: Option<PathBuf>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct AutosaveManager {
    drafts_dir: PathBuf,
    manifest_path: PathBuf,
}

impl AutosaveManager {
    pub fn new(dirs: &AppDirectories) -> Self {
        Self {
            drafts_dir: dirs.drafts_dir.clone(),
            manifest_path: dirs.draft_manifest_path(),
        }
    }

    pub fn save_draft(
        &self,
        draft_id: Option<&str>,
        original_path: Option<&Path>,
        content: &str,
    ) -> Result<String, AutosaveError> {
        fs::create_dir_all(&self.drafts_dir)?;

        let draft_id = draft_id
            .map(str::to_owned)
            .unwrap_or_else(generate_draft_id);
        let draft_path = self.drafts_dir.join(format!("{draft_id}.md"));
        fs::write(&draft_path, content.as_bytes())?;

        let mut manifest = self.load_manifest()?;
        manifest.drafts.retain(|entry| entry.draft_id != draft_id);
        manifest.drafts.push(DraftEntry {
            draft_id: draft_id.clone(),
            original_path: original_path.map(Path::to_path_buf),
            draft_path,
            updated_unix_secs: unix_now_secs(),
        });
        manifest
            .drafts
            .sort_by(|left, right| right.updated_unix_secs.cmp(&left.updated_unix_secs));
        save_json(&self.manifest_path, &manifest)?;

        Ok(draft_id)
    }

    pub fn discard_draft(&self, draft_id: Option<&str>) -> Result<(), AutosaveError> {
        let Some(draft_id) = draft_id else {
            return Ok(());
        };

        let mut manifest = self.load_manifest()?;
        let mut removed_path = None;
        manifest.drafts.retain(|entry| {
            let keep = entry.draft_id != draft_id;
            if !keep {
                removed_path = Some(entry.draft_path.clone());
            }
            keep
        });

        if let Some(path) = removed_path {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }

        save_json(&self.manifest_path, &manifest)?;
        Ok(())
    }

    pub fn restore_drafts(&self) -> Result<Vec<RestoredDraft>, AutosaveError> {
        let manifest = self.load_manifest()?;
        let mut restored = Vec::new();
        let mut retained_entries = Vec::new();

        for entry in manifest.drafts {
            match fs::read_to_string(&entry.draft_path) {
                Ok(content) => {
                    retained_entries.push(entry.clone());
                    restored.push(RestoredDraft {
                        draft_id: entry.draft_id,
                        file_path: entry.original_path,
                        content,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }

        if retained_entries.len() != restored.len() {
            save_json(
                &self.manifest_path,
                &DraftManifest {
                    drafts: retained_entries,
                },
            )?;
        }

        Ok(restored)
    }

    fn load_manifest(&self) -> Result<DraftManifest, AutosaveError> {
        if !self.manifest_path.exists() {
            return Ok(DraftManifest::default());
        }

        let content = fs::read_to_string(&self.manifest_path)?;
        serde_json::from_str(&content)
            .map_err(invalid_data)
            .map_err(AutosaveError::from)
    }
}

fn generate_draft_id() -> String {
    let counter = DRAFT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("draft-{}-{nanos}-{counter}", std::process::id())
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xdg::AppDirectories;

    #[test]
    fn save_restore_and_discard_draft_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let dirs = AppDirectories::from_base_paths(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
        );
        dirs.ensure_exists().unwrap();
        let manager = AutosaveManager::new(&dirs);

        let draft_id = manager
            .save_draft(None, Some(Path::new("notes.md")), "draft body")
            .unwrap();

        let restored = manager.restore_drafts().unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].draft_id, draft_id);
        assert_eq!(restored[0].content, "draft body");

        manager.discard_draft(Some(&draft_id)).unwrap();
        assert!(manager.restore_drafts().unwrap().is_empty());
    }
}
