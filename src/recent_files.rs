// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Persistent recent-files storage.

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::xdg::{AppDirectories, save_json};

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentFilesStore {
    files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct RecentFilesManager {
    path: PathBuf,
}

impl RecentFilesManager {
    pub fn new(dirs: &AppDirectories) -> Self {
        Self {
            path: dirs.recent_files_path(),
        }
    }

    pub fn load(&self) -> io::Result<Vec<PathBuf>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&self.path)?;
        let mut store: RecentFilesStore = serde_json::from_str(&content)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        dedupe_and_truncate(&mut store.files);
        Ok(store.files)
    }

    pub fn save(&self, files: &[PathBuf]) -> io::Result<()> {
        let mut normalized = files.to_vec();
        dedupe_and_truncate(&mut normalized);
        save_json(&self.path, &RecentFilesStore { files: normalized })
    }
}

fn dedupe_and_truncate(files: &mut Vec<PathBuf>) {
    let mut deduped = Vec::with_capacity(files.len());
    for path in files.drain(..) {
        if !deduped.iter().any(|existing| existing == &path) {
            deduped.push(path);
        }
    }
    deduped.truncate(10);
    *files = deduped;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xdg::AppDirectories;

    #[test]
    fn save_and_load_normalize_recent_files() {
        let temp = tempfile::tempdir().unwrap();
        let dirs = AppDirectories::from_base_paths(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
        );
        dirs.ensure_exists().unwrap();
        let manager = RecentFilesManager::new(&dirs);
        let files = vec![
            PathBuf::from("a.md"),
            PathBuf::from("b.md"),
            PathBuf::from("a.md"),
        ];

        manager.save(&files).unwrap();

        assert_eq!(
            manager.load().unwrap(),
            vec![PathBuf::from("a.md"), PathBuf::from("b.md")]
        );
    }
}
