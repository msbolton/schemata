// src/writers/proto/lower.rs
//! Lower ir::Schema to the ProtoFile model.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};

use crate::convert::Warning;
use crate::ir::model::{
    Alias, Annotation, AnnotationValue, Cardinality, Decl, EnumDecl, Field, Member, Primitive,
    Record, Schema, TypeRef,
};
use crate::readers::xsd::transform::naming::{
    enum_unknown_name, enum_value_name, package_to_import_path, to_snake_case,
};
use crate::writers::proto::model::{
    FieldCardinality, ProtoEnum, ProtoEnumValue, ProtoField, ProtoFile, ProtoMessage, ProtoOneof,
};

/// Lower one schema. `all` is the full schema set (for alias resolution
/// across schemas).
pub fn lower(schema: &Schema, all: &[Schema]) -> Result<(ProtoFile, Vec<Warning>)> {
    let mut ctx = Ctx {
        aliases: index_aliases(all),
        warnings: Vec::new(),
        current: schema.name.clone(),
    };

    let mut messages = Vec::new();
    let mut enums = Vec::new();
    for decl in &schema.decls {
        match decl {
            Decl::Record(r) => messages.push(ctx.lower_record(r)?),
            Decl::Enum(e) => enums.push(ctx.lower_enum(e)),
            Decl::Alias(_) => {} // resolved at use sites
        }
    }

    let file = ProtoFile {
        syntax: "proto3".to_string(),
        package: schema.name.clone(),
        imports: schema
            .imports
            .iter()
            .map(|i| package_to_import_path(i))
            .collect(),
        options: Vec::new(),
        messages,
        enums,
        source_xsd_path: schema.source_path.clone(),
    };
    Ok((file, ctx.warnings))
}

struct Ctx {
    /// (schema name, alias name) -> Alias
    aliases: HashMap<(String, String), Alias>,
    warnings: Vec<Warning>,
    current: String,
}

fn index_aliases(all: &[Schema]) -> HashMap<(String, String), Alias> {
    let mut map = HashMap::new();
    for s in all {
        for d in &s.decls {
            if let Decl::Alias(a) = d {
                map.insert((s.name.clone(), a.name.clone()), a.clone());
            }
        }
    }
    map
}

impl Ctx {
    fn warn(&mut self, msg: String) {
        self.warnings.push(Warning::new(msg));
    }

    fn lower_record(&mut self, r: &Record) -> Result<ProtoMessage> {
        // First pass: collect pinned numbers to detect duplicates and reserve
        // them, warning on pins that are present but unusable.
        let mut pinned: HashSet<u32> = HashSet::new();
        for f in r.members.iter().flat_map(member_fields) {
            match pinned_number(f) {
                Some(n) => {
                    if !pinned.insert(n) {
                        bail!("{}.{}: duplicate @proto.field({})", r.name, f.name, n);
                    }
                }
                None => {
                    if let Some(a) = pin_annotation(f) {
                        self.warn(format!(
                            "{}.{}: {} is not a usable field number (must be an integer in 1..={}) — auto-assigned",
                            r.name, f.name, a, MAX_FIELD_NUMBER
                        ));
                    }
                }
            }
        }

        let mut next = 1u32;
        let mut fields = Vec::new();
        let mut oneofs = Vec::new();

        for m in &r.members {
            match m {
                Member::Field(f) => {
                    let number = alloc_number(f, &pinned, &mut next);
                    fields.push(self.lower_field(r, f, number));
                }
                Member::Choice(c) => {
                    let mut oneof_fields = Vec::new();
                    for f in &c.fields {
                        if f.cardinality != Cardinality::Required {
                            self.warn(format!(
                                "{}.{}.{}: oneof fields cannot carry cardinality — treated as singular",
                                r.name, c.name, f.name
                            ));
                        }
                        let number = alloc_number(f, &pinned, &mut next);
                        let mut pf = self.lower_field(r, f, number);
                        pf.cardinality = FieldCardinality::Singular;
                        oneof_fields.push(pf);
                    }
                    oneofs.push(ProtoOneof {
                        name: to_snake_case(&c.name),
                        fields: oneof_fields,
                    });
                }
            }
        }

        Ok(ProtoMessage {
            name: r.name.clone(),
            fields,
            oneofs,
            nested_messages: Vec::new(),
            nested_enums: Vec::new(),
            documentation: r.doc.clone(),
        })
    }

    fn lower_field(&mut self, r: &Record, f: &Field, number: u32) -> ProtoField {
        let context = format!("{}.{}", r.name, f.name);
        let type_name = self.lower_type(&f.ty, &context);

        let cardinality = match f.cardinality {
            Cardinality::Required => FieldCardinality::Singular,
            Cardinality::Optional => FieldCardinality::Optional,
            Cardinality::Many => FieldCardinality::Repeated,
            Cardinality::AtLeastOne => {
                self.warn(format!(
                    "{context}: proto cannot enforce at-least-one — emitted as repeated"
                ));
                FieldCardinality::Repeated
            }
        };

        for a in &f.annotations {
            if a.name != "proto.field" {
                self.warn(format!(
                    "{context}: proto cannot express @{} — dropped",
                    a.name
                ));
            }
        }

        ProtoField {
            name: to_snake_case(&f.name),
            number,
            type_name,
            cardinality,
            documentation: f.doc.clone(),
        }
    }

    /// Resolve aliases transitively, then map to a proto type string.
    fn lower_type(&mut self, ty: &TypeRef, context: &str) -> String {
        let current = self.current.clone();
        let mut visited = HashSet::new();
        self.lower_type_in(ty, context, &current, &mut visited)
    }

    /// `current` is the schema whose namespace unqualified names resolve in
    /// (the alias's defining schema during resolution). `visited` guards
    /// against alias cycles.
    fn lower_type_in(
        &mut self,
        ty: &TypeRef,
        context: &str,
        current: &str,
        visited: &mut HashSet<(String, String)>,
    ) -> String {
        match ty {
            TypeRef::Primitive(p) => self.lower_primitive(*p, context),
            TypeRef::Named { schema, name } => {
                let schema_name = schema.clone().unwrap_or_else(|| current.to_string());
                if let Some(alias) = self
                    .aliases
                    .get(&(schema_name.clone(), name.clone()))
                    .cloned()
                {
                    if !visited.insert((schema_name.clone(), name.clone())) {
                        self.warn(format!(
                            "{context}: alias cycle involving `{schema_name}.{name}` — emitted as string"
                        ));
                        return "string".into();
                    }
                    for a in &alias.annotations {
                        self.warn(format!(
                            "{context}: proto cannot express @{} (from type {}) — dropped",
                            a.name, alias.name
                        ));
                    }
                    // Recurse in the alias's defining schema, so its
                    // unqualified target names resolve there.
                    return self.lower_type_in(&alias.target, context, &schema_name, visited);
                }
                if schema_name == self.current {
                    name.clone()
                } else {
                    format!("{schema_name}.{name}")
                }
            }
        }
    }

    fn lower_primitive(&mut self, p: Primitive, context: &str) -> String {
        match p {
            Primitive::String => "string".into(),
            Primitive::Bool => "bool".into(),
            Primitive::Int32 => "int32".into(),
            Primitive::Int64 => "int64".into(),
            Primitive::UInt32 => "uint32".into(),
            Primitive::UInt64 => "uint64".into(),
            Primitive::Float32 => "float".into(),
            Primitive::Float64 => "double".into(),
            Primitive::Bytes => "bytes".into(),
            Primitive::Decimal
            | Primitive::Date
            | Primitive::Time
            | Primitive::DateTime
            | Primitive::Duration
            | Primitive::Any => {
                self.warn(format!(
                    "{context}: proto has no `{}` — emitted as string",
                    p.name()
                ));
                "string".into()
            }
        }
    }

    fn lower_enum(&mut self, e: &EnumDecl) -> ProtoEnum {
        let mut values = vec![ProtoEnumValue {
            name: enum_unknown_name(&e.name),
            number: 0,
            documentation: None,
        }];
        for (i, v) in e.values.iter().enumerate() {
            let source = v
                .annotations
                .iter()
                .find(|a| a.name == "xml.value")
                .and_then(|a| match a.args.first() {
                    Some(AnnotationValue::Str(s)) => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or(&v.name);
            values.push(ProtoEnumValue {
                name: enum_value_name(&e.name, source),
                number: (i + 1) as i32,
                documentation: v.doc.clone(),
            });
        }
        ProtoEnum {
            name: e.name.clone(),
            values,
            documentation: e.doc.clone(),
        }
    }
}

fn member_fields(m: &Member) -> Vec<&Field> {
    match m {
        Member::Field(f) => vec![f],
        Member::Choice(c) => c.fields.iter().collect(),
    }
}

/// Largest field number proto allows (2^29 - 1).
const MAX_FIELD_NUMBER: i64 = 536_870_911;

/// The `@proto.field(...)` annotation on a field, if any.
fn pin_annotation(f: &Field) -> Option<&Annotation> {
    f.annotations.iter().find(|a| a.name == "proto.field")
}

/// The pinned field number, if the pin exists and is usable.
fn pinned_number(f: &Field) -> Option<u32> {
    pin_annotation(f).and_then(|a| match a.args.first() {
        Some(AnnotationValue::Int(n)) if (1..=MAX_FIELD_NUMBER).contains(n) => Some(*n as u32),
        _ => None,
    })
}

fn alloc_number(f: &Field, pinned: &HashSet<u32>, next: &mut u32) -> u32 {
    if let Some(n) = pinned_number(f) {
        return n;
    }
    while pinned.contains(next) {
        *next += 1;
    }
    let n = *next;
    *next += 1;
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::model::*;
    use crate::writers::proto::model::FieldCardinality;

    fn schema(decls: Vec<Decl>) -> Schema {
        Schema {
            name: "pkg.a".into(),
            annotations: vec![],
            imports: vec![],
            decls,
            source_path: None,
        }
    }

    fn req(name: &str, ty: TypeRef) -> Field {
        Field {
            name: name.into(),
            ty,
            cardinality: Cardinality::Required,
            doc: None,
            annotations: vec![],
        }
    }

    #[test]
    fn lowers_record_with_sequential_numbers() {
        let s = schema(vec![Decl::Record(Record {
            name: "Person".into(),
            doc: Some("A person".into()),
            annotations: vec![],
            members: vec![
                Member::Field(req("FirstName", TypeRef::Primitive(Primitive::String))),
                Member::Field(Field {
                    cardinality: Cardinality::Many,
                    ..req("Nicknames", TypeRef::Primitive(Primitive::String))
                }),
            ],
        })]);
        let (file, warnings) = lower(&s, &[s.clone()]).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(file.package, "pkg.a");
        let m = &file.messages[0];
        assert_eq!(m.fields[0].name, "first_name");
        assert_eq!(m.fields[0].number, 1);
        assert_eq!(m.fields[1].number, 2);
        assert_eq!(m.fields[1].cardinality, FieldCardinality::Repeated);
    }

    #[test]
    fn pinned_field_numbers_are_respected() {
        let mut f1 = req("a", TypeRef::Primitive(Primitive::String));
        f1.annotations.push(Annotation::new(
            "proto.field",
            vec![AnnotationValue::Int(5)],
        ));
        let f2 = req("b", TypeRef::Primitive(Primitive::String));
        let s = schema(vec![Decl::Record(Record {
            name: "R".into(),
            doc: None,
            annotations: vec![],
            members: vec![Member::Field(f1), Member::Field(f2)],
        })]);
        let (file, _) = lower(&s, &[s.clone()]).unwrap();
        assert_eq!(file.messages[0].fields[0].number, 5);
        assert_eq!(file.messages[0].fields[1].number, 1);
    }

    #[test]
    fn alias_resolves_and_warns_on_dropped_constraints() {
        let alias = Decl::Alias(Alias {
            name: "SSN".into(),
            doc: None,
            target: TypeRef::Primitive(Primitive::String),
            annotations: vec![Annotation::str("pattern", r"\d+")],
        });
        let rec = Decl::Record(Record {
            name: "P".into(),
            doc: None,
            annotations: vec![],
            members: vec![Member::Field(req("ssn", TypeRef::named(None, "SSN")))],
        });
        let s = schema(vec![alias, rec]);
        let (file, warnings) = lower(&s, &[s.clone()]).unwrap();
        assert_eq!(file.messages[0].fields[0].type_name, "string");
        assert!(
            warnings.iter().any(|w| w.0.contains("pattern")),
            "warnings: {warnings:?}"
        );
    }

    #[test]
    fn temporal_primitives_map_to_string_with_warning() {
        let s = schema(vec![Decl::Record(Record {
            name: "R".into(),
            doc: None,
            annotations: vec![],
            members: vec![Member::Field(req(
                "when",
                TypeRef::Primitive(Primitive::DateTime),
            ))],
        })]);
        let (file, warnings) = lower(&s, &[s.clone()]).unwrap();
        assert_eq!(file.messages[0].fields[0].type_name, "string");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn choice_lowers_to_oneof_and_enum_gets_unknown_zero() {
        let s = schema(vec![
            Decl::Record(Record {
                name: "R".into(),
                doc: None,
                annotations: vec![],
                members: vec![Member::Choice(Choice {
                    name: "contact".into(),
                    doc: None,
                    fields: vec![req("email", TypeRef::Primitive(Primitive::String))],
                })],
            }),
            Decl::Enum(EnumDecl {
                name: "Status".into(),
                doc: None,
                annotations: vec![],
                values: vec![EnumValue {
                    name: "ACTIVE".into(),
                    doc: None,
                    annotations: vec![],
                }],
            }),
        ]);
        let (file, _) = lower(&s, &[s.clone()]).unwrap();
        assert_eq!(file.messages[0].oneofs[0].name, "contact");
        assert_eq!(file.messages[0].oneofs[0].fields[0].number, 1);
        assert_eq!(file.enums[0].values[0].number, 0);
        assert!(file.enums[0].values[0].name.ends_with("UNKNOWN"));
        assert_eq!(file.enums[0].values[1].name, "STATUS_ACTIVE");
    }

    #[test]
    fn cross_schema_ref_becomes_qualified_name_and_import() {
        let mut s = schema(vec![Decl::Record(Record {
            name: "R".into(),
            doc: None,
            annotations: vec![],
            members: vec![Member::Field(req(
                "w",
                TypeRef::named(Some("pkg.b"), "Widget"),
            ))],
        })]);
        s.imports = vec!["pkg.b".into()];
        let other = Schema {
            name: "pkg.b".into(),
            annotations: vec![],
            imports: vec![],
            decls: vec![Decl::Record(Record {
                name: "Widget".into(),
                doc: None,
                annotations: vec![],
                members: vec![],
            })],
            source_path: None,
        };
        let (file, _) = lower(&s, &[s.clone(), other]).unwrap();
        assert_eq!(file.messages[0].fields[0].type_name, "pkg.b.Widget");
        assert_eq!(file.imports, vec!["pkg/b.proto".to_string()]);
    }

    #[test]
    fn alias_cycle_does_not_overflow() {
        let a = Decl::Alias(Alias {
            name: "A".into(),
            doc: None,
            target: TypeRef::named(None, "B"),
            annotations: vec![],
        });
        let b = Decl::Alias(Alias {
            name: "B".into(),
            doc: None,
            target: TypeRef::named(None, "A"),
            annotations: vec![],
        });
        let rec = Decl::Record(Record {
            name: "R".into(),
            doc: None,
            annotations: vec![],
            members: vec![Member::Field(req("x", TypeRef::named(None, "A")))],
        });
        let s = schema(vec![a, b, rec]);
        let (file, warnings) = lower(&s, &[s.clone()]).unwrap();
        assert_eq!(file.messages[0].fields[0].type_name, "string");
        assert!(
            warnings.iter().any(|w| w.0.contains("cycle")),
            "warnings: {warnings:?}"
        );
    }

    #[test]
    fn cross_schema_alias_target_resolves_in_defining_schema() {
        // pkg.b: `type W = LocalThing` where LocalThing is a record in pkg.b.
        let other = Schema {
            name: "pkg.b".into(),
            annotations: vec![],
            imports: vec![],
            decls: vec![
                Decl::Alias(Alias {
                    name: "W".into(),
                    doc: None,
                    target: TypeRef::named(None, "LocalThing"),
                    annotations: vec![],
                }),
                Decl::Record(Record {
                    name: "LocalThing".into(),
                    doc: None,
                    annotations: vec![],
                    members: vec![],
                }),
            ],
            source_path: None,
        };
        let mut s = schema(vec![Decl::Record(Record {
            name: "R".into(),
            doc: None,
            annotations: vec![],
            members: vec![Member::Field(req("w", TypeRef::named(Some("pkg.b"), "W")))],
        })]);
        s.imports = vec!["pkg.b".into()];
        let (file, _) = lower(&s, &[s.clone(), other]).unwrap();
        assert_eq!(file.messages[0].fields[0].type_name, "pkg.b.LocalThing");
    }

    #[test]
    fn unusable_pin_warns_and_auto_assigns() {
        let mut f1 = req("a", TypeRef::Primitive(Primitive::String));
        f1.annotations.push(Annotation::new(
            "proto.field",
            vec![AnnotationValue::Int(0)],
        ));
        let mut f2 = req("b", TypeRef::Primitive(Primitive::String));
        f2.annotations
            .push(Annotation::str("proto.field", "not-a-number"));
        let s = schema(vec![Decl::Record(Record {
            name: "R".into(),
            doc: None,
            annotations: vec![],
            members: vec![Member::Field(f1), Member::Field(f2)],
        })]);
        let (file, warnings) = lower(&s, &[s.clone()]).unwrap();
        assert_eq!(file.messages[0].fields[0].number, 1);
        assert_eq!(file.messages[0].fields[1].number, 2);
        assert_eq!(
            warnings
                .iter()
                .filter(|w| w.0.contains("proto.field"))
                .count(),
            2,
            "warnings: {warnings:?}"
        );
    }
}
