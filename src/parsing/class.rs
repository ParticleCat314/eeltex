//! TeX atom classes and the inter-atom spacing they drive.

/// TeX's eight atom classes (TeXbook, chapter 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomClass {
    /// An ordinary symbol: a variable, a digit, `\infty`.
    Ord,
    /// A large operator: `\sum`, `\int`, `\lim`, `\sin`.
    Op,
    /// A binary operation: `+`, `\times`.
    Bin,
    /// A relation: `=`, `\leq`, and every arrow.
    Rel,
    /// An opening delimiter: `(`, `\lfloor`.
    Open,
    /// A closing delimiter: `)`, `\rceil`.
    Close,
    /// Punctuation: `,`, `;`.
    Punct,
    /// A sub-formula treated as a unit: a fraction, a matrix, a braced group.
    Inner,
}

/// One space width, in math units (1 mu = 1/18 em of the current style's size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Space {
    mu: u8,
    suppressed: bool,
}

const fn space(mu: u8, suppressed: bool) -> Space {
    Space { mu, suppressed }
}

const NONE: Space = space(0, false);
const THIN: Space = space(3, false);
// `_S` widths are dropped in script styles.
const THIN_S: Space = space(3, true);
const MED_S: Space = space(4, true);
const THICK_S: Space = space(5, true);

/// The TeXbook's chapter 18 table. Rows are the left atom, columns the right.
#[rustfmt::skip]
static SPACING: [[Space; 8]; 8] = [
    /* Ord   */ [NONE,    THIN,    MED_S, THICK_S, NONE,    NONE,   NONE,   THIN_S ],
    /* Op    */ [THIN,    THIN,    NONE,  THICK_S, NONE,    NONE,   NONE,   THIN_S ],
    /* Bin   */ [MED_S,   MED_S,   NONE,  NONE,    MED_S,   NONE,   NONE,   MED_S  ],
    /* Rel   */ [THICK_S, THICK_S, NONE,  NONE,    THICK_S, NONE,   NONE,   THICK_S],
    /* Open  */ [NONE,    NONE,    NONE,  NONE,    NONE,    NONE,   NONE,   NONE   ],
    /* Close */ [NONE,    THIN,    MED_S, THICK_S, NONE,    NONE,   NONE,   THIN_S ],
    /* Punct */ [THIN_S,  THIN_S,  NONE,  THIN_S,  THIN_S,  THIN_S, THIN_S, THIN_S ],
    /* Inner */ [THIN_S,  THIN,    MED_S, THICK_S, THIN_S,  NONE,   THIN_S, THIN_S ],
];

#[must_use]
pub(crate) fn spacing_mu(left: AtomClass, right: AtomClass, script_style: bool) -> f32 {
    SPACING
        .get(left as usize)
        .and_then(|row| row.get(right as usize))
        .filter(|space| !(space.suppressed && script_style))
        .map_or(0.0, |space| f32::from(space.mu))
}

/// Rewrite binary operators that are not, in context, binary.
/// TeXbook rules 5 and 17: a leading `-` is a sign, not a subtraction.
pub(crate) fn demote_binaries(classes: &mut [AtomClass]) {
    use AtomClass::{Bin, Close, Op, Open, Ord, Punct, Rel};

    for i in 0..classes.len() {
        if classes.get(i) != Some(&Bin) {
            continue;
        }
        let prev = i.checked_sub(1).and_then(|p| classes.get(p));
        if matches!(prev, None | Some(Bin | Op | Rel | Open | Punct)) {
            if let Some(slot) = classes.get_mut(i) {
                *slot = Ord;
            }
        }
    }

    for i in 1..classes.len() {
        if matches!(classes.get(i), Some(Rel | Close | Punct)) && classes.get(i - 1) == Some(&Bin) {
            if let Some(slot) = classes.get_mut(i - 1) {
                *slot = Ord;
            }
        }
    }
}

/// The character maths mode actually draws for a source character.
#[must_use]
pub(crate) fn math_char(c: char) -> char {
    match c {
        '-' => '\u{2212}',
        // U+2217 is the axis-centred operator asterisk.
        '*' => '\u{2217}',
        _ => c,
    }
}

#[must_use]
pub(crate) fn class_of_char(c: char) -> AtomClass {
    match c {
        '+' | '-' | '*' | '\u{00B1}' | '\u{00D7}' | '\u{00F7}' | '\u{2212}' | '\u{2213}' => {
            AtomClass::Bin
        }
        '=' | '<' | '>' | ':' | '\u{2264}' | '\u{2265}' | '\u{2260}' | '\u{2248}' | '\u{2261}'
        | '\u{2208}' | '\u{2192}' | '\u{2190}' => AtomClass::Rel,
        '(' | '[' => AtomClass::Open,
        ')' | ']' => AtomClass::Close,
        ',' | ';' => AtomClass::Punct,
        // `/` and `|` are `\mathord` in TeX.
        _ => AtomClass::Ord,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AtomClass::{Bin, Close, Inner, Op, Ord, Punct, Rel};

    #[test]
    fn a_leading_minus_is_demoted() {
        let mut c = [Bin, Ord];
        demote_binaries(&mut c);
        assert_eq!(c, [Ord, Ord]);
    }

    #[test]
    fn a_minus_after_a_relation_is_demoted() {
        // x = -y
        let mut c = [Ord, Rel, Bin, Ord];
        demote_binaries(&mut c);
        assert_eq!(c, [Ord, Rel, Ord, Ord]);
    }

    #[test]
    fn a_real_binary_survives() {
        let mut c = [Ord, Bin, Ord];
        demote_binaries(&mut c);
        assert_eq!(c, [Ord, Bin, Ord]);
    }

    #[test]
    fn a_binary_before_a_closer_is_demoted() {
        let mut c = [Ord, Bin, Close];
        demote_binaries(&mut c);
        assert_eq!(c, [Ord, Ord, Close]);
    }

    #[test]
    fn script_styles_drop_the_suppressed_gaps() {
        assert_eq!(spacing_mu(Ord, Bin, false), 4.0);
        assert_eq!(spacing_mu(Ord, Bin, true), 0.0);
        assert_eq!(spacing_mu(Ord, Op, false), 3.0);
        assert_eq!(spacing_mu(Ord, Op, true), 3.0);
    }

    #[test]
    fn openers_never_add_space() {
        for right in [Ord, Op, Bin, Rel, Inner, Punct] {
            assert_eq!(spacing_mu(AtomClass::Open, right, false), 0.0, "{right:?}");
        }
    }
}
