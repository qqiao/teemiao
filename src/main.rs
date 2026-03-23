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
use rust_i18n::t;
use thiserror::Error;

mod build_info;

rust_i18n::i18n!("locales", fallback = "en");

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
#[command(version,
    about = t!("cli.about"),
    arg_required_else_help = true,
    color = ColorChoice::Auto,
    styles = Styles::styled()
        .header(AnsiColor::Green.on_default().bold())
        .usage(AnsiColor::Green.on_default().bold())
        .literal(AnsiColor::Blue.on_default().bold())
        .placeholder(AnsiColor::Cyan.on_default())
)]
struct Cli {
    /// Verbosity level for logging output.
    #[command(flatten)]
    verbose: Verbosity<WarnLevel>,

    /// The subcommand to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Commands supported by Teemiao.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate build information metadata in JSON format.
    #[command(
        about = t!("commands.build_info.about"),
        long_about = t!("commands.build_info.long_about")
    )]
    BuildInfo(BuildInfoCommand),

    /// Generate configuration from template.
    #[command(
        about = t!("commands.config_template.about"),
        long_about = t!("commands.config_template.long_about")
    )]
    ConfigTemplate,
}

/// Detects the user's locale from environment variables.
///
/// Checks the following environment variables in order of priority:
/// 1. `LC_ALL`
/// 2. `LC_MESSAGES`
/// 3. `LANG`
///
/// Parses values like `zh_CN.UTF-8` into `zh-CN` format.
/// Returns `"en"` as the default if no locale is detected.
fn detect_locale() -> String {
    let raw = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();

    if raw.is_empty() || raw == "C" || raw == "POSIX" {
        return "en".to_string();
    }

    // Strip encoding suffix (e.g. ".UTF-8")
    let locale = raw.split('.').next().unwrap_or("en");

    // Convert underscore to hyphen (e.g. "zh_CN" -> "zh-CN")
    locale.replace('_', "-")
}

#[doc(hidden)]
fn main() {
    // Detect and set locale before parsing CLI args so that
    // all help text produced by clap uses the correct language.
    let locale = detect_locale();
    rust_i18n::set_locale(&locale);

    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    match cli.command {
        Commands::BuildInfo(build_info) => match build_info.run() {
            Ok(_) => (),
            Err(e) => eprintln!("{}", t!("errors.build_info", error = e)),
        },
        Commands::ConfigTemplate => {
            todo!("config template");
        }
    }
}
