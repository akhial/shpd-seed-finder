# SPDX-License-Identifier: GPL-3.0-or-later
# dmgbuild settings for the drag-to-install disk image; consumed by
# scripts/package-macos-dmg.sh. Icon positions line up with the arrow in
# macos/dmg/background.tiff (rendered from assets/icon/dmg-background.svg).

import os.path

app = defines.get("app", "dist/Seed Seeker.app")  # noqa: F821
appname = os.path.basename(app)

files = [app]
symlinks = {"Applications": "/Applications"}

format = "UDZO"
size = None

# The volume shows the app icon instead of the generic disk-drive icon.
icon = defines.get("icon")  # noqa: F821

background = defines.get("background")  # noqa: F821
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False

window_rect = ((200, 120), (660, 420))
default_view = "icon-view"
show_icon_preview = False

arrange_by = None
grid_offset = (0, 0)
grid_spacing = 100
scroll_position = (0, 0)
label_pos = "bottom"
text_size = 13
icon_size = 128
icon_locations = {
    appname: (166, 195),
    "Applications": (494, 195),
}
