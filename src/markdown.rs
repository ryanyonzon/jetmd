// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Markdown rendering logic.
//!
//! This module converts raw Markdown text to HTML using `pulldown-cmark`.

use pulldown_cmark::{Options, Parser, html};

// ---------------------------------------------------------------------------
// Markdown → HTML  (used for preview and export)
// ---------------------------------------------------------------------------

/// Return the `pulldown-cmark` options we use throughout the application.
pub fn parser_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

/// Convert a Markdown string to an HTML fragment.
pub fn markdown_to_html(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, parser_options());
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// Wrap an HTML body fragment in a full HTML document for standalone export.
pub fn wrap_html_document(body_html: &str, title: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{title}</title>
  <style>
    body {{
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
      max-width: 800px; margin: 0 auto; padding: 20px; line-height: 1.6;
      color: #24292e;
    }}
    code {{ background: #f6f8fa; padding: 2px 6px; border-radius: 3px; font-size: 0.9em; }}
    pre  {{ background: #f6f8fa; padding: 16px; border-radius: 6px; overflow-x: auto; }}
    pre code {{ background: none; padding: 0; }}
    blockquote {{ border-left: 4px solid #dfe2e5; margin: 0; padding: 0 16px; color: #6a737d; }}
    table {{ border-collapse: collapse; }}
    th, td {{ border: 1px solid #dfe2e5; padding: 8px 12px; }}
    th {{ background: #f6f8fa; }}
    img {{ max-width: 100%; }}
    a {{ color: #0366d6; }}
  </style>
</head>
<body>
{body_html}
</body>
</html>"#
    )
}

// ---------------------------------------------------------------------------
// Preview HTML shell (used by the WebKitGTK preview)
// ---------------------------------------------------------------------------

/// Comprehensive CSS for the Markdown preview.  CSS custom properties
/// switch between light and dark palettes via a body class.
const PREVIEW_CSS: &str = r#"
:root {
    --bg: #ffffff; --fg: #24292e;
    --code-bg: #f6f8fa; --code-fg: #24292e;
    --border: #dfe2e5; --link: #0366d6;
    --bq-fg: #6a737d; --bq-border: #dfe2e5;
    --th-bg: #f6f8fa;
    --selection: rgba(3,102,214,0.2);
}
body.dark {
    --bg: #1e1e1e; --fg: #d4d4d4;
    --code-bg: #2d2d2d; --code-fg: #d4d4d4;
    --border: #444; --link: #58a6ff;
    --bq-fg: #8b949e; --bq-border: #444;
    --th-bg: #2d2d2d;
    --selection: rgba(88,166,255,0.25);
}
*, *::before, *::after { box-sizing: border-box; }
::selection { background: var(--selection); }
body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica,
                 Arial, sans-serif, 'Apple Color Emoji', 'Segoe UI Emoji';
    margin: 0; padding: 16px 20px; line-height: 1.6;
    color: var(--fg); background: var(--bg);
    overflow-y: auto; overflow-x: hidden; word-wrap: break-word;
}
h1, h2, h3, h4, h5, h6 { margin-top: 1.2em; margin-bottom: 0.6em; font-weight: 600; line-height: 1.3; }
h1 { font-size: 2em; border-bottom: 1px solid var(--border); padding-bottom: 0.3em; }
h2 { font-size: 1.5em; border-bottom: 1px solid var(--border); padding-bottom: 0.3em; }
h3 { font-size: 1.25em; } h4 { font-size: 1em; }
h5 { font-size: 0.875em; } h6 { font-size: 0.85em; color: var(--bq-fg); }
p { margin: 0.5em 0 1em; }
a { color: var(--link); text-decoration: none; }
a:hover { text-decoration: underline; }
code {
    background: var(--code-bg); color: var(--code-fg);
    padding: 2px 6px; border-radius: 3px; font-size: 0.9em;
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
}
pre { background: var(--code-bg); padding: 16px; border-radius: 6px; overflow-x: auto; line-height: 1.45; }
pre code { background: none; padding: 0; font-size: 0.9em; color: var(--code-fg); }
blockquote {
    border-left: 4px solid var(--bq-border); margin: 0.5em 0;
    padding: 0.25em 1em; color: var(--bq-fg);
}
blockquote > :first-child { margin-top: 0; }
blockquote > :last-child { margin-bottom: 0; }
table { border-collapse: collapse; width: auto; margin: 1em 0; }
th, td { border: 1px solid var(--border); padding: 8px 12px; text-align: left; }
th { background: var(--th-bg); font-weight: 600; }
ul, ol { padding-left: 2em; margin: 0.5em 0; }
li { margin: 0.25em 0; } li > p { margin: 0.25em 0; }
ul.contains-task-list, li.task-list-item { list-style-type: none; }
ul.contains-task-list { padding-left: 0; }
input[type="checkbox"] { margin-right: 0.5em; }
hr { border: none; border-top: 1px solid var(--border); margin: 1.5em 0; }
img { max-width: 100%; }
del { color: var(--bq-fg); }
/* -- Find-in-preview highlights --------------------------------------- */
mark.search-hl {
    background: rgba(255,210,0,0.45);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
}
mark.search-hl.active {
    background: rgba(255,165,0,0.8);
    outline: 2px solid #ff8c00;
    outline-offset: 1px;
}
body.dark mark.search-hl {
    background: rgba(255,210,0,0.35);
}
body.dark mark.search-hl.active {
    background: rgba(255,165,0,0.7);
    outline-color: #ff8c00;
}
"#;

/// Build the initial HTML shell loaded into the WebView.
pub fn build_preview_shell(dark: bool, custom_css: &str) -> String {
    build_preview_shell_with_body(dark, custom_css, "")
}

/// Build the HTML shell with pre-populated body content.
///
/// Use this instead of `build_preview_shell` + `update_content` when the
/// WebView hasn't finished its initial load yet.
pub fn build_preview_shell_with_body(dark: bool, custom_css: &str, body_html: &str) -> String {
    let theme_class = if dark { "dark" } else { "light" };
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style id="default-css">{PREVIEW_CSS}</style>
<style id="user-css">{custom_css}</style>
</head>
<body class="{theme_class}" oncontextmenu="return false;">
<div id="content">{body_html}</div>
<script>document.addEventListener('click',function(e){{var a=e.target.closest('a');if(a)e.preventDefault();}},true);</script>
</body>
</html>"#
    )
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_h1() {
        let html = markdown_to_html("# Hello");
        assert!(html.contains("<h1>"));
        assert!(html.contains("Hello"));
        assert!(html.contains("</h1>"));
    }

    #[test]
    fn heading_h2() {
        let html = markdown_to_html("## Sub-heading");
        assert!(html.contains("<h2>"));
    }

    #[test]
    fn bold_text() {
        let html = markdown_to_html("**bold**");
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn italic_text() {
        let html = markdown_to_html("*italic*");
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn strikethrough_text() {
        let html = markdown_to_html("~~struck~~");
        assert!(html.contains("<del>struck</del>"));
    }

    #[test]
    fn inline_code() {
        let html = markdown_to_html("Use `code` here");
        assert!(html.contains("<code>code</code>"));
    }

    #[test]
    fn code_block() {
        let md = "```rust\nfn main() {}\n```";
        let html = markdown_to_html(md);
        assert!(html.contains("<pre>"));
        assert!(html.contains("<code"));
        assert!(html.contains("fn main()"));
    }

    #[test]
    fn unordered_list() {
        let md = "- one\n- two\n- three";
        let html = markdown_to_html(md);
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>one</li>"));
    }

    #[test]
    fn ordered_list() {
        let md = "1. first\n2. second";
        let html = markdown_to_html(md);
        assert!(html.contains("<ol>"));
        assert!(html.contains("<li>first</li>"));
    }

    #[test]
    fn link() {
        let md = "[Rust](https://www.rust-lang.org)";
        let html = markdown_to_html(md);
        assert!(html.contains("<a href=\"https://www.rust-lang.org\">Rust</a>"));
    }

    #[test]
    fn image() {
        let md = "![Alt text](image.png)";
        let html = markdown_to_html(md);
        assert!(html.contains("<img"));
        assert!(html.contains("src=\"image.png\""));
    }

    #[test]
    fn blockquote() {
        let md = "> A quote";
        let html = markdown_to_html(md);
        assert!(html.contains("<blockquote>"));
    }

    #[test]
    fn horizontal_rule() {
        let html = markdown_to_html("---");
        assert!(html.contains("<hr"));
    }

    #[test]
    fn table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let html = markdown_to_html(md);
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>A</th>"));
        assert!(html.contains("<td>1</td>"));
    }

    #[test]
    fn task_list() {
        let md = "- [x] done\n- [ ] todo";
        let html = markdown_to_html(md);
        assert!(html.contains("checked"));
        assert!(html.contains("done"));
    }

    #[test]
    fn empty_input() {
        assert_eq!(markdown_to_html(""), "");
    }

    #[test]
    fn utf8_content() {
        let html = markdown_to_html("# 日本語 🦀");
        assert!(html.contains("日本語"));
        assert!(html.contains("🦀"));
    }

    #[test]
    fn wrap_html_document_contains_boilerplate() {
        let doc = wrap_html_document("<p>hi</p>", "Test");
        assert!(doc.contains("<!DOCTYPE html>"));
        assert!(doc.contains("<title>Test</title>"));
        assert!(doc.contains("<p>hi</p>"));
    }

    #[test]
    fn nested_formatting() {
        let md = "***bold and italic***";
        let html = markdown_to_html(md);
        assert!(html.contains("<strong>"));
        assert!(html.contains("<em>"));
    }

    #[test]
    fn paragraph_separation() {
        let md = "First paragraph.\n\nSecond paragraph.";
        let html = markdown_to_html(md);
        assert_eq!(html.matches("<p>").count(), 2);
    }

    #[test]
    fn multiline_paragraph() {
        let md = "Line one\nLine two";
        let html = markdown_to_html(md);
        assert!(html.contains("Line one"));
        assert!(html.contains("Line two"));
    }

    #[test]
    fn preview_shell_contains_css() {
        let shell = build_preview_shell(true, "");
        assert!(shell.contains("body.dark"));
        assert!(shell.contains("id=\"content\""));
    }
}
