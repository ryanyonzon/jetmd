// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Preview theme management.
//!
//! Themes are **pure CSS files** that style the rendered Markdown preview
//! (WebKitGTK).  They do **not** affect the GTK / libadwaita application UI.
//!
//! # Directory layout
//!
//! ```text
//! ~/.local/share/jetmd/themes/
//!     default/
//!         theme.css
//!         meta.json   (optional — reserved for future use)
//!     dark/
//!         theme.css
//!     light/
//!         theme.css
//!     my-custom-theme/
//!         theme.css
//! ```
//!
//! # CSS class contract
//!
//! All rendered Markdown is wrapped in `<div class="jetmd-preview">` and
//! every element carries a stable `.md-*` class (see [`crate::markdown`]).
//! Theme authors should target these classes exclusively.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Built-in theme CSS (embedded at compile time)
// ---------------------------------------------------------------------------

/// The **default** theme — clean, minimal, neutral.  Optimised for
/// readability with balanced contrast in both light and dark contexts.
const DEFAULT_THEME_CSS: &str = include_str!("themes/default.css");

/// The **light** theme — bright background, subtle contrast, inspired by
/// GitHub-flavoured Markdown.
const LIGHT_THEME_CSS: &str = include_str!("themes/light.css");

/// The **dark** theme — dark background, accessible contrast, optimised
/// for long reading sessions.
const DARK_THEME_CSS: &str = include_str!("themes/dark.css");

/// Name used when a requested theme cannot be found.
pub const DEFAULT_THEME_NAME: &str = "default";

// ---------------------------------------------------------------------------
// ThemeInfo — metadata about a single theme
// ---------------------------------------------------------------------------

/// Optional metadata stored in `meta.json` alongside a theme's CSS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeMeta {
    /// Human-readable display name.
    #[serde(default)]
    pub name: String,
    /// Theme author.
    #[serde(default)]
    pub author: String,
    /// Semantic version string.
    #[serde(default)]
    pub version: String,
    /// Short description.
    #[serde(default)]
    pub description: String,
}

/// A loaded theme ready for injection into the preview.
#[derive(Debug, Clone)]
pub struct ThemeInfo {
    /// Filesystem directory name (used as the unique identifier).
    #[allow(dead_code)]
    pub id: String,
    /// The CSS content to inject.
    pub css: String,
    /// Optional metadata from `meta.json`.
    #[allow(dead_code)]
    pub meta: Option<ThemeMeta>,
}

// ---------------------------------------------------------------------------
// ThemeManager — loads, caches and serves themes
// ---------------------------------------------------------------------------

/// Discovers and caches preview themes from the user's data directory.
///
/// Built-in themes (`default`, `light`, `dark`) are **always** available
/// even if the on-disk files are missing or corrupted — the compiled-in
/// CSS is used as a fallback.
#[derive(Debug)]
pub struct ThemeManager {
    /// Themes keyed by directory name, sorted alphabetically.
    themes: BTreeMap<String, ThemeInfo>,
    /// Root themes directory (`~/.local/share/jetmd/themes`).
    themes_dir: PathBuf,
}

impl ThemeManager {
    /// Create a new manager, seed built-in themes to disk, then discover
    /// all themes (built-in + user-installed).
    pub fn new(themes_dir: &Path) -> Self {
        // Ensure the themes directory exists.
        let _ = fs::create_dir_all(themes_dir);

        // Seed built-in themes — writes them only when they are missing so
        // that user edits to the on-disk copies are preserved.
        seed_builtin_theme(themes_dir, "default", DEFAULT_THEME_CSS);
        seed_builtin_theme(themes_dir, "light", LIGHT_THEME_CSS);
        seed_builtin_theme(themes_dir, "dark", DARK_THEME_CSS);

        let mut mgr = Self {
            themes: BTreeMap::new(),
            themes_dir: themes_dir.to_path_buf(),
        };
        mgr.reload();
        mgr
    }

    /// Re-scan the themes directory and rebuild the internal cache.
    ///
    /// Built-in themes are always added from compiled-in CSS first, so
    /// they survive even if the on-disk copy is deleted or corrupted.
    pub fn reload(&mut self) {
        self.themes.clear();

        // 1. Seed hard-coded built-ins (guaranteed baseline).
        self.insert_builtin("default", DEFAULT_THEME_CSS);
        self.insert_builtin("light", LIGHT_THEME_CSS);
        self.insert_builtin("dark", DARK_THEME_CSS);

        // 2. Scan the filesystem — on-disk copies override the builtins
        //    so that user customisations take effect.
        if let Ok(entries) = fs::read_dir(&self.themes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                let css_path = path.join("theme.css");
                match fs::read_to_string(&css_path) {
                    Ok(css) => {
                        let meta = load_meta(&path);
                        self.themes.insert(
                            dir_name.to_string(),
                            ThemeInfo {
                                id: dir_name.to_string(),
                                css,
                                meta,
                            },
                        );
                    }
                    Err(e) => {
                        eprintln!("theme: failed to read {}: {e}", css_path.display());
                        // If it's a built-in name the compiled-in version
                        // is already in the map — no action needed.
                    }
                }
            }
        }
    }

    /// Get a theme by name.  Falls back to `default` if not found, then
    /// to the compiled-in default if even that is somehow missing.
    pub fn get(&self, name: &str) -> &ThemeInfo {
        self.themes
            .get(name)
            .or_else(|| self.themes.get(DEFAULT_THEME_NAME))
            .expect("built-in default theme is always present")
    }

    /// Get the CSS for a theme (convenience wrapper).
    pub fn css_for(&self, name: &str) -> &str {
        &self.get(name).css
    }

    /// List all available theme names, sorted alphabetically.
    pub fn available_themes(&self) -> Vec<&str> {
        self.themes.keys().map(String::as_str).collect()
    }

    // -- internal helpers ---------------------------------------------------

    fn insert_builtin(&mut self, id: &str, css: &str) {
        self.themes.insert(
            id.to_string(),
            ThemeInfo {
                id: id.to_string(),
                css: css.to_string(),
                meta: None,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// File-system helpers
// ---------------------------------------------------------------------------

/// Write a built-in theme to disk if it does not already exist.
fn seed_builtin_theme(themes_dir: &Path, name: &str, css: &str) {
    let dir = themes_dir.join(name);
    let css_path = dir.join("theme.css");
    if css_path.exists() {
        return; // user may have customised it — don't overwrite
    }
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("theme: failed to create {}: {e}", dir.display());
        return;
    }
    if let Err(e) = fs::write(&css_path, css) {
        eprintln!("theme: failed to write {}: {e}", css_path.display());
    }
}

/// Try to load `meta.json` from a theme directory.
fn load_meta(theme_dir: &Path) -> Option<ThemeMeta> {
    let meta_path = theme_dir.join("meta.json");
    let content = fs::read_to_string(&meta_path).ok()?;
    serde_json::from_str(&content).ok()
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_always_available() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ThemeManager::new(tmp.path());
        let names = mgr.available_themes();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"light"));
        assert!(names.contains(&"dark"));
    }

    #[test]
    fn fallback_to_default() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ThemeManager::new(tmp.path());
        let info = mgr.get("nonexistent");
        assert_eq!(info.id, "default");
    }

    #[test]
    fn user_theme_discovered() {
        let tmp = tempfile::tempdir().unwrap();
        let custom_dir = tmp.path().join("my-theme");
        fs::create_dir_all(&custom_dir).unwrap();
        fs::write(custom_dir.join("theme.css"), "body { color: red; }").unwrap();

        let mgr = ThemeManager::new(tmp.path());
        assert!(mgr.available_themes().contains(&"my-theme"));
        assert!(mgr.css_for("my-theme").contains("color: red"));
    }

    #[test]
    fn seed_does_not_overwrite_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("default");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("theme.css"), "/* custom */").unwrap();

        let mgr = ThemeManager::new(tmp.path());
        assert!(mgr.css_for("default").contains("/* custom */"));
    }

    #[test]
    fn reload_picks_up_new_themes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = ThemeManager::new(tmp.path());
        assert!(!mgr.available_themes().contains(&"added"));

        let added = tmp.path().join("added");
        fs::create_dir_all(&added).unwrap();
        fs::write(added.join("theme.css"), ".test {}").unwrap();

        mgr.reload();
        assert!(mgr.available_themes().contains(&"added"));
    }

    #[test]
    fn meta_json_loaded() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("fancy");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("theme.css"), "body {}").unwrap();
        fs::write(
            dir.join("meta.json"),
            r#"{"name":"Fancy","author":"Test","version":"1.0.0","description":"A fancy theme"}"#,
        )
        .unwrap();

        let mgr = ThemeManager::new(tmp.path());
        let info = mgr.get("fancy");
        let meta = info.meta.as_ref().unwrap();
        assert_eq!(meta.name, "Fancy");
        assert_eq!(meta.author, "Test");
    }
}
