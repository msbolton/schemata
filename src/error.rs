use std::path::PathBuf;

/// Errors that can occur during schema conversion.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse XML in {path}: {message}")]
    XmlParse { path: PathBuf, message: String },

    #[error("unsupported XSD construct in {path}: {message}")]
    UnsupportedXsd { path: PathBuf, message: String },

    #[error("unresolved reference: {qname}")]
    UnresolvedReference { qname: String },

    #[error("failed to write output to {path}: {source}")]
    WriteOutput {
        path: PathBuf,
        source: std::io::Error,
    },
}
