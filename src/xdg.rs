// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! XDG / platform app-directory support.
//!
//! Uses the `directories` crate so Linux follows the XDG Base Directory
//! Specification while other platforms resolve to their native locations.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[derive(Debug, Clone)]
pub struct AppDirectories {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub themes_dir: PathBuf,
    pub drafts_dir: PathBuf,
}

impl AppDirectories {
    pub fn resolve() -> io::Result<Self> {
        let project_dirs = ProjectDirs::from("io", "github.ryanyonzon", "jetmd")
            .ok_or_else(|| io::Error::other("Could not resolve application directories"))?;

        let dirs = Self::from_base_paths(
            project_dirs.config_dir().to_path_buf(),
            project_dirs.data_dir().to_path_buf(),
            project_dirs.cache_dir().to_path_buf(),
        );
        dirs.ensure_exists()?;
        Ok(dirs)
    }

    pub fn from_base_paths(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            themes_dir: data_dir.join("themes"),
            drafts_dir: cache_dir.join("drafts"),
            config_dir,
            data_dir,
            cache_dir,
        }
    }

    pub fn ensure_exists(&self) -> io::Result<()> {
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.themes_dir,
            &self.drafts_dir,
        ] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn recent_files_path(&self) -> PathBuf {
        self.data_dir.join("recent_files.json")
    }

    pub fn draft_manifest_path(&self) -> PathBuf {
        self.data_dir.join("draft_manifest.json")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub theme: String,
    pub auto_save_enabled: bool,
    /// Preview theme name (directory name under `themes/`).
    #[serde(default = "default_preview_theme")]
    pub preview_theme: String,
}

fn default_preview_theme() -> String {
    "default".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            auto_save_enabled: false,
            preview_theme: default_preview_theme(),
        }
    }
}

pub fn load_app_config(dirs: &AppDirectories) -> io::Result<AppConfig> {
    load_json_or_default(&dirs.config_path())
}

pub fn save_app_config(dirs: &AppDirectories, config: &AppConfig) -> io::Result<()> {
    save_json(&dirs.config_path(), config)
}

fn load_json_or_default<T>(path: &Path) -> io::Result<T>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }

    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(invalid_data)
}

pub fn save_json<T>(path: &Path, value: &T) -> io::Result<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_vec_pretty(value).map_err(invalid_data)?;
    fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_paths_are_created_under_base_directories() {
        let temp = tempfile::tempdir().unwrap();
        let dirs = AppDirectories::from_base_paths(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
        );

        dirs.ensure_exists().unwrap();

        assert!(dirs.config_dir.is_dir());
        assert!(dirs.data_dir.is_dir());
        assert!(dirs.cache_dir.is_dir());
        assert!(dirs.themes_dir.is_dir());
        assert!(dirs.drafts_dir.is_dir());
    }
}
