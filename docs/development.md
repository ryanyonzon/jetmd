# Development

## Build and test

```bash
cargo build
cargo test
```

## Release build

```bash
cargo build --release
```

## What is covered by tests

- `state` — view modes, theme helpers, document titles, recent-file ordering, and auto-save conditions
- `file_io` — read/write round-trips, UTF-8 validation, overwrite behavior, and error reporting
- `markdown` — HTML rendering and preview-shell generation
- `autosave` — draft save, restore, and discard behavior
- `recent_files` — deduplication and truncation of persisted recent files
- `xdg` — derived app-directory creation

## Documentation map

- [Installation](installation.md)
- [Usage](usage.md)
- [Architecture](architecture.md)
- [Project Structure](project-structure.md)
