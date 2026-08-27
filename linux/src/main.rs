// SPDX-License-Identifier: GPL-3.0-or-later

mod application;
mod challenges_dialog;
mod config;
mod detail_pane;
mod format;
mod glow;
mod persist;
mod presets;
mod presets_dialog;
mod query_pane;
mod relations;
mod requirement_editor;
mod result_navigation;
mod results_pane;
mod sprites;
mod state;
mod update;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::config::{APP_ID, APP_NAME, RESOURCE_BASE_PATH};

fn main() -> glib::ExitCode {
    gio::resources_register_include!("dev.seedseeker.SeedSeeker.gresource")
        .expect("Seed Seeker resources must be valid");
    glib::set_application_name(APP_NAME);

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .resource_base_path(RESOURCE_BASE_PATH)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_startup(|_| load_stylesheet());
    application::configure(&app);
    app.connect_activate(window::present);
    // A seedseeker:// share link arrives here as a `gio::File`, at cold start
    // or routed from a second invocation by GApplication's single-instance
    // handling. The window action does the decoding so it can reach the
    // window-local query state.
    app.connect_open(|app, files, _hint| {
        window::present(app);
        let Some(window) = app.active_window() else {
            return;
        };
        for file in files {
            let _ = WidgetExt::activate_action(
                &window,
                "win.open-share-link",
                Some(&file.uri().to_variant()),
            );
        }
    });
    app.run()
}

fn load_stylesheet() {
    let provider = gtk::CssProvider::new();
    provider.load_from_resource(&format!("{RESOURCE_BASE_PATH}/style/style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
