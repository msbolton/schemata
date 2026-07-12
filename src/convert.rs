// src/convert.rs
//! Reader/writer traits and shared conversion types.

use std::fmt;
use std::path::Path;

use anyhow::Result;

use crate::ir::model::Schema;

/// A non-fatal information-loss or compatibility notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning(pub String);

impl Warning {
    pub fn new(msg: impl Into<String>) -> Self {
        Warning(msg.into())
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A schema format schemata can read and/or write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Xsd,
    Proto,
    Schemata,
}

impl Format {
    /// Infer a format from a file extension. Directories return None.
    pub fn infer(path: &Path) -> Option<Format> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("xsd") => Some(Format::Xsd),
            Some("proto") => Some(Format::Proto),
            Some("schemata") => Some(Format::Schemata),
            _ => None,
        }
    }
}

/// Reads some source format into IR schemas.
pub trait SchemaReader {
    /// `input` is a file or directory (reader-specific).
    fn read(&self, input: &Path) -> Result<(Vec<Schema>, Vec<Warning>)>;
}

/// Writes IR schemas out in some target format, one file per schema,
/// under `out_dir`.
pub trait SchemaWriter {
    fn write(&self, schemas: &[Schema], out_dir: &Path) -> Result<Vec<Warning>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn format_inference_from_extension() {
        assert_eq!(Format::infer(Path::new("a/b.xsd")), Some(Format::Xsd));
        assert_eq!(
            Format::infer(Path::new("a/b.schemata")),
            Some(Format::Schemata)
        );
        assert_eq!(Format::infer(Path::new("a/b.proto")), Some(Format::Proto));
        assert_eq!(Format::infer(Path::new("a/b.txt")), None);
        assert_eq!(Format::infer(Path::new("somedir")), None);
    }

    #[test]
    fn warning_display() {
        let w = Warning::new("proto cannot express @pattern on Person.ssn — dropped");
        assert!(w.to_string().contains("Person.ssn"));
    }
}
