// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Editor panel — a `sourceview5::View` configured for Markdown editing.

use gtk4::prelude::*;
use sourceview5::prelude::*;

/// Create and configure a GtkSourceView for Markdown editing.
pub fn create_editor() -> sourceview5::View {
    let buffer = sourceview5::Buffer::new(None);

    // Enable undo/redo on the buffer.
    buffer.set_max_undo_levels(u32::MAX);

    // Try to set Markdown syntax highlighting.
    let lang_manager = sourceview5::LanguageManager::default();
    if let Some(lang) = lang_manager.language("markdown") {
        buffer.set_language(Some(&lang));
    }

    let view = sourceview5::View::with_buffer(&buffer);
    view.set_monospace(true);
    view.set_show_line_numbers(true);
    view.set_highlight_current_line(true);
    view.set_tab_width(4);
    view.set_insert_spaces_instead_of_tabs(true);
    view.set_auto_indent(true);
    view.set_wrap_mode(gtk4::WrapMode::WordChar);
    view.set_top_margin(4);
    view.set_bottom_margin(4);
    view.set_left_margin(4);
    view.set_right_margin(4);
    view.set_hexpand(true);
    view.set_vexpand(true);

    view
}

/// Apply a dark or light style scheme to the source view.
pub fn apply_theme(view: &sourceview5::View, dark: bool) {
    let buffer = view
        .buffer()
        .downcast::<sourceview5::Buffer>()
        .expect("editor buffer is a sourceview5::Buffer");

    let scheme_manager = sourceview5::StyleSchemeManager::default();
    let scheme_id = if dark { "Adwaita-dark" } else { "Adwaita" };
    if let Some(scheme) = scheme_manager.scheme(scheme_id) {
        buffer.set_style_scheme(Some(&scheme));
    }
}

/// Get all text from the editor buffer.
pub fn get_text(view: &sourceview5::View) -> String {
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, true).to_string()
}

/// Replace the entire editor buffer content (e.g. after opening a file).
pub fn set_text(view: &sourceview5::View, text: &str) {
    let buffer = view
        .buffer()
        .downcast::<sourceview5::Buffer>()
        .expect("editor buffer is a sourceview5::Buffer");
    // Replace content — set_text internally handles undo grouping
    // (begin_irreversible_action), so no wrapping in begin_user_action.
    buffer.set_text(text);
    // Clear undo history after loading a file.
    buffer.set_max_undo_levels(0);
    buffer.set_max_undo_levels(u32::MAX);

    // Place cursor at the start.
    let start = buffer.start_iter();
    buffer.place_cursor(&start);
}
