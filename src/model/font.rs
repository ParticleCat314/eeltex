//! Font access: metrics, `MATH` constants, glyph lookup.

use ttf_parser::math::{Constants, GlyphInfo, MathValue, Variants};
use ttf_parser::{Face, GlyphId, Rect};

use std::sync::OnceLock;

use crate::error::Error;

fn new_bbox_cache(face: &Face<'_>) -> Vec<OnceLock<Option<Rect>>> {
    (0..face.number_of_glyphs())
        .map(|_| OnceLock::new())
        .collect()
}

fn cached_bbox(cache: &[OnceLock<Option<Rect>>], face: &Face<'_>, glyph: GlyphId) -> Option<Rect> {
    match cache.get(usize::from(glyph.0)) {
        Some(slot) => *slot.get_or_init(|| face.glyph_bounding_box(glyph)),
        None => face.glyph_bounding_box(glyph),
    }
}

/// A face with an OpenType `MATH` table.
#[derive(Clone)]
pub struct MathFont<'a> {
    face: Face<'a>,
    upem: f32,
    constants: Constants<'a>,
    glyph_info: Option<GlyphInfo<'a>>,
    variants: Option<Variants<'a>>,
    /// Memoised ink boxes; `OnceLock` keeps the font `Sync`.
    bbox_cache: Vec<OnceLock<Option<Rect>>>,
    script_scale: f32,
    script_script_scale: f32,
}

impl core::fmt::Debug for MathFont<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MathFont")
            .field("upem", &self.upem)
            .field("glyphs", &self.face.number_of_glyphs())
            .field("has_variants", &self.variants.is_some())
            .finish()
    }
}

impl<'a> MathFont<'a> {
    /// Read a face from font data.
    ///
    /// # Errors
    ///
    /// [`Error::BadFont`] for unreadable data or a zero `unitsPerEm`; [`Error::NoMathTable`]
    /// or [`Error::NoMathConstants`] for a face without maths data.
    pub fn new(data: &'a [u8]) -> Result<MathFont<'a>, Error> {
        Self::from_collection(data, 0)
    }

    /// Read one face out of a font collection.
    ///
    /// # Errors
    ///
    /// As [`MathFont::new`], plus [`Error::BadFont`] for an out-of-range index.
    pub fn from_collection(data: &'a [u8], index: u32) -> Result<MathFont<'a>, Error> {
        let face = Face::parse(data, index)?;
        let upem = f32::from(face.units_per_em());
        // A zero `unitsPerEm` would turn every conversion into NaN.
        if upem <= 0.0 {
            return Err(Error::BadFont);
        }
        let math = face.tables().math.ok_or(Error::NoMathTable)?;
        let constants = math.constants.ok_or(Error::NoMathConstants)?;
        let (script_scale, script_script_scale) = crate::style::script_scales(&constants);
        Ok(MathFont {
            bbox_cache: new_bbox_cache(&face),
            face,
            upem,
            constants,
            glyph_info: math.glyph_info,
            variants: math.variants,
            script_scale,
            script_script_scale,
        })
    }

    #[must_use]
    pub fn face(&self) -> &Face<'a> {
        &self.face
    }

    #[must_use]
    pub fn units_per_em(&self) -> f32 {
        self.upem
    }

    pub(crate) fn constants(&self) -> &Constants<'a> {
        &self.constants
    }

    pub(crate) fn script_scale(&self) -> f32 {
        self.script_scale
    }

    pub(crate) fn script_script_scale(&self) -> f32 {
        self.script_script_scale
    }

    pub(crate) fn variants(&self) -> Option<&Variants<'a>> {
        self.variants.as_ref()
    }

    /// Design units to pixels at `size` pixels per em.
    #[must_use]
    pub fn px(&self, design_units: f32, size: f32) -> f32 {
        design_units * size / self.upem
    }

    /// A `MATH` constant, in pixels at `size`; device tables are ignored.
    pub(crate) fn k(&self, value: MathValue<'_>, size: f32) -> f32 {
        self.px(f32::from(value.value), size)
    }

    #[must_use]
    pub fn glyph_for(&self, ch: char) -> Option<GlyphId> {
        self.face.glyph_index(ch)
    }

    /// The glyph's advance width, in design units.
    #[must_use]
    pub fn advance(&self, glyph: GlyphId) -> f32 {
        f32::from(self.face.glyph_hor_advance(glyph).unwrap_or(0))
    }

    /// The glyph's ink bounding box, in design units, y-up; `None` for a blank glyph.
    #[must_use]
    pub fn bbox(&self, glyph: GlyphId) -> Option<Rect> {
        cached_bbox(&self.bbox_cache, &self.face, glyph)
    }

    /// The glyph's italic correction, in design units.
    #[must_use]
    pub fn italic_correction(&self, glyph: GlyphId) -> f32 {
        self.glyph_info
            .as_ref()
            .and_then(|i| i.italic_corrections.as_ref())
            .and_then(|c| c.get(glyph))
            .map_or(0.0, |v| f32::from(v.value))
    }

    /// Horizontal accent attachment, in design units from the origin; `None` means centre.
    #[must_use]
    pub fn top_accent_attachment(&self, glyph: GlyphId) -> Option<f32> {
        self.glyph_info
            .as_ref()
            .and_then(|i| i.top_accent_attachments.as_ref())
            .and_then(|a| a.get(glyph))
            .map(|v| f32::from(v.value))
    }

    #[must_use]
    pub fn is_extended_shape(&self, glyph: GlyphId) -> bool {
        self.glyph_info
            .as_ref()
            .and_then(|i| i.extended_shapes.as_ref())
            .and_then(|s| s.get(glyph))
            .is_some()
    }

    /// The maths axis height, with TeX's value standing in for zero.
    #[must_use]
    pub fn axis_height(&self, size: f32) -> f32 {
        let raw = self.k(self.constants.axis_height(), size);
        if raw > 0.0 {
            raw
        } else {
            // TeX's `axisheight` for Computer Modern, as a fraction of an em.
            size * 0.25
        }
    }

    /// The fraction bar thickness; zero is meaningful (`\binom`).
    #[must_use]
    pub fn fraction_rule_thickness(&self, size: f32) -> f32 {
        self.k(self.constants.fraction_rule_thickness(), size)
    }

    /// A rule thickness that is never zero, for radicals and over/underlines.
    #[must_use]
    pub fn default_rule_thickness(&self, size: f32) -> f32 {
        let raw = self.fraction_rule_thickness(size);
        if raw > 0.0 { raw } else { size * 0.04 }
    }
}

/// A plain text face, for `\text{…}`.
#[derive(Clone)]
pub struct TextFont<'a> {
    face: Face<'a>,
    upem: f32,
    bbox_cache: Vec<OnceLock<Option<Rect>>>,
}

impl core::fmt::Debug for TextFont<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextFont")
            .field("upem", &self.upem)
            .finish()
    }
}

impl<'a> TextFont<'a> {
    /// Read a face from font data. No `MATH` table is needed.
    ///
    /// # Errors
    ///
    /// [`Error::BadFont`] if the data is not a readable face or has a zero `unitsPerEm`.
    pub fn new(data: &'a [u8]) -> Result<TextFont<'a>, Error> {
        let face = Face::parse(data, 0)?;
        let upem = f32::from(face.units_per_em());
        if upem <= 0.0 {
            return Err(Error::BadFont);
        }
        Ok(TextFont {
            bbox_cache: new_bbox_cache(&face),
            face,
            upem,
        })
    }

    #[must_use]
    pub fn face(&self) -> &Face<'a> {
        &self.face
    }

    #[must_use]
    pub fn units_per_em(&self) -> f32 {
        self.upem
    }

    #[must_use]
    pub fn px(&self, design_units: f32, size: f32) -> f32 {
        design_units * size / self.upem
    }

    #[must_use]
    pub fn glyph_for(&self, ch: char) -> Option<GlyphId> {
        self.face.glyph_index(ch)
    }

    #[must_use]
    pub fn advance(&self, glyph: GlyphId) -> f32 {
        f32::from(self.face.glyph_hor_advance(glyph).unwrap_or(0))
    }

    #[must_use]
    pub fn bbox(&self, glyph: GlyphId) -> Option<Rect> {
        cached_bbox(&self.bbox_cache, &self.face, glyph)
    }
}

/// The faces a layout draws from.
#[derive(Debug, Clone)]
pub struct Fonts<'a> {
    pub math: MathFont<'a>,
    /// Used for `\text{…}`; without one, the maths font's upright Latin is used.
    pub text: Option<TextFont<'a>>,
}

impl<'a> Fonts<'a> {
    /// A maths font on its own.
    ///
    /// # Errors
    ///
    /// As [`MathFont::new`].
    pub fn new(math: &'a [u8]) -> Result<Fonts<'a>, Error> {
        Ok(Fonts {
            math: MathFont::new(math)?,
            text: None,
        })
    }

    /// Add a face for `\text{…}`.
    #[must_use]
    pub fn with_text(mut self, text: TextFont<'a>) -> Fonts<'a> {
        self.text = Some(text);
        self
    }
}

#[cfg(feature = "embed-math-font")]
impl Fonts<'static> {
    /// New Computer Modern Math, embedded in the binary.
    ///
    /// # Errors
    ///
    /// As [`MathFont::new`].
    pub fn embedded() -> Result<Fonts<'static>, Error> {
        Fonts::new(crate::embed::MATH_FONT)
    }
}

// `cfg(test)` must be the outer attribute for clippy to allow `unwrap` in tests.
#[cfg(test)]
#[cfg(feature = "embed-math-font")]
mod tests {
    use super::*;

    fn font() -> MathFont<'static> {
        MathFont::new(crate::embed::MATH_FONT).unwrap()
    }

    #[test]
    fn garbage_is_not_a_font() {
        assert!(matches!(MathFont::new(&[]), Err(Error::BadFont)));
        assert!(matches!(MathFont::new(&[0u8; 16]), Err(Error::BadFont)));
        assert!(matches!(
            MathFont::from_collection(crate::embed::MATH_FONT, 7),
            Err(Error::BadFont)
        ));
    }

    #[test]
    fn the_load_bearing_constants_are_sane() {
        let f = font();
        let em = 100.0;
        let axis = f.axis_height(em);
        assert!((10.0..40.0).contains(&axis), "axis height {axis}");
        let rule = f.default_rule_thickness(em);
        assert!((1.0..15.0).contains(&rule), "rule thickness {rule}");
        let gap = f.k(f.constants().sub_superscript_gap_min(), em);
        assert!((1.0..40.0).contains(&gap), "sub/superscript gap {gap}");
    }

    #[test]
    fn conversion_is_linear_in_the_size() {
        let f = font();
        assert_eq!(f.px(f.units_per_em(), 16.0), 16.0);
        assert_eq!(f.px(f.units_per_em() / 2.0, 32.0), 16.0);
    }

    #[test]
    fn italic_correction_is_nonzero_only_where_it_should_be() {
        let f = font();
        let italic_f = f.glyph_for('\u{1D453}').unwrap_or(GlyphId(0));
        assert!(
            f.italic_correction(italic_f) > 0.0,
            "an italic f should overhang"
        );
        let zero = f.glyph_for('0').unwrap_or(GlyphId(0));
        assert_eq!(f.italic_correction(zero), 0.0);
    }

    #[test]
    fn glyphs_have_ink_and_spaces_do_not() {
        let f = font();
        let x = f.glyph_for('x').unwrap();
        assert!(f.advance(x) > 0.0);
        let bbox = f.bbox(x).expect("x has ink");
        assert!(bbox.y_max > 0);
        assert!(f.bbox(f.glyph_for(' ').unwrap()).is_none());
    }

    #[test]
    fn the_variants_table_is_present() {
        let f = font();
        let variants = f.variants().expect("a maths font has variants");
        let paren = f.glyph_for('(').unwrap();
        let construction = variants
            .vertical_constructions
            .get(paren)
            .expect("( is stretchy");
        assert!(construction.variants.len() > 1 || construction.assembly.is_some());
    }
}
