// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Markdown rendering logic.
//!
//! This module converts raw Markdown text to HTML using `pulldown-cmark`.
//!
//! Two renderers are provided:
//!
//! - [`markdown_to_html`] — plain HTML, used for standalone export.
//! - [`markdown_to_preview_html`] — HTML with stable `.md-*` CSS classes,
//!   used for the live WebKit preview so that themes can target every
//!   element reliably.
//!
//! # CSS class contract (public theming API)
//!
//! All preview HTML is wrapped in `<div class="jetmd-preview">`.  Every
//! Markdown element carries a class from the following set:
//!
//! | Element       | Class(es)                                      |
//! |---------------|-------------------------------------------------|
//! | Heading       | `.md-h1` … `.md-h6`                             |
//! | Paragraph     | `.md-p`                                         |
//! | Link          | `.md-a`                                          |
//! | Unordered list| `.md-ul`                                         |
//! | Ordered list  | `.md-ol`                                         |
//! | List item     | `.md-li`                                         |
//! | Blockquote    | `.md-blockquote`                                 |
//! | Inline code   | `.md-code`                                       |
//! | Code block    | `.md-pre` (wrapper), `.md-codeblock` (inner)     |
//! | Table         | `.md-table`, `.md-thead`, `.md-tbody`, `.md-tr`  |
//! | Table header  | `.md-th`                                         |
//! | Table cell    | `.md-td`                                         |
//! | Image         | `.md-img`                                        |
//! | Horizontal rule| `.md-hr`                                        |
//! | Strong        | `.md-strong`                                     |
//! | Emphasis      | `.md-em`                                         |
//!
//! **These class names are part of the public theming API and MUST NOT
//! change without a major version bump.**

use std::fmt::Write;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};

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

/// Convert Markdown to HTML with stable `.md-*` CSS classes.
///
/// This is the renderer used for the live preview.  Every HTML element
/// carries a class from the public theming API so that user-installed
/// themes can target them reliably.
pub fn markdown_to_preview_html(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, parser_options());
    let mut out = String::with_capacity(markdown.len() + markdown.len() / 2);

    // State tracking.
    let mut in_table_head = false;
    let mut table_body_started = false;
    // For <img> we need to collect alt text between Start(Image) and End(Image).
    let mut image_data: Option<(String, String)> = None; // (dest_url, title)
    let mut image_alt = String::new();

    for event in parser {
        match event {
            // ----- Start tags ------------------------------------------
            Event::Start(tag) => match tag {
                Tag::Paragraph => out.push_str("<p class=\"md-p\">"),
                Tag::Heading { level, .. } => {
                    let n = level as usize;
                    let _ = write!(out, "<h{n} class=\"md-h{n}\">");
                }
                Tag::BlockQuote(_) => {
                    out.push_str("<blockquote class=\"md-blockquote\">");
                }
                Tag::CodeBlock(kind) => match &kind {
                    CodeBlockKind::Fenced(info) if !info.is_empty() => {
                        let lang = info.split_whitespace().next().unwrap_or("");
                        let _ = write!(
                            out,
                            "<pre class=\"md-pre\"><code class=\"md-codeblock language-{}\">",
                            escape_attr(lang)
                        );
                    }
                    _ => {
                        out.push_str("<pre class=\"md-pre\"><code class=\"md-codeblock\">");
                    }
                },
                Tag::List(Some(start)) => {
                    if start == 1 {
                        out.push_str("<ol class=\"md-ol\">");
                    } else {
                        let _ = write!(out, "<ol class=\"md-ol\" start=\"{start}\">");
                    }
                }
                Tag::List(None) => out.push_str("<ul class=\"md-ul\">"),
                Tag::Item => out.push_str("<li class=\"md-li\">"),
                Tag::Table(_) => {
                    table_body_started = false;
                    out.push_str("<table class=\"md-table\">");
                }
                Tag::TableHead => {
                    in_table_head = true;
                    out.push_str("<thead class=\"md-thead\"><tr class=\"md-tr\">");
                }
                Tag::TableRow => {
                    if !table_body_started {
                        out.push_str("<tbody class=\"md-tbody\">");
                        table_body_started = true;
                    }
                    out.push_str("<tr class=\"md-tr\">");
                }
                Tag::TableCell => {
                    if in_table_head {
                        out.push_str("<th class=\"md-th\">");
                    } else {
                        out.push_str("<td class=\"md-td\">");
                    }
                }
                Tag::Emphasis => out.push_str("<em class=\"md-em\">"),
                Tag::Strong => out.push_str("<strong class=\"md-strong\">"),
                Tag::Strikethrough => out.push_str("<del>"),
                Tag::Link {
                    dest_url, title, ..
                } => {
                    let _ = write!(out, "<a class=\"md-a\" href=\"{}\"", escape_attr(&dest_url));
                    if !title.is_empty() {
                        let _ = write!(out, " title=\"{}\"", escape_attr(&title));
                    }
                    out.push('>');
                }
                Tag::Image {
                    dest_url, title, ..
                } => {
                    image_data = Some((dest_url.to_string(), title.to_string()));
                    image_alt.clear();
                }
                Tag::HtmlBlock | Tag::MetadataBlock(_) => {}
                _ => {} // FootnoteDefinition, DefinitionList, etc.
            },

            // ----- End tags --------------------------------------------
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph => out.push_str("</p>\n"),
                TagEnd::Heading(level) => {
                    let n = level as usize;
                    let _ = write!(out, "</h{n}>\n");
                }
                TagEnd::BlockQuote(_) => out.push_str("</blockquote>\n"),
                TagEnd::CodeBlock => out.push_str("</code></pre>\n"),
                TagEnd::List(is_ordered) => {
                    if is_ordered {
                        out.push_str("</ol>\n");
                    } else {
                        out.push_str("</ul>\n");
                    }
                }
                TagEnd::Item => out.push_str("</li>\n"),
                TagEnd::Table => {
                    if table_body_started {
                        out.push_str("</tbody>");
                        table_body_started = false;
                    }
                    out.push_str("</table>\n");
                }
                TagEnd::TableHead => {
                    in_table_head = false;
                    out.push_str("</tr></thead>\n");
                }
                TagEnd::TableRow => out.push_str("</tr>\n"),
                TagEnd::TableCell => {
                    if in_table_head {
                        out.push_str("</th>");
                    } else {
                        out.push_str("</td>");
                    }
                }
                TagEnd::Emphasis => out.push_str("</em>"),
                TagEnd::Strong => out.push_str("</strong>"),
                TagEnd::Strikethrough => out.push_str("</del>"),
                TagEnd::Link => out.push_str("</a>"),
                TagEnd::Image => {
                    if let Some((dest_url, title)) = image_data.take() {
                        let _ = write!(
                            out,
                            "<img class=\"md-img\" src=\"{}\" alt=\"{}\"",
                            escape_attr(&dest_url),
                            escape_attr(&image_alt)
                        );
                        if !title.is_empty() {
                            let _ = write!(out, " title=\"{}\"", escape_attr(&title));
                        }
                        out.push_str(" />");
                    }
                    image_alt.clear();
                }
                TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
                _ => {}
            },

            // ----- Leaf events -----------------------------------------
            Event::Text(text) => {
                if image_data.is_some() {
                    // Collecting alt text for the current image.
                    image_alt.push_str(&text);
                } else {
                    escape_html_to(&text, &mut out);
                }
            }
            Event::Code(code) => {
                out.push_str("<code class=\"md-code\">");
                escape_html_to(&code, &mut out);
                out.push_str("</code>");
            }
            Event::Html(raw) | Event::InlineHtml(raw) => {
                out.push_str(&raw);
            }
            Event::SoftBreak => out.push('\n'),
            Event::HardBreak => out.push_str("<br />\n"),
            Event::Rule => out.push_str("<hr class=\"md-hr\" />\n"),
            Event::TaskListMarker(checked) => {
                if checked {
                    out.push_str("<input type=\"checkbox\" checked=\"\" disabled=\"\" /> ");
                } else {
                    out.push_str("<input type=\"checkbox\" disabled=\"\" /> ");
                }
            }
            Event::FootnoteReference(name) => {
                let _ = write!(
                    out,
                    "<sup class=\"footnote-ref\"><a href=\"#fn-{0}\">[{0}]</a></sup>",
                    escape_html(&name)
                );
            }
            _ => {} // DisplayMath, InlineMath — not enabled.
        }
    }

    out
}

// ---------------------------------------------------------------------------
// HTML-escaping helpers
// ---------------------------------------------------------------------------

/// Escape text for use inside HTML element content.
fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    escape_html_to(input, &mut out);
    out
}

/// Append HTML-escaped text to an existing buffer.
fn escape_html_to(input: &str, out: &mut String) {
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

/// Escape a string for use inside an HTML attribute value.
fn escape_attr(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
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

/// Minimal base CSS for the preview shell — only handles concerns that are
/// independent of the selected theme (body reset, scroll container sizing).
const SHELL_BASE_CSS: &str = r#"
*, *::before, *::after { box-sizing: border-box; }
body { margin: 0; padding: 0; overflow: hidden; }
.jetmd-preview { height: 100vh; overflow-y: auto; overflow-x: hidden; }
"#;

/// Build the initial HTML shell loaded into the WebView.
///
/// `theme_css` is the full CSS content of the currently selected preview
/// theme.  It is placed in a `<style id="theme-css">` element that can
/// be hot-swapped via JavaScript for instant theme switching.
pub fn build_preview_shell(dark: bool, theme_css: &str) -> String {
    build_preview_shell_with_body(dark, theme_css, "")
}

/// Build the HTML shell with pre-populated body content.
///
/// Use this instead of `build_preview_shell` + `update_content` when the
/// WebView hasn't finished its initial load yet.
pub fn build_preview_shell_with_body(dark: bool, theme_css: &str, body_html: &str) -> String {
    let body_class = if dark { "dark" } else { "light" };
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style id="shell-css">{SHELL_BASE_CSS}</style>
<style id="theme-css">{theme_css}</style>
</head>
<body class="{body_class}" oncontextmenu="return false;">
<div class="jetmd-preview" id="content">{body_html}</div>
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
        let shell = build_preview_shell(true, "body { color: red; }");
        assert!(shell.contains("id=\"theme-css\""));
        assert!(shell.contains("id=\"content\""));
        assert!(shell.contains("class=\"jetmd-preview\""));
        assert!(shell.contains("color: red"));
    }

    #[test]
    fn preview_html_heading_classes() {
        let html = markdown_to_preview_html("# Hello");
        assert!(html.contains("class=\"md-h1\""));
        assert!(html.contains("Hello"));
    }

    #[test]
    fn preview_html_paragraph_class() {
        let html = markdown_to_preview_html("Some text");
        assert!(html.contains("class=\"md-p\""));
    }

    #[test]
    fn preview_html_strong_and_em_classes() {
        let html = markdown_to_preview_html("**bold** and *italic*");
        assert!(html.contains("class=\"md-strong\""));
        assert!(html.contains("class=\"md-em\""));
    }

    #[test]
    fn preview_html_link_class() {
        let html = markdown_to_preview_html("[link](http://example.com)");
        assert!(html.contains("class=\"md-a\""));
        assert!(html.contains("href=\"http://example.com\""));
    }

    #[test]
    fn preview_html_inline_code_class() {
        let html = markdown_to_preview_html("Use `code` here");
        assert!(html.contains("class=\"md-code\""));
    }

    #[test]
    fn preview_html_code_block_classes() {
        let html = markdown_to_preview_html("```rust\nfn main() {}\n```");
        assert!(html.contains("class=\"md-pre\""));
        assert!(html.contains("class=\"md-codeblock language-rust\""));
    }

    #[test]
    fn preview_html_list_classes() {
        let html = markdown_to_preview_html("- one\n- two");
        assert!(html.contains("class=\"md-ul\""));
        assert!(html.contains("class=\"md-li\""));
    }

    #[test]
    fn preview_html_ordered_list_class() {
        let html = markdown_to_preview_html("1. first\n2. second");
        assert!(html.contains("class=\"md-ol\""));
    }

    #[test]
    fn preview_html_blockquote_class() {
        let html = markdown_to_preview_html("> A quote");
        assert!(html.contains("class=\"md-blockquote\""));
    }

    #[test]
    fn preview_html_hr_class() {
        let html = markdown_to_preview_html("---");
        assert!(html.contains("class=\"md-hr\""));
    }

    #[test]
    fn preview_html_image_class() {
        let html = markdown_to_preview_html("![Alt](img.png)");
        assert!(html.contains("class=\"md-img\""));
        assert!(html.contains("alt=\"Alt\""));
    }

    #[test]
    fn preview_html_table_classes() {
        let html = markdown_to_preview_html("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(html.contains("class=\"md-table\""));
        assert!(html.contains("class=\"md-thead\""));
        assert!(html.contains("class=\"md-th\""));
        assert!(html.contains("class=\"md-td\""));
        assert!(html.contains("class=\"md-tr\""));
    }

    #[test]
    fn preview_html_escapes_special_chars() {
        let html = markdown_to_preview_html("5 < 10 & \"yes\"");
        assert!(html.contains("5 &lt; 10 &amp; &quot;yes&quot;"));
    }
}
