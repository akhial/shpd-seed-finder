// SPDX-License-Identifier: GPL-3.0-or-later

//! Real item artwork from the bundled Shattered Pixel Dungeon atlases.
//!
//! The atlas geometry mirrors the web frontend's `web/src/lib/sprites.ts` and
//! the Android client's `Components.kt`, so all three render pixel-identical
//! items:
//!
//! * `items.png` is a 16-column grid of 16×16 cells indexed row-major by
//!   [`ItemSprite::art_index`].
//! * Art is anchored to each cell's top-left, so drawing the whole cell leaves
//!   small items (rings, darts, seeds) hugging the corner. We crop to the art's
//!   alpha bounding box — measured at runtime on first use — and centre that
//!   crop in the target box, keeping the pixel scale of a full-cell render.
//! * Rings are drawn as a gem, and Shattered Pixel Dungeon shuffles which gem
//!   each ring class wears once per run, so the art cell of a ring belongs to
//!   the seed rather than to the class. They are told apart by a type glyph
//!   from `item_icons.png` (8×8 cells, 16 columns), drawn at the same scale
//!   anchored to the sprite box's top-right; the glyph names the *class*, so it
//!   is the same in every run. [`ItemSprite`] pairs the two cells.
//!
//! Everything is scaled by nearest-neighbour into the widget's device pixels
//! and then blitted 1:1, so the artwork stays crisp at any scale factor.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{cairo, gdk, gio, glib};
use shpd_seedfinder_core::catalog::ItemDefinition;
use shpd_seedfinder_core::run::RingGems;

use crate::config::RESOURCE_BASE_PATH;
use crate::glow::{self, Glow};
use crate::state::kind_icon;

/// Logical size of one rendered item sprite.
pub const SIZE: i32 = 24;

const CELL: i32 = 16;
const SHEET_COLUMNS: u16 = 16;
const ICON_CELL: i32 = 8;
const ICON_COLUMNS: usize = 16;

/// Art dimensions of each ring glyph within its 8×8 cell, indexed by the ring
/// class's glyph index (Accuracy, Arcana, Elements, … Wealth).
const RING_ICON_SIZES: [(i32, i32); 12] = [
    (7, 7), // Accuracy
    (7, 7), // Arcana
    (7, 7), // Elements
    (7, 5), // Energy
    (7, 7), // Evasion
    (5, 6), // Force
    (7, 6), // Furor
    (6, 6), // Haste
    (7, 7), // Might
    (7, 7), // Sharpshooting
    (6, 6), // Tenacity
    (7, 6), // Wealth
];

/// What to draw for one item: the `items.png` cell holding its art and, for a
/// ring, the `item_icons.png` cell holding its class glyph.
///
/// The two are independent. A ring's art cell is the gem its class was given,
/// which the run's seed decides, while its glyph names the class in every run,
/// so neither can be derived from the other. Build the pair with
/// [`Self::from_catalog`] on a surface that has no run to ask — the requirement
/// editor — and with [`Self::in_run`] on one showing a scouted seed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemSprite {
    definition: &'static ItemDefinition,
    art_index: u16,
    ring_glyph: Option<usize>,
}

impl ItemSprite {
    /// The catalog's own cells: a ring shows the gem its *class* owns, which is
    /// what a seedless surface must draw.
    #[must_use]
    pub fn from_catalog(definition: &'static ItemDefinition) -> Self {
        Self::new(definition, definition.sprite_index)
    }

    /// The cells a run whose ring gems are `gems` draws this item in.
    #[must_use]
    pub fn in_run(definition: &'static ItemDefinition, gems: RingGems) -> Self {
        Self::new(definition, definition.sprite_index_in(gems))
    }

    fn new(definition: &'static ItemDefinition, art_index: u16) -> Self {
        Self {
            definition,
            art_index,
            ring_glyph: definition.ring_glyph_index().map(usize::from),
        }
    }

    /// The `items.png` cell the art comes from.
    #[must_use]
    pub const fn art_index(self) -> u16 {
        self.art_index
    }

    /// The ring class's glyph cell, or `None` for everything that is not a
    /// ring.
    #[must_use]
    pub const fn ring_glyph(self) -> Option<usize> {
        self.ring_glyph
    }
}

/// A rectangle of atlas pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// A decoded atlas: premultiplied ARGB32 words in native byte order, exactly as
/// `cairo::Format::ARgb32` stores them.
struct Pixels {
    width: i32,
    height: i32,
    words: Vec<u32>,
}

impl Pixels {
    fn decode(resource: &str) -> Option<Self> {
        let bytes = gio::resources_lookup_data(resource, gio::ResourceLookupFlags::NONE).ok()?;
        let texture = gdk::Texture::from_bytes(&bytes).ok()?;
        let width = texture.width();
        let height = texture.height();
        let stride = usize::try_from(width).ok()?.checked_mul(4)?;
        let mut data = vec![0u8; stride.checked_mul(usize::try_from(height).ok()?)?];
        // `download` always writes premultiplied ARGB32, matching Cairo.
        texture.download(&mut data, stride);
        let words = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| u32::from_ne_bytes(*chunk))
            .collect();
        Some(Self {
            width,
            height,
            words,
        })
    }

    fn word(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return 0;
        }
        let index = usize::try_from(y).unwrap_or(0) * usize::try_from(self.width).unwrap_or(0)
            + usize::try_from(x).unwrap_or(0);
        self.words.get(index).copied().unwrap_or(0)
    }

    /// Alpha is the top byte of an ARGB32 word, whatever the host endianness.
    fn alpha(&self, x: i32, y: i32) -> u8 {
        u8::try_from(self.word(x, y) >> 24).unwrap_or(0)
    }
}

/// The tightest rectangle inside `cell` that holds every non-transparent pixel,
/// or the whole cell when it is empty — matching the web's bounds fallback.
fn alpha_bounds(pixels: &Pixels, cell: Rect) -> Rect {
    let mut left = cell.width;
    let mut top = cell.height;
    let mut right = -1;
    let mut bottom = -1;
    for row in 0..cell.height {
        for column in 0..cell.width {
            if pixels.alpha(cell.x + column, cell.y + row) == 0 {
                continue;
            }
            left = left.min(column);
            top = top.min(row);
            right = right.max(column);
            bottom = bottom.max(row);
        }
    }
    if right < 0 {
        return Rect {
            x: 0,
            y: 0,
            width: cell.width,
            height: cell.height,
        };
    }
    Rect {
        x: left,
        y: top,
        width: right - left + 1,
        height: bottom - top + 1,
    }
}

/// Copies `source` out of `pixels` into a fresh surface of `width` × `height`
/// device pixels, sampling nearest-neighbour from each destination pixel's
/// centre exactly as `image-rendering: pixelated` does in the browser.
fn scale_nearest(
    pixels: &Pixels,
    source: Rect,
    width: i32,
    height: i32,
) -> Option<cairo::ImageSurface> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    let stride = usize::try_from(surface.stride()).ok()?;
    {
        let mut data = surface.data().ok()?;
        for row in 0..height {
            let source_row = sample(row, height, source.height);
            for column in 0..width {
                let source_column = sample(column, width, source.width);
                let word = pixels.word(source.x + source_column, source.y + source_row);
                let offset =
                    usize::try_from(row).ok()? * stride + usize::try_from(column).ok()? * 4;
                data.get_mut(offset..offset + 4)?
                    .copy_from_slice(&word.to_ne_bytes());
            }
        }
    }
    Some(surface)
}

/// Nearest-neighbour source index for one destination index.
fn sample(destination: i32, destination_extent: i32, source_extent: i32) -> i32 {
    if destination_extent <= 0 || source_extent <= 0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)] // Extents are at most a few hundred pixels.
    let index = ((f64::from(destination) + 0.5) * f64::from(source_extent)
        / f64::from(destination_extent))
    .floor() as i32;
    index.clamp(0, source_extent - 1)
}

/// The two decoded atlases plus everything derived from them, cached per
/// thread. Sprite art and ring glyphs are cached by (index, device size) so the
/// manifest can list dozens of items and animate them without rescaling.
struct Atlas {
    items: Pixels,
    icons: Pixels,
    bounds: RefCell<HashMap<u16, Rect>>,
    art: RefCell<HashMap<(u16, i32), Rc<cairo::ImageSurface>>>,
    glyphs: RefCell<HashMap<(usize, i32), Rc<cairo::ImageSurface>>>,
}

impl Atlas {
    fn load() -> Option<Rc<Self>> {
        let items = Pixels::decode(&format!(
            "{RESOURCE_BASE_PATH}/third_party/shattered-pixel-dungeon/items.png"
        ))?;
        let icons = Pixels::decode(&format!(
            "{RESOURCE_BASE_PATH}/third_party/shattered-pixel-dungeon/item_icons.png"
        ))?;
        Some(Rc::new(Self {
            items,
            icons,
            bounds: RefCell::new(HashMap::new()),
            art: RefCell::new(HashMap::new()),
            glyphs: RefCell::new(HashMap::new()),
        }))
    }

    /// The atlas cell holding one sprite index.
    fn cell(sprite_index: u16) -> Rect {
        Rect {
            x: i32::from(sprite_index % SHEET_COLUMNS) * CELL,
            y: i32::from(sprite_index / SHEET_COLUMNS) * CELL,
            width: CELL,
            height: CELL,
        }
    }

    /// The art's bounding box within its cell, measured once per sprite.
    fn bounds(&self, sprite_index: u16) -> Rect {
        if let Some(bounds) = self.bounds.borrow().get(&sprite_index) {
            return *bounds;
        }
        let bounds = alpha_bounds(&self.items, Self::cell(sprite_index));
        self.bounds.borrow_mut().insert(sprite_index, bounds);
        bounds
    }

    /// The cropped sprite art scaled to a `size`-device-pixel box.
    fn art(&self, sprite_index: u16, size: i32) -> Option<Rc<cairo::ImageSurface>> {
        if let Some(surface) = self.art.borrow().get(&(sprite_index, size)) {
            return Some(Rc::clone(surface));
        }
        let cell = Self::cell(sprite_index);
        let bounds = self.bounds(sprite_index);
        let source = Rect {
            x: cell.x + bounds.x,
            y: cell.y + bounds.y,
            width: bounds.width,
            height: bounds.height,
        };
        let surface = Rc::new(scale_nearest(
            &self.items,
            source,
            scaled_extent(bounds.width, size),
            scaled_extent(bounds.height, size),
        )?);
        self.art
            .borrow_mut()
            .insert((sprite_index, size), Rc::clone(&surface));
        Some(surface)
    }

    /// One ring type glyph scaled to the same `size`-device-pixel box.
    fn glyph(&self, icon: usize, size: i32) -> Option<Rc<cairo::ImageSurface>> {
        if let Some(surface) = self.glyphs.borrow().get(&(icon, size)) {
            return Some(Rc::clone(surface));
        }
        let (width, height) = *RING_ICON_SIZES.get(icon)?;
        let source = Rect {
            x: i32::try_from(icon % ICON_COLUMNS).ok()? * ICON_CELL,
            y: i32::try_from(icon / ICON_COLUMNS).ok()? * ICON_CELL,
            width,
            height,
        };
        let surface = Rc::new(scale_nearest(
            &self.icons,
            source,
            scaled_extent(width, size),
            scaled_extent(height, size),
        )?);
        self.glyphs
            .borrow_mut()
            .insert((icon, size), Rc::clone(&surface));
        Some(surface)
    }
}

/// Scales one source extent into a `size`-device-pixel 16×16 box.
fn scaled_extent(extent: i32, size: i32) -> i32 {
    let scaled = (f64::from(extent) * f64::from(size) / f64::from(CELL)).round();
    #[allow(clippy::cast_possible_truncation)] // Bounded by the widget's pixel size.
    let scaled = scaled as i32;
    scaled.max(1)
}

thread_local! {
    static ATLAS: Option<Rc<Atlas>> = Atlas::load();
}

fn atlas() -> Option<Rc<Atlas>> {
    ATLAS.with(|atlas| atlas.as_ref().map(Rc::clone))
}

/// Whether GTK wants animations; mirrors the web's `prefers-reduced-motion`
/// check, which freezes the pulse at a static value instead.
fn animations_enabled() -> bool {
    gtk::Settings::default().is_none_or(|settings| settings.is_gtk_enable_animations())
}

fn glow_value(frame_time: i64, period: f64) -> f64 {
    if animations_enabled() {
        glow::value_at(frame_time, period)
    } else {
        glow::STATIC_VALUE
    }
}

/// The real sprite for one item, pulsing `glow` when it carries an enchantment
/// or curse. Falls back to the family's symbolic icon if the atlases cannot be
/// decoded.
///
/// The caller chooses the cells: pass [`ItemSprite::in_run`] wherever the item
/// belongs to a scouted seed, so its rings wear that run's gems.
#[must_use]
pub fn item_image(sprite: ItemSprite, glow: Option<Glow>) -> gtk::Widget {
    let definition = sprite.definition;
    let Some(atlas) = atlas() else {
        let image =
            gtk::Image::from_icon_name(kind_icon(definition.kind, definition.weapon_category()));
        image.set_tooltip_text(Some(definition.name));
        return image.upcast();
    };

    let area = gtk::DrawingArea::builder()
        .content_width(SIZE)
        .content_height(SIZE)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .accessible_role(gtk::AccessibleRole::Img)
        .build();
    area.update_property(&[gtk::accessible::Property::Label(definition.name)]);

    area.set_draw_func(move |area, context, width, height| {
        draw(&atlas, area, context, width, height, sprite, glow, SIZE);
    });
    if let Some(glow) = glow {
        animate(&area, glow.period);
    }
    area.upcast()
}

#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)] // Drawing geometry and Copy sprite parameters.
fn draw(
    atlas: &Atlas,
    area: &gtk::DrawingArea,
    context: &cairo::Context,
    width: i32,
    height: i32,
    sprite: ItemSprite,
    glow: Option<Glow>,
    size: i32,
) {
    let factor = area.scale_factor().max(1);
    let box_size = size * factor;
    // Draw in device pixels so every blit lands on exact pixel boundaries.
    let scale = 1.0 / f64::from(factor);
    context.scale(scale, scale);
    let origin_x = f64::from((width - size) * factor) / 2.0;
    let origin_y = f64::from((height - size) * factor) / 2.0;

    let Some(art) = atlas.art(sprite.art_index(), box_size) else {
        return;
    };
    // The atlas anchors art to the cell's top-left; centre the crop instead.
    let art_x = origin_x + (f64::from(box_size - art.width()) / 2.0).round();
    let art_y = origin_y + (f64::from(box_size - art.height()) / 2.0).round();
    if blit(context, &art, art_x, art_y).is_err() {
        return;
    }

    if let Some(glow) = glow {
        let value = area.frame_clock().map_or(glow::STATIC_VALUE, |clock| {
            glow_value(clock.frame_time(), glow.period)
        });
        let (red, green, blue) = glow.rgb();
        context.set_source_rgba(red, green, blue, value);
        // Masking by the sprite's own alpha blends only the art toward the glow
        // colour, reproducing upstream's `texel * (1 - v) + glow * v` shader
        // with no bloom or halo outside the silhouette.
        let _ = context.mask_surface(&*art, art_x, art_y);
    }

    // Ring glyphs sit at the sprite box's top-right, never tinted by the glow.
    if let Some(icon) = sprite.ring_glyph()
        && let Some(glyph) = atlas.glyph(icon, box_size)
    {
        let glyph_x = origin_x + f64::from(box_size - glyph.width());
        let _ = blit(context, &glyph, glyph_x, origin_y);
    }
}

fn blit(
    context: &cairo::Context,
    surface: &cairo::ImageSurface,
    x: f64,
    y: f64,
) -> Result<(), cairo::Error> {
    context.set_source_surface(surface, x, y)?;
    // Already scaled to device pixels, but stay nearest under fractional scales.
    context.source().set_filter(cairo::Filter::Nearest);
    context.paint()
}

/// Drives the pulse off the frame clock, redrawing only when the glow strength
/// actually moves, and drops the tick callback whenever the widget is
/// unrealized so recycled rows never leak one.
fn animate(area: &gtk::DrawingArea, period: f64) {
    let tick: Rc<RefCell<Option<gtk::TickCallbackId>>> = Rc::new(RefCell::new(None));
    let last = Rc::new(Cell::new(f64::NAN));
    area.connect_realize({
        let tick = Rc::clone(&tick);
        move |area| {
            if tick.borrow().is_some() {
                return;
            }
            let last = Rc::clone(&last);
            let id = area.add_tick_callback(move |area, clock| {
                let value = glow_value(clock.frame_time(), period);
                if last.get().is_nan() || (value - last.get()).abs() > 0.002 {
                    last.set(value);
                    area.queue_draw();
                }
                glib::ControlFlow::Continue
            });
            tick.replace(Some(id));
        }
    });
    area.connect_unrealize(move |_| {
        if let Some(id) = tick.take() {
            id.remove();
        }
    });
}

/// A responsive trinket tile. The aspect frame gives all four choices identical
/// square geometry; drawing the name lets it shrink without imposing a minimum
/// width on the pane. Artwork uses the same nearest-neighbour atlas as items.
#[must_use]
pub fn trinket_tile(
    definition: &'static ItemDefinition,
    matched: bool,
    primary: bool,
) -> gtk::Widget {
    let area = gtk::DrawingArea::builder()
        .content_width(0)
        .content_height(if primary { 0 } else { 24 })
        .hexpand(true)
        .accessible_role(gtk::AccessibleRole::Img)
        .tooltip_text(definition.name)
        .build();
    let description = if matched {
        format!("{}, matches requirement", definition.name)
    } else {
        definition.name.to_owned()
    };
    area.update_property(&[gtk::accessible::Property::Label(&description)]);
    if primary {
        area.add_css_class("trinket-choice");
        if matched {
            area.add_css_class("trinket-match");
        }
    }
    area.set_draw_func(move |area, context, width, height| {
        let art_height = if primary { height * 3 / 4 } else { height };
        let size = (width - 8)
            .min(art_height - 8)
            .min(if primary { 48 } else { 24 })
            .max(1);
        if let Some(atlas) = atlas() {
            let _ = context.save();
            draw(
                &atlas,
                area,
                context,
                width,
                art_height,
                ItemSprite::from_catalog(definition),
                None,
                size,
            );
            let _ = context.restore();
        }
        if primary {
            let color = area.color();
            context.set_source_rgba(
                f64::from(color.red()),
                f64::from(color.green()),
                f64::from(color.blue()),
                f64::from(color.alpha()),
            );
            context.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
            context.set_font_size(11.0);
            if let Ok(extents) = context.text_extents(definition.name) {
                let available = f64::from((width - 10).max(1));
                let scale = (available / extents.x_advance().max(1.0)).min(1.0);
                context.set_font_size(11.0 * scale);
                context.move_to(
                    (f64::from(width) - extents.x_advance() * scale) / 2.0,
                    f64::from(height) * 0.88,
                );
                let _ = context.show_text(definition.name);
            }
        }
    });
    if primary {
        gtk::AspectFrame::builder()
            .ratio(1.0)
            .obey_child(false)
            .child(&area)
            .hexpand(true)
            .build()
            .upcast()
    } else {
        area.upcast()
    }
}

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{ITEMS, ItemId, ItemKind, RING_SPRITE_BASE, item};
    use shpd_seedfinder_core::challenges::Challenges;
    use shpd_seedfinder_core::run::RingGems;
    use shpd_seedfinder_core::seed::DungeonSeed;
    use shpd_seedfinder_session::production_scout_world;

    use super::{
        Atlas, ItemSprite, Pixels, RING_ICON_SIZES, Rect, alpha_bounds, sample, scaled_extent,
    };

    /// Registers the bundled resources so the atlas can be decoded in tests.
    fn register_resources() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            gtk::gio::resources_register_include!("dev.seedseeker.SeedSeeker.gresource")
                .expect("Seed Seeker resources must be valid");
        });
    }

    #[test]
    fn catalog_sprites_draw_the_ring_block_in_class_order() {
        // With no run to ask, every item keeps its catalog cell, and the ring
        // classes own the block contiguously from the base — which is also what
        // makes a class's offset its glyph index.
        let mut rings = 0;
        for definition in ITEMS {
            let sprite = ItemSprite::from_catalog(definition);
            assert_eq!(sprite.art_index(), definition.sprite_index);
            assert_eq!(
                sprite.ring_glyph().is_some(),
                definition.kind == ItemKind::Ring,
                "{} maps to the wrong glyph slot",
                definition.name
            );
            if let Some(glyph) = sprite.ring_glyph() {
                assert_eq!(glyph, rings);
                let offset = u16::try_from(glyph).expect("twelve ring classes fit u16");
                assert_eq!(sprite.art_index(), RING_SPRITE_BASE + offset);
                rings += 1;
            }
        }
        assert_eq!(rings, RING_ICON_SIZES.len());
    }

    #[test]
    fn a_scouted_ring_wears_the_gem_its_run_gave_it() {
        // YKH-LGJ-WDQ hands the ring of haste a diamond, the last gem in the
        // block, so that run draws it four cells past the class's own catalog
        // cell. The glyph that names the class does not move with it. Take the
        // table off a scouted world, which is where the manifest reads it.
        let seed = DungeonSeed::from_code("YKH-LGJ-WDQ").expect("a valid seed code");
        let gems = production_scout_world(seed, Challenges::NONE)
            .expect("an unchallenged run of a valid seed must generate")
            .ring_gems;
        let haste = item(ItemId::RingHaste);
        let drawn = ItemSprite::in_run(haste, gems);
        assert_eq!(drawn.art_index(), RING_SPRITE_BASE + 11);
        assert_eq!(drawn.ring_glyph(), Some(7));
        assert_eq!(
            ItemSprite::from_catalog(haste).art_index(),
            RING_SPRITE_BASE + 7
        );

        for definition in ITEMS {
            let catalog = ItemSprite::from_catalog(definition);
            // A run only ever moves rings, and only their art.
            assert_eq!(
                ItemSprite::in_run(definition, gems).ring_glyph(),
                catalog.ring_glyph()
            );
            if definition.kind != ItemKind::Ring {
                assert_eq!(
                    ItemSprite::in_run(definition, gems),
                    catalog,
                    "{} moved with the run",
                    definition.name
                );
            }
            // An unshuffled table is the catalog's own reading of the block.
            assert_eq!(
                ItemSprite::in_run(definition, RingGems::UNSHUFFLED),
                catalog
            );
        }

        // The run permutes the block rather than pointing anywhere else in the
        // atlas, so every ring still lands on real ring art.
        let mut cells: Vec<u16> = ITEMS
            .iter()
            .filter(|definition| definition.kind == ItemKind::Ring)
            .map(|definition| ItemSprite::in_run(definition, gems).art_index())
            .collect();
        cells.sort_unstable();
        assert_eq!(
            cells,
            (0..12)
                .map(|offset| RING_SPRITE_BASE + offset)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn alpha_bounds_crop_to_the_visible_art() {
        // A 4×4 cell at (4, 0) of an 8×4 sheet with one opaque pixel at (1, 2).
        let mut words = vec![0u32; 8 * 4];
        words[2 * 8 + 5] = 0xff00_0000;
        let pixels = Pixels {
            width: 8,
            height: 4,
            words,
        };
        let cell = Rect {
            x: 4,
            y: 0,
            width: 4,
            height: 4,
        };
        assert_eq!(
            alpha_bounds(&pixels, cell),
            Rect {
                x: 1,
                y: 2,
                width: 1,
                height: 1,
            }
        );
        // An empty cell falls back to the whole cell.
        let empty = Pixels {
            width: 8,
            height: 4,
            words: vec![0u32; 8 * 4],
        };
        assert_eq!(
            alpha_bounds(&empty, cell),
            Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            }
        );
    }

    #[test]
    fn nearest_sampling_matches_pixelated_scaling() {
        // Doubling repeats every source pixel exactly twice.
        assert_eq!(
            (0..8).map(|d| sample(d, 8, 4)).collect::<Vec<_>>(),
            vec![0, 0, 1, 1, 2, 2, 3, 3]
        );
        // 1.5× samples from each destination pixel's centre.
        assert_eq!(
            (0..6).map(|d| sample(d, 6, 4)).collect::<Vec<_>>(),
            vec![0, 1, 1, 2, 3, 3]
        );
        assert_eq!(sample(0, 0, 4), 0);
        assert_eq!(scaled_extent(16, 24), 24);
        assert_eq!(scaled_extent(7, 24), 11);
        assert_eq!(scaled_extent(1, 24), 2);
        assert_eq!(scaled_extent(1, 8), 1);
    }

    #[test]
    fn the_bundled_atlases_decode_and_crop_every_catalog_sprite() {
        register_resources();
        let atlas = Atlas::load().expect("the bundled atlases must decode");
        assert_eq!((atlas.items.width, atlas.items.height), (256, 512));
        assert_eq!((atlas.icons.width, atlas.icons.height), (128, 64));

        for definition in ITEMS {
            let bounds = atlas.bounds(definition.sprite_index);
            assert!(
                bounds.width > 0 && bounds.height > 0,
                "{} has empty art",
                definition.name
            );
            assert!(
                bounds.x + bounds.width <= 16 && bounds.y + bounds.height <= 16,
                "{} escapes its cell",
                definition.name
            );
            // Every catalog sprite has real art, so none may fall back to the
            // full cell with a fully transparent border.
            assert!(
                bounds.width < 16 || bounds.height < 16 || bounds.x == 0,
                "{} looks empty",
                definition.name
            );
            let art = atlas
                .art(definition.sprite_index, 24)
                .expect("sprite art must scale");
            assert_eq!(art.width(), scaled_extent(bounds.width, 24));
            assert_eq!(art.height(), scaled_extent(bounds.height, 24));
        }
        for (icon, (width, height)) in RING_ICON_SIZES.iter().enumerate() {
            let glyph = atlas.glyph(icon, 24).expect("ring glyphs must scale");
            assert_eq!(glyph.width(), scaled_extent(*width, 24));
            assert_eq!(glyph.height(), scaled_extent(*height, 24));
        }
        // Both caches are keyed by (index, device size), so repeats are free.
        let distinct: std::collections::HashSet<u16> =
            ITEMS.iter().map(|item| item.sprite_index).collect();
        assert_eq!(atlas.art.borrow().len(), distinct.len());
        assert_eq!(atlas.glyphs.borrow().len(), RING_ICON_SIZES.len());
    }
}
