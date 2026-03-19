// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Application state management.
//!
//! Contains the core state types for the editor: document metadata,
//! view mode toggling, theme preferences, and top-level application state.

use std::path::PathBuf;
use std::time::Instant;

// ---------------------------------------------------------------------------
// View mode
// ---------------------------------------------------------------------------

/// The three layout modes for the editor window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Show only the raw Markdown editor.
    Editor,
    /// Show editor and preview side-by-side.
    Split,
    /// Show only the rendered Markdown preview.
    Preview,
}

impl ViewMode {
    /// Cycle to the next view mode: Editor → Split → Preview → Editor.
    #[allow(dead_code)]
    pub fn cycle(self) -> Self {
        match self {
            ViewMode::Editor => ViewMode::Split,
            ViewMode::Split => ViewMode::Preview,
            ViewMode::Preview => ViewMode::Editor,
        }
    }

    /// Human-readable label for toolbar display.
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Editor => "Editor",
            ViewMode::Split => "Split",
            ViewMode::Preview => "Preview",
        }
    }
}

// ---------------------------------------------------------------------------
// Find / Replace
// ---------------------------------------------------------------------------

/// Whether the find bar is in find-only or find+replace mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindReplaceMode {
    Find,
    #[allow(dead_code)]
    Replace,
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

/// Light / dark theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    #[allow(dead_code)]
    pub fn toggle(self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }

    pub fn persisted_value(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn from_persisted(value: &str) -> Self {
        match value {
            "light" => Theme::Light,
            _ => Theme::Dark,
        }
    }
}

// ---------------------------------------------------------------------------
// Document metadata
// ---------------------------------------------------------------------------

/// Metadata for a single Markdown document.
///
/// The actual text content lives in the `GtkSourceBuffer` — this struct
/// tracks only the file path and modification state.
pub struct Document {
    /// The file path this document was loaded from / saved to.
    pub file_path: Option<PathBuf>,
    /// Whether there are unsaved changes.
    pub modified: bool,
    /// Draft id used for restoring unsaved work from the cache directory.
    pub draft_id: Option<String>,
    /// Last time a cache draft was written for this document.
    pub last_draft_save: Option<Instant>,
}

impl Document {
    /// Create metadata for a new (untitled) document.
    pub fn new() -> Self {
        Self {
            file_path: None,
            modified: false,
            draft_id: None,
            last_draft_save: None,
        }
    }

    /// Mark the document as saved.
    pub fn mark_saved(&mut self) {
        self.modified = false;
        self.last_draft_save = None;
    }

    /// Mark that a cache draft was saved for this document.
    pub fn mark_draft_saved(&mut self) {
        self.last_draft_save = Some(Instant::now());
    }

    /// Remove any tracked cache draft metadata.
    pub fn clear_draft(&mut self) {
        self.draft_id = None;
        self.last_draft_save = None;
    }

    /// A display-friendly title (filename or "Untitled").
    pub fn title(&self) -> String {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".into());
        if self.modified {
            format!("● {name}")
        } else {
            name
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

const AUTO_SAVE_INTERVAL_SECS: u64 = 30;
const STATUS_DISPLAY_SECS: u64 = 4;

/// Top-level application state (global settings shared across all tabs).
///
/// Per-document state (`Document`) now lives in each tab managed by `app.rs`.
#[allow(dead_code)]
pub struct AppState {
    pub view_mode: ViewMode,
    pub theme: Theme,
    pub auto_save_enabled: bool,
    pub last_auto_save: Instant,
    status_message: Option<(String, Instant)>,
    /// Active preview theme name (directory name under `themes/`).
    pub preview_theme: String,
    /// Most-recently-opened files (newest first, max 10).
    pub recent_files: Vec<PathBuf>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            view_mode: ViewMode::Editor,
            theme: Theme::Dark,
            auto_save_enabled: false,
            last_auto_save: Instant::now(),
            status_message: None,
            preview_theme: crate::theme::DEFAULT_THEME_NAME.to_string(),
            recent_files: Vec::new(),
        }
    }

    /// Set a transient status-bar message.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }

    /// Get the current status message, or `None` if it has expired.
    #[allow(dead_code)]
    pub fn current_status(&self) -> Option<&str> {
        self.status_message.as_ref().and_then(|(msg, time)| {
            if time.elapsed().as_secs() < STATUS_DISPLAY_SECS {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }

    /// Add a file to the recent-files list (max 10, most recent first).
    pub fn add_recent_file(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(10);
    }

    /// Whether the global auto-save conditions are met (enabled + interval elapsed).
    ///
    /// Per-document conditions (modified, has path) are checked in the timer
    /// loop in `app.rs`.
    pub fn should_auto_save(&self) -> bool {
        self.auto_save_enabled && self.last_auto_save.elapsed().as_secs() >= AUTO_SAVE_INTERVAL_SECS
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- ViewMode -----------------------------------------------------------

    #[test]
    fn view_mode_cycle_wraps_around() {
        assert_eq!(ViewMode::Editor.cycle(), ViewMode::Split);
        assert_eq!(ViewMode::Split.cycle(), ViewMode::Preview);
        assert_eq!(ViewMode::Preview.cycle(), ViewMode::Editor);
    }

    #[test]
    fn view_mode_labels_are_nonempty() {
        for mode in [ViewMode::Editor, ViewMode::Split, ViewMode::Preview] {
            assert!(!mode.label().is_empty());
        }
    }

    // -- Theme --------------------------------------------------------------

    #[test]
    fn theme_toggle() {
        assert_eq!(Theme::Light.toggle(), Theme::Dark);
        assert_eq!(Theme::Dark.toggle(), Theme::Light);
    }

    // -- Document -----------------------------------------------------------

    #[test]
    fn new_document_is_unmodified() {
        let doc = Document::new();
        assert!(!doc.modified);
        assert!(doc.file_path.is_none());
    }

    #[test]
    fn mark_saved_clears_modified() {
        let mut doc = Document::new();
        doc.modified = true;
        doc.mark_saved();
        assert!(!doc.modified);
    }

    #[test]
    fn title_shows_filename_or_untitled() {
        let doc = Document::new();
        assert_eq!(doc.title(), "Untitled");

        let mut doc = Document::new();
        doc.file_path = Some(std::path::PathBuf::from("/a/b/notes.md"));
        assert_eq!(doc.title(), "notes.md");
    }

    #[test]
    fn title_shows_modified_indicator() {
        let mut doc = Document::new();
        doc.file_path = Some(std::path::PathBuf::from("test.md"));
        assert!(!doc.title().contains('●'));
        doc.modified = true;
        assert!(doc.title().contains('●'));
    }

    // -- AppState -----------------------------------------------------------

    #[test]
    fn status_message_set() {
        let mut state = AppState::new();
        state.set_status("hello");
        assert!(state.current_status().is_some());
    }

    #[test]
    fn auto_save_disabled_by_default() {
        let state = AppState::new();
        // auto_save_enabled is false, so should_auto_save returns false.
        assert!(!state.should_auto_save());
    }
}
