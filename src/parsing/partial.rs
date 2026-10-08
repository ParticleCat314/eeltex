//! Completing a formula that is still arriving.

use crate::tokenizer::{Spanned, Token, Tokenizer};

/// Complete a partial formula so that it parses.
#[must_use]
pub fn repair(source: &str) -> String {
    if is_complete(source) {
        return source.to_owned();
    }

    let tokens = Tokenizer::new(source).tokenize();
    let mut budget = RETREAT_BUDGET;
    // Longest prefix first; for each, fewest empty arguments first.
    for end in (0..=tokens.len()).rev() {
        let Some(prefix) = tokens.get(..end) else {
            continue;
        };
        let cut = prefix.last().map_or(0, |t| t.end);
        let Some(text) = source.get(..cut) else {
            continue;
        };
        // Charge the scan in `close` too, or many token boundaries go quadratic before any parse.
        if budget < text.len() {
            break;
        }
        let mut candidate = close(text, prefix);
        for _ in 0..=MAX_MISSING_ARGUMENTS {
            budget = budget.saturating_sub(candidate.len().max(1));
            if crate::parse(&candidate).is_ok() {
                return candidate;
            }
            if budget == 0 {
                return String::new();
            }
            candidate.push_str("{}");
        }
    }
    String::new()
}

/// Total characters [`repair`] will hand to the parser while retreating.
const RETREAT_BUDGET: usize = 1 << 21;

/// How many empty arguments [`repair`] will offer a command that wants more.
const MAX_MISSING_ARGUMENTS: usize = 3;

/// Whether a formula needs repairing at all.
#[must_use]
pub fn is_complete(source: &str) -> bool {
    crate::parse(source).is_ok()
}

/// Append closers for whatever the token stream leaves open, innermost first.
fn close(text: &str, tokens: &[Spanned<'_>]) -> String {
    enum Open {
        Brace,
        Left,
        Env(String),
    }

    let mut stack: Vec<Open> = Vec::new();
    let mut index = 0;
    while let Some(spanned) = tokens.get(index) {
        match &spanned.tok {
            Token::LBrace => stack.push(Open::Brace),
            Token::RBrace => {
                // A stray `}` past the outermost brace is left for the parser to report.
                if matches!(stack.last(), Some(Open::Brace)) {
                    stack.pop();
                }
            }
            Token::Command("left") => stack.push(Open::Left),
            Token::Command("right") => {
                if matches!(stack.last(), Some(Open::Left)) {
                    stack.pop();
                }
            }
            Token::Command("begin") => {
                if let Some(env) = env_name(tokens, index) {
                    // Past `{name}`, so its braces are not counted as well.
                    index += name_tokens(&env);
                    stack.push(Open::Env(env));
                    continue;
                }
            }
            Token::Command("end") => {
                if let Some(env) = env_name(tokens, index) {
                    if matches!(stack.last(), Some(Open::Env(_))) {
                        stack.pop();
                    }
                    index += name_tokens(&env);
                    continue;
                }
            }
            _ => {}
        }
        index += 1;
    }

    let mut out = text.to_owned();
    for open in stack.iter().rev() {
        match open {
            Open::Brace => out.push('}'),
            // `\right.`: the writer has not chosen a closing delimiter.
            Open::Left => out.push_str(r"\right."),
            Open::Env(name) => {
                out.push_str(r"\end{");
                out.push_str(name);
                out.push('}');
            }
        }
    }
    out
}

/// How many tokens `\begin{name}` spans; the star of `{name*}` is its own.
fn name_tokens(env: &str) -> usize {
    4 + usize::from(env.ends_with('*'))
}

fn env_name(tokens: &[Spanned<'_>], at: usize) -> Option<String> {
    let tok = |i: usize| tokens.get(at + i).map(|t| t.tok);
    match (tok(1)?, tok(2)?, tok(3)?) {
        (Token::LBrace, Token::Literal(name), Token::RBrace) => Some(name.to_owned()),
        (Token::LBrace, Token::Literal(name), Token::Char('*'))
            if tok(4) == Some(Token::RBrace) =>
        {
            Some(format!("{name}*"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parses(source: &str) -> bool {
        crate::parse(source).is_ok()
    }

    #[test]
    fn every_prefix_of_a_real_formula_repairs_into_something_that_parses() {
        const SOURCES: &[&str] = &[
            r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
            r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\left( \frac{\partial f}{\partial x} \right)_y",
            r"\begin{aligned} f(x) &= x^2 \\ &= x \cdot x \end{aligned}",
            r"\underbrace{a + b + c}_{\text{three}} \xrightarrow{f} \binom{n}{k}",
            r"\textcolor{#e0a030}{\nabla} \cdot \vec{E} = \frac{\rho}{\varepsilon_0}",
        ];
        for source in SOURCES {
            for end in 0..=source.len() {
                if !source.is_char_boundary(end) {
                    continue;
                }
                let prefix = &source[..end];
                let repaired = repair(prefix);
                assert!(
                    parses(&repaired),
                    "{prefix:?} repaired to {repaired:?}, which does not parse"
                );
            }
        }
    }

    #[test]
    fn a_complete_formula_is_left_alone() {
        for source in [r"x^2", r"\frac{a}{b}", r"\left( x \right)", ""] {
            assert_eq!(repair(source), source);
            assert!(is_complete(source));
        }
    }

    #[test]
    fn the_common_shapes_are_closed_rather_than_dropped() {
        for (partial, want) in [
            (r"\frac{a}", r"\frac{a}{}"),
            (r"\frac", r"\frac{}{}"),
            (r"\overset{a}", r"\overset{a}{}"),
            (r"\frac{a}{", r"\frac{a}{}"),
            (r"\sqrt{x", r"\sqrt{x}"),
            (r"\left( x", r"\left( x\right."),
            (
                r"{\begin{matrix} x \end{matrix} y",
                r"{\begin{matrix} x \end{matrix} y}",
            ),
            (
                r"{\begin{matrix*} x \end{matrix*} y",
                r"{\begin{matrix*} x \end{matrix*} y}",
            ),
            (
                r"\begin{pmatrix} a & b",
                r"\begin{pmatrix} a & b\end{pmatrix}",
            ),
            (
                r"\begin{aligned} x &= 1 \\",
                r"\begin{aligned} x &= 1 \\\end{aligned}",
            ),
            (r"{a + {b", r"{a + {b}}"),
        ] {
            assert_eq!(repair(partial), want, "repairing {partial:?}");
        }
    }

    #[test]
    fn nested_openers_close_in_the_right_order() {
        let repaired = repair(r"\left( \begin{matrix} a");
        assert_eq!(repaired, r"\left( \begin{matrix} a\end{matrix}\right.");
        assert!(parses(&repaired));
    }

    #[test]
    fn an_unfinished_command_is_dropped_rather_than_guessed() {
        assert_eq!(repair(r"x + \alph"), "x +");
        assert!(parses(&repair(r"x + \")));
        assert_eq!(repair(r"x^"), r"x^{}");
    }

    #[test]
    fn it_completes_structure_and_never_content() {
        assert_eq!(repair(r"\frac{a}"), r"\frac{a}{}");
        assert!(!repair(r"\frac{a}").contains('b'));
        assert!(repair(r"\left(").ends_with(r"\right."));
    }

    #[test]
    fn a_hostile_retreat_stays_within_budget() {
        let hostile = format!("{}{}", r"\frac{".repeat(2000), r"\nope");
        let repaired = repair(&hostile);
        assert!(parses(&repaired), "{repaired:?} does not parse");
    }

    #[test]
    fn repairing_is_idempotent() {
        for source in [r"\frac{a}", r"\left( x", r"x + \alph", r"\begin{matrix} a"] {
            let once = repair(source);
            assert_eq!(repair(&once), once, "repairing {source:?} twice differed");
        }
    }
}
