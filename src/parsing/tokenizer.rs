//! Turns LaTeX maths source into tokens.
//! Tokens carry their source span so `\text{...}` can recover its argument verbatim.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Token<'a> {
    /// A control sequence, without its backslash.
    Command(&'a str),
    /// A run of alphanumerics and dots, e.g. `12`, `x`, `1.5`.
    Literal(&'a str),
    /// A single non-alphanumeric character.
    Char(char),
    LBrace,
    RBrace,
    Caret,
    Underscore,
    /// `\\` — a row break.
    RowBreak,
    /// A bare `&` — a cell separator; `\&` is `Char('&')`.
    Align,
    /// Explicit spacing, in mu (1/18 em). Signed: `\!` is −3.
    Space(i16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Spanned<'a> {
    pub tok: Token<'a>,
    /// Byte offsets; `end` is exclusive.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug)]
pub(crate) struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

/// Width in mu (1/18 em).
fn spacing_mu(name: &str) -> Option<i16> {
    Some(match name {
        "thinspace" => 3,
        "medspace" => 4,
        "thickspace" => 5,
        "space" | "nobreakspace" => 6,
        "enspace" => 9,
        "quad" => 18,
        "qquad" => 36,
        "negthinspace" => -3,
        "negmedspace" => -4,
        "negthickspace" => -5,
        _ => return None,
    })
}

impl<'a> Tokenizer<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Tokenizer { input, pos: 0 }
    }

    pub(crate) fn tokenize(mut self) -> Vec<Spanned<'a>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            let start = self.pos;
            let Some(tok) = self.next_token() else { break };
            tokens.push(Spanned {
                tok,
                start,
                end: self.pos,
            });
        }
        tokens
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn slice(&self, start: usize) -> &'a str {
        self.input.get(start..self.pos).unwrap_or_default()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn next_token(&mut self) -> Option<Token<'a>> {
        Some(match self.bump()? {
            b'\\' => self.read_command(),
            b'{' => Token::LBrace,
            b'}' => Token::RBrace,
            b'^' => Token::Caret,
            b'_' => Token::Underscore,
            b'&' => Token::Align,
            // A tie is a non-breaking interword space.
            b'~' => Token::Space(6),
            c if c.is_ascii_alphanumeric() || c == b'.' => {
                let start = self.pos - 1;
                while matches!(self.peek(), Some(b) if b.is_ascii_alphanumeric() || b == b'.') {
                    self.pos += 1;
                }
                Token::Literal(self.slice(start))
            }
            // Take a whole multi-byte character rather than splitting it.
            c if c >= 0x80 => Token::Char(self.read_utf8()),
            c => Token::Char(char::from(c)),
        })
    }

    /// Finish the UTF-8 sequence whose lead byte was just taken; invalid input yields U+FFFD.
    fn read_utf8(&mut self) -> char {
        let start = self.pos - 1;
        while matches!(self.peek(), Some(b) if b & 0xC0 == 0x80) {
            self.pos += 1;
        }
        self.slice(start)
            .chars()
            .next()
            .unwrap_or(char::REPLACEMENT_CHARACTER)
    }

    fn read_command(&mut self) -> Token<'a> {
        match self.peek() {
            None => Token::Char('\\'),
            Some(c) if c.is_ascii_alphabetic() => {
                let start = self.pos;
                while matches!(self.peek(), Some(b) if b.is_ascii_alphabetic()) {
                    self.pos += 1;
                }
                let name = self.slice(start);
                // A word command swallows the space that terminates it.
                if matches!(self.peek(), Some(b' ' | b'\t')) {
                    self.pos += 1;
                }
                spacing_mu(name).map_or(Token::Command(name), Token::Space)
            }
            Some(b'\\') => {
                self.pos += 1;
                Token::RowBreak
            }
            Some(c @ (b'{' | b'}' | b'$' | b'%' | b'#' | b'&' | b'_' | b'^')) => {
                self.pos += 1;
                Token::Char(char::from(c))
            }
            // `\|` is ‖, distinct from `|`, so it must be a command; the others are accents.
            Some(b'|' | b'\'' | b'`' | b'"' | b'~' | b'=' | b'.') => {
                let start = self.pos;
                self.pos += 1;
                Token::Command(self.slice(start))
            }
            Some(c @ (b',' | b';' | b'!' | b':' | b' ')) => {
                self.pos += 1;
                Token::Space(match c {
                    b',' => 3,
                    b':' => 4,
                    b';' => 5,
                    b' ' => 6,
                    // `\!`
                    _ => -3,
                })
            }
            // `read_utf8`, not one byte: `\α` must stay whole so spans never end mid-character.
            Some(_) => {
                self.pos += 1;
                Token::Char(self.read_utf8())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Token<'_>> {
        Tokenizer::new(s)
            .tokenize()
            .into_iter()
            .map(|s| s.tok)
            .collect()
    }

    #[test]
    fn a_backslash_before_a_multibyte_character_consumes_all_of_it() {
        assert_eq!(toks(r"\α"), vec![Token::Char('α')]);
        assert_eq!(toks(r"\∑"), vec![Token::Char('∑')]);
        assert_eq!(toks(r"\🜛"), vec![Token::Char('🜛')]);

        let spans = Tokenizer::new(r"\α").tokenize();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, r"\α".len());
    }

    #[test]
    fn splits_commands_from_literals() {
        assert_eq!(
            toks("\\frac{1}{2}"),
            vec![
                Token::Command("frac"),
                Token::LBrace,
                Token::Literal("1"),
                Token::RBrace,
                Token::LBrace,
                Token::Literal("2"),
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn row_break_is_its_own_token() {
        assert_eq!(
            toks("a \\\\ b"),
            vec![Token::Literal("a"), Token::RowBreak, Token::Literal("b"),]
        );
    }

    #[test]
    fn escaped_characters_are_literal() {
        assert_eq!(toks("\\{"), vec![Token::Char('{')]);
        assert_eq!(toks("\\%"), vec![Token::Char('%')]);
        assert_eq!(toks("\\_"), vec![Token::Char('_')]);
    }

    #[test]
    fn alignment_and_escaped_ampersand_are_distinct() {
        assert_eq!(toks("&"), vec![Token::Align]);
        assert_eq!(toks("\\&"), vec![Token::Char('&')]);
    }

    #[test]
    fn spacing_commands_carry_a_width_in_mu() {
        assert_eq!(toks("\\,"), vec![Token::Space(3)]);
        assert_eq!(toks("\\quad"), vec![Token::Space(18)]);
        assert_eq!(toks("\\qquad"), vec![Token::Space(36)]);
        assert_eq!(toks("\\!"), vec![Token::Space(-3)]);
    }

    #[test]
    fn double_bar_is_a_command() {
        assert_eq!(toks("\\|"), vec![Token::Command("|")]);
        assert_eq!(toks("|"), vec![Token::Char('|')]);
    }

    #[test]
    fn spans_bracket_each_token() {
        let src = "x + 1";
        for s in Tokenizer::new(src).tokenize() {
            assert!(s.start < s.end && s.end <= src.len(), "bad span {s:?}");
        }
    }

    #[test]
    fn multibyte_characters_stay_whole() {
        assert_eq!(toks("α"), vec![Token::Char('α')]);
        assert_eq!(
            toks("x ≤ y"),
            vec![Token::Literal("x"), Token::Char('≤'), Token::Literal("y"),]
        );
    }

    #[test]
    fn a_trailing_backslash_does_not_loop() {
        assert_eq!(toks("x\\"), vec![Token::Literal("x"), Token::Char('\\')]);
    }
}
