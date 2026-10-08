#![forbid(unsafe_code)]

#[cfg(all(feature = "atlas", not(any(feature = "raster", feature = "sdf"))))]
compile_error!("the `atlas` feature needs `raster` or `sdf` to have something to pack");

#[cfg(feature = "edit")]
pub mod edit;
pub mod export;
mod layout;
mod model;
mod parsing;

#[cfg(feature = "embed-math-font")]
pub(crate) use crate::model::embed;
pub(crate) use crate::model::{alphabet, ast, error, font, scene, style, tex};
pub(crate) use crate::parsing::{class, colour, commands, parser, symbols, tokenizer};

pub use crate::model::attrs;
pub use crate::parsing::partial;
#[cfg(feature = "syntax")]
pub use crate::parsing::syntax;

pub use crate::ast::{
    Accent, Adjust, Alphabet, ArraySpec, BigOpKind, ColumnAlign, Decor, Delim, DelimSize, Enclose,
    EnvKind, Expr, FracStyle, Lap, Limits, SizeSwitch, StyleSwitch,
};
#[cfg(feature = "attrs")]
pub use crate::attrs::ItemAttrs;
pub use crate::attrs::{Node, NodeId, NodeKind, Paint, PaintId, Span};
pub use crate::class::AtomClass;
pub use crate::error::Error;
pub use crate::font::{Fonts, MathFont, TextFont};
pub use crate::scene::{FontKind, GlyphInstance, Rule, Scene, Stroke};
pub use crate::style::MathStyle;
pub use crate::tex::{to_tex, write_tex};

/// Re-exported so consumers use the same `ttf_parser` version as this crate.
pub use ttf_parser::{self, GlyphId};

/// Parse maths-mode LaTeX.
///
/// # Errors
///
/// [`Error::Parse`] for unsupported input, with the byte offset; [`Error::TooDeep`] past the
/// recursion limit.
pub fn parse(src: &str) -> Result<Expr, Error> {
    parse_with(src, &ParseOptions::default())
}

/// [`parse`] with options.
///
/// # Errors
///
/// As [`parse`].
pub fn parse_with(src: &str, opts: &ParseOptions) -> Result<Expr, Error> {
    let tokens = tokenizer::Tokenizer::new(src).tokenize();
    parser::Parser::new(src, tokens)
        .with_spans(opts.spans)
        .parse()
}

/// Options for [`parse_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ParseOptions {
    /// Tag every atom with its source range, as [`Expr::Marked`].
    pub spans: bool,
}

impl ParseOptions {
    #[must_use]
    pub fn with_spans(mut self, spans: bool) -> ParseOptions {
        self.spans = spans;
        self
    }
}

/// Lay a parsed expression out against a font.
///
/// # Errors
///
/// [`Error::MissingGlyph`] under [`MissingGlyph::Error`]; [`Error::BadMetrics`] if the font
/// yields non-finite geometry.
pub fn layout(expr: &Expr, fonts: &Fonts<'_>, opts: &Options) -> Result<Scene, Error> {
    layout::layout(expr, fonts, opts)
}

/// [`parse`] then [`layout`].
///
/// ```
/// # #[cfg(feature = "embed-math-font")] {
/// let fonts = eeltex::Fonts::embedded()?;
/// let options = eeltex::Options::default().with_font_size(32.0);
/// let scene = eeltex::render(r"\frac{a}{b} + x^2", &fonts, &options)?;
/// assert!(scene.width > 0.0 && scene.ascent > 0.0);
/// # }
/// # Ok::<(), eeltex::Error>(())
/// ```
///
/// # Errors
///
/// Anything either step reports.
pub fn render(src: &str, fonts: &Fonts<'_>, opts: &Options) -> Result<Scene, Error> {
    layout(&parse(src)?, fonts, opts)
}

/// Options for [`layout`] and [`render`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Options {
    /// Pixels per em for the outermost style.
    pub font_size: f32,
    /// [`MathStyle::Display`] for a centred equation, [`MathStyle::Text`] for inline.
    pub style: MathStyle,
    pub on_missing_glyph: MissingGlyph,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            font_size: 16.0,
            style: MathStyle::Display,
            on_missing_glyph: MissingGlyph::Notdef,
        }
    }
}

impl Options {
    #[must_use]
    pub fn with_font_size(mut self, font_size: f32) -> Options {
        self.font_size = font_size;
        self
    }

    #[must_use]
    pub fn with_style(mut self, style: MathStyle) -> Options {
        self.style = style;
        self
    }

    #[must_use]
    pub fn with_missing_glyph(mut self, policy: MissingGlyph) -> Options {
        self.on_missing_glyph = policy;
        self
    }
}

/// What to do about a character the font cannot draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MissingGlyph {
    /// Emit `.notdef`, a visible box (the default).
    Notdef,
    /// Emit nothing, and no advance.
    Skip,
    /// Fail the whole layout.
    Error,
}
