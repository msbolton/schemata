// src/writers/schemata_writer.rs
//! Writes IR schemas out as `.schemata` DSL files.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::convert::{SchemaWriter, Warning};
use crate::ir::model::Schema;
use crate::ir::syntax::emitter::emit;

pub struct SchemataWriter;

impl SchemaWriter for SchemataWriter {
    fn write(&self, schemas: &[Schema], out_dir: &Path) -> Result<Vec<Warning>> {
        fs::create_dir_all(out_dir)?;
        for schema in schemas {
            let path = out_dir.join(format!("{}.schemata", schema.name));
            fs::write(&path, emit(schema))
                .with_context(|| format!("failed to write {}", path.display()))?;
            tracing::info!(path = %path.display(), "wrote schemata file");
        }
        Ok(Vec::new())
    }
}
