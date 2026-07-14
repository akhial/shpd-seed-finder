// SPDX-License-Identifier: GPL-3.0-or-later

mod application;
mod challenges_dialog;
mod config;
mod detail_pane;
mod format;
mod persist;
mod query_pane;
mod requirement_editor;
mod results_pane;
mod scout_match;
mod state;
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
        .build();
    app.connect_startup(|_| load_stylesheet());
    application::configure(&app);
    app.connect_activate(window::present);
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
