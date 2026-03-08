# Installation

This guide covers system dependencies, building, and running `jetmd`.

## Requirements

- **Rust 1.85+**
- **Linux** with GTK 4, GtkSourceView 5, and WebKitGTK 6 development packages installed

## Install system dependencies

### Debian / Ubuntu

```bash
sudo apt install libgtk-4-dev libgtksourceview-5-dev libwebkitgtk-6.0-dev
```

### Fedora

```bash
sudo dnf install gtk4-devel gtksourceview5-devel webkitgtk6.0-devel
```

### Arch

```bash
sudo pacman -S gtk4 gtksourceview5 webkit2gtk-6.0
```

### Solus

```bash
sudo eopkg install -y libgtk-4-devel libgtksourceview5-devel libwebkit-gtk6-devel
```

## Build

```bash
# Debug build
cargo build

# Release build
cargo build --release
```

The compiled binary is written to `target/debug/jetmd` or `target/release/jetmd`.

## Run

```bash
# Start with an empty tab
cargo run --release

# Open a file at startup
cargo run --release -- path/to/file.md
```

`jetmd` currently accepts a single optional file path argument at startup.

## Optional bundle build

If you want to create an application bundle:

```bash
cargo install cargo-bundle
cargo bundle --release
```
