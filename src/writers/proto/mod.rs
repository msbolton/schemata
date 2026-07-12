pub mod emitter;
pub mod lower;
pub mod model;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::convert::{SchemaWriter, Warning};
use crate::ir::model::Schema;
use crate::readers::xsd::transform::naming::package_to_import_path;

/// Writes IR schemas out as `.proto` files, one per schema, laid out by
/// proto package path under the output directory.
pub struct ProtoWriter;

impl SchemaWriter for ProtoWriter {
    fn write(&self, schemas: &[Schema], out_dir: &Path) -> Result<Vec<Warning>> {
        let mut warnings = Vec::new();
        let mut written = 0usize;
        for schema in schemas {
            let (file, mut w) = lower::lower(schema, schemas)?;
            warnings.append(&mut w);

            // Edge case: skip empty proto files (no messages and no enums).
            if file.messages.is_empty() && file.enums.is_empty() {
                tracing::debug!(
                    schema = %schema.name,
                    "proto file has no messages or enums; skipping"
                );
                continue;
            }

            let out_path = out_dir.join(package_to_import_path(&file.package));
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create directory {}", parent.display()))?;
            }
            fs::write(&out_path, emitter::emit_proto_file(&file))
                .with_context(|| format!("failed to write {}", out_path.display()))?;

            tracing::info!(path = %out_path.display(), "wrote proto file");
            written += 1;
        }
        tracing::info!(total = written, "proto files written");
        Ok(warnings)
    }
}
