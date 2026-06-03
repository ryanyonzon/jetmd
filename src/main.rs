// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option.
//
// You may not use this file except in compliance with one of these licenses.

//! **jetmd** — a lightweight Markdown editor with live preview.
//!
//! Built with GTK 4, GtkSourceView 5, and WebKitGTK 6.

mod app;
mod autosave;
mod file_io;
mod highlight;
mod markdown;
mod recent_files;
mod state;
mod theme;
mod ui;
mod xdg;

use gtk4::glib;
use gtk4::prelude::*;

fn main() -> glib::ExitCode {
    const APP_ID: &str = "io.github.ryanyonzon.jetmd";

    glib::set_application_name("jetmd");

    let application = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(gtk4::gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    application.connect_activate(|app| {
        app::build_window(app, Vec::new());
    });

    application.connect_open(|app, files, _hint| {
        let initial_files = files
            .iter()
            .filter_map(|file| file.path())
            .collect::<Vec<_>>();
        app::build_window(app, initial_files);
    });

    application.run()
}
