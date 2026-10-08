//! Parser-side command registries.

use crate::ast::{Accent, Alphabet, ArraySpec, BigOpKind, ColumnAlign, EnvKind};
use crate::class::AtomClass;

/// Upright function names and whether they take limits above and below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MathFunction {
    Ordinary(&'static str),
    Limit(&'static str),
}

#[rustfmt::skip]
const ORDINARY: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan",
    "sinh", "cosh", "tanh", "coth", "ln", "log", "lg", "exp", "arg", "hom",
    "sgn", "bmod", "mod", "deg", "dim", "ker",
    // The Russian and French spellings.
    "sh", "ch", "th", "tg", "ctg", "cotg", "cth", "arctg", "arcctg", "cosec",
];

const LIMIT: &[&str] = &["max", "min", "det", "gcd", "sup", "inf", "Pr"];

pub(crate) fn function(name: &str) -> Option<MathFunction> {
    let find = |names: &[&'static str]| names.iter().copied().find(|n| *n == name);
    find(ORDINARY)
        .map(MathFunction::Ordinary)
        .or_else(|| find(LIMIT).map(MathFunction::Limit))
}

/// `\varGamma` and friends: the capital Greek letters, italic.
pub(crate) fn var_greek(name: &str) -> Option<char> {
    Some(match name {
        "varGamma" => '\u{0393}',
        "varDelta" => '\u{0394}',
        "varTheta" => '\u{0398}',
        "varLambda" => '\u{039B}',
        "varXi" => '\u{039E}',
        "varPi" => '\u{03A0}',
        "varSigma" => '\u{03A3}',
        "varUpsilon" => '\u{03A5}',
        "varPhi" => '\u{03A6}',
        "varPsi" => '\u{03A8}',
        "varOmega" => '\u{03A9}',
        _ => return None,
    })
}

/// Inverse of [`var_greek`].
pub(crate) fn var_greek_name(ch: char) -> Option<&'static str> {
    Some(match ch {
        '\u{0393}' => "varGamma",
        '\u{0394}' => "varDelta",
        '\u{0398}' => "varTheta",
        '\u{039B}' => "varLambda",
        '\u{039E}' => "varXi",
        '\u{03A0}' => "varPi",
        '\u{03A3}' => "varSigma",
        '\u{03A5}' => "varUpsilon",
        '\u{03A6}' => "varPhi",
        '\u{03A8}' => "varPsi",
        '\u{03A9}' => "varOmega",
        _ => return None,
    })
}

pub(crate) fn alphabet(name: &str) -> Option<Alphabet> {
    Some(match name {
        "mathbf" | "boldsymbol" | "bm" | "pmb" => Alphabet::Bold,
        "mathit" => Alphabet::Italic,
        "mathbfit" => Alphabet::BoldItalic,
        "mathrm" | "operatorname" => Alphabet::Upright,
        "mathsf" => Alphabet::SansSerif,
        "mathbfsf" | "mathsfbf" => Alphabet::SansSerifBold,
        "mathsfit" => Alphabet::SansSerifItalic,
        "mathtt" => Alphabet::Monospace,
        // One script alphabet in Unicode, so calligraphic and script are the same letters.
        "mathcal" | "mathscr" => Alphabet::Script,
        "mathbfscr" | "mathbfcal" => Alphabet::BoldScript,
        "mathfrak" => Alphabet::Fraktur,
        "mathbffrak" => Alphabet::BoldFraktur,
        "mathbb" | "Bbb" => Alphabet::DoubleStruck,
        _ => return None,
    })
}

/// Text-mode commands and the alphabet their prose is set in; `Upright` is plain `\text`.
pub(crate) fn text(name: &str) -> Option<Alphabet> {
    Some(match name {
        "text" | "textnormal" | "textrm" | "textup" | "textmd" | "mbox" | "hbox" => {
            Alphabet::Upright
        }
        "textbf" => Alphabet::Bold,
        "textit" | "emph" | "textsl" => Alphabet::Italic,
        "textsf" => Alphabet::SansSerif,
        "texttt" => Alphabet::Monospace,
        _ => return None,
    })
}

/// The command that sets prose in `alphabet`, if one does.
pub(crate) fn text_name(alphabet: Alphabet) -> Option<&'static str> {
    Some(match alphabet {
        Alphabet::Bold => "textbf",
        Alphabet::Italic => "textit",
        Alphabet::SansSerif => "textsf",
        Alphabet::Monospace => "texttt",
        _ => return None,
    })
}

pub(crate) fn class(name: &str) -> Option<AtomClass> {
    Some(match name {
        "mathord" => AtomClass::Ord,
        "mathbin" => AtomClass::Bin,
        "mathrel" => AtomClass::Rel,
        "mathopen" => AtomClass::Open,
        "mathclose" => AtomClass::Close,
        "mathpunct" => AtomClass::Punct,
        "mathinner" => AtomClass::Inner,
        // `\mathop` sets the class but does not take limits.
        "mathop" => AtomClass::Op,
        _ => return None,
    })
}

pub(crate) fn accent(name: &str) -> Option<Accent> {
    Some(match name {
        "hat" => Accent::Hat,
        "widehat" => Accent::WideHat,
        "tilde" => Accent::Tilde,
        "widetilde" => Accent::WideTilde,
        "bar" => Accent::Bar,
        "vec" => Accent::Vec,
        "dot" => Accent::Dot,
        "ddot" => Accent::DDot,
        "dddot" => Accent::DDDot,
        "ddddot" => Accent::DDDDot,
        "acute" => Accent::Acute,
        "grave" => Accent::Grave,
        "breve" => Accent::Breve,
        "check" => Accent::Check,
        "mathring" => Accent::Ring,
        "overline" => Accent::Overline,
        "underline" => Accent::Underline,
        "overbrace" => Accent::OverBrace,
        "underbrace" => Accent::UnderBrace,
        "overbracket" => Accent::OverBracket,
        "underbracket" => Accent::UnderBracket,
        "overrightarrow" => Accent::OverRightArrow,
        "overleftarrow" => Accent::OverLeftArrow,
        "overleftrightarrow" => Accent::OverLeftRightArrow,
        "underrightarrow" => Accent::UnderRightArrow,
        "underleftarrow" => Accent::UnderLeftArrow,
        "underleftrightarrow" => Accent::UnderLeftRightArrow,
        "overleftharpoon" => Accent::OverLeftHarpoon,
        "overrightharpoon" => Accent::OverRightHarpoon,
        "Overrightarrow" => Accent::OverRightDoubleArrow,
        "overgroup" => Accent::OverGroup,
        "undergroup" => Accent::UnderGroup,
        "overlinesegment" => Accent::OverLineSegment,
        "underlinesegment" => Accent::UnderLineSegment,
        "widecheck" => Accent::WideCheck,
        "utilde" => Accent::UnderTilde,
        "underbar" => Accent::Underline,
        // The text-mode accents, read the same way in maths.
        "'" => Accent::Acute,
        "`" => Accent::Grave,
        "\"" => Accent::DDot,
        "~" => Accent::Tilde,
        "=" => Accent::Bar,
        "." => Accent::Dot,
        "u" => Accent::Breve,
        "v" => Accent::Check,
        "H" => Accent::DoubleAcute,
        "r" => Accent::Ring,
        _ => return None,
    })
}

pub(crate) fn environment(name: &str) -> Option<EnvKind> {
    Some(match name {
        // `alignat`'s `{n}` pair count is dropped by `parse_env`.
        "align" | "align*" | "aligned" | "split" | "equation" | "equation*" | "displaymath"
        | "alignat" | "alignat*" | "alignedat" => EnvKind::Aligned,
        // Rows are centred, not aligned, so `gather` needs its own variant.
        "gather" | "gather*" | "gathered" => EnvKind::Gathered,
        // `smallmatrix` is a matrix the parser wraps in a script-size style.
        "matrix" | "smallmatrix" => EnvKind::Matrix { delims: None },
        "pmatrix" => EnvKind::Matrix {
            delims: Some(('(', ')')),
        },
        "bmatrix" => EnvKind::Matrix {
            delims: Some(('[', ']')),
        },
        "Bmatrix" => EnvKind::Matrix {
            delims: Some(('{', '}')),
        },
        "vmatrix" => EnvKind::Matrix {
            delims: Some(('|', '|')),
        },
        "Vmatrix" => EnvKind::Matrix {
            delims: Some(('\u{2016}', '\u{2016}')),
        },
        "cases" => EnvKind::Cases,
        "dcases" => EnvKind::DCases,
        "rcases" => EnvKind::RCases,
        "drcases" => EnvKind::DRCases,
        // Its first and last lines pushed to the margins in LaTeX; there are no margins here.
        "multline" | "multline*" => EnvKind::Gathered,
        "matrix*" => EnvKind::AlignedMatrix {
            delims: None,
            align: ColumnAlign::Centre,
        },
        "pmatrix*" => EnvKind::AlignedMatrix {
            delims: Some(('(', ')')),
            align: ColumnAlign::Centre,
        },
        "bmatrix*" => EnvKind::AlignedMatrix {
            delims: Some(('[', ']')),
            align: ColumnAlign::Centre,
        },
        "Bmatrix*" => EnvKind::AlignedMatrix {
            delims: Some(('{', '}')),
            align: ColumnAlign::Centre,
        },
        "vmatrix*" => EnvKind::AlignedMatrix {
            delims: Some(('|', '|')),
            align: ColumnAlign::Centre,
        },
        "Vmatrix*" => EnvKind::AlignedMatrix {
            delims: Some(('\u{2016}', '\u{2016}')),
            align: ColumnAlign::Centre,
        },
        // The spec is read by the parser, which fills these in.
        "array" | "darray" => EnvKind::Array(ArraySpec::parse("")?),
        "subarray" => EnvKind::SubArray(ColumnAlign::Centre),
        _ => return None,
    })
}

pub(crate) fn xarrow(name: &str) -> Option<char> {
    Some(match name {
        "xrightarrow" => '\u{2192}',
        "xleftarrow" => '\u{2190}',
        "xleftrightarrow" => '\u{2194}',
        "xRightarrow" => '\u{21D2}',
        "xLeftarrow" => '\u{21D0}',
        "xLeftrightarrow" => '\u{21D4}',
        "xmapsto" => '\u{21A6}',
        "xhookrightarrow" => '\u{21AA}',
        "xhookleftarrow" => '\u{21A9}',
        "xtwoheadrightarrow" => '\u{21A0}',
        "xrightharpoonup" => '\u{21C0}',
        "xleftharpoondown" => '\u{21BD}',
        "xrightharpoondown" => '\u{21C1}',
        "xleftharpoonup" => '\u{21BC}',
        "xrightleftharpoons" => '\u{21CC}',
        "xleftrightharpoons" => '\u{21CB}',
        "xtwoheadleftarrow" => '\u{219E}',
        "xtofrom" => '\u{21C4}',
        "xequal" | "xlongequal" => '=',
        _ => return None,
    })
}

pub(crate) fn bigop(name: &str) -> Option<BigOpKind> {
    Some(match name {
        "sum" => BigOpKind::Sum,
        "prod" => BigOpKind::Product,
        "coprod" => BigOpKind::Coproduct,
        "int" => BigOpKind::Integral,
        "iint" => BigOpKind::DoubleIntegral,
        "iiint" => BigOpKind::TripleIntegral,
        "oint" => BigOpKind::ContourIntegral,
        "iiiint" => BigOpKind::QuadrupleIntegral,
        "oiint" => BigOpKind::SurfaceIntegral,
        "oiiint" => BigOpKind::VolumeIntegral,
        "ointclockwise" => BigOpKind::ClockwiseContourIntegral,
        "ointctrclockwise" => BigOpKind::AnticlockwiseContourIntegral,
        "bigcup" => BigOpKind::Union,
        "bigcap" => BigOpKind::Intersection,
        "bigvee" => BigOpKind::Disjunction,
        "bigwedge" => BigOpKind::Conjunction,
        "bigoplus" => BigOpKind::DirectSum,
        "bigotimes" => BigOpKind::TensorProduct,
        "bigodot" => BigOpKind::CircledDot,
        "bigsqcup" => BigOpKind::SquareUnion,
        "biguplus" => BigOpKind::MultisetUnion,
        "lim" => BigOpKind::Limit("lim"),
        "limsup" => BigOpKind::Limit("lim sup"),
        "liminf" => BigOpKind::Limit("lim inf"),
        "injlim" => BigOpKind::Limit("inj lim"),
        "projlim" => BigOpKind::Limit("proj lim"),
        "varliminf" => BigOpKind::VarLimInf,
        "varlimsup" => BigOpKind::VarLimSup,
        "varinjlim" => BigOpKind::VarInjLim,
        "varprojlim" => BigOpKind::VarProjLim,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_keep_their_limit_behaviour() {
        assert_eq!(function("sin"), Some(MathFunction::Ordinary("sin")));
        assert_eq!(function("max"), Some(MathFunction::Limit("max")));
        assert_eq!(function("unknown"), None);
    }

    #[test]
    fn matrix_variants_keep_their_delimiters() {
        assert_eq!(
            environment("matrix"),
            Some(EnvKind::Matrix { delims: None })
        );
        assert_eq!(
            environment("pmatrix"),
            Some(EnvKind::Matrix {
                delims: Some(('(', ')')),
            })
        );
        assert_eq!(environment("cases"), Some(EnvKind::Cases));
    }

    #[test]
    fn extensible_arrows_stay_centralised() {
        assert_eq!(xarrow("xrightarrow"), Some('\u{2192}'));
        assert_eq!(xarrow("xequal"), Some('='));
        assert_eq!(xarrow("xfoo"), None);
    }
}
