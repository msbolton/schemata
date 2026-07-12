// src/ir/syntax/parser.rs
//! Recursive-descent parser for the .schemata DSL.

use std::fmt;

use crate::ir::model::*;
use crate::ir::syntax::lexer::{lex, Token, TokenKind};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for ParseError {}

pub fn parse(src: &str) -> Result<Schema, ParseError> {
    let tokens = lex(src).map_err(|e| ParseError {
        message: e.message,
        line: e.line,
        col: e.col,
    })?;
    Parser {
        tokens,
        pos: 0,
        imports: Vec::new(),
    }
    .parse_file()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    imports: Vec<String>,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn err_here(&self, message: String) -> ParseError {
        let (line, col) = self.peek().map(|t| (t.line, t.col)).unwrap_or_else(|| {
            self.tokens
                .last()
                .map(|t| (t.line, t.col))
                .unwrap_or((1, 1))
        });
        ParseError { message, line, col }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, ParseError> {
        match self.peek_kind() {
            Some(TokenKind::Ident(_)) => {
                let Some(Token {
                    kind: TokenKind::Ident(s),
                    ..
                }) = self.bump()
                else {
                    unreachable!()
                };
                Ok(s)
            }
            _ => Err(self.err_here(format!("expected {what}"))),
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<(), ParseError> {
        if self.peek_kind() == Some(&kind) {
            self.bump();
            Ok(())
        } else {
            Err(self.err_here(format!("expected {what}")))
        }
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// True if the current token is Ident(word) — contextual keyword check.
    fn at_keyword(&self, word: &str) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Ident(s)) if s == word)
    }

    fn dotted_ident(&mut self, what: &str) -> Result<String, ParseError> {
        let mut s = self.expect_ident(what)?;
        while self.eat(&TokenKind::Dot) {
            s.push('.');
            s.push_str(&self.expect_ident("identifier after `.`")?);
        }
        Ok(s)
    }

    fn doc_lines(&mut self) -> Option<String> {
        let mut lines = Vec::new();
        while let Some(TokenKind::Doc(_)) = self.peek_kind() {
            let Some(Token {
                kind: TokenKind::Doc(s),
                ..
            }) = self.bump()
            else {
                unreachable!()
            };
            lines.push(s);
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    fn parse_file(mut self) -> Result<Schema, ParseError> {
        if !self.at_keyword("schema") {
            return Err(self.err_here("expected `schema <name>` header".into()));
        }
        self.bump();
        let name = self.dotted_ident("schema name")?;
        let annotations = self.annotations()?;

        let mut decls = Vec::new();
        while self.peek().is_some() {
            let doc = self.doc_lines();
            if self.at_keyword("record") {
                self.bump();
                decls.push(Decl::Record(self.record_decl(doc)?));
            } else if self.at_keyword("type") {
                self.bump();
                decls.push(Decl::Alias(self.alias_decl(doc)?));
            } else if self.at_keyword("enum") {
                self.bump();
                decls.push(Decl::Enum(self.enum_decl(doc)?));
            } else {
                return Err(self.err_here("expected `record`, `type`, or `enum`".into()));
            }
        }

        let mut imports = self.imports;
        imports.sort();
        imports.dedup();
        Ok(Schema {
            name,
            annotations,
            imports,
            decls,
            source_path: None,
        })
    }

    fn record_decl(&mut self, doc: Option<String>) -> Result<Record, ParseError> {
        let name = self.expect_ident("record name")?;
        let annotations = self.annotations()?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut members = Vec::new();
        while !self.eat(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(self.err_here("unclosed record body".into()));
            }
            let member_doc = self.doc_lines();
            if self.at_keyword("choice")
                && matches!(
                    self.tokens.get(self.pos + 1).map(|t| &t.kind),
                    Some(TokenKind::Ident(_))
                )
                && matches!(
                    self.tokens.get(self.pos + 2).map(|t| &t.kind),
                    Some(TokenKind::LBrace)
                )
            {
                self.bump();
                members.push(Member::Choice(self.choice_decl(member_doc)?));
            } else {
                members.push(Member::Field(self.field(member_doc)?));
            }
        }
        Ok(Record {
            name,
            doc,
            annotations,
            members,
        })
    }

    fn choice_decl(&mut self, doc: Option<String>) -> Result<Choice, ParseError> {
        let name = self.expect_ident("choice name")?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        while !self.eat(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(self.err_here("unclosed choice body".into()));
            }
            let field_doc = self.doc_lines();
            fields.push(self.field(field_doc)?);
        }
        Ok(Choice { name, doc, fields })
    }

    fn field(&mut self, doc: Option<String>) -> Result<Field, ParseError> {
        let name = self.expect_ident("field name")?;
        self.expect(TokenKind::Colon, "expected `:`")?;
        let ty = self.type_ref()?;
        let cardinality = if self.eat(&TokenKind::Question) {
            Cardinality::Optional
        } else if self.eat(&TokenKind::Star) {
            Cardinality::Many
        } else if self.eat(&TokenKind::Plus) {
            Cardinality::AtLeastOne
        } else {
            Cardinality::Required
        };
        let annotations = self.annotations()?;
        Ok(Field {
            name,
            ty,
            cardinality,
            doc,
            annotations,
        })
    }

    fn alias_decl(&mut self, doc: Option<String>) -> Result<Alias, ParseError> {
        let name = self.expect_ident("type alias name")?;
        self.expect(TokenKind::Equals, "`=`")?;
        let target = self.type_ref()?;
        let annotations = self.annotations()?;
        Ok(Alias {
            name,
            doc,
            target,
            annotations,
        })
    }

    fn enum_decl(&mut self, doc: Option<String>) -> Result<EnumDecl, ParseError> {
        let name = self.expect_ident("enum name")?;
        let annotations = self.annotations()?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut values = Vec::new();
        while !self.eat(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(self.err_here("unclosed enum body".into()));
            }
            let value_doc = self.doc_lines();
            let value_name = self.expect_ident("enum value")?;
            let value_annotations = self.annotations()?;
            values.push(EnumValue {
                name: value_name,
                doc: value_doc,
                annotations: value_annotations,
            });
        }
        Ok(EnumDecl {
            name,
            doc,
            annotations,
            values,
        })
    }

    fn type_ref(&mut self) -> Result<TypeRef, ParseError> {
        let dotted = self.dotted_ident("type name")?;
        if let Some(p) = Primitive::from_name(&dotted) {
            return Ok(TypeRef::Primitive(p));
        }
        match dotted.rsplit_once('.') {
            Some((schema, name)) => {
                self.imports.push(schema.to_string());
                Ok(TypeRef::named(Some(schema), name))
            }
            None => Ok(TypeRef::named(None, &dotted)),
        }
    }

    fn annotations(&mut self) -> Result<Vec<Annotation>, ParseError> {
        let mut out = Vec::new();
        while self.eat(&TokenKind::At) {
            let name = self.dotted_ident("annotation name")?;
            let mut args = Vec::new();
            if self.eat(&TokenKind::LParen) {
                loop {
                    match self.peek_kind() {
                        Some(TokenKind::Str(_)) => {
                            let Some(Token {
                                kind: TokenKind::Str(s),
                                ..
                            }) = self.bump()
                            else {
                                unreachable!()
                            };
                            args.push(AnnotationValue::Str(s));
                        }
                        Some(TokenKind::Int(_)) => {
                            let Some(Token {
                                kind: TokenKind::Int(v),
                                ..
                            }) = self.bump()
                            else {
                                unreachable!()
                            };
                            args.push(AnnotationValue::Int(v));
                        }
                        Some(TokenKind::Float(_)) => {
                            let Some(Token {
                                kind: TokenKind::Float(v),
                                ..
                            }) = self.bump()
                            else {
                                unreachable!()
                            };
                            args.push(AnnotationValue::Float(v));
                        }
                        Some(TokenKind::Ident(_)) => {
                            let ident = self.dotted_ident("annotation argument")?;
                            args.push(AnnotationValue::Ident(ident));
                        }
                        _ => return Err(self.err_here("expected annotation argument".into())),
                    }
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RParen, "`)`")?;
            }
            out.push(Annotation { name, args });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
schema justice.person @xml.namespace("http://example.org/person/1.0")

/// A human being involved in a case
record Person {
  /// Full legal name
  name: string @maxLength(100)
  ssn: SSN?
  aliases: string*
  citizenships: string+
  status: PersonStatus

  choice contact {
    email: string
    phone: string
  }
}

type SSN = string @pattern("\\d{3}-\\d{2}-\\d{4}")

enum PersonStatus {
  ACTIVE
  INACTIVE
}
"#;

    #[test]
    fn parses_the_full_example() {
        let schema = parse(EXAMPLE).unwrap();
        assert_eq!(schema.name, "justice.person");
        assert_eq!(
            schema.annotations,
            vec![Annotation::str(
                "xml.namespace",
                "http://example.org/person/1.0"
            )]
        );
        assert_eq!(schema.decls.len(), 3);

        let Decl::Record(person) = &schema.decls[0] else {
            panic!("expected record")
        };
        assert_eq!(person.name, "Person");
        assert_eq!(
            person.doc.as_deref(),
            Some("A human being involved in a case")
        );
        assert_eq!(person.members.len(), 6);

        let Member::Field(name) = &person.members[0] else {
            panic!()
        };
        assert_eq!(name.name, "name");
        assert_eq!(name.ty, TypeRef::Primitive(Primitive::String));
        assert_eq!(name.cardinality, Cardinality::Required);
        assert_eq!(name.doc.as_deref(), Some("Full legal name"));
        assert_eq!(
            name.annotations,
            vec![Annotation::new(
                "maxLength",
                vec![AnnotationValue::Int(100)]
            )]
        );

        let Member::Field(ssn) = &person.members[1] else {
            panic!()
        };
        assert_eq!(ssn.ty, TypeRef::named(None, "SSN"));
        assert_eq!(ssn.cardinality, Cardinality::Optional);

        let Member::Field(aliases) = &person.members[2] else {
            panic!()
        };
        assert_eq!(aliases.cardinality, Cardinality::Many);

        let Member::Field(cit) = &person.members[3] else {
            panic!()
        };
        assert_eq!(cit.cardinality, Cardinality::AtLeastOne);

        let Member::Choice(contact) = &person.members[5] else {
            panic!("expected choice")
        };
        assert_eq!(contact.name, "contact");
        assert_eq!(contact.fields.len(), 2);

        let Decl::Alias(ssn_ty) = &schema.decls[1] else {
            panic!("expected alias")
        };
        assert_eq!(ssn_ty.name, "SSN");
        assert_eq!(ssn_ty.target, TypeRef::Primitive(Primitive::String));
        assert_eq!(
            ssn_ty.annotations,
            vec![Annotation::str("pattern", r"\d{3}-\d{2}-\d{4}")]
        );

        let Decl::Enum(status) = &schema.decls[2] else {
            panic!("expected enum")
        };
        assert_eq!(status.name, "PersonStatus");
        assert_eq!(status.values.len(), 2);
        assert_eq!(status.values[0].name, "ACTIVE");
    }

    #[test]
    fn qualified_type_ref_records_import() {
        let src = "schema a.b\nrecord R {\n  x: other.pkg.Widget\n}";
        let schema = parse(src).unwrap();
        let Decl::Record(r) = &schema.decls[0] else {
            panic!()
        };
        let Member::Field(f) = &r.members[0] else {
            panic!()
        };
        assert_eq!(f.ty, TypeRef::named(Some("other.pkg"), "Widget"));
        assert_eq!(schema.imports, vec!["other.pkg".to_string()]);
    }

    #[test]
    fn missing_schema_header_errors() {
        let err = parse("record R {}").unwrap_err();
        assert!(err.message.contains("schema"), "got: {}", err.message);
        assert_eq!(err.line, 1);
    }

    #[test]
    fn error_reports_line_and_column() {
        let err = parse("schema a\nrecord R {\n  bad field\n}").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("expected `:`"), "got: {}", err.message);
    }

    #[test]
    fn keywords_usable_as_field_names() {
        let src = "schema a\nrecord R {\n  type: string\n  record: string\n}";
        let schema = parse(src).unwrap();
        let Decl::Record(r) = &schema.decls[0] else {
            panic!()
        };
        assert_eq!(r.members.len(), 2);
    }
}
