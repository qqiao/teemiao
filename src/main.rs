mod build_info;

use clap::builder::styling::{AnsiColor, Styles};
use clap::{ColorChoice, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

use crate::build_info::BuildInfoCommand;

/// Teemiao error
pub struct TeemiaoError {
    /// Error message
    message: String,
}

impl std::fmt::Display for TeemiaoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Teemiao is a tool that I have created for no particular reason at all just yet.
#[doc(hidden)]
#[derive(Debug, Parser)]
#[command(version, about, arg_required_else_help = true,
    color = ColorChoice::Auto,
    styles = Styles::styled()
        .header(AnsiColor::Green.on_default().bold())
        .usage(AnsiColor::Green.on_default().bold())
        .literal(AnsiColor::Blue.on_default().bold())
        .placeholder(AnsiColor::Cyan.on_default())
)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,

    /// Command to run
    #[command(subcommand)]
    command: Commands,
}

/// Commands
#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum Commands {
    /// Print build information
    BuildInfo(BuildInfoCommand),

    /// Generate configuration from template
    ConfigTemplate,
}

#[doc(hidden)]
fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::BuildInfo(build_info) => match build_info.run() {
            Ok(_) => (),
            Err(e) => eprintln!("Error: {}", e),
        },
        Commands::ConfigTemplate => {
            todo!("config template");
        }
    }
}
