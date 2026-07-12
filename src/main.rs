use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use schemata::cli::{Cli, Command};
use schemata::convert::SchemaReader;
use schemata::ir::validate::validate;
use schemata::readers::xsd::transform::naming::package_to_import_path;
use schemata::readers::xsd::transform::profile::SchemaProfile;
use schemata::readers::xsd::XsdReader;
use schemata::writers::proto::emitter::emit_proto_file;
use schemata::writers::proto::lower::lower;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Convert {
            input,
            output,
            profile,
        } => {
            tracing::info!(?input, ?output, ?profile, "starting conversion");
            run_pipeline(&input, &output, profile)
        }
    }
}

/// Run the full XSD-to-proto conversion pipeline: read XSD into IR,
/// validate the IR, then lower and emit one proto file per schema.
fn run_pipeline(input: &Path, output: &Path, profile: SchemaProfile) -> Result<()> {
    // Stage 1: Read XSD files into IR schemas.
    tracing::info!("stage 1: reading XSD into IR");
    let reader = XsdReader { profile };
    let (schemas, warnings) = reader.read(input)?;
    for warning in &warnings {
        tracing::warn!("{warning}");
    }
    tracing::info!(count = schemas.len(), "IR schemas produced");

    if schemas.is_empty() {
        tracing::warn!("no schemas produced from {}", input.display());
        return Ok(());
    }

    // Stage 2: Validate the IR.
    tracing::info!("stage 2: validating IR");
    let errors = validate(&schemas);
    if !errors.is_empty() {
        for error in &errors {
            tracing::error!("{error}");
        }
        anyhow::bail!("IR validation failed with {} error(s)", errors.len());
    }

    // Stage 3: Lower each schema to proto and emit it.
    tracing::info!("stage 3: writing proto files to {}", output.display());
    fs::create_dir_all(output)
        .with_context(|| format!("failed to create output directory {}", output.display()))?;

    let mut written = 0usize;
    for schema in &schemas {
        let (proto_file, warnings) = lower(schema, &schemas)?;
        for warning in &warnings {
            tracing::warn!("{warning}");
        }

        // Edge case: skip empty proto files (no messages and no enums).
        if proto_file.messages.is_empty() && proto_file.enums.is_empty() {
            tracing::debug!(
                schema = %schema.name,
                "proto file has no messages or enums; skipping"
            );
            continue;
        }

        let rel_path = package_to_import_path(&proto_file.package);
        let out_path = output.join(&rel_path);

        // Create subdirectories as needed.
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let content = emit_proto_file(&proto_file);
        fs::write(&out_path, &content)
            .with_context(|| format!("failed to write {}", out_path.display()))?;

        tracing::info!(path = %out_path.display(), "wrote proto file");
        written += 1;
    }

    tracing::info!(total = written, "conversion complete");
    Ok(())
}
