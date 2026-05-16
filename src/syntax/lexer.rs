//! Lexer for the Vita programming language.
//!
//! Converts source text into a stream of tokens.

use crate::diagnostics::error::{CompileError, ErrorKind, Result, Span};
use crate::syntax::token::{lookup_keyword, Token, TokenKind};

/// The lexer state.
pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    /// Tokenize the entire source into a vector of tokens.
    pub fn tokenize(source: &str) -> Result<Vec<Token>> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();

        loop {
            lexer.skip_whitespace_and_comments();
            if lexer.is_eof() {
                tokens.push(Token::new(TokenKind::Eof, "", lexer.line, lexer.col));
                break;
            }

            let token = lexer.next_token()?;
            tokens.push(token);
        }

        Ok(tokens)
    }

    // --- Helpers ---

    fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.source.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        if self.is_eof() {
            return None;
        }
        let ch = self.source[self.pos];
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') | Some('\n') => {
                    self.advance();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    // Line comment
                    while let Some(ch) = self.peek() {
                        if ch == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn make_span(&self) -> Span {
        Span::new(self.line, self.col, self.pos)
    }

    fn error(&self, kind: ErrorKind) -> CompileError {
        CompileError::new(kind, self.make_span())
    }

    // --- Token parsers ---

    fn next_token(&mut self) -> Result<Token> {
        let line = self.line;
        let col = self.col;

        let ch = self.peek().unwrap();
        let kind = match ch {
            // Two-char operators must be checked first
            '?' => {
                self.advance();
                if self.peek() == Some('?') {
                    self.advance();
                    TokenKind::DoubleQuestion
                } else {
                    TokenKind::Question
                }
            }
            '!' => {
                self.advance();
                match self.peek() {
                    Some('?') => {
                        self.advance();
                        TokenKind::BangQuestion
                    }
                    Some('$') => {
                        self.advance();
                        TokenKind::BangDollar
                    }
                    Some('!') => {
                        self.advance();
                        TokenKind::DoubleBang
                    }
                    Some('=') => {
                        self.advance();
                        TokenKind::BangEq
                    }
                    _ => TokenKind::Bang,
                }
            }
            '$' => {
                self.advance();
                TokenKind::Dollar
            }
            '*' => {
                self.advance();
                if self.peek() == Some('?') {
                    self.advance();
                    TokenKind::StarQuestion
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            '+' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::PlusEq
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                self.advance();
                if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::Arrow
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::MinusEq
                } else {
                    TokenKind::Minus
                }
            }
            '/' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::PercentEq
                } else {
                    TokenKind::Percent
                }
            }
            '&' => {
                self.advance();
                if self.peek() == Some('&') {
                    self.advance();
                    TokenKind::AmpAmp
                } else {
                    TokenKind::Amp
                }
            }
            '|' => {
                self.advance();
                if self.peek() == Some('|') {
                    self.advance();
                    TokenKind::PipePipe
                } else {
                    TokenKind::Pipe
                }
            }
            '^' => {
                self.advance();
                TokenKind::Caret
            }
            '<' => {
                self.advance();
                if self.peek() == Some('<') {
                    self.advance();
                    TokenKind::LtLt
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                self.advance();
                if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::GtGt
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '=' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::EqEq
                } else if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::FatArrow
                } else {
                    TokenKind::Eq
                }
            }
            ':' => {
                self.advance();
                if self.peek() == Some(':') {
                    self.advance();
                    TokenKind::DoubleColon
                } else {
                    TokenKind::Colon
                }
            }
            '(' => {
                self.advance();
                TokenKind::LParen
            }
            ')' => {
                self.advance();
                TokenKind::RParen
            }
            '{' => {
                self.advance();
                TokenKind::LBrace
            }
            '}' => {
                self.advance();
                TokenKind::RBrace
            }
            '[' => {
                self.advance();
                TokenKind::LBracket
            }
            ']' => {
                self.advance();
                TokenKind::RBracket
            }
            ',' => {
                self.advance();
                TokenKind::Comma
            }
            ';' => {
                self.advance();
                TokenKind::Semicolon
            }
            '.' => {
                self.advance();
                if self.peek() == Some('.') {
                    self.advance();
                    TokenKind::DotDot
                } else {
                    TokenKind::Dot
                }
            }
            '_' => {
                self.advance();
                TokenKind::Underscore
            }
            '#' => {
                self.advance();
                TokenKind::Hash
            }

            // String literal
            '"' => {
                self.advance(); // consume opening quote
                return self.read_string(line, col);
            }

            // Char literal
            '\'' => {
                self.advance();
                return self.read_char_literal(line, col);
            }

            // Number literal (integer or float)
            '0'..='9' => {
                return self.read_number(line, col);
            }

            // Identifier or keyword
            'a'..='z' | 'A'..='Z' => {
                return self.read_identifier(line, col);
            }

            _ => {
                self.advance();
                return Err(self.error(ErrorKind::UnexpectedChar(ch)));
            }
        };

        // Reconstruct text from start position
        let start_pos = if self.pos > 0 {
            // We already advanced past the token characters
            // So reconstruct from source
            self.pos // approximate
        } else {
            0
        };
        let _text = self.source[start_pos.saturating_sub(2)..self.pos]
            .iter()
            .collect::<String>();

        Ok(Token::new(kind, &kind.to_string(), line, col))
    }

    fn read_identifier(&mut self, line: usize, col: usize) -> Result<Token> {
        let mut text = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                text.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let kind = if let Some(kw) = lookup_keyword(&text) {
            kw
        } else {
            TokenKind::Ident
        };

        Ok(Token::new(kind, &text, line, col))
    }

    fn read_number(&mut self, line: usize, col: usize) -> Result<Token> {
        let mut text = String::new();
        let mut is_float = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                text.push(ch);
                self.advance();
            } else if ch == '.' {
                // Check if this is a decimal point or a method call
                // e.g. "3.14" vs "tuple.0"
                if let Some(next) = self.peek_at(1) {
                    if next.is_ascii_digit() {
                        is_float = true;
                        text.push(ch);
                        self.advance();
                        continue;
                    }
                }
                // Not followed by digit: treat as dot operator
                break;
            } else {
                break;
            }
        }

        if matches!(self.peek(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_') {
            while let Some(ch) = self.peek() {
                if ch.is_alphanumeric() || ch == '_' {
                    text.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
            return Err(CompileError::new(
                ErrorKind::InvalidNumber(text),
                Span::new(line, col, self.pos),
            ));
        }

        let kind = if is_float {
            TokenKind::FloatLiteral
        } else {
            TokenKind::IntLiteral
        };

        Ok(Token::new(kind, &text, line, col))
    }

    fn read_string(&mut self, line: usize, col: usize) -> Result<Token> {
        let mut text = String::new();
        while let Some(ch) = self.peek() {
            match ch {
                '"' => {
                    self.advance(); // consume closing quote
                    return Ok(Token::new(TokenKind::StringLiteral, &text, line, col));
                }
                '\\' => {
                    self.advance();
                    match self.peek() {
                        Some('n') => {
                            text.push('\n');
                            self.advance();
                        }
                        Some('t') => {
                            text.push('\t');
                            self.advance();
                        }
                        Some('r') => {
                            text.push('\r');
                            self.advance();
                        }
                        Some('\\') => {
                            text.push('\\');
                            self.advance();
                        }
                        Some('"') => {
                            text.push('"');
                            self.advance();
                        }
                        Some(c) => {
                            text.push(c);
                            self.advance();
                        }
                        None => return Err(self.error(ErrorKind::UnterminatedString)),
                    }
                }
                '\n' => return Err(self.error(ErrorKind::UnterminatedString)),
                _ => {
                    text.push(ch);
                    self.advance();
                }
            }
        }
        Err(self.error(ErrorKind::UnterminatedString))
    }

    fn read_char_literal(&mut self, line: usize, col: usize) -> Result<Token> {
        let ch = match self.peek() {
            Some('\\') => {
                self.advance();
                match self.peek() {
                    Some('n') => {
                        self.advance();
                        '\n'
                    }
                    Some('t') => {
                        self.advance();
                        '\t'
                    }
                    Some('r') => {
                        self.advance();
                        '\r'
                    }
                    Some('\\') => {
                        self.advance();
                        '\\'
                    }
                    Some('\'') => {
                        self.advance();
                        '\''
                    }
                    Some(c) => {
                        self.advance();
                        c
                    }
                    None => return Err(self.error(ErrorKind::UnterminatedChar)),
                }
            }
            Some(c) => {
                self.advance();
                c
            }
            None => return Err(self.error(ErrorKind::UnterminatedChar)),
        };

        if self.peek() == Some('\'') {
            self.advance();
            Ok(Token::new(
                TokenKind::CharLiteral,
                &ch.to_string(),
                line,
                col,
            ))
        } else {
            Err(self.error(ErrorKind::UnterminatedChar))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let tokens = Lexer::tokenize("let x = 10;").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::KwLet);
        assert_eq!(tokens[1].kind, TokenKind::Ident);
        assert_eq!(tokens[2].kind, TokenKind::Eq);
        assert_eq!(tokens[3].kind, TokenKind::IntLiteral);
        assert_eq!(tokens[4].kind, TokenKind::Semicolon);
    }

    #[test]
    fn test_control_flow_symbols() {
        let tokens = Lexer::tokenize("? x > 0 { } ! { }").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Question);
        assert_eq!(tokens[6].kind, TokenKind::Bang);
    }

    #[test]
    fn test_def_and_impl() {
        let tokens =
            Lexer::tokenize("def Dog { name: str, } impl Dog { fn bark(self) { } }").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::KwDef);
        assert_eq!(tokens[8].kind, TokenKind::KwImpl);
        assert_eq!(tokens[11].kind, TokenKind::KwFn);
    }

    #[test]
    fn test_float_literal() {
        let tokens = Lexer::tokenize("3.14").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::FloatLiteral);
    }

    #[test]
    fn test_numeric_suffixes_are_rejected() {
        assert!(Lexer::tokenize("42i32").is_err());
        assert!(Lexer::tokenize("42u64").is_err());
        assert!(Lexer::tokenize("2.5f32").is_err());
    }
}
