// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! File I/O operations.
//!
//! Provides cross-platform functions for reading and writing Markdown files.
//! File dialogs are handled by GTK 4's native `FileDialog` in the UI layer.

use std::fs;
use std::io;
use std::path::Path;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during file operations.
#[derive(Debug)]
pub enum FileError {
    /// An underlying I/O error.
    Io(io::Error),
    /// The file contents are not valid UTF-8.
    InvalidUtf8,
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::Io(e) => write!(f, "I/O error: {e}"),
            FileError::InvalidUtf8 => write!(f, "File is not valid UTF-8"),
        }
    }
}

impl std::error::Error for FileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FileError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for FileError {
    fn from(e: io::Error) -> Self {
        FileError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Core operations
// ---------------------------------------------------------------------------

/// Read the contents of a file as a UTF-8 string.
pub fn read_file(path: &Path) -> Result<String, FileError> {
    let bytes = fs::read(path)?;
    String::from_utf8(bytes).map_err(|_| FileError::InvalidUtf8)
}

/// Write `content` to `path`, creating or overwriting the file.
pub fn write_file(path: &Path, content: &str) -> Result<(), FileError> {
    fs::write(path, content.as_bytes())?;
    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");

        write_file(&path, "# Hello World").unwrap();
        let content = read_file(&path).unwrap();
        assert_eq!(content, "# Hello World");
    }

    #[test]
    fn read_nonexistent_file_returns_io_error() {
        let result = read_file(Path::new("/tmp/__jetmd_nonexistent_file__.md"));
        assert!(result.is_err());
        match result.unwrap_err() {
            FileError::Io(e) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
            other => panic!("Expected Io error, got: {other:?}"),
        }
    }

    #[test]
    fn write_to_invalid_path_returns_io_error() {
        let result = write_file(Path::new("/nonexistent_dir_xyz/test.md"), "data");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FileError::Io(_)));
    }

    #[test]
    fn utf8_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("utf8.md");
        let text = "日本語 🇺🇸 Ñoño café ρ∞∑";

        write_file(&path, text).unwrap();
        let content = read_file(&path).unwrap();
        assert_eq!(content, text);
    }

    #[test]
    fn invalid_utf8_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.md");
        std::fs::write(&path, &[0xFF, 0xFE, 0x80]).unwrap();

        let result = read_file(&path);
        assert!(matches!(result.unwrap_err(), FileError::InvalidUtf8));
    }

    #[test]
    fn empty_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.md");

        write_file(&path, "").unwrap();
        let content = read_file(&path).unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn overwrite_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("over.md");

        write_file(&path, "original").unwrap();
        write_file(&path, "replaced").unwrap();
        let content = read_file(&path).unwrap();
        assert_eq!(content, "replaced");
    }

    #[test]
    fn file_error_display() {
        let e = FileError::InvalidUtf8;
        assert_eq!(format!("{e}"), "File is not valid UTF-8");

        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let e = FileError::Io(io_err);
        assert!(format!("{e}").contains("denied"));
    }
}
