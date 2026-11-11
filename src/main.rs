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

use crate::agents::AgentsCommand;
use crate::build_info::BuildInfoCommand;
use clap::builder::styling::{AnsiColor, Styles};
use clap::{ColorChoice, Parser, Subcommand};
use clap_verbosity_flag::{Verbosity, WarnLevel};
use rust_i18n::t;
use std::process::ExitCode;
use thiserror::Error;

mod agents;
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

    /// Generate configuration from template (currently not implemented; invoking it panics).
    #[command(
        about = t!("commands.config_template.about"),
        long_about = t!("commands.config_template.long_about")
    )]
    ConfigTemplate,

    /// Discover and update installed AI coding-agent CLIs on PATH.
    #[command(
        about = t!("commands.agents.about"),
        long_about = t!("commands.agents.long_about")
    )]
    Agents(AgentsCommand),
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

#[doc(hidden)]
fn main() -> ExitCode {
    // Detect and set locale before parsing CLI args so that
    // all help text produced by clap uses the correct language.
    let locale = detect_locale();
    rust_i18n::set_locale(&locale);

    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    match cli.command {
        Commands::BuildInfo(build_info) => {
            if let Err(e) = build_info.run() {
                eprintln!("{}", t!("errors.build_info", error = e));
            }
            ExitCode::SUCCESS
        }
        Commands::ConfigTemplate => {
            todo!("config template");
        }
        Commands::Agents(agents) => agents.run(),
    }
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

    // --- Localization catalog parity (explicit per-call locale, no global state mutation) ---

    #[test]
    fn localization_catalog_parity_across_en_zh_cn_and_fallback_locales() {
        let static_keys = [
            "cli.about",
            "commands.build_info.about",
            "commands.build_info.long_about",
            "commands.config_template.about",
            "commands.config_template.long_about",
            "commands.agents.about",
            "commands.agents.long_about",
            "commands.agents.list.about",
            "commands.agents.list.long_about",
            "commands.agents.update.about",
            "commands.agents.update.long_about",
            "agents.update.agent.help",
            "agents.update.agent.long_help",
            "agents.update.dry_run.help",
            "agents.update.dry_run.long_help",
            "agents.list.empty",
            "agents.version_unknown",
            "agents.update.empty",
            "agents.update.summary_header",
            "build_info.out.help",
            "build_info.out.long_help",
            "log.generating_build_info",
            "log.getting_cwd",
            "log.determining_output",
            "log.repo_opened",
            "log.getting_head",
        ];

        for key in static_keys {
            let en = t!(key, locale = "en");
            let zh = t!(key, locale = "zh-CN");
            assert!(!en.is_empty() && en.as_ref() != key, "missing en key {key}");
            assert!(
                !zh.is_empty() && zh.as_ref() != key,
                "missing zh-CN key {key}"
            );
            assert_ne!(
                en.as_ref(),
                zh.as_ref(),
                "expected distinct translation for {key}"
            );

            for fallback_locale in ["en-US", "fr-FR", "zh-TW", "ja-JP"] {
                assert_eq!(
                    t!(key, locale = fallback_locale).as_ref(),
                    en.as_ref(),
                    "fallback mismatch on {key} for {fallback_locale}"
                );
            }
        }

        // Multiline strings, backticks, single quotes, parentheses, and Chinese quotes
        assert_eq!(
            t!("commands.build_info.long_about", locale = "en").as_ref(),
            "Write a pretty-printed JSON file containing the current repository's shortened HEAD git revision (`revision`) and UTC Unix timestamp in seconds (`build_time`)."
        );
        assert_eq!(
            t!("commands.build_info.long_about", locale = "zh-CN").as_ref(),
            "创建一个格式化的 JSON 文件，包含当前仓库缩短后的 HEAD git 修订版本（`revision`）和 UTC Unix 秒级时间戳（`build_time`）。"
        );
        assert_eq!(
            t!("commands.agents.long_about", locale = "en").as_ref(),
            "Discover and update active command-line coding agents (Codex, Antigravity, OpenCode, Kimi, Cursor, Claude Code, and Qoder) resolved from the current process PATH.\n\nOnly active CLI installations on PATH are inspected; desktop apps and editor extensions are excluded. If an installed agent is missing from the output, add its binary directory to PATH in your shell profile."
        );
        assert_eq!(
            t!("commands.agents.long_about", locale = "zh-CN").as_ref(),
            "从当前进程的 PATH 中发现并更新活动的命令行编程代理（Codex、Antigravity、OpenCode、Kimi、Cursor、Claude Code 和 Qoder）。\n\n仅检查 PATH 中的命令行安装，不包含桌面应用和编辑器扩展。如果已安装的代理未显示，请将其可执行文件目录添加到 shell 配置的 PATH 中。"
        );
        assert_eq!(
            t!("commands.agents.update.long_about", locale = "en").as_ref(),
            "Update a single named coding-agent CLI or every detected coding-agent CLI on PATH using its verified installation owner and existing release channel. Each attempted agent's diagnostics are grouped under a '--- [<index>/<total>] <Name> (<id>) ---' section on stderr, followed by a separated 'Update summary' block on stdout.\n\nUse --dry-run to preview the selected update command without modifying installed software."
        );
        assert_eq!(
            t!("commands.agents.update.long_about", locale = "zh-CN").as_ref(),
            "使用经过验证的安装归属管理器并保持现有发布通道，更新单个指定的编程代理 CLI 或 PATH 中检测到的所有编程代理 CLI。每个尝试更新的代理诊断输出会在 stderr 中按 '--- [<index>/<total>] <Name> (<id>) ---' 分段显示，随后在 stdout 中输出分隔的“更新摘要”。\n\n使用 --dry-run 可预览选定的更新命令而不修改已安装的软件。"
        );
        assert_eq!(
            t!("build_info.out.long_help", locale = "en").as_ref(),
            "Path to the output file where build information will be written.\n\nIf not specified, defaults to `./build_info.json` in the current working directory."
        );
        assert_eq!(
            t!("build_info.out.long_help", locale = "zh-CN").as_ref(),
            "构建信息将写入的输出文件路径。\n\n如果未指定，默认为当前工作目录下的 `./build_info.json`。"
        );

        // Parameterized interpolation parity across remaining catalog keys
        let interpolated_pairs = [
            (
                t!(
                    "agents.list.row",
                    locale = "en",
                    name = "Codex",
                    id = "codex",
                    version = "0.167.0",
                    path = "/opt/bin/codex",
                    method = "native (codex update)"
                )
                .into_owned(),
                "Codex (codex) | version: 0.167.0 | path: /opt/bin/codex | update: native (codex update)",
                t!(
                    "agents.list.row",
                    locale = "zh-CN",
                    name = "Codex",
                    id = "codex",
                    version = "0.167.0",
                    path = "/opt/bin/codex",
                    method = "原生自更新 (codex update)"
                )
                .into_owned(),
                "Codex (codex) | 版本: 0.167.0 | 路径: /opt/bin/codex | 更新方式: 原生自更新 (codex update)",
            ),
            (
                t!(
                    "agents.method.native",
                    locale = "en",
                    command = "agy update"
                )
                .into_owned(),
                "native (agy update)",
                t!(
                    "agents.method.native",
                    locale = "zh-CN",
                    command = "agy update"
                )
                .into_owned(),
                "原生自更新 (agy update)",
            ),
            (
                t!(
                    "agents.method.npm",
                    locale = "en",
                    package = "@openai/codex",
                    prefix = "/usr/local"
                )
                .into_owned(),
                "npm (@openai/codex in /usr/local)",
                t!(
                    "agents.method.npm",
                    locale = "zh-CN",
                    package = "@openai/codex",
                    prefix = "/usr/local"
                )
                .into_owned(),
                "npm (@openai/codex，位于 /usr/local)",
            ),
            (
                t!(
                    "agents.method.homebrew_cask",
                    locale = "en",
                    package = "codex"
                )
                .into_owned(),
                "homebrew cask (codex)",
                t!(
                    "agents.method.homebrew_cask",
                    locale = "zh-CN",
                    package = "codex"
                )
                .into_owned(),
                "Homebrew Cask (codex)",
            ),
            (
                t!(
                    "agents.method.homebrew_formula",
                    locale = "en",
                    package = "opencode"
                )
                .into_owned(),
                "homebrew formula (opencode)",
                t!(
                    "agents.method.homebrew_formula",
                    locale = "zh-CN",
                    package = "opencode"
                )
                .into_owned(),
                "Homebrew Formula (opencode)",
            ),
            (
                t!(
                    "agents.method.winget",
                    locale = "en",
                    package = "OpenAI.Codex"
                )
                .into_owned(),
                "winget (OpenAI.Codex)",
                t!(
                    "agents.method.winget",
                    locale = "zh-CN",
                    package = "OpenAI.Codex"
                )
                .into_owned(),
                "WinGet (OpenAI.Codex)",
            ),
            (
                t!(
                    "agents.method.standalone",
                    locale = "en",
                    target = "/home/u/.local/bin"
                )
                .into_owned(),
                "standalone installer (/home/u/.local/bin)",
                t!(
                    "agents.method.standalone",
                    locale = "zh-CN",
                    target = "/home/u/.local/bin"
                )
                .into_owned(),
                "官方独立安装器 (/home/u/.local/bin)",
            ),
            (
                t!(
                    "agents.method.manual",
                    locale = "en",
                    reason = "pinned channel"
                )
                .into_owned(),
                "manual action required (pinned channel)",
                t!(
                    "agents.method.manual",
                    locale = "zh-CN",
                    reason = "pinned channel"
                )
                .into_owned(),
                "需要手动处理 (pinned channel)",
            ),
            (
                t!(
                    "agents.warning.probe_timeout",
                    locale = "en",
                    name = "Codex",
                    path = "/bin/codex",
                    seconds = 5
                )
                .into_owned(),
                "Warning: version probe for Codex (/bin/codex) timed out after 5s; reporting version as unknown.",
                t!(
                    "agents.warning.probe_timeout",
                    locale = "zh-CN",
                    name = "Codex",
                    path = "/bin/codex",
                    seconds = 5
                )
                .into_owned(),
                "警告：Codex (/bin/codex) 的版本探测在 5 秒后超时；版本报告为未知。",
            ),
            (
                t!(
                    "agents.warning.probe_failed",
                    locale = "en",
                    name = "Codex",
                    path = "/bin/codex",
                    detail = "exit code 1"
                )
                .into_owned(),
                "Warning: version probe for Codex (/bin/codex) failed (exit code 1); reporting version as unknown.",
                t!(
                    "agents.warning.probe_failed",
                    locale = "zh-CN",
                    name = "Codex",
                    path = "/bin/codex",
                    detail = "exit code 1"
                )
                .into_owned(),
                "警告：Codex (/bin/codex) 的版本探测失败（exit code 1）；版本报告为未知。",
            ),
            (
                t!(
                    "agents.warning.probe_oversized",
                    locale = "en",
                    name = "Codex",
                    path = "/bin/codex"
                )
                .into_owned(),
                "Warning: version probe output for Codex (/bin/codex) exceeded 64 KiB limit; reporting version as unknown.",
                t!(
                    "agents.warning.probe_oversized",
                    locale = "zh-CN",
                    name = "Codex",
                    path = "/bin/codex"
                )
                .into_owned(),
                "警告：Codex (/bin/codex) 的版本探测输出超过 64 KiB 限制；版本报告为未知。",
            ),
            (
                t!(
                    "agents.warning.probe_unparseable",
                    locale = "en",
                    name = "Codex",
                    path = "/bin/codex"
                )
                .into_owned(),
                "Warning: could not parse version for Codex (/bin/codex) from probe output; reporting version as unknown.",
                t!(
                    "agents.warning.probe_unparseable",
                    locale = "zh-CN",
                    name = "Codex",
                    path = "/bin/codex"
                )
                .into_owned(),
                "警告：无法从探测输出中解析 Codex (/bin/codex) 的版本；版本报告为未知。",
            ),
            (
                t!(
                    "agents.update.not_installed",
                    locale = "en",
                    name = "Codex",
                    id = "codex"
                )
                .into_owned(),
                "Error: agent 'Codex' (codex) is not installed on PATH. Teemiao only updates existing installations on PATH.",
                t!(
                    "agents.update.not_installed",
                    locale = "zh-CN",
                    name = "Codex",
                    id = "codex"
                )
                .into_owned(),
                "错误：代理 'Codex' (codex) 未安装在 PATH 中。Teemiao 仅更新 PATH 中已有的安装。",
            ),
            (
                t!(
                    "agents.update.section_header",
                    locale = "fr-FR",
                    index = 2,
                    total = 7,
                    name = "Kimi",
                    id = "kimi"
                )
                .into_owned(),
                "--- [2/7] Kimi (kimi) ---",
                t!(
                    "agents.update.section_header",
                    locale = "zh-CN",
                    index = 2,
                    total = 7,
                    name = "Kimi",
                    id = "kimi"
                )
                .into_owned(),
                "--- [2/7] Kimi (kimi) ---",
            ),
            (
                t!(
                    "agents.update.dry_run_manual",
                    locale = "en",
                    name = "Codex",
                    id = "codex",
                    reason = "prerelease",
                    guidance = "update manually"
                )
                .into_owned(),
                "Codex (codex): manual action required (prerelease; guidance: update manually)",
                t!(
                    "agents.update.dry_run_manual",
                    locale = "zh-CN",
                    name = "Codex",
                    id = "codex",
                    reason = "prerelease",
                    guidance = "update manually"
                )
                .into_owned(),
                "Codex (codex): 需要手动处理 (prerelease; 指引: update manually)",
            ),
            (
                t!(
                    "agents.update.outcome.updated",
                    locale = "en",
                    name = "Antigravity",
                    id = "antigravity",
                    before = "1.0.0",
                    after = "1.1.0",
                    path = "/bin/agy"
                )
                .into_owned(),
                "Antigravity (antigravity): updated (1.0.0 -> 1.1.0; path: /bin/agy)",
                t!(
                    "agents.update.outcome.updated",
                    locale = "zh-CN",
                    name = "Antigravity",
                    id = "antigravity",
                    before = "1.0.0",
                    after = "1.1.0",
                    path = "/bin/agy"
                )
                .into_owned(),
                "Antigravity (antigravity): 已更新 (1.0.0 -> 1.1.0; 路径: /bin/agy)",
            ),
            (
                t!(
                    "agents.update.outcome.up_to_date",
                    locale = "en",
                    name = "Kimi",
                    id = "kimi",
                    version = "0.5.0",
                    path = "/bin/kimi"
                )
                .into_owned(),
                "Kimi (kimi): up to date (version: 0.5.0; path: /bin/kimi)",
                t!(
                    "agents.update.outcome.up_to_date",
                    locale = "zh-CN",
                    name = "Kimi",
                    id = "kimi",
                    version = "0.5.0",
                    path = "/bin/kimi"
                )
                .into_owned(),
                "Kimi (kimi): 已是最新 (版本: 0.5.0; 路径: /bin/kimi)",
            ),
            (
                t!(
                    "agents.update.outcome.manual",
                    locale = "en",
                    name = "Codex",
                    id = "codex",
                    reason = "pinned",
                    guidance = "run manually"
                )
                .into_owned(),
                "Codex (codex): manual action required (pinned; guidance: run manually)",
                t!(
                    "agents.update.outcome.manual",
                    locale = "zh-CN",
                    name = "Codex",
                    id = "codex",
                    reason = "pinned",
                    guidance = "run manually"
                )
                .into_owned(),
                "Codex (codex): 需要手动处理 (pinned; 指引: run manually)",
            ),
            (
                t!(
                    "agents.update.outcome.unverified",
                    locale = "en",
                    name = "Codex",
                    id = "codex",
                    reason = "probe timed out",
                    path = "/bin/codex"
                )
                .into_owned(),
                "Codex (codex): unverified (probe timed out; path: /bin/codex)",
                t!(
                    "agents.update.outcome.unverified",
                    locale = "zh-CN",
                    name = "Codex",
                    id = "codex",
                    reason = "probe timed out",
                    path = "/bin/codex"
                )
                .into_owned(),
                "Codex (codex): 未能验证更新结果 (probe timed out; 路径: /bin/codex)",
            ),
            (
                t!(
                    "agents.update.outcome.failed",
                    locale = "en",
                    name = "OpenCode",
                    id = "opencode",
                    reason = "exit 1",
                    path = "/bin/opencode"
                )
                .into_owned(),
                "OpenCode (opencode): failed (exit 1; path: /bin/opencode)",
                t!(
                    "agents.update.outcome.failed",
                    locale = "zh-CN",
                    name = "OpenCode",
                    id = "opencode",
                    reason = "exit 1",
                    path = "/bin/opencode"
                )
                .into_owned(),
                "OpenCode (opencode): 更新失败 (exit 1; 路径: /bin/opencode)",
            ),
            (
                t!(
                    "agents.update.outcome.interrupted",
                    locale = "en",
                    name = "Kimi",
                    id = "kimi"
                )
                .into_owned(),
                "Kimi (kimi): interrupted (update cancelled before completion)",
                t!(
                    "agents.update.outcome.interrupted",
                    locale = "zh-CN",
                    name = "Kimi",
                    id = "kimi"
                )
                .into_owned(),
                "Kimi (kimi): 已中断 (更新在完成前被取消)",
            ),
            (
                t!(
                    "agents.update.diagnostic",
                    locale = "en",
                    name = "Codex",
                    id = "codex",
                    detail = "warning line"
                )
                .into_owned(),
                "Diagnostics for Codex (codex): warning line",
                t!(
                    "agents.update.diagnostic",
                    locale = "zh-CN",
                    name = "Codex",
                    id = "codex",
                    detail = "warning line"
                )
                .into_owned(),
                "Codex (codex) 诊断信息: warning line",
            ),
            (
                t!(
                    "errors.build_info",
                    locale = "en",
                    error = "Head ID not found"
                )
                .into_owned(),
                "Error generating build info: Head ID not found",
                t!(
                    "errors.build_info",
                    locale = "zh-CN",
                    error = "Head ID not found"
                )
                .into_owned(),
                "生成构建信息时出错：Head ID not found",
            ),
            (
                t!("log.cwd", locale = "en", path = "/repo").into_owned(),
                "Current working directory: /repo",
                t!("log.cwd", locale = "zh-CN", path = "/repo").into_owned(),
                "当前工作目录：/repo",
            ),
            (
                t!("log.output_file", locale = "en", path = "/repo/out.json").into_owned(),
                "Output file: /repo/out.json",
                t!("log.output_file", locale = "zh-CN", path = "/repo/out.json").into_owned(),
                "输出文件：/repo/out.json",
            ),
            (
                t!("log.opening_repo", locale = "en", path = "/repo").into_owned(),
                "Opening /repo as git repository...",
                t!("log.opening_repo", locale = "zh-CN", path = "/repo").into_owned(),
                "正在打开 /repo 作为 git 仓库...",
            ),
            (
                t!("log.head_obtained", locale = "en", id = "abc1234").into_owned(),
                "Head obtained successfully: abc1234",
                t!("log.head_obtained", locale = "zh-CN", id = "abc1234").into_owned(),
                "成功获取 HEAD：abc1234",
            ),
            (
                t!("log.getting_short_rev", locale = "en", id = "abc1234").into_owned(),
                "Getting short revision for abc1234...",
                t!("log.getting_short_rev", locale = "zh-CN", id = "abc1234").into_owned(),
                "正在获取 abc1234 的短修订版本...",
            ),
            (
                t!(
                    "log.short_rev_obtained",
                    locale = "en",
                    revision = "abc1234"
                )
                .into_owned(),
                "Short revision obtained successfully: abc1234",
                t!(
                    "log.short_rev_obtained",
                    locale = "zh-CN",
                    revision = "abc1234"
                )
                .into_owned(),
                "成功获取短修订版本：abc1234",
            ),
            (
                t!(
                    "log.build_info_created",
                    locale = "en",
                    info = "BuildInfo { revision: \"abc1234\" }"
                )
                .into_owned(),
                "Build info created successfully: BuildInfo { revision: \"abc1234\" }",
                t!(
                    "log.build_info_created",
                    locale = "zh-CN",
                    info = "BuildInfo { revision: \"abc1234\" }"
                )
                .into_owned(),
                "构建信息创建成功：BuildInfo { revision: \"abc1234\" }",
            ),
            (
                t!(
                    "log.writing_build_info",
                    locale = "en",
                    path = "/repo/out.json"
                )
                .into_owned(),
                "Writing build info to /repo/out.json...",
                t!(
                    "log.writing_build_info",
                    locale = "zh-CN",
                    path = "/repo/out.json"
                )
                .into_owned(),
                "正在将构建信息写入 /repo/out.json...",
            ),
            (
                t!(
                    "log.build_info_written",
                    locale = "en",
                    path = "/repo/out.json"
                )
                .into_owned(),
                "Build info successfully written to /repo/out.json",
                t!(
                    "log.build_info_written",
                    locale = "zh-CN",
                    path = "/repo/out.json"
                )
                .into_owned(),
                "构建信息已成功写入 /repo/out.json",
            ),
        ];

        for (actual_en, expected_en, actual_zh, expected_zh) in interpolated_pairs {
            assert_eq!(actual_en, expected_en);
            assert_eq!(actual_zh, expected_zh);
            assert!(!actual_en.contains("%{"));
            assert!(!actual_zh.contains("%{"));
        }
    }
}
