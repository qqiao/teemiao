pub mod build_info;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

use crate::build_info::BuildInfo;

pub struct TeemiaoError {
    message: String,
}

impl std::fmt::Display for TeemiaoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Teemiao is a tool that I have created for no particular reason at all just yet.
#[derive(Debug, Parser)]
#[command(version, about, arg_required_else_help = true)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,

    /// Command to run
    #[command(subcommand)]
    command: Commands,
}

/// Commands
#[derive(Debug, Subcommand)]
enum Commands {
    /// Print build information
    BuildInfo(BuildInfo),

    /// Generate configuration from template
    ConfigTemplate,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::BuildInfo(build_info) => match build_info.generate() {
            Ok(_) => (),
            Err(e) => eprintln!("Error: {}", e),
        },
        Commands::ConfigTemplate => {
            todo!("config template");
        }
    }
}
