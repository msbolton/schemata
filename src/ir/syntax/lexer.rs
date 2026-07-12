//! Tokenizer for the .schemata DSL.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Str(String),
    Int(i64),
    Float(f64),
    /// One `///` doc-comment line (one leading separator space and any
    /// trailing whitespace stripped; interior indentation preserved).
    Doc(String),
    LBrace,
    RBrace,
    LParen,
    RParen,
    Colon,
    Comma,
    Dot,
    Equals,
    At,
    Question,
    Star,
    Plus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let (mut line, mut col) = (1usize, 1usize);

    while i < chars.len() {
        let c = chars[i];
        let (tline, tcol) = (line, col);

        match c {
            '\n' => {
                i += 1;
                line += 1;
                col = 1;
            }
            c if c.is_whitespace() => {
                i += 1;
                col += 1;
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                let is_doc = chars.get(i + 2) == Some(&'/') && chars.get(i + 3) != Some(&'/');
                let start = i + if is_doc { 3 } else { 2 };
                let mut end = start;
                while end < chars.len() && chars[end] != '\n' {
                    end += 1;
                }
                if is_doc {
                    let text: String = chars[start..end].iter().collect();
                    // Strip exactly one leading space (the canonical `/// `
                    // separator) so interior indentation survives round-trips.
                    let text = text.strip_prefix(' ').unwrap_or(&text).trim_end();
                    tokens.push(Token {
                        kind: TokenKind::Doc(text.to_string()),
                        line: tline,
                        col: tcol,
                    });
                }
                col += end - i;
                i = end;
            }
            '{' => {
                tokens.push(Token {
                    kind: TokenKind::LBrace,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '}' => {
                tokens.push(Token {
                    kind: TokenKind::RBrace,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '(' => {
                tokens.push(Token {
                    kind: TokenKind::LParen,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            ')' => {
                tokens.push(Token {
                    kind: TokenKind::RParen,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            ':' => {
                tokens.push(Token {
                    kind: TokenKind::Colon,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            ',' => {
                tokens.push(Token {
                    kind: TokenKind::Comma,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '.' => {
                tokens.push(Token {
                    kind: TokenKind::Dot,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '=' => {
                tokens.push(Token {
                    kind: TokenKind::Equals,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '@' => {
                tokens.push(Token {
                    kind: TokenKind::At,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '?' => {
                tokens.push(Token {
                    kind: TokenKind::Question,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '*' => {
                tokens.push(Token {
                    kind: TokenKind::Star,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '+' => {
                tokens.push(Token {
                    kind: TokenKind::Plus,
                    line: tline,
                    col: tcol,
                });
                i += 1;
                col += 1;
            }
            '"' => {
                let mut s = String::new();
                let mut j = i + 1;
                let mut closed = false;
                while j < chars.len() {
                    match chars[j] {
                        '\\' => {
                            match chars.get(j + 1) {
                                Some('"') => s.push('"'),
                                Some('\\') => s.push('\\'),
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(other) => {
                                    return Err(LexError {
                                        message: format!("invalid escape `\\{other}` in string"),
                                        line,
                                        col,
                                    });
                                }
                                None => {
                                    return Err(LexError {
                                        message: "unterminated string literal".to_string(),
                                        line,
                                        col,
                                    });
                                }
                            }
                            j += 2;
                        }
                        '"' => {
                            closed = true;
                            j += 1;
                            break;
                        }
                        '\n' => {
                            return Err(LexError {
                                message: "unterminated string literal".to_string(),
                                line,
                                col,
                            });
                        }
                        ch => {
                            s.push(ch);
                            j += 1;
                        }
                    }
                }
                if !closed {
                    return Err(LexError {
                        message: "unterminated string literal".to_string(),
                        line,
                        col,
                    });
                }
                tokens.push(Token {
                    kind: TokenKind::Str(s),
                    line: tline,
                    col: tcol,
                });
                col += j - i;
                i = j;
            }
            c if c.is_ascii_digit()
                || (c == '-' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) =>
            {
                let mut j = i + 1;
                let mut is_float = false;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                    if chars[j] == '.' {
                        if chars.get(j + 1).is_some_and(|d| d.is_ascii_digit()) {
                            is_float = true;
                        } else {
                            break;
                        }
                    }
                    j += 1;
                }
                let text: String = chars[i..j].iter().collect();
                if is_float {
                    match text.parse::<f64>() {
                        Ok(v) => tokens.push(Token {
                            kind: TokenKind::Float(v),
                            line: tline,
                            col: tcol,
                        }),
                        Err(_) => {
                            return Err(LexError {
                                message: format!("invalid number `{text}`"),
                                line,
                                col,
                            });
                        }
                    }
                } else {
                    match text.parse::<i64>() {
                        Ok(v) => tokens.push(Token {
                            kind: TokenKind::Int(v),
                            line: tline,
                            col: tcol,
                        }),
                        Err(_) => {
                            return Err(LexError {
                                message: format!("invalid number `{text}`"),
                                line,
                                col,
                            });
                        }
                    }
                }
                col += j - i;
                i = j;
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let text: String = chars[i..j].iter().collect();
                tokens.push(Token {
                    kind: TokenKind::Ident(text),
                    line: tline,
                    col: tcol,
                });
                col += j - i;
                i = j;
            }
            other => {
                return Err(LexError {
                    message: format!("unexpected character `{other}`"),
                    line,
                    col,
                });
            }
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_a_field_line() {
        assert_eq!(
            kinds(r#"ssn: SSN? @maxLength(100)"#),
            vec![
                TokenKind::Ident("ssn".into()),
                TokenKind::Colon,
                TokenKind::Ident("SSN".into()),
                TokenKind::Question,
                TokenKind::At,
                TokenKind::Ident("maxLength".into()),
                TokenKind::LParen,
                TokenKind::Int(100),
                TokenKind::RParen,
            ]
        );
    }

    #[test]
    fn lexes_strings_with_escapes() {
        assert_eq!(
            kinds(r#"@pattern("\\d{3}-\"x\"")"#),
            vec![
                TokenKind::At,
                TokenKind::Ident("pattern".into()),
                TokenKind::LParen,
                TokenKind::Str(r#"\d{3}-"x""#.into()),
                TokenKind::RParen,
            ]
        );
    }

    #[test]
    fn doc_comments_are_tokens_and_line_comments_are_skipped() {
        let toks = lex("/// hello doc\nname: string // trailing\n").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Doc("hello doc".into()));
        assert_eq!(toks[1].kind, TokenKind::Ident("name".into()));
        assert!(!toks
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "trailing")));
    }

    #[test]
    fn tracks_line_and_column() {
        let toks = lex("record Person {\n  name: string\n}").unwrap();
        let name_tok = toks
            .iter()
            .find(|t| t.kind == TokenKind::Ident("name".into()))
            .unwrap();
        assert_eq!((name_tok.line, name_tok.col), (2, 3));
    }

    #[test]
    fn unterminated_string_is_an_error() {
        let err = lex(r#"@pattern("abc"#).unwrap_err();
        assert!(err.message.contains("unterminated"));
        assert_eq!(err.line, 1);
    }

    #[test]
    fn dots_stars_pluses() {
        assert_eq!(
            kinds("schema a.b\nx: T*\ny: U+"),
            vec![
                TokenKind::Ident("schema".into()),
                TokenKind::Ident("a".into()),
                TokenKind::Dot,
                TokenKind::Ident("b".into()),
                TokenKind::Ident("x".into()),
                TokenKind::Colon,
                TokenKind::Ident("T".into()),
                TokenKind::Star,
                TokenKind::Ident("y".into()),
                TokenKind::Colon,
                TokenKind::Ident("U".into()),
                TokenKind::Plus,
            ]
        );
    }
}
