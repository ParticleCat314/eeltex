//! The serializer's contract: a tree written out and read back is the same tree.

// `clippy.toml`'s `allow-unwrap-in-tests` does not reach integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use eeltex::{parse, to_tex};

const CORPUS: &[&str] = &[
    // Everyday formulae.
    r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
    r"\int_{0}^{\infty} e^{-x^2} \, dx = \frac{\sqrt{\pi}}{2}",
    r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}",
    "E = mc^2",
    r"\nabla \times \mathbf{B} = \mu_0 \mathbf{J}",
    r"\left( \frac{\partial f}{\partial x} \right)_{y}",
    r"P(A \mid B) = \frac{P(B \mid A) P(A)}{P(B)}",
    r"\lim_{x \to 0} \frac{\sin x}{x} = 1",
    r"\mathbb{R}^{n} \text{ with } n \geq 2",
    // Symbols in bulk.
    r"\alpha \beta \gamma \delta \epsilon \varepsilon \phi \varphi \theta \vartheta",
    r"\Gamma \Delta \Theta \Lambda \Xi \Pi \Sigma \Upsilon \Phi \Psi \Omega",
    r"a \leq b \geq c \neq d \approx e \equiv f \propto g \sim h \simeq i \cong j",
    r"A \subset B \subseteq C \supset D \supseteq E \in F \notin G \ni H",
    r"a \to b \leftarrow c \leftrightarrow d \Rightarrow e \Leftarrow f \Leftrightarrow g",
    r"\mapsto \longrightarrow \longleftarrow \hookrightarrow \uparrow \downarrow \nearrow \searrow",
    r"\cdot \times \div \pm \mp \oplus \ominus \otimes \odot \circ \bullet \star \ast",
    r"\setminus \wedge \vee \neg \cap \cup \perp \parallel \mid \models \vdash",
    r"\infty \partial \nabla \forall \exists \nexists \emptyset \top \bot \angle \degree",
    r"\hbar \ell \Re \Im \wp \aleph \prime \therefore \because",
    r"\ldots \cdots \vdots \ddots",
    r"\ll \gg \asymp \nsubseteq \varnothing \omicron",
    r"\Alpha \Beta \Epsilon \Zeta \Eta \Iota \Kappa \Mu \Nu \Omicron \Rho \Tau \Chi",
    r"\varGamma \varDelta \varTheta \varLambda \varXi \varPi \varSigma \varUpsilon \varPhi \varPsi \varOmega",
    r"a \lt b \gt c \leqq d \geqq e \lneqq f \lnsim g \lessdot h \curlyeqprec i \ntriangleleft j",
    r"\eqcirc \circeq \bumpeq \Bumpeq \vcentcolon \coloneqq \backepsilon \Subset \Supset \Lsh \Rsh",
    r"a \Cup b \Cap c \centerdot d \doublebarwedge e",
    r"\Bbbk \euro \cent \iddots \diagup \diagdown \AA \ae \OE \ss \i \j \textasciitilde",
    r"\lparen x \rparen",
    // Fractions, delimiters and switches.
    r"\left( \frac{a}{b} \middle| c \right) \left\uparrow x \right\Downarrow \left< y \right>",
    r"\cfrac{1}{1+\cfrac{1}{x}} \genfrac{[}{]}{0pt}{1}{n}{k} {a \atop b} {n \brace k} {n \brack k}",
    // Fonts, prose and accents.
    r"\mathscr{L} \mathbfit{x} \mathsfit{y} \mathbfsf{z} \mathbffrak{g} \mathbfscr{F}",
    r"\textbf{bold} \textit{italic} \textsf{sans} \texttt{mono} \emph{this}",
    r"\widecheck{ab} \utilde{x} \overgroup{ab} \undergroup{ab} \overlinesegment{AB} \underlinesegment{CD}",
    r"\overleftharpoon{v} \overrightharpoon{w} \Overrightarrow{AB} \underleftrightarrow{xy}",
    r#"\'{a} \`{e} \"{o} \~{n} \={u} \.{z} \u{a} \v{c} \H{o} \r{a}"#,
    // Spacing.
    r"a \hspace{1em} b \kern 7mu c \mkern-5mu d \enspace e",
    r"a \equiv b \pmod{n+1} \quad c \pod{m}",
    // Environments.
    r"\begin{array}{lcr} a & b & c \\ d & e & f \end{array}",
    r"\begin{darray}{cc} \frac12 & 3 \end{darray} \begin{subarray}{l} i \\ j \end{subarray}",
    r"\begin{pmatrix*}[r] -1 & 2 \\ 3 & -4 \end{pmatrix*} \begin{matrix*}[l] a \end{matrix*}",
    r"\begin{dcases} a & b \end{dcases} \begin{rcases} c & d \end{rcases} \begin{drcases} e \end{drcases}",
    r"a~b",
    r"\sh x \ch x \tg x \ctg x \cth x \arcctg x \cosec x",
    r"\varliminf_{n} a_n \varlimsup_{n} a_n \varinjlim_{i} A_i \varprojlim_{i} A_i",
    r"\intop_0^1 f \smallint f \ointclockwise_C f \ointctrclockwise_C f",
    r"\xrightleftharpoons[a]{b} \xleftrightharpoons{c} \xlongequal{d} \xtofrom{e} \xtwoheadleftarrow{f}",
    // Structures.
    r"\lfloor x \rfloor \lceil y \rceil \langle z \rangle",
    r"\iint \iiint \oint \prod \coprod \bigcup \bigcap",
    r"\sin \cos \tan \cot \sec \csc \arcsin \arccos \arctan \sinh \cosh \tanh \coth",
    r"\ln \log \exp \max \min \det \deg \dim \ker \gcd \arg \sup \inf",
    r"\limsup \liminf \injlim \projlim",
    r"\mathbf{a} \mathit{b} \mathrm{c} \mathsf{d} \mathtt{e} \mathcal{F} \mathfrak{G} \mathbb{H}",
    r"\mathord{a} \mathbin{b} \mathrel{c} \mathopen{d} \mathclose{e} \mathpunct{f} \mathinner{g}",
    r"\hat{a} \tilde{b} \bar{c} \vec{d} \dot{e} \ddot{f} \overline{g} \underline{h}",
    r"\dddot{a} \ddddot{b} \acute{c} \grave{d} \breve{e} \check{f} \mathring{g}",
    r"\overbracket{a} \underbracket{b} \overrightarrow{c} \overleftarrow{d}",
    r"\overleftrightarrow{a} \underrightarrow{b} \underleftarrow{c}",
    r"\widehat{abc} \widetilde{xy}",
    r"\sqrt[3]{x} + \sqrt{y}",
    "x_{i}^{j} + \\sum_{k} a_k",
    r"f''(x)",
    r"a \quad b \qquad c \, d \; e \: f \! g",
    r"a \thinspace b \medspace c \thickspace d \space e",
    r"a \negthinspace b \negmedspace c \negthickspace d",
    r"\left[ \frac{a}{b} \right] \left\{ c \right\} \left| d \right|",
    r"\left\| v \right\| = 1",
    r"\left. x \right)",
    r"\left\langle a \right\rangle \left\lfloor b \right\rfloor \left\lceil c \right\rceil",
    r"\frac12 + \frac{1}{2}",
    r"\int\limits_0^1 f \qquad \sum\nolimits_k a_k",
    r"\dfrac{a}{b} + \tfrac{c}{d}",
    r"\binom{n}{k} + \dbinom{n}{k} + \tbinom{n}{k}",
    r"\overset{f}{\longrightarrow} \stackrel{g}{=} \underset{h}{\to}",
    r"a \not= b \not\subset c",
    r"\underbrace{x + y + z}_{\text{sum}} \overbrace{a+b}^{n}",
    r"\overbrace{x}_{i}",
    r"\phantom{xyz} \hphantom{xyz} \vphantom{X}",
    r"A \xrightarrow[\text{iso}]{f} B \xleftarrow{\text{a longer label}} C",
    r"\xrightarrow{} \xleftrightarrow{a} \xRightarrow{b} \xLeftarrow{c} \xLeftrightarrow{d}",
    r"\xmapsto{a} \xhookrightarrow{b} \xhookleftarrow{c} \xtwoheadrightarrow{d}",
    r"\xrightharpoonup{a} \xleftharpoondown{b} \xequal{c}",
    r"\textcolor{red}{a} + \textcolor{#0080ff}{b}",
    r"{\color{#e0a030} x + y}",
    r"a {+} b",
    // Environments.
    r"\begin{align} a &= b + c \\ d &= e \end{align}",
    r"\begin{aligned} f(x) &= x^2 \\ &= x \cdot x \end{aligned}",
    r"\begin{gathered} a = b \\ c = d \end{gathered}",
    r"\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}",
    r"\begin{bmatrix} a \\ b \end{bmatrix}",
    r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}",
    r"\begin{Bmatrix} a \end{Bmatrix}",
    r"\begin{Vmatrix} a \end{Vmatrix}",
    r"\begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix}",
    r"f(x) = \begin{cases} 1 & x > 0 \\ 0 & \text{otherwise} \end{cases}",
    r"\sum_{\substack{i < j \\ j < k}} a_{ij}",
    r"\substack{a \\ bb}",
    // Deliberately awkward.
    "",
    "{}",
    r"\frac{}{}",
    r"x^{}",
    r"\left( \begin{matrix} a \end{matrix} \right)",
    r"\frac{\frac{a}{b}}{c}",
    r"x^{y^{z^w}}",
    r"\text{}",
    r"\text{ if }",
    r"\& \% \# \$ \_ \{ \}",
    r"\colon : \mid |",
    r"\backslash \Vert \| \lvert \rvert \lVert \rVert",
];

/// What only the `formatting` feature reads.
#[cfg(feature = "formatting")]
const FORMATTING_CORPUS: &[&str] = &[
    r"{\tiny a} {\small b} {\large c} {\Large d} {\LARGE e} {\huge f} {\Huge g}",
    r"\rule{1em}{0.5em} \rule[-0.25em]{0.1em}{1em} \strut \mathstrut",
    r"\smash{x} \smash[t]{y} \smash[b]{z} \mathllap{a} \mathrlap{b} \mathclap{c} \raisebox{0.5em}{d}",
    r"\boxed{x} \fbox{text $x^2$} \colorbox{#ffff00}{hi} \fcolorbox{#ff0000}{#ffff00}{there}",
    r"\cancel{a} \bcancel{b} \xcancel{c} \sout{d} \cancelto{0}{x}",
    r"\begin{array}{l|c:r} a & b & c \\ \hline d & e & f \\ \hdashline \end{array}",
];

fn corpus() -> Vec<&'static str> {
    #[cfg(feature = "formatting")]
    return [CORPUS, FORMATTING_CORPUS].concat();
    #[cfg(not(feature = "formatting"))]
    CORPUS.to_vec()
}

fn round_trip(src: &str) {
    let expr = match parse(src) {
        Ok(expr) => expr,
        Err(e) => panic!("{src:?} does not parse: {e}"),
    };
    let written = to_tex(&expr);
    match parse(&written) {
        Ok(again) => assert_eq!(expr, again, "{src:?} was written {written:?}"),
        Err(e) => panic!("{src:?} was written {written:?}, which does not parse: {e}"),
    }
}

#[test]
fn the_corpus_round_trips() {
    for src in corpus() {
        round_trip(src);
    }
}

#[test]
fn writing_is_stable_under_repetition() {
    for src in corpus() {
        let once = to_tex(&parse(src).unwrap());
        let twice = to_tex(&parse(&once).unwrap());
        assert_eq!(once, twice, "{src:?} was written {once:?} then {twice:?}");
    }
}

fn symbol_names() -> Vec<String> {
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

#[test]
fn every_symbol_in_the_table_round_trips() {
    let names = symbol_names();
    assert!(
        names.len() > 300,
        "only found {} names — the extraction is broken and this test is vacuous",
        names.len()
    );
    for name in names {
        round_trip(&format!("\\{name}"));
        // Next to a letter, a word command needs its separating space.
        round_trip(&format!("\\{name} x"));
    }
}

#[test]
fn the_characters_with_meaning_survive_being_written() {
    for src in [r"\{", r"\}", r"\$", r"\%", r"\#", r"\&", r"\_", r"\^"] {
        round_trip(src);
        assert_eq!(to_tex(&parse(src).unwrap()), src);
    }
}

#[test]
fn the_substituted_characters_go_back_to_their_keys() {
    assert_eq!(to_tex(&parse("a - b").unwrap()), "a-b");
    assert_eq!(to_tex(&parse("a * b").unwrap()), "a*b");
    round_trip("a - b");
    round_trip("a * b");
}
