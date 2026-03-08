use std::path::PathBuf;

/// Represents an entire `.proto` file.
#[derive(Debug, Clone)]
pub struct ProtoFile {
    /// Always `"proto3"`.
    pub syntax: String,
    /// Package name, e.g. `"uc2.battlefield_entity.v4"`.
    pub package: String,
    /// Import paths for other proto files.
    pub imports: Vec<String>,
    /// File-level options.
    pub options: Vec<ProtoOption>,
    /// Top-level message definitions.
    pub messages: Vec<ProtoMessage>,
    /// Top-level enum definitions.
    pub enums: Vec<ProtoEnum>,
    /// Path to the source XSD file, used for comment attribution.
    pub source_xsd_path: Option<PathBuf>,
}

/// A file-level or message-level option (e.g. `option java_package = "...";`).
#[derive(Debug, Clone)]
pub struct ProtoOption {
    pub name: String,
    pub value: String,
}

/// A protobuf message definition.
#[derive(Debug, Clone)]
pub struct ProtoMessage {
    /// PascalCase message name.
    pub name: String,
    /// Regular fields in the message.
    pub fields: Vec<ProtoField>,
    /// Oneof groups.
    pub oneofs: Vec<ProtoOneof>,
    /// Nested message definitions.
    pub nested_messages: Vec<ProtoMessage>,
    /// Nested enum definitions.
    pub nested_enums: Vec<ProtoEnum>,
    /// Documentation comment for the message.
    pub documentation: Option<String>,
}

/// A single field within a message or oneof.
#[derive(Debug, Clone)]
pub struct ProtoField {
    /// snake_case field name.
    pub name: String,
    /// 1-indexed field number.
    pub number: u32,
    /// Proto type string, e.g. `"string"`, `"int64"`,
    /// or a fully-qualified message type like `"uc2.core_types.v4.WGS84LocationType"`.
    pub type_name: String,
    /// Whether the field is singular, repeated, or explicitly optional.
    pub cardinality: FieldCardinality,
    /// Documentation comment for the field.
    pub documentation: Option<String>,
}

/// Cardinality of a proto field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldCardinality {
    /// Normal proto3 field (implicit zero-value default).
    Singular,
    /// `repeated` field.
    Repeated,
    /// `optional` field (explicit presence tracking).
    Optional,
}

/// A `oneof` group inside a message.
#[derive(Debug, Clone)]
pub struct ProtoOneof {
    /// Name of the oneof group.
    pub name: String,
    /// The alternative fields within the oneof.
    pub fields: Vec<ProtoField>,
}

/// A protobuf enum definition.
#[derive(Debug, Clone)]
pub struct ProtoEnum {
    /// PascalCase enum name.
    pub name: String,
    /// Enum values (the first must be the unknown/default with number 0).
    pub values: Vec<ProtoEnumValue>,
    /// Documentation comment for the enum.
    pub documentation: Option<String>,
}

/// A single value within a protobuf enum.
#[derive(Debug, Clone)]
pub struct ProtoEnumValue {
    /// UPPER_SNAKE_CASE value name.
    pub name: String,
    /// 0-indexed number (0 must be the unknown/default value).
    pub number: i32,
    /// Documentation comment for this value.
    pub documentation: Option<String>,
}
