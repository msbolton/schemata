use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::convert::Format;
use crate::readers::xsd::transform::profile::SchemaProfile;

/// schemata - schema converter with a rich intermediary language
#[derive(Debug, Parser)]
#[command(name = "schemata", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Convert schemas between formats (via the schemata IR)
    Convert {
        /// Input file or directory (.xsd or .schemata; directories are scanned)
        #[arg(short, long)]
        input: PathBuf,

        /// Output directory
        #[arg(short, long)]
        output: PathBuf,

        /// Input format (inferred from the input extension when omitted;
        /// directories are inferred from their contents, defaulting to xsd)
        #[arg(long, value_enum)]
        from: Option<Format>,

        /// Output format
        #[arg(long, value_enum, default_value = "proto")]
        to: Format,

        /// Schema profile controlling NIEM-specific transforms (XSD input only)
        #[arg(long, value_enum, default_value_t = SchemaProfile::Niem)]
        profile: SchemaProfile,

        /// Treat information-loss warnings as errors
        #[arg(long)]
        deny_warnings: bool,
    },
}
