// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Syntax highlighting for fenced code blocks.
//!
//! This module wraps [`syntect`] to provide two things:
//!
//! 1. **[`highlight_code`]** — converts a raw code string for a known
//!    language into a string of `<span class="hl-…">` elements that can be
//!    embedded directly inside `<code>…</code>`.
//!
//! 2. **[`highlight_css`]** — returns a `&'static str` containing the CSS
//!    rules for all token classes, scoped to `body.light` and `body.dark`
//!    so that syntax colours adapt automatically whenever the app toggles
//!    between the two appearances (see [`crate::ui::preview::set_theme`]).
//!
//! # Design choices
//!
//! * **CSS classes, not inline styles.**  `ClassedHTMLGenerator` emits
//!   `<span class="hl-keyword">` etc.  The matching CSS is injected once into
//!   the WebKit preview shell as `<style id="highlight-css">`.  This keeps
//!   the per-document HTML compact and lets themes override colours via CSS
//!   custom properties if desired.
//!
//! * **Lazy initialisation.**  `SyntaxSet` and `ThemeSet` are large objects
//!   (~3 MB each when fully loaded).  Both are initialised on first use and
//!   then cached for the lifetime of the process via [`OnceLock`].
//!
//! * **Graceful fallback.** [`highlight_code`] returns `None` for unrecognised
//!   language tokens; the caller is responsible for rendering plain escaped
//!   text in that case.

use std::sync::OnceLock;

use syntect::highlighting::ThemeSet;
use syntect::html::{ClassStyle, ClassedHTMLGenerator, css_for_theme_with_class_style};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

// ---------------------------------------------------------------------------
// Syntect globals (initialised once, reused for every render)
// ---------------------------------------------------------------------------

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// CSS class style used for all generated token spans.
///
/// `SpacedPrefixed { prefix: "hl-" }` produces classes like `hl-keyword`,
/// `hl-string`, etc., which are unlikely to collide with application CSS.
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Syntax-highlight `code` for language `lang` (e.g. `"rust"`, `"python"`).
///
/// Returns a string of `<span class="hl-…">…</span>` elements suitable for
/// embedding inside a `<code>` element.  Returns `None` when the language
/// token is not recognised by the built-in syntax set, allowing the caller
/// to fall back to plain HTML-escaped text.
pub fn highlight_code(code: &str, lang: &str) -> Option<String> {
    let ss = syntax_set();
    let syntax = ss.find_syntax_by_token(lang)?;
    let mut generator = ClassedHTMLGenerator::new_with_class_style(syntax, ss, CLASS_STYLE);
    for line in LinesWithEndings::from(code) {
        generator
            .parse_html_for_line_which_includes_newline(line)
            .ok()?;
    }
    Some(generator.finalize())
}

/// Return the combined syntax-highlight CSS for light and dark appearances.
///
/// Rules are scoped with `body.light` and `body.dark` prefixes so they
/// switch automatically when `document.body.className` is toggled by
/// [`crate::ui::preview::set_theme`].
///
/// The returned `&'static str` is computed on first call and cached.
pub fn highlight_css() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        let ts = theme_set();
        let light_raw = css_for_theme_with_class_style(&ts.themes["InspiredGitHub"], CLASS_STYLE)
            .unwrap_or_default();
        let dark_raw = css_for_theme_with_class_style(&ts.themes["base16-ocean.dark"], CLASS_STYLE)
            .unwrap_or_default();
        let mut css = scope_css(&light_raw, "body.light");
        css.push('\n');
        css.push_str(&scope_css(&dark_raw, "body.dark"));
        css
    })
}

// ---------------------------------------------------------------------------
// CSS scoping helper
// ---------------------------------------------------------------------------

/// Prefix every CSS selector in `raw` with `scope`.
///
/// For example, given `scope = "body.light"` and a rule `.hl-keyword { … }`,
/// the output will be `body.light .hl-keyword { … }`.
/// Comma-separated selectors are each prefixed individually.
fn scope_css(raw: &str, scope: &str) -> String {
    let mut out = String::with_capacity(raw.len() + raw.len() / 4);
    for chunk in raw.split('}') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        if let Some((selector_part, body_part)) = chunk.split_once('{') {
            let selector = selector_part.trim();
            let body = body_part.trim();
            if selector.is_empty() || body.is_empty() {
                continue;
            }
            let scoped: Vec<String> = selector
                .split(',')
                .map(|s| format!("{} {}", scope, s.trim()))
                .collect();
            out.push_str(&scoped.join(", "));
            out.push_str(" { ");
            out.push_str(body);
            out.push_str(" }\n");
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_rust_produces_spans() {
        let html = highlight_code("fn main() {}", "rust").expect("rust is a known language");
        assert!(html.contains("<span"), "expected span elements in output");
        assert!(html.contains("fn"), "source text must appear somewhere");
    }

    #[test]
    fn highlight_unknown_lang_returns_none() {
        assert!(highlight_code("hello", "frobnicator99").is_none());
    }

    #[test]
    fn highlight_css_contains_both_modes() {
        let css = highlight_css();
        assert!(css.contains("body.light"), "light rules expected");
        assert!(css.contains("body.dark"), "dark rules expected");
    }

    #[test]
    fn scope_css_prefixes_selectors() {
        let raw = ".hl-keyword { color: red; }\n.hl-string { color: blue; }\n";
        let scoped = scope_css(raw, "body.light");
        assert!(scoped.contains("body.light .hl-keyword"));
        assert!(scoped.contains("body.light .hl-string"));
        assert!(!scoped.contains("\n.hl-keyword")); // original unscopd rules should be gone
    }
}
