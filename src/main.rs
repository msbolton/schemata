use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use schemata::cli::{Cli, Command};
use schemata::convert::{Format, SchemaReader, SchemaWriter, Warning};
use schemata::ir::validate::validate;
use schemata::readers::schemata_reader::SchemataReader;
use schemata::readers::xsd::transform::profile::SchemaProfile;
use schemata::readers::xsd::XsdReader;
use schemata::writers::proto::ProtoWriter;
use schemata::writers::schemata_writer::SchemataWriter;
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
            from,
            to,
            profile,
            deny_warnings,
        } => {
            tracing::info!(?input, ?output, ?from, ?to, ?profile, "starting conversion");
            run_pipeline(&input, &output, from, to, profile, deny_warnings)
        }
    }
}

/// Run the conversion pipeline: read the input format into IR schemas,
/// validate the IR, then write them out in the target format.
fn run_pipeline(
    input: &Path,
    output: &Path,
    from: Option<Format>,
    to: Format,
    profile: SchemaProfile,
    deny_warnings: bool,
) -> Result<()> {
    let from = from
        .or_else(|| Format::infer(input))
        .or_else(|| infer_dir_format(input))
        .context("cannot infer input format; pass --from")?;

    let reader: Box<dyn SchemaReader> = match from {
        Format::Xsd => Box::new(XsdReader { profile }),
        Format::Schemata => Box::new(SchemataReader),
        Format::Proto => anyhow::bail!("reading .proto is not supported yet"),
    };
    let writer: Box<dyn SchemaWriter> = match to {
        Format::Proto => Box::new(ProtoWriter),
        Format::Schemata => Box::new(SchemataWriter),
        Format::Xsd => anyhow::bail!("writing .xsd is not supported yet"),
    };

    let (schemas, mut warnings) = reader.read(input)?;
    tracing::info!(count = schemas.len(), "IR schemas produced");
    if schemas.is_empty() {
        tracing::warn!("no schemas found under {}", input.display());
        return Ok(());
    }

    let errors = validate(&schemas);
    if !errors.is_empty() {
        for error in &errors {
            tracing::error!("{error}");
        }
        anyhow::bail!("IR validation failed with {} error(s)", errors.len());
    }

    fs::create_dir_all(output)
        .with_context(|| format!("failed to create output directory {}", output.display()))?;
    warnings.extend(writer.write(&schemas, output)?);

    report_warnings(&warnings, deny_warnings)
}

/// Infer the input format of a directory from the files it contains
/// (recursively): .xsd wins over .schemata; empty directories default to xsd.
fn infer_dir_format(dir: &Path) -> Option<Format> {
    if !dir.is_dir() {
        return None;
    }
    fn scan(dir: &Path, saw_schemata: &mut bool) -> bool {
        let Ok(entries) = fs::read_dir(dir) else {
            return false;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if scan(&p, saw_schemata) {
                    return true;
                }
            } else {
                match Format::infer(&p) {
                    Some(Format::Xsd) => return true,
                    Some(Format::Schemata) => *saw_schemata = true,
                    _ => {}
                }
            }
        }
        false
    }
    let mut saw_schemata = false;
    if scan(dir, &mut saw_schemata) {
        Some(Format::Xsd)
    } else if saw_schemata {
        Some(Format::Schemata)
    } else {
        Some(Format::Xsd)
    }
}

fn report_warnings(warnings: &[Warning], deny: bool) -> Result<()> {
    for warning in warnings {
        tracing::warn!("{warning}");
    }
    if deny && !warnings.is_empty() {
        anyhow::bail!("{} warning(s) with --deny-warnings set", warnings.len());
    }
    Ok(())
}
