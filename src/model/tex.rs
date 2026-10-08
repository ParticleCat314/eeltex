//! Writing an [`Expr`] back out as LaTeX.

use crate::ast::{
    Accent, Adjust, Alphabet, BigOpKind, Delim, DelimSize, Enclose, EnvKind, Expr, FracStyle, Lap,
    Limits, StyleSwitch,
};
use crate::attrs::Paint;
use crate::class::{AtomClass, class_of_char, math_char};
use crate::commands;
use crate::symbols;

#[must_use]
pub fn to_tex(expr: &Expr) -> String {
    let mut out = String::new();
    write_tex(expr, &mut out);
    out
}

pub fn write_tex(expr: &Expr, out: &mut String) {
    Writer {
        buf: out,
        word: false,
    }
    .expr(expr);
}

struct Writer<'a> {
    buf: &'a mut String,
    /// Last output was a letter-named command, so a following letter needs a space.
    word: bool,
}

impl Writer<'_> {
    fn raw(&mut self, text: &str) {
        if self.word && text.starts_with(|c: char| c.is_ascii_alphabetic()) {
            self.buf.push(' ');
        }
        self.word = false;
        self.buf.push_str(text);
    }

    fn cmd(&mut self, name: &str) {
        self.raw("\\");
        self.buf.push_str(name);
        self.word = name.ends_with(|c: char| c.is_ascii_alphabetic());
    }

    fn ch(&mut self, ch: char) {
        self.raw(ch.encode_utf8(&mut [0; 4]));
    }

    fn scripts(&mut self, sub: Option<&Expr>, sup: Option<&Expr>) {
        if let Some(sub) = sub {
            self.raw("_");
            self.arg(sub);
        }
        if let Some(sup) = sup {
            self.raw("^");
            self.arg(sup);
        }
    }

    fn braced(&mut self, text: &str) {
        self.raw("{");
        self.buf.push_str(text);
        self.buf.push('}');
    }

    /// An argument to a command: braced, unless it is a single digit.
    fn arg(&mut self, expr: &Expr) {
        let inner = to_tex(expr);
        let mut chars = inner.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_digit() => self.raw(&inner),
            _ => self.braced(&inner),
        }
    }

    /// Atom position, where braces would parse as an `Expr::Group`: only non-atoms are braced.
    fn atom(&mut self, expr: &Expr) {
        if is_atom(expr) {
            self.expr(expr);
        } else {
            self.braced(&to_tex(expr));
        }
    }

    /// A run inside a bracketed argument (`\sqrt[3]{x}`), where a brace would be an `Expr::Group`.
    fn bracketed(&mut self, expr: &Expr) {
        self.raw("[");
        let inner = to_tex(expr);
        self.buf.push_str(&inner);
        self.word = ends_in_word_command(&inner);
        self.raw("]");
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Marked { inner, .. } => self.expr(inner),
            Expr::Seq(items) => {
                for item in items {
                    self.expr(item);
                }
            }
            Expr::Sym {
                ch,
                class,
                alphabet,
            } => self.sym(*ch, *class, *alphabet),
            Expr::Space(mu) => match space_name(*mu) {
                Some(name) => self.cmd(name),
                None => {
                    self.cmd("mkern");
                    self.raw(&format!("{mu}mu"));
                }
            },
            Expr::Group(inner) => self.braced(&to_tex(inner)),
            Expr::Func(name) => self.cmd(name),
            Expr::Text(text) => {
                self.cmd("text");
                self.braced(text);
            }
            // `\cancelto{value}{body}` reads as the arrow with the value as its superscript.
            Expr::Scripts {
                base,
                sub: None,
                sup: Some(sup),
            } if matches!(
                peel(base),
                Expr::Enclose {
                    kind: Enclose::CancelTo,
                    ..
                }
            ) =>
            {
                if let Expr::Enclose { body, .. } = peel(base) {
                    self.cmd("cancelto");
                    self.braced(&to_tex(sup));
                    self.braced(&to_tex(body));
                }
            }
            Expr::Scripts { base, sub, sup } => {
                self.atom(base);
                self.scripts(sub.as_deref(), sup.as_deref());
            }
            Expr::Frac {
                num,
                den,
                style,
                bar: true,
            } => {
                self.cmd(frac_name(*style));
                self.arg(num);
                self.arg(den);
            }
            // Barless outside `\binom`: only `\genfrac` spells it without a group around it.
            Expr::Frac {
                num,
                den,
                style,
                bar: false,
            } => {
                self.cmd("genfrac");
                self.raw("{}{}{0pt}");
                self.braced(match style {
                    FracStyle::Auto => "",
                    FracStyle::Display | FracStyle::Continued => "0",
                    FracStyle::Text => "1",
                });
                self.arg(num);
                self.arg(den);
            }
            Expr::Sqrt { degree, radicand } => {
                self.cmd("sqrt");
                if let Some(degree) = degree {
                    self.bracketed(degree);
                }
                self.arg(radicand);
            }
            Expr::Delimited { left, body, right } => self.delimited(*left, body, *right),
            Expr::BigOp {
                kind,
                lower,
                upper,
                limits,
            } => {
                self.cmd(bigop_name(*kind));
                match limits {
                    Limits::Auto => {}
                    Limits::Always => self.cmd("limits"),
                    Limits::Never => self.cmd("nolimits"),
                }
                self.scripts(lower.as_deref(), upper.as_deref());
            }
            Expr::Alphabet(alphabet, inner)
                if matches!(peel(inner), Expr::Text(_))
                    && commands::text_name(*alphabet).is_some() =>
            {
                if let (Some(name), Expr::Text(text)) =
                    (commands::text_name(*alphabet), peel(inner))
                {
                    self.cmd(name);
                    self.braced(text);
                }
            }
            Expr::Alphabet(alphabet, inner) => match alphabet_name(*alphabet) {
                Some(name) => {
                    self.cmd(name);
                    self.arg(inner);
                }
                // No command for this alphabet: drop the wrapper.
                None => self.expr(inner),
            },
            Expr::Class(class, inner) => {
                self.cmd(class_name(*class));
                self.arg(inner);
            }
            Expr::Accent { kind, base } => match kind {
                // `\not` takes the atom that follows, not a braced argument.
                Accent::Not => {
                    self.cmd("not");
                    self.atom(base);
                }
                kind => {
                    self.cmd(accent_name(*kind));
                    self.arg(base);
                }
            },
            Expr::Phantom {
                body,
                width,
                height,
            } => {
                self.cmd(match (width, height) {
                    (false, _) => "vphantom",
                    (_, false) => "hphantom",
                    _ => "phantom",
                });
                self.arg(body);
            }
            Expr::Styled { decor, body } => match decor.colour {
                Some(colour) => {
                    self.cmd("textcolor");
                    self.braced(&hex(colour));
                    self.arg(body);
                }
                None => self.expr(body),
            },
            Expr::Stack {
                over,
                base,
                under,
                class,
            } => self.stack(over.as_deref(), base, under.as_deref(), *class),
            Expr::XArrow { ch, over, under } => {
                self.cmd(xarrow_name(*ch));
                if let Some(under) = under {
                    self.bracketed(under);
                }
                match over {
                    Some(over) => self.arg(over),
                    // Explicit empty argument, else the next token is swallowed.
                    None => self.raw("{}"),
                }
            }
            Expr::Env(kind, rows) => self.env(*kind, rows),
            Expr::Style { style, body } => {
                self.cmd(match style {
                    StyleSwitch::Display => "displaystyle",
                    StyleSwitch::Text => "textstyle",
                    StyleSwitch::Script => "scriptstyle",
                    StyleSwitch::ScriptScript => "scriptscriptstyle",
                    StyleSwitch::Size(size) => size.name(),
                });
                self.expr(body);
            }
            Expr::Rule {
                width,
                height,
                raise,
            } => {
                self.cmd("rule");
                if *raise != 0.0 {
                    self.raw(&format!("[{raise}em]"));
                }
                self.raw(&format!("{{{width}em}}{{{height}em}}"));
            }
            Expr::Adjust { adjust, body } => match adjust {
                Adjust::Smash { top, bottom } => {
                    self.cmd("smash");
                    match (top, bottom) {
                        (true, false) => self.raw("[t]"),
                        (false, true) => self.raw("[b]"),
                        _ => {}
                    }
                    self.braced(&to_tex(body));
                }
                Adjust::Lap(lap) => {
                    self.cmd(match lap {
                        Lap::Left => "mathllap",
                        Lap::Right => "mathrlap",
                        Lap::Centre => "mathclap",
                    });
                    self.braced(&to_tex(body));
                }
                Adjust::Raise(em) => {
                    self.cmd("raisebox");
                    self.raw(&format!("{{{em}em}}"));
                    self.braced(&to_tex(body));
                }
            },
            Expr::Enclose { kind, body } => self.enclose(*kind, body),
            Expr::HLine { dashed } => self.cmd(if *dashed { "hdashline" } else { "hline" }),
            Expr::Middle(delim) => {
                self.cmd("middle");
                self.delim(*delim);
            }
            Expr::SizedDelim { ch, class, size } => {
                let stem = match size {
                    DelimSize::Big => "big",
                    DelimSize::BigCap => "Big",
                    DelimSize::Bigg => "bigg",
                    DelimSize::BiggCap => "Bigg",
                };
                let suffix = match class {
                    AtomClass::Open => "l",
                    AtomClass::Close => "r",
                    AtomClass::Rel => "m",
                    _ => "",
                };
                self.cmd(&format!("{stem}{suffix}"));
                self.delim(Delim::Char(*ch));
            }
        }
    }

    fn sym(&mut self, ch: char, class: AtomClass, alphabet: Alphabet) {
        if alphabet == Alphabet::Italic && class == AtomClass::Ord {
            if let Some(name) = commands::var_greek_name(ch) {
                return self.cmd(name);
            }
        }
        if let Some(name) = alphabet_name(alphabet) {
            self.cmd(name);
            self.raw("{");
            self.sym(ch, class, Alphabet::Auto);
            return self.raw("}");
        }

        // Undo the substitutions `math_char` makes.
        if class == AtomClass::Bin {
            match ch {
                '\u{2212}' => return self.raw("-"),
                '\u{2217}' => return self.raw("*"),
                _ => {}
            }
        }

        if let Some(escaped) = plain(ch, class) {
            return self.raw(escaped);
        }
        // ASCII only, and never `\` (starts a command), `'` (becomes a prime) or `~` (a space).
        let literal = ch.is_ascii()
            && !matches!(ch, '\\' | '\'' | '~')
            && math_char(ch) == ch
            && class_of_char(ch) == class;
        match symbols::spell(ch, class) {
            Some(name) if !literal => self.cmd(name),
            _ => self.ch(ch),
        }
    }

    fn enclose(&mut self, kind: Enclose, body: &Expr) {
        match kind {
            Enclose::Frame => {
                if let Expr::Text(text) = peel(body) {
                    self.cmd("fbox");
                    return self.braced(text);
                }
                self.cmd("boxed");
                self.braced(&to_tex(body));
            }
            Enclose::Fill(fill) => {
                self.cmd("colorbox");
                self.braced(&hex(fill));
                self.braced(&text_arg(body));
            }
            Enclose::FramedFill { frame, fill } => {
                self.cmd("fcolorbox");
                self.braced(&hex(frame));
                self.braced(&hex(fill));
                self.braced(&text_arg(body));
            }
            kind => {
                self.cmd(match kind {
                    Enclose::BackCancel => "bcancel",
                    Enclose::CrossCancel => "xcancel",
                    Enclose::StrikeOut => "sout",
                    // `\cancelto` with no value has no spelling of its own.
                    _ => "cancel",
                });
                self.braced(&to_tex(body));
            }
        }
    }

    fn delimited(&mut self, left: Delim, body: &Expr, right: Delim) {
        // `\binom` parses to a barless fraction inside parentheses.
        if let (Delim::Char('('), Delim::Char(')')) = (left, right) {
            if let Expr::Frac {
                num,
                den,
                style,
                bar: false,
            } = peel(body)
            {
                self.cmd(binom_name(*style));
                self.arg(num);
                self.arg(den);
                return;
            }
        }
        self.cmd("left");
        self.delim(left);
        self.expr(body);
        self.cmd("right");
        self.delim(right);
    }

    fn delim(&mut self, delim: Delim) {
        let ch = match delim {
            Delim::None => return self.raw("."),
            Delim::Char(ch) => ch,
        };
        // A bare `{` after `\left` would be grouping.
        if let Some(escaped) = plain(ch, class_of_char(ch)) {
            return self.raw(escaped);
        }
        // Brackets, bars and the slash are written as themselves.
        if matches!(ch, '(' | ')' | '[' | ']' | '|' | '/') {
            return self.ch(ch);
        }
        let name = symbols::spell(ch, AtomClass::Ord)
            .or_else(|| symbols::spell(ch, AtomClass::Open))
            .or_else(|| symbols::spell(ch, AtomClass::Close))
            .or_else(|| symbols::spell(ch, AtomClass::Rel));
        match name {
            Some(name) => self.cmd(name),
            None => self.ch(ch),
        }
    }

    fn stack(
        &mut self,
        over: Option<&Expr>,
        base: &Expr,
        under: Option<&Expr>,
        class: Option<AtomClass>,
    ) {
        // `\stackrel` forces a relation; `\overset` keeps the base class.
        match (over, under) {
            (Some(over), None) => {
                self.cmd(if class == Some(AtomClass::Rel) {
                    "stackrel"
                } else {
                    "overset"
                });
                self.arg(over);
                self.arg(base);
            }
            (None, Some(under)) => {
                self.cmd("underset");
                self.arg(under);
                self.arg(base);
            }
            // No command sets both, so they nest.
            (Some(over), Some(under)) => {
                self.cmd("overset");
                self.arg(over);
                self.raw("{");
                self.stack(None, base, Some(under), None);
                self.raw("}");
            }
            (None, None) => self.expr(base),
        }
    }

    fn env(&mut self, kind: EnvKind, rows: &[Vec<Expr>]) {
        // Closed by a brace rather than `\end`, and its `&` is ordinary.
        if kind == EnvKind::SubStack {
            self.cmd("substack");
            let mut inner = String::new();
            for (index, row) in rows.iter().enumerate() {
                if index > 0 {
                    inner.push_str(" \\\\ ");
                }
                for cell in row {
                    write_tex(cell, &mut inner);
                }
            }
            self.braced(&inner);
            return;
        }

        let name = env_name(kind);
        self.cmd("begin");
        self.braced(name);
        match kind {
            EnvKind::Array(spec) => self.braced(&spec.to_spec()),
            EnvKind::SubArray(align) => self.braced(&align.letter().to_string()),
            EnvKind::AlignedMatrix { align, .. } => self.raw(&format!("[{}]", align.letter())),
            _ => {}
        }
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                self.raw(" \\\\ ");
            }
            for (index, cell) in row.iter().enumerate() {
                if index > 0 {
                    self.raw(" & ");
                }
                self.expr(cell);
            }
        }
        self.raw(" ");
        self.cmd("end");
        self.braced(name);
    }
}

/// A text-mode argument: prose as it is, maths between dollars.
fn text_arg(expr: &Expr) -> String {
    let items = match peel(expr) {
        Expr::Seq(items) => items.as_slice(),
        other => core::slice::from_ref(other),
    };
    let mut out = String::new();
    for item in items {
        match peel(item) {
            Expr::Text(text) => out.push_str(text),
            math => {
                out.push('$');
                write_tex(math, &mut out);
                out.push('$');
            }
        }
    }
    out
}

/// Strip spans and single-item runs.
fn peel(expr: &Expr) -> &Expr {
    match expr {
        Expr::Marked { inner, .. } => peel(inner),
        Expr::Seq(items) => match items.as_slice() {
            [single] => peel(single),
            _ => expr,
        },
        other => other,
    }
}

/// Whether `parse_atom` alone would produce this expression.
fn is_atom(expr: &Expr) -> bool {
    match expr {
        Expr::Marked { inner, .. } => is_atom(inner),
        // A run of one is that one; bracing it would turn `x^2` into `{x}^2`.
        Expr::Seq(items) => match items.as_slice() {
            [single] => is_atom(single),
            _ => false,
        },
        Expr::Scripts { .. } => false,
        Expr::BigOp {
            lower,
            upper,
            limits,
            ..
        } => lower.is_none() && upper.is_none() && *limits == Limits::Auto,
        _ => true,
    }
}

fn ends_in_word_command(text: &str) -> bool {
    if !text.ends_with(|c: char| c.is_ascii_alphabetic()) {
        return false;
    }
    let letters = text.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    letters.ends_with('\\') && !letters.ends_with("\\\\")
}

/// The backslash escape of a TeX special character standing for itself.
fn plain(ch: char, class: AtomClass) -> Option<&'static str> {
    if class_of_char(ch) != class || math_char(ch) != ch {
        return None;
    }
    Some(match ch {
        '{' => r"\{",
        '}' => r"\}",
        '$' => r"\$",
        '%' => r"\%",
        '#' => r"\#",
        '&' => r"\&",
        '_' => r"\_",
        '^' => r"\^",
        _ => return None,
    })
}

fn hex(paint: Paint) -> String {
    let [r, g, b, a] = paint.rgba;
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

fn frac_name(style: FracStyle) -> &'static str {
    match style {
        FracStyle::Auto => "frac",
        FracStyle::Display => "dfrac",
        FracStyle::Text => "tfrac",
        FracStyle::Continued => "cfrac",
    }
}

fn binom_name(style: FracStyle) -> &'static str {
    match style {
        FracStyle::Auto => "binom",
        FracStyle::Display => "dbinom",
        FracStyle::Text => "tbinom",
        // No continued binomial; the display one is closest.
        FracStyle::Continued => "dbinom",
    }
}

fn alphabet_name(alphabet: Alphabet) -> Option<&'static str> {
    Some(match alphabet {
        Alphabet::Bold => "mathbf",
        Alphabet::Italic => "mathit",
        Alphabet::BoldItalic => "mathbfit",
        Alphabet::SansSerifBold => "mathbfsf",
        Alphabet::SansSerifItalic => "mathsfit",
        Alphabet::BoldScript => "mathbfscr",
        Alphabet::BoldFraktur => "mathbffrak",
        Alphabet::Upright => "mathrm",
        Alphabet::SansSerif => "mathsf",
        Alphabet::Monospace => "mathtt",
        Alphabet::Script => "mathcal",
        Alphabet::Fraktur => "mathfrak",
        Alphabet::DoubleStruck => "mathbb",
        // `Auto`, and those with no command in this parser.
        _ => return None,
    })
}

fn class_name(class: AtomClass) -> &'static str {
    match class {
        AtomClass::Ord => "mathord",
        AtomClass::Op => "mathop",
        AtomClass::Bin => "mathbin",
        AtomClass::Rel => "mathrel",
        AtomClass::Open => "mathopen",
        AtomClass::Close => "mathclose",
        AtomClass::Punct => "mathpunct",
        AtomClass::Inner => "mathinner",
    }
}

fn accent_name(kind: Accent) -> &'static str {
    match kind {
        Accent::Hat => "hat",
        Accent::WideHat => "widehat",
        Accent::Tilde => "tilde",
        Accent::WideTilde => "widetilde",
        Accent::Bar => "bar",
        Accent::Vec => "vec",
        Accent::Dot => "dot",
        Accent::DDot => "ddot",
        Accent::DDDot => "dddot",
        Accent::DDDDot => "ddddot",
        Accent::Acute => "acute",
        Accent::Grave => "grave",
        Accent::Breve => "breve",
        Accent::Check => "check",
        Accent::Ring => "mathring",
        Accent::Overline => "overline",
        Accent::Underline => "underline",
        Accent::OverBrace => "overbrace",
        Accent::UnderBrace => "underbrace",
        Accent::OverBracket => "overbracket",
        Accent::UnderBracket => "underbracket",
        Accent::OverRightArrow => "overrightarrow",
        Accent::OverLeftArrow => "overleftarrow",
        Accent::OverLeftRightArrow => "overleftrightarrow",
        Accent::UnderRightArrow => "underrightarrow",
        Accent::UnderLeftArrow => "underleftarrow",
        Accent::Not => "not",
        Accent::DoubleAcute => "H",
        Accent::WideCheck => "widecheck",
        Accent::UnderTilde => "utilde",
        Accent::UnderLeftRightArrow => "underleftrightarrow",
        Accent::OverLeftHarpoon => "overleftharpoon",
        Accent::OverRightHarpoon => "overrightharpoon",
        Accent::OverRightDoubleArrow => "Overrightarrow",
        Accent::OverGroup => "overgroup",
        Accent::UnderGroup => "undergroup",
        Accent::OverLineSegment => "overlinesegment",
        Accent::UnderLineSegment => "underlinesegment",
    }
}

fn bigop_name(kind: BigOpKind) -> &'static str {
    match kind {
        BigOpKind::Sum => "sum",
        BigOpKind::Product => "prod",
        BigOpKind::Coproduct => "coprod",
        BigOpKind::Integral => "int",
        BigOpKind::DoubleIntegral => "iint",
        BigOpKind::TripleIntegral => "iiint",
        BigOpKind::QuadrupleIntegral => "iiiint",
        BigOpKind::ContourIntegral => "oint",
        BigOpKind::SurfaceIntegral => "oiint",
        BigOpKind::VolumeIntegral => "oiiint",
        BigOpKind::Union => "bigcup",
        BigOpKind::Intersection => "bigcap",
        BigOpKind::Disjunction => "bigvee",
        BigOpKind::Conjunction => "bigwedge",
        BigOpKind::DirectSum => "bigoplus",
        BigOpKind::TensorProduct => "bigotimes",
        BigOpKind::CircledDot => "bigodot",
        BigOpKind::SquareUnion => "bigsqcup",
        BigOpKind::MultisetUnion => "biguplus",
        BigOpKind::ClockwiseContourIntegral => "ointclockwise",
        BigOpKind::AnticlockwiseContourIntegral => "ointctrclockwise",
        BigOpKind::VarLimInf => "varliminf",
        BigOpKind::VarLimSup => "varlimsup",
        BigOpKind::VarInjLim => "varinjlim",
        BigOpKind::VarProjLim => "varprojlim",
        // Two-word names are spelled as one command.
        BigOpKind::Limit(name) => match name {
            "lim sup" => "limsup",
            "lim inf" => "liminf",
            "inj lim" => "injlim",
            "proj lim" => "projlim",
            name => name,
        },
    }
}

fn xarrow_name(ch: char) -> &'static str {
    match ch {
        '\u{2190}' => "xleftarrow",
        '\u{2194}' => "xleftrightarrow",
        '\u{21D2}' => "xRightarrow",
        '\u{21D0}' => "xLeftarrow",
        '\u{21D4}' => "xLeftrightarrow",
        '\u{21A6}' => "xmapsto",
        '\u{21AA}' => "xhookrightarrow",
        '\u{21A9}' => "xhookleftarrow",
        '\u{21A0}' => "xtwoheadrightarrow",
        '\u{21C0}' => "xrightharpoonup",
        '\u{21BD}' => "xleftharpoondown",
        '\u{21C1}' => "xrightharpoondown",
        '\u{21BC}' => "xleftharpoonup",
        '\u{21CC}' => "xrightleftharpoons",
        '\u{21CB}' => "xleftrightharpoons",
        '\u{219E}' => "xtwoheadleftarrow",
        '\u{21C4}' => "xtofrom",
        '=' => "xequal",
        _ => "xrightarrow",
    }
}

fn env_name(kind: EnvKind) -> &'static str {
    match kind {
        EnvKind::Aligned => "aligned",
        EnvKind::Gathered => "gathered",
        EnvKind::Cases => "cases",
        EnvKind::DCases => "dcases",
        EnvKind::RCases => "rcases",
        EnvKind::DRCases => "drcases",
        EnvKind::SubStack => "substack",
        EnvKind::Array(spec) if spec.display => "darray",
        EnvKind::Array(_) => "array",
        EnvKind::SubArray(_) => "subarray",
        EnvKind::AlignedMatrix { delims, .. } => match delims {
            Some(('(', ')')) => "pmatrix*",
            Some(('[', ']')) => "bmatrix*",
            Some(('{', '}')) => "Bmatrix*",
            Some(('|', '|')) => "vmatrix*",
            Some(('\u{2016}', '\u{2016}')) => "Vmatrix*",
            _ => "matrix*",
        },
        EnvKind::Matrix { delims } => match delims {
            Some(('(', ')')) => "pmatrix",
            Some(('[', ']')) => "bmatrix",
            Some(('{', '}')) => "Bmatrix",
            Some(('|', '|')) => "vmatrix",
            Some(('\u{2016}', '\u{2016}')) => "Vmatrix",
            _ => "matrix",
        },
    }
}

/// The named space exactly `mu` wide.
fn space_name(mu: i16) -> Option<&'static str> {
    Some(match mu {
        -5 => "negthickspace",
        -4 => "negmedspace",
        -3 => "negthinspace",
        3 => "thinspace",
        4 => "medspace",
        5 => "thickspace",
        6 => "space",
        9 => "enspace",
        18 => "quad",
        36 => "qquad",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(src: &str) {
        let Ok(expr) = crate::parse(src) else {
            panic!("{src:?} does not parse")
        };
        let written = to_tex(&expr);
        match crate::parse(&written) {
            Ok(again) => assert_eq!(expr, again, "{src:?} became {written:?}"),
            Err(e) => panic!("{src:?} became {written:?}, which does not parse: {e}"),
        }
    }

    #[test]
    fn style_switches_and_sized_delimiters_survive_a_round_trip() {
        round_trip(r"{\textstyle a b}");
        round_trip(r"x {\displaystyle \frac{a}{b}} y");
        round_trip(r"\bigl( x \bigr)");
        round_trip(r"\Biggm| y");
        round_trip(r"{1 \over 2}");
    }

    #[test]
    fn a_binomial_is_not_written_as_a_fraction_in_brackets() {
        assert_eq!(
            to_tex(&crate::parse(r"\binom{n}{k}").unwrap()),
            r"\binom{n}{k}"
        );
        round_trip(r"\binom{n}{k}");
        round_trip(r"\dbinom{n}{k}");
        round_trip(r"\tbinom{n}{k}");
    }

    #[test]
    fn a_script_base_keeps_its_bracing() {
        assert_eq!(to_tex(&crate::parse("x^2").unwrap()), "x^2");
        round_trip("x^2");
        round_trip("{x}^2");
        round_trip(r"\frac{a}{b}^2");
    }

    #[test]
    fn a_word_command_is_separated_from_a_letter() {
        assert_eq!(to_tex(&crate::parse(r"\alpha x").unwrap()), r"\alpha x");
        assert_eq!(to_tex(&crate::parse(r"\alpha 1").unwrap()), r"\alpha1");
        round_trip(r"\alpha x");
        assert_eq!(to_tex(&crate::parse(r"\sqrt x").unwrap()), r"\sqrt{x}");
        round_trip(r"\sqrt x");
    }

    #[test]
    fn the_shapes_with_their_own_readers_survive() {
        round_trip(r"\substack{a \\ b}");
        round_trip(r"\left\{ x \right\}");
        round_trip(r"\left( x \right.");
        round_trip(r"\sqrt[3]{x}");
        round_trip(r"\xrightarrow[f]{g}");
        round_trip(r"\xrightarrow{}");
        round_trip(r"\not=");
        round_trip(r"\text{ if }");
        round_trip(r"\textcolor{#e0a030}{x}");
    }

    #[test]
    fn spacing_keeps_its_width() {
        for (src, mu) in [(r"\,", 3), (r"\!", -3), (r"\quad", 18), (r"\qquad", 36)] {
            let expr = crate::parse(src).unwrap();
            assert_eq!(expr, Expr::Space(mu));
            round_trip(src);
        }
    }
}
