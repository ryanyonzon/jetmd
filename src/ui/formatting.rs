// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Keyboard-driven Markdown formatting engine.
//!
//! Every public function operates on a `sourceview5::View` and wraps its
//! buffer mutations inside a single `begin_user_action` / `end_user_action`
//! pair so that each formatting operation is a single undo step.

use gtk4::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get text for a single line (without the trailing newline).
fn get_line_text(buf: &gtk4::TextBuffer, line: i32) -> Option<String> {
    let start = buf.iter_at_line(line)?;
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    Some(buf.text(&start, &end, true).to_string())
}

/// Determine the range of lines covered by the selection (or the cursor line).
/// Returns `(first_line, last_line)` inclusive.
fn selected_line_range(buf: &gtk4::TextBuffer) -> (i32, i32) {
    match buf.selection_bounds() {
        Some((start, end)) => {
            let start_line = start.line();
            // If the selection ends exactly at a line start, exclude that line.
            let end_line = if end.starts_line() && end.line() > start_line {
                end.line() - 1
            } else {
                end.line()
            };
            (start_line, end_line)
        }
        None => {
            let cursor = buf.iter_at_mark(&buf.get_insert());
            let line = cursor.line();
            (line, line)
        }
    }
}

/// Return line-start and line-end iters for a given line number.
fn line_bounds(buf: &gtk4::TextBuffer, line: i32) -> Option<(gtk4::TextIter, gtk4::TextIter)> {
    let start = buf.iter_at_line(line)?;
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    Some((start, end))
}

/// Check if a string starts with `N. ` (a numbered list prefix).
/// Returns the byte-length of the prefix if found.
fn numbered_prefix_len(text: &str) -> Option<usize> {
    let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() == text.len() {
        return None; // no leading digits
    }
    if rest.starts_with(". ") {
        Some(text.len() - rest.len() + 2) // digits + ". "
    } else {
        None
    }
}

/// Detect the current heading level (0 = no heading, 1–6 for `#`–`######`).
fn detect_heading_level(line_text: &str) -> u32 {
    let hashes = line_text.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) {
        // Must be followed by a space (or be the entire line content).
        let rest = &line_text[hashes..];
        if rest.is_empty() || rest.starts_with(' ') {
            return hashes as u32;
        }
    }
    0
}

/// Remove the heading prefix from a line string (returns the content only).
fn strip_heading(line_text: &str) -> &str {
    let level = detect_heading_level(line_text);
    if level == 0 {
        return line_text;
    }
    let prefix_len = level as usize; // count of '#'
    let rest = &line_text[prefix_len..];
    // Skip the single space that follows the `#` characters.
    if let Some(stripped) = rest.strip_prefix(' ') {
        stripped
    } else {
        rest
    }
}

/// Detect any list prefix (bullet `- `, task `- [ ] ` / `- [x] `, or numbered `N. `).
/// Returns the byte-length of the prefix if found, including leading whitespace.
fn list_prefix_len(line: &str) -> Option<usize> {
    let stripped = line.trim_start_matches(' ');
    let indent = line.len() - stripped.len();

    if stripped.starts_with("- [") && stripped.len() >= 6 {
        // "- [ ] " (unchecked) or "- [x] " (checked) — both are 6 chars
        let after_bracket = &stripped[3..];
        if (after_bracket.starts_with(" ] ") || after_bracket.starts_with("x] "))
            && after_bracket.len() >= 3
        {
            return Some(indent + 6);
        }
    }
    if stripped.starts_with("- ") {
        return Some(indent + 2);
    }
    if stripped.starts_with("* ") {
        return Some(indent + 2);
    }
    if let Some(num_len) = numbered_prefix_len(stripped) {
        return Some(indent + num_len);
    }
    None
}

/// Build the "next" prefix for list continuation on Enter.
/// For numbered lists the number is incremented.
fn next_list_prefix(current_line: &str) -> Option<String> {
    let stripped = current_line.trim_start_matches(' ');
    let indent: String = current_line[..current_line.len() - stripped.len()].into();

    // Task list: - [ ] text
    if stripped.starts_with("- [ ] ") || stripped.starts_with("- [x] ") {
        return Some(format!("{indent}- [ ] "));
    }
    // Bullet list: - text
    if stripped.starts_with("- ") {
        return Some(format!("{indent}- "));
    }
    if stripped.starts_with("* ") {
        return Some(format!("{indent}* "));
    }
    // Numbered list: N. text
    if let Some(prefix_byte_len) = numbered_prefix_len(stripped) {
        let digits_str = &stripped[..prefix_byte_len - 2]; // everything before ". "
        if let Ok(n) = digits_str.parse::<u64>() {
            return Some(format!("{indent}{}. ", n + 1));
        }
    }
    // Block quote: > text
    if stripped.starts_with("> ") {
        return Some(format!("{indent}> "));
    }
    None
}

// ---------------------------------------------------------------------------
// Inline Formatting  (Bold, Italic, Strikethrough, Inline Code)
// ---------------------------------------------------------------------------

/// Toggle inline formatting around the selection or cursor.
///
/// `marker` is the syntax pair, e.g. `"**"`, `"*"`, `"~~"`, `` "`" ``.
pub fn toggle_inline(view: &sourceview5::View, marker: &str) {
    let buf = view.buffer();
    let mchars = marker.chars().count() as i32;

    buf.begin_user_action();

    match buf.selection_bounds() {
        Some((sel_start, sel_end)) => {
            let selected = buf.text(&sel_start, &sel_end, true).to_string();
            let sel_chars = selected.chars().count() as i32;

            // --- Case A: selection itself is wrapped (e.g. user selected "**foo**")
            if selected.starts_with(marker) && selected.ends_with(marker) && sel_chars >= mchars * 2
            {
                let inner = &selected[marker.len()..selected.len() - marker.len()];
                let inner_owned = inner.to_string();
                let inner_chars = inner.chars().count() as i32;

                let mark = buf.create_mark(None, &sel_start, true);
                let mut s = sel_start;
                let mut e = sel_end;
                buf.delete(&mut s, &mut e);
                let mut ins = buf.iter_at_mark(&mark);
                buf.insert(&mut ins, &inner_owned);

                let start = buf.iter_at_mark(&mark);
                let mut end = start;
                end.forward_chars(inner_chars);
                buf.select_range(&start, &end);
                buf.delete_mark(&mark);
            }
            // --- Case B: markers sit just outside the selection
            else {
                let mut before = sel_start;
                let can_look_back = before.backward_chars(mchars);
                let mut after = sel_end;
                let can_look_fwd = after.forward_chars(mchars);

                let text_before = if can_look_back {
                    buf.text(&before, &sel_start, true).to_string()
                } else {
                    String::new()
                };
                let text_after = if can_look_fwd {
                    buf.text(&sel_end, &after, true).to_string()
                } else {
                    String::new()
                };

                if text_before == marker && text_after == marker {
                    // Remove the outer markers.
                    let mark = buf.create_mark(None, &before, true);
                    let selected_clone = selected.clone();
                    let mut bs = before;
                    let mut ae = after;
                    buf.delete(&mut bs, &mut ae);
                    let mut ins = buf.iter_at_mark(&mark);
                    buf.insert(&mut ins, &selected_clone);

                    let start = buf.iter_at_mark(&mark);
                    let mut end = start;
                    end.forward_chars(sel_chars);
                    buf.select_range(&start, &end);
                    buf.delete_mark(&mark);
                } else {
                    // --- Case C: wrap selection with markers.
                    let new_text = format!("{marker}{selected}{marker}");
                    let mark = buf.create_mark(None, &sel_start, true);
                    let mut s = sel_start;
                    let mut e = sel_end;
                    buf.delete(&mut s, &mut e);
                    let mut ins = buf.iter_at_mark(&mark);
                    buf.insert(&mut ins, &new_text);

                    // Re-select only the inner text.
                    let origin = buf.iter_at_mark(&mark);
                    let mut sel_s = origin;
                    sel_s.forward_chars(mchars);
                    let mut sel_e = sel_s;
                    sel_e.forward_chars(sel_chars);
                    buf.select_range(&sel_s, &sel_e);
                    buf.delete_mark(&mark);
                }
            }
        }
        None => {
            // No selection — check if cursor is already between markers.
            let cursor = buf.iter_at_mark(&buf.get_insert());
            let mut before = cursor;
            let ok_back = before.backward_chars(mchars);
            let mut after = cursor;
            let ok_fwd = after.forward_chars(mchars);

            let txt_before = if ok_back {
                buf.text(&before, &cursor, true).to_string()
            } else {
                String::new()
            };
            let txt_after = if ok_fwd {
                buf.text(&cursor, &after, true).to_string()
            } else {
                String::new()
            };

            if txt_before == marker && txt_after == marker {
                // Remove the empty markers around cursor.
                let mut bs = before;
                let mut ae = after;
                buf.delete(&mut bs, &mut ae);
            } else {
                // Insert paired markers and place cursor between them.
                let paired = format!("{marker}{marker}");
                let mut ins = cursor;
                buf.insert(&mut ins, &paired);
                // `ins` now points past the second marker — go back.
                ins.backward_chars(mchars);
                buf.place_cursor(&ins);
            }
        }
    }

    buf.end_user_action();
}

// ---------------------------------------------------------------------------
// Headings
// ---------------------------------------------------------------------------

/// Set the current line to heading `level` (1–6).
///
/// If the line is already at the requested level, remove the heading
/// (toggle back to paragraph).  Heading shortcuts apply to the entire
/// current line regardless of selection.
pub fn set_heading(view: &sourceview5::View, level: u32) {
    assert!((1..=6).contains(&level));

    let buf = view.buffer();
    let cursor = buf.iter_at_mark(&buf.get_insert());
    let line = cursor.line();

    let Some(text) = get_line_text(&buf, line) else {
        return;
    };
    let current_level = detect_heading_level(&text);
    let content = strip_heading(&text);

    buf.begin_user_action();

    let Some((mut ls, mut le)) = line_bounds(&buf, line) else {
        buf.end_user_action();
        return;
    };
    buf.delete(&mut ls, &mut le);

    let new_line = if current_level == level {
        // Toggle off — return to plain paragraph.
        content.to_string()
    } else {
        let hashes: String = "#".repeat(level as usize);
        format!("{hashes} {content}")
    };

    let mark = buf.create_mark(None, &ls, true);
    buf.insert(&mut ls, &new_line);

    // Place cursor at the end of the (new) line so the user can keep typing.
    let line_start = buf.iter_at_mark(&mark);
    let new_len = new_line.chars().count() as i32;
    let mut end_of_line = line_start;
    end_of_line.forward_chars(new_len);
    buf.place_cursor(&end_of_line);
    buf.delete_mark(&mark);

    buf.end_user_action();
}

// ---------------------------------------------------------------------------
// Links & Images
// ---------------------------------------------------------------------------

/// Insert a Markdown link.  If text is selected it becomes the link text.
pub fn insert_link(view: &sourceview5::View) {
    insert_link_or_image(view, false);
}

/// Insert a Markdown image.  If text is selected it becomes the alt text.
pub fn insert_image(view: &sourceview5::View) {
    insert_link_or_image(view, true);
}

fn insert_link_or_image(view: &sourceview5::View, is_image: bool) {
    let buf = view.buffer();
    let bang = if is_image { "!" } else { "" };

    buf.begin_user_action();

    match buf.selection_bounds() {
        Some((sel_start, sel_end)) => {
            let selected = buf.text(&sel_start, &sel_end, true).to_string();
            let new_text = format!("{bang}[{selected}](url)");
            let mark = buf.create_mark(None, &sel_start, true);
            let mut s = sel_start;
            let mut e = sel_end;
            buf.delete(&mut s, &mut e);
            let mut ins = buf.iter_at_mark(&mark);
            buf.insert(&mut ins, &new_text);

            // Select "url" so the user can type the URL.
            let origin = buf.iter_at_mark(&mark);
            let url_start_offset = bang.len() + 1 + selected.len() + 2; // ![text](
            let mut url_s = origin;
            url_s.forward_chars(url_start_offset as i32);
            let mut url_e = url_s;
            url_e.forward_chars(3); // "url"
            buf.select_range(&url_s, &url_e);
            buf.delete_mark(&mark);
        }
        None => {
            let cursor = buf.iter_at_mark(&buf.get_insert());
            let template = format!("{bang}[text](url)");
            let mark = buf.create_mark(None, &cursor, true);
            let mut ins = cursor;
            buf.insert(&mut ins, &template);

            // Select "text" so the user can type immediately.
            let origin = buf.iter_at_mark(&mark);
            let text_start = bang.len() + 1; // past "![" or "["
            let mut ts = origin;
            ts.forward_chars(text_start as i32);
            let mut te = ts;
            te.forward_chars(4); // "text"
            buf.select_range(&ts, &te);
            buf.delete_mark(&mark);
        }
    }

    buf.end_user_action();
}

// ---------------------------------------------------------------------------
// Lists  (Bullet / Numbered / Task)
// ---------------------------------------------------------------------------

/// Toggle unordered (bullet) list prefix `- ` on the current line(s).
pub fn toggle_bullet_list(view: &sourceview5::View) {
    toggle_simple_prefix(view, "- ");
}

/// Toggle task list prefix `- [ ] ` on the current line(s).
pub fn toggle_task_list(view: &sourceview5::View) {
    toggle_simple_prefix(view, "- [ ] ");
}

/// Toggle ordered (numbered) list prefix `1. ` on the current line(s).
///
/// When toggling on, lines are auto-numbered sequentially.
pub fn toggle_numbered_list(view: &sourceview5::View) {
    let buf = view.buffer();
    let (start_line, end_line) = selected_line_range(&buf);

    // Determine whether ALL lines already have a numbered prefix.
    let all_numbered = (start_line..=end_line).all(|ln| {
        get_line_text(&buf, ln)
            .as_deref()
            .and_then(|t| numbered_prefix_len(t.trim_start()))
            .is_some()
    });

    buf.begin_user_action();

    if all_numbered {
        // Remove numbered prefixes (process bottom-up).
        for ln in (start_line..=end_line).rev() {
            if let Some(text) = get_line_text(&buf, ln) {
                let stripped = text.trim_start_matches(' ');
                let indent = text.len() - stripped.len();
                if let Some(plen) = numbered_prefix_len(stripped)
                    && let Some((mut ls, _)) = line_bounds(&buf, ln)
                {
                    let mut pe = ls;
                    pe.forward_chars((indent + plen) as i32);
                    buf.delete(&mut ls, &mut pe);
                }
            }
        }
    } else {
        // Add numbered prefixes — first strip any existing list prefix, then add.
        let mut number = 1u64;
        for ln in start_line..=end_line {
            if let Some(text) = get_line_text(&buf, ln) {
                // Remove any existing list prefix.
                let stripped = text.trim_start_matches(' ');
                if let Some(plen) = list_prefix_len(&text)
                    && let Some((mut ls, _)) = line_bounds(&buf, ln)
                {
                    let mut pe = ls;
                    pe.forward_chars(plen as i32);
                    buf.delete(&mut ls, &mut pe);
                }
                // Insert numbered prefix at line start.
                if let Some(mut ls) = buf.iter_at_line(ln) {
                    let prefix = format!("{number}. ");
                    buf.insert(&mut ls, &prefix);
                }
                let _ = stripped; // used above
                number += 1;
            }
        }
    }

    buf.end_user_action();
}

/// Generic toggle for a fixed line prefix (e.g. `"- "`, `"- [ ] "`, `"> "`).
fn toggle_simple_prefix(view: &sourceview5::View, prefix: &str) {
    let buf = view.buffer();
    let (start_line, end_line) = selected_line_range(&buf);
    let prefix_chars = prefix.chars().count() as i32;

    // Check whether ALL lines already have this prefix.
    let all_have = (start_line..=end_line).all(|ln| {
        get_line_text(&buf, ln)
            .map(|t| t.starts_with(prefix))
            .unwrap_or(false)
    });

    buf.begin_user_action();

    if all_have {
        // Remove the prefix (process bottom-up to keep line numbers stable).
        for ln in (start_line..=end_line).rev() {
            if let Some(mut ls) = buf.iter_at_line(ln) {
                let mut pe = ls;
                pe.forward_chars(prefix_chars);
                buf.delete(&mut ls, &mut pe);
            }
        }
    } else {
        // Add the prefix to every line that doesn't have it.
        for ln in start_line..=end_line {
            let already = get_line_text(&buf, ln)
                .map(|t| t.starts_with(prefix))
                .unwrap_or(false);
            if !already && let Some(mut ls) = buf.iter_at_line(ln) {
                buf.insert(&mut ls, prefix);
            }
        }
    }

    buf.end_user_action();
}

// ---------------------------------------------------------------------------
// Block Elements  (Code Block / Block Quote / Horizontal Rule)
// ---------------------------------------------------------------------------

/// Toggle block quote (`> `) prefix on the current line(s).
pub fn toggle_block_quote(view: &sourceview5::View) {
    toggle_simple_prefix(view, "> ");
}

/// Insert a fenced code block.
///
/// If text is selected the selection is wrapped; otherwise an empty block is
/// inserted with the cursor inside.
pub fn insert_code_block(view: &sourceview5::View) {
    let buf = view.buffer();

    buf.begin_user_action();

    match buf.selection_bounds() {
        Some((sel_start, sel_end)) => {
            let selected = buf.text(&sel_start, &sel_end, true).to_string();
            let replacement = format!("```\n{selected}\n```");
            let mark = buf.create_mark(None, &sel_start, true);
            let mut s = sel_start;
            let mut e = sel_end;
            buf.delete(&mut s, &mut e);
            let mut ins = buf.iter_at_mark(&mark);
            buf.insert(&mut ins, &replacement);

            // Place cursor right after the opening ``` (on the language hint line).
            let origin = buf.iter_at_mark(&mark);
            let mut cursor = origin;
            cursor.forward_chars(3); // past "```"
            buf.place_cursor(&cursor);
            buf.delete_mark(&mark);
        }
        None => {
            let cursor = buf.iter_at_mark(&buf.get_insert());
            let on_empty_line = cursor.starts_line()
                && (cursor.ends_line() || {
                    let mut peek = cursor;
                    peek.forward_to_line_end();
                    buf.text(&cursor, &peek, true).trim().is_empty()
                });

            let (template, cursor_offset) = if on_empty_line {
                ("```\n\n```".to_string(), 4) // cursor after "```\n"
            } else {
                // Insert on a new line below.
                ("\n```\n\n```".to_string(), 5)
            };

            let mut ins = cursor;
            buf.insert(&mut ins, &template);
            // `ins` now past the whole inserted text — go back to the empty line.
            ins.backward_chars((template.chars().count() as i32) - cursor_offset);
            buf.place_cursor(&ins);
        }
    }

    buf.end_user_action();
}

/// Insert a horizontal rule (`---`) on its own line.
pub fn insert_horizontal_rule(view: &sourceview5::View) {
    let buf = view.buffer();

    buf.begin_user_action();

    let cursor = buf.iter_at_mark(&buf.get_insert());
    let on_empty_line = cursor.starts_line() && cursor.ends_line();
    let text = if on_empty_line { "---\n" } else { "\n---\n" };
    let mut ins = cursor;
    buf.insert(&mut ins, text);
    buf.place_cursor(&ins);

    buf.end_user_action();
}

// ---------------------------------------------------------------------------
// Structure & Editing  (Move / Duplicate / Indent / Outdent)
// ---------------------------------------------------------------------------

/// Move the current line (or selected lines) up or down.
pub fn move_line(view: &sourceview5::View, up: bool) {
    let buf = view.buffer();
    let (first, last) = selected_line_range(&buf);
    let line_count = buf.line_count();

    if up && first == 0 {
        return; // already at top
    }
    if !up && last >= line_count - 1 {
        return; // already at bottom
    }

    buf.begin_user_action();

    // Grab the full text of the lines to move.
    let Some(block_start) = buf.iter_at_line(first) else {
        buf.end_user_action();
        return;
    };
    let block_end = if last >= line_count - 1 {
        buf.end_iter()
    } else {
        buf.iter_at_line(last + 1).unwrap_or_else(|| buf.end_iter())
    };
    let block_text = buf.text(&block_start, &block_end, true).to_string();

    // Also grab the swap line text.
    let swap_line = if up { first - 1 } else { last + 1 };
    let swap_text = get_line_text(&buf, swap_line).unwrap_or_default();

    // Remember cursor offset within the block.
    let cursor_iter = buf.iter_at_mark(&buf.get_insert());
    let cursor_line_in_block = cursor_iter.line() - first;
    let cursor_col = cursor_iter.line_offset();

    // Also remember selection bound for preserving selection.
    let sel_bound_iter = buf.iter_at_mark(&buf.selection_bound());
    let sel_line_in_block = sel_bound_iter.line() - first;
    let sel_col = sel_bound_iter.line_offset();
    let has_selection = buf.selection_bounds().is_some();

    // Determine the range to replace: min(first, swap_line) .. max(last, swap_line)+1
    let range_start_line = if up { swap_line } else { first };
    let range_end_line = if up { last } else { swap_line };

    let Some(mut rs) = buf.iter_at_line(range_start_line) else {
        buf.end_user_action();
        return;
    };
    let re = if range_end_line >= line_count - 1 {
        buf.end_iter()
    } else {
        buf.iter_at_line(range_end_line + 1)
            .unwrap_or_else(|| buf.end_iter())
    };
    let mut re = re;

    // Build the replacement: if moving up, block goes first then swap line.
    let new_text = if up {
        let block_needs_newline = !block_text.ends_with('\n');
        let swap_needs_newline = !swap_text.ends_with('\n') && range_end_line < line_count - 1;
        format!(
            "{}{}{}{}",
            block_text,
            if block_needs_newline { "\n" } else { "" },
            swap_text,
            if swap_needs_newline { "\n" } else { "" },
        )
    } else {
        let swap_needs_newline = !swap_text.ends_with('\n');
        let block_needs_newline = !block_text.ends_with('\n') && range_end_line < line_count - 1;
        format!(
            "{}{}{}{}",
            swap_text,
            if swap_needs_newline { "\n" } else { "" },
            block_text,
            if block_needs_newline { "\n" } else { "" },
        )
    };

    buf.delete(&mut rs, &mut re);
    let mark = buf.create_mark(None, &rs, true);
    buf.insert(&mut rs, &new_text);

    // Restore cursor position.
    let new_first = if up { first - 1 } else { first + 1 };
    let cursor_target_line = new_first + cursor_line_in_block;
    if let Some(line_iter) = buf.iter_at_line(cursor_target_line) {
        let line_text = get_line_text(&buf, cursor_target_line).unwrap_or_default();
        let max_col = line_text.chars().count() as i32;
        let mut target = line_iter;
        target.forward_chars(cursor_col.min(max_col));

        if has_selection {
            let sel_target_line = new_first + sel_line_in_block;
            if let Some(sel_iter) = buf.iter_at_line(sel_target_line) {
                let sel_text = get_line_text(&buf, sel_target_line).unwrap_or_default();
                let sel_max = sel_text.chars().count() as i32;
                let mut sel_target = sel_iter;
                sel_target.forward_chars(sel_col.min(sel_max));
                buf.select_range(&target, &sel_target);
            } else {
                buf.place_cursor(&target);
            }
        } else {
            buf.place_cursor(&target);
        }
    }

    buf.delete_mark(&mark);
    buf.end_user_action();
}

/// Duplicate the current line (or selected lines) below.
pub fn duplicate_line(view: &sourceview5::View) {
    let buf = view.buffer();
    let (first, last) = selected_line_range(&buf);

    buf.begin_user_action();

    // Collect the text of the block.
    let Some(block_start) = buf.iter_at_line(first) else {
        buf.end_user_action();
        return;
    };
    let block_end = if last < buf.line_count() - 1 {
        buf.iter_at_line(last + 1).unwrap_or_else(|| buf.end_iter())
    } else {
        let mut e = buf.end_iter();
        // Ensure we include the last line fully.
        if !e.starts_line() || e.line() == last {
            e = buf.end_iter();
        }
        e
    };

    let text = buf.text(&block_start, &block_end, true).to_string();

    // Insert the duplicate text right after the block.
    let needs_newline = !text.ends_with('\n');
    let insertion = if needs_newline {
        format!("\n{text}")
    } else {
        text.clone()
    };

    // Insert at the start of the line after the block.
    let insert_pos = if last < buf.line_count() - 1 {
        buf.iter_at_line(last + 1).unwrap_or_else(|| buf.end_iter())
    } else {
        buf.end_iter()
    };
    let mut ins = insert_pos;
    buf.insert(&mut ins, &insertion);

    buf.end_user_action();
}

/// Indent the current line(s) by one level (4 spaces).
pub fn indent(view: &sourceview5::View) {
    let buf = view.buffer();
    let (start_line, end_line) = selected_line_range(&buf);

    buf.begin_user_action();
    for ln in start_line..=end_line {
        if let Some(mut ls) = buf.iter_at_line(ln) {
            buf.insert(&mut ls, "    ");
        }
    }
    buf.end_user_action();
}

/// Outdent the current line(s) by one level (up to 4 leading spaces).
pub fn outdent(view: &sourceview5::View) {
    let buf = view.buffer();
    let (start_line, end_line) = selected_line_range(&buf);

    buf.begin_user_action();
    for ln in (start_line..=end_line).rev() {
        if let Some(text) = get_line_text(&buf, ln) {
            let spaces: usize = text.bytes().take_while(|&b| b == b' ').count().min(4);
            if spaces > 0
                && let Some(mut ls) = buf.iter_at_line(ln)
            {
                let mut end = ls;
                end.forward_chars(spaces as i32);
                buf.delete(&mut ls, &mut end);
            }
        }
    }
    buf.end_user_action();
}

// ---------------------------------------------------------------------------
// Smart Enter  (list / quote continuation)
// ---------------------------------------------------------------------------

/// Handle the Enter key with smart list continuation.
/// Returns `true` if the key press was consumed (caller should stop propagation).
pub fn handle_enter(view: &sourceview5::View) -> bool {
    let buf = view.buffer();

    // Only act when there's no selection (normal Enter at cursor).
    if buf.selection_bounds().is_some() {
        return false;
    }

    let cursor = buf.iter_at_mark(&buf.get_insert());
    let line = cursor.line();
    let Some(line_text) = get_line_text(&buf, line) else {
        return false;
    };

    // Check if line has a list / quote prefix.
    let Some(next_prefix) = next_list_prefix(&line_text) else {
        return false;
    };

    // Determine the content after the prefix on the current line.
    let plen = if let Some(l) = list_prefix_len(&line_text) {
        l
    } else if line_text.trim_start().starts_with("> ") {
        let indent_len = line_text.len() - line_text.trim_start().len();
        indent_len + 2
    } else {
        return false;
    };

    let content_after_prefix = &line_text[plen..];

    buf.begin_user_action();

    if content_after_prefix.trim().is_empty() {
        // Current item is empty — clear the prefix and exit the list.
        if let Some((mut ls, mut le)) = line_bounds(&buf, line) {
            buf.delete(&mut ls, &mut le);
            buf.place_cursor(&ls);
        }
    } else {
        // Continue the list: insert newline + prefix.
        let insert_text = format!("\n{next_prefix}");
        let mut ins = cursor;
        buf.insert(&mut ins, &insert_text);
        buf.place_cursor(&ins);
    }

    buf.end_user_action();
    true
}

// ---------------------------------------------------------------------------
// Smart Tab  (list indent / outdent, link tab-stop)
// ---------------------------------------------------------------------------

/// Handle the Tab / Shift+Tab key for list indentation.
/// Returns `true` if the key press was consumed.
pub fn handle_tab(view: &sourceview5::View, shift: bool) -> bool {
    let buf = view.buffer();
    let (start_line, end_line) = selected_line_range(&buf);

    // Check if any line in range is a list item.
    let any_list = (start_line..=end_line).any(|ln| {
        get_line_text(&buf, ln)
            .map(|t| list_prefix_len(&t).is_some())
            .unwrap_or(false)
    });

    if !any_list {
        // Try link tab-stop navigation (no-selection only).
        if !shift && buf.selection_bounds().is_none() {
            return handle_link_tab_stop(view);
        }
        return false;
    }

    buf.begin_user_action();

    if shift {
        // Outdent: remove up to 2 leading spaces per line.
        for ln in (start_line..=end_line).rev() {
            if let Some(text) = get_line_text(&buf, ln) {
                let spaces: usize = text.bytes().take_while(|&b| b == b' ').count().min(2);
                if spaces > 0
                    && let Some(mut ls) = buf.iter_at_line(ln)
                {
                    let mut end = ls;
                    end.forward_chars(spaces as i32);
                    buf.delete(&mut ls, &mut end);
                }
            }
        }
    } else {
        // Indent: add 2 spaces at the start of each line.
        for ln in start_line..=end_line {
            if let Some(mut ls) = buf.iter_at_line(ln) {
                buf.insert(&mut ls, "  ");
            }
        }
    }

    buf.end_user_action();
    true
}

/// Jump the cursor from inside `[…]` to inside `(…)` in a link/image.
fn handle_link_tab_stop(view: &sourceview5::View) -> bool {
    let buf = view.buffer();
    let cursor = buf.iter_at_mark(&buf.get_insert());

    // Look backwards for `[`, forwards for `](`.
    let mut scan = cursor;
    let max_scan = 200; // don't scan too far

    // Find the `]` after cursor, then expect `(`.
    for _ in 0..max_scan {
        if !scan.forward_char() {
            return false;
        }
        let ch = scan.char();
        if ch == ']' {
            let mut after_bracket = scan;
            if after_bracket.forward_char() && after_bracket.char() == '(' {
                // Jump cursor inside the parentheses.
                let mut target = after_bracket;
                target.forward_char();
                // Select the content inside (...) up to the closing paren.
                let mut sel_end = target;
                for _ in 0..max_scan {
                    if sel_end.char() == ')' {
                        break;
                    }
                    if !sel_end.forward_char() {
                        return false;
                    }
                }
                if sel_end.char() == ')' {
                    buf.select_range(&target, &sel_end);
                    return true;
                }
            }
            return false;
        }
        // If we encounter a newline, stop.
        if ch == '\n' {
            return false;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Table insertion
// ---------------------------------------------------------------------------

/// Insert a Markdown table at the next available blank line.
///
/// - If the cursor line is blank, the table is inserted there.
/// - If the cursor line contains text, the buffer is scanned forward for the
///   first blank line; if none is found the table is appended at the end of
///   the document.
pub fn insert_table(view: &sourceview5::View) {
    const TABLE: &str = "| Header 1 | Header 2 | Header 3 |\n\
                         | :--- | :---: | ---: |\n\
                         | Cell 1 | Cell 2 | Cell 3 |\n";

    let buf = view.buffer();
    buf.begin_user_action();

    let cursor = buf.iter_at_mark(&buf.get_insert());
    let cursor_line = cursor.line();
    let line_count = buf.line_count();

    let current_line_blank = get_line_text(&buf, cursor_line)
        .map(|t| t.trim().is_empty())
        .unwrap_or(true);

    if current_line_blank {
        // Insert at the start of the current blank line.
        if let Some(mut ins) = buf.iter_at_line(cursor_line) {
            buf.insert(&mut ins, TABLE);
            buf.place_cursor(&ins);
        }
    } else {
        // Find the next blank line below the cursor.
        let blank_line = ((cursor_line + 1)..line_count).find(|&ln| {
            get_line_text(&buf, ln)
                .map(|t| t.trim().is_empty())
                .unwrap_or(false)
        });

        if let Some(ln) = blank_line {
            if let Some(mut ins) = buf.iter_at_line(ln) {
                buf.insert(&mut ins, TABLE);
                buf.place_cursor(&ins);
            }
        } else {
            // No blank line found — append after end of document.
            let mut ins = buf.end_iter();
            let prefix = if ins.starts_line() { "\n" } else { "\n\n" };
            let full = format!("{prefix}{TABLE}");
            buf.insert(&mut ins, &full);
            buf.place_cursor(&ins);
        }
    }

    buf.end_user_action();
}

// ---------------------------------------------------------------------------
// View Mode helpers  (called from app.rs actions)
// ---------------------------------------------------------------------------

/// Cycle through Editor → Split → Preview → Editor.
pub fn cycle_view_mode(current: crate::state::ViewMode) -> crate::state::ViewMode {
    current.cycle()
}

/// Toggle split: if currently Split → Editor, else → Split.
pub fn toggle_split(current: crate::state::ViewMode) -> crate::state::ViewMode {
    match current {
        crate::state::ViewMode::Split => crate::state::ViewMode::Editor,
        _ => crate::state::ViewMode::Split,
    }
}
