//! Parses LaTeX maths tokens into an [`Expr`] tree.

use crate::ast::{
    Accent, Alphabet, ArraySpec, BigOpKind, ColumnAlign, Decor, Delim, DelimSize, EnvKind, Expr,
    FracStyle, Limits, SizeSwitch, StyleSwitch,
};
#[cfg(feature = "formatting")]
use crate::ast::{Adjust, Enclose, Lap};
use crate::attrs::{Paint, Span};
use crate::class::{AtomClass, class_of_char, math_char};
use crate::colour;
use crate::commands::{self, MathFunction};
use crate::error::Error;
use crate::symbols;
use crate::tokenizer::{Spanned, Token};

fn is_empty(expr: &Expr) -> bool {
    match expr {
        Expr::Seq(items) => items.is_empty(),
        Expr::Group(inner) | Expr::Marked { inner, .. } => is_empty(inner),
        _ => false,
    }
}

fn delim_char(c: char) -> Option<char> {
    match c {
        '(' | ')' | '[' | ']' | '{' | '}' | '|' | '/' => Some(c),
        // TeX reads `\left<` as an angle bracket.
        '<' => Some('\u{27E8}'),
        '>' => Some('\u{27E9}'),
        _ => None,
    }
}

/// The arrows that stretch vertically, the only relations that can be delimiters.
fn is_vertical_arrow(c: char) -> bool {
    matches!(
        c,
        '\u{2191}' | '\u{2193}' | '\u{2195}' | '\u{21D1}' | '\u{21D3}' | '\u{21D5}'
    )
}

/// A barless fraction in a pair of delimiters: `\binom`, `\choose`, `\brace`, `\brack`.
fn fenced(left: char, frac: Expr, right: char) -> Expr {
    Expr::Delimited {
        left: Delim::Char(left),
        body: Box::new(frac),
        right: Delim::Char(right),
    }
}

/// Old-style font switches (`\rm`, `\bf`, ...).
fn alphabet_switch(name: &str) -> Option<Alphabet> {
    Some(match name {
        "rm" => Alphabet::Upright,
        "bf" | "boldmath" => Alphabet::Bold,
        "it" | "mit" => Alphabet::Italic,
        "cal" => Alphabet::Script,
        "tt" => Alphabet::Monospace,
        "sf" => Alphabet::SansSerif,
        "frak" => Alphabet::Fraktur,
        _ => return None,
    })
}

fn style_switch(name: &str) -> Option<StyleSwitch> {
    Some(match name {
        "displaystyle" => StyleSwitch::Display,
        "textstyle" => StyleSwitch::Text,
        "scriptstyle" => StyleSwitch::Script,
        "scriptscriptstyle" => StyleSwitch::ScriptScript,
        #[cfg(feature = "formatting")]
        _ => return SizeSwitch::from_name(name).map(StyleSwitch::Size),
        #[cfg(not(feature = "formatting"))]
        _ => return None,
    })
}

/// `\big` family: the stem picks the size, the suffix (`l`/`r`/`m`/none) the class.
fn sized_delim_command(name: &str) -> Option<(DelimSize, AtomClass)> {
    let (stem, class) = match name.strip_suffix(['l', 'r', 'm']) {
        Some(stem) => {
            let class = match name.as_bytes().last() {
                Some(b'l') => AtomClass::Open,
                Some(b'r') => AtomClass::Close,
                _ => AtomClass::Rel,
            };
            (stem, class)
        }
        None => (name, AtomClass::Ord),
    };
    let size = match stem {
        "big" => DelimSize::Big,
        "Big" => DelimSize::BigCap,
        "bigg" => DelimSize::Bigg,
        "Bigg" => DelimSize::BiggCap,
        _ => return None,
    };
    Some((size, class))
}

#[derive(Debug)]
pub(crate) struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Spanned<'a>>,
    pos: usize,
    depth: usize,
    spans: bool,
    /// How many `\left`s are open, so `\middle` knows whether it has a pair to grow with.
    lefts: usize,
    /// How many environments are open, so `\hline` knows whether it has rows to rule.
    envs: usize,
}

/// Recursion limit; exceeded only by pathological input.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunScope {
    /// A braced group or the whole formula: `}`, `\right`, end of input.
    Group,
    /// An environment cell: the group terminators plus `&`, `\\` and `\end`.
    Cell,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(src: &'a str, tokens: Vec<Spanned<'a>>) -> Parser<'a> {
        Parser {
            src,
            tokens,
            pos: 0,
            depth: 0,
            spans: false,
            lefts: 0,
            envs: 0,
        }
    }

    /// Record the source range of every atom, as [`Expr::Marked`] wrappers.
    pub(crate) fn with_spans(mut self, spans: bool) -> Parser<'a> {
        self.spans = spans;
        self
    }

    fn marked(&self, start: usize, expr: Expr) -> Expr {
        if !self.spans {
            return expr;
        }
        let end = self.consumed_end(start);
        if end <= start {
            return expr;
        }
        Expr::Marked {
            span: Span::new(start as u32, end as u32),
            inner: Box::new(expr),
        }
    }

    fn atom_start(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map_or(self.src.len(), |s| s.start)
    }

    /// One past the last byte an atom consumed.
    /// A partly consumed literal token stays in place, so its start is the end.
    fn consumed_end(&self, start: usize) -> usize {
        let previous = self
            .pos
            .checked_sub(1)
            .and_then(|i| self.tokens.get(i))
            .map_or(0, |s| s.end);
        if previous > start {
            return previous;
        }
        self.atom_start().max(start)
    }

    /// Parse the whole token stream, which must be consumed entirely.
    pub(crate) fn parse(mut self) -> Result<Expr, Error> {
        let expr = self.parse_expr()?;
        match self.peek() {
            None => Ok(expr),
            Some(tok) => Err(self.err(format!("trailing {tok:?}"))),
        }
    }

    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.pos).map(|s| &s.tok)
    }

    fn bump(&mut self) -> Option<Token<'a>> {
        let tok = self.tokens.get(self.pos).map(|s| s.tok);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    /// The byte offset to blame for an error raised while looking *ahead*.
    fn here(&self) -> usize {
        self.tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .map_or(0, |s| s.start)
    }

    fn err(&self, msg: impl Into<String>) -> Error {
        Error::parse(msg, self.here())
    }

    /// The byte offset of the token just consumed.
    fn err_consumed(&self, msg: impl Into<String>) -> Error {
        let at = self
            .pos
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map_or_else(|| self.here(), |spanned| spanned.start);
        Error::parse(msg, at)
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(Error::TooDeep);
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn parse_expr(&mut self) -> Result<Expr, Error> {
        self.parse_run(RunScope::Group)
    }

    fn parse_run(&mut self, scope: RunScope) -> Result<Expr, Error> {
        self.enter()?;
        let mut items: Vec<Expr> = Vec::new();
        loop {
            match self.peek() {
                None | Some(Token::RBrace) => break,
                Some(Token::Command(cmd)) if *cmd == "right" => break,
                Some(Token::RowBreak | Token::Align) if scope == RunScope::Cell => break,
                Some(Token::Command(cmd)) if scope == RunScope::Cell && *cmd == "end" => break,
                // `\color` is a switch: it takes the rest of the run as its body.
                Some(Token::Command(cmd)) if *cmd == "color" => {
                    let start = self.atom_start();
                    self.pos += 1;
                    let decor = Decor::colour(self.read_colour()?);
                    let body = self.parse_run(scope)?;
                    let styled = Expr::Styled {
                        decor,
                        body: Box::new(body),
                    };
                    items.push(self.marked(start, styled));
                    break;
                }
                Some(Token::Command(cmd)) if style_switch(cmd).is_some() => {
                    let start = self.atom_start();
                    let style = style_switch(cmd).unwrap_or(StyleSwitch::Text);
                    self.pos += 1;
                    let body = self.parse_run(scope)?;
                    let switched = Expr::Style {
                        style,
                        body: Box::new(body),
                    };
                    items.push(self.marked(start, switched));
                    break;
                }
                Some(Token::Command(cmd)) if alphabet_switch(cmd).is_some() => {
                    let start = self.atom_start();
                    let alphabet = alphabet_switch(cmd).unwrap_or(Alphabet::Upright);
                    self.pos += 1;
                    let body = self.parse_run(scope)?;
                    items.push(self.marked(start, Expr::Alphabet(alphabet, Box::new(body))));
                    break;
                }
                // `{a \over b}`: the run so far is the numerator, the rest the denominator.
                Some(Token::Command(cmd))
                    if matches!(*cmd, "over" | "choose" | "atop" | "brace" | "brack") =>
                {
                    let cmd = *cmd;
                    self.pos += 1;
                    let num = flatten(std::mem::take(&mut items));
                    let den = self.parse_run(scope)?;
                    let frac = Expr::Frac {
                        num: Box::new(num),
                        den: Box::new(den),
                        style: FracStyle::Auto,
                        bar: cmd == "over",
                    };
                    items.push(match cmd {
                        "choose" => fenced('(', frac, ')'),
                        "brace" => fenced('{', frac, '}'),
                        "brack" => fenced('[', frac, ']'),
                        _ => frac,
                    });
                    break;
                }
                _ => {}
            }
            let start = self.atom_start();
            // Inside a group a leading script hangs off an empty base, as in TeX; at top level it is an error.
            let item = if self.depth > 1
                && matches!(self.peek(), Some(Token::Underscore | Token::Caret))
            {
                self.attach_scripts(Expr::Seq(Vec::new()))?
            } else {
                let atom = self.parse_atom()?;
                self.attach_scripts(atom)?
            };
            items.push(self.marked(start, item));
        }
        self.leave();
        Ok(flatten(items))
    }

    /// One stretch of a cell. A `}` or `\right` would end it having read nothing, forever.
    fn parse_cell(&mut self) -> Result<Expr, Error> {
        let before = self.pos;
        let run = self.parse_run(RunScope::Cell)?;
        if self.pos == before {
            return Err(match self.peek() {
                Some(Token::RBrace) => self.err("unmatched '}'"),
                _ => self.err("\\right without \\left"),
            });
        }
        Ok(run)
    }

    fn read_colour(&mut self) -> Result<Paint, Error> {
        let at = self.here();
        let spec = self.parse_raw_group()?;
        colour::parse(&spec).ok_or_else(|| {
            Error::parse(
                format!("unknown colour {spec:?}: expected a name, #rrggbb or rgb(r, g, b)"),
                at,
            )
        })
    }

    /// A text-mode argument, `{prose $maths$ prose}`: the prose verbatim, the maths parsed.
    #[cfg(feature = "formatting")]
    fn parse_text_arg(&mut self) -> Result<Expr, Error> {
        let open = self.here() + 1;
        let raw = self.parse_raw_group()?;
        let mut items = Vec::new();
        let mut offset = 0;
        for (index, piece) in raw.split('$').enumerate() {
            if index % 2 == 0 {
                if !piece.is_empty() {
                    items.push(Expr::Text(piece.to_owned()));
                }
            } else {
                let tokens = crate::tokenizer::Tokenizer::new(piece).tokenize();
                let maths = Parser::new(piece, tokens)
                    .parse()
                    .map_err(|error| match error {
                        Error::Parse {
                            message,
                            offset: at,
                        } => Error::parse(message, open + offset + 1 + at),
                        other => other,
                    })?;
                items.push(maths);
            }
            offset += piece.len() + 1;
        }
        if raw.split('$').count() % 2 == 0 {
            return Err(Error::parse("an unclosed '$'", open + raw.len()));
        }
        Ok(match items.len() {
            0 => Expr::Text(String::new()),
            _ => flatten(items),
        })
    }

    /// An optional `[...]` argument, verbatim.
    fn parse_raw_bracket(&mut self) -> Result<Option<String>, Error> {
        let Some(Spanned {
            tok: Token::Char('['),
            end: open,
            ..
        }) = self.tokens.get(self.pos).copied()
        else {
            return Ok(None);
        };
        let mut index = self.pos + 1;
        loop {
            match self.tokens.get(index) {
                None => return Err(self.err("expected ']'")),
                Some(Spanned {
                    tok: Token::Char(']'),
                    start,
                    ..
                }) => {
                    let raw = self.src.get(open..*start).unwrap_or_default().to_owned();
                    self.pos = index + 1;
                    return Ok(Some(raw));
                }
                Some(_) => index += 1,
            }
        }
    }

    fn attach_scripts(&mut self, base: Expr) -> Result<Expr, Error> {
        let base = self.attach_limits(base);

        let mut sub: Option<Expr> = None;
        // `f'` is `f^\prime`; primes may share the superscript with one explicit `^` (texvc's `g'^{2}`).
        let mut sup = self.take_primes();
        let mut primed = sup.is_some();
        loop {
            match self.peek() {
                Some(Token::Underscore) if sub.is_none() => {
                    self.pos += 1;
                    sub = Some(self.parse_script_arg()?);
                }
                Some(Token::Caret) if sup.is_none() || primed => {
                    self.pos += 1;
                    let arg = self.parse_script_arg()?;
                    sup = Some(match sup.take() {
                        Some(primes) => flatten(vec![primes, arg]),
                        None => arg,
                    });
                    primed = false;
                }
                Some(Token::Underscore | Token::Caret) => {
                    return Err(self.err("double subscript or superscript"));
                }
                _ => break,
            }
        }

        // Big operators take their scripts as limits.
        if let Expr::BigOp {
            kind,
            lower: None,
            upper: None,
            limits,
        } = base
        {
            return Ok(Expr::BigOp {
                kind,
                lower: sub.map(Box::new),
                upper: sup.map(Box::new),
                limits,
            });
        }

        // A horizontal brace stacks its script as a label.
        if let Expr::Accent { kind, .. } = &base {
            if kind.takes_limits() && (sub.is_some() || sup.is_some()) {
                let under = kind.is_under();
                // Only the script on the brace's own side becomes the label.
                let (label, other) = if under { (sub, sup) } else { (sup, sub) };
                let base = match other.map(Box::new) {
                    None => base,
                    Some(other) if under => Expr::Scripts {
                        base: Box::new(base),
                        sub: None,
                        sup: Some(other),
                    },
                    Some(other) => Expr::Scripts {
                        base: Box::new(base),
                        sub: Some(other),
                        sup: None,
                    },
                };
                let label = label.map(Box::new);
                let (over, under) = if under { (None, label) } else { (label, None) };
                return Ok(Expr::Stack {
                    over,
                    base: Box::new(base),
                    under,
                    class: None,
                });
            }
        }

        Ok(match (sub, sup) {
            (None, None) => base,
            (sub, sup) => Expr::Scripts {
                base: Box::new(base),
                sub: sub.map(Box::new),
                sup: sup.map(Box::new),
            },
        })
    }

    /// `\limits` / `\nolimits` come before the scripts they govern.
    fn attach_limits(&mut self, base: Expr) -> Expr {
        let Expr::BigOp {
            kind, lower, upper, ..
        } = base
        else {
            return base;
        };
        let mut limits = Limits::Auto;
        while let Some(Token::Command(cmd)) = self.peek() {
            match *cmd {
                "limits" => limits = Limits::Always,
                "nolimits" => limits = Limits::Never,
                _ => break,
            }
            self.pos += 1;
        }
        Expr::BigOp {
            kind,
            lower,
            upper,
            limits,
        }
    }

    fn take_primes(&mut self) -> Option<Expr> {
        let mut primes: Vec<Expr> = Vec::new();
        while matches!(self.peek(), Some(Token::Char('\''))) {
            self.pos += 1;
            primes.push(Expr::Sym {
                ch: '\u{2032}',
                class: AtomClass::Ord,
                alphabet: Alphabet::Auto,
            });
        }
        match primes.len() {
            0 => None,
            _ => Some(flatten(primes)),
        }
    }

    fn parse_script_arg(&mut self) -> Result<Expr, Error> {
        match self.peek() {
            Some(Token::LBrace | Token::Command(_) | Token::Literal(_) | Token::Char(_)) => {
                self.parse_group()
            }
            _ => Err(self.err("expected a subscript or superscript")),
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, Error> {
        self.enter()?;
        let atom = self.parse_atom_inner();
        self.leave();
        atom
    }

    fn parse_atom_inner(&mut self) -> Result<Expr, Error> {
        // Scripts attach to one character: `12^3` is 1 then 2 cubed, `\frac12` is one half.
        if matches!(self.peek(), Some(Token::Literal(_))) {
            return Ok(self.take_one_literal_char());
        }
        match self.bump() {
            Some(Token::Space(mu)) => Ok(Expr::Space(mu)),
            Some(Token::Char(c)) => Ok(Expr::Sym {
                ch: math_char(c),
                class: class_of_char(c),
                alphabet: Alphabet::Auto,
            }),
            Some(Token::LBrace) => {
                self.pos -= 1;
                Ok(Expr::Group(Box::new(self.parse_group()?)))
            }
            Some(Token::Command(name)) => self.parse_command(name),
            Some(Token::RowBreak) => Err(self.err_consumed("a row break outside an environment")),
            Some(Token::Align) => {
                Err(self.err_consumed("an alignment marker outside an environment"))
            }
            Some(Token::Caret | Token::Underscore) => {
                Err(self.err_consumed("a script with nothing to attach to"))
            }
            Some(Token::RBrace) => Err(self.err("unmatched '}'")),
            Some(Token::Literal(_)) | None => Err(self.err("unexpected end of expression")),
        }
    }

    /// Take one character off the front of a `Literal`, leaving the rest in place.
    fn take_one_literal_char(&mut self) -> Expr {
        let mut ch = char::REPLACEMENT_CHARACTER;
        if let Some(spanned) = self.tokens.get_mut(self.pos) {
            if let Token::Literal(s) = &mut spanned.tok {
                ch = s.chars().next().unwrap_or(ch);
                // Literals are ASCII, so one byte is one char.
                if s.len() > 1 {
                    *s = s.get(1..).unwrap_or_default();
                    spanned.start += 1;
                } else {
                    self.pos += 1;
                }
            }
        }
        Expr::Sym {
            ch,
            class: class_of_char(ch),
            alphabet: Alphabet::Auto,
        }
    }

    fn parse_command(&mut self, name: &str) -> Result<Expr, Error> {
        if let Some(kind) = commands::bigop(name) {
            return Ok(Expr::BigOp {
                kind,
                lower: None,
                upper: None,
                limits: Limits::Auto,
            });
        }
        if let Some(ch) = commands::var_greek(name) {
            return Ok(Expr::Sym {
                ch,
                class: AtomClass::Ord,
                alphabet: Alphabet::Italic,
            });
        }
        if let Some(alphabet) = commands::alphabet(name) {
            let inner = self.parse_group()?;
            return Ok(Expr::Alphabet(alphabet, Box::new(inner)));
        }
        if let Some(class) = commands::class(name) {
            let inner = self.parse_group()?;
            return Ok(Expr::Class(class, Box::new(inner)));
        }
        if let Some(kind) = commands::accent(name) {
            let base = self.parse_group()?;
            return Ok(Expr::Accent {
                kind,
                base: Box::new(base),
            });
        }
        if let Some(func) = commands::function(name) {
            return Ok(match func {
                MathFunction::Limit(func) => Expr::BigOp {
                    kind: BigOpKind::Limit(func),
                    lower: None,
                    upper: None,
                    limits: Limits::Auto,
                },
                MathFunction::Ordinary(func) => Expr::Func(func),
            });
        }
        if let Some(ch) = commands::xarrow(name) {
            // amsmath order: optional `[under]` first, `{over}` second.
            let under = self.parse_bracket_arg()?.map(Box::new);
            let over = self.parse_group()?;
            return Ok(Expr::XArrow {
                ch,
                // An empty `{}` means no label above.
                over: (!is_empty(&over)).then(|| Box::new(over)),
                under,
            });
        }
        if let Some((size, class)) = sized_delim_command(name) {
            let at = self.here();
            return match self.read_delim()? {
                Delim::Char(ch) => Ok(Expr::SizedDelim { ch, class, size }),
                // `\big.` has nothing to size.
                Delim::None => Err(Error::parse(format!("\\{name} needs a delimiter"), at)),
            };
        }

        match name {
            "frac" | "dfrac" | "tfrac" | "binom" | "dbinom" | "tbinom" => {
                let style = match name.as_bytes().first() {
                    Some(b'd') => FracStyle::Display,
                    Some(b't') => FracStyle::Text,
                    _ => FracStyle::Auto,
                };
                let binom = name.ends_with("binom");
                let num = self.parse_group()?;
                let den = self.parse_group()?;
                let frac = Expr::Frac {
                    num: Box::new(num),
                    den: Box::new(den),
                    style,
                    bar: !binom,
                };
                Ok(if binom { fenced('(', frac, ')') } else { frac })
            }
            // `\int` with its limits above and below.
            "intop" => Ok(Expr::BigOp {
                kind: BigOpKind::Integral,
                lower: None,
                upper: None,
                limits: Limits::Always,
            }),
            "cfrac" => {
                // The numerator's alignment, `[l]` or `[r]`, is read and ignored.
                self.parse_bracket_arg()?;
                let num = self.parse_group()?;
                let den = self.parse_group()?;
                Ok(Expr::Frac {
                    num: Box::new(num),
                    den: Box::new(den),
                    style: FracStyle::Continued,
                    bar: true,
                })
            }
            "genfrac" => self.parse_genfrac(),
            "middle" => {
                if self.lefts == 0 {
                    return Err(self.err_consumed("\\middle outside \\left…\\right"));
                }
                Ok(Expr::Middle(self.read_delim()?))
            }
            #[cfg(feature = "formatting")]
            "rule" => {
                let raise = match self.parse_raw_bracket()? {
                    Some(raw) => length_of(&raw).ok_or_else(|| {
                        self.err_consumed(format!("expected a length, got {raw:?}"))
                    })?,
                    None => 0.0,
                };
                let width = self.read_length()?;
                let height = self.read_length()?;
                Ok(Expr::Rule {
                    width,
                    height,
                    raise,
                })
            }
            // 0.7 and 0.3 of a 1.2em baseline skip, as LaTeX's `\strut` is.
            #[cfg(feature = "formatting")]
            "strut" => Ok(Expr::Rule {
                width: 0.0,
                height: 1.2,
                raise: -0.36,
            }),
            #[cfg(feature = "formatting")]
            "smash" => {
                let at = self.here();
                let (top, bottom) = match self.parse_raw_bracket()?.as_deref().map(str::trim) {
                    None | Some("") => (true, true),
                    Some("t") => (true, false),
                    Some("b") => (false, true),
                    Some(other) => {
                        return Err(Error::parse(
                            format!("\\smash takes [t] or [b], not [{other}]"),
                            at,
                        ));
                    }
                };
                let body = self.parse_group()?;
                Ok(Expr::Adjust {
                    adjust: Adjust::Smash { top, bottom },
                    body: Box::new(body),
                })
            }
            #[cfg(feature = "formatting")]
            "llap" | "rlap" | "clap" | "mathllap" | "mathrlap" | "mathclap" => {
                let lap = match name.trim_start_matches("math") {
                    "llap" => Lap::Left,
                    "rlap" => Lap::Right,
                    _ => Lap::Centre,
                };
                let body = self.parse_group()?;
                Ok(Expr::Adjust {
                    adjust: Adjust::Lap(lap),
                    body: Box::new(body),
                })
            }
            #[cfg(feature = "formatting")]
            "raisebox" => {
                let em = self.read_length()?;
                // The optional height and depth it claims are read and ignored.
                self.parse_raw_bracket()?;
                self.parse_raw_bracket()?;
                let body = self.parse_group()?;
                Ok(Expr::Adjust {
                    adjust: Adjust::Raise(em),
                    body: Box::new(body),
                })
            }
            #[cfg(feature = "formatting")]
            "fbox" => Ok(Expr::Enclose {
                kind: Enclose::Frame,
                body: Box::new(self.parse_text_arg()?),
            }),
            #[cfg(feature = "formatting")]
            "colorbox" => {
                let fill = self.read_colour()?;
                Ok(Expr::Enclose {
                    kind: Enclose::Fill(fill),
                    body: Box::new(self.parse_text_arg()?),
                })
            }
            #[cfg(feature = "formatting")]
            "fcolorbox" => {
                let frame = self.read_colour()?;
                let fill = self.read_colour()?;
                Ok(Expr::Enclose {
                    kind: Enclose::FramedFill { frame, fill },
                    body: Box::new(self.parse_text_arg()?),
                })
            }
            #[cfg(feature = "formatting")]
            "boxed" | "cancel" | "bcancel" | "xcancel" | "sout" => {
                let kind = match name {
                    "boxed" => Enclose::Frame,
                    "cancel" => Enclose::Cancel,
                    "bcancel" => Enclose::BackCancel,
                    "xcancel" => Enclose::CrossCancel,
                    _ => Enclose::StrikeOut,
                };
                let body = self.parse_group()?;
                Ok(Expr::Enclose {
                    kind,
                    body: Box::new(body),
                })
            }
            #[cfg(feature = "formatting")]
            "cancelto" => {
                let value = self.parse_group()?;
                let body = self.parse_group()?;
                Ok(Expr::Scripts {
                    base: Box::new(Expr::Enclose {
                        kind: Enclose::CancelTo,
                        body: Box::new(body),
                    }),
                    sub: None,
                    sup: Some(Box::new(value)),
                })
            }
            #[cfg(feature = "formatting")]
            "hline" | "hdashline" => {
                if self.envs == 0 {
                    return Err(self.err_consumed(format!("\\{name} outside an environment")));
                }
                Ok(Expr::HLine {
                    dashed: name == "hdashline",
                })
            }
            "hspace" => {
                // `\hspace*` keeps its space at a line break; there are none here.
                if self.peek() == Some(&Token::Char('*')) {
                    self.pos += 1;
                }
                if self.peek() != Some(&Token::LBrace) {
                    return Err(self.err("expected '{' after \\hspace"));
                }
                Ok(Expr::Space(em_to_mu(self.read_length()?)))
            }
            "kern" | "mkern" | "hskip" | "mskip" => Ok(Expr::Space(em_to_mu(self.read_length()?))),
            // `\vphantom{(}`, as plain TeX defines it.
            "mathstrut" => Ok(Expr::Phantom {
                body: Box::new(Expr::Sym {
                    ch: '(',
                    class: AtomClass::Open,
                    alphabet: Alphabet::Auto,
                }),
                width: false,
                height: true,
            }),
            "pmod" | "pod" => {
                let arg = self.parse_group()?;
                let mut items = vec![
                    Expr::Space(18),
                    Expr::Sym {
                        ch: '(',
                        class: AtomClass::Open,
                        alphabet: Alphabet::Auto,
                    },
                ];
                if name == "pmod" {
                    items.extend([Expr::Func("mod"), Expr::Space(6)]);
                }
                match arg {
                    Expr::Seq(parts) => items.extend(parts),
                    other => items.push(other),
                }
                items.push(Expr::Sym {
                    ch: ')',
                    class: AtomClass::Close,
                    alphabet: Alphabet::Auto,
                });
                // A group, so it writes out and reads back as one piece.
                Ok(Expr::Group(Box::new(Expr::Seq(items))))
            }
            "overset" | "stackrel" => {
                let over = self.parse_group()?;
                let base = self.parse_group()?;
                Ok(Expr::Stack {
                    over: Some(Box::new(over)),
                    base: Box::new(base),
                    under: None,
                    // `\stackrel` is always a relation; `\overset` takes its base's class.
                    class: (name == "stackrel").then_some(AtomClass::Rel),
                })
            }
            "underset" => {
                let under = self.parse_group()?;
                let base = self.parse_group()?;
                Ok(Expr::Stack {
                    over: None,
                    base: Box::new(base),
                    under: Some(Box::new(under)),
                    class: None,
                })
            }
            "phantom" | "hphantom" | "vphantom" => {
                let body = self.parse_group()?;
                Ok(Expr::Phantom {
                    body: Box::new(body),
                    width: name != "vphantom",
                    height: name != "hphantom",
                })
            }
            "not" => {
                // `\not` applies to the next atom, not to a braced argument.
                let base = self.parse_atom()?;
                Ok(Expr::Accent {
                    kind: Accent::Not,
                    base: Box::new(base),
                })
            }
            "substack" => self.parse_substack(),
            "sqrt" => {
                let degree = self.parse_bracket_arg()?.map(Box::new);
                let radicand = self.parse_group()?;
                Ok(Expr::Sqrt {
                    degree,
                    radicand: Box::new(radicand),
                })
            }
            _ if commands::text(name).is_some() => {
                let text = Expr::Text(self.parse_raw_group()?);
                Ok(match commands::text(name) {
                    Some(Alphabet::Upright) | None => text,
                    Some(alphabet) => Expr::Alphabet(alphabet, Box::new(text)),
                })
            }
            "textcolor" | "mathcolor" => {
                let decor = Decor::colour(self.read_colour()?);
                let body = self.parse_group()?;
                Ok(Expr::Styled {
                    decor,
                    body: Box::new(body),
                })
            }
            "color" => Err(self.err_consumed("\\color outside a group")),
            "left" => self.parse_delimited(),
            "right" => Err(self.err_consumed("\\right without \\left")),
            "begin" => self.parse_env(),
            "end" => Err(self.err_consumed("\\end without \\begin")),
            _ => symbols::resolve(name)
                .map(|(ch, class)| Expr::Sym {
                    ch,
                    class,
                    alphabet: Alphabet::Auto,
                })
                .ok_or_else(|| {
                    if is_formatting_command(name) {
                        self.err_consumed(format!("\\{name} needs the `formatting` feature"))
                    } else {
                        self.err_consumed(format!("unknown command \\{name}"))
                    }
                }),
        }
    }

    /// A `{...}` group, or the single atom LaTeX allows in place of one.
    fn parse_group(&mut self) -> Result<Expr, Error> {
        match self.peek() {
            Some(Token::LBrace) => {
                self.pos += 1;
                let inner = self.parse_expr()?;
                self.expect_rbrace()?;
                Ok(inner)
            }
            _ => self.parse_atom(),
        }
    }

    /// Verbatim and not trimmed: the spaces in `\text{ if }` are significant.
    fn parse_raw_group(&mut self) -> Result<String, Error> {
        let Some(Spanned {
            tok: Token::LBrace,
            end: open,
            ..
        }) = self.tokens.get(self.pos).copied()
        else {
            return Err(self.err("expected '{' after a text command"));
        };
        self.pos += 1;

        let mut depth = 1usize;
        loop {
            let Some(spanned) = self.tokens.get(self.pos) else {
                return Err(self.err("unclosed text group"));
            };
            match spanned.tok {
                Token::LBrace => depth += 1,
                Token::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        let close = spanned.start;
                        self.pos += 1;
                        let raw = self.src.get(open..close).unwrap_or_default();
                        return Ok(raw.to_owned());
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
    }

    fn parse_substack(&mut self) -> Result<Expr, Error> {
        self.enter()?;
        if self.peek() != Some(&Token::LBrace) {
            return Err(self.err("expected '{' after \\substack"));
        }
        self.pos += 1;

        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut cell: Vec<Expr> = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unclosed \\substack")),
                Some(Token::RBrace) => {
                    self.pos += 1;
                    break;
                }
                Some(Token::RowBreak) => {
                    self.pos += 1;
                    rows.push(vec![flatten(std::mem::take(&mut cell))]);
                }
                // One column: `&` separates nothing here, and `parse_run` would stall on it.
                Some(Token::Align) => {
                    return Err(self.err("an alignment marker inside \\substack"));
                }
                _ => cell.push(self.parse_cell()?),
            }
        }
        rows.push(vec![flatten(cell)]);
        self.leave();
        Ok(Expr::Env(EnvKind::SubStack, rows))
    }

    fn parse_env(&mut self) -> Result<Expr, Error> {
        self.enter()?;
        self.envs += 1;
        let env = self.parse_env_inner();
        self.envs -= 1;
        env
    }

    fn parse_env_inner(&mut self) -> Result<Expr, Error> {
        let name = self.parse_raw_group()?;
        let kind = commands::environment(&name)
            .ok_or_else(|| self.err(format!("unsupported environment {name}")))?;
        // `alignat{n}`: the count only tunes spacing; the cells give the columns.
        if name.starts_with("alignat") || name.starts_with("alignedat") {
            self.parse_raw_group()?;
        }
        let kind = match kind {
            EnvKind::Array(_) => {
                let at = self.here();
                let raw = self.parse_raw_group()?;
                let mut spec = ArraySpec::parse(&raw)
                    .ok_or_else(|| Error::parse(format!("unsupported column spec {raw:?}"), at))?;
                let ruled = (0..=spec.columns()).any(|i| spec.rule(i).is_some());
                if ruled && !cfg!(feature = "formatting") {
                    return Err(Error::parse(
                        "column rules need the `formatting` feature",
                        at,
                    ));
                }
                spec.display = name == "darray";
                EnvKind::Array(spec)
            }
            EnvKind::SubArray(_) => {
                let at = self.here();
                let raw = self.parse_raw_group()?;
                let align = align_letter(&raw).ok_or_else(|| {
                    Error::parse(format!("subarray takes one of l, c, r, not {raw:?}"), at)
                })?;
                EnvKind::SubArray(align)
            }
            EnvKind::AlignedMatrix { delims, .. } => {
                let at = self.here();
                let align = match self.parse_raw_bracket()? {
                    None => ColumnAlign::Centre,
                    Some(raw) => align_letter(&raw).ok_or_else(|| {
                        Error::parse(format!("{name} takes [l], [c] or [r], not [{raw}]"), at)
                    })?,
                };
                EnvKind::AlignedMatrix { delims, align }
            }
            other => other,
        };

        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut row: Vec<Expr> = Vec::new();
        let mut cell: Vec<Expr> = Vec::new();

        loop {
            match self.peek() {
                None => return Err(self.err(format!("unclosed \\begin{{{name}}}"))),
                Some(Token::Command(cmd)) if *cmd == "end" => {
                    self.pos += 1;
                    let closing = self.parse_raw_group()?;
                    if closing != name {
                        return Err(
                            self.err(format!("\\end{{{closing}}} closes \\begin{{{name}}}"))
                        );
                    }
                    break;
                }
                Some(Token::RowBreak) => {
                    self.pos += 1;
                    // `\\[5pt]`: the extra row space is read and ignored.
                    self.parse_bracket_arg()?;
                    row.push(flatten(std::mem::take(&mut cell)));
                    rows.push(std::mem::take(&mut row));
                }
                Some(Token::Align) => {
                    self.pos += 1;
                    row.push(flatten(std::mem::take(&mut cell)));
                }
                _ => cell.push(self.parse_cell()?),
            }
        }

        row.push(flatten(cell));
        rows.push(row);
        // A trailing `\\` is normal LaTeX and must not add a blank row.
        if rows.len() > 1
            && rows
                .last()
                .is_some_and(|row| row.iter().all(Expr::is_blank))
        {
            rows.pop();
        }

        self.leave();
        let env = Expr::Env(kind, rows);
        // `smallmatrix` is a matrix at script size.
        Ok(if name == "smallmatrix" {
            Expr::Style {
                style: StyleSwitch::Script,
                body: Box::new(env),
            }
        } else {
            env
        })
    }

    /// `\genfrac{left}{right}{thickness}{style}{num}{den}`. A thickness other than zero is the default.
    fn parse_genfrac(&mut self) -> Result<Expr, Error> {
        let left = self.read_optional_delim()?;
        let right = self.read_optional_delim()?;
        let thickness = self.parse_raw_group()?;
        let bar = !is_zero_length(&thickness);
        let style_at = self.here();
        let style = self.parse_raw_group()?;
        let num = self.parse_group()?;
        let den = self.parse_group()?;
        let (frac_style, switch) = match style.trim() {
            "" => (FracStyle::Auto, None),
            "0" => (FracStyle::Display, None),
            "1" => (FracStyle::Text, None),
            "2" => (FracStyle::Auto, Some(StyleSwitch::Script)),
            "3" => (FracStyle::Auto, Some(StyleSwitch::ScriptScript)),
            other => {
                return Err(Error::parse(
                    format!("\\genfrac style {other:?}: expected 0, 1, 2, 3 or nothing"),
                    style_at,
                ));
            }
        };
        let frac = Expr::Frac {
            num: Box::new(num),
            den: Box::new(den),
            style: frac_style,
            bar,
        };
        let fenced = match (left, right) {
            (Delim::None, Delim::None) => frac,
            (left, right) => Expr::Delimited {
                left,
                body: Box::new(frac),
                right,
            },
        };
        Ok(match switch {
            Some(style) => Expr::Style {
                style,
                body: Box::new(fenced),
            },
            None => fenced,
        })
    }

    /// A braced delimiter that may be left empty, as `\genfrac` takes them.
    fn read_optional_delim(&mut self) -> Result<Delim, Error> {
        if self.peek() != Some(&Token::LBrace) {
            // KaTeX also takes a bare delimiter: `\genfrac ( ] {0pt}{}{a}{b}`.
            return self.read_delim();
        }
        self.pos += 1;
        if self.peek() == Some(&Token::RBrace) {
            self.pos += 1;
            return Ok(Delim::None);
        }
        let delim = self.read_delim()?;
        self.expect_rbrace()?;
        Ok(delim)
    }

    fn parse_delimited(&mut self) -> Result<Expr, Error> {
        self.enter()?;
        let left = self.read_delim()?;
        self.lefts += 1;
        let body = self.parse_expr();
        self.lefts -= 1;
        let body = body?;
        match self.bump() {
            Some(Token::Command("right")) => {}
            _ => return Err(self.err("\\left without \\right")),
        }
        let right = self.read_delim()?;
        self.leave();
        Ok(Expr::Delimited {
            left,
            body: Box::new(body),
            right,
        })
    }

    fn read_delim(&mut self) -> Result<Delim, Error> {
        // Offset taken up front: `take_one_literal_char` can shrink the current token in place.
        let at = self.here();
        // `\left.` is the null delimiter; the tokenizer delivers `.` as a `Literal`.
        if matches!(self.peek(), Some(Token::Literal(_))) {
            return match self.take_one_literal_char() {
                Expr::Sym { ch: '.', .. } => Ok(Delim::None),
                Expr::Sym { ch, .. } => Err(Error::parse(format!("{ch:?} is not a delimiter"), at)),
                _ => Err(Error::parse("expected a delimiter", at)),
            };
        }
        match self.bump() {
            Some(Token::Char(c)) => delim_char(c)
                .map(Delim::Char)
                .ok_or_else(|| Error::parse(format!("{c:?} is not a delimiter"), at)),
            Some(Token::LBrace) => Ok(Delim::Char('{')),
            Some(Token::RBrace) => Ok(Delim::Char('}')),
            Some(Token::Command(cmd)) => match symbols::resolve(cmd) {
                Some((ch, AtomClass::Open | AtomClass::Close | AtomClass::Ord)) => {
                    Ok(Delim::Char(ch))
                }
                Some((ch, AtomClass::Rel)) if is_vertical_arrow(ch) => Ok(Delim::Char(ch)),
                _ => Err(Error::parse(format!("unknown delimiter \\{cmd}"), at)),
            },
            other => Err(Error::parse(
                format!("expected a delimiter, got {other:?}"),
                at,
            )),
        }
    }

    /// A TeX length, `1.5em` or `{-3mu}`, in ems. Glue's `plus` and `minus` parts are read and dropped.
    fn read_length(&mut self) -> Result<f32, Error> {
        let at = self.here();
        if self.peek() == Some(&Token::LBrace) {
            let raw = self.parse_raw_group()?;
            return length_of(&raw)
                .ok_or_else(|| Error::parse(format!("expected a length, got {raw:?}"), at));
        }
        // Unbraced, the length runs across token boundaries, so it is read from the source.
        let start = self.atom_start();
        let rest = self.src.get(start..).unwrap_or_default();
        let (em, mut used) =
            scan_length(rest).ok_or_else(|| Error::parse("expected a length", at))?;
        loop {
            let tail = rest.get(used..).unwrap_or_default();
            let word = tail.trim_start();
            let skipped = tail.len() - word.len();
            let Some(after) = word
                .strip_prefix("plus")
                .or_else(|| word.strip_prefix("minus"))
            else {
                break;
            };
            let Some(glue) = scan_glue(after) else {
                break;
            };
            used += skipped + (word.len() - after.len()) + glue;
        }
        self.skip_to(start + used);
        Ok(em)
    }

    /// Step past every token that ends by `end`, cutting a literal that straddles it.
    fn skip_to(&mut self, end: usize) {
        while let Some(spanned) = self.tokens.get_mut(self.pos) {
            if spanned.end <= end {
                self.pos += 1;
                continue;
            }
            if spanned.start < end {
                if let Token::Literal(text) = &mut spanned.tok {
                    *text = text.get(end - spanned.start..).unwrap_or_default();
                    spanned.start = end;
                }
            }
            break;
        }
    }

    /// An optional `[...]` argument, as in `\sqrt[3]`.
    fn parse_bracket_arg(&mut self) -> Result<Option<Expr>, Error> {
        if !matches!(self.peek(), Some(Token::Char('['))) {
            return Ok(None);
        }
        self.pos += 1;
        self.parse_until_char(']').map(Some)
    }

    fn parse_until_char(&mut self, end: char) -> Result<Expr, Error> {
        let mut items: Vec<Expr> = Vec::new();
        loop {
            match self.peek() {
                Some(Token::Char(c)) if *c == end => {
                    self.pos += 1;
                    break;
                }
                None => return Err(self.err(format!("expected {end:?}"))),
                _ => items.push(self.parse_atom()?),
            }
        }
        Ok(flatten(items))
    }

    fn expect_rbrace(&mut self) -> Result<(), Error> {
        match self.bump() {
            Some(Token::RBrace) => Ok(()),
            other => Err(self.err(format!("expected '}}', got {other:?}"))),
        }
    }
}

/// A lone `l`, `c` or `r`.
fn align_letter(raw: &str) -> Option<ColumnAlign> {
    let mut chars = raw.trim().chars();
    match (chars.next(), chars.next()) {
        (Some(letter), None) => ColumnAlign::from_letter(letter),
        _ => None,
    }
}

/// Commands the `formatting` feature adds, so that without it they say why they fail.
fn is_formatting_command(name: &str) -> bool {
    !cfg!(feature = "formatting")
        && (SizeSwitch::from_name(name).is_some()
            || matches!(
                name,
                "rule"
                    | "strut"
                    | "smash"
                    | "llap"
                    | "rlap"
                    | "clap"
                    | "mathllap"
                    | "mathrlap"
                    | "mathclap"
                    | "raisebox"
                    | "boxed"
                    | "fbox"
                    | "colorbox"
                    | "fcolorbox"
                    | "cancel"
                    | "bcancel"
                    | "xcancel"
                    | "cancelto"
                    | "sout"
                    | "hline"
                    | "hdashline"
            ))
}

/// Ems to whole maths units, the only spacing [`Expr::Space`] holds.
fn em_to_mu(em: f32) -> i16 {
    (em * 18.0)
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

/// Ems per unit, taking 1em as 10pt, as KaTeX does.
fn unit_em(unit: &str) -> Option<f32> {
    Some(match unit {
        "em" => 1.0,
        "ex" => 0.431,
        "mu" => 1.0 / 18.0,
        "pt" => 0.1,
        "pc" => 1.2,
        "bp" => 0.1 * 72.27 / 72.0,
        "dd" => 0.1 * 1238.0 / 1157.0,
        "cc" => 1.2 * 1238.0 / 1157.0,
        "in" => 7.227,
        "cm" => 7.227 / 2.54,
        "mm" => 7.227 / 25.4,
        "sp" => 0.1 / 65536.0,
        _ => return None,
    })
}

/// A signed number and a unit at the front of `text`: the length in ems and the bytes it took.
fn scan_length(text: &str) -> Option<(f32, usize)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut negative = false;
    loop {
        match bytes.get(i) {
            Some(b' ' | b'\t' | b'\n' | b'+') => i += 1,
            Some(b'-') => {
                negative = !negative;
                i += 1;
            }
            _ => break,
        }
    }
    let number_start = i;
    while matches!(bytes.get(i), Some(b) if b.is_ascii_digit() || *b == b'.') {
        i += 1;
    }
    let number: f32 = text.get(number_start..i)?.parse().ok()?;
    while matches!(bytes.get(i), Some(b' ' | b'\t' | b'\n')) {
        i += 1;
    }
    let per_unit = unit_em(text.get(i..i + 2)?)?;
    let em = number * per_unit;
    Some((if negative { -em } else { em }, i + 2))
}

/// The stretch of a glue, `1fill` or `2pt`, after `plus` or `minus`: the bytes it took.
fn scan_glue(text: &str) -> Option<usize> {
    if let Some((_, used)) = scan_length(text) {
        return Some(used);
    }
    let rest = text.trim_start();
    let start = text.len() - rest.len();
    let digits = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    let after = rest.get(digits..)?.trim_start();
    let gap = rest.len() - digits - after.len();
    let fil = after.strip_prefix("fil")?;
    let ls = fil.len() - fil.trim_start_matches('l').len();
    Some(start + digits + gap + 3 + ls)
}

/// A whole string as one length, in ems.
fn length_of(text: &str) -> Option<f32> {
    let (em, used) = scan_length(text)?;
    text.get(used..)?.trim().is_empty().then_some(em)
}

/// Whether a length such as `0pt`, `0`, or `0.0em` is zero.
fn is_zero_length(text: &str) -> bool {
    let number = text
        .trim()
        .trim_end_matches(|c: char| c.is_ascii_alphabetic());
    number.trim().parse::<f32>().is_ok_and(|n| n == 0.0)
}

fn flatten(mut items: Vec<Expr>) -> Expr {
    match items.len() {
        1 => items.remove(0),
        _ => Expr::Seq(items),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    fn parse(src: &str) -> Result<Expr, Error> {
        Parser::new(src, Tokenizer::new(src).tokenize()).parse()
    }

    fn sym(ch: char) -> Expr {
        Expr::Sym {
            ch,
            class: class_of_char(ch),
            alphabet: Alphabet::Auto,
        }
    }

    #[test]
    fn parses_a_fraction() {
        assert_eq!(
            parse("\\frac{a}{b}"),
            Ok(Expr::Frac {
                num: Box::new(sym('a')),
                den: Box::new(sym('b')),
                style: FracStyle::Auto,
                bar: true,
            })
        );
    }

    #[test]
    fn braces_are_optional_around_a_single_character() {
        assert_eq!(
            parse("\\frac12"),
            Ok(Expr::Frac {
                num: Box::new(sym('1')),
                den: Box::new(sym('2')),
                style: FracStyle::Auto,
                bar: true,
            })
        );
    }

    #[test]
    fn a_script_binds_to_one_character() {
        let Ok(Expr::Seq(items)) = parse("12^3") else {
            panic!("expected a sequence");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items.first(), Some(&sym('1')));
        assert!(matches!(items.get(1), Some(Expr::Scripts { .. })));
    }

    #[test]
    fn fraction_variants_carry_their_style() {
        for (src, want) in [
            ("\\frac ab", FracStyle::Auto),
            ("\\dfrac ab", FracStyle::Display),
            ("\\tfrac ab", FracStyle::Text),
        ] {
            let Ok(Expr::Frac { style, .. }) = parse(src) else {
                panic!("{src:?} did not parse as a fraction");
            };
            assert_eq!(style, want, "{src:?}");
        }
    }

    #[test]
    fn big_operators_take_limits() {
        let Ok(Expr::Seq(items)) = parse("\\sum_{i=1}^{n} i") else {
            panic!("expected a sequence");
        };
        assert!(matches!(
            items.first(),
            Some(Expr::BigOp {
                kind: BigOpKind::Sum,
                lower: Some(_),
                upper: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn limits_modifiers_are_recorded() {
        for (src, want) in [
            ("\\int_0^1", Limits::Auto),
            ("\\int\\limits_0^1", Limits::Always),
            ("\\sum\\nolimits_0^1", Limits::Never),
        ] {
            let Ok(Expr::BigOp { limits, .. }) = parse(src) else {
                panic!("{src:?} did not parse as a big operator");
            };
            assert_eq!(limits, want, "{src:?}");
        }
    }

    #[test]
    fn text_keeps_its_spaces_including_the_outer_ones() {
        assert_eq!(
            parse("\\text{if x is even}"),
            Ok(Expr::Text("if x is even".into()))
        );
        assert_eq!(parse("\\text{ with }"), Ok(Expr::Text(" with ".into())));
    }

    #[test]
    fn nested_text_braces_are_balanced() {
        assert_eq!(parse("\\text{a {b} c}"), Ok(Expr::Text("a {b} c".into())));
    }

    #[test]
    fn braces_survive_as_a_group() {
        assert_eq!(parse("{+}").map(|e| e.class()), Ok(AtomClass::Ord));
        assert_eq!(parse("+").map(|e| e.class()), Ok(AtomClass::Bin));
    }

    #[test]
    fn delimiters_carry_their_codepoint() {
        let Ok(Expr::Delimited { left, right, .. }) = parse("\\left\\lfloor x \\right\\rfloor")
        else {
            panic!("expected a delimited expression");
        };
        assert_eq!(left, Delim::Char('\u{230A}'));
        assert_eq!(right, Delim::Char('\u{230B}'));
    }

    #[test]
    fn an_absent_delimiter_is_allowed() {
        let Ok(Expr::Delimited { left, right, .. }) = parse("\\left( x \\right.") else {
            panic!("expected a delimited expression");
        };
        assert_eq!(left, Delim::Char('('));
        assert_eq!(right, Delim::None);
    }

    #[test]
    fn gather_and_align_are_distinct() {
        assert!(matches!(
            parse("\\begin{gather} a \\\\ b \\end{gather}"),
            Ok(Expr::Env(EnvKind::Gathered, _))
        ));
        assert!(matches!(
            parse("\\begin{align} a \\\\ b \\end{align}"),
            Ok(Expr::Env(EnvKind::Aligned, _))
        ));
    }

    #[test]
    fn matrix_delimiters_are_recorded() {
        for (src, want) in [
            ("\\begin{matrix} a \\end{matrix}", None),
            ("\\begin{pmatrix} a \\end{pmatrix}", Some(('(', ')'))),
            ("\\begin{bmatrix} a \\end{bmatrix}", Some(('[', ']'))),
            (
                "\\begin{Vmatrix} a \\end{Vmatrix}",
                Some(('\u{2016}', '\u{2016}')),
            ),
        ] {
            let Ok(Expr::Env(EnvKind::Matrix { delims }, _)) = parse(src) else {
                panic!("{src:?} did not parse as a matrix");
            };
            assert_eq!(delims, want, "{src:?}");
        }
    }

    #[test]
    fn a_trailing_row_break_adds_no_row() {
        let Ok(Expr::Env(_, rows)) = parse("\\begin{matrix} a \\\\ b \\\\ \\end{matrix}") else {
            panic!("expected a matrix");
        };
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn unsupported_constructs_are_errors() {
        for src in [
            "\\begin{array}{p{2cm}} 1 \\end{array}",
            "\\begin{matrix} x } \\end{matrix}",
            "\\begin{matrix} x \\right) \\end{matrix}",
            "\\substack{a } b}",
            "\\begin{CD} A @>>> B \\end{CD}",
            "\\middle|",
            "\\hline",
            "\\begin{align} x \\end{gather}",
            "\\begin{align} x",
            "\\end{align}",
            "a \\\\ b",
            "\\thereisnosuchcommand",
            "}",
            "^2",
            "\\frac{a}",
            "\\left( x",
            "x^2^3",
            "\\left\\weird x \\right.",
        ] {
            assert!(parse(src).is_err(), "{src:?} should not have parsed");
        }
    }

    #[test]
    fn over_splits_its_run_into_a_fraction() {
        assert_eq!(
            parse(r"{1 \over 2}"),
            Ok(Expr::Group(Box::new(Expr::Frac {
                num: Box::new(sym('1')),
                den: Box::new(sym('2')),
                style: FracStyle::Auto,
                bar: true,
            })))
        );
        assert!(matches!(parse(r"a \over b"), Ok(Expr::Frac { .. })));
    }

    #[test]
    fn style_switches_take_the_rest_of_their_group() {
        let Ok(Expr::Seq(items)) = parse(r"a {\textstyle b c} d") else {
            panic!("expected a sequence");
        };
        let Some(Expr::Group(inner)) = items.get(1) else {
            panic!("expected a group, got {:?}", items.get(1));
        };
        assert!(matches!(
            &**inner,
            Expr::Style {
                style: StyleSwitch::Text,
                ..
            }
        ));
    }

    #[test]
    fn sized_delimiters_carry_size_and_class() {
        for (src, size, class) in [
            (r"\bigl(", DelimSize::Big, AtomClass::Open),
            (r"\Bigr]", DelimSize::BigCap, AtomClass::Close),
            (r"\biggm|", DelimSize::Bigg, AtomClass::Rel),
            (r"\Bigg\langle", DelimSize::BiggCap, AtomClass::Ord),
            (r"\big/", DelimSize::Big, AtomClass::Ord),
        ] {
            match parse(src) {
                Ok(Expr::SizedDelim {
                    size: got_size,
                    class: got_class,
                    ..
                }) => {
                    assert_eq!(got_size, size, "{src:?}");
                    assert_eq!(got_class, class, "{src:?}");
                }
                other => panic!("{src:?} parsed as {other:?}"),
            }
        }
        assert!(parse(r"\big.").is_err());
    }

    #[test]
    fn a_script_in_a_group_may_hang_off_an_empty_base() {
        assert!(parse(r"{_0}").is_ok());
        assert!(parse(r"\epsilon_{_{0}}").is_ok());
        assert!(parse("^2").is_err());
    }

    #[test]
    fn old_style_font_switches_take_the_rest_of_their_group() {
        let Ok(Expr::Group(inner)) = parse(r"{\rm d}x") else {
            let Ok(Expr::Seq(items)) = parse(r"{\rm d}x") else {
                panic!("expected a sequence");
            };
            assert!(matches!(
                items.first(),
                Some(Expr::Group(inner)) if matches!(&**inner, Expr::Alphabet(Alphabet::Upright, _))
            ));
            return;
        };
        assert!(matches!(&*inner, Expr::Alphabet(Alphabet::Upright, _)));
    }

    #[test]
    fn primes_merge_with_an_explicit_superscript() {
        let Ok(Expr::Scripts { sup: Some(sup), .. }) = parse(r"g'^{2}") else {
            panic!("expected scripts");
        };
        let Expr::Seq(parts) = *sup else {
            panic!("expected the primes and the argument together");
        };
        assert_eq!(parts.len(), 2);
        assert!(parse("x^2^3").is_err());
    }

    #[test]
    fn errors_carry_an_offset() {
        match parse("x + \\nope") {
            Err(Error::Parse { offset, .. }) => assert_eq!(offset, 4),
            other => panic!("expected a located parse error, got {other:?}"),
        }
    }

    #[test]
    fn deep_nesting_errors_rather_than_overflowing() {
        assert_eq!(parse(&"{".repeat(5000)), Err(Error::TooDeep));
    }

    #[test]
    fn arbitrary_input_does_not_panic() {
        let fragments = [
            "\\frac",
            "{",
            "}",
            "^",
            "_",
            "\\sqrt",
            "\\left",
            "\\right",
            "(",
            ")",
            "[",
            "]",
            "x",
            "1",
            "\\alpha",
            "\\\\",
            "\\text",
            "&",
            "\\",
            "|",
            ".",
            "\\sum",
            "α",
            "\\begin",
            "\\end",
            "matrix",
            "\\limits",
            "\\!",
            "\\middle",
            "\\hline",
            "\\cancel",
            "\\rule",
            "\\kern",
            "\\genfrac",
            "\\fbox",
            "$",
            "array",
            "1em",
            "-",
            "\\smash",
            "\\'",
        ];
        let mut state: u64 = 0x9E37_79B9;
        for _ in 0..20000 {
            let mut s = String::new();
            for _ in 0..10 {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let i = (state >> 33) as usize % fragments.len();
                s.push_str(fragments.get(i).copied().unwrap_or_default());
            }
            let _ = parse(&s);
        }
    }
}
