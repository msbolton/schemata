use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::transform::profile::SchemaProfile;

/// schemata - XSD to Protocol Buffers converter for NIEM-based schemas
#[derive(Debug, Parser)]
#[command(name = "schemata", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Convert XSD schemas to Protocol Buffers definitions
    Convert {
        /// Path to the input XSD file or directory
        #[arg(short, long)]
        input: PathBuf,

        /// Output directory for generated .proto files
        #[arg(short, long)]
        output: PathBuf,

        /// Schema profile controlling NIEM-specific transforms
        #[arg(long, value_enum, default_value_t = SchemaProfile::Niem)]
        profile: SchemaProfile,
    },
}
