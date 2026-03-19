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
mod markdown;
mod recent_files;
mod state;
mod theme;
mod ui;
mod xdg;

use gtk4::glib;
use gtk4::prelude::*;

fn main() -> glib::ExitCode {
    let application = gtk4::Application::builder()
        .application_id("io.github.ryanyonzon.jetmd")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    application.connect_activate(|app| {
        let initial_file = std::env::args().nth(1);
        app::build_window(app, initial_file);
    });

    application.run()
}
