// src/ir/validate.rs
//! Referential validation over a set of IR schemas.

use std::collections::{HashMap, HashSet};

use crate::ir::model::{Decl, Member, Schema, TypeRef};

/// Validate a set of schemas. Returns human-readable error strings; empty = valid.
pub fn validate(schemas: &[Schema]) -> Vec<String> {
    let mut errors = Vec::new();

    // Index: schema name -> set of decl names.
    let mut index: HashMap<&str, HashSet<&str>> = HashMap::new();
    for s in schemas {
        let names = index.entry(s.name.as_str()).or_default();
        for d in &s.decls {
            if !names.insert(d.name()) {
                errors.push(format!("{}: duplicate declaration `{}`", s.name, d.name()));
            }
        }
    }

    for s in schemas {
        for d in &s.decls {
            match d {
                Decl::Record(r) => {
                    let mut field_names: HashSet<&str> = HashSet::new();
                    for m in &r.members {
                        let fields: Vec<_> = match m {
                            Member::Field(f) => vec![f],
                            Member::Choice(c) => c.fields.iter().collect(),
                        };
                        for f in fields {
                            if !field_names.insert(f.name.as_str()) {
                                errors.push(format!(
                                    "{}.{}: duplicate field `{}`",
                                    s.name, r.name, f.name
                                ));
                            }
                            check_ref(
                                &f.ty,
                                s,
                                &index,
                                &format!("{}.{}.{}", s.name, r.name, f.name),
                                &mut errors,
                            );
                        }
                    }
                }
                Decl::Alias(a) => {
                    check_ref(
                        &a.target,
                        s,
                        &index,
                        &format!("{}.{}", s.name, a.name),
                        &mut errors,
                    );
                }
                Decl::Enum(_) => {}
            }
        }
    }
    errors
}

fn check_ref(
    ty: &TypeRef,
    current: &Schema,
    index: &HashMap<&str, HashSet<&str>>,
    context: &str,
    errors: &mut Vec<String>,
) {
    if let TypeRef::Named { schema, name } = ty {
        // Synthetic non-identifier names (e.g. the XSD reader's
        // `map<string, string>` shim) are not declarations; skip them.
        if name.contains('<') {
            return;
        }
        let target_schema = schema.as_deref().unwrap_or(current.name.as_str());
        // Qualified refs into schemas outside the provided set are treated as
        // external (e.g. google.protobuf well-known types) and not validated.
        let Some(names) = index.get(target_schema) else {
            if schema.is_none() {
                errors.push(format!("{context}: unresolved type reference `{ty}`"));
            }
            return;
        };
        if !names.contains(name.as_str()) {
            errors.push(format!("{context}: unresolved type reference `{ty}`"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::model::*;

    fn schema_with(decls: Vec<Decl>) -> Schema {
        Schema {
            name: "test".into(),
            annotations: vec![],
            imports: vec![],
            decls,
            source_path: None,
        }
    }

    fn record(name: &str, fields: Vec<Field>) -> Decl {
        Decl::Record(Record {
            name: name.into(),
            doc: None,
            annotations: vec![],
            members: fields.into_iter().map(Member::Field).collect(),
        })
    }

    fn field(name: &str, ty: TypeRef) -> Field {
        Field {
            name: name.into(),
            ty,
            cardinality: Cardinality::Required,
            doc: None,
            annotations: vec![],
        }
    }

    #[test]
    fn valid_schema_passes() {
        let s = schema_with(vec![record(
            "Person",
            vec![field("name", TypeRef::Primitive(Primitive::String))],
        )]);
        assert!(validate(&[s]).is_empty());
    }

    #[test]
    fn unresolved_local_ref_is_reported() {
        let s = schema_with(vec![record(
            "Person",
            vec![field("home", TypeRef::named(None, "Address"))],
        )]);
        let errs = validate(&[s]);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("Address"), "got: {}", errs[0]);
    }

    #[test]
    fn cross_schema_ref_resolves() {
        let mut common = schema_with(vec![record("Address", vec![])]);
        common.name = "common".into();
        let s = schema_with(vec![record(
            "Person",
            vec![field("home", TypeRef::named(Some("common"), "Address"))],
        )]);
        assert!(validate(&[common, s]).is_empty());
    }

    #[test]
    fn ref_to_external_schema_is_not_validated() {
        // Well-known external types (e.g. google.protobuf.Timestamp) live in
        // schemas outside the provided set and must not be flagged.
        let s = schema_with(vec![record(
            "Event",
            vec![field(
                "when",
                TypeRef::named(Some("google.protobuf"), "Timestamp"),
            )],
        )]);
        assert!(validate(&[s]).is_empty());
    }

    #[test]
    fn qualified_ref_to_known_schema_missing_decl_is_reported() {
        let mut common = schema_with(vec![record("Address", vec![])]);
        common.name = "common".into();
        let s = schema_with(vec![record(
            "Person",
            vec![field("home", TypeRef::named(Some("common"), "Missing"))],
        )]);
        let errs = validate(&[common, s]);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("Missing"), "got: {}", errs[0]);
    }

    #[test]
    fn duplicate_decl_name_is_reported() {
        let s = schema_with(vec![record("Person", vec![]), record("Person", vec![])]);
        let errs = validate(&[s]);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("duplicate"), "got: {}", errs[0]);
    }

    #[test]
    fn duplicate_field_name_is_reported() {
        let s = schema_with(vec![record(
            "Person",
            vec![
                field("name", TypeRef::Primitive(Primitive::String)),
                field("name", TypeRef::Primitive(Primitive::String)),
            ],
        )]);
        let errs = validate(&[s]);
        assert_eq!(errs.len(), 1);
    }
}
