//! The parsed form of a maths expression.
//! Font-independent: a codepoint and an atom class, never a glyph.

use crate::attrs::{Paint, Span};
use crate::class::AtomClass;

/// How an [`Expr::Styled`] wrapper changes the drawing of what it contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Decor {
    pub colour: Option<Paint>,
}

impl Decor {
    #[must_use]
    pub fn colour(colour: Paint) -> Decor {
        Decor {
            colour: Some(colour),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Expr {
    /// One maths character, with the class that drives its spacing.
    Sym {
        ch: char,
        class: AtomClass,
        alphabet: Alphabet,
    },
    /// Juxtaposition. Inter-atom spacing is computed across this list.
    Seq(Vec<Expr>),
    /// A base with at most one subscript and one superscript; `\sum_a^b` is an [`Expr::BigOp`].
    Scripts {
        base: Box<Expr>,
        sub: Option<Box<Expr>>,
        sup: Option<Box<Expr>>,
    },
    Frac {
        num: Box<Expr>,
        den: Box<Expr>,
        style: FracStyle,
        /// `false` for `\binom`; orthogonal to [`FracStyle`] (`\dbinom` is display and barless).
        bar: bool,
    },
    Sqrt {
        degree: Option<Box<Expr>>,
        radicand: Box<Expr>,
    },
    /// A braced group; it forces the ordinary class: `{+}` is ordinary, a bare `+` binary.
    Group(Box<Expr>),
    Delimited {
        left: Delim,
        body: Box<Expr>,
        right: Delim,
    },
    /// `\sin` and friends: upright, class [`AtomClass::Op`], no limits.
    Func(&'static str),
    BigOp {
        kind: BigOpKind,
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        limits: Limits,
    },
    /// `\mathbb{…}` and friends: sets the alphabet of every symbol beneath without its own.
    Alphabet(Alphabet, Box<Expr>),
    Accent {
        kind: Accent,
        base: Box<Expr>,
    },
    /// `\text{…}`: prose, set upright, with its spaces intact.
    Text(String),
    /// Rows of cells.
    Env(EnvKind, Vec<Vec<Expr>>),
    /// Explicit spacing, in mu (1/18 em). Signed.
    Space(i16),
    /// Colour only, layout unaffected: `\color`, `\textcolor{…}{…}`.
    Styled {
        decor: Decor,
        body: Box<Expr>,
    },
    /// An explicitly stated atom class: `\mathbin{\star}`.
    Class(AtomClass, Box<Expr>),
    /// `\phantom{…}` and its two halves: laid out, measured, not drawn.
    Phantom {
        body: Box<Expr>,
        /// `\vphantom` clears this: it contributes height and no width.
        width: bool,
        /// `\hphantom` clears this: it contributes width and no height.
        height: bool,
    },
    /// One thing set over or under another: `\overset`, `\underset`, `\stackrel`, brace labels.
    Stack {
        over: Option<Box<Expr>>,
        base: Box<Expr>,
        under: Option<Box<Expr>>,
        /// `\stackrel` forces a relation; `None` takes the base's own class.
        class: Option<AtomClass>,
    },
    /// `\xrightarrow[under]{over}`: an arrow stretched to fit its labels.
    XArrow {
        ch: char,
        over: Option<Box<Expr>>,
        under: Option<Box<Expr>>,
    },
    /// `\displaystyle` etc.: a style switch, scoped by the parser to the rest of the group.
    Style {
        style: StyleSwitch,
        body: Box<Expr>,
    },
    /// `\bigl(`, `\Big/`, …: a delimiter at a fixed, hand-picked size.
    SizedDelim {
        ch: char,
        class: AtomClass,
        size: DelimSize,
    },
    /// `\rule[raise]{width}{height}`, all in ems: a filled box. Zero wide, it is `\strut`.
    Rule {
        width: f32,
        height: f32,
        /// How far the bottom edge sits above the baseline; negative hangs below it.
        raise: f32,
    },
    /// Drawn as usual, but measured or placed differently: `\smash`, `\rlap`, `\raisebox`.
    Adjust {
        adjust: Adjust,
        body: Box<Expr>,
    },
    /// Something drawn around or across the body: `\boxed`, `\colorbox`, `\cancel`.
    Enclose {
        kind: Enclose,
        body: Box<Expr>,
    },
    /// `\hline` or `\hdashline`: a rule across an environment, above the row it begins.
    HLine {
        dashed: bool,
    },
    /// `\middle|`: a delimiter inside `\left…\right`, grown to the same size as the pair.
    Middle(Delim),
    /// A source range tag; emitted only when [`ParseOptions::spans`](crate::ParseOptions) is on.
    Marked {
        span: Span,
        inner: Box<Expr>,
    },
}

/// How an [`Expr::Adjust`] changes the box of its body.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Adjust {
    /// `\smash`: no height (`top`), no depth (`bottom`), or neither (both set).
    Smash { top: bool, bottom: bool },
    /// `\llap`, `\rlap`, `\clap`: no width, the body hanging off one side or centred.
    Lap(Lap),
    /// `\raisebox`: moved up by this many ems, its box with it.
    Raise(f32),
}

/// Which way a [`Adjust::Lap`] body hangs from the point it is set at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lap {
    /// `\llap`: ending at the point.
    Left,
    /// `\rlap`: starting at the point.
    Right,
    /// `\clap`: centred on the point.
    Centre,
}

/// What an [`Expr::Enclose`] draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Enclose {
    /// `\boxed`, `\fbox`: a frame.
    Frame,
    /// `\colorbox`: a fill behind the body. Drawn only with `attrs`, which carries colour.
    Fill(Paint),
    /// `\fcolorbox`: a fill, and a frame in its own colour.
    FramedFill { frame: Paint, fill: Paint },
    /// `\cancel`: a stroke rising left to right.
    Cancel,
    /// `\bcancel`: a stroke falling left to right.
    BackCancel,
    /// `\xcancel`: both.
    CrossCancel,
    /// `\cancelto`'s arrow; the value it points to is the superscript.
    CancelTo,
    /// `\sout`: a horizontal stroke through the middle.
    StrikeOut,
}

/// The style a [`Expr::Style`] switch selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StyleSwitch {
    Display,
    Text,
    Script,
    ScriptScript,
    /// `\tiny` through `\Huge`: scales the font, leaving the maths style alone.
    Size(SizeSwitch),
}

/// The ten LaTeX font sizes, relative to `\normalsize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeSwitch {
    Tiny,
    ScriptSize,
    FootnoteSize,
    Small,
    NormalSize,
    Large,
    /// `\Large`
    Larger,
    /// `\LARGE`
    Largest,
    Huge,
    /// `\Huge`
    Huger,
}

impl SizeSwitch {
    /// The font size, as a multiple of the size the formula was laid out at.
    #[must_use]
    pub fn factor(self) -> f32 {
        match self {
            SizeSwitch::Tiny => 0.5,
            SizeSwitch::ScriptSize => 0.7,
            SizeSwitch::FootnoteSize => 0.8,
            SizeSwitch::Small => 0.9,
            SizeSwitch::NormalSize => 1.0,
            SizeSwitch::Large => 1.2,
            SizeSwitch::Larger => 1.44,
            SizeSwitch::Largest => 1.728,
            SizeSwitch::Huge => 2.074,
            SizeSwitch::Huger => 2.488,
        }
    }

    /// The command that selects this size.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            SizeSwitch::Tiny => "tiny",
            SizeSwitch::ScriptSize => "scriptsize",
            SizeSwitch::FootnoteSize => "footnotesize",
            SizeSwitch::Small => "small",
            SizeSwitch::NormalSize => "normalsize",
            SizeSwitch::Large => "large",
            SizeSwitch::Larger => "Large",
            SizeSwitch::Largest => "LARGE",
            SizeSwitch::Huge => "huge",
            SizeSwitch::Huger => "Huge",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<SizeSwitch> {
        Some(match name {
            "tiny" => SizeSwitch::Tiny,
            "scriptsize" => SizeSwitch::ScriptSize,
            "footnotesize" => SizeSwitch::FootnoteSize,
            "small" => SizeSwitch::Small,
            "normalsize" => SizeSwitch::NormalSize,
            "large" => SizeSwitch::Large,
            "Large" => SizeSwitch::Larger,
            "LARGE" => SizeSwitch::Largest,
            "huge" => SizeSwitch::Huge,
            "Huge" => SizeSwitch::Huger,
            _ => return None,
        })
    }
}

/// The four hand-picked delimiter sizes, `\big` through `\Bigg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DelimSize {
    Big,
    BigCap,
    Bigg,
    BiggCap,
}

impl DelimSize {
    /// The total height this size asks for, in em.
    #[must_use]
    pub fn em(self) -> f32 {
        match self {
            DelimSize::Big => 1.2,
            DelimSize::BigCap => 1.8,
            DelimSize::Bigg => 2.4,
            DelimSize::BiggCap => 3.0,
        }
    }
}

impl Expr {
    /// The atom class this expression contributes to its neighbours' spacing.
    #[must_use]
    pub fn class(&self) -> AtomClass {
        match self {
            Expr::Sym { class, .. } | Expr::SizedDelim { class, .. } | Expr::Class(class, _) => {
                *class
            }
            Expr::Middle(_) | Expr::Rule { .. } | Expr::Enclose { .. } | Expr::HLine { .. } => {
                AtomClass::Ord
            }
            Expr::Func(_) | Expr::BigOp { .. } => AtomClass::Op,
            Expr::Frac { .. } | Expr::Delimited { .. } | Expr::Env(..) => AtomClass::Inner,
            // TeX makes a radical an ordinary, not an inner.
            // An explicit kern is transparent to spacing; see `layout::seq`.
            Expr::Group(_)
            | Expr::Sqrt { .. }
            | Expr::Accent { .. }
            | Expr::Text(_)
            | Expr::Space(_) => AtomClass::Ord,
            // Scripts do not change the class of the base.
            Expr::Alphabet(_, inner)
            | Expr::Marked { inner, .. }
            | Expr::Phantom { body: inner, .. }
            | Expr::Styled { body: inner, .. }
            | Expr::Style { body: inner, .. }
            | Expr::Adjust { body: inner, .. }
            | Expr::Scripts { base: inner, .. } => inner.class(),
            Expr::Stack { class, base, .. } => class.unwrap_or_else(|| base.class()),
            Expr::XArrow { .. } => AtomClass::Rel,
            Expr::Seq(items) => match items.as_slice() {
                [single] => single.class(),
                _ => AtomClass::Inner,
            },
        }
    }

    /// The class of the first atom in this expression, looking inside a run.
    #[must_use]
    pub fn leading_class(&self) -> AtomClass {
        match self {
            Expr::Seq(items) => items
                .iter()
                .find(|e| !e.is_transparent_kern())
                .map_or(AtomClass::Ord, Expr::leading_class),
            Expr::Marked { inner, .. } | Expr::Styled { body: inner, .. } => inner.leading_class(),
            other => other.class(),
        }
    }

    /// Whether this is an explicit kern, seeing through a span wrapper.
    pub(crate) fn is_transparent_kern(&self) -> bool {
        match self {
            // A table rule takes no room in the cell that carries it.
            Expr::Space(_) | Expr::HLine { .. } => true,
            Expr::Marked { inner, .. } | Expr::Styled { body: inner, .. } => {
                inner.is_transparent_kern()
            }
            _ => false,
        }
    }

    /// The class of the last atom in this expression. See [`Expr::leading_class`].
    #[must_use]
    pub fn trailing_class(&self) -> AtomClass {
        match self {
            Expr::Seq(items) => items
                .iter()
                .rev()
                .find(|e| !e.is_transparent_kern())
                .map_or(AtomClass::Ord, Expr::trailing_class),
            Expr::Marked { inner, .. } | Expr::Styled { body: inner, .. } => inner.trailing_class(),
            other => other.class(),
        }
    }

    /// Whether this carries no ink.
    pub(crate) fn is_blank(&self) -> bool {
        match self {
            Expr::Seq(items) => items.iter().all(Expr::is_blank),
            Expr::Marked { inner, .. }
            | Expr::Styled { body: inner, .. }
            | Expr::Style { body: inner, .. } => inner.is_blank(),
            Expr::Space(_) => true,
            _ => false,
        }
    }
}

/// Which style a fraction forces on itself before laying its parts out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FracStyle {
    /// `\frac`: inherit the enclosing style.
    Auto,
    /// `\dfrac`: display, however deeply nested.
    Display,
    /// `\tfrac`: text, even in a displayed equation.
    Text,
    /// `\cfrac`: display, with a strut in the numerator so a continued fraction's levels line up.
    Continued,
}

/// Whether a big operator sets its scripts above and below, or beside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limits {
    /// The conventional choice for the operator and the current style.
    Auto,
    /// `\limits`.
    Always,
    /// `\nolimits`.
    Never,
}

/// A delimiter, as a codepoint so the font's `MATH` variants can be looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Delim {
    Char(char),
    /// `\left.` — a delimiter that occupies no space.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvKind {
    /// `align`/`aligned`/`split`: cells alternate right- and left-aligned.
    Aligned,
    /// `gather`/`gathered`: one cell per row, centred.
    Gathered,
    /// The `matrix` family. `delims` is the stretchy pair, `None` for bare.
    Matrix { delims: Option<(char, char)> },
    /// `cases`: a stretched `{` with left-aligned cells.
    Cases,
    /// `\substack{…}`: one centred column at script size, tightly set.
    SubStack,
    /// `array` and `darray`: columns aligned and ruled as the spec says.
    Array(ArraySpec),
    /// `pmatrix*` and the rest of the starred family: every column aligned one way.
    AlignedMatrix {
        delims: Option<(char, char)>,
        align: ColumnAlign,
    },
    /// `subarray`: one column at script size, tightly set, aligned one way.
    SubArray(ColumnAlign),
    /// `dcases`: `cases` with its cells in display style.
    DCases,
    /// `rcases`: the brace on the right.
    RCases,
    /// `drcases`: both.
    DRCases,
}

/// How a column sets its cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnAlign {
    Left,
    #[default]
    Centre,
    Right,
}

impl ColumnAlign {
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            ColumnAlign::Left => 'l',
            ColumnAlign::Centre => 'c',
            ColumnAlign::Right => 'r',
        }
    }

    #[must_use]
    pub fn from_letter(letter: char) -> Option<ColumnAlign> {
        Some(match letter {
            'l' => ColumnAlign::Left,
            'c' => ColumnAlign::Centre,
            'r' => ColumnAlign::Right,
            _ => return None,
        })
    }
}

/// An `array` column spec such as `{l|cc:r}`: alignments, and the rules between columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ArraySpec {
    align: [ColumnAlign; ArraySpec::MAX_COLUMNS],
    columns: u8,
    /// Bit `i`: a rule before column `i`; bit `columns` is after the last.
    rules: u64,
    /// Of those, the dashed ones (`:`).
    dashed: u64,
    /// `darray`: cells in display style.
    pub display: bool,
}

impl ArraySpec {
    pub const MAX_COLUMNS: usize = 32;

    /// Read a spec, `l|c:r`. `p{…}`-style widths are not supported; spaces are ignored.
    #[must_use]
    pub fn parse(spec: &str) -> Option<ArraySpec> {
        let mut out = ArraySpec {
            align: [ColumnAlign::Centre; ArraySpec::MAX_COLUMNS],
            columns: 0,
            rules: 0,
            dashed: 0,
            display: false,
        };
        for letter in spec.chars() {
            let at = 1u64 << out.columns;
            match letter {
                ' ' | '\t' | '\n' => {}
                '|' => out.rules |= at,
                ':' => {
                    out.rules |= at;
                    out.dashed |= at;
                }
                letter => {
                    *out.align.get_mut(usize::from(out.columns))? =
                        ColumnAlign::from_letter(letter)?;
                    out.columns += 1;
                }
            }
        }
        Some(out)
    }

    #[must_use]
    pub fn columns(&self) -> usize {
        usize::from(self.columns)
    }

    /// Column `index`'s alignment; centred past the end of the spec.
    #[must_use]
    pub fn align(&self, index: usize) -> ColumnAlign {
        // Slots past `columns` are never written, so they are still centred.
        self.align.get(index).copied().unwrap_or_default()
    }

    /// The rule before column `index`, if any: `Some(true)` for a dashed one.
    #[must_use]
    pub fn rule(&self, index: usize) -> Option<bool> {
        let bit = 1u64.checked_shl(u32::try_from(index).ok()?)?;
        (self.rules & bit != 0).then_some(self.dashed & bit != 0)
    }

    #[must_use]
    pub fn to_spec(&self) -> String {
        let mut out = String::new();
        for index in 0..=self.columns() {
            match self.rule(index) {
                Some(true) => out.push(':'),
                Some(false) => out.push('|'),
                None => {}
            }
            if index < self.columns() {
                out.push(self.align(index).letter());
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BigOpKind {
    Sum,
    Product,
    Coproduct,
    Integral,
    DoubleIntegral,
    TripleIntegral,
    ContourIntegral,
    Union,
    Intersection,
    Disjunction,
    Conjunction,
    DirectSum,
    TensorProduct,
    CircledDot,
    SquareUnion,
    MultisetUnion,
    QuadrupleIntegral,
    SurfaceIntegral,
    VolumeIntegral,
    ClockwiseContourIntegral,
    AnticlockwiseContourIntegral,
    /// `\varliminf`: `lim` with a rule under it.
    VarLimInf,
    /// `\varlimsup`: `lim` with a rule over it.
    VarLimSup,
    /// `\varinjlim`: `lim` over a right arrow.
    VarInjLim,
    /// `\varprojlim`: `lim` over a left arrow.
    VarProjLim,
    /// `\lim`, `\max`, `\sup` and friends: words that take limits like an operator.
    Limit(&'static str),
}

impl BigOpKind {
    /// The codepoint that draws this operator; `None` for the upright words.
    #[must_use]
    pub fn char(self) -> Option<char> {
        Some(match self {
            BigOpKind::Sum => '\u{2211}',
            BigOpKind::Product => '\u{220F}',
            BigOpKind::Coproduct => '\u{2210}',
            BigOpKind::Integral => '\u{222B}',
            BigOpKind::DoubleIntegral => '\u{222C}',
            BigOpKind::TripleIntegral => '\u{222D}',
            BigOpKind::ContourIntegral => '\u{222E}',
            BigOpKind::Union => '\u{22C3}',
            BigOpKind::Intersection => '\u{22C2}',
            BigOpKind::Disjunction => '\u{22C1}',
            BigOpKind::Conjunction => '\u{22C0}',
            BigOpKind::DirectSum => '\u{2A01}',
            BigOpKind::TensorProduct => '\u{2A02}',
            BigOpKind::CircledDot => '\u{2A00}',
            BigOpKind::SquareUnion => '\u{2A06}',
            BigOpKind::MultisetUnion => '\u{2A04}',
            BigOpKind::QuadrupleIntegral => '\u{2A0C}',
            BigOpKind::SurfaceIntegral => '\u{222F}',
            BigOpKind::VolumeIntegral => '\u{2230}',
            BigOpKind::ClockwiseContourIntegral => '\u{2232}',
            BigOpKind::AnticlockwiseContourIntegral => '\u{2233}',
            BigOpKind::Limit(_)
            | BigOpKind::VarLimInf
            | BigOpKind::VarLimSup
            | BigOpKind::VarInjLim
            | BigOpKind::VarProjLim => return None,
        })
    }

    /// Whether this operator sets its scripts beside it even in display style.
    #[must_use]
    pub fn scripts_by_default(self) -> bool {
        matches!(
            self,
            BigOpKind::Integral
                | BigOpKind::DoubleIntegral
                | BigOpKind::TripleIntegral
                | BigOpKind::QuadrupleIntegral
                | BigOpKind::ContourIntegral
                | BigOpKind::SurfaceIntegral
                | BigOpKind::VolumeIntegral
                | BigOpKind::ClockwiseContourIntegral
                | BigOpKind::AnticlockwiseContourIntegral
        )
    }
}

/// A styled alphabet, drawn from the Mathematical Alphanumeric Symbols block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Alphabet {
    /// Italic letters, upright digits.
    Auto,
    Upright,
    Bold,
    Italic,
    BoldItalic,
    Script,
    BoldScript,
    Fraktur,
    BoldFraktur,
    DoubleStruck,
    SansSerif,
    SansSerifBold,
    SansSerifItalic,
    Monospace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Accent {
    Hat,
    WideHat,
    Tilde,
    WideTilde,
    Bar,
    Vec,
    Dot,
    DDot,
    DDDot,
    DDDDot,
    Acute,
    Grave,
    Breve,
    Check,
    Ring,
    /// `\H`: the Hungarian double acute.
    DoubleAcute,
    WideCheck,
    /// `\utilde`: a wide tilde under the base.
    UnderTilde,
    /// A rule above the base, not a glyph — it stretches to any width.
    Overline,
    Underline,
    /// `\not`: a `/` overlaid on the base, keeping its class, so `\not=` still spaces as a relation.
    Not,
    OverBrace,
    UnderBrace,
    OverBracket,
    UnderBracket,
    OverRightArrow,
    OverLeftArrow,
    OverLeftRightArrow,
    UnderRightArrow,
    UnderLeftArrow,
    UnderLeftRightArrow,
    OverLeftHarpoon,
    OverRightHarpoon,
    /// `\Overrightarrow`: a stretched double arrow.
    OverRightDoubleArrow,
    OverGroup,
    UnderGroup,
    /// A rule with a tick down at each end, drawn rather than taken from the font.
    OverLineSegment,
    UnderLineSegment,
}

impl Accent {
    /// Whether this accent sits under its base rather than over it.
    #[must_use]
    pub fn is_under(self) -> bool {
        matches!(
            self,
            Accent::Underline
                | Accent::UnderBrace
                | Accent::UnderBracket
                | Accent::UnderRightArrow
                | Accent::UnderLeftArrow
                | Accent::UnderLeftRightArrow
                | Accent::UnderTilde
                | Accent::UnderGroup
                | Accent::UnderLineSegment
        )
    }

    /// Whether this accent stretches to the width of what it covers.
    #[must_use]
    pub fn is_wide(self) -> bool {
        matches!(
            self,
            Accent::WideHat
                | Accent::WideTilde
                | Accent::WideCheck
                | Accent::UnderTilde
                | Accent::UnderLeftRightArrow
                | Accent::OverLeftHarpoon
                | Accent::OverRightHarpoon
                | Accent::OverRightDoubleArrow
                | Accent::OverGroup
                | Accent::UnderGroup
                | Accent::OverBrace
                | Accent::UnderBrace
                | Accent::OverBracket
                | Accent::UnderBracket
                | Accent::OverRightArrow
                | Accent::OverLeftArrow
                | Accent::OverLeftRightArrow
                | Accent::UnderRightArrow
                | Accent::UnderLeftArrow
        )
    }

    /// Whether a script after this accent is set above or below it (braces and brackets only).
    #[must_use]
    pub fn takes_limits(self) -> bool {
        matches!(
            self,
            Accent::OverBrace | Accent::UnderBrace | Accent::OverBracket | Accent::UnderBracket
        )
    }

    /// Whether this accent clears the base rather than nestling into it.
    #[must_use]
    pub fn clears_base(self) -> bool {
        self.is_wide()
            && !matches!(
                self,
                Accent::WideHat | Accent::WideTilde | Accent::WideCheck
            )
    }

    /// The combining codepoint for this accent; `None` for the rule-drawn kinds.
    #[must_use]
    pub fn char(self) -> Option<char> {
        Some(match self {
            Accent::Hat | Accent::WideHat => '\u{0302}',
            Accent::Tilde | Accent::WideTilde => '\u{0303}',
            Accent::Bar => '\u{0304}',
            Accent::Vec => '\u{20D7}',
            Accent::Dot => '\u{0307}',
            Accent::DDot => '\u{0308}',
            Accent::DDDot => '\u{20DB}',
            Accent::DDDDot => '\u{20DC}',
            Accent::Acute => '\u{0301}',
            Accent::Grave => '\u{0300}',
            Accent::Breve => '\u{0306}',
            Accent::Check | Accent::WideCheck => '\u{030C}',
            Accent::Ring => '\u{030A}',
            Accent::DoubleAcute => '\u{030B}',
            Accent::UnderTilde => '\u{0330}',
            Accent::UnderLeftRightArrow => '\u{2194}',
            Accent::OverLeftHarpoon => '\u{20D0}',
            Accent::OverRightHarpoon => '\u{20D1}',
            Accent::OverRightDoubleArrow => '\u{21D2}',
            Accent::OverGroup => '\u{23E0}',
            Accent::UnderGroup => '\u{23E1}',
            Accent::Not => '\u{0338}',
            Accent::OverBrace => '\u{23DE}',
            Accent::UnderBrace => '\u{23DF}',
            Accent::OverBracket => '\u{23B4}',
            Accent::UnderBracket => '\u{23B5}',
            Accent::OverRightArrow | Accent::UnderRightArrow => '\u{2192}',
            Accent::OverLeftArrow | Accent::UnderLeftArrow => '\u{2190}',
            Accent::OverLeftRightArrow => '\u{2194}',
            Accent::Overline
            | Accent::Underline
            | Accent::OverLineSegment
            | Accent::UnderLineSegment => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_is_ordinary_however_binary_its_contents() {
        let plus = Expr::Sym {
            ch: '+',
            class: AtomClass::Bin,
            alphabet: Alphabet::Auto,
        };
        assert_eq!(plus.class(), AtomClass::Bin);
        assert_eq!(Expr::Group(Box::new(plus)).class(), AtomClass::Ord);
    }

    #[test]
    fn scripts_keep_the_base_class() {
        let e = Expr::Scripts {
            base: Box::new(Expr::BigOp {
                kind: BigOpKind::Sum,
                lower: None,
                upper: None,
                limits: Limits::Auto,
            }),
            sub: None,
            sup: None,
        };
        assert_eq!(e.class(), AtomClass::Op);
    }

    #[test]
    fn integrals_take_scripts_and_sums_take_limits() {
        assert!(BigOpKind::Integral.scripts_by_default());
        assert!(BigOpKind::ContourIntegral.scripts_by_default());
        assert!(!BigOpKind::Sum.scripts_by_default());
        assert!(!BigOpKind::Limit("lim").scripts_by_default());
    }
}
