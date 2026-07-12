// src/ir/syntax/emitter.rs
//! Canonical pretty-printer for the .schemata DSL.
//!
//! Canonical form: schema header first, blank line between declarations,
//! two-space indent, docs as `///` lines, annotations inline.

use crate::ir::model::*;

pub fn emit(schema: &Schema) -> String {
    let mut out = String::new();
    out.push_str("schema ");
    out.push_str(&schema.name);
    for a in &schema.annotations {
        out.push(' ');
        out.push_str(&a.to_string());
    }
    out.push('\n');

    for decl in &schema.decls {
        out.push('\n');
        match decl {
            Decl::Record(r) => emit_record(r, &mut out),
            Decl::Alias(a) => emit_alias(a, &mut out),
            Decl::Enum(e) => emit_enum(e, &mut out),
        }
    }
    out
}

fn emit_doc(doc: &Option<String>, indent: &str, out: &mut String) {
    if let Some(doc) = doc {
        for line in doc.lines() {
            out.push_str(indent);
            out.push_str("/// ");
            out.push_str(line);
            out.push('\n');
        }
    }
}

fn emit_annotations(annotations: &[Annotation], out: &mut String) {
    for a in annotations {
        out.push(' ');
        out.push_str(&a.to_string());
    }
}

fn emit_record(r: &Record, out: &mut String) {
    emit_doc(&r.doc, "", out);
    out.push_str("record ");
    out.push_str(&r.name);
    emit_annotations(&r.annotations, out);
    out.push_str(" {\n");
    let mut first = true;
    let mut prev_was_choice = false;
    for m in &r.members {
        match m {
            Member::Field(f) => {
                if prev_was_choice {
                    out.push('\n');
                }
                emit_field(f, "  ", out);
                prev_was_choice = false;
            }
            Member::Choice(c) => {
                if !first {
                    out.push('\n');
                }
                emit_doc(&c.doc, "  ", out);
                out.push_str("  choice ");
                out.push_str(&c.name);
                out.push_str(" {\n");
                for f in &c.fields {
                    emit_field(f, "    ", out);
                }
                out.push_str("  }\n");
                prev_was_choice = true;
            }
        }
        first = false;
    }
    out.push_str("}\n");
}

fn emit_field(f: &Field, indent: &str, out: &mut String) {
    emit_doc(&f.doc, indent, out);
    out.push_str(indent);
    out.push_str(&f.name);
    out.push_str(": ");
    out.push_str(&f.ty.to_string());
    match f.cardinality {
        Cardinality::Required => {}
        Cardinality::Optional => out.push('?'),
        Cardinality::Many => out.push('*'),
        Cardinality::AtLeastOne => out.push('+'),
    }
    emit_annotations(&f.annotations, out);
    out.push('\n');
}

fn emit_alias(a: &Alias, out: &mut String) {
    emit_doc(&a.doc, "", out);
    out.push_str("type ");
    out.push_str(&a.name);
    out.push_str(" = ");
    out.push_str(&a.target.to_string());
    emit_annotations(&a.annotations, out);
    out.push('\n');
}

fn emit_enum(e: &EnumDecl, out: &mut String) {
    emit_doc(&e.doc, "", out);
    out.push_str("enum ");
    out.push_str(&e.name);
    emit_annotations(&e.annotations, out);
    out.push_str(" {\n");
    for v in &e.values {
        emit_doc(&v.doc, "  ", out);
        out.push_str("  ");
        out.push_str(&v.name);
        emit_annotations(&v.annotations, out);
        out.push('\n');
    }
    out.push_str("}\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::syntax::parser::parse;

    const EXAMPLE: &str = r#"schema justice.person @xml.namespace("http://example.org/person/1.0")

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
    fn canonical_text_round_trips_exactly() {
        let schema = parse(EXAMPLE).unwrap();
        assert_eq!(emit(&schema), EXAMPLE);
    }

    #[test]
    fn emit_then_parse_is_identity_on_model() {
        let schema = parse(EXAMPLE).unwrap();
        let reparsed = parse(&emit(&schema)).unwrap();
        assert_eq!(schema, reparsed);
    }

    #[test]
    fn multiline_docs_emit_one_slash_block_per_line() {
        let src = "schema a\n\n/// line one\n/// line two\nrecord R {\n  x: string\n}\n";
        let schema = parse(src).unwrap();
        let out = emit(&schema);
        assert!(out.contains("/// line one\n/// line two\n"));
        assert_eq!(parse(&out).unwrap(), schema);
    }
}
