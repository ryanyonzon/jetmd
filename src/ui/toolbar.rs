// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! Toolbar — titlebar controls placed above the Notebook tab area.
//!
//! Layout: `[Open icon] [Recent icon] [+]                     [☰]`

use gtk4::gio;
use gtk4::prelude::*;

/// Individual widgets for the titlebar controls.
pub struct TopBarWidgets {
    /// Hamburger menu button (☰) — placed at the far right.
    pub hamburger_btn: gtk4::MenuButton,
    /// "+" new-tab button — placed after tabs.
    pub new_tab_btn: gtk4::Button,
    /// Open-file button — placed at the right, between new-tab and hamburger.
    pub open_btn: gtk4::Button,
    /// Recent-files dropdown button (▼) — placed before tabs.
    pub recent_btn: gtk4::MenuButton,
    /// The underlying `gio::Menu` for recent files (rebuilt dynamically).
    pub recent_menu: gio::Menu,
}

/// Build the flat menu model used inside the hamburger popover.
///
/// Single-level menu with separators; View is the only sub-menu.
pub fn build_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();

    // -- New / Open -----------------------------------------------------
    let new_open_section = gio::Menu::new();
    new_open_section.append(Some("New"), Some("win.new-file"));
    new_open_section.append(Some("Open"), Some("win.open-file"));
    menu.append_section(None, &new_open_section);

    // -- Save / Save As -------------------------------------------------
    let save_section = gio::Menu::new();
    save_section.append(Some("Save"), Some("win.save-file"));
    save_section.append(Some("Save As"), Some("win.save-file-as"));
    menu.append_section(None, &save_section);

    // -- Find -----------------------------------------------------------
    let find_section = gio::Menu::new();
    find_section.append(Some("Find..."), Some("win.find"));
    menu.append_section(None, &find_section);

    // -- Close Tab ------------------------------------------------------
    let tab_section = gio::Menu::new();
    tab_section.append(Some("Close Tab"), Some("win.close-tab"));
    menu.append_section(None, &tab_section);

    // -- Export As (sub-menu) -------------------------------------------
    let export_section = gio::Menu::new();
    let export_submenu = gio::Menu::new();
    export_submenu.append(Some("HTML"), Some("win.export-html"));
    export_section.append_submenu(Some("Export As"), &export_submenu);
    menu.append_section(None, &export_section);

    // -- Auto-save [checkbox] / Dark Mode [checkbox] --------------------
    let prefs_section = gio::Menu::new();
    prefs_section.append(Some("Auto-save"), Some("win.auto-save"));
    prefs_section.append(Some("Dark Mode"), Some("win.dark-mode"));
    menu.append_section(None, &prefs_section);

    // -- View (sub-menu, radio group) -----------------------------------
    let view_section = gio::Menu::new();
    let view_submenu = gio::Menu::new();
    // Radio items: action state == target value → shown as active.
    view_submenu.append(Some("Editor"), Some("win.view-mode::editor"));
    view_submenu.append(Some("Split"), Some("win.view-mode::split"));
    view_submenu.append(Some("Preview"), Some("win.view-mode::preview"));
    view_section.append_submenu(Some("View"), &view_submenu);
    menu.append_section(None, &view_section);

    // -- About ----------------------------------------------------------
    let about_section = gio::Menu::new();
    about_section.append(Some("About"), Some("win.about"));
    menu.append_section(None, &about_section);

    // -- Exit -----------------------------------------------------------
    let exit_section = gio::Menu::new();
    exit_section.append(Some("Exit"), Some("win.quit"));
    menu.append_section(None, &exit_section);

    menu
}

/// Create the individual titlebar widgets.
pub fn create_top_bar_widgets() -> TopBarWidgets {
    // -- Hamburger menu button (☰) --------------------------------------
    let menu_model = build_menu_model();
    let popover = gtk4::PopoverMenu::from_model(Some(&menu_model));

    let hamburger_icon = gtk4::Image::from_icon_name("open-menu-symbolic");
    hamburger_icon.set_pixel_size(14);
    let hamburger_btn = gtk4::MenuButton::new();
    hamburger_btn.set_child(Some(&hamburger_icon));
    hamburger_btn.set_popover(Some(&popover));
    hamburger_btn.set_tooltip_text(Some("Menu"));
    hamburger_btn.add_css_class("flat");

    // -- New-tab "+" button ---------------------------------------------
    let new_tab_icon = gtk4::Image::from_icon_name("list-add-symbolic");
    new_tab_icon.set_pixel_size(14);
    let new_tab_btn = gtk4::Button::new();
    new_tab_btn.set_child(Some(&new_tab_icon));
    new_tab_btn.set_tooltip_text(Some("New Tab  (Ctrl+N)"));
    new_tab_btn.add_css_class("flat");

    // -- Open-file button -----------------------------------------------
    let open_icon = gtk4::Image::from_icon_name("document-open-symbolic");
    open_icon.set_pixel_size(14);
    let open_btn = gtk4::Button::new();
    open_btn.set_child(Some(&open_icon));
    open_btn.set_tooltip_text(Some("Open File  (Ctrl+O)"));
    open_btn.add_css_class("flat");

    // -- Recent-files dropdown button (▼) -------------------------------
    let recent_menu = gio::Menu::new();
    // Placeholder for empty state.
    recent_menu.append(Some("(No recent files)"), None);

    let recent_popover = gtk4::PopoverMenu::from_model(Some(&recent_menu));

    let recent_icon = gtk4::Image::from_icon_name("pan-down-symbolic");
    recent_icon.set_pixel_size(14);
    let recent_btn = gtk4::MenuButton::new();
    recent_btn.set_child(Some(&recent_icon));
    recent_btn.set_popover(Some(&recent_popover));
    recent_btn.set_tooltip_text(Some("Recent Files"));
    recent_btn.add_css_class("flat");

    TopBarWidgets {
        hamburger_btn,
        new_tab_btn,
        open_btn,
        recent_btn,
        recent_menu,
    }
}

/// Rebuild the recent-files menu model from the given list of paths.
pub fn rebuild_recent_menu(menu: &gio::Menu, recent_files: &[std::path::PathBuf]) {
    menu.remove_all();
    if recent_files.is_empty() {
        menu.append(Some("(No recent files)"), None);
    } else {
        for path in recent_files {
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            let item = gio::MenuItem::new(Some(&label), None);
            item.set_action_and_target_value(
                Some("win.open-recent"),
                Some(&path.display().to_string().to_variant()),
            );
            item.set_attribute_value("tooltip", Some(&path.display().to_string().to_variant()));
            menu.append_item(&item);
        }
    }
}
