//! Every symbol command in `src/parsing/symbols.rs`, checked against the embedded font.

#![cfg(feature = "embed-math-font")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Commands New Computer Modern Math has no glyph for; asserted exact below.
const NO_GLYPH_IN_EMBEDDED_FONT: &[&str] = &["circledS"];

/// Arms look like `"a" | "b" => (…)`, arrow on the same line.
fn names() -> Vec<String> {
    let source = include_str!("../src/parsing/symbols.rs");
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('"') {
            continue;
        }
        let Some((patterns, _)) = trimmed.split_once("=>") else {
            continue;
        };
        for piece in patterns.split('|') {
            let piece = piece.trim();
            if let Some(name) = piece.strip_prefix('"').and_then(|p| p.strip_suffix('"')) {
                out.push(name.to_owned());
            }
        }
    }
    out
}

fn fonts() -> eeltex::Fonts<'static> {
    eeltex::Fonts::embedded().expect("the embedded font parses")
}

#[test]
fn the_table_is_not_a_handful_of_entries() {
    // A floor, so an extraction that matched nothing cannot pass vacuously.
    let names = names();
    assert!(
        names.len() > 300,
        "only found {} names in the table — the extraction is broken",
        names.len()
    );
}

#[test]
fn every_command_in_the_table_lays_out() {
    let fonts = fonts();
    let options = eeltex::Options::default();
    let mut broken = Vec::new();
    for name in names() {
        let source = format!("\\{name}");
        match eeltex::render(&source, &fonts, &options) {
            Err(error) => broken.push(format!("\\{name}: {error}")),
            Ok(scene) => {
                if scene.glyphs.is_empty() {
                    broken.push(format!("\\{name}: laid out to nothing"));
                }
            }
        }
    }
    assert!(
        broken.is_empty(),
        "{} broken:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

#[test]
fn the_font_coverage_exceptions_are_exactly_right() {
    let fonts = fonts();
    let options = eeltex::Options::default();
    let mut missing = Vec::new();
    for name in names() {
        let source = format!("\\{name}");
        let Ok(scene) = eeltex::render(&source, &fonts, &options) else {
            continue; // reported by the test above
        };
        if !scene.missing.is_empty() {
            missing.push(name);
        }
    }
    missing.sort();
    missing.dedup();
    let expected: Vec<String> = NO_GLYPH_IN_EMBEDDED_FONT
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(
        missing, expected,
        "the set of commands the embedded font cannot draw has changed; \
         update NO_GLYPH_IN_EMBEDDED_FONT"
    );
}

#[test]
fn the_common_commands_are_all_there() {
    let fonts = fonts();
    let options = eeltex::Options::default();
    const WANTED: &[&str] = &[
        "dagger",
        "ddagger",
        "Box",
        "square",
        "blacksquare",
        "triangle",
        "lozenge",
        "clubsuit",
        "diamondsuit",
        "heartsuit",
        "spadesuit",
        "flat",
        "natural",
        "sharp",
        "checkmark",
        "maltese",
        "surd",
        "imath",
        "jmath",
        "complement",
        "mho",
        "eth",
        // Binary operators past the obvious four.
        "uplus",
        "sqcap",
        "sqcup",
        "amalg",
        "wr",
        "diamond",
        "bigcirc",
        "oslash",
        "boxplus",
        "boxtimes",
        "ltimes",
        "rtimes",
        "intercal",
        "dotplus",
        "veebar",
        "bigtriangleup",
        "bigtriangledown",
        "triangleleft",
        "triangleright",
        // Orderings and containments.
        "prec",
        "succ",
        "preceq",
        "succeq",
        "subsetneq",
        "supsetneq",
        "sqsubseteq",
        "sqsupseteq",
        "doteq",
        "bowtie",
        "smile",
        "frown",
        "lesssim",
        "gtrsim",
        "leqslant",
        "geqslant",
        "nleq",
        "ngeq",
        "nmid",
        "nparallel",
        "ncong",
        "nsim",
        // Logic.
        "vDash",
        "Vdash",
        "dashv",
        "multimap",
        "therefore",
        "because",
        // Arrows.
        "Longrightarrow",
        "Longleftarrow",
        "Longleftrightarrow",
        "longmapsto",
        "hookleftarrow",
        "twoheadrightarrow",
        "rightarrowtail",
        "rightrightarrows",
        "rightharpoonup",
        "leftharpoondown",
        "rightleftharpoons",
        "leadsto",
        "curvearrowright",
        "circlearrowleft",
        "nwarrow",
        "swarrow",
        "Uparrow",
        "Downarrow",
        "Updownarrow",
        "nrightarrow",
        "nRightarrow",
        // Delimiters by name.
        "lbrace",
        "rbrace",
        "lbrack",
        "rbrack",
        "lVert",
        "rVert",
        "llbracket",
        "rrbracket",
        "ulcorner",
        "lrcorner",
        "lgroup",
        "rgroup",
        // Letters and marks.
        "beth",
        "gimel",
        "daleth",
        "varkappa",
        "backprime",
        "dprime",
        "S",
        "P",
        "pounds",
        "yen",
        "copyright",
    ];
    let mut broken = Vec::new();
    for name in WANTED {
        if eeltex::render(&format!("\\{name}"), &fonts, &options).is_err() {
            broken.push(*name);
        }
    }
    assert!(broken.is_empty(), "still unknown: {}", broken.join(" "));
}

#[test]
fn the_common_constructs_are_all_there() {
    let fonts = fonts();
    let options = eeltex::Options::default();
    const WANTED: &[&str] = &[
        r"\bigvee_i x",
        r"\bigwedge_i x",
        r"\bigoplus_{i=1}^{n} x",
        r"\bigotimes_i x",
        r"\bigodot_i x",
        r"\bigsqcup_i x",
        r"\biguplus_i x",
        r"\iiiint f",
        r"\oiint_S f",
        r"\oiiint_V f",
        r"\acute{a}",
        r"\grave{a}",
        r"\breve{a}",
        r"\check{a}",
        r"\mathring{a}",
        r"\dddot{x}",
        r"\ddddot{x}",
        r"\limsup_{n} a_n",
        r"\liminf_{n} a_n",
        r"\lg x",
        r"\Pr[X]",
        r"\hom(A, B)",
        r"\sgn x",
        r"a \bmod b",
        r"\binom{n}{k}",
        r"\dbinom{n}{k}",
        r"\tbinom{n}{k}",
        r"\overset{a}{=}",
        r"\underset{a}{=}",
        r"\stackrel{f}{\to}",
        r"\overbrace{x+y}^{n}",
        r"\underbrace{x+y}_{n}",
        r"\overbracket{ab}",
        r"\underbracket{ab}",
        r"\overrightarrow{AB}",
        r"\overleftarrow{AB}",
        r"\overleftrightarrow{AB}",
        r"\underrightarrow{AB}",
        r"\underleftarrow{AB}",
        r"a \not= b",
        r"x \not\subset y",
        r"\phantom{xyz}",
        r"\hphantom{xyz}",
        r"\vphantom{X}",
        r"\mathord{x}",
        r"\mathbin{\star}",
        r"\mathrel{\lhd}",
        r"\mathopen{(}",
        r"\mathclose{)}",
        r"\mathpunct{,}",
        r"\mathinner{x}",
        r"\mathop{X}",
        r"\sum_{\substack{i<j \\ j<k}} a_{ij}",
        r"A \xrightarrow{f} B",
        r"\xrightarrow[\text{iso}]{\sim}",
        r"\xleftarrow{f}",
        r"\xleftrightarrow{f}",
        r"\xRightarrow{n}",
        r"\xLeftarrow{n}",
        r"\xLeftrightarrow{n}",
        r"\xmapsto{\phi}",
        r"\xhookrightarrow{i}",
        r"\xhookleftarrow{i}",
        r"\xtwoheadrightarrow{p}",
        r"\xrightharpoonup{a}",
        r"\xleftharpoondown{a}",
        r"\xequal{d}",
    ];
    let mut broken = Vec::new();
    for source in WANTED {
        if let Err(error) = eeltex::render(source, &fonts, &options) {
            broken.push(format!("{source}: {error}"));
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

/// `\implies` and `\iff` are the *long* arrows in amsmath.
#[test]
fn implies_and_iff_are_the_long_arrows() {
    let fonts = fonts();
    let options = eeltex::Options::default();
    let long = eeltex::render(r"\implies", &fonts, &options).expect("lays out");
    let short = eeltex::render(r"\Rightarrow", &fonts, &options).expect("lays out");
    assert!(
        long.width > short.width * 1.2,
        "\\implies ({}px) is no longer than \\Rightarrow ({}px)",
        long.width,
        short.width
    );
    let biconditional = eeltex::render(r"\iff", &fonts, &options).expect("lays out");
    let short = eeltex::render(r"\Leftrightarrow", &fonts, &options).expect("lays out");
    assert!(biconditional.width > short.width * 1.2);
}

#[test]
fn an_unknown_command_is_still_rejected() {
    let fonts = fonts();
    let options = eeltex::Options::default();
    // Letters only: `\bigvee2` is `\bigvee` followed by a digit.
    for name in ["thereisnosuchcommand", "daggerr", "Boxx", "bigveee"] {
        assert!(
            eeltex::render(&format!("\\{name}"), &fonts, &options).is_err(),
            "\\{name} was accepted"
        );
    }
}
