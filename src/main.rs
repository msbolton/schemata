use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use schemata::cli::{Cli, Command};
use schemata::readers::xsd::parser::parse_schema;
use schemata::readers::xsd::resolver::build_type_registry;
use schemata::readers::xsd::transform::naming::package_to_import_path;
use schemata::readers::xsd::transform::profile::SchemaProfile;
use schemata::readers::xsd::transform::transform_schema;
use schemata::writers::proto::emitter::emit_proto_file;
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

/// Run the full XSD-to-proto conversion pipeline.
fn run_pipeline(input: &Path, output: &Path, profile: SchemaProfile) -> Result<()> {
    // Stage 1: Discover XSD files.
    tracing::info!("stage 1: discovering XSD files");
    let xsd_paths = discover_xsd_files(input)?;
    tracing::info!(count = xsd_paths.len(), "discovered XSD files");

    if xsd_paths.is_empty() {
        tracing::warn!("no XSD files found under {}", input.display());
        return Ok(());
    }

    // Stage 2: Parse all XSD files.
    tracing::info!("stage 2: parsing XSD files");
    let mut parsed = Vec::new();
    for path in &xsd_paths {
        let xml = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        match parse_schema(&xml, path) {
            Ok(schema) => {
                if schema.target_namespace.is_none() {
                    tracing::debug!(
                        path = %path.display(),
                        "schema has no target namespace; skipping"
                    );
                }
                parsed.push((schema, path.clone()));
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to parse schema; skipping"
                );
            }
        }
    }
    tracing::info!(count = parsed.len(), "parsed schemas");

    // Stage 3: Build the type registry.
    tracing::info!("stage 3: building type registry");
    let registry = build_type_registry(parsed);
    tracing::info!(
        namespaces = registry.schemas.len(),
        complex_types = registry.complex_types.len(),
        simple_types = registry.simple_types.len(),
        elements = registry.elements.len(),
        "type registry built"
    );

    // Stage 4: Transform each namespace into a ProtoFile.
    tracing::info!("stage 4: transforming schemas to proto");
    let namespaces: Vec<_> = registry.schemas.keys().cloned().collect();
    let mut proto_files = Vec::new();
    for ns in &namespaces {
        if let Some(proto_file) = transform_schema(ns, &registry, profile) {
            // Edge case: skip empty proto files (no messages and no enums).
            if proto_file.messages.is_empty() && proto_file.enums.is_empty() {
                tracing::debug!(
                    namespace = %ns,
                    package = %proto_file.package,
                    "proto file has no messages or enums; skipping"
                );
                continue;
            }
            proto_files.push(proto_file);
        }
    }
    tracing::info!(count = proto_files.len(), "proto files generated");

    // Stage 5: Emit each ProtoFile to the output directory.
    tracing::info!("stage 5: writing proto files to {}", output.display());
    fs::create_dir_all(output)
        .with_context(|| format!("failed to create output directory {}", output.display()))?;

    for proto_file in &proto_files {
        let rel_path = package_to_import_path(&proto_file.package);
        let out_path = output.join(&rel_path);

        // Create subdirectories as needed.
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let content = emit_proto_file(proto_file);
        fs::write(&out_path, &content)
            .with_context(|| format!("failed to write {}", out_path.display()))?;

        tracing::info!(path = %out_path.display(), "wrote proto file");
    }

    tracing::info!(total = proto_files.len(), "conversion complete");
    Ok(())
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
