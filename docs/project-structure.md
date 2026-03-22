# Project Structure

```text
src/
├── main.rs           GTK application entry point
├── app.rs            Main window, tabs, actions, dialogs, autosave, and shortcuts
├── autosave.rs       Draft persistence and restore logic
├── file_io.rs        UTF-8 file read/write helpers
├── highlight.rs      Syntax highlighting for fenced code blocks via syntect
├── markdown.rs       Markdown-to-HTML conversion and preview/export HTML shell
├── recent_files.rs   Persistent recent-files storage
├── state.rs          Shared app state, view modes, theme, and document metadata
├── theme.rs          Preview theme discovery, loading, and hot-swap support
├── xdg.rs            App config/data/cache directory handling
├── themes/
│   ├── default.css   Built-in default preview theme
│   ├── light.css     Built-in light preview theme
│   └── dark.css      Built-in dark preview theme
└── ui/
    ├── mod.rs            UI module exports
    ├── editor.rs         GtkSourceView editor setup and theming
    ├── find_replace.rs   Floating find/replace overlay
    ├── formatting.rs     Keyboard-driven Markdown formatting actions
    ├── preview.rs        WebKitGTK preview integration
    └── toolbar.rs        Header-bar buttons and application menu model
```

## Notes

- `res/` contains bundled icons and images used by the desktop UI.
- `Cargo.toml` contains crate metadata and dependency declarations.
- `LICENSE-MIT` and `LICENSE-APACHE` provide the dual-license texts.
