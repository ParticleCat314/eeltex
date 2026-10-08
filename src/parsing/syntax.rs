//! The tokenizer, for highlighting a formula as it is typed.
//!
//! ```
//! # #[cfg(feature = "syntax")] {
//! use eeltex::syntax::{highlight, TokenKind};
//!
//! let spans = highlight(r"\frac{a}{b}");
//! assert_eq!(spans.first().map(|(_, kind)| *kind), Some(TokenKind::Command));
//! # }
//! ```
//!
//! Spans are byte offsets into the source, in order and non-overlapping. They do not cover
//! whitespace, except the space that ends a control word (`\le x` is not `\lex`).

use crate::attrs::Span;
use crate::error::Error;
use crate::tokenizer::{Token, Tokenizer};

/// What a token is, to the degree a highlighter cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenKind {
    /// A control sequence: `\frac`, `\alpha`, `\left`.
    Command,
    /// A run of alphanumerics and dots: `x`, `12`, `1.5`.
    Literal,
    /// One of `(`, `)`, `[`, `]`, `|`.
    Delimiter,
    /// Any other single character: `+`, `=`, `,`, and the rest.
    Punctuation,
    /// `^` or `_`.
    Script,
    /// `{` or `}`.
    Brace,
    /// `&` or `\\`.
    Align,
    /// Explicit spacing: `\,`, `\quad`, `\!`.
    Space,
    /// Where the parser gave up. `\intt` tokenizes as a command; only the parse rejects it.
    Error,
}

/// Classify every token in `src`. At most one span is [`TokenKind::Error`].
///
/// Parses `src` to find the error; use [`highlight_with`] if you already have the result.
#[must_use]
pub fn highlight(src: &str) -> Vec<(Span, TokenKind)> {
    highlight_with(src, crate::parse(src).err().as_ref())
}

/// [`highlight`] with the error from [`parse`](crate::parse), or `None` if it succeeded.
///
/// Only [`Error::Parse`] carries an offset; any other error marks nothing.
#[must_use]
pub fn highlight_with(src: &str, error: Option<&Error>) -> Vec<(Span, TokenKind)> {
    let mut out: Vec<(Span, TokenKind)> = Tokenizer::new(src)
        .tokenize()
        .into_iter()
        .map(|spanned| {
            let span = Span::new(as_u32(spanned.start), as_u32(spanned.end));
            (span, kind_of(&spanned.tok))
        })
        .collect();

    if let Some(Error::Parse { offset, .. }) = error {
        let offset = as_u32(*offset);
        // A truncated formula reports the end of the source, past every token.
        let hit = out
            .iter()
            .position(|(span, _)| span.start <= offset && offset < span.end)
            .or_else(|| out.iter().position(|(span, _)| span.start >= offset))
            .or_else(|| out.len().checked_sub(1));
        if let Some(slot) = hit.and_then(|index| out.get_mut(index)) {
            slot.1 = TokenKind::Error;
        }
    }
    out
}

/// Saturates: wrapping would put a span somewhere plausible and wrong.
fn as_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn kind_of(token: &Token<'_>) -> TokenKind {
    match token {
        Token::Command(_) => TokenKind::Command,
        Token::Literal(_) => TokenKind::Literal,
        Token::Char(ch) => match ch {
            '(' | ')' | '[' | ']' | '|' => TokenKind::Delimiter,
            _ => TokenKind::Punctuation,
        },
        Token::LBrace | Token::RBrace => TokenKind::Brace,
        Token::Caret | Token::Underscore => TokenKind::Script,
        Token::RowBreak | Token::Align => TokenKind::Align,
        Token::Space(_) => TokenKind::Space,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        highlight(src).into_iter().map(|(_, kind)| kind).collect()
    }

    fn text(src: &str, span: Span) -> &str {
        let (start, end) = (span.start as usize, span.end as usize);
        src.get(start..end).unwrap_or("")
    }

    #[test]
    fn a_formula_classifies() {
        assert_eq!(
            kinds(r"\frac{a}{b}"),
            [
                TokenKind::Command,
                TokenKind::Brace,
                TokenKind::Literal,
                TokenKind::Brace,
                TokenKind::Brace,
                TokenKind::Literal,
                TokenKind::Brace,
            ]
        );
        assert_eq!(
            kinds("x^2 + 1"),
            [
                TokenKind::Literal,
                TokenKind::Script,
                TokenKind::Literal,
                TokenKind::Punctuation,
                TokenKind::Literal,
            ]
        );
    }

    #[test]
    fn a_span_slices_its_own_token() {
        let src = r"\alpha_{i} \le 2";
        let spans = highlight(src);
        let got: Vec<&str> = spans.iter().map(|(span, _)| text(src, *span)).collect();
        // `\le ` keeps the space that ends the control word.
        assert_eq!(got, ["\\alpha", "_", "{", "i", "}", "\\le ", "2"]);
    }

    /// The gaps between spans are exactly the whitespace.
    #[test]
    fn spans_are_ordered_and_do_not_overlap() {
        let src = r"\sum_{n=1}^{\infty} \frac{1}{n^2}";
        let mut at = 0;
        for (span, _) in highlight(src) {
            assert!(span.start >= at, "{span:?} goes backwards");
            assert!(span.end > span.start, "{span:?} is empty");
            let skipped = src.get(at as usize..span.start as usize).unwrap_or("x");
            assert!(skipped.trim().is_empty(), "{skipped:?} was not whitespace");
            at = span.end;
        }
    }

    /// A bare `&` splits a cell; `\&` draws an ampersand.
    #[test]
    fn an_escaped_ampersand_is_not_alignment() {
        let cells = kinds(r"\begin{matrix} a & b \end{matrix}");
        assert!(cells.contains(&TokenKind::Align), "{cells:?}");
        // Outside an environment the `&` itself is blamed, not the token after it.
        let src = "a & b";
        let bad: Vec<&str> = highlight(src)
            .iter()
            .filter(|(_, kind)| *kind == TokenKind::Error)
            .map(|(span, _)| text(src, *span))
            .collect();
        assert_eq!(bad, ["&"]);
        assert_eq!(
            kinds(r"a \& b"),
            [
                TokenKind::Literal,
                TokenKind::Punctuation,
                TokenKind::Literal
            ]
        );
    }

    /// An unknown command is lexically ordinary; only the parser can reject it.
    #[test]
    fn a_typo_is_marked_where_it_is() {
        let src = r"x + \intt y";
        let spans = highlight(src);
        let bad: Vec<&str> = spans
            .iter()
            .filter(|(_, kind)| *kind == TokenKind::Error)
            .map(|(span, _)| text(src, *span))
            .collect();
        assert_eq!(bad, ["\\intt "]);
    }

    #[test]
    fn a_good_formula_is_all_good() {
        for src in [r"\frac{a}{b}", "x^2", r"\left( x \right)", r"a \not= b"] {
            let spans = highlight(src);
            assert!(
                !spans.iter().any(|(_, kind)| *kind == TokenKind::Error),
                "{src} was marked bad"
            );
        }
    }

    /// A truncated source reports an offset past the last token.
    #[test]
    fn a_half_typed_formula_marks_its_last_token() {
        let src = r"\frac{a}{";
        let spans = highlight(src);
        let last = spans.last().copied();
        assert_eq!(last.map(|(_, kind)| kind), Some(TokenKind::Error));
        assert_eq!(last.map(|(span, _)| text(src, span)), Some("{"));
    }

    #[test]
    fn an_empty_source_has_no_spans() {
        assert!(highlight("").is_empty());
        assert!(highlight("   ").is_empty());
    }

    #[test]
    fn passing_the_error_in_gives_the_same_answer() {
        for src in [r"x + \intt y", r"\frac{a}{b}", r"\begin{matrix} a \\ b"] {
            let error = crate::parse(src).err();
            assert_eq!(highlight(src), highlight_with(src, error.as_ref()));
        }
    }
}
