//! The one error type.

/// Everything that can go wrong parsing or laying out maths.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The source is not maths this crate models.
    Parse { message: String, offset: usize },
    /// Nested deeper than the parser's fixed recursion limit.
    TooDeep,
    /// The font data is not a face this crate can read.
    BadFont,
    /// The face has no `MATH` table.
    NoMathTable,
    /// The `MATH` table has no `MathConstants` sub-table.
    NoMathConstants,
    /// A character had no glyph and [`MissingGlyph::Error`](crate::MissingGlyph) was selected.
    MissingGlyph(char),
    /// Layout produced a non-finite coordinate, e.g. from a zero `unitsPerEm`.
    BadMetrics,
}

impl Error {
    pub(crate) fn parse(message: impl Into<String>, offset: usize) -> Error {
        Error::Parse {
            message: message.into(),
            offset,
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Parse { message, offset } => write!(f, "{message} (at byte {offset})"),
            Error::TooDeep => f.write_str("expression nested too deeply"),
            Error::BadFont => f.write_str("not a readable font"),
            Error::NoMathTable => f.write_str("the font has no MATH table"),
            Error::NoMathConstants => f.write_str("the font's MATH table has no constants"),
            Error::MissingGlyph(c) => write!(f, "the font has no glyph for {c:?}"),
            Error::BadMetrics => f.write_str("the font produced non-finite metrics"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ttf_parser::FaceParsingError> for Error {
    fn from(_: ttf_parser::FaceParsingError) -> Error {
        Error::BadFont
    }
}
