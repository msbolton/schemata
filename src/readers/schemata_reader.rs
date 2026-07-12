// src/readers/schemata_reader.rs
//! Reads `.schemata` DSL files into IR schemas.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::convert::{SchemaReader, Warning};
use crate::ir::model::Schema;
use crate::ir::syntax::parser::parse;

pub struct SchemataReader;

impl SchemaReader for SchemataReader {
    fn read(&self, input: &Path) -> Result<(Vec<Schema>, Vec<Warning>)> {
        let mut files = Vec::new();
        if input.is_dir() {
            collect(input, &mut files)?;
        } else {
            files.push(input.to_path_buf());
        }
        files.sort();
        let mut schemas = Vec::new();
        for path in &files {
            let src = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let mut schema = parse(&src).map_err(|e| anyhow::anyhow!("{}:{e}", path.display()))?;
            // An explicit @source annotation (the original source file)
            // wins over the .schemata path we read from.
            if schema.source_path.is_none() {
                schema.source_path = Some(path.clone());
            }
            schemas.push(schema);
        }
        Ok((schemas, Vec::new()))
    }
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect(&p, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("schemata") {
            out.push(p);
        }
    }
    Ok(())
}
