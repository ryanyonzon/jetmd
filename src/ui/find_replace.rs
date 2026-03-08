// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Find / Replace overlay panel — a floating top-right panel that sits above
//! the editor via `gtk4::Overlay`.  Opening it does **not** change the editor
//! layout or shift any content.
//!
//! Layout (collapsed — Find only):
//! ┌--------------------------------------------------------------------┐
//! │ [Find…          ] [Aa] [ab] [.*] No matches [↑] [↓] [⋯] [✕]        │
//! └--------------------------------------------------------------------┘
//!
//! Layout (expanded — Find + Replace):
//! ┌--------------------------------------------------------------------┐
//! │ [Find…          ] [Aa] [ab] [.*] No matches [↑] [↓] [⋯] [✕]        │
//! │ [Replace with…               ] [Replace] [All]                     │
//! └--------------------------------------------------------------------┘

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;

use gtk4::gdk;
use gtk4::prelude::*;
use sourceview5::prelude::*;
use webkit6::prelude::*;

use crate::state::FindReplaceMode;

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

const OVERLAY_CSS: &str = "
/* -- Dark theme --------------------------------------------------------- */
box.find-overlay-dark {
    background-color: #242424;
    color: #ffffff;
    border: 1px solid #484848;
    border-radius: 8px;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.55);
    padding: 6px 8px;
}
box.find-overlay-dark entry,
box.find-overlay-dark searchentry {
    min-width: 140px;
}
box.find-overlay-dark .toggle-active {
    background-color: #3584e4;
    color: #ffffff;
    border-radius: 4px;
}

/* -- Light theme -------------------------------------------------------- */
box.find-overlay-light {
    background-color: #f2f2f2;
    color: #1e1e1e;
    border: 1px solid #b0b0b0;
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
    padding: 6px 8px;
}
box.find-overlay-light label,
box.find-overlay-light button {
    color: #1e1e1e;
}
box.find-overlay-light entry,
box.find-overlay-light searchentry {
    min-width: 140px;
    background-color: #ffffff;
    color: #1e1e1e;
}
box.find-overlay-light entry image,
box.find-overlay-light searchentry image {
    color: #1e1e1e;
    -gtk-icon-style: symbolic;
}
box.find-overlay-light entry > text > placeholder,
box.find-overlay-light searchentry > text > placeholder {
    color: #888888;
}
box.find-overlay-light .toggle-active {
    background-color: #3584e4;
    color: #ffffff;
    border-radius: 4px;
}
";

static CSS_INIT: Once = Once::new();

fn ensure_css() {
    CSS_INIT.call_once(|| {
        use gtk4::gdk::Display;
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(OVERLAY_CSS);
        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 100,
            );
        }
    });
}

// ---------------------------------------------------------------------------
// Toggle helper
// ---------------------------------------------------------------------------

fn style_toggle(btn: &gtk4::ToggleButton) {
    let b = btn.clone();
    btn.connect_toggled(move |_| {
        if b.is_active() {
            b.add_css_class("toggle-active");
        } else {
            b.remove_css_class("toggle-active");
        }
    });
}

// ---------------------------------------------------------------------------
// Public struct
// ---------------------------------------------------------------------------

/// Widgets that make up the floating find/replace overlay.
#[allow(dead_code)]
pub struct FindReplaceBar {
    pub panel: gtk4::Box,
    pub search_entry: gtk4::SearchEntry,
    pub replace_entry: gtk4::Entry,
    pub replace_button: gtk4::Button,
    pub replace_all_button: gtk4::Button,
    pub replace_box: gtk4::Box,
    pub match_label: gtk4::Label,
    pub search_context: sourceview5::SearchContext,
    mode: Rc<RefCell<FindReplaceMode>>,
    current_index: Rc<RefCell<i32>>,
}

impl FindReplaceBar {
    /// Build the overlay and wire it up to `buffer`.
    ///
    /// `view` is needed to scroll the editor to matches.
    /// `webview` is needed to highlight/scroll in the preview pane.
    pub fn new(
        buffer: &sourceview5::Buffer,
        view: &sourceview5::View,
        webview: &webkit6::WebView,
    ) -> Self {
        ensure_css();

        // -- Search engine -----------------------------------------------
        let search_settings = sourceview5::SearchSettings::new();
        search_settings.set_wrap_around(true);
        let search_context = sourceview5::SearchContext::new(buffer, Some(&search_settings));
        search_context.set_highlight(true);

        // -- Row 1 — All find controls on one horizontal line ---------
        let search_entry = gtk4::SearchEntry::new();
        search_entry.set_hexpand(true);
        search_entry.set_placeholder_text(Some("Find…"));

        let case_toggle = gtk4::ToggleButton::new();
        case_toggle.set_label("Aa");
        case_toggle.set_tooltip_text(Some("Match Case"));
        case_toggle.add_css_class("flat");
        style_toggle(&case_toggle);

        let word_toggle = gtk4::ToggleButton::new();
        word_toggle.set_label("ab");
        word_toggle.set_tooltip_text(Some("Match Whole Word"));
        word_toggle.add_css_class("flat");
        style_toggle(&word_toggle);

        let regex_toggle = gtk4::ToggleButton::new();
        regex_toggle.set_label(".*");
        regex_toggle.set_tooltip_text(Some("Use Regular Expression"));
        regex_toggle.add_css_class("flat");
        style_toggle(&regex_toggle);

        let match_label = gtk4::Label::new(Some("No matches"));
        match_label.add_css_class("dim-label");
        match_label.set_xalign(0.5);

        let prev_button = gtk4::Button::from_icon_name("go-up-symbolic");
        prev_button.set_tooltip_text(Some("Previous match"));
        prev_button.add_css_class("flat");

        let next_button = gtk4::Button::from_icon_name("go-down-symbolic");
        next_button.set_tooltip_text(Some("Next match"));
        next_button.add_css_class("flat");

        let expand_button = gtk4::Button::from_icon_name("view-more-symbolic");
        expand_button.set_tooltip_text(Some("Toggle Replace"));
        expand_button.add_css_class("flat");

        let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
        close_button.set_tooltip_text(Some("Close (Esc)"));
        close_button.add_css_class("flat");

        let find_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        find_row.append(&search_entry);
        find_row.append(&case_toggle);
        find_row.append(&word_toggle);
        find_row.append(&regex_toggle);
        find_row.append(&match_label);
        find_row.append(&prev_button);
        find_row.append(&next_button);
        find_row.append(&expand_button);
        find_row.append(&close_button);

        // -- Row 2 — Replace (collapsed by default) ----------------------
        let replace_entry = gtk4::Entry::new();
        replace_entry.set_hexpand(true);
        replace_entry.set_placeholder_text(Some("Replace with…"));

        let replace_button = gtk4::Button::with_label("Replace");
        replace_button.add_css_class("flat");
        replace_button.set_tooltip_text(Some("Replace current match"));

        let replace_all_button = gtk4::Button::with_label("All");
        replace_all_button.add_css_class("flat");
        replace_all_button.set_tooltip_text(Some("Replace all matches"));

        let replace_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        replace_box.append(&replace_entry);
        replace_box.append(&replace_button);
        replace_box.append(&replace_all_button);
        replace_box.set_visible(false);

        // -- Outer panel --------------------------------------------------
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        panel.add_css_class("find-overlay-dark");
        panel.set_margin_top(8);
        panel.set_margin_end(16);
        panel.set_halign(gtk4::Align::End);
        panel.set_valign(gtk4::Align::Start);
        panel.set_hexpand(false);
        panel.set_vexpand(false);
        panel.set_visible(false);

        panel.append(&find_row);
        panel.append(&replace_box);

        // -- State --------------------------------------------------------
        let mode = Rc::new(RefCell::new(FindReplaceMode::Find));
        let current_index: Rc<RefCell<i32>> = Rc::new(RefCell::new(0));

        // -- Signals ------------------------------------------------------

        // Search text → settings + preview highlight.
        {
            let settings = search_settings.clone();
            let idx = current_index.clone();
            let wv = webview.clone();
            search_entry.connect_search_changed(move |entry| {
                let text = entry.text();
                settings.set_search_text(Some(&text));
                *idx.borrow_mut() = 0;
                highlight_in_preview(&wv, &text, 0);
            });
        }

        // Case sensitivity toggle.
        {
            let settings = search_settings.clone();
            case_toggle.connect_toggled(move |btn| {
                settings.set_case_sensitive(btn.is_active());
            });
        }

        // Whole word toggle.
        {
            let settings = search_settings.clone();
            word_toggle.connect_toggled(move |btn| {
                settings.set_at_word_boundaries(btn.is_active());
            });
        }

        // Regex toggle.
        {
            let settings = search_settings.clone();
            regex_toggle.connect_toggled(move |btn| {
                settings.set_regex_enabled(btn.is_active());
            });
        }

        // Update match count label.
        {
            let label = match_label.clone();
            let idx = current_index.clone();
            search_context.connect_occurrences_count_notify(move |ctx| {
                let total = ctx.occurrences_count();
                let cur = *idx.borrow();
                if total > 0 {
                    label.set_text(&format!("{cur} / {total}"));
                } else {
                    label.set_text("No matches");
                }
            });
        }

        // Next match.
        {
            let ctx = search_context.clone();
            let buf = buffer.clone();
            let v = view.clone();
            let wv = webview.clone();
            let label = match_label.clone();
            let idx = current_index.clone();
            let entry = search_entry.clone();
            next_button.connect_clicked(move |_| {
                navigate_forward(&ctx, &buf);
                scroll_editor_to_cursor(&v);
                let cur = update_index(&ctx, &buf, &idx, &label);
                highlight_in_preview(&wv, &entry.text(), cur.saturating_sub(1) as u32);
            });
        }

        // Prev match.
        {
            let ctx = search_context.clone();
            let buf = buffer.clone();
            let v = view.clone();
            let wv = webview.clone();
            let label = match_label.clone();
            let idx = current_index.clone();
            let entry = search_entry.clone();
            prev_button.connect_clicked(move |_| {
                navigate_backward(&ctx, &buf);
                scroll_editor_to_cursor(&v);
                let cur = update_index(&ctx, &buf, &idx, &label);
                highlight_in_preview(&wv, &entry.text(), cur.saturating_sub(1) as u32);
            });
        }

        // Enter → next match.
        {
            let ctx = search_context.clone();
            let buf = buffer.clone();
            let v = view.clone();
            let wv = webview.clone();
            let label = match_label.clone();
            let idx = current_index.clone();
            let entry_c = search_entry.clone();
            search_entry.connect_activate(move |_| {
                navigate_forward(&ctx, &buf);
                scroll_editor_to_cursor(&v);
                let cur = update_index(&ctx, &buf, &idx, &label);
                highlight_in_preview(&wv, &entry_c.text(), cur.saturating_sub(1) as u32);
            });
        }

        // Replace current match.
        {
            let ctx = search_context.clone();
            let buf = buffer.clone();
            let v = view.clone();
            let wv = webview.clone();
            let repl_entry = replace_entry.clone();
            let label = match_label.clone();
            let idx = current_index.clone();
            let entry = search_entry.clone();
            replace_button.connect_clicked(move |_| {
                let replacement = repl_entry.text();
                let (sel_start, sel_end) = buf.selection_bounds().unwrap_or_else(|| {
                    let cursor = buf.iter_at_mark(&buf.get_insert());
                    (cursor.clone(), cursor)
                });
                let mut s = sel_start;
                let mut e = sel_end;
                let _ = ctx.replace(&mut s, &mut e, &replacement);
                navigate_forward(&ctx, &buf);
                scroll_editor_to_cursor(&v);
                let cur = update_index(&ctx, &buf, &idx, &label);
                highlight_in_preview(&wv, &entry.text(), cur.saturating_sub(1) as u32);
            });
        }

        // Replace all.
        {
            let ctx = search_context.clone();
            let repl_entry = replace_entry.clone();
            let wv = webview.clone();
            replace_all_button.connect_clicked(move |_| {
                let replacement = repl_entry.text();
                let _ = ctx.replace_all(&replacement);
                // Clear preview highlights after replace-all.
                highlight_in_preview(&wv, "", 0);
            });
        }

        // Expand / collapse replace section.
        {
            let replace_box_c = replace_box.clone();
            expand_button.connect_clicked(move |_| {
                replace_box_c.set_visible(!replace_box_c.is_visible());
            });
        }

        // Close button — also clear preview highlights.
        {
            let panel_c = panel.clone();
            let wv = webview.clone();
            close_button.connect_clicked(move |_| {
                panel_c.set_visible(false);
                highlight_in_preview(&wv, "", 0);
            });
        }

        // Esc on search entry.
        {
            let panel_c = panel.clone();
            let wv = webview.clone();
            let key_ctrl = gtk4::EventControllerKey::new();
            key_ctrl.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Escape {
                    panel_c.set_visible(false);
                    highlight_in_preview(&wv, "", 0);
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            });
            search_entry.add_controller(key_ctrl);
        }

        // Esc on replace entry.
        {
            let panel_c = panel.clone();
            let wv = webview.clone();
            let key_ctrl = gtk4::EventControllerKey::new();
            key_ctrl.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Escape {
                    panel_c.set_visible(false);
                    highlight_in_preview(&wv, "", 0);
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            });
            replace_entry.add_controller(key_ctrl);
        }

        Self {
            panel,
            search_entry,
            replace_entry,
            replace_button,
            replace_all_button,
            replace_box,
            match_label,
            search_context,
            mode,
            current_index,
        }
    }

    // -- Public API --------------------------------------------------------

    /// Show the overlay in **Find-only** mode.
    pub fn open_find(&self) {
        *self.mode.borrow_mut() = FindReplaceMode::Find;
        self.replace_box.set_visible(false);
        self.panel.set_visible(true);
        self.search_entry.grab_focus();
    }

    /// Show the overlay in **Find + Replace** mode.
    pub fn open_replace(&self) {
        *self.mode.borrow_mut() = FindReplaceMode::Replace;
        self.replace_box.set_visible(true);
        self.panel.set_visible(true);
        self.search_entry.grab_focus();
    }

    /// Hide the overlay.
    #[allow(dead_code)]
    pub fn close(&self) {
        self.panel.set_visible(false);
    }

    /// Returns `true` if the overlay is currently visible.
    #[allow(dead_code)]
    pub fn is_open(&self) -> bool {
        self.panel.is_visible()
    }

    /// Switch between dark and light opaque backgrounds.
    pub fn set_dark(&self, dark: bool) {
        if dark {
            self.panel.remove_css_class("find-overlay-light");
            self.panel.add_css_class("find-overlay-dark");
        } else {
            self.panel.remove_css_class("find-overlay-dark");
            self.panel.add_css_class("find-overlay-light");
        }
    }
}

// ---------------------------------------------------------------------------
// Navigation helpers
// ---------------------------------------------------------------------------

/// Move to the next match and select it.
fn navigate_forward(ctx: &sourceview5::SearchContext, buffer: &sourceview5::Buffer) {
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    if let Some((start, end, _wrapped)) = ctx.forward(&cursor) {
        buffer.select_range(&start, &end);
    }
}

/// Move to the previous match and select it.
fn navigate_backward(ctx: &sourceview5::SearchContext, buffer: &sourceview5::Buffer) {
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    if let Some((start, end, _wrapped)) = ctx.backward(&cursor) {
        buffer.select_range(&start, &end);
    }
}

/// Scroll the editor so the cursor (i.e. the active match) is centred in the
/// visible area.  Uses `scroll_to_mark` which smoothly scrolls the view.
fn scroll_editor_to_cursor(view: &sourceview5::View) {
    let buf = view.buffer();
    let insert_mark = buf.get_insert();
    // within_margin = 0.25 keeps the match away from the very edge.
    // use_align = true  + yalign = 0.5 centres vertically.
    view.scroll_to_mark(&insert_mark, 0.25, true, 0.0, 0.5);
}

/// Determine the 1-based index of the active match and update the label.
/// Returns the current 1-based index.
fn update_index(
    ctx: &sourceview5::SearchContext,
    buffer: &sourceview5::Buffer,
    idx: &Rc<RefCell<i32>>,
    label: &gtk4::Label,
) -> i32 {
    let total = ctx.occurrences_count();
    if total <= 0 {
        *idx.borrow_mut() = 0;
        label.set_text("No matches");
        return 0;
    }
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    let pos = ctx.occurrence_position(&cursor, &cursor);
    if pos > 0 {
        *idx.borrow_mut() = pos;
    }
    let cur = *idx.borrow();
    label.set_text(&format!("{cur} / {total}"));
    cur
}

// ---------------------------------------------------------------------------
// Preview synchronisation
// ---------------------------------------------------------------------------

/// Inject JavaScript into the preview WebView that:
///   1. Removes all previous search highlights.
///   2. If `term` is non-empty, walks the DOM text nodes, wrapping every
///      occurrence in a `<mark class="search-hl">` element.  The Nth match
///      (0-based `active_index`) gets an additional `active` class and is
///      scrolled into view.
///
/// The JS is intentionally self-contained — it does not depend on any
/// previously injected scripts or libraries.
fn highlight_in_preview(webview: &webkit6::WebView, term: &str, active_index: u32) {
    // Escape the search term for safe embedding in a JS string literal.
    let escaped = term
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r");

    let script = format!(
        r#"(function() {{
  // -- 1. Remove old highlights --------------------------------------
  document.querySelectorAll('mark.search-hl').forEach(function(m) {{
    var p = m.parentNode;
    p.replaceChild(document.createTextNode(m.textContent), m);
    p.normalize();
  }});

  var term = '{escaped}';
  if (!term) return;

  // -- 2. Walk text nodes inside #content -----------------------------
  var container = document.getElementById('content');
  if (!container) return;

  var walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT, null);
  var matches = [];
  var node;
  var termLower = term.toLowerCase();

  while ((node = walker.nextNode())) {{
    var text = node.textContent;
    var textLower = text.toLowerCase();
    var idx = 0;
    while ((idx = textLower.indexOf(termLower, idx)) !== -1) {{
      matches.push({{ node: node, offset: idx }});
      idx += term.length;
    }}
  }}

  // -- 3. Wrap matches in <mark> (reverse order to keep offsets valid) -
  var activeEl = null;
  for (var i = matches.length - 1; i >= 0; i--) {{
    var m = matches[i];
    var range = document.createRange();
    range.setStart(m.node, m.offset);
    range.setEnd(m.node, m.offset + term.length);
    var mark = document.createElement('mark');
    mark.className = 'search-hl' + (i === {active_index} ? ' active' : '');
    range.surroundContents(mark);
    if (i === {active_index}) activeEl = mark;
  }}

  // -- 4. Scroll to active match --------------------------------------
  if (activeEl) {{
    activeEl.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
  }}
}})();"#
    );

    webview.evaluate_javascript(&script, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
}
