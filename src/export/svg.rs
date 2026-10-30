//! Rendering a [`Scene`] to SVG.
//!
//! [`to_svg`] writes a standalone document: every glyph is its own `<path>` in absolute
//! coordinates, with no `<defs>`, `<use>` or `transform`. [`SvgSprite`] is for a page that shows
//! many formulas at once (a list of search results, a table of problems): each glyph's outline is
//! written once, in font units, and every formula becomes a small `<svg>` of `<use>` references.

use core::fmt::Write as _;
use std::collections::HashMap;

use ttf_parser::{Face, OutlineBuilder};

use crate::error::Error;
use crate::font::Fonts;
use crate::scene::{FontKind, GlyphInstance, Scene};

/// The face and units-per-em for a glyph; the maths face stands in for a missing text face.
fn face_for<'a, 'b>(fonts: &'b Fonts<'a>, glyph: &GlyphInstance) -> (&'b Face<'a>, f32) {
    match (glyph.font, fonts.text.as_ref()) {
        (FontKind::Text, Some(text)) => (text.face(), text.units_per_em()),
        _ => (fonts.math.face(), fonts.math.units_per_em()),
    }
}

/// Options for [`to_svg`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct SvgOptions<'a> {
    /// Padding in pixels on all four sides of the formula's box.
    pub padding: f32,
    /// Decimal places for coordinates.
    pub precision: u8,
    /// The fill for glyphs and rules; `None` gives `currentColor`. XML-escaped on output.
    pub fill: Option<&'a str>,
    pub background: Option<&'a str>,
}

impl Default for SvgOptions<'_> {
    fn default() -> Self {
        SvgOptions {
            padding: 2.0,
            precision: 2,
            fill: None,
            background: None,
        }
    }
}

impl<'a> SvgOptions<'a> {
    #[must_use]
    pub fn with_padding(mut self, padding: f32) -> SvgOptions<'a> {
        self.padding = padding;
        self
    }

    #[must_use]
    pub fn with_precision(mut self, precision: u8) -> SvgOptions<'a> {
        self.precision = precision;
        self
    }

    #[must_use]
    pub fn with_fill(mut self, fill: Option<&'a str>) -> SvgOptions<'a> {
        self.fill = fill;
        self
    }

    #[must_use]
    pub fn with_background(mut self, background: Option<&'a str>) -> SvgOptions<'a> {
        self.background = background;
        self
    }
}

/// Render a laid-out formula as a standalone SVG document.
///
/// `fonts` must be the faces the scene was laid out against.
///
/// # Errors
///
/// [`Error::BadMetrics`] if the scene carries non-finite geometry.
pub fn to_svg(scene: &Scene, fonts: &Fonts<'_>, opts: &SvgOptions<'_>) -> Result<String, Error> {
    if !scene.is_finite() {
        return Err(Error::BadMetrics);
    }
    let pad = opts.padding;
    let precision = usize::from(opts.precision);
    let x = fmt(-pad, precision);
    let y = fmt(-scene.ascent - pad, precision);
    let w = fmt(scene.width + 2.0 * pad, precision);
    let h = fmt(scene.height() + 2.0 * pad, precision);

    let mut out = String::with_capacity(1024 + scene.glyphs.len() * 256);
    // Scene coordinates are SVG user space (both y-down); only outlines are flipped.
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{w}\" height=\"{h}\" viewBox=\"{x} {y} {w} {h}\">",
    );
    if let Some(background) = opts.background {
        let _ = write!(
            out,
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{}\"/>",
            attr(background),
        );
    }
    let _ = write!(
        out,
        "<g fill=\"{}\">",
        attr(opts.fill.unwrap_or("currentColor"))
    );

    for rule in scene.backgrounds.iter().chain(&scene.rules) {
        for (name, value) in [
            ("<rect x=\"", rule.x),
            ("\" y=\"", rule.y),
            ("\" width=\"", rule.width),
            ("\" height=\"", rule.height),
        ] {
            out.push_str(name);
            fmt_to(&mut out, value, precision);
        }
        out.push('"');
        paint_attr_to(&mut out, scene, rule.paint(), rule.alpha());
        out.push_str("/>");
    }

    for stroke in &scene.strokes {
        out.push_str("<path d=\"");
        for (index, [px, py]) in stroke.corners().into_iter().enumerate() {
            out.push(if index == 0 { 'M' } else { 'L' });
            fmt_to(&mut out, px, precision);
            out.push(' ');
            fmt_to(&mut out, py, precision);
        }
        out.push_str("Z\"");
        paint_attr_to(&mut out, scene, stroke.paint(), stroke.alpha());
        out.push_str("/>");
    }

    for glyph in &scene.glyphs {
        let (face, upem) = face_for(fonts, glyph);
        // Written in place; truncated back if the glyph has no outline.
        let element_start = out.len();
        out.push_str("<path d=\"");
        let data_start = out.len();
        let mut writer = PathWriter {
            out: &mut out,
            origin_x: glyph.x,
            origin_y: glyph.y,
            scale: glyph.size / upem,
            precision,
            pending: None,
        };
        let outlined = face.outline_glyph(glyph.glyph, &mut writer).is_some();
        if !outlined || out.len() == data_start {
            out.truncate(element_start);
            continue;
        }
        out.push('"');
        paint_attr_to(&mut out, scene, glyph.paint(), glyph.alpha());
        out.push_str("/>");
    }

    out.push_str("</g></svg>");
    Ok(out)
}

/// Options for [`SvgSprite`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SpriteOptions {
    /// Prefix for the glyph ids, so two sprites can share a document. Must be a valid id start.
    pub prefix: String,
    /// Padding in pixels on all four sides of each formula's box.
    pub padding: f32,
    /// Decimal places for coordinates.
    pub precision: u8,
    /// Decimal places for outline coordinates in the `<defs>`; these are font units, so `0`
    /// is exact for a TrueType-style font and `1` is plenty for the rest.
    pub outline_precision: u8,
    /// Emit `style="vertical-align:…"` so the fragment sits on the baseline of the text around
    /// it, as a KaTeX or MathJax inline formula does.
    pub inline: bool,
}

impl Default for SpriteOptions {
    fn default() -> Self {
        SpriteOptions {
            prefix: String::from("eel"),
            padding: 1.0,
            precision: 2,
            outline_precision: 1,
            inline: true,
        }
    }
}

impl SpriteOptions {
    #[must_use]
    pub fn with_prefix(mut self, prefix: &str) -> SpriteOptions {
        self.prefix = prefix.to_owned();
        self
    }

    #[must_use]
    pub fn with_padding(mut self, padding: f32) -> SpriteOptions {
        self.padding = padding;
        self
    }

    #[must_use]
    pub fn with_precision(mut self, precision: u8) -> SpriteOptions {
        self.precision = precision;
        self
    }

    #[must_use]
    pub fn with_outline_precision(mut self, precision: u8) -> SpriteOptions {
        self.outline_precision = precision;
        self
    }

    #[must_use]
    pub fn with_inline(mut self, inline: bool) -> SpriteOptions {
        self.inline = inline;
        self
    }
}

/// A glyph table shared by many formulas.
///
/// Every distinct glyph is written once, as a `<path>` in font units (y flipped to page space),
/// and each formula rendered through [`SvgSprite::fragment`] is an `<svg>` whose glyphs are
/// `<use>` references scaled to their size. The `<defs>` grow as new glyphs appear;
/// [`SvgSprite::take_defs`] hands back only what is new, so a page can append to one hidden
/// `<svg><defs>` as it goes instead of rebuilding it. The references are by id, so the defs and
/// the fragments must land in the same document.
///
/// ```ignore
/// let mut sprite = SvgSprite::new(SpriteOptions::default());
/// let a = sprite.fragment(&scene_a, &fonts)?;
/// let b = sprite.fragment(&scene_b, &fonts)?;
/// page.push_str(&sprite.take_defs());   // `<svg …><defs>…</defs></svg>`, once
/// page.push_str(&a);
/// page.push_str(&b);
/// ```
///
/// A hundred rows of inline maths is a few kilobytes of `<use>` elements on top of one table of
/// outlines, where a hundred standalone documents repeat every outline they touch.
#[derive(Debug)]
pub struct SvgSprite {
    opts: SpriteOptions,
    ids: HashMap<(FontKind, u16), u32>,
    /// Outlines not yet handed out by [`SvgSprite::take_defs`].
    pending: String,
    /// Every outline so far, for [`SvgSprite::defs`].
    all: String,
}

impl SvgSprite {
    #[must_use]
    pub fn new(opts: SpriteOptions) -> SvgSprite {
        SvgSprite {
            opts,
            ids: HashMap::new(),
            pending: String::new(),
            all: String::new(),
        }
    }

    /// How many distinct glyphs the table holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// One formula as an `<svg>` of `<use>` references into this sprite's table.
    ///
    /// `fonts` must be the faces the scene was laid out against, and the same faces for every
    /// call: a glyph id names an outline in a particular face.
    ///
    /// # Errors
    ///
    /// [`Error::BadMetrics`] if the scene carries non-finite geometry.
    pub fn fragment(&mut self, scene: &Scene, fonts: &Fonts<'_>) -> Result<String, Error> {
        if !scene.is_finite() {
            return Err(Error::BadMetrics);
        }
        let pad = self.opts.padding;
        let precision = usize::from(self.opts.precision);
        let x = fmt(-pad, precision);
        let y = fmt(-scene.ascent - pad, precision);
        let w = fmt(scene.width + 2.0 * pad, precision);
        let h = fmt(scene.height() + 2.0 * pad, precision);

        let mut out = String::with_capacity(160 + scene.glyphs.len() * 72);
        let _ = write!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"{x} {y} {w} {h}\"",
        );
        if self.opts.inline {
            // The box's bottom edge sits `descent + pad` below the baseline.
            out.push_str(" style=\"vertical-align:-");
            fmt_to(&mut out, scene.descent + pad, precision);
            out.push_str("px\"");
        }
        out.push_str("><g fill=\"currentColor\">");

        for rule in scene.backgrounds.iter().chain(&scene.rules) {
            for (name, value) in [
                ("<rect x=\"", rule.x),
                ("\" y=\"", rule.y),
                ("\" width=\"", rule.width),
                ("\" height=\"", rule.height),
            ] {
                out.push_str(name);
                fmt_to(&mut out, value, precision);
            }
            out.push('"');
            paint_attr_to(&mut out, scene, rule.paint(), rule.alpha());
            out.push_str("/>");
        }

        for stroke in &scene.strokes {
            out.push_str("<path d=\"");
            for (index, [px, py]) in stroke.corners().into_iter().enumerate() {
                out.push(if index == 0 { 'M' } else { 'L' });
                fmt_to(&mut out, px, precision);
                out.push(' ');
                fmt_to(&mut out, py, precision);
            }
            out.push_str("Z\"");
            paint_attr_to(&mut out, scene, stroke.paint(), stroke.alpha());
            out.push_str("/>");
        }

        for glyph in &scene.glyphs {
            let Some(id) = self.define(fonts, glyph) else {
                continue;
            };
            let (_, upem) = face_for(fonts, glyph);
            let _ = write!(
                out,
                "<use href=\"#{}{id}\" transform=\"translate(",
                self.opts.prefix
            );
            fmt_to(&mut out, glyph.x, precision);
            out.push(' ');
            fmt_to(&mut out, glyph.y, precision);
            out.push_str(") scale(");
            // Four places: at 20 px per em and 1000 units per em the step is 0.02 px.
            fmt_to(&mut out, glyph.size / upem, 5);
            out.push_str(")\"");
            paint_attr_to(&mut out, scene, glyph.paint(), glyph.alpha());
            out.push_str("/>");
        }

        out.push_str("</g></svg>");
        Ok(out)
    }

    /// The outline's id, defining it on first sight; `None` for a glyph with no outline.
    fn define(&mut self, fonts: &Fonts<'_>, glyph: &GlyphInstance) -> Option<u32> {
        let key = (glyph.font, glyph.glyph.0);
        if let Some(&id) = self.ids.get(&key) {
            return Some(id);
        }
        let (face, _) = face_for(fonts, glyph);
        let id = u32::try_from(self.ids.len()).ok()?;
        let mut path = String::with_capacity(256);
        let mut writer = PathWriter {
            out: &mut path,
            origin_x: 0.0,
            origin_y: 0.0,
            scale: 1.0,
            precision: usize::from(self.opts.outline_precision),
            pending: None,
        };
        let outlined = face.outline_glyph(glyph.glyph, &mut writer).is_some();
        if !outlined || path.is_empty() {
            return None;
        }
        let element = format!("<path id=\"{}{id}\" d=\"{path}\"/>", self.opts.prefix);
        self.pending.push_str(&element);
        self.all.push_str(&element);
        self.ids.insert(key, id);
        Some(id)
    }

    /// Outlines defined since the last call, wrapped as a zero-sized `<svg><defs>` block to
    /// append to the document; empty when there is nothing new.
    pub fn take_defs(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        let defs = wrap_defs(&self.pending);
        self.pending.clear();
        defs
    }

    /// Every outline so far, as one `<svg><defs>` block.
    #[must_use]
    pub fn defs(&self) -> String {
        if self.all.is_empty() {
            return String::new();
        }
        wrap_defs(&self.all)
    }
}

fn wrap_defs(paths: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"0\" height=\"0\" \
         style=\"position:absolute;width:0;height:0;overflow:hidden\" aria-hidden=\"true\">\
         <defs>{paths}</defs></svg>"
    )
}

/// Appends the `fill` and `opacity` an item needs, if any.
#[allow(unused_variables)]
fn paint_attr_to(out: &mut String, scene: &Scene, paint: crate::attrs::PaintId, alpha: f32) {
    #[cfg(feature = "attrs")]
    if let Some(colour) = scene.resolve(paint) {
        let [r, g, b, a] = colour.rgba;
        let _ = write!(out, " fill=\"#{r:02x}{g:02x}{b:02x}\"");
        if a != 255 {
            let _ = write!(out, " fill-opacity=\"{:.3}\"", f32::from(a) / 255.0);
        }
    }
    if alpha < 1.0 {
        // The item's own opacity, kept separate from `fill-opacity`.
        let _ = write!(out, " opacity=\"{:.3}\"", alpha.max(0.0));
    }
}

/// Turns a glyph outline into path data, flipping font-space y-up to page-space y-down.
struct PathWriter<'a> {
    out: &'a mut String,
    origin_x: f32,
    origin_y: f32,
    scale: f32,
    precision: usize,
    /// The last command letter written, so a run can drop repeats: `L1 2L3 4` becomes `L1 2 3 4`.
    pending: Option<char>,
}

impl PathWriter<'_> {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.origin_x + x * self.scale,
            self.origin_y - y * self.scale,
        )
    }

    fn command(&mut self, letter: char) {
        if self.pending != Some(letter) {
            self.out.push(letter);
            self.pending = Some(letter);
        } else {
            self.out.push(' ');
        }
    }

    fn coords(&mut self, points: &[(f32, f32)]) {
        for (i, &(x, y)) in points.iter().enumerate() {
            if i > 0 {
                self.out.push(' ');
            }
            fmt_to(self.out, x, self.precision);
            self.out.push(' ');
            fmt_to(self.out, y, self.precision);
        }
    }
}

impl OutlineBuilder for PathWriter<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.command('M');
        self.coords(&[p]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.command('L');
        self.coords(&[p]);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c = self.point(x1, y1);
        let p = self.point(x, y);
        self.command('Q');
        self.coords(&[c, p]);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let a = self.point(x1, y1);
        let b = self.point(x2, y2);
        let p = self.point(x, y);
        self.command('C');
        self.coords(&[a, b, p]);
    }

    fn close(&mut self) {
        self.out.push('Z');
        // `Z` ends the run, so the next command always names itself.
        self.pending = None;
    }
}

/// A caller-supplied string made safe as an XML attribute value.
fn attr(value: &str) -> String {
    // `&` first, so the entities it introduces are not escaped again.
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// A number, rounded, with trailing zeros stripped and `-0` normalised.
fn fmt(value: f32, precision: usize) -> String {
    let mut s = String::new();
    fmt_to(&mut s, value, precision);
    s
}

fn fmt_to(out: &mut String, value: f32, precision: usize) {
    let start = out.len();
    let _ = write!(out, "{value:.precision$}");
    if out.get(start..).is_some_and(|s| s.contains('.')) {
        while out.ends_with('0') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
    }
    if out.get(start..).is_some_and(|s| s.is_empty() || s == "-0") {
        out.truncate(start);
        out.push('0');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "embed-math-font")]
    #[test]
    fn sprite_shares_outlines_between_formulas() {
        let fonts = Fonts::embedded().expect("embedded font");
        let opts = crate::Options::default().with_font_size(16.0);
        let mut sprite = SvgSprite::new(SpriteOptions::default());
        let a = sprite
            .fragment(&crate::render("x^2", &fonts, &opts).expect("x^2"), &fonts)
            .expect("fragment");
        let n = sprite.len();
        assert!(a.contains("<use href=\"#eel0\""), "{a}");
        assert!(a.contains("vertical-align:-"), "{a}");
        assert!(
            !a.contains("<path d="),
            "glyph outlines belong in the defs: {a}"
        );
        let b = sprite
            .fragment(&crate::render("2x", &fonts, &opts).expect("2x"), &fonts)
            .expect("fragment");
        assert_eq!(sprite.len(), n, "2x adds no new glyphs after x^2: {b}");
        let defs = sprite.take_defs();
        assert_eq!(defs.matches("<path id=\"eel").count(), n);
        assert!(
            sprite.take_defs().is_empty(),
            "nothing new since the last take"
        );
        assert_eq!(sprite.defs().matches("<path id=").count(), n);
    }

    #[test]
    fn numbers_are_trimmed_and_zero_is_unsigned() {
        assert_eq!(fmt(1.0, 2), "1");
        assert_eq!(fmt(1.5, 2), "1.5");
        assert_eq!(fmt(1.239, 2), "1.24");
        assert_eq!(fmt(-0.001, 2), "0");
        assert_eq!(fmt(-0.0, 2), "0");
        assert_eq!(fmt(-2.25, 2), "-2.25");
    }
}
