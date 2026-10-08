//! Reading a colour specification out of a `\color{…}` argument.

use crate::attrs::Paint;

/// Named colours, sorted for binary search.
#[rustfmt::skip]
static NAMED: &[(&str, [u8; 3])] = &[
    ("black",     [0x00, 0x00, 0x00]),
    ("blue",      [0x00, 0x00, 0xFF]),
    ("brown",     [0xBF, 0x80, 0x40]),
    ("cyan",      [0x00, 0xFF, 0xFF]),
    ("darkgray",  [0x40, 0x40, 0x40]),
    ("gray",      [0x80, 0x80, 0x80]),
    ("green",     [0x00, 0xFF, 0x00]),
    ("lightgray", [0xBF, 0xBF, 0xBF]),
    ("lime",      [0xBF, 0xFF, 0x00]),
    ("magenta",   [0xFF, 0x00, 0xFF]),
    ("olive",     [0x80, 0x80, 0x00]),
    ("orange",    [0xFF, 0x80, 0x00]),
    ("pink",      [0xFF, 0xBF, 0xBF]),
    ("purple",    [0xBF, 0x00, 0x40]),
    ("red",       [0xFF, 0x00, 0x00]),
    ("teal",      [0x00, 0x80, 0x80]),
    ("violet",    [0x80, 0x00, 0x80]),
    ("white",     [0xFF, 0xFF, 0xFF]),
    ("yellow",    [0xFF, 0xFF, 0x00]),
];

/// Parse a name, `#rgb`, `#rrggbb`, `#rrggbbaa` or `rgb(r, g, b)`; `None` if unrecognised.
#[must_use]
pub(crate) fn parse(spec: &str) -> Option<Paint> {
    let spec = spec.trim();
    if let Some(hex) = spec.strip_prefix('#') {
        return from_hex(hex);
    }
    if let Some(rest) = spec.strip_prefix("rgb(") {
        return from_rgb(rest.strip_suffix(')')?);
    }
    // Case-insensitive: `dvipsnames` spells it `\color{Red}`.
    let lower = spec.to_ascii_lowercase();
    let index = NAMED.binary_search_by_key(&lower.as_str(), |&(name, _)| name);
    let &(_, [r, g, b]) = NAMED.get(index.ok()?)?;
    Some(Paint::rgb(r, g, b))
}

fn from_hex(hex: &str) -> Option<Paint> {
    let bytes = hex.as_bytes();
    let nibble =
        |i: usize| -> Option<u8> { u8::try_from(char::from(*bytes.get(i)?).to_digit(16)?).ok() };
    let byte = |i: usize| -> Option<u8> { Some(nibble(i * 2)? << 4 | nibble(i * 2 + 1)?) };
    match hex.len() {
        // `#abc` is `#aabbcc`.
        3 => Some(Paint::rgb(
            nibble(0)? * 0x11,
            nibble(1)? * 0x11,
            nibble(2)? * 0x11,
        )),
        6 => Some(Paint::rgb(byte(0)?, byte(1)?, byte(2)?)),
        8 => Some(Paint::rgba(byte(0)?, byte(1)?, byte(2)?, byte(3)?)),
        _ => None,
    }
}

fn from_rgb(body: &str) -> Option<Paint> {
    let mut parts = body.split(',');
    let r = component(parts.next()?)?;
    let g = component(parts.next()?)?;
    let b = component(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some(Paint::rgb(r, g, b))
}

/// One channel, `0..=255` or `0.0..=1.0`; a bare `1` means 1/255.
fn component(text: &str) -> Option<u8> {
    let text = text.trim();
    if text.contains('.') {
        let value: f32 = text.parse().ok()?;
        if !(0.0..=1.0).contains(&value) {
            return None;
        }
        return Some((value * 255.0).round() as u8);
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_so_the_search_works() {
        assert!(NAMED.windows(2).all(|w| match w {
            [(a, _), (b, _)] => a < b,
            _ => true,
        }));
        for (name, _) in NAMED {
            assert!(parse(name).is_some(), "{name} is not findable");
        }
    }

    #[test]
    fn names_are_case_insensitive() {
        assert_eq!(parse("red"), parse("Red"));
        assert_eq!(parse("  blue  "), Some(Paint::rgb(0, 0, 255)));
    }

    #[test]
    fn short_hex_doubles_its_nibbles() {
        assert_eq!(parse("#fff"), Some(Paint::rgb(255, 255, 255)));
        assert_eq!(parse("#f80"), Some(Paint::rgb(0xFF, 0x88, 0x00)));
        assert_eq!(parse("#FF8800"), Some(Paint::rgb(0xFF, 0x88, 0x00)));
        assert_eq!(
            parse("#ff880040"),
            Some(Paint::rgba(0xFF, 0x88, 0x00, 0x40))
        );
    }

    #[test]
    fn rgb_takes_either_scale() {
        assert_eq!(parse("rgb(255,128,0)"), Some(Paint::rgb(255, 128, 0)));
        assert_eq!(parse("rgb(1.0, 0.0, 0.0)"), Some(Paint::rgb(255, 0, 0)));
    }

    #[test]
    fn nonsense_is_not_quietly_black() {
        for spec in [
            "",
            "chartreuse",
            "#ff",
            "#gggggg",
            "rgb(1,2)",
            "rgb(1,2,3,4)",
            "rgb(300,0,0)",
            "rgb(2.0,0,0)",
            "red!50!blue",
        ] {
            assert_eq!(parse(spec), None, "{spec:?} was accepted");
        }
    }
}
