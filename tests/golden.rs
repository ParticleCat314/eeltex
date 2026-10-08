//! Golden snapshots of the text exporters.
//! Re-bless with `EELTEX_BLESS=1 cargo test --all-features --test golden`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "embed-math-font", feature = "svg"))]

use std::path::PathBuf;

use eeltex::export::svg::{SvgOptions, to_svg};
use eeltex::{Fonts, MathStyle, Options};

/// `(file stem, source, style)`.
#[rustfmt::skip]
const CASES: &[(&str, &str, MathStyle)] = &[
    ("letter", "x", MathStyle::Display),
    ("digits", "2024", MathStyle::Display),
    ("binary", "a + b - c", MathStyle::Display),
    ("unary-minus", "-x = -(y + z)", MathStyle::Display),
    ("group-forces-ord", "a {+} b", MathStyle::Display),
    ("superscript", "x^2", MathStyle::Display),
    ("subscript", "x_i", MathStyle::Display),
    ("both-scripts", "x_i^2", MathStyle::Display),
    ("nested-scripts", "x^{y^{z^w}}", MathStyle::Display),
    ("primes", "f''(x)", MathStyle::Display),
    ("fraction", "\\frac{a}{b}", MathStyle::Display),
    ("fraction-inline", "\\frac{a}{b}", MathStyle::Text),
    ("dfrac-tfrac", "\\dfrac{a}{b} \\tfrac{c}{d}", MathStyle::Text),
    ("nested-fraction", "\\frac{\\frac{a}{b}}{c}", MathStyle::Display),
    ("quadratic", "\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}", MathStyle::Display),
    ("radical", "\\sqrt{x + 1}", MathStyle::Display),
    ("radical-degree", "\\sqrt[3]{x}", MathStyle::Display),
    ("sum-display", "\\sum_{n=1}^{\\infty} \\frac{1}{n^2}", MathStyle::Display),
    ("sum-inline", "\\sum_{n=1}^{\\infty} \\frac{1}{n^2}", MathStyle::Text),
    ("integral", "\\int_0^\\infty e^{-x^2} \\, dx", MathStyle::Display),
    ("integral-limits", "\\int\\limits_0^1 f", MathStyle::Display),
    ("integral-tucked", "\\int_0^1 f(x) \\, dx", MathStyle::Display),
    ("contour-integral", "\\oint_C \\vec{F} \\cdot d\\vec{r}", MathStyle::Display),
    ("double-integral", "\\iint_S \\nabla \\cdot F", MathStyle::Display),
    ("limit", "\\lim_{x \\to 0} \\frac{\\sin x}{x} = 1", MathStyle::Display),
    ("delimiters-small", "\\left( x \\right)", MathStyle::Display),
    ("delimiters-grown", "\\left( \\frac{\\partial f}{\\partial x} \\right)_y", MathStyle::Display),
    ("delimiters-assembled", "\\left[ \\begin{matrix} a \\\\ b \\\\ c \\\\ d \\\\ e \\end{matrix} \\right]", MathStyle::Display),
    ("accents", "\\hat{a} \\tilde{b} \\bar{c} \\vec{d}", MathStyle::Display),
    ("bars", "\\overline{x + y} \\underline{z}", MathStyle::Display),
    ("wide-accents", "\\widehat{abc} \\widetilde{xy}", MathStyle::Display),
    ("alphabets", "\\mathbb{R} \\mathcal{L} \\mathfrak{g} \\mathbf{v} \\mathtt{k}", MathStyle::Display),
    ("greek", "\\alpha \\Gamma \\phi \\varphi \\epsilon \\varepsilon", MathStyle::Display),
    ("text-run", "\\mathbb{R}^n \\text{ with } n \\geq 2", MathStyle::Display),
    ("spacing", "a \\, b \\; c \\quad d \\! e", MathStyle::Display),
    ("functions", "\\sin x + \\log y", MathStyle::Display),
    ("matrix", "\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}", MathStyle::Display),
    ("matrix-bars", "\\begin{vmatrix} a & b \\\\ c & d \\end{vmatrix}", MathStyle::Display),
    ("cases", "\\begin{cases} 1 & x > 0 \\\\ 0 & \\text{otherwise} \\end{cases}", MathStyle::Display),
    ("aligned", "\\begin{aligned} f(x) &= x^2 \\\\ &= x \\cdot x \\end{aligned}", MathStyle::Display),
    ("gathered", "\\begin{gathered} a = b \\\\ c = d \\end{gathered}", MathStyle::Display),
    ("binomial", "\\binom{n}{k} + \\tbinom{n}{k}", MathStyle::Display),
    ("horizontal-brace", "\\underbrace{x + y + z}_{\\text{sum}}", MathStyle::Display),
    ("stacked", "\\overset{f}{\\longrightarrow} \\stackrel{g}{=} \\underset{h}{\\to}", MathStyle::Display),
    ("negated", "a \\not= b \\not\\subset c", MathStyle::Display),
    ("substack", "\\sum_{\\substack{i < j \\\\ j < k}} a_{ij}", MathStyle::Display),
    ("phantom", "\\frac{1}{2} + \\phantom{\\frac{1}{2}} = \\hphantom{x}y", MathStyle::Display),
    ("extensible-arrow", "A \\xrightarrow[\\text{iso}]{f} B \\xleftarrow{\\text{a longer label}} C", MathStyle::Display),
    // Without `attrs` the colour is dropped, so the golden differs per build.
    #[cfg(feature = "attrs")]
    ("colour", "\\frac{\\textcolor{red}{a}}{\\color{#0080ff} b} + c", MathStyle::Display),
];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn render(source: &str, style: MathStyle) -> String {
    let fonts = Fonts::embedded().expect("the embedded font parses");
    let options = Options::default().with_font_size(40.0).with_style(style);
    let scene =
        eeltex::render(source, &fonts, &options).unwrap_or_else(|e| panic!("{source:?}: {e}"));
    assert!(
        scene.missing.is_empty(),
        "{source:?}: the bundled font has no glyph for {:?}",
        scene.missing
    );
    let svg = to_svg(&scene, &fonts, &SvgOptions::default()).expect("a finite scene renders");
    // One element per line keeps diffs readable.
    format!("{}\n", svg.replace("><", ">\n<"))
}

#[test]
fn golden_svgs_match() {
    let bless = std::env::var_os("EELTEX_BLESS").is_some();
    let dir = golden_dir();
    if bless {
        std::fs::create_dir_all(&dir).expect("create the golden directory");
    }

    let mut stale: Vec<String> = Vec::new();
    for (name, source, style) in CASES {
        let path = dir.join(format!("{name}.svg"));
        check(
            &path,
            render(source, *style).into_bytes(),
            bless,
            &mut stale,
        );
    }

    assert!(
        stale.is_empty(),
        "{} golden(s) differ:\n{}\n\nIf the change was deliberate:\n  \
         EELTEX_BLESS=1 cargo test --all-features --test golden",
        stale.len(),
        stale.join("\n")
    );
}

/// Bytes, not `String`: a PDF is not valid UTF-8.
fn check(path: &std::path::Path, actual: Vec<u8>, bless: bool, stale: &mut Vec<String>) {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let expected = std::fs::read(path).ok();

    if bless {
        if expected.as_deref() != Some(actual.as_slice()) {
            std::fs::write(path, &actual).expect("write the golden file");
            println!("blessed {name}");
        }
        return;
    }

    match expected {
        None => stale.push(format!("{name}: no golden file")),
        Some(expected) if expected != actual => {
            let a = String::from_utf8_lossy(&expected);
            let b = String::from_utf8_lossy(&actual);
            stale.push(format!("{name}: {}", first_difference(&a, &b)));
        }
        Some(_) => {}
    }
}

fn first_difference(expected: &str, actual: &str) -> String {
    for (i, (a, b)) in expected.lines().zip(actual.lines()).enumerate() {
        if a != b {
            return format!(
                "line {}\n  expected {}\n  actual   {}",
                i + 1,
                trim(a),
                trim(b)
            );
        }
    }
    format!(
        "same prefix, {} lines expected and {} actual",
        expected.lines().count(),
        actual.lines().count()
    )
}

fn trim(line: &str) -> String {
    if line.len() <= 110 {
        return line.to_owned();
    }
    match line.char_indices().nth(107) {
        Some((end, _)) => format!("{}...", line.get(..end).unwrap_or(line)),
        None => line.to_owned(),
    }
}

#[test]
fn case_names_are_unique() {
    let mut names: Vec<&str> = CASES.iter().map(|(name, _, _)| *name).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(count, names.len(), "duplicate golden name");
}
