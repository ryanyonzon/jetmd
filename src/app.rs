// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Main application window — builds the GTK 4 UI and wires up actions.
//!
//! Uses a `gtk4::Notebook` to provide multi-tab / multi-document editing.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Once;
use std::time::Instant;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use webkit6::prelude::*;

use crate::autosave::AutosaveManager;
use crate::file_io;
use crate::markdown;
use crate::recent_files::RecentFilesManager;
use crate::state::{AppState, Document, Theme, ViewMode};
use crate::ui::{editor, find_replace, preview, toolbar};
use crate::xdg::{self, AppConfig, AppDirectories};

// ---------------------------------------------------------------------------
// Per-tab data
// ---------------------------------------------------------------------------

/// Widgets and metadata for a single editor tab.
struct TabWidgets {
    document: Rc<RefCell<Document>>,
    source_view: sourceview5::View,
    webview: webkit6::WebView,
    editor_scroll: gtk4::ScrolledWindow,
    paned: gtk4::Paned,
    find_bar: find_replace::FindReplaceBar,
    tab_label: gtk4::Label,
    page_widget: gtk4::Box,
    debounce_id: Rc<RefCell<Option<glib::SourceId>>>,
    /// Signal handler id for the buffer `changed` signal (disconnected on tab close).
    buffer_changed_handler: glib::SignalHandlerId,
}

/// Shared references passed to helpers and action closures.
#[derive(Clone)]
struct AppContext {
    notebook: gtk4::Notebook,
    state: Rc<RefCell<AppState>>,
    tabs: Rc<RefCell<Vec<TabWidgets>>>,
    window: gtk4::ApplicationWindow,
    status_right: gtk4::Label,
    status_autosave: gtk4::Label,
    /// The `gio::Menu` backing the recent-files dropdown (rebuilt dynamically).
    recent_menu: gio::Menu,
    /// Suppresses the `switch-page` handler during programmatic tab removal.
    suppress_switch: Rc<Cell<bool>>,
    /// Allows a programmatic window close to bypass the unsaved-work prompt.
    allow_window_close: Rc<Cell<bool>>,
    dirs: Rc<AppDirectories>,
    recent_files: Rc<RecentFilesManager>,
    autosave: Rc<AutosaveManager>,
}

#[derive(Clone, Default)]
struct InitialTab {
    content: Option<String>,
    file_path: Option<std::path::PathBuf>,
    draft_id: Option<String>,
    modified: bool,
}

// ---------------------------------------------------------------------------
// Window construction
// ---------------------------------------------------------------------------

const NOTEBOOK_CSS: &str = "
notebook.jetmd-tab-section > header.top,
notebook.jetmd-tab-section > header.top tabs,
notebook.jetmd-tab-section > header.top tab {
    background-color: #121212;
}
";

static NOTEBOOK_CSS_INIT: Once = Once::new();
const DRAFT_AUTO_SAVE_INTERVAL_SECS: u64 = 30;

/// Build the application window and connect all signals/actions.
pub fn build_window(app: &gtk4::Application, initial_file: Option<String>) {
    ensure_notebook_css();

    let dirs =
        Rc::new(AppDirectories::resolve().expect("failed to resolve application directories"));
    let persisted_config = xdg::load_app_config(&dirs).unwrap_or_default();
    let recent_files = Rc::new(RecentFilesManager::new(&dirs));
    let autosave = Rc::new(AutosaveManager::new(&dirs));

    let mut initial_state = AppState::new();
    initial_state.theme = Theme::from_persisted(&persisted_config.theme);
    initial_state.auto_save_enabled = persisted_config.auto_save_enabled;
    initial_state.recent_files = recent_files.load().unwrap_or_default();

    let state = Rc::new(RefCell::new(initial_state));
    let tabs: Rc<RefCell<Vec<TabWidgets>>> = Rc::new(RefCell::new(Vec::new()));

    let tb = toolbar::create_top_bar_widgets();

    // ---- Status bar -------------------------------------------------------
    let status_right = gtk4::Label::new(Some("0 lines · 0 bytes"));
    status_right.add_css_class("dim-label");
    status_right.set_halign(gtk4::Align::End);
    status_right.set_hexpand(true);

    let status_autosave = gtk4::Label::new(Some(if persisted_config.auto_save_enabled {
        "Auto-save: On"
    } else {
        "Auto-save: Off"
    }));
    status_autosave.add_css_class("dim-label");
    status_autosave.set_halign(gtk4::Align::End);

    let status_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    status_bar.set_margin_start(8);
    status_bar.set_margin_end(8);
    status_bar.set_margin_top(2);
    status_bar.set_margin_bottom(2);
    status_bar.append(&status_right);
    status_bar.append(&status_autosave);

    // ---- Notebook (tab container) -----------------------------------------
    let notebook = gtk4::Notebook::new();
    notebook.set_scrollable(true);
    notebook.set_hexpand(true);
    notebook.set_vexpand(true);
    notebook.add_css_class("jetmd-tab-section");

    // ---- Titlebar ---------------------------------------------------------
    let titlebar = gtk4::HeaderBar::new();
    titlebar.set_show_title_buttons(true);

    let titlebar_start = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    titlebar_start.append(&tb.open_btn);
    titlebar_start.append(&tb.recent_btn);
    titlebar_start.append(&tb.new_tab_btn);
    titlebar.pack_start(&titlebar_start);

    let titlebar_end = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    titlebar_end.append(&tb.hamburger_btn);
    titlebar.pack_end(&titlebar_end);

    // ---- Layout -----------------------------------------------------------
    let main_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    main_box.append(&notebook);
    main_box.append(&status_bar);

    // ---- Window -----------------------------------------------------------
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("Untitled")
        .default_width(1100)
        .default_height(720)
        .child(&main_box)
        .build();
    window.set_titlebar(Some(&titlebar));
    window.maximize();

    // Set initial dark theme.
    apply_global_theme(matches!(state.borrow().theme, Theme::Dark));

    toolbar::rebuild_recent_menu(&tb.recent_menu, &state.borrow().recent_files);

    // ---- Shared context ---------------------------------------------------
    let ctx = AppContext {
        notebook: notebook.clone(),
        state: state.clone(),
        tabs: tabs.clone(),
        window: window.clone(),
        status_right: status_right.clone(),
        status_autosave: status_autosave.clone(),
        recent_menu: tb.recent_menu.clone(),
        suppress_switch: Rc::new(Cell::new(false)),
        allow_window_close: Rc::new(Cell::new(false)),
        dirs,
        recent_files,
        autosave,
    };

    // ---- Wire open button to open-file action --------------------------------
    {
        let win_c = ctx.window.clone();
        tb.open_btn.connect_clicked(move |_| {
            gtk4::prelude::WidgetExt::activate_action(
                &win_c,
                "win.open-file",
                None::<&glib::Variant>,
            )
            .ok();
        });
    }

    // ---- Wire "+" button to new tab ----------------------------------------
    {
        let ctx_c = ctx.clone();
        tb.new_tab_btn.connect_clicked(move |_| {
            create_new_tab(&ctx_c, InitialTab::default());
        });
    }

    // ---- Create initial tab -----------------------------------------------
    let restored_drafts = ctx.autosave.restore_drafts().unwrap_or_default();
    for draft in restored_drafts {
        create_new_tab(
            &ctx,
            InitialTab {
                content: Some(draft.content),
                file_path: draft.file_path,
                draft_id: Some(draft.draft_id),
                modified: true,
            },
        );
    }

    let mut opened_any = ctx.notebook.n_pages() > 0;
    if let Some(path_str) = initial_file {
        let path = std::path::PathBuf::from(&path_str);
        let already_open = {
            let tabs_ref = ctx.tabs.borrow();
            tabs_ref
                .iter()
                .any(|tw| tw.document.borrow().file_path.as_ref() == Some(&path))
        };

        if !already_open {
            match file_io::read_file(&path) {
                Ok(content) => {
                    create_new_tab(
                        &ctx,
                        InitialTab {
                            content: Some(content),
                            file_path: Some(path.clone()),
                            draft_id: None,
                            modified: false,
                        },
                    );
                    add_recent_file_and_refresh(&ctx, path);
                    opened_any = true;
                }
                Err(e) => {
                    eprintln!("Failed to open: {e}");
                }
            }
        }
    }

    if !opened_any {
        create_new_tab(&ctx, InitialTab::default());
    }

    // ---- Tab switch signal ------------------------------------------------
    {
        let ctx_c = ctx.clone();
        notebook.connect_switch_page(move |_nb, _page, page_num| {
            if ctx_c.suppress_switch.get() {
                return;
            }
            let tabs_ref = ctx_c.tabs.borrow();
            if let Some(tw) = tabs_ref.get(page_num as usize) {
                ctx_c.window.set_title(Some(&tw.document.borrow().title()));
                let text = editor::get_text(&tw.source_view);
                let lines = text.lines().count();
                let bytes = text.len();
                ctx_c
                    .status_right
                    .set_text(&format!("{lines} lines · {bytes} bytes"));
            }
        });
    }

    // ---- Auto-save timer --------------------------------------------------
    {
        let ctx_c = ctx.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
            let file_auto_save_enabled = ctx_c.state.borrow().should_auto_save();
            let mut save_targets: Vec<(Rc<RefCell<Document>>, std::path::PathBuf, String)> =
                Vec::new();
            let mut draft_targets: Vec<(
                Rc<RefCell<Document>>,
                Option<std::path::PathBuf>,
                String,
            )> = Vec::new();
            {
                let tabs_ref = ctx_c.tabs.borrow();
                for tw in tabs_ref.iter() {
                    let text = editor::get_text(&tw.source_view);
                    let doc = tw.document.borrow();

                    if file_auto_save_enabled && doc.modified {
                        if let Some(path) = doc.file_path.clone() {
                            save_targets.push((tw.document.clone(), path, text.clone()));
                        }
                    }

                    let needs_draft_save = doc.modified
                        && doc
                            .last_draft_save
                            .map(|saved_at| {
                                saved_at.elapsed().as_secs() >= DRAFT_AUTO_SAVE_INTERVAL_SECS
                            })
                            .unwrap_or(true);
                    if needs_draft_save {
                        draft_targets.push((tw.document.clone(), doc.file_path.clone(), text));
                    }
                }
            }

            let had_draft_targets = !draft_targets.is_empty();
            for (doc_rc, original_path, text) in draft_targets {
                let draft_id = doc_rc.borrow().draft_id.clone();
                match ctx_c.autosave.save_draft(
                    draft_id.as_deref(),
                    original_path.as_deref(),
                    &text,
                ) {
                    Ok(saved_draft_id) => {
                        let mut doc = doc_rc.borrow_mut();
                        doc.draft_id = Some(saved_draft_id);
                        doc.mark_draft_saved();
                    }
                    Err(error) => {
                        ctx_c
                            .state
                            .borrow_mut()
                            .set_status(format!("Draft auto-save failed: {error}"));
                    }
                }
            }

            let mut any_saved = false;
            for (doc_rc, path, text) in save_targets {
                match file_io::write_file(&path, &text) {
                    Ok(()) => {
                        doc_rc.borrow_mut().mark_saved();
                        cleanup_document_draft(&ctx_c, &doc_rc);
                        any_saved = true;
                    }
                    Err(e) => {
                        ctx_c
                            .state
                            .borrow_mut()
                            .set_status(format!("Auto-save failed: {e}"));
                    }
                }
            }

            if any_saved {
                let mut st = ctx_c.state.borrow_mut();
                st.last_auto_save = Instant::now();
                st.set_status("Auto-saved");
            }

            if !had_draft_targets && !any_saved {
                return glib::ControlFlow::Continue;
            }

            // Refresh tab labels after save.
            {
                let tabs_ref = ctx_c.tabs.borrow();
                for tw in tabs_ref.iter() {
                    tw.tab_label.set_text(&tw.document.borrow().title());
                }
                if let Some(page) = ctx_c.notebook.current_page() {
                    if let Some(tw) = tabs_ref.get(page as usize) {
                        ctx_c.window.set_title(Some(&tw.document.borrow().title()));
                    }
                }
            }

            glib::ControlFlow::Continue
        });
    }

    // ---- Actions ----------------------------------------------------------
    setup_actions(&ctx);

    // Set keyboard shortcuts on the application.
    setup_accels(app);

    connect_close_handler(&ctx);

    window.present();

    // Give focus to the editor of the first tab.
    if let Some(tw) = tabs.borrow().first() {
        tw.source_view.grab_focus();
    }
}

// ---------------------------------------------------------------------------
// Tab creation
// ---------------------------------------------------------------------------

/// Create a new editor tab, add it to the notebook, and switch to it.
fn create_new_tab(ctx: &AppContext, initial: InitialTab) {
    let dark = matches!(ctx.state.borrow().theme, Theme::Dark);
    let view_mode = ctx.state.borrow().view_mode;

    // -- Per-tab widgets ----------------------------------------------------
    let source_view = editor::create_editor();
    editor::apply_theme(&source_view, dark);

    // Build the preview WebView — if we already have content, embed it in the
    // initial HTML shell so it is visible without waiting for the page load.
    let initial_html = initial.content.as_deref().map(markdown::markdown_to_html);
    let webview = match initial_html.as_deref() {
        Some(body) => preview::create_preview_with_body(dark, body),
        None => preview::create_preview(dark),
    };

    let sv_buffer = source_view
        .buffer()
        .downcast::<sourceview5::Buffer>()
        .expect("editor buffer is a sourceview5::Buffer");

    let find_bar = find_replace::FindReplaceBar::new(&sv_buffer, &source_view, &webview);
    find_bar.set_dark(dark);

    let editor_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(&source_view)
        .build();

    let paned = gtk4::Paned::new(gtk4::Orientation::Horizontal);
    paned.set_start_child(Some(&editor_scroll));
    paned.set_end_child(Some(&webview));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);
    paned.set_wide_handle(true);
    paned.set_hexpand(true);
    paned.set_vexpand(true);

    // Apply current view mode.
    match view_mode {
        ViewMode::Editor => {
            editor_scroll.set_visible(true);
            webview.set_visible(false);
        }
        ViewMode::Split => {
            editor_scroll.set_visible(true);
            webview.set_visible(true);
        }
        ViewMode::Preview => {
            editor_scroll.set_visible(false);
            webview.set_visible(true);
        }
    }

    // Page container: an Overlay wrapping the full paned so the find panel
    // floats over the entire tab area (editor + preview) at the top-right.
    let page_overlay = gtk4::Overlay::new();
    page_overlay.set_child(Some(&paned));
    page_overlay.add_overlay(&find_bar.panel);
    page_overlay.set_hexpand(true);
    page_overlay.set_vexpand(true);

    let page_widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    page_widget.append(&page_overlay);

    // -- Document -----------------------------------------------------------
    let document = Rc::new(RefCell::new(Document::new()));
    {
        let mut doc = document.borrow_mut();
        doc.file_path = initial.file_path.clone();
        doc.draft_id = initial.draft_id.clone();
        doc.modified = initial.modified;
        if doc.draft_id.is_some() {
            doc.mark_draft_saved();
        }
    }

    // -- Set initial content BEFORE connecting signals ----------------------
    if let Some(text) = initial.content.as_deref() {
        editor::set_text(&source_view, text);
        // Preview already loaded with content via create_preview_with_body.
        document.borrow_mut().modified = initial.modified;
    }

    // -- Tab label with close button ----------------------------------------
    let title = document.borrow().title();
    let tab_label = gtk4::Label::new(Some(&title));

    let close_btn = gtk4::Button::from_icon_name("window-close-symbolic");
    close_btn.set_has_frame(false);
    close_btn.add_css_class("flat");

    let tab_label_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    tab_label_box.append(&tab_label);
    tab_label_box.append(&close_btn);

    // -- Buffer changed signal (connected AFTER initial content) ------------
    let debounce_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let handler_id;
    {
        let doc = document.clone();
        let tab_lbl = tab_label.clone();
        let webview_c = webview.clone();
        let status_right_c = ctx.status_right.clone();
        let window_c = ctx.window.clone();
        let notebook_c = ctx.notebook.clone();
        let page_widget_c = page_widget.clone();
        let deb = debounce_id.clone();

        handler_id = sv_buffer.connect_changed(move |buf| {
            doc.borrow_mut().modified = true;
            tab_lbl.set_text(&doc.borrow().title());

            let (start, end) = buf.bounds();
            let text = buf.text(&start, &end, true);

            // Debounced preview update.
            if let Some(id) = deb.borrow_mut().take() {
                unsafe {
                    glib::ffi::g_source_remove(id.as_raw());
                }
            }
            let text_owned = text.to_string();
            let webview_cc = webview_c.clone();
            let deb_inner = deb.clone();
            *deb.borrow_mut() = Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(200),
                move || {
                    deb_inner.borrow_mut().take();
                    let html = markdown::markdown_to_html(&text_owned);
                    preview::update_content(&webview_cc, &html);
                },
            ));

            // Update status bar / window title only if this is the active tab.
            let page_num = notebook_c.page_num(&page_widget_c);
            let current = notebook_c.current_page();
            if page_num.is_some() && page_num == current {
                let lines = text.lines().count();
                let bytes = text.len();
                status_right_c.set_text(&format!("{lines} lines · {bytes} bytes"));
                window_c.set_title(Some(&doc.borrow().title()));
            }
        });
    }

    // -- Add page to notebook -----------------------------------------------
    let page_idx = ctx.notebook.append_page(&page_widget, Some(&tab_label_box));

    // -- Store tab widgets --------------------------------------------------
    let tw = TabWidgets {
        document: document.clone(),
        source_view: source_view.clone(),
        webview,
        editor_scroll,
        paned: paned.clone(),
        find_bar,
        tab_label,
        page_widget: page_widget.clone(),
        debounce_id,
        buffer_changed_handler: handler_id,
    };
    ctx.tabs.borrow_mut().push(tw);

    // -- Close button handler -----------------------------------------------
    {
        let ctx_c = ctx.clone();
        let page_widget_c = page_widget.clone();
        close_btn.connect_clicked(move |_| {
            request_close_tab_by_widget(&ctx_c, &page_widget_c);
        });
    }

    // -- Paned position (50/50 split after allocation) ----------------------
    {
        let paned_c = paned.clone();
        paned.connect_map(move |_| {
            let p = paned_c.clone();
            glib::idle_add_local_once(move || {
                let width = p.width();
                if width > 0 {
                    p.set_position(width / 2);
                }
            });
        });
    }

    // -- Switch to the new tab and update UI --------------------------------
    ctx.notebook.set_current_page(Some(page_idx));
    ctx.window.set_title(Some(&document.borrow().title()));
    {
        let text = editor::get_text(&source_view);
        let lines = text.lines().count();
        let bytes = text.len();
        ctx.status_right
            .set_text(&format!("{lines} lines · {bytes} bytes"));
    }
    source_view.grab_focus();
}

fn add_recent_file_and_refresh(ctx: &AppContext, path: std::path::PathBuf) {
    let save_result = {
        let mut state = ctx.state.borrow_mut();
        state.add_recent_file(path);
        ctx.recent_files.save(&state.recent_files)
    };

    if let Err(error) = save_result {
        ctx.state
            .borrow_mut()
            .set_status(format!("Failed to store recent files: {error}"));
    }

    toolbar::rebuild_recent_menu(&ctx.recent_menu, &ctx.state.borrow().recent_files);
}

fn persist_app_config(ctx: &AppContext) {
    let config = {
        let state = ctx.state.borrow();
        AppConfig {
            theme: state.theme.persisted_value().into(),
            auto_save_enabled: state.auto_save_enabled,
        }
    };

    if let Err(error) = xdg::save_app_config(&ctx.dirs, &config) {
        ctx.state
            .borrow_mut()
            .set_status(format!("Failed to store settings: {error}"));
    }
}

fn cleanup_document_draft(ctx: &AppContext, document: &Rc<RefCell<Document>>) {
    let draft_id = document.borrow().draft_id.clone();
    if let Err(error) = ctx.autosave.discard_draft(draft_id.as_deref()) {
        ctx.state
            .borrow_mut()
            .set_status(format!("Failed to remove draft: {error}"));
    }
    document.borrow_mut().clear_draft();
}

fn current_page_widget(ctx: &AppContext) -> Option<gtk4::Box> {
    let page = ctx.notebook.current_page()? as usize;
    let tabs_ref = ctx.tabs.borrow();
    Some(tabs_ref.get(page)?.page_widget.clone())
}

fn save_tab_by_widget(
    ctx: &AppContext,
    page_widget: &gtk4::Box,
    force_save_as: bool,
    post_save: Option<Rc<dyn Fn()>>,
) {
    let Some(page_num) = ctx.notebook.page_num(page_widget) else {
        return;
    };
    let page = page_num as usize;

    let (document, source_view, tab_label) = {
        let tabs_ref = ctx.tabs.borrow();
        let Some(tw) = tabs_ref.get(page) else { return };
        (
            tw.document.clone(),
            tw.source_view.clone(),
            tw.tab_label.clone(),
        )
    };

    if !force_save_as {
        let file_path = document.borrow().file_path.clone();
        if let Some(path) = file_path {
            let text = editor::get_text(&source_view);
            match file_io::write_file(&path, &text) {
                Ok(()) => {
                    {
                        let mut doc = document.borrow_mut();
                        doc.mark_saved();
                    }
                    cleanup_document_draft(ctx, &document);
                    tab_label.set_text(&document.borrow().title());
                    {
                        let mut st = ctx.state.borrow_mut();
                        st.last_auto_save = Instant::now();
                        st.set_status("Saved");
                    }
                    add_recent_file_and_refresh(ctx, path);
                    ctx.window.set_title(Some(&document.borrow().title()));
                    if let Some(callback) = post_save {
                        callback();
                    }
                }
                Err(error) => {
                    ctx.state
                        .borrow_mut()
                        .set_status(format!("Save failed: {error}"));
                }
            }
            return;
        }
    }

    let dialog = gtk4::FileDialog::builder()
        .title("Save Markdown File")
        .build();

    let default_name = document
        .borrow()
        .file_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled.md".into());
    dialog.set_initial_name(Some(&default_name));

    let ctx_c = ctx.clone();
    dialog.save(
        Some(&ctx.window),
        None::<&gio::Cancellable>,
        move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    let text = editor::get_text(&source_view);
                    match file_io::write_file(&path, &text) {
                        Ok(()) => {
                            {
                                let mut doc = document.borrow_mut();
                                doc.file_path = Some(path.clone());
                                doc.mark_saved();
                            }
                            cleanup_document_draft(&ctx_c, &document);
                            tab_label.set_text(&document.borrow().title());
                            {
                                let mut st = ctx_c.state.borrow_mut();
                                st.last_auto_save = Instant::now();
                                st.set_status("Saved");
                            }
                            add_recent_file_and_refresh(&ctx_c, path);
                            ctx_c.window.set_title(Some(&document.borrow().title()));
                            if let Some(callback) = post_save.clone() {
                                callback();
                            }
                        }
                        Err(error) => {
                            ctx_c
                                .state
                                .borrow_mut()
                                .set_status(format!("Save failed: {error}"));
                        }
                    }
                }
            }
        },
    );
}

fn show_unsaved_changes_dialog(
    ctx: &AppContext,
    page_widget: &gtk4::Box,
    on_save: Rc<dyn Fn()>,
    on_discard: Rc<dyn Fn()>,
) {
    let Some(page_num) = ctx.notebook.page_num(page_widget) else {
        return;
    };
    let page = page_num as usize;

    let document = {
        let tabs_ref = ctx.tabs.borrow();
        let Some(tw) = tabs_ref.get(page) else { return };
        tw.document.clone()
    };

    let title = document.borrow().title().replace('●', "");
    let dialog = gtk4::AlertDialog::builder()
        .modal(true)
        .message(format!("Save changes to {title}?"))
        .detail(
            "This document has unsaved changes. You can save it, discard its draft, or cancel closing.",
        )
        .buttons(["Save", "Discard", "Cancel"])
        .default_button(0)
        .cancel_button(2)
        .build();

    let ctx_c = ctx.clone();
    let page_widget_c = page_widget.clone();
    dialog.choose(
        Some(&ctx.window),
        None::<&gio::Cancellable>,
        move |result| match result.ok() {
            Some(0) => {
                save_tab_by_widget(&ctx_c, &page_widget_c, false, Some(on_save.clone()));
            }
            Some(1) => {
                if let Some(page_num) = ctx_c.notebook.page_num(&page_widget_c) {
                    let document = {
                        let tabs_ref = ctx_c.tabs.borrow();
                        tabs_ref
                            .get(page_num as usize)
                            .map(|tw| tw.document.clone())
                    };
                    if let Some(document) = document {
                        cleanup_document_draft(&ctx_c, &document);
                    }
                }
                on_discard();
            }
            _ => {}
        },
    );
}

fn request_close_tab_by_widget(ctx: &AppContext, page_widget: &gtk4::Box) {
    let Some(page_num) = ctx.notebook.page_num(page_widget) else {
        return;
    };
    let page = page_num as usize;
    let modified = {
        let tabs_ref = ctx.tabs.borrow();
        tabs_ref
            .get(page)
            .map(|tw| tw.document.borrow().modified)
            .unwrap_or(false)
    };

    if !modified {
        force_close_tab_by_widget(ctx, page_widget);
        return;
    }

    let ctx_c = ctx.clone();
    let page_widget_save = page_widget.clone();
    let page_widget_discard = page_widget.clone();
    show_unsaved_changes_dialog(
        ctx,
        page_widget,
        Rc::new(move || {
            force_close_tab_by_widget(&ctx_c, &page_widget_save);
        }),
        Rc::new({
            let ctx_c = ctx.clone();
            move || {
                force_close_tab_by_widget(&ctx_c, &page_widget_discard);
            }
        }),
    );
}

fn request_window_close(ctx: &AppContext) {
    if let Some((page_idx, page_widget)) = {
        let tabs_ref = ctx.tabs.borrow();
        tabs_ref
            .iter()
            .enumerate()
            .find(|(_, tw)| tw.document.borrow().modified)
            .map(|(idx, tw)| (idx, tw.page_widget.clone()))
    } {
        ctx.notebook.set_current_page(Some(page_idx as u32));
        let ctx_save = ctx.clone();
        let ctx_discard = ctx.clone();
        let page_widget_save = page_widget.clone();
        let page_widget_discard = page_widget.clone();
        show_unsaved_changes_dialog(
            ctx,
            &page_widget,
            Rc::new(move || {
                force_close_tab_by_widget(&ctx_save, &page_widget_save);
                request_window_close(&ctx_save);
            }),
            Rc::new(move || {
                force_close_tab_by_widget(&ctx_discard, &page_widget_discard);
                request_window_close(&ctx_discard);
            }),
        );
        return;
    }

    ctx.allow_window_close.set(true);
    ctx.window.close();
}

fn connect_close_handler(ctx: &AppContext) {
    let ctx_c = ctx.clone();
    ctx.window.connect_close_request(move |_| {
        if ctx_c.allow_window_close.replace(false) {
            return glib::Propagation::Proceed;
        }

        let has_modified = {
            let tabs_ref = ctx_c.tabs.borrow();
            tabs_ref.iter().any(|tw| tw.document.borrow().modified)
        };
        if has_modified {
            request_window_close(&ctx_c);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
}

/// Close a tab identified by its page widget.
fn force_close_tab_by_widget(ctx: &AppContext, page_widget: &gtk4::Box) {
    let Some(page_num) = ctx.notebook.page_num(page_widget) else {
        return;
    };
    let idx = page_num as usize;

    // Remove from Vec first (returns the removed TabWidgets).
    let tw = ctx.tabs.borrow_mut().remove(idx);

    // Disconnect buffer signal to break reference cycles.
    tw.source_view
        .buffer()
        .disconnect(tw.buffer_changed_handler);

    // Cancel any pending debounce timeout.
    if let Some(id) = tw.debounce_id.borrow_mut().take() {
        unsafe {
            glib::ffi::g_source_remove(id.as_raw());
        }
    }

    // Suppress switch-page while removing to avoid stale index lookups.
    ctx.suppress_switch.set(true);
    ctx.notebook.remove_page(Some(page_num));
    ctx.suppress_switch.set(false);

    // If no tabs left, close the window.
    if ctx.notebook.n_pages() == 0 {
        ctx.window.close();
        return;
    }

    // Manually update UI for the now-current tab.
    if let Some(new_page) = ctx.notebook.current_page() {
        let tabs_ref = ctx.tabs.borrow();
        if let Some(tw) = tabs_ref.get(new_page as usize) {
            ctx.window.set_title(Some(&tw.document.borrow().title()));
            let text = editor::get_text(&tw.source_view);
            let lines = text.lines().count();
            let bytes = text.len();
            ctx.status_right
                .set_text(&format!("{lines} lines · {bytes} bytes"));
            tw.source_view.grab_focus();
        }
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

fn setup_actions(ctx: &AppContext) {
    // -- New File (new tab) ---------------------------------------------
    {
        let action = gio::SimpleAction::new("new-file", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            create_new_tab(&ctx_c, InitialTab::default());
        });
        ctx.window.add_action(&action);
    }

    // -- Open File (in new tab) -----------------------------------------
    {
        let action = gio::SimpleAction::new("open-file", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            let dialog = gtk4::FileDialog::builder()
                .title("Open Markdown File")
                .build();

            let md_filter = gtk4::FileFilter::new();
            md_filter.set_name(Some("Markdown"));
            md_filter.add_pattern("*.md");
            md_filter.add_pattern("*.markdown");
            md_filter.add_pattern("*.txt");

            let all_filter = gtk4::FileFilter::new();
            all_filter.set_name(Some("All Files"));
            all_filter.add_pattern("*");

            let filters = gio::ListStore::new::<gtk4::FileFilter>();
            filters.append(&md_filter);
            filters.append(&all_filter);
            dialog.set_filters(Some(&filters));

            let ctx_cc = ctx_c.clone();
            dialog.open(
                Some(&ctx_c.window),
                None::<&gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            match file_io::read_file(&path) {
                                Ok(content) => {
                                    create_new_tab(
                                        &ctx_cc,
                                        InitialTab {
                                            content: Some(content),
                                            file_path: Some(path.clone()),
                                            draft_id: None,
                                            modified: false,
                                        },
                                    );
                                    add_recent_file_and_refresh(&ctx_cc, path);
                                }
                                Err(e) => {
                                    ctx_cc
                                        .state
                                        .borrow_mut()
                                        .set_status(format!("Open failed: {e}"));
                                }
                            }
                        }
                    }
                },
            );
        });
        ctx.window.add_action(&action);
    }

    // -- Save File ------------------------------------------------------
    {
        let action = gio::SimpleAction::new("save-file", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            if let Some(page_widget) = current_page_widget(&ctx_c) {
                save_tab_by_widget(&ctx_c, &page_widget, false, None);
            }
        });
        ctx.window.add_action(&action);
    }

    // -- Save File As ---------------------------------------------------
    {
        let action = gio::SimpleAction::new("save-file-as", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            if let Some(page_widget) = current_page_widget(&ctx_c) {
                save_tab_by_widget(&ctx_c, &page_widget, true, None);
            }
        });
        ctx.window.add_action(&action);
    }

    // -- Close Tab ------------------------------------------------------
    {
        let action = gio::SimpleAction::new("close-tab", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            if let Some(page_widget) = current_page_widget(&ctx_c) {
                request_close_tab_by_widget(&ctx_c, &page_widget);
            }
        });
        ctx.window.add_action(&action);
    }

    // -- Next Tab -------------------------------------------------------
    {
        let action = gio::SimpleAction::new("next-tab", None);
        let notebook_c = ctx.notebook.clone();
        action.connect_activate(move |_, _| {
            let n = notebook_c.n_pages();
            if n > 1 {
                if let Some(current) = notebook_c.current_page() {
                    let next = (current + 1) % n;
                    notebook_c.set_current_page(Some(next));
                }
            }
        });
        ctx.window.add_action(&action);
    }

    // -- Previous Tab ---------------------------------------------------
    {
        let action = gio::SimpleAction::new("prev-tab", None);
        let notebook_c = ctx.notebook.clone();
        action.connect_activate(move |_, _| {
            let n = notebook_c.n_pages();
            if n > 1 {
                if let Some(current) = notebook_c.current_page() {
                    let prev = if current == 0 { n - 1 } else { current - 1 };
                    notebook_c.set_current_page(Some(prev));
                }
            }
        });
        ctx.window.add_action(&action);
    }

    // -- Export HTML -----------------------------------------------------
    {
        let action = gio::SimpleAction::new("export-html", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            let page = match ctx_c.notebook.current_page() {
                Some(p) => p as usize,
                None => return,
            };
            let tabs_ref = ctx_c.tabs.borrow();
            let Some(tw) = tabs_ref.get(page) else { return };

            let text = editor::get_text(&tw.source_view);
            let title = tw
                .document
                .borrow()
                .title()
                .replace('●', "")
                .trim()
                .to_string();
            let body = markdown::markdown_to_html(&text);
            let html = markdown::wrap_html_document(&body, &title);

            let default_name = tw
                .document
                .borrow()
                .file_path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map(|n| format!("{}.html", n.to_string_lossy()))
                .unwrap_or_else(|| "export.html".into());
            drop(tabs_ref);

            let dialog = gtk4::FileDialog::builder().title("Export as HTML").build();
            dialog.set_initial_name(Some(&default_name));

            let ctx_cc = ctx_c.clone();
            dialog.save(
                Some(&ctx_c.window),
                None::<&gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            match file_io::write_file(&path, &html) {
                                Ok(()) => {
                                    ctx_cc.state.borrow_mut().set_status("Exported HTML");
                                }
                                Err(e) => {
                                    ctx_cc
                                        .state
                                        .borrow_mut()
                                        .set_status(format!("Export failed: {e}"));
                                }
                            }
                        }
                    }
                },
            );
        });
        ctx.window.add_action(&action);
    }

    // -- Print (via window.print) ---------------------------------------
    //
    // A hidden WebView is created for each invocation, the rendered HTML is
    // loaded into it, and once the page signals `LoadEvent::Finished` we
    // inject `window.print()`.  WebKit translates that JS call into the
    // `WebView::print` signal, passing a `webkit6::PrintOperation` that we
    // hand to GTK's native print-to-PDF dialog.
    {
        let action = gio::SimpleAction::new("print", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            let page = match ctx_c.notebook.current_page() {
                Some(p) => p as usize,
                None => return,
            };
            let tabs_ref = ctx_c.tabs.borrow();
            let Some(tw) = tabs_ref.get(page) else { return };

            let text = editor::get_text(&tw.source_view);
            let title = tw
                .document
                .borrow()
                .title()
                .replace('\u{25cf}', "")
                .trim()
                .to_string();
            let body = markdown::markdown_to_html(&text);
            let html = markdown::wrap_html_document(&body, &title);
            drop(tabs_ref);

            // Off-screen WebView used solely for the print pipeline.
            let print_view = webkit6::WebView::new();

            // `WebView::print` fires when JavaScript calls `window.print()`.
            // The supplied `PrintOperation` owns WebKit's print settings;
            // `run_dialog` presents the native GTK print-to-PDF dialog.
            let win = ctx_c.window.clone();
            print_view.connect_print(move |_view, print_op: &webkit6::PrintOperation| {
                print_op.run_dialog(Some(&win));
                true // mark as handled — suppresses WebKit's own fallback
            });

            // Trigger `window.print()` once the HTML document is fully loaded.
            let print_view_c = print_view.clone();
            print_view.connect_load_changed(move |_view, event| {
                if event == webkit6::LoadEvent::Finished {
                    print_view_c.evaluate_javascript(
                        "window.print();",
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            });

            print_view.load_html(&html, None);
            ctx_c
                .state
                .borrow_mut()
                .set_status("Opening print dialog\u{2026}");
        });
        ctx.window.add_action(&action);
    }

    // -- Quit -----------------------------------------------------------
    {
        let action = gio::SimpleAction::new("quit", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            request_window_close(&ctx_c);
        });
        ctx.window.add_action(&action);
    }

    // -- Undo / Redo ----------------------------------------------------
    {
        let action = gio::SimpleAction::new("undo", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            let page = match ctx_c.notebook.current_page() {
                Some(p) => p as usize,
                None => return,
            };
            let tabs_ref = ctx_c.tabs.borrow();
            if let Some(tw) = tabs_ref.get(page) {
                let buf = tw
                    .source_view
                    .buffer()
                    .downcast::<sourceview5::Buffer>()
                    .unwrap();
                if buf.can_undo() {
                    buf.undo();
                }
            }
        });
        ctx.window.add_action(&action);
    }
    {
        let action = gio::SimpleAction::new("redo", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            let page = match ctx_c.notebook.current_page() {
                Some(p) => p as usize,
                None => return,
            };
            let tabs_ref = ctx_c.tabs.borrow();
            if let Some(tw) = tabs_ref.get(page) {
                let buf = tw
                    .source_view
                    .buffer()
                    .downcast::<sourceview5::Buffer>()
                    .unwrap();
                if buf.can_redo() {
                    buf.redo();
                }
            }
        });
        ctx.window.add_action(&action);
    }

    // -- Find / Replace -------------------------------------------------
    {
        let action = gio::SimpleAction::new("find", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            let page = match ctx_c.notebook.current_page() {
                Some(p) => p as usize,
                None => return,
            };
            let tabs_ref = ctx_c.tabs.borrow();
            if let Some(tw) = tabs_ref.get(page) {
                tw.find_bar.open_find();
            }
        });
        ctx.window.add_action(&action);
    }
    {
        let action = gio::SimpleAction::new("replace", None);
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, _| {
            let page = match ctx_c.notebook.current_page() {
                Some(p) => p as usize,
                None => return,
            };
            let tabs_ref = ctx_c.tabs.borrow();
            if let Some(tw) = tabs_ref.get(page) {
                tw.find_bar.open_replace();
            }
        });
        ctx.window.add_action(&action);
    }

    // -- View mode — stateful string action (radio behaviour) ------------
    // State is one of "editor", "split", "preview".  Menu items target the
    // matching string so GIO renders the active entry as a radio bullet.
    {
        let initial_mode = match ctx.state.borrow().view_mode {
            ViewMode::Editor => "editor",
            ViewMode::Split => "split",
            ViewMode::Preview => "preview",
        };
        let action = gio::SimpleAction::new_stateful(
            "view-mode",
            Some(&String::static_variant_type()),
            &initial_mode.to_variant(),
        );
        let ctx_c = ctx.clone();
        action.connect_activate(move |act, param| {
            let Some(variant) = param else { return };
            let Some(mode_str) = variant.get::<String>() else {
                return;
            };
            act.set_state(variant);
            match mode_str.as_str() {
                "editor" => {
                    ctx_c.state.borrow_mut().view_mode = ViewMode::Editor;
                    for tw in ctx_c.tabs.borrow().iter() {
                        tw.editor_scroll.set_visible(true);
                        tw.webview.set_visible(false);
                    }
                }
                "split" => {
                    ctx_c.state.borrow_mut().view_mode = ViewMode::Split;
                    for tw in ctx_c.tabs.borrow().iter() {
                        tw.editor_scroll.set_visible(true);
                        tw.webview.set_visible(true);
                        let width = tw.paned.width();
                        if width > 0 {
                            tw.paned.set_position(width / 2);
                        }
                    }
                }
                "preview" => {
                    ctx_c.state.borrow_mut().view_mode = ViewMode::Preview;
                    for tw in ctx_c.tabs.borrow().iter() {
                        tw.editor_scroll.set_visible(false);
                        tw.webview.set_visible(true);
                    }
                }
                _ => {}
            }
        });
        ctx.window.add_action(&action);
    }

    // -- Dark mode — stateful bool action (checkbox behaviour) ---------
    // Initial state mirrors the startup theme (dark = true).
    {
        let initial_dark = matches!(ctx.state.borrow().theme, Theme::Dark);
        let action = gio::SimpleAction::new_stateful("dark-mode", None, &initial_dark.to_variant());
        let ctx_c = ctx.clone();
        action.connect_activate(move |act, _| {
            let current = act.state().and_then(|v| v.get::<bool>()).unwrap_or(false);
            let new_dark = !current;
            act.set_state(&new_dark.to_variant());
            {
                let mut st = ctx_c.state.borrow_mut();
                st.theme = if new_dark { Theme::Dark } else { Theme::Light };
            }
            persist_app_config(&ctx_c);
            apply_global_theme(new_dark);
            for tw in ctx_c.tabs.borrow().iter() {
                editor::apply_theme(&tw.source_view, new_dark);
                preview::set_theme(&tw.webview, new_dark);
                tw.find_bar.set_dark(new_dark);
            }
        });
        ctx.window.add_action(&action);
    }

    // -- Auto-save — stateful bool action (checkbox behaviour) ---------
    {
        let action = gio::SimpleAction::new_stateful(
            "auto-save",
            None,
            &ctx.state.borrow().auto_save_enabled.to_variant(),
        );
        let ctx_c = ctx.clone();
        action.connect_activate(move |act, _| {
            let current = act.state().and_then(|v| v.get::<bool>()).unwrap_or(false);
            let new_val = !current;
            act.set_state(&new_val.to_variant());
            let mut st = ctx_c.state.borrow_mut();
            st.auto_save_enabled = new_val;
            if new_val {
                ctx_c.status_autosave.set_text("Auto-save: On");
                st.set_status("Auto-save enabled");
            } else {
                ctx_c.status_autosave.set_text("Auto-save: Off");
                st.set_status("Auto-save disabled");
            }
            drop(st);
            persist_app_config(&ctx_c);
        });
        ctx.window.add_action(&action);
    }

    // -- About --------------------------------------------------------
    {
        let action = gio::SimpleAction::new("about", None);
        let win = ctx.window.clone();
        action.connect_activate(move |_, _| {
            show_about_window(&win);
        });
        ctx.window.add_action(&action);
    }

    // -- Open Recent File -----------------------------------------------
    {
        let action = gio::SimpleAction::new("open-recent", Some(&String::static_variant_type()));
        let ctx_c = ctx.clone();
        action.connect_activate(move |_, param| {
            let Some(variant) = param else { return };
            let Some(path_str) = variant.get::<String>() else {
                return;
            };
            let path = std::path::PathBuf::from(&path_str);

            // If this file is already open in a tab, switch to that tab.
            {
                let tabs_ref = ctx_c.tabs.borrow();
                for (i, tw) in tabs_ref.iter().enumerate() {
                    let doc = tw.document.borrow();
                    if doc.file_path.as_ref() == Some(&path) {
                        ctx_c.notebook.set_current_page(Some(i as u32));
                        return;
                    }
                }
            }

            // Otherwise, open in a new tab.
            match file_io::read_file(&path) {
                Ok(content) => {
                    create_new_tab(
                        &ctx_c,
                        InitialTab {
                            content: Some(content),
                            file_path: Some(path.clone()),
                            draft_id: None,
                            modified: false,
                        },
                    );
                    add_recent_file_and_refresh(&ctx_c, path);
                }
                Err(e) => {
                    ctx_c
                        .state
                        .borrow_mut()
                        .set_status(format!("Open failed: {e}"));
                }
            }
        });
        ctx.window.add_action(&action);
    }
}

// ---------------------------------------------------------------------------
// Keyboard accelerators
// ---------------------------------------------------------------------------

fn setup_accels(app: &gtk4::Application) {
    app.set_accels_for_action("win.new-file", &["<Ctrl>n"]);
    app.set_accels_for_action("win.open-file", &["<Ctrl>o"]);
    app.set_accels_for_action("win.save-file", &["<Ctrl>s"]);
    app.set_accels_for_action("win.save-file-as", &["<Ctrl><Shift>s"]);
    app.set_accels_for_action("win.close-tab", &["<Ctrl>w"]);
    app.set_accels_for_action("win.next-tab", &["<Ctrl>Page_Down"]);
    app.set_accels_for_action("win.prev-tab", &["<Ctrl>Page_Up"]);
    app.set_accels_for_action("win.undo", &["<Ctrl>z"]);
    app.set_accels_for_action("win.redo", &["<Ctrl>y", "<Ctrl><Shift>z"]);
    app.set_accels_for_action("win.find", &["<Ctrl>f"]);
    app.set_accels_for_action("win.replace", &["<Ctrl>h"]);
    app.set_accels_for_action("win.view-mode::editor", &["<Ctrl>1"]);
    app.set_accels_for_action("win.view-mode::split", &["<Ctrl>2"]);
    app.set_accels_for_action("win.view-mode::preview", &["<Ctrl>3"]);
    app.set_accels_for_action("win.dark-mode", &["<Ctrl><Shift>d"]);
    app.set_accels_for_action("win.auto-save", &["<Ctrl><Shift>a"]);
    app.set_accels_for_action("win.quit", &["<Ctrl>q"]);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Apply global GTK dark/light theme preference.
fn apply_global_theme(dark: bool) {
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(dark);
    }
}

fn ensure_notebook_css() {
    NOTEBOOK_CSS_INIT.call_once(|| {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(NOTEBOOK_CSS);
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 100,
            );
        }
    });
}

/// Show a minimal About dialog centered on the main window.
fn show_about_window(parent: &gtk4::ApplicationWindow) {
    const APP_NAME: &str = env!("CARGO_PKG_NAME");
    const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
    const APP_WEBSITE: &str = "https://github.com/ryanyonzon/jetmd";
    const ICON_BYTES: &[u8] = include_bytes!("../res/images/512x512.png");

    let about_win = gtk4::Window::builder()
        .title("About")
        .transient_for(parent)
        .modal(true)
        .resizable(false)
        .decorated(false)
        .default_width(300)
        .build();

    // Floating close button — top-right corner, overlaid on content.
    let close_btn = gtk4::Button::from_icon_name("window-close-symbolic");
    close_btn.add_css_class("circular");
    close_btn.add_css_class("flat");
    close_btn.set_halign(gtk4::Align::End);
    close_btn.set_valign(gtk4::Align::Start);
    close_btn.set_margin_top(10);
    close_btn.set_margin_end(10);
    {
        let win = about_win.clone();
        close_btn.connect_clicked(move |_| win.close());
    }

    // Logo — load from embedded icon bytes using Picture for proper scaling.
    let logo: gtk4::Widget =
        match gtk4::gdk::Texture::from_bytes(&glib::Bytes::from_static(ICON_BYTES)) {
            Ok(tex) => {
                let pic = gtk4::Picture::for_paintable(&tex);
                pic.set_size_request(160, 160);
                pic.set_can_shrink(true);
                pic.set_halign(gtk4::Align::Center);
                pic.set_hexpand(false);
                pic.set_margin_top(40);
                pic.set_margin_start(48);
                pic.set_margin_end(48);
                pic.upcast()
            }
            Err(_) => {
                let img = gtk4::Image::from_icon_name("image-missing");
                img.set_pixel_size(128);
                img.set_margin_top(48);
                img.upcast()
            }
        };

    // Application name — bold.
    let name_label = gtk4::Label::new(None);
    name_label.set_markup(&format!("<b>{APP_NAME}</b>"));
    name_label.set_halign(gtk4::Align::Center);
    name_label.set_margin_top(16);

    // Application version.
    let version_label = gtk4::Label::new(Some(&format!("Version {APP_VERSION}")));
    version_label.add_css_class("dim-label");
    version_label.set_halign(gtk4::Align::Center);
    version_label.set_margin_top(6);
    version_label.set_margin_bottom(24);

    // Website row.
    let website_label = gtk4::Label::new(Some("Website"));
    website_label.set_halign(gtk4::Align::Start);
    website_label.set_hexpand(true);

    let website_icon = gtk4::Image::from_icon_name("external-link-symbolic");

    let website_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    website_row.set_margin_start(14);
    website_row.set_margin_end(14);
    website_row.set_margin_top(12);
    website_row.set_margin_bottom(12);
    website_row.append(&website_label);
    website_row.append(&website_icon);

    let website_btn = gtk4::Button::new();
    website_btn.set_child(Some(&website_row));
    website_btn.set_halign(gtk4::Align::Fill);
    website_btn.set_hexpand(true);
    website_btn.set_margin_start(24);
    website_btn.set_margin_end(24);
    website_btn.set_margin_bottom(24);
    {
        let parent = parent.clone();
        website_btn.connect_clicked(move |_| {
            if let Err(err) =
                gio::AppInfo::launch_default_for_uri(APP_WEBSITE, None::<&gio::AppLaunchContext>)
            {
                eprintln!("Failed to open website: {err}");
                parent.present();
            }
        });
    }

    // Vertical layout, center-aligned.
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_halign(gtk4::Align::Center);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.append(&logo);
    content.append(&name_label);
    content.append(&version_label);
    content.append(&website_btn);

    // Overlay: content underneath, floating close button on top.
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&content));
    overlay.add_overlay(&close_btn);

    about_win.set_child(Some(&overlay));
    about_win.present();
}
