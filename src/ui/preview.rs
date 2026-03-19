// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Preview panel — a WebKitGTK `WebView` for rendering Markdown as HTML.
//!
//! The preview displays Markdown rendered with stable `.md-*` CSS classes
//! inside a `<div class="jetmd-preview">` root container.  Theme CSS is
//! injected via a `<style id="theme-css">` element and can be hot-swapped
//! at runtime for instant theme switching.

use webkit6::prelude::*;

use crate::markdown;

/// Create and configure a WebKitGTK WebView for the Markdown preview.
///
/// `theme_css` is the full CSS content of the currently active preview
/// theme.  Pass an empty string to use no theme styling.
pub fn create_preview(dark: bool, theme_css: &str) -> webkit6::WebView {
    let webview = webkit6::WebView::new();
    webview.set_hexpand(true);
    webview.set_vexpand(true);

    // Disable context menu and navigation within the preview.
    let settings = WebViewExt::settings(&webview).expect("WebView has settings");
    settings.set_enable_developer_extras(false);
    settings.set_allow_modal_dialogs(false);

    // Load the initial HTML shell.
    let shell = markdown::build_preview_shell(dark, theme_css);
    webview.load_html(&shell, None);

    webview
}

/// Create a WebView and load the shell with pre-rendered HTML body.
///
/// Unlike `create_preview` + `update_content`, this embeds the body
/// directly in the initial `load_html` call, so content is visible
/// immediately without waiting for the page to finish loading.
pub fn create_preview_with_body(dark: bool, theme_css: &str, body_html: &str) -> webkit6::WebView {
    let webview = webkit6::WebView::new();
    webview.set_hexpand(true);
    webview.set_vexpand(true);

    let settings = WebViewExt::settings(&webview).expect("WebView has settings");
    settings.set_enable_developer_extras(false);
    settings.set_allow_modal_dialogs(false);

    let shell = markdown::build_preview_shell_with_body(dark, theme_css, body_html);
    webview.load_html(&shell, None);

    webview
}

/// Push new rendered-HTML into the WebView's `#content` div via JavaScript.
/// This preserves scroll position better than a full `load_html()` call.
pub fn update_content(webview: &webkit6::WebView, html_body: &str) {
    let escaped = html_body
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${");

    let script = format!("document.getElementById('content').innerHTML = `{escaped}`;");
    webview.evaluate_javascript(&script, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
}

/// Switch the preview between dark and light mode by changing the `<body>`
/// class.  Themes that key on `body.dark` / `body.light` will update
/// automatically.
pub fn set_theme(webview: &webkit6::WebView, dark: bool) {
    let class = if dark { "dark" } else { "light" };
    let script = format!("document.body.className = '{class}';");
    webview.evaluate_javascript(&script, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
}

/// Hot-swap the preview theme CSS.
///
/// Replaces the content of the `<style id="theme-css">` element in the
/// WebView with the provided CSS string.  This is near-instant (<50 ms)
/// and flicker-free because no page reload occurs.
pub fn apply_theme_css(webview: &webkit6::WebView, css: &str) {
    let escaped = css
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${");

    let script = format!("document.getElementById('theme-css').textContent = `{escaped}`;");
    webview.evaluate_javascript(&script, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
}
