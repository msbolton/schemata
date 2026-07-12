// src/ir/model.rs
//! The intermediary representation (IR): the hub every schema format
//! converts into and out of.

use std::fmt;
use std::path::PathBuf;

/// Root of the IR: one named schema (≈ one namespace / one output file).
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    /// Dotted name, e.g. "example_com.schemas.vehicle".
    pub name: String,
    pub annotations: Vec<Annotation>,
    /// Dotted names of other IR schemas referenced by qualified type refs.
    /// Readers must deduplicate entries; writers expect no duplicates.
    pub imports: Vec<String>,
    pub decls: Vec<Decl>,
    /// Source file this schema came from (attribution comments only).
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Record(Record),
    Enum(EnumDecl),
    Alias(Alias),
}

impl Decl {
    pub fn name(&self) -> &str {
        match self {
            Decl::Record(r) => &r.name,
            Decl::Enum(e) => &e.name,
            Decl::Alias(a) => &a.name,
        }
    }
}

/// Named product type (XSD complex type, proto message).
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub name: String,
    pub doc: Option<String>,
    pub annotations: Vec<Annotation>,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    Field(Field),
    Choice(Choice),
}

/// Variant group (XSD xs:choice, proto oneof).
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub name: String,
    pub doc: Option<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: TypeRef,
    pub cardinality: Cardinality,
    pub doc: Option<String>,
    pub annotations: Vec<Annotation>,
}

/// DSL spelling: bare = Required, `?` = Optional, `*` = Many, `+` = AtLeastOne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    Required,
    Optional,
    Many,
    AtLeastOne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Primitive(Primitive),
    Named {
        /// Dotted schema name for cross-schema refs; None for same-schema.
        schema: Option<String>,
        /// Unqualified declaration name within the target schema.
        name: String,
    },
}

impl TypeRef {
    pub fn named(schema: Option<&str>, name: &str) -> Self {
        TypeRef::Named {
            schema: schema.map(String::from),
            name: name.into(),
        }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeRef::Primitive(p) => f.write_str(p.name()),
            TypeRef::Named {
                schema: Some(s),
                name,
            } => write!(f, "{s}.{name}"),
            TypeRef::Named { schema: None, name } => f.write_str(name),
        }
    }
}

/// The fixed primitive set. Readers map in; writers map out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    String,
    Bool,
    Int32,
    Int64,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Bytes,
    Decimal,
    Date,
    Time,
    DateTime,
    Duration,
    Any,
}

impl Primitive {
    /// The canonical set of all primitives, in declaration order.
    pub const ALL: [Primitive; 15] = [
        Primitive::String,
        Primitive::Bool,
        Primitive::Int32,
        Primitive::Int64,
        Primitive::UInt32,
        Primitive::UInt64,
        Primitive::Float32,
        Primitive::Float64,
        Primitive::Bytes,
        Primitive::Decimal,
        Primitive::Date,
        Primitive::Time,
        Primitive::DateTime,
        Primitive::Duration,
        Primitive::Any,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Primitive::String => "string",
            Primitive::Bool => "bool",
            Primitive::Int32 => "int32",
            Primitive::Int64 => "int64",
            Primitive::UInt32 => "uint32",
            Primitive::UInt64 => "uint64",
            Primitive::Float32 => "float32",
            Primitive::Float64 => "float64",
            Primitive::Bytes => "bytes",
            Primitive::Decimal => "decimal",
            Primitive::Date => "date",
            Primitive::Time => "time",
            Primitive::DateTime => "datetime",
            Primitive::Duration => "duration",
            Primitive::Any => "any",
        }
    }

    pub fn from_name(s: &str) -> Option<Primitive> {
        Primitive::ALL.iter().copied().find(|p| p.name() == s)
    }
}

/// Named refinement of another type (XSD simpleType restriction).
#[derive(Debug, Clone, PartialEq)]
pub struct Alias {
    pub name: String,
    pub doc: Option<String>,
    pub target: TypeRef,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub doc: Option<String>,
    pub annotations: Vec<Annotation>,
    pub values: Vec<EnumValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    /// DSL-safe identifier. If the source value wasn't a valid identifier,
    /// the original is preserved in an `@xml.value("...")` annotation.
    pub name: String,
    pub doc: Option<String>,
    pub annotations: Vec<Annotation>,
}

/// Open key-value metadata, e.g. `@pattern("...")`, `@proto.field(3)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Dotted name, e.g. "pattern" or "xml.namespace".
    pub name: String,
    pub args: Vec<AnnotationValue>,
}

impl Annotation {
    pub fn new(name: &str, args: Vec<AnnotationValue>) -> Self {
        Annotation {
            name: name.into(),
            args,
        }
    }
    pub fn str(name: &str, value: &str) -> Self {
        Annotation::new(name, vec![AnnotationValue::Str(value.into())])
    }
}

/// A single annotation argument.
///
/// Note: the `Float` variant means this type (and every type containing it)
/// is `PartialEq` but not `Eq`, so these types cannot be used as map keys.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationValue {
    Str(String),
    Int(i64),
    Float(f64),
    Ident(String),
}

impl fmt::Display for AnnotationValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnnotationValue::Str(s) => {
                write!(f, "\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            }
            AnnotationValue::Int(i) => write!(f, "{i}"),
            AnnotationValue::Float(x) => {
                let s = format!("{x}");
                if s.contains('.') || s.contains('e') || s.contains('E') {
                    f.write_str(&s)
                } else {
                    write!(f, "{s}.0")
                }
            }
            AnnotationValue::Ident(s) => f.write_str(s),
        }
    }
}

impl fmt::Display for Annotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.name)?;
        if !self.args.is_empty() {
            let args: Vec<String> = self.args.iter().map(|a| a.to_string()).collect();
            write!(f, "({})", args.join(", "))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_parses_from_name_and_displays() {
        assert_eq!(Primitive::from_name("string"), Some(Primitive::String));
        assert_eq!(Primitive::from_name("datetime"), Some(Primitive::DateTime));
        assert_eq!(Primitive::from_name("Person"), None);
        assert_eq!(Primitive::DateTime.name(), "datetime");
        assert_eq!(Primitive::Float64.name(), "float64");
    }

    #[test]
    fn type_ref_display_forms() {
        let p = TypeRef::Primitive(Primitive::Int64);
        assert_eq!(p.to_string(), "int64");
        let local = TypeRef::named(None, "Person");
        assert_eq!(local.to_string(), "Person");
        let foreign = TypeRef::named(Some("example_com.common"), "WGS84Location");
        assert_eq!(foreign.to_string(), "example_com.common.WGS84Location");
    }

    #[test]
    fn annotation_display() {
        let a = Annotation {
            name: "pattern".into(),
            args: vec![AnnotationValue::Str(r"\d{3}".into())],
        };
        assert_eq!(a.to_string(), r#"@pattern("\\d{3}")"#);
        let b = Annotation {
            name: "deprecated".into(),
            args: vec![],
        };
        assert_eq!(b.to_string(), "@deprecated");
        let c = Annotation {
            name: "occurs".into(),
            args: vec![AnnotationValue::Int(2), AnnotationValue::Int(5)],
        };
        assert_eq!(c.to_string(), "@occurs(2, 5)");
    }

    #[test]
    fn float_annotation_display_is_round_trippable() {
        let whole = Annotation::new("ratio", vec![AnnotationValue::Float(1.0)]);
        assert_eq!(whole.to_string(), "@ratio(1.0)");
        let fractional = Annotation::new("ratio", vec![AnnotationValue::Float(2.5)]);
        assert_eq!(fractional.to_string(), "@ratio(2.5)");
    }
}
