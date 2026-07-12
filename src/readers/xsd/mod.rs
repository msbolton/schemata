pub mod model;
pub mod names;
pub mod parser;
pub mod resolver;
pub mod transform;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::convert::{SchemaReader, Warning};
use crate::ir::model::Schema;
use crate::readers::xsd::transform::profile::SchemaProfile;

/// Reads XSD files (a single file or a directory tree) into IR schemas,
/// one per XML namespace.
pub struct XsdReader {
    pub profile: SchemaProfile,
}

impl SchemaReader for XsdReader {
    fn read(&self, input: &Path) -> Result<(Vec<Schema>, Vec<Warning>)> {
        let mut warnings = Vec::new();

        let paths = discover_xsd_files(input)?;

        let mut parsed = Vec::new();
        for path in &paths {
            let xml = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            match parser::parse_schema(&xml, path) {
                Ok(schema) => parsed.push((schema, path.clone())),
                Err(err) => warnings.push(Warning::new(format!(
                    "failed to parse {} — skipped: {err}",
                    path.display()
                ))),
            }
        }

        let registry = resolver::build_type_registry(parsed);

        // Sort namespaces for deterministic processing order.
        let mut namespaces: Vec<_> = registry.schemas.keys().cloned().collect();
        namespaces.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        let mut schemas = Vec::new();
        for ns in &namespaces {
            if let Some(schema) = transform::transform_schema(ns, &registry, self.profile) {
                // Skip schemas with nothing to declare.
                if !schema.decls.is_empty() {
                    schemas.push(schema);
                }
            }
        }
        Ok((schemas, warnings))
    }
}

/// Discover `.xsd` files from an input path.
///
/// If `input` is a single file, returns a vec containing just that file.
/// If `input` is a directory, recursively walks it for `.xsd` files.
fn discover_xsd_files(input: &Path) -> Result<Vec<PathBuf>> {
    if input.is_file() {
        return Ok(vec![input.to_path_buf()]);
    }

    if !input.is_dir() {
        anyhow::bail!(
            "input path does not exist or is not a file/directory: {}",
            input.display()
        );
    }

    let mut files = Vec::new();
    collect_xsd_files_recursive(input, &mut files);
    files.sort();
    Ok(files)
}

/// Recursively collect all `.xsd` files under a directory.
fn collect_xsd_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(
                dir = %dir.display(),
                error = %err,
                "failed to read directory"
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_xsd_files_recursive(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("xsd") {
            out.push(path);
        }
    }
}
