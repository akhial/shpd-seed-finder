// SPDX-License-Identifier: GPL-3.0-or-later

//! Application-level actions, accelerators, and the About dialog.

use adw::prelude::*;
use gtk::gio;
use gtk::glib::markup_escape_text;
use shpd_seedfinder_core::SHPD_VERSION;

use crate::config::{APP_ID, APP_NAME, RESOURCE_BASE_PATH};

/// Bundled attribution for the Shattered Pixel Dungeon item artwork the app
/// ships and draws. A sibling copy is shared with the Android client, so the
/// two never drift.
const ATTRIBUTION_RESOURCE: &str = "third_party/shattered-pixel-dungeon/ATTRIBUTION.md";

/// The upstream project's full GNU GPL text, bundled because the app now
/// conveys their artwork.
const LICENSE_RESOURCE: &str = "third_party/shattered-pixel-dungeon/LICENSE.txt";

pub fn configure(app: &adw::Application) {
    let about = gio::SimpleAction::new("about", None);
    let weak_app = app.downgrade();
    about.connect_activate(move |_, _| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let dialog = adw::AboutDialog::new();
        dialog.set_application_icon(APP_ID);
        dialog.set_application_name(APP_NAME);
        dialog.set_comments(&format!(
            "Find and inspect Shattered Pixel Dungeon v{SHPD_VERSION} seeds offline."
        ));
        dialog.set_copyright("© 2026 Seed Seeker contributors");
        dialog.set_developer_name("Seed Seeker contributors");
        dialog.set_license_type(gtk::License::Gpl30);
        dialog.set_version(env!("CARGO_PKG_VERSION"));
        dialog.set_website("https://github.com/akhial/shpd-seed-seeker");

        dialog.add_legal_section(
            "Item Artwork",
            Some("© 2012–2015 Oleg Dolya\n© 2014–2026 Evan Debenham"),
            gtk::License::Custom,
            Some(&attribution_markup()),
        );
        dialog.add_legal_section(
            "GNU General Public License, version 3 or later",
            None,
            gtk::License::Custom,
            Some(&license_markup()),
        );
        dialog.present(app.active_window().as_ref());
    });
    app.add_action(&about);

    let quit = gio::SimpleAction::new("quit", None);
    let weak_app = app.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(app) = weak_app.upgrade() {
            app.quit();
        }
    });
    app.add_action(&quit);

    app.set_accels_for_action("app.quit", &["<primary>q"]);
    app.set_accels_for_action("win.start-search", &["<primary>Return"]);
    app.set_accels_for_action("win.add-requirement", &["<primary>n"]);
    app.set_accels_for_action("win.challenges", &["<primary>comma"]);
    app.set_accels_for_action("win.focus-seed", &["<primary>l"]);
    app.set_accels_for_action("win.shortcuts", &["<primary>question"]);
}

/// Loads a bundled UTF-8 resource, or `None` when it is missing or malformed.
fn bundled_text(path: &str) -> Option<String> {
    let bytes = gio::resources_lookup_data(
        &format!("{RESOURCE_BASE_PATH}/{path}"),
        gio::ResourceLookupFlags::NONE,
    )
    .ok()?;
    String::from_utf8(bytes.to_vec()).ok()
}

fn attribution_markup() -> String {
    bundled_text(ATTRIBUTION_RESOURCE).map_or_else(
        || {
            markup_escape_text(
                "The item sprites and ring type glyphs are unchanged copies of Shattered \
                 Pixel Dungeon's item atlases, used under the GNU General Public License \
                 v3.0 or later.",
            )
            .into()
        },
        |text| plain_markup(&render_attribution(&text)),
    )
}

fn license_markup() -> String {
    bundled_text(LICENSE_RESOURCE).map_or_else(
        || {
            markup_escape_text(
                "The full licence text could not be loaded. It is available at \
                 https://www.gnu.org/licenses/gpl-3.0.html.",
            )
            .into()
        },
        |text| plain_markup(&text),
    )
}

/// Escapes text for the Pango markup that `AdwAboutDialog` renders.
fn plain_markup(text: &str) -> String {
    markup_escape_text(text.trim_end()).into()
}

/// Renders the bundled Markdown attribution as the plain prose the About
/// dialog shows: headings and code fences lose their punctuation, bullets keep
/// a real bullet character, and blank-line paragraph breaks are preserved.
fn render_attribution(markdown: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for raw in markdown.lines() {
        let line = raw.trim();
        let line = line.strip_prefix("## ").unwrap_or(line);
        let line = line.strip_prefix("# ").unwrap_or(line);
        let rendered = match line.strip_prefix("- ") {
            Some(bullet) => format!("• {}", bullet.replace('`', "")),
            None => line.replace('`', ""),
        };
        // Collapse runs of blank lines into a single paragraph break.
        if rendered.is_empty() && lines.last().is_some_and(String::is_empty) {
            continue;
        }
        lines.push(rendered);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{attribution_markup, license_markup, plain_markup, render_attribution};

    /// Registers the bundled resources so the licence files can be read here.
    fn register_resources() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            gtk::gio::resources_register_include!("dev.seedseeker.SeedSeeker.gresource")
                .expect("Seed Seeker resources must be valid");
        });
    }

    #[test]
    fn the_bundled_attribution_and_licence_reach_the_about_dialog() {
        register_resources();

        let attribution = attribution_markup();
        assert!(attribution.contains("Shattered Pixel Dungeon item artwork"));
        assert!(attribution.contains("items.png"));
        assert!(attribution.contains("Oleg Dolya"));
        assert!(attribution.contains("Evan Debenham"));
        // The v4.0.0 artwork is pinned by the release JAR's digest; the
        // v3.3.8 commit remains for the byte-identical item_icons.png.
        assert!(
            attribution
                .contains("f62f8ac2ef6d36c72223c1a4e78f18e98d0bb1282cd4f1fca123082d43edccc9")
        );
        assert!(attribution.contains("7b8b845a76fe76c6b7c031ae9e570852411f56db"));
        // Markdown punctuation is rendered away rather than shown verbatim.
        assert!(!attribution.contains('`'));
        assert!(!attribution.contains("# "));

        let license = license_markup();
        assert!(license.contains("GNU GENERAL PUBLIC LICENSE"));
        assert!(license.contains("Version 3, 29 June 2007"));
        // The licence header's angle brackets must survive as Pango entities.
        assert!(license.contains("&lt;http://fsf.org/&gt;"));
        assert!(!license.contains("<http://fsf.org/>"));
        assert!(license.len() > 30_000, "the full licence text must ship");
    }

    #[test]
    fn attribution_markdown_renders_as_plain_prose() {
        let rendered = render_attribution(
            "# Shattered Pixel Dungeon item artwork\n\
             \n\
             `items.png` is an unchanged copy of:\n\
             \n\
             \n\
             `core/src/main/assets/sprites/items.png`\n\
             \n\
             - Pixel Dungeon: Copyright © 2012–2015 Oleg Dolya\n\
             - License: GNU GPL v3.0 or later (see `LICENSE.txt`)\n\
             \n",
        );
        assert_eq!(
            rendered,
            "Shattered Pixel Dungeon item artwork\n\
             \n\
             items.png is an unchanged copy of:\n\
             \n\
             core/src/main/assets/sprites/items.png\n\
             \n\
             • Pixel Dungeon: Copyright © 2012–2015 Oleg Dolya\n\
             • License: GNU GPL v3.0 or later (see LICENSE.txt)"
        );
    }

    #[test]
    fn license_text_is_escaped_for_pango_markup() {
        // The GPL's own text contains angle brackets and ampersands.
        assert_eq!(
            plain_markup("Copyright (C) <year> <name>  Foo & Bar\n"),
            "Copyright (C) &lt;year&gt; &lt;name&gt;  Foo &amp; Bar"
        );
    }
}
