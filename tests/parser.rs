//! Parser tests over a corpus of realistic OCR output.

// `clippy.toml`'s `allow-unwrap-in-tests` does not reach integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use eeltex::{AtomClass, Error, Expr, parse};

const SAMPLES: &[&str] = &[
    // Everyday formulae.
    "\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
    "\\int_{0}^{\\infty} e^{-x^2} \\, dx = \\frac{\\sqrt{\\pi}}{2}",
    "\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\frac{\\pi^2}{6}",
    "E = mc^2",
    "\\nabla \\times \\mathbf{B} = \\mu_0 \\mathbf{J}",
    "\\left( \\frac{\\partial f}{\\partial x} \\right)_{y}",
    "P(A \\mid B) = \\frac{P(B \\mid A) P(A)}{P(B)}",
    "\\lim_{x \\to 0} \\frac{\\sin x}{x} = 1",
    "\\mathbb{R}^{n} \\text{ with } n \\geq 2",
    // The symbol table, exhaustively.
    "\\alpha \\beta \\gamma \\delta \\epsilon \\varepsilon \\phi \\varphi \\theta \\vartheta",
    "\\Gamma \\Delta \\Theta \\Lambda \\Xi \\Pi \\Sigma \\Upsilon \\Phi \\Psi \\Omega",
    "a \\leq b \\geq c \\neq d \\approx e \\equiv f \\propto g \\sim h \\simeq i \\cong j",
    "A \\subset B \\subseteq C \\supset D \\supseteq E \\in F \\notin G \\ni H",
    "a \\to b \\leftarrow c \\leftrightarrow d \\Rightarrow e \\Leftarrow f \\Leftrightarrow g",
    "\\mapsto \\longrightarrow \\longleftarrow \\hookrightarrow \\uparrow \\downarrow \\nearrow \\searrow",
    "\\cdot \\times \\div \\pm \\mp \\oplus \\ominus \\otimes \\odot \\circ \\bullet \\star \\ast",
    "\\setminus \\wedge \\vee \\neg \\cap \\cup \\perp \\parallel \\mid \\models \\vdash",
    "\\infty \\partial \\nabla \\forall \\exists \\nexists \\emptyset \\top \\bot \\angle \\degree",
    "\\hbar \\ell \\Re \\Im \\wp \\aleph \\prime \\therefore \\because",
    "\\ldots \\cdots \\vdots \\ddots",
    "\\ll \\gg \\asymp \\nsubseteq \\varnothing \\omicron",
    // Structures.
    "\\lfloor x \\rfloor \\lceil y \\rceil \\langle z \\rangle",
    "\\iint \\iiint \\oint \\prod \\coprod \\bigcup \\bigcap",
    "\\sin \\cos \\tan \\cot \\sec \\csc \\arcsin \\arccos \\arctan \\sinh \\cosh \\tanh \\coth",
    "\\ln \\log \\exp \\max \\min \\det \\deg \\dim \\ker \\gcd \\arg \\sup \\inf",
    "\\mathbf{a} \\mathit{b} \\mathrm{c} \\mathsf{d} \\mathtt{e} \\mathcal{F} \\mathfrak{G} \\mathbb{H}",
    "\\hat{a} \\tilde{b} \\bar{c} \\vec{d} \\dot{e} \\ddot{f} \\overline{g} \\underline{h}",
    "\\widehat{abc} \\widetilde{xy}",
    "\\sqrt[3]{x} + \\sqrt{y}",
    "x_{i}^{j} + \\sum_{k} a_k",
    "a \\quad b \\qquad c \\, d \\; e \\: f \\! g",
    "\\left[ \\frac{a}{b} \\right] \\left\\{ c \\right\\} \\left| d \\right|",
    "\\left\\| v \\right\\| = 1",
    "\\frac12 + \\frac{1}{2}",
    "\\int\\limits_0^1 f \\qquad \\sum\\nolimits_k a_k",
    "\\dfrac{a}{b} + \\tfrac{c}{d}",
    // Environments.
    "\\begin{align} a &= b + c \\\\ d &= e \\end{align}",
    "\\begin{aligned} f(x) &= x^2 \\\\ &= x \\cdot x \\end{aligned}",
    "\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}",
    "\\begin{bmatrix} a \\\\ b \\end{bmatrix}",
    "\\begin{vmatrix} a & b \\\\ c & d \\end{vmatrix}",
    "\\begin{Bmatrix} a \\end{Bmatrix}",
    "\\begin{Vmatrix} a \\end{Vmatrix}",
    "\\begin{matrix} 1 & 2 \\\\ 3 & 4 \\end{matrix}",
    "f(x) = \\begin{cases} 1 & x > 0 \\\\ 0 & \\text{otherwise} \\end{cases}",
    "\\begin{gather} a = b \\\\ c = d \\end{gather}",
];

#[test]
fn every_sample_parses() {
    for src in SAMPLES {
        assert!(parse(src).is_ok(), "{src:?} -> {:?}", parse(src));
    }
}

#[test]
fn unsupported_constructs_are_rejected() {
    for src in [
        // Paragraph columns need line breaking.
        "\\begin{array}{p{2cm}} 1 \\end{array}",
        "x \\someunknowncommand y",
        "\\begin{align} a &= b \\end{gather}",
        // Alignment separators mean nothing outside an environment.
        "a & b",
        "a \\\\ b",
        // Ambiguous even in LaTeX.
        "x^2^3",
    ] {
        assert!(parse(src).is_err(), "{src:?} should not have parsed");
    }
}

#[test]
fn atom_classes_reach_the_tree() {
    // A bare `+` is binary; braced, it is ordinary.
    assert_eq!(parse("+").map(|e| e.class()), Ok(AtomClass::Bin));
    assert_eq!(parse("{+}").map(|e| e.class()), Ok(AtomClass::Ord));
    assert_eq!(
        parse("\\frac{a}{b}").map(|e| e.class()),
        Ok(AtomClass::Inner)
    );
    assert_eq!(parse("\\sqrt{a}").map(|e| e.class()), Ok(AtomClass::Ord));
    assert_eq!(parse("\\sum_i").map(|e| e.class()), Ok(AtomClass::Op));
    assert_eq!(parse("x_i").map(|e| e.class()), Ok(AtomClass::Ord));
}

#[test]
fn text_runs_keep_their_spaces() {
    let Ok(Expr::Seq(items)) = parse("n \\text{ with } m") else {
        panic!("expected a sequence");
    };
    assert!(
        items.contains(&Expr::Text(" with ".into())),
        "outer spaces were trimmed: {items:?}"
    );
}

#[test]
fn runaway_nesting_is_refused() {
    assert_eq!(parse(&"{".repeat(10_000)), Err(Error::TooDeep));
    assert_eq!(parse(&"\\frac{".repeat(10_000)), Err(Error::TooDeep));
    assert!(parse(&"\\sqrt".repeat(10_000)).is_err());
}

#[test]
fn colour_commands_scope_the_way_latex_does() {
    let Ok(Expr::Seq(items)) = parse("a \\textcolor{red}{b} c") else {
        panic!("expected a sequence");
    };
    assert_eq!(items.len(), 3, "\\textcolor swallowed its neighbours");
    assert!(matches!(items.get(1), Some(Expr::Styled { .. })));

    let Ok(Expr::Seq(items)) = parse("a \\color{red} b c") else {
        panic!("expected a sequence");
    };
    assert_eq!(items.len(), 2, "\\color did not run to the end: {items:?}");
    let Some(Expr::Styled { body, .. }) = items.get(1) else {
        panic!("the tail is not styled: {items:?}");
    };
    assert_eq!(**body, parse("b c").unwrap());

    let Ok(Expr::Seq(items)) = parse("{\\color{red} a} b") else {
        panic!("expected a sequence");
    };
    assert_eq!(items.len(), 2, "\\color escaped its group: {items:?}");
}

#[test]
fn an_unreadable_colour_is_refused() {
    assert!(parse("\\textcolor{red}{x}").is_ok());
    assert!(parse("\\textcolor{#ff8800}{x}").is_ok());
    assert!(parse("\\textcolor{rgb(1.0, 0.5, 0)}{x}").is_ok());
    for bad in [
        "\\textcolor{chartreuse}{x}",
        "\\textcolor{red!50!blue}{x}",
        "\\color{#gg0000} x",
    ] {
        assert!(matches!(parse(bad), Err(Error::Parse { .. })), "{bad}");
    }
}
