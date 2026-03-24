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

/// Parses a raw locale string into a normalized locale identifier.
///
/// Handles POSIX locale format: `language[_territory][.codeset][@modifier]`
///
/// - Strips encoding suffixes (e.g. `.UTF-8`) and modifiers (e.g. `@euro`)
/// - Converts underscore separators to hyphens
/// - Normalizes casing: lowercase language, uppercase region
/// - Returns `"en"` for empty, `"C"`, and `"POSIX"` locales
///
/// # Examples
///
/// - `"zh_CN.UTF-8"` → `"zh-CN"`
/// - `"en_US.UTF-8@euro"` → `"en-US"`
/// - `"C.UTF-8"` → `"en"`
/// - `""` → `"en"`
fn parse_locale_string(raw: &str) -> String {
    // Strip encoding suffix (e.g. ".UTF-8") and modifier (e.g. "@latin", "@euro")
    let locale = raw.split(&['.', '@'][..]).next().unwrap_or("");

    // Handle empty, C, and POSIX locales (including "C.UTF-8", "POSIX.UTF-8", etc.)
    if locale.is_empty() || locale == "C" || locale == "POSIX" {
        return "en".to_string();
    }

    // Normalize to language-region with proper casing (e.g. "zh_CN" / "zh-cn" -> "zh-CN")
    let locale = locale.replace('_', "-");
    let mut parts = locale.split('-');
    let lang = parts.next().unwrap_or("en").to_lowercase();

    if let Some(region) = parts.next() {
        format!("{}-{}", lang, region.to_uppercase())
    } else {
        lang
    }
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

    parse_locale_string(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Standard POSIX locale strings with encoding ---

    #[test]
    fn standard_locales_with_utf8_encoding() {
        assert_eq!(parse_locale_string("en_US.UTF-8"), "en-US");
        assert_eq!(parse_locale_string("zh_CN.UTF-8"), "zh-CN");
        assert_eq!(parse_locale_string("zh_TW.UTF-8"), "zh-TW");
        assert_eq!(parse_locale_string("fr_FR.UTF-8"), "fr-FR");
        assert_eq!(parse_locale_string("de_DE.UTF-8"), "de-DE");
        assert_eq!(parse_locale_string("ja_JP.UTF-8"), "ja-JP");
        assert_eq!(parse_locale_string("ko_KR.UTF-8"), "ko-KR");
        assert_eq!(parse_locale_string("pt_BR.UTF-8"), "pt-BR");
        assert_eq!(parse_locale_string("es_ES.UTF-8"), "es-ES");
        assert_eq!(parse_locale_string("it_IT.UTF-8"), "it-IT");
        assert_eq!(parse_locale_string("ru_RU.UTF-8"), "ru-RU");
    }

    #[test]
    fn standard_locales_with_non_utf8_encoding() {
        assert_eq!(parse_locale_string("en_GB.iso88591"), "en-GB");
        assert_eq!(parse_locale_string("de_DE.iso885915"), "de-DE");
        assert_eq!(parse_locale_string("ja_JP.eucjp"), "ja-JP");
        assert_eq!(parse_locale_string("zh_CN.gb2312"), "zh-CN");
    }

    // --- Language only (no region, no encoding) ---

    #[test]
    fn language_only() {
        assert_eq!(parse_locale_string("en"), "en");
        assert_eq!(parse_locale_string("fr"), "fr");
        assert_eq!(parse_locale_string("zh"), "zh");
        assert_eq!(parse_locale_string("ja"), "ja");
        assert_eq!(parse_locale_string("de"), "de");
    }

    #[test]
    fn language_with_encoding_no_region() {
        assert_eq!(parse_locale_string("en.UTF-8"), "en");
        assert_eq!(parse_locale_string("fr.UTF-8"), "fr");
        assert_eq!(parse_locale_string("zh.UTF-8"), "zh");
    }

    // --- Default / fallback values ---

    #[test]
    fn empty_string_returns_default() {
        assert_eq!(parse_locale_string(""), "en");
    }

    #[test]
    fn c_locale_returns_default() {
        assert_eq!(parse_locale_string("C"), "en");
    }

    #[test]
    fn posix_locale_returns_default() {
        assert_eq!(parse_locale_string("POSIX"), "en");
    }

    #[test]
    fn c_locale_with_encoding_returns_default() {
        assert_eq!(parse_locale_string("C.UTF-8"), "en");
        assert_eq!(parse_locale_string("C.utf8"), "en");
    }

    #[test]
    fn posix_locale_with_encoding_returns_default() {
        assert_eq!(parse_locale_string("POSIX.UTF-8"), "en");
    }

    // --- Modifiers ---

    #[test]
    fn locale_with_modifier_stripped() {
        assert_eq!(parse_locale_string("sr_RS@latin"), "sr-RS");
        assert_eq!(parse_locale_string("ca_ES@valencia"), "ca-ES");
        assert_eq!(parse_locale_string("uz_UZ@cyrillic"), "uz-UZ");
    }

    #[test]
    fn locale_with_encoding_and_modifier_stripped() {
        assert_eq!(parse_locale_string("de_DE.UTF-8@euro"), "de-DE");
        assert_eq!(parse_locale_string("sr_RS.UTF-8@latin"), "sr-RS");
        assert_eq!(parse_locale_string("ca_ES.UTF-8@valencia"), "ca-ES");
    }

    #[test]
    fn modifier_only_returns_default() {
        assert_eq!(parse_locale_string("@euro"), "en");
        assert_eq!(parse_locale_string("@latin"), "en");
    }

    // --- Case normalization ---

    #[test]
    fn lowercase_region_is_uppercased() {
        assert_eq!(parse_locale_string("zh_cn"), "zh-CN");
        assert_eq!(parse_locale_string("en_us"), "en-US");
        assert_eq!(parse_locale_string("pt_br"), "pt-BR");
        assert_eq!(parse_locale_string("zh_cn.UTF-8"), "zh-CN");
    }

    #[test]
    fn uppercase_language_is_lowercased() {
        assert_eq!(parse_locale_string("EN"), "en");
        assert_eq!(parse_locale_string("FR"), "fr");
        assert_eq!(parse_locale_string("ZH_CN"), "zh-CN");
        assert_eq!(parse_locale_string("EN_US"), "en-US");
        assert_eq!(parse_locale_string("ZH_CN.UTF-8"), "zh-CN");
    }

    #[test]
    fn mixed_case_is_normalized() {
        assert_eq!(parse_locale_string("eN_uS"), "en-US");
        assert_eq!(parse_locale_string("Zh_cN"), "zh-CN");
        assert_eq!(parse_locale_string("En_Gb.UTF-8"), "en-GB");
    }

    // --- Hyphen as separator (already BCP-47 style) ---

    #[test]
    fn hyphen_separated_passthrough() {
        assert_eq!(parse_locale_string("zh-CN"), "zh-CN");
        assert_eq!(parse_locale_string("en-US"), "en-US");
        assert_eq!(parse_locale_string("pt-BR"), "pt-BR");
    }

    #[test]
    fn hyphen_separated_case_normalized() {
        assert_eq!(parse_locale_string("zh-cn"), "zh-CN");
        assert_eq!(parse_locale_string("en-us"), "en-US");
        assert_eq!(parse_locale_string("ZH-CN"), "zh-CN");
    }

    // --- Edge cases ---

    #[test]
    fn encoding_only_returns_default() {
        assert_eq!(parse_locale_string(".UTF-8"), "en");
        assert_eq!(parse_locale_string(".iso88591"), "en");
    }

    #[test]
    fn three_letter_language_code() {
        assert_eq!(parse_locale_string("ast_ES.UTF-8"), "ast-ES");
        assert_eq!(parse_locale_string("ber_DZ.UTF-8"), "ber-DZ");
    }

    #[test]
    fn locale_without_encoding_or_modifier() {
        assert_eq!(parse_locale_string("en_US"), "en-US");
        assert_eq!(parse_locale_string("zh_TW"), "zh-TW");
        assert_eq!(parse_locale_string("fr_CA"), "fr-CA");
    }
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
