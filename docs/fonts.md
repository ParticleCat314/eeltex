# Fonts

[← README](../README.md)

Bring your own font bytes. `MathFont::new` rejects a face without a `MATH`
table: without one, every vertical dimension would be a guess.

Every dimension comes from the font: `axisHeight`, `fractionRuleThickness`, the
script scale percentages, the script shifts and gaps, italic correction, accent
attachment, stretchy variants and assemblies. Where a font leaves a constant at
an implausible zero, `font.rs` substitutes TeX's value.

`\text{…}` uses a second face if you supply one, otherwise the maths font's
upright Latin.

The `embed-math-font` feature builds in New Computer Modern Math (GUST Font License,
see `assets/`). **The default build ships no font.**
