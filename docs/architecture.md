# Architecture

## Overview

`jetmd` is a GTK 4 desktop application organized around a tabbed editor window. Each tab owns its own editor, preview, find/replace overlay, and document metadata, while global state manages layout mode, theme preference, auto-save state, and persisted recent files.

## Major components

- **`app`** — builds the main window, manages tabs, wires actions and shortcuts, shows dialogs, and runs auto-save and draft-recovery flows.
- **`state`** — stores shared application state such as view mode, theme, auto-save settings, recent files, and per-document metadata.
- **`markdown`** — converts Markdown to HTML and builds the HTML shell used for preview and export.
- **`file_io`** — reads and writes UTF-8 files and reports file-related errors.
- **`autosave`** — persists unsaved drafts into the cache directory and restores them on launch.
- **`recent_files`** — saves and loads the recent-files list.
- **`xdg`** — resolves config, data, and cache directories and stores JSON-backed settings.
- **`ui`** — contains focused widget builders for the editor, preview, toolbar, and find/replace overlay.

## Runtime behavior

### Tabs and documents

- Documents open in tabs.
- Each tab tracks its file path, modified state, and draft identifier independently.
- Closing a modified tab triggers an unsaved-changes flow.

### Preview pipeline

- Markdown text is rendered to HTML with `pulldown-cmark`.
- The preview is updated in a debounced way while the user types.
- The same rendering pipeline is reused for HTML export.

### Persistence

- Theme and auto-save preferences are stored in the app config directory.
- Recent files are stored in the app data directory.
- Draft recovery data is stored in the app cache directory.

## Testing scope

Current unit tests cover:

- `state`
- `file_io`
- `markdown`
- `autosave`
- `recent_files`
- `xdg`
