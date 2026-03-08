use super::model::*;
use std::fmt::Write;

/// Proto3 reserved words. If a field name matches one of these, append `_`.
const RESERVED_WORDS: &[&str] = &[
    "syntax",
    "import",
    "package",
    "option",
    "message",
    "enum",
    "service",
    "rpc",
    "returns",
    "stream",
    "repeated",
    "optional",
    "oneof",
    "map",
    "reserved",
    "extensions",
    "to",
    "max",
    "true",
    "false",
];

/// Emit a complete `.proto` file as a string from the given `ProtoFile` IR.
pub fn emit_proto_file(file: &ProtoFile) -> String {
    let mut out = String::new();

    // Source comment
    if let Some(ref path) = file.source_xsd_path {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        writeln!(out, "// Generated from {}", file_name).unwrap();
    }

    // Syntax
    writeln!(out, "syntax = \"{}\";", file.syntax).unwrap();

    // Package
    if !file.package.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "package {};", file.package).unwrap();
    }

    // Imports (sorted, deduplicated)
    let imports = sorted_deduped_imports(&file.imports);
    if !imports.is_empty() {
        writeln!(out).unwrap();
        for imp in &imports {
            writeln!(out, "import \"{}\";", imp).unwrap();
        }
    }

    // File-level options
    if !file.options.is_empty() {
        writeln!(out).unwrap();
        for opt in &file.options {
            writeln!(out, "option {} = \"{}\";", opt.name, opt.value).unwrap();
        }
    }

    // Top-level enums
    for proto_enum in &file.enums {
        writeln!(out).unwrap();
        emit_enum(&mut out, proto_enum, 0);
    }

    // Top-level messages
    for msg in &file.messages {
        writeln!(out).unwrap();
        emit_message(&mut out, msg, 0);
    }

    out
}

/// Return sorted and deduplicated imports.
fn sorted_deduped_imports(imports: &[String]) -> Vec<String> {
    let mut sorted: Vec<String> = imports.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
}

/// Escape a field name if it collides with a proto3 reserved word.
fn escape_field_name(name: &str) -> String {
    if RESERVED_WORDS.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// Write `count * 2` spaces of indentation.
fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

/// Emit a doc comment line at the given indentation level.
fn emit_doc_comment(out: &mut String, doc: &Option<String>, level: usize) {
    if let Some(ref text) = doc {
        for line in text.lines() {
            indent(out, level);
            writeln!(out, "// {}", line).unwrap();
        }
    }
}

/// Emit an enum definition.
fn emit_enum(out: &mut String, proto_enum: &ProtoEnum, level: usize) {
    emit_doc_comment(out, &proto_enum.documentation, level);
    indent(out, level);
    writeln!(out, "enum {} {{", proto_enum.name).unwrap();
    for value in &proto_enum.values {
        emit_doc_comment(out, &value.documentation, level + 1);
        indent(out, level + 1);
        writeln!(out, "{} = {};", value.name, value.number).unwrap();
    }
    indent(out, level);
    writeln!(out, "}}").unwrap();
}

/// Emit a message definition.
fn emit_message(out: &mut String, msg: &ProtoMessage, level: usize) {
    emit_doc_comment(out, &msg.documentation, level);
    indent(out, level);

    // Check for empty message (no fields, oneofs, nested messages, nested enums)
    let is_empty = msg.fields.is_empty()
        && msg.oneofs.is_empty()
        && msg.nested_messages.is_empty()
        && msg.nested_enums.is_empty();

    if is_empty {
        writeln!(out, "message {} {{}}", msg.name).unwrap();
        return;
    }

    writeln!(out, "message {} {{", msg.name).unwrap();

    // Nested enums
    for nested_enum in &msg.nested_enums {
        emit_enum(out, nested_enum, level + 1);
    }

    // Nested messages
    for nested_msg in &msg.nested_messages {
        emit_message(out, nested_msg, level + 1);
    }

    // Regular fields
    for field in &msg.fields {
        emit_field(out, field, level + 1);
    }

    // Oneofs
    for oneof in &msg.oneofs {
        emit_oneof(out, oneof, level + 1);
    }

    indent(out, level);
    writeln!(out, "}}").unwrap();
}

/// Emit a single field.
fn emit_field(out: &mut String, field: &ProtoField, level: usize) {
    emit_doc_comment(out, &field.documentation, level);
    indent(out, level);

    let name = escape_field_name(&field.name);

    // Check if this is a map type (type_name starts with "map<")
    if field.type_name.starts_with("map<") {
        writeln!(out, "{} {} = {};", field.type_name, name, field.number).unwrap();
    } else {
        let prefix = match field.cardinality {
            FieldCardinality::Repeated => "repeated ",
            FieldCardinality::Optional => "optional ",
            FieldCardinality::Singular => "",
        };
        writeln!(
            out,
            "{}{} {} = {};",
            prefix, field.type_name, name, field.number
        )
        .unwrap();
    }
}

/// Emit a oneof group.
fn emit_oneof(out: &mut String, oneof: &ProtoOneof, level: usize) {
    indent(out, level);
    writeln!(out, "oneof {} {{", oneof.name).unwrap();
    for field in &oneof.fields {
        // Oneof fields don't have cardinality prefixes
        emit_doc_comment(out, &field.documentation, level + 1);
        indent(out, level + 1);
        let name = escape_field_name(&field.name);
        writeln!(out, "{} {} = {};", field.type_name, name, field.number).unwrap();
    }
    indent(out, level);
    writeln!(out, "}}").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Helper to create a minimal ProtoFile.
    fn minimal_proto_file() -> ProtoFile {
        ProtoFile {
            syntax: "proto3".to_string(),
            package: "test.package.v1".to_string(),
            imports: vec![],
            options: vec![],
            messages: vec![],
            enums: vec![],
            source_xsd_path: None,
        }
    }

    #[test]
    fn test_simple_message_and_enum() {
        let file = ProtoFile {
            syntax: "proto3".to_string(),
            package: "uc2.uc2_types.v4".to_string(),
            imports: vec!["niem/structures/v5.proto".to_string()],
            options: vec![],
            enums: vec![ProtoEnum {
                name: "ConfidenceCode".to_string(),
                values: vec![
                    ProtoEnumValue {
                        name: "CONFIDENCE_CODE_UNKNOWN".to_string(),
                        number: 0,
                        documentation: None,
                    },
                    ProtoEnumValue {
                        name: "CONFIDENCE_CODE_HIGH".to_string(),
                        number: 1,
                        documentation: None,
                    },
                ],
                documentation: Some(
                    "A data type for an enumeration of Confidence types.".to_string(),
                ),
            }],
            messages: vec![ProtoMessage {
                name: "WGS84LocationType".to_string(),
                fields: vec![
                    ProtoField {
                        name: "latitude".to_string(),
                        number: 1,
                        type_name: "double".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: Some("A latitude in degrees.".to_string()),
                    },
                    ProtoField {
                        name: "longitude".to_string(),
                        number: 2,
                        type_name: "double".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: Some("A longitude in degrees.".to_string()),
                    },
                ],
                oneofs: vec![],
                nested_messages: vec![],
                nested_enums: vec![],
                documentation: Some(
                    "A data type for a position in WGS 84 coordinates.".to_string(),
                ),
            }],
            source_xsd_path: Some(PathBuf::from("uc2-core-types.xsd")),
        };

        let output = emit_proto_file(&file);

        assert!(output.contains("// Generated from uc2-core-types.xsd"));
        assert!(output.contains("syntax = \"proto3\";"));
        assert!(output.contains("package uc2.uc2_types.v4;"));
        assert!(output.contains("import \"niem/structures/v5.proto\";"));
        assert!(output.contains("// A data type for an enumeration of Confidence types."));
        assert!(output.contains("enum ConfidenceCode {"));
        assert!(output.contains("  CONFIDENCE_CODE_UNKNOWN = 0;"));
        assert!(output.contains("  CONFIDENCE_CODE_HIGH = 1;"));
        assert!(output.contains("// A data type for a position in WGS 84 coordinates."));
        assert!(output.contains("message WGS84LocationType {"));
        assert!(output.contains("  // A latitude in degrees."));
        assert!(output.contains("  double latitude = 1;"));
        assert!(output.contains("  // A longitude in degrees."));
        assert!(output.contains("  double longitude = 2;"));
    }

    #[test]
    fn test_reserved_word_escaping() {
        let file = ProtoFile {
            messages: vec![ProtoMessage {
                name: "TestMessage".to_string(),
                fields: vec![
                    ProtoField {
                        name: "message".to_string(),
                        number: 1,
                        type_name: "string".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: None,
                    },
                    ProtoField {
                        name: "import".to_string(),
                        number: 2,
                        type_name: "string".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: None,
                    },
                    ProtoField {
                        name: "true".to_string(),
                        number: 3,
                        type_name: "bool".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: None,
                    },
                    ProtoField {
                        name: "normal_name".to_string(),
                        number: 4,
                        type_name: "string".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: None,
                    },
                ],
                oneofs: vec![],
                nested_messages: vec![],
                nested_enums: vec![],
                documentation: None,
            }],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);

        assert!(output.contains("string message_ = 1;"));
        assert!(output.contains("string import_ = 2;"));
        assert!(output.contains("bool true_ = 3;"));
        assert!(output.contains("string normal_name = 4;"));
    }

    #[test]
    fn test_nested_messages() {
        let file = ProtoFile {
            messages: vec![ProtoMessage {
                name: "OuterMessage".to_string(),
                fields: vec![ProtoField {
                    name: "inner".to_string(),
                    number: 1,
                    type_name: "InnerMessage".to_string(),
                    cardinality: FieldCardinality::Singular,
                    documentation: None,
                }],
                oneofs: vec![],
                nested_messages: vec![ProtoMessage {
                    name: "InnerMessage".to_string(),
                    fields: vec![ProtoField {
                        name: "value".to_string(),
                        number: 1,
                        type_name: "string".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: None,
                    }],
                    oneofs: vec![],
                    nested_messages: vec![],
                    nested_enums: vec![],
                    documentation: Some("An inner message.".to_string()),
                }],
                nested_enums: vec![],
                documentation: Some("An outer message.".to_string()),
            }],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);

        // Verify nesting with 2-space indentation
        assert!(output.contains("message OuterMessage {"));
        assert!(output.contains("  // An inner message."));
        assert!(output.contains("  message InnerMessage {"));
        assert!(output.contains("    string value = 1;"));
        // The inner message closing brace should be at 2-space indent
        // and the outer at 0-space indent
        let lines: Vec<&str> = output.lines().collect();
        let inner_close = lines
            .iter()
            .position(|l| *l == "  }")
            .expect("inner closing brace");
        let outer_close = lines
            .iter()
            .rposition(|l| *l == "}")
            .expect("outer closing brace");
        assert!(inner_close < outer_close);
    }

    #[test]
    fn test_oneof_emission() {
        let file = ProtoFile {
            messages: vec![ProtoMessage {
                name: "Shape".to_string(),
                fields: vec![],
                oneofs: vec![ProtoOneof {
                    name: "shape_kind".to_string(),
                    fields: vec![
                        ProtoField {
                            name: "circle".to_string(),
                            number: 1,
                            type_name: "Circle".to_string(),
                            cardinality: FieldCardinality::Singular,
                            documentation: Some("A circle shape.".to_string()),
                        },
                        ProtoField {
                            name: "rectangle".to_string(),
                            number: 2,
                            type_name: "Rectangle".to_string(),
                            cardinality: FieldCardinality::Singular,
                            documentation: None,
                        },
                    ],
                }],
                nested_messages: vec![],
                nested_enums: vec![],
                documentation: None,
            }],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);

        assert!(output.contains("  oneof shape_kind {"));
        assert!(output.contains("    // A circle shape."));
        assert!(output.contains("    Circle circle = 1;"));
        assert!(output.contains("    Rectangle rectangle = 2;"));
        assert!(output.contains("  }"));
    }

    #[test]
    fn test_import_deduplication_and_sorting() {
        let file = ProtoFile {
            imports: vec![
                "niem/niem_core/v5.proto".to_string(),
                "niem/structures/v5.proto".to_string(),
                "niem/niem_core/v5.proto".to_string(), // duplicate
                "common/types.proto".to_string(),
            ],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);

        // Find import lines
        let import_lines: Vec<&str> = output
            .lines()
            .filter(|l| l.starts_with("import"))
            .collect();

        assert_eq!(import_lines.len(), 3, "duplicates should be removed");
        assert_eq!(import_lines[0], "import \"common/types.proto\";");
        assert_eq!(import_lines[1], "import \"niem/niem_core/v5.proto\";");
        assert_eq!(import_lines[2], "import \"niem/structures/v5.proto\";");
    }

    #[test]
    fn test_doc_comments() {
        let file = ProtoFile {
            enums: vec![ProtoEnum {
                name: "Status".to_string(),
                values: vec![
                    ProtoEnumValue {
                        name: "STATUS_UNKNOWN".to_string(),
                        number: 0,
                        documentation: Some("Unknown status.".to_string()),
                    },
                    ProtoEnumValue {
                        name: "STATUS_ACTIVE".to_string(),
                        number: 1,
                        documentation: Some("Active status.".to_string()),
                    },
                ],
                documentation: Some("A status enumeration.".to_string()),
            }],
            messages: vec![ProtoMessage {
                name: "Item".to_string(),
                fields: vec![ProtoField {
                    name: "name".to_string(),
                    number: 1,
                    type_name: "string".to_string(),
                    cardinality: FieldCardinality::Singular,
                    documentation: Some("The item name.".to_string()),
                }],
                oneofs: vec![],
                nested_messages: vec![],
                nested_enums: vec![],
                documentation: Some("An item message.".to_string()),
            }],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);

        assert!(output.contains("// A status enumeration.\nenum Status {"));
        assert!(output.contains("  // Unknown status.\n  STATUS_UNKNOWN = 0;"));
        assert!(output.contains("  // Active status.\n  STATUS_ACTIVE = 1;"));
        assert!(output.contains("// An item message.\nmessage Item {"));
        assert!(output.contains("  // The item name.\n  string name = 1;"));
    }

    #[test]
    fn test_empty_message() {
        let file = ProtoFile {
            messages: vec![ProtoMessage {
                name: "EmptyMessage".to_string(),
                fields: vec![],
                oneofs: vec![],
                nested_messages: vec![],
                nested_enums: vec![],
                documentation: None,
            }],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);
        assert!(output.contains("message EmptyMessage {}"));
    }

    #[test]
    fn test_repeated_and_optional_fields() {
        let file = ProtoFile {
            messages: vec![ProtoMessage {
                name: "Container".to_string(),
                fields: vec![
                    ProtoField {
                        name: "items".to_string(),
                        number: 1,
                        type_name: "string".to_string(),
                        cardinality: FieldCardinality::Repeated,
                        documentation: None,
                    },
                    ProtoField {
                        name: "description".to_string(),
                        number: 2,
                        type_name: "string".to_string(),
                        cardinality: FieldCardinality::Optional,
                        documentation: None,
                    },
                    ProtoField {
                        name: "id".to_string(),
                        number: 3,
                        type_name: "int64".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: None,
                    },
                ],
                oneofs: vec![],
                nested_messages: vec![],
                nested_enums: vec![],
                documentation: None,
            }],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);

        assert!(output.contains("  repeated string items = 1;"));
        assert!(output.contains("  optional string description = 2;"));
        assert!(output.contains("  int64 id = 3;"));
    }

    #[test]
    fn test_map_field() {
        let file = ProtoFile {
            messages: vec![ProtoMessage {
                name: "Dictionary".to_string(),
                fields: vec![ProtoField {
                    name: "entries".to_string(),
                    number: 1,
                    type_name: "map<string, int32>".to_string(),
                    cardinality: FieldCardinality::Singular,
                    documentation: None,
                }],
                oneofs: vec![],
                nested_messages: vec![],
                nested_enums: vec![],
                documentation: None,
            }],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);
        assert!(output.contains("  map<string, int32> entries = 1;"));
    }

    #[test]
    fn test_file_level_options() {
        let file = ProtoFile {
            options: vec![
                ProtoOption {
                    name: "java_package".to_string(),
                    value: "com.example.test".to_string(),
                },
                ProtoOption {
                    name: "go_package".to_string(),
                    value: "example.com/test".to_string(),
                },
            ],
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);

        assert!(output.contains("option java_package = \"com.example.test\";"));
        assert!(output.contains("option go_package = \"example.com/test\";"));
    }

    #[test]
    fn test_no_source_path() {
        let file = ProtoFile {
            source_xsd_path: None,
            ..minimal_proto_file()
        };

        let output = emit_proto_file(&file);
        assert!(!output.contains("// Generated from"));
        assert!(output.starts_with("syntax = \"proto3\";"));
    }

    #[test]
    fn test_full_format_example() {
        // Test the exact format from the task description
        let file = ProtoFile {
            syntax: "proto3".to_string(),
            package: "uc2.uc2_types.v4".to_string(),
            imports: vec![
                "niem/structures/v5.proto".to_string(),
                "niem/niem_core/v5.proto".to_string(),
            ],
            options: vec![],
            enums: vec![ProtoEnum {
                name: "ConfidenceCode".to_string(),
                values: vec![
                    ProtoEnumValue {
                        name: "CONFIDENCE_CODE_UNKNOWN".to_string(),
                        number: 0,
                        documentation: None,
                    },
                    ProtoEnumValue {
                        name: "CONFIDENCE_CODE_HIGH".to_string(),
                        number: 1,
                        documentation: None,
                    },
                    ProtoEnumValue {
                        name: "CONFIDENCE_CODE_MEDIUM".to_string(),
                        number: 2,
                        documentation: None,
                    },
                    ProtoEnumValue {
                        name: "CONFIDENCE_CODE_LOW".to_string(),
                        number: 3,
                        documentation: None,
                    },
                    ProtoEnumValue {
                        name: "CONFIDENCE_CODE_VERY_HIGH".to_string(),
                        number: 4,
                        documentation: None,
                    },
                    ProtoEnumValue {
                        name: "CONFIDENCE_CODE_VERY_LOW".to_string(),
                        number: 5,
                        documentation: None,
                    },
                ],
                documentation: Some(
                    "A data type for an enumeration of Confidence types.".to_string(),
                ),
            }],
            messages: vec![ProtoMessage {
                name: "WGS84LocationType".to_string(),
                fields: vec![
                    ProtoField {
                        name: "latitude".to_string(),
                        number: 1,
                        type_name: "double".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: Some("A latitude in degrees.".to_string()),
                    },
                    ProtoField {
                        name: "longitude".to_string(),
                        number: 2,
                        type_name: "double".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: Some("A longitude in degrees.".to_string()),
                    },
                    ProtoField {
                        name: "height".to_string(),
                        number: 3,
                        type_name: "double".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: Some(
                            "A height in meters above the WGS 84 ellipsoid.".to_string(),
                        ),
                    },
                ],
                oneofs: vec![],
                nested_messages: vec![],
                nested_enums: vec![],
                documentation: Some(
                    "A data type for a position in WGS 84 coordinates.".to_string(),
                ),
            }],
            source_xsd_path: Some(PathBuf::from("uc2-core-types.xsd")),
        };

        let output = emit_proto_file(&file);

        let expected = "\
// Generated from uc2-core-types.xsd
syntax = \"proto3\";

package uc2.uc2_types.v4;

import \"niem/niem_core/v5.proto\";
import \"niem/structures/v5.proto\";

// A data type for an enumeration of Confidence types.
enum ConfidenceCode {
  CONFIDENCE_CODE_UNKNOWN = 0;
  CONFIDENCE_CODE_HIGH = 1;
  CONFIDENCE_CODE_MEDIUM = 2;
  CONFIDENCE_CODE_LOW = 3;
  CONFIDENCE_CODE_VERY_HIGH = 4;
  CONFIDENCE_CODE_VERY_LOW = 5;
}

// A data type for a position in WGS 84 coordinates.
message WGS84LocationType {
  // A latitude in degrees.
  double latitude = 1;
  // A longitude in degrees.
  double longitude = 2;
  // A height in meters above the WGS 84 ellipsoid.
  double height = 3;
}
";

        assert_eq!(output, expected);
    }
}
