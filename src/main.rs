// Copyright 2024 Qian Qiao
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Teemiao is a versatile toolkit designed to streamline application
//! development workflows.

use crate::build_info::BuildInfoCommand;
use clap::builder::styling::{AnsiColor, Styles};
use clap::{ColorChoice, Parser, Subcommand};
use clap_verbosity_flag::{Verbosity, WarnLevel};
use thiserror::Error;

mod build_info;

/// Different types of errors that can occur in Teemiao.
#[derive(Debug, Error)]
pub enum TeemiaoError {
    #[error("Failed to generate build info: {0}")]
    BuildInfo(#[from] build_info::BuildInfoError),
    #[error("Failed to generate configuration from template: {0}")]
    ConfigTemplate(String),
}

/// Teemiao is a set of convenient tools for building other applications.
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
    verbose: Verbosity<WarnLevel>,

    #[command(subcommand)]
    command: Commands,
}

/// Commands supported by Teemiao.
#[derive(Debug, Subcommand)]
enum Commands {
    BuildInfo(BuildInfoCommand),

    /// Generate configuration from template
    ConfigTemplate,
}

#[doc(hidden)]
fn main() {
    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    match cli.command {
        Commands::BuildInfo(build_info) => match build_info.run() {
            Ok(_) => (),
            Err(e) => eprintln!("Error generating build info: {}", e),
        },
        Commands::ConfigTemplate => {
            todo!("config template");
        }
    }
}
