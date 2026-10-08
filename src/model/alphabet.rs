//! Mapping letters into the Mathematical Alphanumeric Symbols block.
//! Styling is a codepoint substitution, not a font switch; [`hole`] covers the letters
//! Unicode had already encoded in Letterlike Symbols.

use crate::ast::Alphabet;

/// Where a styled alphabet's runs begin.
struct Block {
    upper: u32,
    lower: u32,
    /// `None` where Unicode encodes no styled digits.
    digit: Option<u32>,
    /// The Greek capital run, and the small run 26 codepoints later.
    greek: Option<u32>,
}

const fn block(upper: u32, lower: u32, digit: Option<u32>, greek: Option<u32>) -> Block {
    Block {
        upper,
        lower,
        digit,
        greek,
    }
}

fn block_of(alphabet: Alphabet) -> Option<Block> {
    Some(match alphabet {
        // Handled by the caller: no substitution at all.
        Alphabet::Auto | Alphabet::Upright => return None,
        Alphabet::Bold => block(0x1D400, 0x1D41A, Some(0x1D7CE), Some(0x1D6A8)),
        Alphabet::Italic => block(0x1D434, 0x1D44E, None, Some(0x1D6E2)),
        Alphabet::BoldItalic => block(0x1D468, 0x1D482, None, Some(0x1D71C)),
        Alphabet::Script => block(0x1D49C, 0x1D4B6, None, None),
        Alphabet::BoldScript => block(0x1D4D0, 0x1D4EA, None, None),
        Alphabet::Fraktur => block(0x1D504, 0x1D51E, None, None),
        Alphabet::DoubleStruck => block(0x1D538, 0x1D552, Some(0x1D7D8), None),
        Alphabet::BoldFraktur => block(0x1D56C, 0x1D586, None, None),
        Alphabet::SansSerif => block(0x1D5A0, 0x1D5BA, Some(0x1D7E2), None),
        Alphabet::SansSerifBold => block(0x1D5D4, 0x1D5EE, Some(0x1D7EC), Some(0x1D756)),
        Alphabet::SansSerifItalic => block(0x1D608, 0x1D622, None, None),
        Alphabet::Monospace => block(0x1D670, 0x1D68A, Some(0x1D7F6), None),
    })
}

/// Letters pre-encoded in Letterlike Symbols, left as holes in the styled ranges.
fn hole(alphabet: Alphabet, ch: char) -> Option<char> {
    Some(match (alphabet, ch) {
        (Alphabet::Italic | Alphabet::Auto, 'h') => '\u{210E}',
        (Alphabet::Script, 'B') => '\u{212C}',
        (Alphabet::Script, 'E') => '\u{2130}',
        (Alphabet::Script, 'F') => '\u{2131}',
        (Alphabet::Script, 'H') => '\u{210B}',
        (Alphabet::Script, 'I') => '\u{2110}',
        (Alphabet::Script, 'L') => '\u{2112}',
        (Alphabet::Script, 'M') => '\u{2133}',
        (Alphabet::Script, 'R') => '\u{211B}',
        (Alphabet::Script, 'e') => '\u{212F}',
        (Alphabet::Script, 'g') => '\u{210A}',
        (Alphabet::Script, 'o') => '\u{2134}',
        (Alphabet::Fraktur, 'C') => '\u{212D}',
        (Alphabet::Fraktur, 'H') => '\u{210C}',
        (Alphabet::Fraktur, 'I') => '\u{2111}',
        (Alphabet::Fraktur, 'R') => '\u{211C}',
        (Alphabet::Fraktur, 'Z') => '\u{2128}',
        (Alphabet::DoubleStruck, 'C') => '\u{2102}',
        (Alphabet::DoubleStruck, 'H') => '\u{210D}',
        (Alphabet::DoubleStruck, 'N') => '\u{2115}',
        (Alphabet::DoubleStruck, 'P') => '\u{2119}',
        (Alphabet::DoubleStruck, 'Q') => '\u{211A}',
        (Alphabet::DoubleStruck, 'R') => '\u{211D}',
        (Alphabet::DoubleStruck, 'Z') => '\u{2124}',
        _ => return None,
    })
}

/// Offsets of the Greek variant forms, nabla and partial within a styled Greek block.
fn greek_variant_offset(ch: char) -> Option<u32> {
    Some(match ch {
        // nabla sits just after the capitals, partial just after the smalls.
        '\u{2207}' => 0x19,
        '\u{2202}' => 0x33,
        '\u{03F5}' => 0x34, // lunate epsilon
        '\u{03D1}' => 0x35, // script theta
        '\u{03F0}' => 0x36, // script kappa
        '\u{03D5}' => 0x37, // straight phi
        '\u{03F1}' => 0x38, // script rho
        '\u{03D6}' => 0x39, // script pi
        _ => return None,
    })
}

/// U+03A2 is reserved in both source and target, so plain offsets line up.
fn greek_offset(ch: char) -> Option<u32> {
    let c = ch as u32;
    match c {
        0x391..=0x3A9 => Some(c - 0x391),
        0x3B1..=0x3C9 => Some(c - 0x3B1 + 0x1A),
        _ => greek_variant_offset(ch),
    }
}

/// The codepoint that draws `ch` in `alphabet`; `ch` itself where the style has no form.
#[must_use]
pub(crate) fn remap(ch: char, alphabet: Alphabet) -> char {
    if alphabet == Alphabet::Upright {
        return ch;
    }
    // Auto: letters italic, digits upright.
    if alphabet == Alphabet::Auto {
        return match ch {
            // TeX convention: lowercase Greek italic, uppercase upright.
            'a'..='z'
            | 'A'..='Z'
            | '\u{03B1}'..='\u{03C9}'
            | '\u{03D1}'
            | '\u{03D5}'
            | '\u{03D6}'
            | '\u{03F0}'
            | '\u{03F1}'
            | '\u{03F5}' => remap(ch, Alphabet::Italic),
            _ => ch,
        };
    }

    if let Some(replacement) = hole(alphabet, ch) {
        return replacement;
    }
    let Some(block) = block_of(alphabet) else {
        return ch;
    };
    let base = match ch {
        'A'..='Z' => block.upper + (ch as u32 - 'A' as u32),
        'a'..='z' => block.lower + (ch as u32 - 'a' as u32),
        '0'..='9' => match block.digit {
            Some(d) => d + (ch as u32 - '0' as u32),
            None => return ch,
        },
        _ => match (block.greek, greek_offset(ch)) {
            (Some(g), Some(offset)) => g + offset,
            _ => return ch,
        },
    };
    char::from_u32(base).unwrap_or(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_italicises_letters_and_leaves_digits_alone() {
        assert_eq!(remap('x', Alphabet::Auto), '\u{1D465}');
        assert_eq!(remap('2', Alphabet::Auto), '2');
    }

    #[test]
    fn auto_italicises_lowercase_greek_only() {
        assert_eq!(remap('\u{03B1}', Alphabet::Auto), '\u{1D6FC}');
        assert_eq!(remap('\u{0393}', Alphabet::Auto), '\u{0393}');
    }

    #[test]
    fn the_letterlike_holes_are_covered() {
        for (alphabet, ch, want) in [
            (Alphabet::DoubleStruck, 'R', '\u{211D}'),
            (Alphabet::DoubleStruck, 'C', '\u{2102}'),
            (Alphabet::DoubleStruck, 'Z', '\u{2124}'),
            (Alphabet::Fraktur, 'R', '\u{211C}'),
            (Alphabet::Fraktur, 'I', '\u{2111}'),
            (Alphabet::Script, 'L', '\u{2112}'),
            (Alphabet::Script, 'e', '\u{212F}'),
            (Alphabet::Italic, 'h', '\u{210E}'),
        ] {
            assert_eq!(remap(ch, alphabet), want, "{alphabet:?} {ch}");
        }
    }

    #[test]
    fn the_contiguous_runs_line_up() {
        assert_eq!(remap('A', Alphabet::Bold), '\u{1D400}');
        assert_eq!(remap('z', Alphabet::Bold), '\u{1D433}');
        assert_eq!(remap('0', Alphabet::DoubleStruck), '\u{1D7D8}');
        assert_eq!(remap('A', Alphabet::Monospace), '\u{1D670}');
        assert_eq!(remap('a', Alphabet::SansSerifItalic), '\u{1D622}');
    }

    #[test]
    fn greek_runs_survive_the_reserved_slot() {
        // Capital sigma is four letters past the reserved slot.
        assert_eq!(remap('\u{03A3}', Alphabet::Italic), '\u{1D6F4}');
        assert_eq!(remap('\u{03A9}', Alphabet::Italic), '\u{1D6FA}');
        assert_eq!(remap('\u{03C9}', Alphabet::Bold), '\u{1D6DA}');
    }

    #[test]
    fn unstyled_characters_pass_through() {
        assert_eq!(remap('7', Alphabet::Script), '7');
        assert_eq!(remap('\u{03B1}', Alphabet::Fraktur), '\u{03B1}');
        assert_eq!(remap('+', Alphabet::Bold), '+');
        assert_eq!(remap('x', Alphabet::Upright), 'x');
    }
}
