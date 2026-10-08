//! Packs glyph bitmaps into one texture and turns a scene into quads.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use ttf_parser::GlyphId;

use crate::font::Fonts;
use crate::scene::{FontKind, GlyphInstance, Scene};

/// FxHash-style hasher for [`Key`]; SipHash is overkill for an 11-byte key.
#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
        for chunk in bytes.chunks(8) {
            let word = chunk
                .iter()
                .rev()
                .fold(0, |word, &byte| word << 8 | u64::from(byte));
            self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(SEED);
        }
    }
}

type EntryMap = HashMap<Key, Option<Entry>, BuildHasherDefault<KeyHasher>>;

/// Which backend fills the atlas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Source {
    /// An 8-bit alpha bitmap per glyph *per size*.
    #[cfg(feature = "raster")]
    Coverage,
    /// A signed distance field per glyph, resampled in the shader.
    #[cfg(feature = "sdf")]
    SignedDistance {
        /// Pixels per em to sample at. 48 is ample for text-sized rendering.
        sample_size: f32,
        /// How far the encoding reaches before it saturates.
        spread: f32,
    },
}

impl Source {
    /// A distance-field source with the defaults from [`crate::export::sdf`].
    #[cfg(feature = "sdf")]
    #[must_use]
    pub fn signed_distance() -> Source {
        Source::SignedDistance {
            sample_size: 48.0,
            spread: crate::export::sdf::DEFAULT_SPREAD,
        }
    }
}

/// What identifies one packed bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub font: FontKind,
    pub glyph: GlyphId,
    /// Pixels per em, rounded to a whole pixel; zero for a distance field.
    pub size: u32,
}

/// Where one glyph landed, and how big it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// Position in the texture, in texels.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Offset from the pen origin to the bitmap's left and top edges, y-down,
    /// in pixels at [`Entry::size`].
    pub left: f32,
    pub top: f32,
    /// The pixels-per-em the bitmap and the offsets describe.
    pub size: f32,
}

/// One glyph, ready to draw.
///
/// `min`/`max` are scene-space pixels (y down); `uv_*` are normalised texture coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    /// Tint colour, or [`PaintId::INHERIT`](crate::PaintId::INHERIT); resolve with [`Scene::resolve`].
    #[cfg(feature = "attrs")]
    pub paint: crate::attrs::PaintId,
    /// Opacity in `0.0..=1.0`.
    #[cfg(feature = "attrs")]
    pub alpha: f32,
}

/// Empty texels around every entry, so bilinear filtering does not bleed between neighbours.
const PADDING: u32 = 1;

/// Edge lengths of the synthetic patches, in texels.
const SOLID: u32 = 2;
const DOT: u32 = 32;
const BAR: u32 = 32;
/// How far the rule patch's contour sits in from its border.
const BAR_PAD: u32 = 8;

/// A shelf-packed 8-bit texture of glyph bitmaps.
#[derive(Debug)]
pub struct Atlas {
    source: Source,
    width: u32,
    height: u32,
    max_height: u32,
    data: Vec<u8>,
    /// `None` marks a glyph with no outline.
    entries: EntryMap,
    /// The open shelf: its top edge, its height, and how far along it is full.
    shelf_y: u32,
    shelf_height: u32,
    shelf_x: u32,
    dirty: bool,
    resized: bool,
    /// Number of clear-and-repack cycles so far.
    evictions: u64,
    /// Full-coverage block for drawing rules.
    solid: Option<(u32, u32)>,
    /// Radial distance-field patch for particles.
    dot: Option<(u32, u32)>,
    /// Rectangle distance-field patch for rules.
    bar: Option<(u32, u32)>,
}

impl Atlas {
    /// An empty atlas of fixed `width`; the height doubles up to `max_height`, then it repacks.
    #[must_use]
    pub fn new(width: u32, max_height: u32, source: Source) -> Atlas {
        let width = width.max(1);
        let height = 64.min(max_height.max(1));
        let mut atlas = Atlas {
            source,
            width,
            height,
            max_height: max_height.max(height),
            data: vec![0; (width as usize) * (height as usize)],
            entries: EntryMap::default(),
            shelf_y: 0,
            shelf_height: 0,
            shelf_x: 0,
            dirty: false,
            resized: false,
            evictions: 0,
            solid: None,
            dot: None,
            bar: None,
        };
        atlas.reserve_patches();
        atlas
    }

    fn reserve_patches(&mut self) {
        let coverage = self.is_coverage();
        self.solid = self.reserve_patch(SOLID, |_, _| 1.0);

        // Radial distance field, same encoding as a glyph; the disc fills half the patch.
        let centre = (DOT as f32 - 1.0) / 2.0;
        let radius = centre * 0.5;
        let spread = centre - radius;
        self.dot = self.reserve_patch(DOT, |x, y| {
            let (dx, dy) = (x as f32 - centre, y as f32 - centre);
            // Positive inside, matching the sign convention in `sdf`.
            let signed = radius - (dx * dx + dy * dy).sqrt();
            // A coverage shader samples alpha directly, so store coverage rather than distance.
            if coverage {
                signed + 0.5
            } else {
                signed / spread.max(1e-3) * 0.5 + 0.5
            }
        });

        // Rectangle distance field for rules, applied as a nine-slice. Distance-field atlases only.
        if !coverage {
            self.bar = self.reserve_patch(BAR, |x, y| {
                let inset = (x.min(BAR - 1 - x)).min(y.min(BAR - 1 - y)) as f32;
                let signed = inset - (BAR_PAD as f32 - 0.5);
                signed / BAR_PAD as f32 * 0.5 + 0.5
            });
        }
    }

    /// Pack a square patch whose texels are `value(x, y)`, clamped to `0.0..=1.0`.
    fn reserve_patch(&mut self, size: u32, value: impl Fn(u32, u32) -> f32) -> Option<(u32, u32)> {
        let pixels: Vec<u8> = (0..size * size)
            .map(|i| (value(i % size, i / size).clamp(0.0, 1.0) * 255.0) as u8)
            .collect();
        let (x, y) = self.allocate(size, size)?;
        self.blit(x, y, size, size, &pixels);
        Some((x, y))
    }

    /// The rectangle patch a rule is drawn over, as `[u_min, v_min, u_max,
    /// v_max]` and the fraction of it that is border.
    ///
    /// `None` for a [`Source::Coverage`] atlas, which draws a rule as one quad over a solid texel.
    #[must_use]
    pub fn bar_uv(&self) -> Option<([f32; 4], f32)> {
        let (x, y) = self.bar?;
        let size = BAR as f32;
        Some((
            [
                x as f32 / self.width as f32,
                y as f32 / self.height as f32,
                (x as f32 + size) / self.width as f32,
                (y as f32 + size) / self.height as f32,
            ],
            BAR_PAD as f32 / size,
        ))
    }

    /// Texture coordinates of a texel that is entirely covered.
    ///
    /// `None` for an atlas too small to hold it.
    #[must_use]
    pub fn solid_uv(&self) -> Option<[f32; 2]> {
        let (x, y) = self.solid?;
        Some([
            (x as f32 + 0.5) / self.width as f32,
            (y as f32 + 0.5) / self.height as f32,
        ])
    }

    /// Texture coordinates of the radial patch a particle is drawn over.
    ///
    /// `[u_min, v_min, u_max, v_max]`, inset half a texel. `None` for an atlas too small to hold it.
    #[must_use]
    pub fn dot_uv(&self) -> Option<[f32; 4]> {
        const DOT: f32 = self::DOT as f32;
        let (x, y) = self.dot?;
        let (w, h) = (self.width as f32, self.height as f32);
        Some([
            (x as f32 + 0.5) / w,
            (y as f32 + 0.5) / h,
            (x as f32 + DOT - 0.5) / w,
            (y as f32 + DOT - 0.5) / h,
        ])
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The texture: `width * height` coverage or distance values, row-major.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    #[must_use]
    pub fn source(&self) -> Source {
        self.source
    }

    /// Whether this atlas holds coverage rather than distance.
    #[must_use]
    pub fn is_coverage(&self) -> bool {
        #[cfg(feature = "raster")]
        {
            matches!(self.source, Source::Coverage)
        }
        #[cfg(not(feature = "raster"))]
        {
            false
        }
    }

    /// How many glyphs are packed, blanks included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the texture has changed since this was last asked; clears the flag.
    pub fn take_dirty(&mut self) -> bool {
        core::mem::take(&mut self.dirty)
    }

    /// Whether the texture changed *size* since this was last asked; clears the flag.
    pub fn take_resized(&mut self) -> bool {
        core::mem::take(&mut self.resized)
    }

    /// Forget every entry and start the packing again.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.data.fill(0);
        self.shelf_y = 0;
        self.shelf_height = 0;
        self.shelf_x = 0;
        self.dirty = true;
        self.evictions += 1;
        // The patches were zeroed too; re-reserve them before any glyph takes their place.
        self.reserve_patches();
    }

    /// The key a glyph instance looks up under.
    #[must_use]
    pub fn key(&self, glyph: &GlyphInstance) -> Key {
        Key {
            font: glyph.font,
            glyph: glyph.glyph,
            size: self.size_key(glyph.size),
        }
    }

    // Without `raster` every size shares one entry.
    #[cfg_attr(not(feature = "raster"), allow(unused_variables))]
    fn size_key(&self, size: f32) -> u32 {
        match self.source {
            #[cfg(feature = "sdf")]
            Source::SignedDistance { .. } => 0,
            #[cfg(feature = "raster")]
            // Whole pixels: only the bitmap is baked at the rounded size; the quad keeps the exact one.
            Source::Coverage => {
                if size.is_finite() {
                    size.round().clamp(1.0, 4096.0) as u32
                } else {
                    0
                }
            }
        }
    }

    /// Pack every glyph a scene needs.
    ///
    /// Call it before [`Atlas::quad`]; a glyph already present costs a hash lookup.
    pub fn prepare(&mut self, scene: &Scene, fonts: &Fonts<'_>) {
        // Two passes at most: a mid-pass repack evicts what this pass already packed.
        for _ in 0..2 {
            let evictions = self.evictions;
            for glyph in &scene.glyphs {
                let key = self.key(glyph);
                if self.entries.contains_key(&key) {
                    continue;
                }
                let entry = self.render(key, fonts);
                self.entries.insert(key, entry);
            }
            if self.evictions == evictions {
                break;
            }
        }
    }

    /// Where a glyph instance goes, if it is packed and has any ink.
    ///
    /// `None` for a glyph with no outline, one too large to pack, or one not yet prepared.
    #[must_use]
    pub fn quad(&self, glyph: &GlyphInstance) -> Option<Quad> {
        let entry = (*self.entries.get(&self.key(glyph))?)?;
        if entry.size <= 0.0 {
            return None;
        }
        // Entry metrics are at the baked size; one ratio converts them all.
        let scale = glyph.size / entry.size;
        let x0 = glyph.x + entry.left * scale;
        let y0 = glyph.y + entry.top * scale;
        let (width, height) = (self.width as f32, self.height as f32);
        Some(Quad {
            min: [x0, y0],
            max: [
                x0 + entry.width as f32 * scale,
                y0 + entry.height as f32 * scale,
            ],
            uv_min: [entry.x as f32 / width, entry.y as f32 / height],
            uv_max: [
                (entry.x + entry.width) as f32 / width,
                (entry.y + entry.height) as f32 / height,
            ],
            #[cfg(feature = "attrs")]
            paint: glyph.attrs.paint,
            #[cfg(feature = "attrs")]
            alpha: glyph.attrs.alpha,
        })
    }

    fn render(&mut self, key: Key, fonts: &Fonts<'_>) -> Option<Entry> {
        let (mut entry, pixels) = self.bake(key, fonts)?;
        (entry.x, entry.y) = self.allocate(entry.width, entry.height)?;
        self.blit(entry.x, entry.y, entry.width, entry.height, &pixels);
        Some(entry)
    }

    /// The bitmap for a key and its entry, not yet placed.
    fn bake(&self, key: Key, fonts: &Fonts<'_>) -> Option<(Entry, Vec<u8>)> {
        let unplaced = |width, height, left: i32, top: i32, size| Entry {
            x: 0,
            y: 0,
            width,
            height,
            left: left as f32,
            top: top as f32,
            size,
        };
        match self.source {
            #[cfg(feature = "raster")]
            Source::Coverage => {
                use crate::export::raster::{rasterise_glyph, rasterise_text_glyph};
                let size = key.size as f32;
                let c = match key.font {
                    FontKind::Math => rasterise_glyph(&fonts.math, key.glyph, size),
                    FontKind::Text => rasterise_text_glyph(fonts.text.as_ref()?, key.glyph, size),
                }?;
                Some((unplaced(c.width, c.height, c.left, c.top, size), c.data))
            }
            #[cfg(feature = "sdf")]
            Source::SignedDistance {
                sample_size,
                spread,
            } => {
                use crate::export::sdf::{sdf_glyph, sdf_text_glyph};
                let text = fonts.text.as_ref();
                let f = match key.font {
                    FontKind::Math => sdf_glyph(&fonts.math, key.glyph, sample_size, spread),
                    FontKind::Text => sdf_text_glyph(text?, key.glyph, sample_size, spread),
                }?;
                Some((unplaced(f.width, f.height, f.left, f.top, f.size), f.data))
            }
        }
    }

    /// Find room for a `width` by `height` bitmap, growing or repacking if needed.
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let needed_width = width + 2 * PADDING;
        let needed_height = height + 2 * PADDING;
        // A glyph wider than the atlas can never fit, however much it grows.
        if needed_width > self.width || needed_height > self.max_height {
            return None;
        }

        for _ in 0..2 {
            if self.shelf_x + needed_width > self.width {
                self.shelf_y += self.shelf_height;
                self.shelf_height = 0;
                self.shelf_x = 0;
            }
            let shelf_height = self.shelf_height.max(needed_height);
            if self.shelf_y + shelf_height <= self.height {
                self.shelf_height = shelf_height;
                let x = self.shelf_x + PADDING;
                let y = self.shelf_y + PADDING;
                self.shelf_x += needed_width;
                return Some((x, y));
            }
            if self.height < self.max_height {
                self.grow(self.shelf_y + shelf_height);
            } else if self.entries.is_empty() {
                // Nothing packed to reclaim: a repack would fail again, and recurse through `clear`.
                return None;
            } else {
                self.clear();
            }
        }
        None
    }

    fn grow(&mut self, wanted: u32) {
        let height = (self.height.saturating_mul(2))
            .max(wanted)
            .min(self.max_height);
        if height <= self.height {
            return;
        }
        self.data
            .resize((self.width as usize) * (height as usize), 0);
        self.height = height;
        self.dirty = true;
        self.resized = true;
    }

    fn blit(&mut self, x: u32, y: u32, width: u32, height: u32, pixels: &[u8]) {
        // `allocate` guarantees the placement fits; the `else` avoids a panic if it ever does not.
        for row in 0..height as usize {
            let from = row * width as usize;
            let to = (y as usize + row) * self.width as usize + x as usize;
            let (Some(src), Some(dst)) = (
                pixels.get(from..from + width as usize),
                self.data.get_mut(to..to + width as usize),
            ) else {
                continue;
            };
            dst.copy_from_slice(src);
        }
        self.dirty = true;
    }
}

#[cfg(test)]
#[cfg(all(feature = "embed-math-font", feature = "raster"))]
mod tests {
    use super::*;
    use crate::{Fonts, Options};

    fn scene(source: &str, size: f32) -> (Fonts<'static>, Scene) {
        let fonts = Fonts::embedded().unwrap();
        let options = Options::default().with_font_size(size);
        let scene = crate::render(source, &fonts, &options).unwrap();
        (fonts, scene)
    }

    #[test]
    fn every_glyph_of_a_formula_gets_a_quad() {
        let (fonts, scene) = scene(r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}", 32.0);
        let mut atlas = Atlas::new(512, 2048, Source::Coverage);
        atlas.prepare(&scene, &fonts);
        assert!(atlas.take_dirty(), "packing glyphs did not dirty the atlas");

        let mut drawn = 0;
        for glyph in &scene.glyphs {
            let quad = atlas.quad(glyph).expect("a laid-out glyph has ink");
            assert!(quad.min[0] < quad.max[0] && quad.min[1] < quad.max[1]);
            // Padding plus rasteriser slack: a sanity bound, not a tight one.
            assert!(
                quad.min[1] > -scene.ascent - 4.0 && quad.max[1] < scene.descent + 4.0,
                "{quad:?} escapes the formula's box"
            );
            assert!((0.0..=1.0).contains(&quad.uv_min[0]) && (0.0..=1.0).contains(&quad.uv_max[1]));
            drawn += 1;
        }
        assert!(drawn > 10, "only {drawn} glyphs in a quadratic formula");
    }

    #[test]
    fn a_coverage_quad_lands_where_a_blit_would() {
        let (fonts, scene) = scene(r"\sqrt{x} + \frac{1}{2}", 37.0);
        let mut atlas = Atlas::new(512, 2048, Source::Coverage);
        atlas.prepare(&scene, &fonts);

        for glyph in &scene.glyphs {
            let quad = atlas.quad(glyph).expect("packed");
            let baked = atlas.key(glyph).size as f32;
            let coverage = crate::export::raster::rasterise_glyph(&fonts.math, glyph.glyph, baked)
                .expect("the same glyph the atlas baked");
            let scale = glyph.size / baked;

            assert!((quad.min[0] - (glyph.x + coverage.left as f32 * scale)).abs() < 1e-3);
            assert!((quad.min[1] - (glyph.y + coverage.top as f32 * scale)).abs() < 1e-3);
            assert!((quad.max[0] - quad.min[0] - coverage.width as f32 * scale).abs() < 1e-3);
            assert!((quad.max[1] - quad.min[1] - coverage.height as f32 * scale).abs() < 1e-3);

            let x = (quad.uv_min[0] * atlas.width() as f32).round() as u32;
            let y = (quad.uv_min[1] * atlas.height() as f32).round() as u32;
            let index = y as usize * atlas.width() as usize + x as usize;
            assert_eq!(
                atlas.data().get(index).copied(),
                coverage.data.first().copied(),
                "the quad points somewhere else in the texture"
            );
        }
    }

    #[test]
    fn a_second_pass_packs_nothing() {
        let (fonts, scene) = scene(r"x^2 + y^2 = z^2", 24.0);
        let mut atlas = Atlas::new(256, 1024, Source::Coverage);
        atlas.prepare(&scene, &fonts);
        let count = atlas.len();
        atlas.take_dirty();

        atlas.prepare(&scene, &fonts);
        assert_eq!(atlas.len(), count);
        assert!(!atlas.take_dirty(), "an unchanged atlas reported dirty");
    }

    #[test]
    fn coverage_keys_by_size_and_a_distance_field_does_not() {
        let (fonts, scene) = scene(r"x^{x^{x}}", 40.0);
        let sizes: std::collections::HashSet<u32> =
            scene.glyphs.iter().map(|g| g.size.round() as u32).collect();
        assert!(sizes.len() >= 3, "no script scaling in {sizes:?}");

        let mut coverage = Atlas::new(512, 2048, Source::Coverage);
        coverage.prepare(&scene, &fonts);
        assert_eq!(coverage.len(), sizes.len(), "one `x` per script level");

        #[cfg(feature = "sdf")]
        {
            let mut field = Atlas::new(512, 2048, Source::signed_distance());
            field.prepare(&scene, &fonts);
            assert_eq!(field.len(), 1, "one `x`, whatever the size");
        }
    }

    #[cfg(feature = "sdf")]
    #[test]
    fn one_distance_field_entry_serves_every_size() {
        let (fonts, small) = scene(r"\sum x", 16.0);
        let big = small.scaled(4.0);
        let mut atlas = Atlas::new(512, 2048, Source::signed_distance());
        atlas.prepare(&small, &fonts);
        atlas.prepare(&big, &fonts);
        assert_eq!(
            atlas.len(),
            small.glyphs.len(),
            "the big scene added entries"
        );

        for (a, b) in small.glyphs.iter().zip(&big.glyphs) {
            let (Some(qa), Some(qb)) = (atlas.quad(a), atlas.quad(b)) else {
                continue;
            };
            let wa = qa.max[0] - qa.min[0];
            let wb = qb.max[0] - qb.min[0];
            assert!(
                (wb - 4.0 * wa).abs() < 1e-3,
                "{wa} px did not quadruple to {wb}"
            );
            assert_eq!(qa.uv_min, qb.uv_min, "the same glyph moved in the texture");
        }
    }

    #[test]
    fn a_full_atlas_grows_without_disturbing_its_entries() {
        let (fonts, scene) = scene(r"\alpha\beta\gamma\delta\epsilon\zeta\eta\theta", 40.0);
        let mut atlas = Atlas::new(128, 4096, Source::Coverage);

        let first = scene.glyphs.first().copied().expect("a glyph");
        atlas.prepare(
            &Scene {
                glyphs: vec![first],
                ..scene.clone()
            },
            &fonts,
        );
        let before = atlas.quad(&first).expect("packed");
        let texel_before = atlas.entries.get(&atlas.key(&first)).copied().flatten();

        atlas.prepare(&scene, &fonts);
        assert!(atlas.height() > 64, "the atlas never grew");
        assert_eq!(
            texel_before,
            atlas.entries.get(&atlas.key(&first)).copied().flatten(),
            "growing repacked an existing glyph"
        );
        // Same texel, taller texture: the normalised coordinate moves proportionally.
        let after = atlas.quad(&first).expect("still packed");
        assert!(after.uv_min[1] < before.uv_min[1]);
        assert_eq!(after.min, before.min, "the quad moved on screen");
    }

    #[test]
    fn an_oversized_glyph_is_declined() {
        let (fonts, scene) = scene(r"\sum", 200.0);
        let mut atlas = Atlas::new(16, 32, Source::Coverage);
        atlas.prepare(&scene, &fonts);
        assert!(scene.glyphs.iter().all(|g| atlas.quad(g).is_none()));
    }

    #[test]
    fn an_atlas_of_any_size_is_usable() {
        let (fonts, scene) = scene("x", 40.0);
        for edge in [1, 2, 4, 8, 16, 33, 64, 256] {
            let mut atlas = Atlas::new(edge, edge, Source::Coverage);
            atlas.prepare(&scene, &fonts);
            // Whether any given glyph fits is not the point; returning is.
            let _ = scene.glyphs.iter().filter_map(|g| atlas.quad(g)).count();
        }
    }

    #[cfg(feature = "sdf")]
    #[test]
    fn a_too_small_distance_field_atlas_refuses_rather_than_repacking_forever() {
        for edge in [1, 2, 4, 8, 16, 33, 64, 256] {
            let atlas = Atlas::new(edge, edge, Source::signed_distance());
            assert_eq!(atlas.width(), edge.max(1));
        }
    }
}
