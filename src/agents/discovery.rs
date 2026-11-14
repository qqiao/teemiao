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

//! Executable discovery, bounded version/identity probing, and installation
//! ownership resolution for supported coding-agent CLIs.

use crate::agents::{AgentId, ProcessError, ProcessOutput, ProcessRequest, ProcessRunner};
use rust_i18n::t;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Maximum duration allowed for any single discovery/version probe child process.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum bytes captured per stream (stdout/stderr) during a discovery probe.
pub const PROBE_MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Environment variables passed to probe child processes to suppress vendor
/// background auto-updaters and update notifications without persisting state.
pub const PROBE_ENV_OVERRIDES: &[(&str, &str)] = &[
    ("CI", "1"),
    ("DISABLE_AUTOUPDATER", "1"),
    ("NO_UPDATE_NOTIFIER", "1"),
    ("NPM_CONFIG_UPDATE_NOTIFIER", "false"),
    ("HOMEBREW_NO_AUTO_UPDATE", "1"),
    ("CODEX_DISABLE_AUTO_UPDATE", "1"),
    ("OPENCODE_DISABLE_AUTOUPDATE", "1"),
];

/// Snapshot of process environment used for deterministic PATH and ownership resolution.
#[derive(Debug, Clone)]
pub struct DiscoveryEnv {
    /// Current `PATH` variable value.
    pub path: OsString,
    /// Windows `PATHEXT` variable value (when running on or simulating Windows).
    pub pathext: Option<OsString>,
    /// User home directory (`HOME` or `USERPROFILE`).
    pub home_dir: Option<PathBuf>,
    /// Windows `LOCALAPPDATA` directory.
    pub local_app_data: Option<PathBuf>,
    /// Explicit `CODEX_HOME` override if set in the environment.
    pub codex_home: Option<PathBuf>,
    /// Explicit `CODEX_INSTALL_DIR` override if set in the environment.
    pub codex_install_dir: Option<PathBuf>,
    /// Explicit `HOMEBREW_PREFIX` override if set in the environment.
    pub homebrew_prefix: Option<PathBuf>,
    /// Explicit `QODER_CLI_BIN` override if set in the environment.
    pub qoder_cli_bin: Option<PathBuf>,
    /// Whether Windows executable/shim rules apply.
    pub is_windows: bool,
}

impl DiscoveryEnv {
    /// Captures the active process environment.
    pub fn from_process() -> Self {
        let home_dir = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        Self {
            path: std::env::var_os("PATH").unwrap_or_default(),
            pathext: std::env::var_os("PATHEXT"),
            home_dir,
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            codex_home: std::env::var_os("CODEX_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            codex_install_dir: std::env::var_os("CODEX_INSTALL_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            homebrew_prefix: std::env::var_os("HOMEBREW_PREFIX")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            qoder_cli_bin: std::env::var_os("QODER_CLI_BIN")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            is_windows: cfg!(windows),
        }
    }

    /// Resolves the effective Codex home directory (`CODEX_HOME` or `~/.codex`).
    pub fn effective_codex_home(&self) -> Option<PathBuf> {
        if let Some(ref custom) = self.codex_home {
            return Some(custom.clone());
        }
        self.home_dir.as_ref().map(|h| h.join(".codex"))
    }

    /// Resolves the expected default Codex standalone binary directory.
    pub fn expected_codex_bin_dir(&self) -> Option<PathBuf> {
        if let Some(ref custom) = self.codex_install_dir {
            return Some(custom.clone());
        }
        if self.is_windows {
            self.local_app_data.as_ref().map(|lad| {
                lad.join("Programs")
                    .join("OpenAI")
                    .join("Codex")
                    .join("bin")
            })
        } else {
            self.home_dir.as_ref().map(|h| h.join(".local").join("bin"))
        }
    }
}

/// Verified ownership and update strategy for a discovered agent installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOwner {
    /// Native self-updater subcommand on the active CLI binary itself.
    NativeSelfUpdate {
        program: PathBuf,
        args: Vec<String>,
        display_command: String,
    },
    /// Global npm package owned by a specific prefix.
    NpmGlobal {
        npm_program: PathBuf,
        prefix: PathBuf,
        package: String,
    },
    /// Homebrew cask installation with an exact cask token (preserving channel).
    HomebrewCask {
        brew_program: PathBuf,
        cask_token: String,
    },
    /// Homebrew formula installation.
    HomebrewFormula {
        brew_program: PathBuf,
        formula: String,
    },
    /// Windows Package Manager (WinGet) installation with an exact package identifier.
    WinGet {
        winget_program: PathBuf,
        package_id: String,
    },
    /// Official OpenAI Codex standalone installation with verified target paths.
    CodexStandalone {
        bin_dir: PathBuf,
        codex_home: PathBuf,
        is_windows: bool,
    },
    /// Installation whose owner/channel cannot be safely updated automatically.
    ManualActionRequired { reason: String, guidance: String },
}

impl UpdateOwner {
    /// Formats a localized human-readable summary of the update method for `agents list`.
    pub fn display_method(&self) -> String {
        match self {
            UpdateOwner::NativeSelfUpdate {
                display_command, ..
            } => t!("agents.method.native", command = display_command).to_string(),
            UpdateOwner::NpmGlobal {
                prefix, package, ..
            } => t!(
                "agents.method.npm",
                package = package,
                prefix = prefix.display()
            )
            .to_string(),
            UpdateOwner::HomebrewCask { cask_token, .. } => {
                t!("agents.method.homebrew_cask", package = cask_token).to_string()
            }
            UpdateOwner::HomebrewFormula { formula, .. } => {
                t!("agents.method.homebrew_formula", package = formula).to_string()
            }
            UpdateOwner::WinGet { package_id, .. } => {
                t!("agents.method.winget", package = package_id).to_string()
            }
            UpdateOwner::CodexStandalone { bin_dir, .. } => {
                t!("agents.method.standalone", target = bin_dir.display()).to_string()
            }
            UpdateOwner::ManualActionRequired { reason, .. } => {
                t!("agents.method.manual", reason = reason).to_string()
            }
        }
    }
}

/// Record describing one discovered coding-agent CLI installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredAgent {
    /// Canonical catalog entry.
    pub id: AgentId,
    /// Parsed semantic version, or `None` when the probe failed/timed out/returned unparseable output.
    pub version: Option<String>,
    /// Active executable path selected from `PATH`.
    pub launcher_path: PathBuf,
    /// Canonical symlink-resolved target path.
    pub resolved_path: PathBuf,
    /// Verified installation owner and update method.
    pub owner: UpdateOwner,
    /// Non-fatal warnings generated while probing this installation.
    pub warnings: Vec<String>,
}

impl DiscoveredAgent {
    /// Returns the version string or the localized `unknown` placeholder.
    pub fn display_version(&self) -> String {
        self.version
            .clone()
            .unwrap_or_else(|| t!("agents.version_unknown").to_string())
    }
}

/// Per-invocation cache of read-only package-manager queries so we never run
/// duplicate inventory commands across multiple agents.
#[derive(Default)]
pub struct PackageManagerCache {
    brew_casks: HashMap<PathBuf, Option<Vec<String>>>,
    brew_formulas: HashMap<PathBuf, Option<Vec<String>>>,
    brew_prefix: HashMap<PathBuf, Option<PathBuf>>,
    winget_packages: HashMap<String, bool>,
}

impl PackageManagerCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn brew_casks(&mut self, brew_bin: &Path, runner: &dyn ProcessRunner) -> Option<Vec<String>> {
        if let Some(cached) = self.brew_casks.get(brew_bin) {
            return cached.clone();
        }
        let output = run_bounded_probe(brew_bin, &["list", "--cask"], runner).ok()?;
        let result = if output.success && !output.timed_out && !output.stdout_truncated {
            let text = String::from_utf8_lossy(&output.stdout);
            Some(
                text.split_whitespace()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
            )
        } else {
            None
        };
        self.brew_casks
            .insert(brew_bin.to_path_buf(), result.clone());
        result
    }

    fn brew_formulas(
        &mut self,
        brew_bin: &Path,
        runner: &dyn ProcessRunner,
    ) -> Option<Vec<String>> {
        if let Some(cached) = self.brew_formulas.get(brew_bin) {
            return cached.clone();
        }
        let output = run_bounded_probe(brew_bin, &["list", "--formula"], runner).ok()?;
        let result = if output.success && !output.timed_out && !output.stdout_truncated {
            let text = String::from_utf8_lossy(&output.stdout);
            Some(
                text.split_whitespace()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
            )
        } else {
            None
        };
        self.brew_formulas
            .insert(brew_bin.to_path_buf(), result.clone());
        result
    }

    fn brew_prefix(&mut self, brew_bin: &Path, runner: &dyn ProcessRunner) -> Option<PathBuf> {
        if let Some(cached) = self.brew_prefix.get(brew_bin) {
            return cached.clone();
        }
        let output = run_bounded_probe(brew_bin, &["--prefix"], runner).ok();
        let result = output.and_then(|out| {
            if out.success && !out.timed_out && !out.stdout_truncated {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let candidate = PathBuf::from(&text);
                if !text.is_empty() && candidate.is_absolute() {
                    return Some(candidate);
                }
            }
            None
        });
        self.brew_prefix
            .insert(brew_bin.to_path_buf(), result.clone());
        result
    }

    fn winget_has_package(
        &mut self,
        winget_bin: &Path,
        package_id: &str,
        runner: &dyn ProcessRunner,
    ) -> bool {
        if let Some(&cached) = self.winget_packages.get(package_id) {
            return cached;
        }
        let args = [
            "list",
            "--exact",
            "--id",
            package_id,
            "--accept-source-agreements",
            "--disable-interactivity",
        ];
        let present = match run_bounded_probe(winget_bin, &args, runner) {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                out.success
                    && !out.timed_out
                    && stdout
                        .to_ascii_lowercase()
                        .contains(&package_id.to_ascii_lowercase())
            }
            Err(_) => false,
        };
        self.winget_packages.insert(package_id.to_string(), present);
        present
    }
}

/// Discovers all installed supported coding-agent CLIs on `PATH` in catalog order.
pub fn discover_all_agents(
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> Vec<DiscoveredAgent> {
    let mut discovered = Vec::new();
    for id in AgentId::ALL {
        if let Some(agent) = discover_agent(id, env, runner, cache) {
            discovered.push(agent);
        }
    }
    discovered
}

/// Discovers a single coding-agent CLI on `PATH`, if installed.
pub fn discover_agent(
    id: AgentId,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> Option<DiscoveredAgent> {
    match id {
        AgentId::Cursor => discover_cursor(env, runner),
        AgentId::Qoder => discover_qoder(env, runner),
        _ => {
            for &candidate in id.cli_candidates() {
                if let Some((launcher_path, resolved_path)) =
                    resolve_executable_on_path(candidate, env)
                {
                    return Some(inspect_candidate(
                        id,
                        launcher_path,
                        resolved_path,
                        env,
                        runner,
                        cache,
                    ));
                }
            }
            None
        }
    }
}

/// Discovers Cursor CLI, preferring a verified `agent` binary over legacy `cursor-agent`
/// and rejecting unverified generic `agent` binaries.
fn discover_cursor(env: &DiscoveryEnv, runner: &dyn ProcessRunner) -> Option<DiscoveredAgent> {
    if let Some((launcher_path, resolved_path)) = resolve_executable_on_path("agent", env)
        && let Some(verified) = try_verify_cursor_agent(launcher_path, resolved_path, env, runner)
    {
        return Some(verified);
    }

    if let Some((launcher_path, resolved_path)) = resolve_executable_on_path("cursor-agent", env) {
        let mut warnings = Vec::new();
        let version = probe_version_with_warnings(
            AgentId::Cursor,
            &launcher_path,
            env,
            runner,
            &mut warnings,
        );
        let owner = determine_cursor_owner(&launcher_path, &resolved_path, env);
        return Some(DiscoveredAgent {
            id: AgentId::Cursor,
            version,
            launcher_path,
            resolved_path,
            owner,
            warnings,
        });
    }

    None
}

fn try_verify_cursor_agent(
    launcher_path: PathBuf,
    resolved_path: PathBuf,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
) -> Option<DiscoveredAgent> {
    let has_local_cursor_evidence =
        path_has_cursor_evidence(&launcher_path) || path_has_cursor_evidence(&resolved_path);

    if env.is_windows && is_unsafe_windows_cmd_shim(&launcher_path) {
        if !has_local_cursor_evidence {
            return None;
        }
        let warning = t!(
            "agents.warning.probe_failed",
            name = AgentId::Cursor.display_name(),
            path = launcher_path.display(),
            detail = "unsafe Windows .cmd shim path metacharacters"
        )
        .to_string();
        return Some(DiscoveredAgent {
            id: AgentId::Cursor,
            version: None,
            owner: UpdateOwner::ManualActionRequired {
                reason: "Windows .cmd path contains unsafe shell metacharacters".to_string(),
                guidance:
                    "Reinstall Cursor CLI in a standard directory without shell metacharacters."
                        .to_string(),
            },
            launcher_path,
            resolved_path,
            warnings: vec![warning],
        });
    }

    let mut warnings = Vec::new();
    let probe_result = run_bounded_probe(&launcher_path, &["--version"], runner);

    match probe_result {
        Ok(output) => {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if has_conflicting_non_cursor_product(&combined) {
                return None;
            }
            let mut branding_verified =
                contains_cursor_branding(&combined) || has_local_cursor_evidence;

            if !branding_verified
                && output.success
                && !output.timed_out
                && !output.stdout_truncated
                && let Ok(help_out) = run_bounded_probe(&launcher_path, &["--help"], runner)
                && help_out.success
                && !help_out.timed_out
            {
                let help_text = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&help_out.stdout),
                    String::from_utf8_lossy(&help_out.stderr)
                );
                if contains_cursor_branding(&help_text) {
                    branding_verified = true;
                }
            }

            if !branding_verified {
                return None;
            }

            let version = interpret_version_probe_output(
                AgentId::Cursor,
                &launcher_path,
                &output,
                &mut warnings,
            );
            let owner = determine_cursor_owner(&launcher_path, &resolved_path, env);
            Some(DiscoveredAgent {
                id: AgentId::Cursor,
                version,
                launcher_path,
                resolved_path,
                owner,
                warnings,
            })
        }
        Err(err) => {
            if !has_local_cursor_evidence {
                return None;
            }
            warnings.push(
                t!(
                    "agents.warning.probe_failed",
                    name = AgentId::Cursor.display_name(),
                    path = launcher_path.display(),
                    detail = err.to_string()
                )
                .to_string(),
            );
            let owner = determine_cursor_owner(&launcher_path, &resolved_path, env);
            Some(DiscoveredAgent {
                id: AgentId::Cursor,
                version: None,
                launcher_path,
                resolved_path,
                owner,
                warnings,
            })
        }
    }
}

fn path_has_cursor_evidence(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if normalized.contains("/.cursor/bin/")
        || normalized.contains("/cursor.app/contents/resources/app/bin/")
        || normalized.contains("/programs/cursor/resources/app/bin/")
        || normalized.contains("/cursor-agent/")
    {
        return true;
    }
    if let Some(parent) = path.parent() {
        let marker = parent.join(".cursor-agent");
        if marker.is_file() {
            return true;
        }
        for candidate_dir in [Some(parent), parent.parent()].into_iter().flatten() {
            let pkg_json = candidate_dir.join("package.json");
            if let Ok(contents) = read_discovery_metadata(pkg_json)
                && let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents)
                && let Some(name) = val.get("name").and_then(|n| n.as_str())
                && matches!(
                    name.to_ascii_lowercase().as_str(),
                    "@cursor/agent" | "@anysphere/cursor-agent" | "cursor-agent"
                )
            {
                return true;
            }
        }
    }
    false
}

fn contains_cursor_branding(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("anysphere")
        || lower.contains("cursor.com")
        || lower.contains("cursor agent")
        || lower.contains("cursor-agent")
        || lower.contains("cursor cli")
        || lower.lines().any(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("cursor ")
                .is_some_and(|rest| is_version_or_build_token(rest.trim()))
        })
}

fn has_conflicting_non_cursor_product(text: &str) -> bool {
    for raw_token in text.split_whitespace() {
        if is_version_or_build_token(raw_token) {
            continue;
        }
        for word in raw_token
            .split(|c: char| !c.is_ascii_alphabetic())
            .filter(|w| !w.is_empty())
        {
            let lower = word.to_ascii_lowercase();
            if !matches!(
                lower.as_str(),
                "cursor"
                    | "anysphere"
                    | "agent"
                    | "cli"
                    | "version"
                    | "v"
                    | "release"
                    | "build"
                    | "stable"
                    | "beta"
                    | "alpha"
                    | "rc"
                    | "nightly"
                    | "canary"
                    | "preview"
            ) {
                return true;
            }
        }
    }
    false
}

fn is_version_or_build_token(raw_token: &str) -> bool {
    let trimmed = raw_token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if trimmed.is_empty() {
        return true;
    }
    let without_v = trimmed
        .strip_prefix("rust-v")
        .or_else(|| trimmed.strip_prefix('v'))
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    if is_valid_version_token(without_v) {
        return true;
    }
    let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
    has_digit
        && trimmed
            .chars()
            .all(|c| c.is_ascii_hexdigit() || matches!(c, '.' | '-' | '+' | '_'))
}

#[derive(Debug, Clone, Default)]
struct QoderTargetOrigin {
    entry_root: Option<PathBuf>,
    from_qoder_cli_bin: bool,
}

#[derive(Debug, Clone)]
struct DispatcherTarget {
    launcher_path: PathBuf,
    resolved_path: PathBuf,
    from_qoder_cli_bin: bool,
}

enum QoderEntryStatus {
    NotDispatcher,
    Invalid,
    Verified { entry_root: PathBuf },
}

fn qoder_entry_root(path: &Path) -> Option<PathBuf> {
    let entry_dir = path.parent()?;
    let dir_name = entry_dir.file_name()?.to_str()?;
    if !dir_name.eq_ignore_ascii_case("entry") {
        return None;
    }
    let entry_root = entry_dir.parent()?;
    let root_name = entry_root.file_name()?.to_string_lossy();
    if !root_name.to_ascii_lowercase().contains("qoder") {
        return None;
    }
    Some(entry_root.to_path_buf())
}

fn is_qoder_entry_dir_executable(path: &Path) -> bool {
    qoder_entry_root(path).is_some()
}

fn read_qoder_dispatcher_text(path: &Path) -> Option<String> {
    read_text_wrapper_sample(path).filter(|s| !s.contains('\u{fffd}'))
}

fn path_is_qoder_entry_location(path: &Path, is_windows: bool) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name_matches = if is_windows {
        name.eq_ignore_ascii_case("qoder.cmd") || name.eq_ignore_ascii_case("qoder")
    } else {
        name == "qoder"
    };
    name_matches && qoder_entry_root(path).is_some()
}

fn first_posix_command_word(stmt: &str) -> &str {
    let mut rest = stmt.trim_start();
    loop {
        let Some(token) = rest.split_whitespace().next() else {
            return "";
        };
        let is_plain_env_assign = token.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && !token.contains("$(")
                && !token.contains('`')
        });
        if is_plain_env_assign && rest.len() > token.len() {
            rest = rest[token.len()..].trim_start();
            continue;
        }
        return token;
    }
}

fn normalize_posix_statement(raw: &str) -> Option<String> {
    let collapsed = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
        .replace('\\', "/");
    let mut rest = collapsed.as_str();
    while let Some(stripped) = ["if ", "then ", "elif ", "else ", "do ", "! ", "{ "]
        .iter()
        .find_map(|prefix| rest.strip_prefix(prefix))
    {
        rest = stripped.trim_start();
    }
    if rest.is_empty() {
        return None;
    }
    if matches!(first_posix_command_word(rest), "echo" | "printf") {
        return None;
    }
    Some(rest.to_string())
}

fn extract_posix_executable_statements(content: &str) -> Vec<String> {
    let trimmed = content.trim_start_matches('\u{feff}');
    let chars: Vec<char> = trimmed.chars().collect();
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_double = false;
    let mut in_single = false;
    let mut escape = false;
    let mut paren_depth: usize = 0;
    let mut prev_ws_or_sep = true;
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if escape {
            escape = false;
            if (!in_double || ch != '\n') && !in_single {
                current.push(ch);
            }
            prev_ws_or_sep = false;
            i += 1;
            continue;
        }
        if in_single {
            // Single quotes suppress command/variable expansion in POSIX sh.
            if ch == '\'' {
                in_single = false;
            }
            prev_ws_or_sep = false;
            i += 1;
            continue;
        }
        if in_double {
            match ch {
                '\\' => {
                    escape = true;
                    current.push(ch);
                }
                '"' => {
                    in_double = false;
                    current.push(ch);
                }
                _ => current.push(ch),
            }
            prev_ws_or_sep = false;
            i += 1;
            continue;
        }

        match ch {
            '\\' => {
                escape = true;
                current.push(ch);
                prev_ws_or_sep = false;
            }
            '\'' => {
                in_single = true;
                prev_ws_or_sep = false;
            }
            '"' => {
                in_double = true;
                current.push(ch);
                prev_ws_or_sep = false;
            }
            '#' if prev_ws_or_sep => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                if i < chars.len() && paren_depth == 0 {
                    if let Some(stmt) = normalize_posix_statement(&current) {
                        statements.push(stmt);
                    }
                    current.clear();
                } else {
                    current.push(' ');
                }
                prev_ws_or_sep = true;
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
                prev_ws_or_sep = true;
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
                prev_ws_or_sep = false;
            }
            '\n' | ';' if paren_depth == 0 => {
                if let Some(stmt) = normalize_posix_statement(&current) {
                    statements.push(stmt);
                }
                current.clear();
                prev_ws_or_sep = true;
            }
            '&' if paren_depth == 0 && i + 1 < chars.len() && chars[i + 1] == '&' => {
                if let Some(stmt) = normalize_posix_statement(&current) {
                    statements.push(stmt);
                }
                current.clear();
                prev_ws_or_sep = true;
                i += 1;
            }
            '|' if paren_depth == 0 && i + 1 < chars.len() && chars[i + 1] == '|' => {
                if let Some(stmt) = normalize_posix_statement(&current) {
                    statements.push(stmt);
                }
                current.clear();
                prev_ws_or_sep = true;
                i += 1;
            }
            c if c.is_ascii_whitespace() => {
                current.push(' ');
                prev_ws_or_sep = true;
            }
            _ => {
                current.push(ch);
                prev_ws_or_sep = matches!(ch, ';' | '(' | '&' | '|');
            }
        }
        i += 1;
    }

    if let Some(stmt) = normalize_posix_statement(&current) {
        statements.push(stmt);
    }
    statements
}

fn unix_statement_has_var_exec(stmt: &str) -> bool {
    let Some(after_exec) = stmt.strip_prefix("exec ") else {
        return false;
    };
    let Some(var_pos) = after_exec.find("\"$") else {
        return false;
    };
    after_exec[var_pos + 2..].contains("\"$@\"")
}

fn verify_unix_qoder_dispatcher(script_path: &Path) -> bool {
    let Some(content) = read_qoder_dispatcher_text(script_path) else {
        return false;
    };
    if !content.trim_start_matches('\u{feff}').starts_with("#!") {
        return false;
    }
    if extract_verified_wrapper_exec_target(script_path).is_some() {
        return false;
    }
    let statements = extract_posix_executable_statements(&content);
    let has_lookup = statements.iter().any(|stmt| {
        stmt.contains("command -v qodercli")
            || stmt.contains("type -p qodercli")
            || stmt.contains("/.local/bin/qodercli")
            || stmt.contains("/.qoder/bin/qodercli")
            || stmt.contains("/.qoder-cn/bin/qodercli")
    });
    let has_var_exec = statements
        .iter()
        .any(|stmt| unix_statement_has_var_exec(stmt));
    has_lookup && has_var_exec
}

fn split_cmd_line_segments(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b'&' if !in_quotes => {
                let prev = i.checked_sub(1).map(|idx| bytes[idx]);
                if !matches!(prev, Some(b'>' | b'<')) {
                    segments.push(&line[start..i]);
                    if i + 1 < bytes.len() && bytes[i + 1] == b'&' {
                        i += 1;
                    }
                    start = i + 1;
                }
            }
            b'|' if !in_quotes && i + 1 < bytes.len() && bytes[i + 1] == b'|' => {
                segments.push(&line[start..i]);
                i += 1;
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    segments.push(&line[start..]);
    segments
}

fn batch_keyword_matches(stmt: &str, keyword: &str) -> bool {
    stmt == keyword
        || stmt.strip_prefix(keyword).is_some_and(|rest| {
            rest.chars()
                .next()
                .is_some_and(|c| c.is_ascii_whitespace() || matches!(c, '.' | '/' | ':' | '('))
        })
}

fn tokenize_cmd_segment(stmt: &str) -> Vec<(String, bool)> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = stmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        if chars[i] == '"' {
            i += 1;
            let mut tok = String::new();
            while i < chars.len() && chars[i] != '"' {
                tok.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            tokens.push((tok.to_ascii_lowercase().replace('\\', "/"), true));
        } else {
            let mut tok = String::new();
            while i < chars.len() && !chars[i].is_ascii_whitespace() {
                tok.push(chars[i]);
                i += 1;
            }
            tokens.push((tok.to_ascii_lowercase().replace('\\', "/"), false));
        }
    }
    tokens
}

fn verify_windows_cmd_dispatcher(cmd_text: &str) -> bool {
    let trimmed = cmd_text.trim_start_matches('\u{feff}');
    let mut dispatcher_vars: Vec<String> = Vec::new();

    for raw_line in trimmed.lines() {
        let trimmed_line = raw_line.trim();
        if trimmed_line.is_empty() || trimmed_line.starts_with("::") {
            continue;
        }
        for raw_seg in split_cmd_line_segments(trimmed_line) {
            let seg = raw_seg.trim().trim_start_matches('@').trim_start();
            if seg.is_empty() {
                continue;
            }
            let seg_lower = seg.to_ascii_lowercase().replace('\\', "/");
            if seg_lower.starts_with("::") || batch_keyword_matches(&seg_lower, "rem") {
                break;
            }
            if batch_keyword_matches(&seg_lower, "echo") {
                continue;
            }
            if let Some(rest) = seg_lower
                .strip_prefix("set ")
                .or_else(|| seg_lower.strip_prefix("set\t"))
            {
                let assign = rest.trim().trim_matches('"');
                if let Some((var, val)) = assign.split_once('=') {
                    let var_name = var.trim();
                    let val_norm = val.trim().trim_matches('"');
                    if !var_name.is_empty()
                        && matches!(
                            val_norm,
                            "%~dp0qoder-dispatcher.ps1" | "%~dp0/qoder-dispatcher.ps1"
                        )
                    {
                        dispatcher_vars.push(format!("%{var_name}%"));
                    }
                }
                continue;
            }

            let tokens = tokenize_cmd_segment(seg);
            let mut idx = 0;
            if tokens
                .get(idx)
                .is_some_and(|(tok, quoted)| !quoted && tok == "call")
            {
                idx += 1;
            }
            let Some((exe_tok, _)) = tokens.get(idx) else {
                continue;
            };
            let exe_name = exe_tok.rsplit('/').next().unwrap_or(exe_tok.as_str());
            if !matches!(
                exe_name,
                "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
            ) {
                continue;
            }
            let args = &tokens[idx + 1..];
            let Some(file_flag_idx) = args
                .iter()
                .position(|(tok, quoted)| !quoted && tok == "-file")
            else {
                continue;
            };
            let Some((target_arg, _)) = args.get(file_flag_idx + 1) else {
                continue;
            };
            let targets_dispatcher = matches!(
                target_arg.as_str(),
                "%~dp0qoder-dispatcher.ps1" | "%~dp0/qoder-dispatcher.ps1"
            ) || dispatcher_vars.iter().any(|var| var == target_arg);
            if targets_dispatcher && args[file_flag_idx + 2..].iter().any(|(tok, _)| tok == "%*") {
                return true;
            }
        }
    }

    false
}

fn normalize_powershell_statement(raw: &str) -> Option<String> {
    let collapsed = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
        .replace('\\', "/");
    if collapsed.is_empty() {
        return None;
    }
    let first_word = collapsed.split_whitespace().next().unwrap_or("");
    if matches!(
        first_word,
        "write-host"
            | "write-output"
            | "write-debug"
            | "write-verbose"
            | "write-warning"
            | "write-information"
            | "echo"
    ) {
        return None;
    }
    Some(collapsed)
}

fn extract_powershell_executable_statements(content: &str) -> Vec<String> {
    let trimmed = content.trim_start_matches('\u{feff}');
    let chars: Vec<char> = trimmed.chars().collect();
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_block_comment = false;
    let mut backtick = false;
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if in_block_comment {
            if ch == '#' && i + 1 < chars.len() && chars[i + 1] == '>' {
                in_block_comment = false;
                current.push(' ');
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_single {
            current.push(ch);
            if ch == '\'' {
                if i + 1 < chars.len() && chars[i + 1] == '\'' {
                    current.push('\'');
                    i += 2;
                    continue;
                }
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            current.push(ch);
            if backtick {
                backtick = false;
            } else if ch == '`' {
                backtick = true;
            } else if ch == '"' {
                in_double = false;
            }
            i += 1;
            continue;
        }

        if ch == '<' && i + 1 < chars.len() && chars[i + 1] == '#' {
            in_block_comment = true;
            i += 2;
            continue;
        }
        if ch == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            if let Some(stmt) = normalize_powershell_statement(&current) {
                statements.push(stmt);
            }
            current.clear();
            i += 1;
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                current.push(ch);
            }
            '"' => {
                in_double = true;
                current.push(ch);
            }
            '\n' | '\r' | ';' | '{' | '}' => {
                if let Some(stmt) = normalize_powershell_statement(&current) {
                    statements.push(stmt);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
        i += 1;
    }

    if let Some(stmt) = normalize_powershell_statement(&current) {
        statements.push(stmt);
    }
    statements
}

fn verify_windows_ps1_dispatcher(ps_text: &str) -> bool {
    let statements = extract_powershell_executable_statements(ps_text);
    let has_lookup = statements.iter().any(|stmt| {
        stmt.contains("$env:qoder_cli_bin")
            || stmt.contains("$env:qodercn_cli_bin")
            || stmt.contains("get-command qodercli")
            || stmt.contains("get-command \"qodercli\"")
            || stmt.contains("get-command 'qodercli'")
            || stmt.contains(".qoder/bin/qodercli/qodercli.exe")
            || stmt.contains(".qoder-cn/bin/qodercli/qodercli.exe")
    });
    let has_var_call = statements.iter().any(|stmt| {
        let starts_with_var_call = stmt.starts_with("& $") || stmt.starts_with("& \"$");
        starts_with_var_call
            && stmt
                .split_whitespace()
                .any(|tok| matches!(tok, "@args" | "@argv" | "$args"))
    });
    has_lookup && has_var_call
}

fn verify_windows_qoder_dispatcher(cmd_path: &Path) -> bool {
    let has_cmd_ext = cmd_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd"));
    if !has_cmd_ext {
        return false;
    }
    let Some(cmd_text) = read_qoder_dispatcher_text(cmd_path) else {
        return false;
    };
    if !verify_windows_cmd_dispatcher(&cmd_text) {
        return false;
    }
    let Some(entry_dir) = cmd_path.parent() else {
        return false;
    };
    let ps1_path = entry_dir.join("qoder-dispatcher.ps1");
    let ps1_resolved = canonicalize_clean(&ps1_path);
    if path_has_qoder_desktop_bundle(&ps1_path, &ps1_resolved) {
        return false;
    }
    let Some(ps_text) = read_qoder_dispatcher_text(&ps1_path) else {
        return false;
    };
    verify_windows_ps1_dispatcher(&ps_text)
}

fn is_compiled_native_binary(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut buf = Vec::with_capacity(4);
    if file.take(4).read_to_end(&mut buf).is_err() {
        return false;
    }
    if buf.len() >= 2 && &buf[..2] == b"MZ" {
        return true;
    }
    buf.len() == 4
        && matches!(
            u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
            0x7F45_4C46 // ELF
                | 0xFEED_FACE // Mach-O 32-bit BE
                | 0xCEFA_EDFE // Mach-O 32-bit LE
                | 0xFEED_FACF // Mach-O 64-bit BE
                | 0xCFFA_EDFE // Mach-O 64-bit LE
                | 0xCAFE_BABE // Mach-O Universal BE
                | 0xBEBA_FECA // Mach-O Universal LE
                | 0xCAFE_BABF // Mach-O Universal64 BE
                | 0xBFBA_FECA // Mach-O Universal64 LE
        )
}

fn classify_qoder_entry_dispatcher(
    launcher_path: &Path,
    resolved_path: &Path,
    env: &DiscoveryEnv,
) -> QoderEntryStatus {
    let launcher_in_entry = path_is_qoder_entry_location(launcher_path, env.is_windows);
    let resolved_in_entry = path_is_qoder_entry_location(resolved_path, env.is_windows);
    if !launcher_in_entry && !resolved_in_entry {
        return QoderEntryStatus::NotDispatcher;
    }

    let check_path = if resolved_in_entry {
        resolved_path
    } else {
        launcher_path
    };
    let Some(entry_root) =
        qoder_entry_root(launcher_path).or_else(|| qoder_entry_root(resolved_path))
    else {
        return QoderEntryStatus::NotDispatcher;
    };

    let verified = if env.is_windows {
        verify_windows_qoder_dispatcher(check_path)
    } else {
        verify_unix_qoder_dispatcher(check_path)
    };
    if verified {
        return QoderEntryStatus::Verified { entry_root };
    }

    // If `.qoder*/entry/qoder` is a direct file (or symlink inside entry) that fails bounded
    // dispatcher verification, fail closed so oversized or unverified scripts are never executed.
    if !resolved_in_entry
        && launcher_path != resolved_path
        && is_compiled_native_binary(resolved_path)
    {
        // Direct symlink outside `entry/` pointing to a compiled binary (e.g. Mach-O/ELF/PE).
        return QoderEntryStatus::NotDispatcher;
    }
    QoderEntryStatus::Invalid
}

fn resolve_qodercli_on_path(env: &DiscoveryEnv) -> Option<(PathBuf, PathBuf)> {
    resolve_executable_on_path_filtering("qodercli", env, |launcher, resolved| {
        !path_is_qoder_entry_location(launcher, env.is_windows)
            && !path_is_qoder_entry_location(resolved, env.is_windows)
            && !is_qoder_entry_dir_executable(launcher)
            && !is_qoder_entry_dir_executable(resolved)
    })
    .map(|(_, l, r)| (l, r))
}

fn resolve_qoder_dispatcher_target(
    entry_root: &Path,
    env: &DiscoveryEnv,
) -> Option<DispatcherTarget> {
    if env.is_windows {
        if let Some(override_path) = &env.qoder_cli_bin
            && !path_is_qoder_entry_location(override_path, true)
            && !is_qoder_entry_dir_executable(override_path)
            && is_runnable_file(override_path, true)
        {
            return Some(DispatcherTarget {
                launcher_path: override_path.clone(),
                resolved_path: canonicalize_clean(override_path),
                from_qoder_cli_bin: true,
            });
        }
        if let Some((launcher_path, resolved_path)) = resolve_qodercli_on_path(env) {
            return Some(DispatcherTarget {
                launcher_path,
                resolved_path,
                from_qoder_cli_bin: false,
            });
        }
        let mut fallback_candidates =
            vec![entry_root.join("bin").join("qodercli").join("qodercli.exe")];
        if let Some(home) = &env.home_dir {
            let home_cand = home
                .join(".qoder")
                .join("bin")
                .join("qodercli")
                .join("qodercli.exe");
            if !fallback_candidates.contains(&home_cand) {
                fallback_candidates.push(home_cand);
            }
        }
        for cand in fallback_candidates {
            if is_runnable_file(&cand, true) {
                return Some(DispatcherTarget {
                    resolved_path: canonicalize_clean(&cand),
                    launcher_path: cand,
                    from_qoder_cli_bin: false,
                });
            }
        }
        None
    } else {
        if let Some((launcher_path, resolved_path)) = resolve_qodercli_on_path(env) {
            return Some(DispatcherTarget {
                launcher_path,
                resolved_path,
                from_qoder_cli_bin: false,
            });
        }
        let mut fallback_candidates = Vec::new();
        if let Some(home) = &env.home_dir {
            fallback_candidates.push(home.join(".local").join("bin").join("qodercli"));
        }
        fallback_candidates.push(entry_root.join("bin").join("qodercli").join("qodercli"));
        if let Some(home) = &env.home_dir {
            let home_cand = home
                .join(".qoder")
                .join("bin")
                .join("qodercli")
                .join("qodercli");
            if !fallback_candidates.contains(&home_cand) {
                fallback_candidates.push(home_cand);
            }
        }
        for cand in fallback_candidates {
            if is_runnable_file(&cand, false) {
                return Some(DispatcherTarget {
                    resolved_path: canonicalize_clean(&cand),
                    launcher_path: cand,
                    from_qoder_cli_bin: false,
                });
            }
        }
        None
    }
}

/// Discovers Qoder CLI, preferring `qodercli` on `PATH` and falling back to a verified
/// `qoder` CLI binary while rejecting Qoder Desktop/IDE launchers and unrelated tools.
fn discover_qoder(env: &DiscoveryEnv, runner: &dyn ProcessRunner) -> Option<DiscoveredAgent> {
    if env.is_windows
        && env.qoder_cli_bin.is_some()
        && let Some((entry_idx, entry_launcher, entry_resolved)) =
            resolve_executable_on_path_filtering("qoder", env, |_, _| true)
    {
        let cli_idx = resolve_executable_on_path_filtering("qodercli", env, |_, _| true)
            .map(|(idx, _, _)| idx);
        if cli_idx.is_none_or(|cidx| entry_idx < cidx)
            && matches!(
                classify_qoder_entry_dispatcher(&entry_launcher, &entry_resolved, env),
                QoderEntryStatus::Verified { .. }
            )
            && let Some(verified) =
                try_verify_qoder_candidate("qoder", entry_launcher, entry_resolved, env, runner)
        {
            return Some(verified);
        }
    }

    for &candidate in AgentId::Qoder.cli_candidates() {
        if let Some((launcher_path, resolved_path)) = resolve_executable_on_path(candidate, env)
            && let Some(verified) =
                try_verify_qoder_candidate(candidate, launcher_path, resolved_path, env, runner)
        {
            return Some(verified);
        }
    }
    None
}

#[derive(Debug, Clone)]
struct QoderResolvedTarget {
    is_dedicated_cli_name: bool,
    public_launcher_path: PathBuf,
    public_resolved_path: PathBuf,
    target_launcher_path: PathBuf,
    target_resolved_path: PathBuf,
    origin: QoderTargetOrigin,
}

fn try_verify_qoder_candidate(
    candidate: &str,
    launcher_path: PathBuf,
    resolved_path: PathBuf,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
) -> Option<DiscoveredAgent> {
    if path_has_qoder_desktop_bundle(&launcher_path, &resolved_path) {
        return None;
    }

    match classify_qoder_entry_dispatcher(&launcher_path, &resolved_path, env) {
        QoderEntryStatus::Invalid => None,
        QoderEntryStatus::Verified { entry_root } => {
            let target = resolve_qoder_dispatcher_target(&entry_root, env)?;
            verify_qoder_target_candidate(
                QoderResolvedTarget {
                    is_dedicated_cli_name: true,
                    public_launcher_path: launcher_path,
                    public_resolved_path: target.resolved_path.clone(),
                    target_launcher_path: target.launcher_path,
                    target_resolved_path: target.resolved_path,
                    origin: QoderTargetOrigin {
                        entry_root: Some(entry_root),
                        from_qoder_cli_bin: target.from_qoder_cli_bin,
                    },
                },
                env,
                runner,
            )
        }
        QoderEntryStatus::NotDispatcher => verify_qoder_target_candidate(
            QoderResolvedTarget {
                is_dedicated_cli_name: candidate.eq_ignore_ascii_case("qodercli"),
                public_launcher_path: launcher_path.clone(),
                public_resolved_path: resolved_path.clone(),
                target_launcher_path: launcher_path,
                target_resolved_path: resolved_path,
                origin: QoderTargetOrigin::default(),
            },
            env,
            runner,
        ),
    }
}

fn verify_qoder_target_candidate(
    candidate: QoderResolvedTarget,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
) -> Option<DiscoveredAgent> {
    let QoderResolvedTarget {
        is_dedicated_cli_name,
        public_launcher_path,
        public_resolved_path,
        target_launcher_path,
        target_resolved_path,
        origin,
    } = candidate;

    let wrapper_targets =
        collect_verified_wrapper_targets(&target_launcher_path, &target_resolved_path);
    if path_has_qoder_desktop_bundle(&target_launcher_path, &target_resolved_path)
        || wrapper_targets
            .iter()
            .any(|(t, rt)| path_has_qoder_desktop_bundle(t, rt))
    {
        return None;
    }

    if origin.from_qoder_cli_bin && !target_launcher_path.is_absolute() {
        let warning = t!(
            "agents.warning.probe_failed",
            name = AgentId::Qoder.display_name(),
            path = target_launcher_path.display(),
            detail = "relative QODER_CLI_BIN override path"
        )
        .to_string();
        return Some(DiscoveredAgent {
            id: AgentId::Qoder,
            version: None,
            owner: UpdateOwner::ManualActionRequired {
                reason: "QODER_CLI_BIN override must be an absolute path".to_string(),
                guidance:
                    "Set QODER_CLI_BIN to a verified absolute qodercli executable path, or unset it."
                        .to_string(),
            },
            launcher_path: public_launcher_path,
            resolved_path: public_resolved_path,
            warnings: vec![warning],
        });
    }

    if env.is_windows
        && (is_unsafe_windows_cmd_shim(&public_launcher_path)
            || is_unsafe_windows_cmd_shim(&target_launcher_path))
    {
        let npm_entry_verified = detect_npm_ownership(
            &target_launcher_path,
            &target_resolved_path,
            None,
            "@qoder-ai/qodercli",
            env.is_windows,
        )
        .is_some();
        if !is_dedicated_cli_name && !npm_entry_verified {
            return None;
        }
        let unsafe_path = if is_unsafe_windows_cmd_shim(&public_launcher_path) {
            &public_launcher_path
        } else {
            &target_launcher_path
        };
        let warning = t!(
            "agents.warning.probe_failed",
            name = AgentId::Qoder.display_name(),
            path = unsafe_path.display(),
            detail = "unsafe Windows .cmd shim path metacharacters"
        )
        .to_string();
        return Some(DiscoveredAgent {
            id: AgentId::Qoder,
            version: None,
            owner: UpdateOwner::ManualActionRequired {
                reason: "Windows .cmd path contains unsafe shell metacharacters".to_string(),
                guidance:
                    "Reinstall Qoder CLI in a standard directory without cmd.exe metacharacters."
                        .to_string(),
            },
            launcher_path: public_launcher_path,
            resolved_path: public_resolved_path,
            warnings: vec![warning],
        });
    }

    let mut warnings = Vec::new();
    let probe_result = run_bounded_probe(&target_launcher_path, &["--version"], runner);

    match probe_result {
        Ok(version_out) => {
            let version_text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&version_out.stdout),
                String::from_utf8_lossy(&version_out.stderr)
            );
            if has_qoder_desktop_or_ide_signature(&version_text)
                || has_qoder_cli_not_installed_signature(&version_text)
                || version_out.status_code == Some(127)
                || (version_out.success && has_conflicting_non_qoder_product(&version_text))
            {
                return None;
            }

            let version = interpret_version_probe_output(
                AgentId::Qoder,
                &target_launcher_path,
                &version_out,
                &mut warnings,
            );

            let npm_signal = detect_npm_ownership(
                &target_launcher_path,
                &target_resolved_path,
                version.as_deref(),
                "@qoder-ai/qodercli",
                env.is_windows,
            );

            let mut help_text: Option<String> = None;
            let mut help_negated_update = false;
            if npm_signal.is_none()
                && !version_out.timed_out
                && !version_out.stdout_truncated
                && !version_out.stderr_truncated
                && version_out.status_code.is_some()
                && let Ok(help_out) = run_bounded_probe(&target_launcher_path, &["--help"], runner)
            {
                if help_out.status_code == Some(127) {
                    return None;
                }
                if !help_out.timed_out && !help_out.stdout_truncated && !help_out.stderr_truncated {
                    let combined_help = format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&help_out.stdout),
                        String::from_utf8_lossy(&help_out.stderr)
                    );
                    if has_qoder_desktop_or_ide_signature(&combined_help)
                        || has_qoder_cli_not_installed_signature(&combined_help)
                        || help_has_conflicting_usage_program(&combined_help)
                        || help_has_conflicting_product_banner(&combined_help)
                    {
                        return None;
                    }
                    if help_out.success {
                        help_text = Some(combined_help);
                    } else if help_output_negates_update(&combined_help) {
                        help_negated_update = true;
                    }
                }
            }

            let install_resource = detect_qoder_install_resource(
                &target_launcher_path,
                &target_resolved_path,
                &wrapper_targets,
                env.is_windows,
            );
            let has_verified_install_resource = matches!(install_resource, Ok(Some(_)))
                && !has_conflicting_non_qoder_product(&version_text);

            let is_verified_cli = npm_signal.is_some()
                || version_has_qoder_cli_branding(&version_text)
                || help_text
                    .as_deref()
                    .is_some_and(help_has_qoder_cli_evidence)
                || has_verified_install_resource
                || (is_dedicated_cli_name
                    && (!version_out.success
                        || version_text.to_ascii_lowercase().contains("qoder")));

            if !is_verified_cli {
                return None;
            }

            let help_checked = help_text.is_some() || help_negated_update;
            let has_native_update_cmd = help_text
                .as_deref()
                .is_some_and(help_supports_qoder_native_update);
            let owner = determine_qoder_owner_with_origin(
                QoderOwnerInput {
                    launcher_path: &target_launcher_path,
                    resolved_path: &target_resolved_path,
                    wrapper_targets: &wrapper_targets,
                    version: version.as_deref(),
                    npm_signal,
                    install_resource: Some(install_resource),
                    has_native_update_cmd,
                    help_checked,
                    origin: &origin,
                },
                env,
            );

            Some(DiscoveredAgent {
                id: AgentId::Qoder,
                version,
                launcher_path: public_launcher_path,
                resolved_path: public_resolved_path,
                owner,
                warnings,
            })
        }
        Err(err) => {
            let npm_signal = detect_npm_ownership(
                &target_launcher_path,
                &target_resolved_path,
                None,
                "@qoder-ai/qodercli",
                env.is_windows,
            );
            if !is_dedicated_cli_name && npm_signal.is_none() {
                return None;
            }
            warnings.push(
                t!(
                    "agents.warning.probe_failed",
                    name = AgentId::Qoder.display_name(),
                    path = target_launcher_path.display(),
                    detail = err.to_string()
                )
                .to_string(),
            );
            let owner = determine_qoder_owner_with_origin(
                QoderOwnerInput {
                    launcher_path: &target_launcher_path,
                    resolved_path: &target_resolved_path,
                    wrapper_targets: &wrapper_targets,
                    version: None,
                    npm_signal,
                    install_resource: None,
                    has_native_update_cmd: false,
                    help_checked: false,
                    origin: &origin,
                },
                env,
            );
            Some(DiscoveredAgent {
                id: AgentId::Qoder,
                version: None,
                launcher_path: public_launcher_path,
                resolved_path: public_resolved_path,
                owner,
                warnings,
            })
        }
    }
}

fn path_has_qoder_desktop_bundle(launcher_path: &Path, resolved_path: &Path) -> bool {
    for path in [launcher_path, resolved_path] {
        let normalized = path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if normalized.contains("/qoder.app/")
            || normalized.contains("/qoder ide.app/")
            || normalized.contains("/qoder-ide.app/")
            || normalized.contains("/usr/share/qoder-ide/")
            || normalized.contains("/usr/share/qoder/")
            || normalized.contains("/remote-cli/")
        {
            return true;
        }
    }
    false
}

fn has_qoder_cli_not_installed_signature(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("qoder cli is not installed")
        || lower.contains("qodercli is not installed")
        || lower.contains("qoder-cli is not installed")
        || lower.contains("qoder cli not installed")
}

fn has_qoder_desktop_or_ide_signature(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("qoder desktop")
        || lower.contains("qoder ide")
        || lower.contains("qoder editor")
        || lower.contains("qoder studio")
        || lower.contains("desktop application")
        || lower.contains("electron")
        || lower.contains("--open-window")
        || lower.contains("--new-window")
        || lower.contains("--reuse-window")
}

fn has_conflicting_non_qoder_product(text: &str) -> bool {
    for raw_token in text.split_whitespace() {
        if is_version_or_build_token(raw_token) {
            continue;
        }
        for word in raw_token
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
        {
            if word.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let lower = word.to_ascii_lowercase();
            if !matches!(
                lower.as_str(),
                "qoder"
                    | "qodercli"
                    | "ai"
                    | "cli"
                    | "agent"
                    | "coding"
                    | "code"
                    | "version"
                    | "v"
                    | "release"
                    | "build"
                    | "commit"
                    | "date"
                    | "darwin"
                    | "linux"
                    | "windows"
                    | "win32"
                    | "win64"
                    | "macos"
                    | "x86"
                    | "x64"
                    | "x86_64"
                    | "arm64"
                    | "aarch64"
                    | "amd64"
                    | "stable"
                    | "beta"
                    | "alpha"
                    | "rc"
                    | "nightly"
                    | "canary"
                    | "preview"
                    | "next"
                    | "dev"
            ) {
                return true;
            }
        }
    }
    false
}

fn version_has_qoder_cli_branding(text: &str) -> bool {
    if has_qoder_cli_not_installed_signature(text) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    lower.contains("qodercli")
        || lower.contains("qoder cli")
        || lower.contains("qoder-cli")
        || lower.contains("@qoder-ai/qodercli")
}

fn is_help_section_break(lower: &str) -> bool {
    lower.starts_with("commands:")
        || lower.starts_with("available commands:")
        || lower.starts_with("subcommands:")
        || lower.starts_with("options:")
        || lower.starts_with("global options:")
        || lower.starts_with("flags:")
        || lower.starts_with("global flags:")
        || lower.starts_with("arguments:")
        || lower.starts_with("positionals:")
        || lower.starts_with("examples:")
        || lower.starts_with("environment:")
        || lower.starts_with("aliases:")
        || lower.starts_with("further help:")
        || lower.starts_with("learn more:")
}

fn extract_usage_program_stem(synopsis: &str) -> Option<&str> {
    let synopsis = strip_ascii_prefix_case_insensitive(synopsis.trim(), "or:")
        .map(str::trim)
        .unwrap_or_else(|| synopsis.trim());
    let first_token = synopsis.split_whitespace().next()?;
    if first_token.starts_with('[')
        || first_token.starts_with('<')
        || first_token.starts_with('(')
        || first_token.starts_with('-')
    {
        return None;
    }
    let base = first_token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(first_token);
    let cleaned =
        base.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.');
    let stem = Path::new(cleaned)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(cleaned);
    if stem.is_empty() { None } else { Some(stem) }
}

fn is_usage_synopsis_line(trimmed: &str) -> bool {
    let synopsis = strip_ascii_prefix_case_insensitive(trimmed, "or:")
        .map(str::trim)
        .unwrap_or(trimmed);
    if synopsis.is_empty()
        || synopsis.ends_with(['!', '?', ';', ','])
        || (synopsis.ends_with('.') && !synopsis.ends_with("..."))
        || [" - ", " — ", " – ", ": "]
            .into_iter()
            .any(|sep| synopsis.contains(sep))
    {
        return false;
    }
    let mut tokens = synopsis.split_whitespace();
    let Some(first_token) = tokens.next() else {
        return false;
    };
    if first_token.starts_with(['[', '<', '(', '{', '-']) {
        return true;
    }
    if first_token.ends_with(':') {
        return false;
    }
    let Some(stem) = extract_usage_program_stem(synopsis) else {
        return false;
    };
    let rest: Vec<&str> = tokens.collect();
    if rest.is_empty() {
        return true;
    }
    if rest.iter().any(|tok| {
        tok.starts_with('-')
            || tok.contains(['[', ']', '<', '>', '(', ')', '{', '}', '|', '=', '.'])
    }) {
        return true;
    }
    !matches!(stem.to_ascii_lowercase().as_str(), "a" | "an" | "the")
        && rest.iter().all(|tok| {
            tok.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | ':'))
        })
}

#[derive(Clone, Copy)]
enum HelpLine<'a> {
    UsageSynopsis(&'a str),
    Preamble(&'a str),
    PreambleBreak,
    Body(&'a str),
}

fn parse_help_lines(help_text: &str) -> Vec<HelpLine<'_>> {
    let mut out = Vec::new();
    let mut in_usage_block = false;
    let mut usage_saw_synopsis = false;
    let mut past_section_break = false;

    for raw_line in help_text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            if in_usage_block && usage_saw_synopsis {
                in_usage_block = false;
            }
            if !past_section_break
                && !in_usage_block
                && matches!(out.last(), Some(HelpLine::Preamble(_)))
            {
                out.push(HelpLine::PreambleBreak);
            }
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if is_help_section_break(&lower) {
            in_usage_block = false;
            past_section_break = true;
            continue;
        }
        if let Some(rest) = strip_ascii_prefix_case_insensitive(trimmed, "usage:") {
            in_usage_block = true;
            let rest_trimmed = rest.trim();
            if !rest_trimmed.is_empty() {
                usage_saw_synopsis = true;
                out.push(HelpLine::UsageSynopsis(rest_trimmed));
            } else {
                usage_saw_synopsis = false;
            }
            continue;
        }
        if in_usage_block {
            let is_indented = raw_line.starts_with(char::is_whitespace);
            let is_or_line = strip_ascii_prefix_case_insensitive(trimmed, "or:").is_some();
            if (!usage_saw_synopsis || is_indented || is_or_line) && is_usage_synopsis_line(trimmed)
            {
                usage_saw_synopsis = true;
                out.push(HelpLine::UsageSynopsis(trimmed));
                continue;
            }
            in_usage_block = false;
        }
        if !past_section_break {
            out.push(HelpLine::Preamble(trimmed));
        } else {
            out.push(HelpLine::Body(trimmed));
        }
    }
    out
}

fn is_banner_head_word(lower_word: &str) -> bool {
    matches!(lower_word, "qoder" | "qodercli" | "cli" | "agent")
}

fn is_generic_description_noun_word(lower_word: &str) -> bool {
    matches!(
        lower_word,
        "a" | "an"
            | "the"
            | "ai"
            | "coding"
            | "code"
            | "cli"
            | "agent"
            | "assistant"
            | "terminal"
            | "interactive"
            | "official"
            | "local"
            | "modern"
            | "autonomous"
            | "interface"
            | "utility"
            | "qoder"
            | "qodercli"
    )
}

fn is_phrase_boundary_word(lower_word: &str) -> bool {
    matches!(
        lower_word,
        "a" | "an"
            | "the"
            | "is"
            | "are"
            | "was"
            | "were"
            | "be"
            | "for"
            | "in"
            | "on"
            | "at"
            | "to"
            | "from"
            | "with"
            | "without"
            | "by"
            | "of"
            | "into"
            | "as"
            | "and"
            | "or"
            | "that"
            | "which"
            | "your"
            | "our"
            | "my"
            | "their"
            | "its"
            | "this"
            | "these"
            | "those"
    )
}

fn is_conflicting_banner_segment(segment: &str) -> bool {
    let trimmed = segment.trim();
    let lower = trimmed.to_ascii_lowercase();
    !lower.contains("qoder") || has_conflicting_non_qoder_product(trimmed)
}

fn description_has_conflicting_brand(desc: &str) -> bool {
    for clause in desc.split(['.', '!', '?', ';', ',', ':', '(', ')']) {
        let words: Vec<&str> = clause
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()))
            .collect();
        for (idx, word) in words.iter().enumerate() {
            let lower = word.to_ascii_lowercase();
            if is_banner_head_word(&lower) {
                let mut j = idx;
                while j > 0 {
                    let prev = words[j - 1];
                    let prev_lower = prev.to_ascii_lowercase();
                    if is_phrase_boundary_word(&prev_lower) {
                        break;
                    }
                    if has_conflicting_non_qoder_product(prev)
                        && !is_generic_description_noun_word(&prev_lower)
                    {
                        return true;
                    }
                    j -= 1;
                }
            }
        }
    }
    false
}

fn is_unbranded_description_line(trimmed: &str) -> bool {
    let words: Vec<&str> = trimmed
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()))
        .collect();
    let Some(&first) = words.first() else {
        return false;
    };
    if !matches!(first.to_ascii_lowercase().as_str(), "a" | "an" | "the") {
        return false;
    }
    let Some(head_idx) = words.iter().position(|w| {
        matches!(
            w.to_ascii_lowercase().as_str(),
            "assistant" | "agent" | "cli" | "interface" | "utility"
        )
    }) else {
        return false;
    };
    head_idx >= 1
        && words[..=head_idx]
            .iter()
            .all(|w| is_generic_description_noun_word(&w.to_ascii_lowercase()))
        && !description_has_conflicting_brand(trimmed)
}

fn ends_with_sentence_punctuation(trimmed: &str) -> bool {
    if trimmed.ends_with(['!', '?']) {
        return true;
    }
    if trimmed.ends_with('.') && !trimmed.ends_with("...") {
        return trimmed.split_whitespace().last().is_some_and(|tok| {
            let core = tok.trim_end_matches('.');
            !core.is_empty()
                && !is_version_or_build_token(core)
                && core.chars().all(|c| c.is_ascii_alphabetic())
        });
    }
    false
}

fn split_sentence_banner_and_description(trimmed: &str) -> (&str, Option<&str>) {
    let mut tokens = Vec::new();
    let mut byte_idx = 0;
    for raw_tok in trimmed.split_whitespace() {
        let start = trimmed[byte_idx..]
            .find(raw_tok)
            .map(|off| byte_idx + off)
            .unwrap_or(byte_idx);
        let end = start + raw_tok.len();
        byte_idx = end;
        tokens.push((raw_tok, end));
    }
    let token_has_banner_head = |raw_tok: &str| {
        raw_tok
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|w| is_banner_head_word(&w.to_ascii_lowercase()))
    };
    let Some(first_head_idx) = tokens
        .iter()
        .position(|(tok, _)| token_has_banner_head(tok))
    else {
        return (trimmed, None);
    };
    let mut last_banner_idx = first_head_idx;
    for (idx, (tok, _)) in tokens.iter().enumerate().skip(first_head_idx + 1) {
        if !has_conflicting_non_qoder_product(tok) || token_has_banner_head(tok) {
            last_banner_idx = idx;
        } else if idx + 1 < tokens.len() && token_has_banner_head(tokens[idx + 1].0) {
            last_banner_idx = idx + 1;
        } else {
            break;
        }
    }
    let banner_end = tokens[last_banner_idx].1;
    let banner = trimmed[..banner_end].trim();
    let rest = trimmed[banner_end..].trim();
    if rest.is_empty() {
        (banner, None)
    } else {
        (banner, Some(rest))
    }
}

fn is_exempt_preamble_line(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("error:")
        || lower.starts_with("fatal:")
        || lower.starts_with("warning:")
        || lower.starts_with("note:")
        || lower.starts_with('-')
        || lower.starts_with('*')
        || line_negates_or_rejects_update(&lower)
}

fn split_banner_separator(trimmed: &str) -> Option<(&str, &str)> {
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("http://") || lower.contains("https://") {
        return None;
    }
    [" - ", " — ", " – ", ": "]
        .into_iter()
        .find_map(|sep| trimmed.split_once(sep))
        .map(|(left, right)| (left.trim(), right.trim()))
        .filter(|(banner, _)| !banner.is_empty())
}

fn preamble_line_has_conflicting_banner(trimmed: &str) -> bool {
    if is_exempt_preamble_line(trimmed) {
        return false;
    }
    if let Some((banner, description)) = split_banner_separator(trimmed) {
        return is_conflicting_banner_segment(banner)
            || description_has_conflicting_brand(description);
    }
    if is_unbranded_description_line(trimmed) {
        return false;
    }
    if ends_with_sentence_punctuation(trimmed) {
        let (banner, description) = split_sentence_banner_and_description(trimmed);
        return is_conflicting_banner_segment(banner)
            || description.is_some_and(description_has_conflicting_brand);
    }
    is_conflicting_banner_segment(trimmed)
}

fn preamble_paragraph_has_conflicting_banner(paragraph: &[&str]) -> bool {
    if paragraph.len() <= 1 {
        return paragraph
            .first()
            .is_some_and(|line| preamble_line_has_conflicting_banner(line));
    }

    let mut desc_accum: Option<String> = None;
    for (idx, &trimmed) in paragraph.iter().enumerate() {
        if is_exempt_preamble_line(trimmed) {
            continue;
        }
        if let Some(accum) = desc_accum.as_mut() {
            if !accum.is_empty() {
                accum.push(' ');
            }
            accum.push_str(trimmed);
            if description_has_conflicting_brand(trimmed)
                || description_has_conflicting_brand(accum)
            {
                return true;
            }
            continue;
        }
        if let Some((banner, description)) = split_banner_separator(trimmed) {
            if is_conflicting_banner_segment(banner)
                || description_has_conflicting_brand(description)
            {
                return true;
            }
            desc_accum = Some(description.to_string());
            continue;
        }
        if is_unbranded_description_line(trimmed) {
            desc_accum = Some(trimmed.to_string());
            continue;
        }
        if ends_with_sentence_punctuation(trimmed) {
            let (banner, description) = split_sentence_banner_and_description(trimmed);
            if is_conflicting_banner_segment(banner)
                || description.is_some_and(description_has_conflicting_brand)
            {
                return true;
            }
            if let Some(desc) = description {
                desc_accum = Some(desc.to_string());
            }
            continue;
        }
        let (line_banner, line_desc) = split_sentence_banner_and_description(trimmed);
        if line_desc.is_some() && !is_conflicting_banner_segment(line_banner) {
            let joined = paragraph[idx..].join(" ");
            if ends_with_sentence_punctuation(&joined) {
                let (banner, description) = split_sentence_banner_and_description(&joined);
                return is_conflicting_banner_segment(banner)
                    || description.is_some_and(description_has_conflicting_brand);
            }
        }
        if is_conflicting_banner_segment(trimmed) {
            return true;
        }
    }
    false
}

fn help_lines_have_conflicting_usage_program(lines: &[HelpLine<'_>]) -> bool {
    lines.iter().any(|line| match line {
        HelpLine::UsageSynopsis(synopsis) => {
            extract_usage_program_stem(synopsis).is_some_and(|stem| {
                !stem.eq_ignore_ascii_case("qoder") && !stem.eq_ignore_ascii_case("qodercli")
            })
        }
        _ => false,
    })
}

fn help_lines_have_conflicting_product_banner(lines: &[HelpLine<'_>]) -> bool {
    let mut paragraph: Vec<&str> = Vec::new();
    for &line in lines {
        match line {
            HelpLine::Preamble(trimmed) => paragraph.push(trimmed),
            HelpLine::PreambleBreak | HelpLine::UsageSynopsis(_) | HelpLine::Body(_) => {
                if !paragraph.is_empty() && preamble_paragraph_has_conflicting_banner(&paragraph) {
                    return true;
                }
                paragraph.clear();
            }
        }
    }
    !paragraph.is_empty() && preamble_paragraph_has_conflicting_banner(&paragraph)
}

fn help_has_conflicting_usage_program(help_text: &str) -> bool {
    help_lines_have_conflicting_usage_program(&parse_help_lines(help_text))
}

fn help_has_conflicting_product_banner(help_text: &str) -> bool {
    help_lines_have_conflicting_product_banner(&parse_help_lines(help_text))
}

fn help_has_qoder_cli_evidence(help_text: &str) -> bool {
    let parsed_lines = parse_help_lines(help_text);
    if has_qoder_desktop_or_ide_signature(help_text)
        || has_qoder_cli_not_installed_signature(help_text)
        || help_lines_have_conflicting_usage_program(&parsed_lines)
        || help_lines_have_conflicting_product_banner(&parsed_lines)
    {
        return false;
    }
    let lower = help_text.to_ascii_lowercase();
    version_has_qoder_cli_branding(&lower)
        || (lower.contains("qoder")
            && help_lines_support_qoder_native_update(help_text, &parsed_lines))
}

fn line_negates_or_rejects_update(lower_line: &str) -> bool {
    const NEGATION_AND_REJECTION_PATTERNS: &[&str] = &[
        "do not",
        "don't",
        "dont",
        "never",
        "cannot",
        "can't",
        "cant",
        "must not",
        "should not",
        "shouldn't",
        "is not",
        "are not",
        "not supported",
        "unsupported",
        "not available",
        "unavailable",
        "not allowed",
        "disallowed",
        "not permitted",
        "forbidden",
        "prohibited",
        "disabled",
        "deprecated",
        "removed",
        "instead",
        "package manager",
        "use brew",
        "use npm",
        "manually",
        "error",
        "fatal",
        "failed",
        "denied",
        "policy",
    ];
    NEGATION_AND_REJECTION_PATTERNS
        .iter()
        .any(|pat| lower_line.contains(pat))
}

fn is_prose_auxiliary_token(token: &str) -> bool {
    matches!(
        token,
        "is" | "was"
            | "are"
            | "were"
            | "has"
            | "have"
            | "had"
            | "will"
            | "would"
            | "can"
            | "cannot"
            | "could"
            | "should"
            | "must"
            | "may"
            | "might"
            | "failed"
            | "disabled"
    )
}

fn description_targets_unrelated_artifact(lower_desc: &str) -> bool {
    const UNRELATED_TARGETS: &[&str] = &[
        "index",
        "database",
        "schema",
        "cache",
        "submodule",
        "snapshot",
        "document",
    ];
    UNRELATED_TARGETS.iter().any(|t| lower_desc.contains(t))
}

fn help_supports_qoder_native_update(help_text: &str) -> bool {
    let parsed_lines = parse_help_lines(help_text);
    if has_qoder_desktop_or_ide_signature(help_text)
        || has_qoder_cli_not_installed_signature(help_text)
        || help_lines_have_conflicting_usage_program(&parsed_lines)
        || help_lines_have_conflicting_product_banner(&parsed_lines)
    {
        return false;
    }
    help_lines_support_qoder_native_update(help_text, &parsed_lines)
}

fn help_output_negates_update(help_text: &str) -> bool {
    help_text.lines().any(|raw_line| {
        let lower = raw_line.trim().to_ascii_lowercase();
        (lower.contains("update") || lower.contains("upgrade"))
            && line_negates_or_rejects_update(&lower)
    })
}

fn help_lines_support_qoder_native_update(help_text: &str, parsed_lines: &[HelpLine<'_>]) -> bool {
    let full_lower = help_text.to_ascii_lowercase();
    if !full_lower.contains("qoder") {
        return false;
    }
    let has_cli_branding = version_has_qoder_cli_branding(&full_lower);

    if help_output_negates_update(help_text)
        || help_text.lines().any(|raw_line| {
            let lower = raw_line.trim().to_ascii_lowercase();
            lower.starts_with("error:") || lower.starts_with("fatal:")
        })
    {
        return false;
    }

    for &line in parsed_lines {
        match line {
            HelpLine::UsageSynopsis(synopsis) => {
                let syn_clean = strip_ascii_prefix_case_insensitive(synopsis, "or:")
                    .map(str::trim)
                    .unwrap_or(synopsis)
                    .to_ascii_lowercase();
                let tokens: Vec<&str> = syn_clean.split_whitespace().collect();
                if tokens.len() >= 2
                    && matches!(tokens[0], "qodercli" | "qoder")
                    && tokens[1] == "update"
                    && tokens.get(2).is_none_or(|t| !is_prose_auxiliary_token(t))
                {
                    return true;
                }
            }
            HelpLine::Preamble(trimmed) | HelpLine::Body(trimmed) => {
                let lower = trimmed.to_ascii_lowercase();
                let entry = lower
                    .strip_prefix("- ")
                    .or_else(|| lower.strip_prefix("* "))
                    .unwrap_or(&lower)
                    .trim_start();

                for prefix in ["qodercli update", "qoder update"] {
                    if let Some(rest) = entry.strip_prefix(prefix)
                        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
                    {
                        let rest_trimmed = rest.trim();
                        let first_rest = rest_trimmed.split_whitespace().next().unwrap_or("");
                        if !is_prose_auxiliary_token(first_rest)
                            && !description_targets_unrelated_artifact(rest_trimmed)
                        {
                            return true;
                        }
                    }
                }

                let mut tokens = entry.split_whitespace();
                if let Some(first) = tokens.next() {
                    let cleaned = first.trim_matches(|c: char| matches!(c, ',' | ':' | '|'));
                    if cleaned == "update" {
                        let rest_tokens: Vec<&str> = tokens.collect();
                        if rest_tokens
                            .first()
                            .is_some_and(|t| is_prose_auxiliary_token(t))
                        {
                            continue;
                        }
                        let rest_joined = rest_tokens.join(" ");
                        if description_targets_unrelated_artifact(&rest_joined) {
                            continue;
                        }
                        let desc_has_positive_update_target = rest_joined.contains("qoder")
                            || rest_joined.contains("cli")
                            || rest_joined.contains("self")
                            || rest_joined.contains("latest")
                            || rest_joined.contains("version");
                        if has_cli_branding || desc_has_positive_update_target {
                            return true;
                        }
                    }
                }
            }
            HelpLine::PreambleBreak => {}
        }
    }
    false
}

fn collect_verified_wrapper_targets(
    launcher_path: &Path,
    resolved_path: &Path,
) -> Vec<(PathBuf, PathBuf)> {
    let mut out = Vec::new();
    let mut visited = vec![launcher_path.to_path_buf(), resolved_path.to_path_buf()];
    let mut frontier = vec![launcher_path.to_path_buf()];
    if resolved_path != launcher_path {
        frontier.push(resolved_path.to_path_buf());
    }

    for _ in 0..4 {
        let mut next_frontier = Vec::new();
        for current in frontier {
            if let Some(exec_target) = extract_verified_wrapper_exec_target(&current) {
                let resolved_target = canonicalize_clean(&exec_target);
                let pair = (exec_target.clone(), resolved_target.clone());
                if !out.contains(&pair) {
                    out.push(pair);
                }
                for candidate in [exec_target, resolved_target] {
                    if !visited.contains(&candidate) {
                        visited.push(candidate.clone());
                        next_frontier.push(candidate);
                    }
                }
            }
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }
    out
}

fn has_unverified_wrapper_forwarding(launcher_path: &Path, resolved_path: &Path) -> bool {
    for path in [launcher_path, resolved_path] {
        let Some(sample) = read_text_wrapper_sample(path) else {
            continue;
        };
        let lower = sample.to_ascii_lowercase().replace('\\', "/");
        if lower.contains("/cellar/")
            || lower.contains("/caskroom/")
            || lower.contains("node_modules")
            || lower.contains("@qoder-ai/qodercli")
            || lower.contains("/.bun/")
            || lower.contains("/.pnpm/")
            || lower.contains("/.yarn/")
            || lower.contains("/nix/store/")
            || lower.contains("microsoft/winget/")
            || lower.contains("/winget/packages/")
            || lower.contains("/winget/links/")
            || lower.contains("/scoop/apps/")
            || lower.contains("/scoop/shims/")
            || lower.contains("/chocolatey/")
        {
            return true;
        }
        for raw_line in sample.lines() {
            let trimmed = raw_line.trim();
            if is_wrapper_comment_or_empty_line(trimmed) {
                continue;
            }
            if trimmed.contains("\"$@\"")
                || trimmed.contains("\"$*\"")
                || trimmed.contains("%*")
                || trimmed.starts_with("exec ")
                || trimmed.contains(" exec ")
            {
                return true;
            }
        }
    }
    false
}

fn detect_homebrew_in_target_prefix(
    target_path: &Path,
    resolved_target_path: &Path,
    active_version: Option<&str>,
    agent_key: &str,
) -> Vec<ManagerSignal> {
    let mut out = Vec::new();
    for path in [target_path, resolved_target_path] {
        if let Some(bin_dir) = path.parent()
            && bin_dir
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
            && let Some(prefix) = bin_dir.parent()
        {
            let caskroom = prefix.join("Caskroom");
            if caskroom.is_dir() {
                for cask_token in find_matching_casks_in_dir(&caskroom, agent_key, active_version) {
                    let sig = ManagerSignal::HomebrewCask {
                        brew_prefix: prefix.to_path_buf(),
                        cask_token,
                    };
                    if !out.contains(&sig) {
                        out.push(sig);
                    }
                }
            }
            let cellar = prefix.join("Cellar");
            if cellar.is_dir() && cellar.join(agent_key).is_dir() {
                let sig = ManagerSignal::HomebrewFormula {
                    brew_prefix: prefix.to_path_buf(),
                    formula: agent_key.to_string(),
                };
                if !out.contains(&sig) {
                    out.push(sig);
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum QoderInstallResource {
    CurlBash,
    HomebrewCask,
    HomebrewFormula,
    Npm,
}

fn detect_qoder_install_resource(
    launcher_path: &Path,
    resolved_path: &Path,
    wrapper_targets: &[(PathBuf, PathBuf)],
    is_windows: bool,
) -> Result<Option<QoderInstallResource>, String> {
    let mut dirs: Vec<&Path> = Vec::new();
    for path in std::iter::once(launcher_path)
        .chain(std::iter::once(resolved_path))
        .chain(
            wrapper_targets
                .iter()
                .flat_map(|(t, rt)| [t.as_path(), rt.as_path()]),
        )
    {
        if let Some(parent) = path.parent()
            && !dirs
                .iter()
                .any(|existing| paths_refer_to_same_dir(existing, parent, is_windows))
        {
            dirs.push(parent);
        }
    }

    let mut found: Vec<QoderInstallResource> = Vec::new();
    for dir in dirs {
        let marker_path = dir.join(".qodercli-install-resource");
        let sym_meta = match fs::symlink_metadata(&marker_path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                return Err(format!(
                    "unreadable .qodercli-install-resource metadata at {}",
                    marker_path.display()
                ));
            }
        };
        if sym_meta.file_type().is_symlink() || !sym_meta.file_type().is_file() {
            return Err(format!(
                "unreadable, symlinked, or oversized .qodercli-install-resource at {}",
                marker_path.display()
            ));
        }
        let Ok(text) = read_discovery_metadata(&marker_path) else {
            return Err(format!(
                "unreadable, symlinked, or oversized .qodercli-install-resource at {}",
                marker_path.display()
            ));
        };
        if text.contains('\0') {
            return Err(format!(
                "invalid binary content in .qodercli-install-resource at {}",
                marker_path.display()
            ));
        }
        let resource = match text.trim().to_ascii_lowercase().as_str() {
            "curl-bash" => QoderInstallResource::CurlBash,
            "homebrew-cask" => QoderInstallResource::HomebrewCask,
            "homebrew" | "homebrew-formula" => QoderInstallResource::HomebrewFormula,
            "npm" => QoderInstallResource::Npm,
            other => {
                return Err(format!(
                    "unrecognized .qodercli-install-resource marker value '{other}'"
                ));
            }
        };
        if !found.contains(&resource) {
            found.push(resource);
        }
    }

    match found.len() {
        0 => Ok(None),
        1 => Ok(found.into_iter().next()),
        _ => Err("contradictory .qodercli-install-resource markers detected".to_string()),
    }
}

fn is_canonical_qoder_standalone_path(
    path: &Path,
    entry_root: Option<&Path>,
    env: &DiscoveryEnv,
) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if normalized.ends_with("/.qoder/bin/qodercli/qodercli")
        || normalized.ends_with("/.qoder/bin/qodercli/qodercli.exe")
        || normalized.ends_with("/.qoder-cn/bin/qodercli/qodercli")
        || normalized.ends_with("/.qoder-cn/bin/qodercli/qodercli.exe")
    {
        return true;
    }
    if let Some(root) = entry_root {
        for name in ["qodercli", "qodercli.exe"] {
            let expected = root.join("bin").join("qodercli").join(name);
            if paths_refer_to_same_dir(path, &expected, env.is_windows) {
                return true;
            }
        }
    }
    if let Some(home) = &env.home_dir {
        let local_expected = home.join(".local").join("bin").join("qodercli");
        if paths_refer_to_same_dir(path, &local_expected, env.is_windows) {
            return true;
        }
    }
    false
}

fn is_verified_qoder_cli_bin_standalone(
    launcher_path: &Path,
    resolved_path: &Path,
    install_res: Option<&QoderInstallResource>,
    entry_root: Option<&Path>,
    env: &DiscoveryEnv,
) -> bool {
    if !launcher_path.is_absolute() || is_unsafe_windows_cmd_shim(launcher_path) {
        return false;
    }
    let has_cli_stem = launcher_path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("qodercli"));
    if !has_cli_stem {
        return false;
    }
    matches!(install_res, Some(QoderInstallResource::CurlBash))
        || is_canonical_qoder_standalone_path(launcher_path, entry_root, env)
        || is_canonical_qoder_standalone_path(resolved_path, entry_root, env)
}

struct QoderOwnerInput<'a> {
    launcher_path: &'a Path,
    resolved_path: &'a Path,
    wrapper_targets: &'a [(PathBuf, PathBuf)],
    version: Option<&'a str>,
    npm_signal: Option<ManagerSignal>,
    install_resource: Option<Result<Option<QoderInstallResource>, String>>,
    has_native_update_cmd: bool,
    help_checked: bool,
    origin: &'a QoderTargetOrigin,
}

fn determine_qoder_owner_with_origin(
    input: QoderOwnerInput<'_>,
    env: &DiscoveryEnv,
) -> UpdateOwner {
    let QoderOwnerInput {
        launcher_path,
        resolved_path,
        wrapper_targets,
        version,
        npm_signal,
        install_resource,
        has_native_update_cmd,
        help_checked,
        origin,
    } = input;
    let install_resource_res = install_resource.unwrap_or_else(|| {
        detect_qoder_install_resource(
            launcher_path,
            resolved_path,
            wrapper_targets,
            env.is_windows,
        )
    });
    let install_resource = match install_resource_res {
        Ok(res) => res,
        Err(reason) => {
            return UpdateOwner::ManualActionRequired {
                reason,
                guidance: "Update Qoder CLI manually or reinstall via a verified installer or package manager.".to_string(),
            };
        }
    };

    let mut signals = Vec::new();
    if let Some(npm_sig) = npm_signal {
        signals.push(npm_sig);
    }
    for key in ["qodercli", "qoder"] {
        for brew_sig in detect_homebrew_ownership(launcher_path, resolved_path, version, key, env) {
            push_unique_manager_signal(&mut signals, brew_sig, env.is_windows);
        }
        for (target_path, resolved_target_path) in wrapper_targets {
            for brew_sig in
                detect_homebrew_ownership(target_path, resolved_target_path, version, key, env)
            {
                push_unique_manager_signal(&mut signals, brew_sig, env.is_windows);
            }
            for brew_sig in
                detect_homebrew_in_target_prefix(target_path, resolved_target_path, version, key)
            {
                push_unique_manager_signal(&mut signals, brew_sig, env.is_windows);
            }
        }
    }
    if let Some(unsupported) =
        detect_unsupported_foreign_manager(launcher_path, resolved_path, false)
    {
        push_unique_manager_signal(
            &mut signals,
            ManagerSignal::UnsupportedManager {
                manager: unsupported,
            },
            env.is_windows,
        );
    }
    for (target_path, resolved_target_path) in wrapper_targets {
        if let Some(unsupported) =
            detect_unsupported_foreign_manager(target_path, resolved_target_path, false)
        {
            push_unique_manager_signal(
                &mut signals,
                ManagerSignal::UnsupportedManager {
                    manager: unsupported,
                },
                env.is_windows,
            );
        }
    }

    match &install_resource {
        Some(QoderInstallResource::HomebrewCask) => {
            if signals
                .iter()
                .any(|s| !matches!(s, ManagerSignal::HomebrewCask { .. }))
            {
                return UpdateOwner::ManualActionRequired {
                    reason: "contradictory ownership metadata detected between .qodercli-install-resource and detected package managers".to_string(),
                    guidance: "Ensure only one package manager or native installation owns the active Qoder CLI binary.".to_string(),
                };
            }
            if signals.is_empty() {
                signals.push(ManagerSignal::HomebrewCask {
                    brew_prefix: launcher_path
                        .parent()
                        .unwrap_or(launcher_path)
                        .to_path_buf(),
                    cask_token: "qodercli".to_string(),
                });
            }
        }
        Some(QoderInstallResource::HomebrewFormula) => {
            if signals
                .iter()
                .any(|s| !matches!(s, ManagerSignal::HomebrewFormula { .. }))
            {
                return UpdateOwner::ManualActionRequired {
                    reason: "contradictory ownership metadata detected between .qodercli-install-resource and detected package managers".to_string(),
                    guidance: "Ensure only one package manager or native installation owns the active Qoder CLI binary.".to_string(),
                };
            }
            if signals.is_empty() {
                signals.push(ManagerSignal::HomebrewFormula {
                    brew_prefix: launcher_path
                        .parent()
                        .unwrap_or(launcher_path)
                        .to_path_buf(),
                    formula: "qodercli".to_string(),
                });
            }
        }
        Some(QoderInstallResource::Npm) => {
            if !signals
                .iter()
                .any(|s| matches!(s, ManagerSignal::Npm { .. }))
            {
                return UpdateOwner::ManualActionRequired {
                    reason: "Qoder CLI '.qodercli-install-resource' indicates npm, but global '@qoder-ai/qodercli' ownership could not be verified".to_string(),
                    guidance: "Update manually using npm (e.g. 'npm install -g @qoder-ai/qodercli@latest').".to_string(),
                };
            }
        }
        Some(QoderInstallResource::CurlBash) if !signals.is_empty() => {
            return UpdateOwner::ManualActionRequired {
                reason: "contradictory ownership metadata detected between '.qodercli-install-resource' (curl-bash) and detected package managers".to_string(),
                guidance: "Ensure only one installation method owns the active Qoder CLI binary.".to_string(),
            };
        }
        Some(QoderInstallResource::CurlBash) | None => {}
    }

    if has_contradictory_signals(&signals) {
        return UpdateOwner::ManualActionRequired {
            reason: "contradictory ownership metadata detected across multiple installation managers".to_string(),
            guidance: "Ensure only one package manager or native installation owns the active Qoder CLI binary on PATH.".to_string(),
        };
    }

    if let Some(signal) = signals.first() {
        match signal {
            ManagerSignal::UnsupportedManager { manager } => {
                return UpdateOwner::ManualActionRequired {
                    reason: format!(
                        "Qoder CLI installation is managed by unsupported manager '{manager}'"
                    ),
                    guidance: format!(
                        "Update Qoder CLI using {manager} directly, or switch to a verified native or npm '@qoder-ai/qodercli' installation."
                    ),
                };
            }
            ManagerSignal::HomebrewCask { cask_token, .. } => {
                return UpdateOwner::ManualActionRequired {
                    reason: format!(
                        "Qoder CLI is installed via Homebrew cask '{cask_token}', which is not a verified automated update path"
                    ),
                    guidance: format!("Update manually with 'brew upgrade --cask {cask_token}'."),
                };
            }
            ManagerSignal::HomebrewFormula { formula, .. } => {
                return UpdateOwner::ManualActionRequired {
                    reason: format!(
                        "Qoder CLI is installed via Homebrew formula '{formula}', which is not a verified automated update path"
                    ),
                    guidance: format!("Update manually with 'brew upgrade {formula}'."),
                };
            }
            ManagerSignal::Npm { prefix, package } => {
                if is_non_default_channel_version(version)
                    || package_json_has_prerelease(prefix, package)
                {
                    return UpdateOwner::ManualActionRequired {
                        reason: "prerelease npm channel detected; automatic latest upgrade would change release channels".to_string(),
                        guidance: format!(
                            "Run 'npm install -g --prefix {} {}@<channel>' manually to preserve your release channel.",
                            prefix.display(),
                            package
                        ),
                    };
                }
                let Some((npm_bin, _)) = resolve_executable_on_path("npm", env) else {
                    return UpdateOwner::ManualActionRequired {
                        reason: "active Qoder CLI installation is owned by npm, but 'npm' was not found on PATH".to_string(),
                        guidance: format!(
                            "Add npm to PATH and run 'npm install -g --prefix {} {}@latest'.",
                            prefix.display(),
                            package
                        ),
                    };
                };
                if env.is_windows && is_unsafe_windows_cmd_shim(&npm_bin) {
                    return UpdateOwner::ManualActionRequired {
                        reason: "npm shim path on Windows contains unsafe shell metacharacters"
                            .to_string(),
                        guidance: "Ensure npm is installed in a directory path without cmd.exe metacharacters.".to_string(),
                    };
                }
                return UpdateOwner::NpmGlobal {
                    npm_program: npm_bin,
                    prefix: prefix.clone(),
                    package: package.clone(),
                };
            }
            ManagerSignal::WinGet { package_id } => {
                return UpdateOwner::ManualActionRequired {
                    reason: format!(
                        "WinGet package '{package_id}' is not a verified automated Qoder CLI update path"
                    ),
                    guidance: "Update Qoder CLI manually using the method that installed it."
                        .to_string(),
                };
            }
            ManagerSignal::CodexStandalone { .. } => {}
        }
    }

    if has_ambiguous_or_unverified_qoder_npm_evidence(launcher_path, resolved_path, env.is_windows)
        || wrapper_targets
            .iter()
            .any(|(t, rt)| has_ambiguous_or_unverified_qoder_npm_evidence(t, rt, env.is_windows))
    {
        return UpdateOwner::ManualActionRequired {
            reason: "could not verify npm prefix ownership for Qoder CLI launcher".to_string(),
            guidance: "Ensure the active Qoder CLI wrapper and its '@qoder-ai/qodercli' global package reside in the same npm prefix, or update manually.".to_string(),
        };
    }

    if !wrapper_targets.is_empty()
        || has_unverified_wrapper_forwarding(launcher_path, resolved_path)
    {
        return UpdateOwner::ManualActionRequired {
            reason: "could not verify installation ownership for Qoder CLI wrapper target"
                .to_string(),
            guidance: "Update Qoder CLI manually using the package manager or installation method that owns the wrapped target.".to_string(),
        };
    }

    if origin.from_qoder_cli_bin
        && !is_verified_qoder_cli_bin_standalone(
            launcher_path,
            resolved_path,
            install_resource.as_ref(),
            origin.entry_root.as_deref(),
            env,
        )
    {
        return UpdateOwner::ManualActionRequired {
            reason: "QODER_CLI_BIN override points to an unverified Qoder CLI binary".to_string(),
            guidance: "Update Qoder CLI manually using the method that manages the overridden binary, or unset QODER_CLI_BIN.".to_string(),
        };
    }

    if has_native_update_cmd
        || (matches!(install_resource, Some(QoderInstallResource::CurlBash)) && !help_checked)
    {
        let bin_name = launcher_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("qodercli");
        let cmd_label = if origin.entry_root.is_some() || !bin_name.eq_ignore_ascii_case("qoder") {
            "qodercli update"
        } else {
            "qoder update"
        };
        UpdateOwner::NativeSelfUpdate {
            program: launcher_path.to_path_buf(),
            args: vec!["update".to_string()],
            display_command: cmd_label.to_string(),
        }
    } else {
        UpdateOwner::ManualActionRequired {
            reason: "could not verify native self-update interface or proven npm '@qoder-ai/qodercli' ownership for active Qoder CLI binary".to_string(),
            guidance: "Update Qoder CLI manually using your original installation method (for example 'qodercli update' or 'npm install -g @qoder-ai/qodercli@latest').".to_string(),
        }
    }
}

fn has_ambiguous_or_unverified_qoder_npm_evidence(
    launcher_path: &Path,
    resolved_path: &Path,
    is_windows: bool,
) -> bool {
    for path in [launcher_path, resolved_path] {
        let normalized = path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if normalized.contains("/node_modules/") {
            return true;
        }
    }
    if let Some(sample) = read_text_wrapper_sample(launcher_path) {
        let lower = sample.to_ascii_lowercase();
        if lower.contains("node_modules") || lower.contains("@qoder-ai/qodercli") {
            return true;
        }
    }
    if let Some(bin_dir) = launcher_path.parent() {
        let candidate_prefixes = if bin_dir
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
        {
            vec![
                bin_dir.parent().map(Path::to_path_buf),
                Some(bin_dir.to_path_buf()),
            ]
        } else {
            vec![Some(bin_dir.to_path_buf())]
        };
        for prefix in candidate_prefixes.into_iter().flatten() {
            let bases = if is_windows {
                vec![
                    prefix.join("lib").join("node_modules"),
                    prefix.join("node_modules"),
                ]
            } else {
                vec![prefix.join("lib").join("node_modules")]
            };
            for base in bases {
                if base.join("@qoder-ai").join("qodercli").exists() {
                    return true;
                }
            }
        }
    }
    false
}

fn inspect_candidate(
    id: AgentId,
    launcher_path: PathBuf,
    resolved_path: PathBuf,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> DiscoveredAgent {
    let mut warnings = Vec::new();
    let probe_output = if env.is_windows && is_unsafe_windows_cmd_shim(&launcher_path) {
        warnings.push(
            t!(
                "agents.warning.probe_failed",
                name = id.display_name(),
                path = launcher_path.display(),
                detail = "unsafe Windows .cmd shim path metacharacters"
            )
            .to_string(),
        );
        None
    } else {
        match run_bounded_probe(&launcher_path, &["--version"], runner) {
            Ok(out) => Some(out),
            Err(err) => {
                warnings.push(
                    t!(
                        "agents.warning.probe_failed",
                        name = id.display_name(),
                        path = launcher_path.display(),
                        detail = err.to_string()
                    )
                    .to_string(),
                );
                None
            }
        }
    };

    let version = probe_output
        .as_ref()
        .and_then(|out| interpret_version_probe_output(id, &launcher_path, out, &mut warnings));

    let probe_text = probe_output
        .as_ref()
        .map(|out| {
            format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
        .unwrap_or_default();

    let owner = if env.is_windows && is_unsafe_windows_cmd_shim(&launcher_path) {
        UpdateOwner::ManualActionRequired {
            reason: "Windows .cmd path contains unsafe shell metacharacters".to_string(),
            guidance: format!(
                "Reinstall {} in a path without cmd.exe metacharacters (&, |, <, >, ^, %, !).",
                id.display_name()
            ),
        }
    } else {
        match id {
            AgentId::Antigravity => determine_antigravity_owner(&launcher_path, &resolved_path),
            AgentId::OpenCode => determine_opencode_owner(
                &launcher_path,
                &resolved_path,
                version.as_deref(),
                &probe_text,
            ),
            AgentId::Kimi => determine_kimi_owner(
                &launcher_path,
                &resolved_path,
                version.as_deref(),
                &probe_text,
            ),
            AgentId::Cursor => determine_cursor_owner(&launcher_path, &resolved_path, env),
            AgentId::Codex => determine_codex_owner(
                &launcher_path,
                &resolved_path,
                version.as_deref(),
                env,
                runner,
                cache,
            ),
            AgentId::ClaudeCode => determine_claude_owner(
                &launcher_path,
                &resolved_path,
                version.as_deref(),
                env,
                runner,
                cache,
            ),
            AgentId::Qoder => determine_qoder_owner_with_origin(
                QoderOwnerInput {
                    launcher_path: &launcher_path,
                    resolved_path: &resolved_path,
                    wrapper_targets: &collect_verified_wrapper_targets(
                        &launcher_path,
                        &resolved_path,
                    ),
                    version: version.as_deref(),
                    npm_signal: detect_npm_ownership(
                        &launcher_path,
                        &resolved_path,
                        version.as_deref(),
                        "@qoder-ai/qodercli",
                        env.is_windows,
                    ),
                    install_resource: None,
                    has_native_update_cmd: false,
                    help_checked: false,
                    origin: &QoderTargetOrigin::default(),
                },
                env,
            ),
        }
    };

    DiscoveredAgent {
        id,
        version,
        launcher_path,
        resolved_path,
        owner,
        warnings,
    }
}

fn probe_version_with_warnings(
    id: AgentId,
    launcher_path: &Path,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    warnings: &mut Vec<String>,
) -> Option<String> {
    if env.is_windows && is_unsafe_windows_cmd_shim(launcher_path) {
        warnings.push(
            t!(
                "agents.warning.probe_failed",
                name = id.display_name(),
                path = launcher_path.display(),
                detail = "unsafe Windows .cmd shim path metacharacters"
            )
            .to_string(),
        );
        return None;
    }
    match run_bounded_probe(launcher_path, &["--version"], runner) {
        Ok(output) => interpret_version_probe_output(id, launcher_path, &output, warnings),
        Err(err) => {
            warnings.push(
                t!(
                    "agents.warning.probe_failed",
                    name = id.display_name(),
                    path = launcher_path.display(),
                    detail = err.to_string()
                )
                .to_string(),
            );
            None
        }
    }
}

/// Runs a probe command with the 5-second deadline, 64 KiB stream limit, and
/// child-only auto-update suppression environment variables.
pub fn run_bounded_probe(
    program: &Path,
    args: &[&str],
    runner: &dyn ProcessRunner,
) -> Result<ProcessOutput, ProcessError> {
    let request = ProcessRequest {
        program: program.to_path_buf(),
        args: args.iter().map(OsString::from).collect(),
        env_overrides: PROBE_ENV_OVERRIDES
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect(),
        timeout: Some(PROBE_TIMEOUT),
        max_output_bytes: PROBE_MAX_OUTPUT_BYTES,
    };
    request.validate_batch_command()?;
    runner.run_probe(&request)
}

fn interpret_version_probe_output(
    id: AgentId,
    launcher_path: &Path,
    output: &ProcessOutput,
    warnings: &mut Vec<String>,
) -> Option<String> {
    if output.timed_out {
        warnings.push(
            t!(
                "agents.warning.probe_timeout",
                name = id.display_name(),
                path = launcher_path.display(),
                seconds = PROBE_TIMEOUT.as_secs()
            )
            .to_string(),
        );
        return None;
    }

    if output.stdout_truncated || output.stderr_truncated {
        warnings.push(
            t!(
                "agents.warning.probe_oversized",
                name = id.display_name(),
                path = launcher_path.display()
            )
            .to_string(),
        );
        return None;
    }

    if !output.success {
        let code_str = output
            .status_code
            .map(|c| format!("exit code {c}"))
            .unwrap_or_else(|| "terminated by signal".to_string());
        warnings.push(
            t!(
                "agents.warning.probe_failed",
                name = id.display_name(),
                path = launcher_path.display(),
                detail = code_str
            )
            .to_string(),
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(version) = parse_version_text(&stdout).or_else(|| parse_version_text(&stderr)) {
        Some(version)
    } else {
        warnings.push(
            t!(
                "agents.warning.probe_unparseable",
                name = id.display_name(),
                path = launcher_path.display()
            )
            .to_string(),
        );
        None
    }
}

/// Extracts a clean semantic version string from `--version` output.
pub fn parse_version_text(text: &str) -> Option<String> {
    for line in text.lines().take(10) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        for raw_token in trimmed.split(|c: char| {
            c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | ',' | ';' | ':' | '"' | '=')
        }) {
            let trimmed_token = raw_token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            let token = trimmed_token
                .strip_prefix("rust-v")
                .or_else(|| trimmed_token.strip_prefix('v'))
                .or_else(|| trimmed_token.strip_prefix('V'))
                .unwrap_or(trimmed_token);

            if is_valid_version_token(token) {
                return Some(token.to_string());
            }
        }
    }
    None
}

pub(super) fn is_valid_version_token(token: &str) -> bool {
    if token.is_empty() || token.len() > 64 {
        return false;
    }
    let main_and_meta: Vec<&str> = token.splitn(2, ['-', '+']).collect();
    let numbers_part = main_and_meta[0];
    let parts: Vec<&str> = numbers_part.split('.').collect();
    if parts.len() < 2 || parts.len() > 4 {
        return false;
    }
    if !parts
        .iter()
        .all(|p| !p.is_empty() && p.len() <= 8 && p.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    if let Some(suffix) = main_and_meta.get(1)
        && (suffix.is_empty()
            || !suffix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')))
    {
        return false;
    }
    true
}

/// Resolves the first matching executable on `PATH`, returning `(launcher_path, resolved_path)`.
pub fn resolve_executable_on_path(name: &str, env: &DiscoveryEnv) -> Option<(PathBuf, PathBuf)> {
    resolve_executable_on_path_filtering(name, env, |_, _| true).map(|(_, l, r)| (l, r))
}

fn resolve_executable_on_path_filtering<F>(
    name: &str,
    env: &DiscoveryEnv,
    mut allow: F,
) -> Option<(usize, PathBuf, PathBuf)>
where
    F: FnMut(&Path, &Path) -> bool,
{
    for (idx, dir) in split_path_entries(&env.path, env.is_windows)
        .into_iter()
        .enumerate()
    {
        if let Some(launcher_path) = find_executable_in_dir(&dir, name, env) {
            let resolved_path = canonicalize_clean(&launcher_path);
            if allow(&launcher_path, &resolved_path) {
                return Some((idx, launcher_path, resolved_path));
            }
        }
    }
    None
}

/// Splits a `PATH` value into cleaned directory paths, handling quoted segments
/// and cross-platform Windows simulation in unit tests.
pub fn split_path_entries(raw_path: &OsStr, is_windows: bool) -> Vec<PathBuf> {
    if raw_path.is_empty() {
        return Vec::new();
    }

    let raw_lossy = raw_path.to_string_lossy();
    let entries: Vec<PathBuf> = if is_windows && (cfg!(windows) || raw_lossy.contains(';')) {
        split_quoted_path_str(&raw_lossy, ';')
    } else if !is_windows && raw_lossy.contains('"') {
        split_quoted_path_str(&raw_lossy, ':')
    } else {
        std::env::split_paths(raw_path)
            .map(|p| {
                let s = p.to_string_lossy();
                let cleaned = s.trim().trim_matches('"');
                PathBuf::from(cleaned)
            })
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    };

    entries
}

fn split_quoted_path_str(raw: &str, sep: char) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in raw.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == sep && !in_quotes {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                out.push(PathBuf::from(trimmed));
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(PathBuf::from(trimmed));
    }
    out
}

fn find_executable_in_dir(dir: &Path, name: &str, env: &DiscoveryEnv) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }

    if env.is_windows {
        let exts = windows_extensions(env.pathext.as_deref());
        let name_has_ext = Path::new(name).extension().is_some();
        if name_has_ext {
            let candidate = dir.join(name);
            if is_runnable_file(&candidate, true) {
                return Some(candidate);
            }
        }
        for ext in &exts {
            let lower_name = format!("{name}{}", ext.to_ascii_lowercase());
            let lower_candidate = dir.join(&lower_name);
            if is_runnable_file(&lower_candidate, true) {
                return Some(lower_candidate);
            }
            let upper_name = format!("{name}{}", ext.to_ascii_uppercase());
            if upper_name != lower_name {
                let upper_candidate = dir.join(&upper_name);
                if is_runnable_file(&upper_candidate, true) {
                    return Some(upper_candidate);
                }
            }
        }
        None
    } else {
        let candidate = dir.join(name);
        if is_runnable_file(&candidate, false) {
            Some(candidate)
        } else {
            None
        }
    }
}

fn windows_extensions(pathext: Option<&OsStr>) -> Vec<String> {
    let raw = pathext
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD;.PS1".to_string());
    let mut out = Vec::new();
    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('.') {
            out.push(trimmed.to_ascii_uppercase());
        } else {
            out.push(format!(".{}", trimmed.to_ascii_uppercase()));
        }
    }
    if out.is_empty() {
        vec![
            ".COM".to_string(),
            ".EXE".to_string(),
            ".BAT".to_string(),
            ".CMD".to_string(),
        ]
    } else {
        out
    }
}

fn is_runnable_file(path: &Path, is_windows: bool) -> bool {
    // fs::metadata follows symlinks; broken symlinks return Err and are skipped.
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    if is_windows {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Canonicalizes a path and strips Windows verbatim `\\?\` prefixes so path
/// prefix comparisons remain consistent.
pub fn canonicalize_clean(path: &Path) -> PathBuf {
    match fs::canonicalize(path) {
        Ok(canon) => {
            let s = canon.to_string_lossy();
            if let Some(stripped) = s.strip_prefix(r"\\?\UNC\") {
                PathBuf::from(format!(r"\\{stripped}"))
            } else if let Some(stripped) = s.strip_prefix(r"\\?\") {
                PathBuf::from(stripped)
            } else {
                canon
            }
        }
        Err(_) => path.to_path_buf(),
    }
}

/// Checks if a path is a Windows `.cmd` or `.bat` shim containing `cmd.exe`
/// injection metacharacters (`&`, `|`, `<`, `>`, `^`, `%`, `!`, `\r`, `\n`).
pub fn is_unsafe_windows_cmd_shim(path: &Path) -> bool {
    is_unsafe_windows_batch_command(path, &[])
}

/// Batch files reparse arguments through cmd.exe, including `%*` forwarding.
/// Reject expansion/control characters rather than relying on CRT quoting.
pub fn is_unsafe_windows_batch_command(path: &Path, args: &[OsString]) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    if !ext.eq_ignore_ascii_case("cmd") && !ext.eq_ignore_ascii_case("bat") {
        return false;
    }
    std::iter::once(path.as_os_str())
        .chain(args.iter().map(OsString::as_os_str))
        .any(|value| {
            value.as_encoded_bytes().iter().any(|c| {
                matches!(
                    c,
                    b'&' | b'|' | b'<' | b'>' | b'^' | b'%' | b'!' | b'\r' | b'\n' | b'"'
                )
            })
        })
}

// --- Ownership classification ---

#[derive(Debug, Clone, PartialEq, Eq)]
enum ManagerSignal {
    Npm {
        prefix: PathBuf,
        package: String,
    },
    HomebrewCask {
        brew_prefix: PathBuf,
        cask_token: String,
    },
    HomebrewFormula {
        brew_prefix: PathBuf,
        formula: String,
    },
    WinGet {
        package_id: String,
    },
    CodexStandalone {
        bin_dir: PathBuf,
        codex_home: PathBuf,
    },
    UnsupportedManager {
        manager: String,
    },
}

fn determine_antigravity_owner(launcher_path: &Path, resolved_path: &Path) -> UpdateOwner {
    if let Some(unsupported) =
        detect_unsupported_foreign_manager(launcher_path, resolved_path, true)
    {
        return UpdateOwner::ManualActionRequired {
            reason: format!("installation is managed by {unsupported}"),
            guidance: "Update Antigravity using the package manager that installed it, or use a native agy installation.".to_string(),
        };
    }
    UpdateOwner::NativeSelfUpdate {
        program: launcher_path.to_path_buf(),
        args: vec!["update".to_string()],
        display_command: "agy update".to_string(),
    }
}

fn determine_opencode_owner(
    launcher_path: &Path,
    resolved_path: &Path,
    version: Option<&str>,
    probe_text: &str,
) -> UpdateOwner {
    let lower_probe = probe_text.to_ascii_lowercase();
    if lower_probe.contains("legacy")
        || version.is_some_and(|v| v.contains("legacy"))
        || has_adjacent_marker(launcher_path, resolved_path, ".opencode-legacy")
    {
        return UpdateOwner::ManualActionRequired {
            reason: "legacy OpenCode installation requires manual migration".to_string(),
            guidance: "Follow official OpenCode migration instructions for your installation lineage instead of running an automatic cross-generation upgrade.".to_string(),
        };
    }
    if let Some(unsupported) =
        detect_unsupported_foreign_manager(launcher_path, resolved_path, false)
    {
        return UpdateOwner::ManualActionRequired {
            reason: format!("installation is managed by {unsupported}"),
            guidance: "Update OpenCode through the system package manager that owns this path."
                .to_string(),
        };
    }
    UpdateOwner::NativeSelfUpdate {
        program: launcher_path.to_path_buf(),
        args: vec!["upgrade".to_string()],
        display_command: "opencode upgrade".to_string(),
    }
}

fn determine_kimi_owner(
    launcher_path: &Path,
    resolved_path: &Path,
    version: Option<&str>,
    probe_text: &str,
) -> UpdateOwner {
    let lower_probe = probe_text.to_ascii_lowercase();
    if lower_probe.contains("legacy")
        || version.is_some_and(|v| v.contains("legacy"))
        || has_adjacent_marker(launcher_path, resolved_path, ".kimi-legacy")
        || is_legacy_kimi_wrapper(launcher_path, resolved_path)
    {
        return UpdateOwner::ManualActionRequired {
            reason: "legacy Kimi CLI installation does not support non-interactive self-upgrade"
                .to_string(),
            guidance: "Upgrade Kimi manually using the tool that installed it (for example: uv tool upgrade kimi-cli or pip install -U kimi-cli).".to_string(),
        };
    }
    if let Some(unsupported) =
        detect_unsupported_foreign_manager(launcher_path, resolved_path, false)
    {
        return UpdateOwner::ManualActionRequired {
            reason: format!("installation is managed by {unsupported}"),
            guidance: "Update Kimi through the package manager that owns this installation."
                .to_string(),
        };
    }
    UpdateOwner::NativeSelfUpdate {
        program: launcher_path.to_path_buf(),
        args: vec!["upgrade".to_string(), "--yes".to_string()],
        display_command: "kimi upgrade --yes".to_string(),
    }
}

fn is_legacy_kimi_wrapper(launcher_path: &Path, resolved_path: &Path) -> bool {
    for path in [launcher_path, resolved_path] {
        if let Some(sample) = read_text_wrapper_sample(path) {
            let lower = sample.to_ascii_lowercase();
            if lower.contains("kimi-legacy") || lower.contains("no-self-upgrade") {
                return true;
            }
        }
    }
    false
}

fn determine_cursor_owner(
    launcher_path: &Path,
    resolved_path: &Path,
    env: &DiscoveryEnv,
) -> UpdateOwner {
    if env.is_windows && is_unsafe_windows_cmd_shim(launcher_path) {
        return UpdateOwner::ManualActionRequired {
            reason: "Windows .cmd path contains unsafe shell metacharacters".to_string(),
            guidance: "Reinstall Cursor CLI in a standard directory without shell metacharacters."
                .to_string(),
        };
    }
    if let Some(unsupported) =
        detect_unsupported_foreign_manager(launcher_path, resolved_path, true)
    {
        return UpdateOwner::ManualActionRequired {
            reason: format!("installation is managed by {unsupported}"),
            guidance: "Update Cursor CLI through the package manager that owns this installation."
                .to_string(),
        };
    }
    let bin_name = launcher_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("agent");
    let cmd_label = if bin_name.eq_ignore_ascii_case("cursor-agent") {
        "cursor-agent update"
    } else {
        "agent update"
    };
    UpdateOwner::NativeSelfUpdate {
        program: launcher_path.to_path_buf(),
        args: vec!["update".to_string()],
        display_command: cmd_label.to_string(),
    }
}

fn determine_codex_owner(
    launcher_path: &Path,
    resolved_path: &Path,
    version: Option<&str>,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> UpdateOwner {
    let signals = collect_codex_signals(launcher_path, resolved_path, version, env, runner, cache);

    if signals.is_empty() {
        return UpdateOwner::ManualActionRequired {
            reason: "could not verify installation owner (not a proven npm, Homebrew, or standalone Codex installation)".to_string(),
            guidance: "Update Codex using the method you used to install it (npm install -g @openai/codex, brew upgrade --cask codex, or the official Codex standalone installer).".to_string(),
        };
    }

    if has_contradictory_signals(&signals) {
        return UpdateOwner::ManualActionRequired {
            reason: "contradictory ownership metadata detected across multiple installation managers".to_string(),
            guidance: "Remove duplicate or conflicting Codex wrappers so only one installation manager owns the active codex binary on PATH.".to_string(),
        };
    }

    match &signals[0] {
        ManagerSignal::UnsupportedManager { manager } => UpdateOwner::ManualActionRequired {
            reason: format!("Codex installation is managed by unsupported manager '{manager}'"),
            guidance: format!(
                "Update Codex manually using {manager}, or switch to npm, Homebrew, or the official standalone installer."
            ),
        },
        ManagerSignal::Npm { prefix, package } => {
            if is_non_default_channel_version(version)
                || package_json_has_prerelease(prefix, package)
            {
                return UpdateOwner::ManualActionRequired {
                    reason: "prerelease npm channel detected; automatic latest upgrade would change release channels".to_string(),
                    guidance: format!(
                        "Run 'npm install -g --prefix {} {}@<channel>' manually to stay on your selected prerelease channel.",
                        prefix.display(),
                        package
                    ),
                };
            }
            let Some((npm_bin, _)) = resolve_executable_on_path("npm", env) else {
                return UpdateOwner::ManualActionRequired {
                    reason:
                        "active Codex installation is owned by npm, but 'npm' was not found on PATH"
                            .to_string(),
                    guidance: format!(
                        "Add npm to PATH and run 'npm install -g --prefix {} {}@latest'.",
                        prefix.display(),
                        package
                    ),
                };
            };
            if env.is_windows && is_unsafe_windows_cmd_shim(&npm_bin) {
                return UpdateOwner::ManualActionRequired {
                    reason: "npm shim path on Windows contains unsafe shell metacharacters"
                        .to_string(),
                    guidance: "Ensure npm is installed in a directory path without cmd.exe metacharacters.".to_string(),
                };
            }
            UpdateOwner::NpmGlobal {
                npm_program: npm_bin,
                prefix: prefix.clone(),
                package: package.clone(),
            }
        }
        ManagerSignal::HomebrewCask {
            brew_prefix,
            cask_token,
        } => {
            let Some(brew_bin) = find_verified_brew_binary(brew_prefix, env, runner, cache) else {
                return UpdateOwner::ManualActionRequired {
                    reason: "active Codex installation is in Homebrew Caskroom, but a matching 'brew' executable for this prefix was not found".to_string(),
                    guidance: format!("Add the owning Homebrew installation to PATH and run 'brew upgrade --cask {cask_token}'."),
                };
            };
            if let Some(installed_casks) = cache.brew_casks(&brew_bin, runner) {
                if !installed_casks.iter().any(|c| c == cask_token) {
                    return UpdateOwner::ManualActionRequired {
                        reason: format!(
                            "Homebrew cask '{cask_token}' is not registered in the active Homebrew inventory"
                        ),
                        guidance: format!(
                            "Verify your Homebrew installation with 'brew list --cask' or run 'brew upgrade --cask {cask_token}' manually."
                        ),
                    };
                }
            } else {
                return UpdateOwner::ManualActionRequired {
                    reason: "could not verify Homebrew cask inventory with 'brew list --cask'"
                        .to_string(),
                    guidance: format!("Run 'brew upgrade --cask {cask_token}' manually."),
                };
            }
            UpdateOwner::HomebrewCask {
                brew_program: brew_bin,
                cask_token: cask_token.clone(),
            }
        }
        ManagerSignal::HomebrewFormula {
            brew_prefix,
            formula,
        } => {
            let Some(brew_bin) = find_verified_brew_binary(brew_prefix, env, runner, cache) else {
                return UpdateOwner::ManualActionRequired {
                    reason:
                        "active Codex installation is in Homebrew Cellar, but a matching 'brew' executable for this prefix was not found"
                            .to_string(),
                    guidance: format!("Add the owning Homebrew installation to PATH and run 'brew upgrade {formula}'."),
                };
            };
            if let Some(installed_formulas) = cache.brew_formulas(&brew_bin, runner) {
                if !installed_formulas.iter().any(|f| f == formula) {
                    return UpdateOwner::ManualActionRequired {
                        reason: format!(
                            "Homebrew formula '{formula}' is not registered in the active Homebrew inventory"
                        ),
                        guidance: format!("Run 'brew upgrade {formula}' manually."),
                    };
                }
            } else {
                return UpdateOwner::ManualActionRequired {
                    reason:
                        "could not verify Homebrew formula inventory with 'brew list --formula'"
                            .to_string(),
                    guidance: format!("Run 'brew upgrade {formula}' manually."),
                };
            }
            UpdateOwner::HomebrewFormula {
                brew_program: brew_bin,
                formula: formula.clone(),
            }
        }
        ManagerSignal::CodexStandalone {
            bin_dir,
            codex_home,
        } => {
            if is_non_default_channel_version(version)
                || codex_standalone_is_pinned_or_prerelease(
                    codex_home,
                    version,
                    launcher_path,
                    resolved_path,
                )
            {
                return UpdateOwner::ManualActionRequired {
                    reason: "standalone Codex installation is pinned, on a prerelease channel, or its active selection could not be verified"
                        .to_string(),
                    guidance: "Run the official Codex installer with '--release <version>' manually to preserve your pinned or prerelease channel.".to_string(),
                };
            }
            if let Some(expected_bin_dir) = env.expected_codex_bin_dir()
                && !paths_refer_to_same_dir(bin_dir, &expected_bin_dir, env.is_windows)
                && env.codex_install_dir.is_some()
            {
                return UpdateOwner::ManualActionRequired {
                    reason: format!(
                        "CODEX_INSTALL_DIR ({}) does not match active Codex launcher directory ({})",
                        expected_bin_dir.display(),
                        bin_dir.display()
                    ),
                    guidance: "Unset CODEX_INSTALL_DIR or set it to the directory containing your active codex binary before updating.".to_string(),
                };
            }
            UpdateOwner::CodexStandalone {
                bin_dir: bin_dir.clone(),
                codex_home: codex_home.clone(),
                is_windows: env.is_windows,
            }
        }
        ManagerSignal::WinGet { package_id } => UpdateOwner::ManualActionRequired {
            reason: format!(
                "WinGet package '{package_id}' is not a supported automated Codex update path"
            ),
            guidance:
                "Update Codex via npm, Homebrew, or the official standalone PowerShell installer."
                    .to_string(),
        },
    }
}

fn determine_claude_owner(
    launcher_path: &Path,
    resolved_path: &Path,
    version: Option<&str>,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> UpdateOwner {
    let signals = collect_claude_signals(launcher_path, resolved_path, version, env, runner, cache);

    if has_contradictory_signals(&signals) {
        return UpdateOwner::ManualActionRequired {
            reason: "contradictory ownership metadata detected across multiple installation managers".to_string(),
            guidance: "Ensure only one package manager or native installation owns the active 'claude' binary on PATH.".to_string(),
        };
    }

    if let Some(signal) = signals.first() {
        match signal {
            ManagerSignal::UnsupportedManager { manager } => {
                return UpdateOwner::ManualActionRequired {
                    reason: format!(
                        "Claude Code installation is managed by system/external package manager '{manager}'"
                    ),
                    guidance: format!(
                        "Update Claude Code using {manager} directly instead of running 'claude update'."
                    ),
                };
            }
            ManagerSignal::Npm { prefix, package } => {
                if is_non_default_channel_version(version)
                    || package_json_has_prerelease(prefix, package)
                {
                    return UpdateOwner::ManualActionRequired {
                        reason: "prerelease npm channel detected; automatic latest upgrade would change release channels".to_string(),
                        guidance: format!(
                            "Run 'npm install -g --prefix {} {}@<channel>' manually to preserve your release channel.",
                            prefix.display(),
                            package
                        ),
                    };
                }
                let Some((npm_bin, _)) = resolve_executable_on_path("npm", env) else {
                    return UpdateOwner::ManualActionRequired {
                        reason: "active Claude Code installation is owned by npm, but 'npm' was not found on PATH".to_string(),
                        guidance: format!(
                            "Add npm to PATH and run 'npm install -g --prefix {} {}@latest'.",
                            prefix.display(),
                            package
                        ),
                    };
                };
                if env.is_windows && is_unsafe_windows_cmd_shim(&npm_bin) {
                    return UpdateOwner::ManualActionRequired {
                        reason: "npm shim path on Windows contains unsafe shell metacharacters"
                            .to_string(),
                        guidance: "Ensure npm is installed in a directory path without cmd.exe metacharacters.".to_string(),
                    };
                }
                return UpdateOwner::NpmGlobal {
                    npm_program: npm_bin,
                    prefix: prefix.clone(),
                    package: package.clone(),
                };
            }
            ManagerSignal::HomebrewCask {
                brew_prefix,
                cask_token,
            } => {
                let Some(brew_bin) = find_verified_brew_binary(brew_prefix, env, runner, cache)
                else {
                    return UpdateOwner::ManualActionRequired {
                        reason: "active Claude Code installation is in Homebrew Caskroom, but a matching 'brew' executable for this prefix was not found".to_string(),
                        guidance: format!("Add the owning Homebrew installation to PATH and run 'brew upgrade --cask {cask_token}'."),
                    };
                };
                if let Some(installed_casks) = cache.brew_casks(&brew_bin, runner) {
                    if !installed_casks.iter().any(|c| c == cask_token) {
                        return UpdateOwner::ManualActionRequired {
                            reason: format!(
                                "Homebrew cask '{cask_token}' is not registered in the active Homebrew inventory"
                            ),
                            guidance: format!("Run 'brew upgrade --cask {cask_token}' manually."),
                        };
                    }
                } else {
                    return UpdateOwner::ManualActionRequired {
                        reason: "could not verify Homebrew cask inventory with 'brew list --cask'"
                            .to_string(),
                        guidance: format!("Run 'brew upgrade --cask {cask_token}' manually."),
                    };
                }
                return UpdateOwner::HomebrewCask {
                    brew_program: brew_bin,
                    cask_token: cask_token.clone(),
                };
            }
            ManagerSignal::HomebrewFormula {
                brew_prefix: _,
                formula,
            } => {
                return UpdateOwner::ManualActionRequired {
                    reason: format!(
                        "Claude Code is installed as Homebrew formula '{formula}' rather than a supported cask"
                    ),
                    guidance: format!("Update manually with 'brew upgrade {formula}'."),
                };
            }
            ManagerSignal::WinGet { package_id } => {
                let Some((winget_bin, _)) = resolve_executable_on_path("winget", env) else {
                    return UpdateOwner::ManualActionRequired {
                        reason: "active Claude Code installation is owned by WinGet, but 'winget' was not found on PATH".to_string(),
                        guidance: format!("Run 'winget upgrade --exact --id {package_id}' manually."),
                    };
                };
                if !cache.winget_has_package(&winget_bin, package_id, runner) {
                    return UpdateOwner::ManualActionRequired {
                        reason: format!(
                            "WinGet package '{package_id}' could not be verified in local WinGet inventory"
                        ),
                        guidance: format!(
                            "Run 'winget upgrade --exact --id {package_id}' manually."
                        ),
                    };
                }
                return UpdateOwner::WinGet {
                    winget_program: winget_bin,
                    package_id: package_id.clone(),
                };
            }
            ManagerSignal::CodexStandalone { .. } => {}
        }
    }

    if is_proven_native_claude_install(launcher_path, resolved_path, env) {
        UpdateOwner::NativeSelfUpdate {
            program: launcher_path.to_path_buf(),
            args: vec!["update".to_string()],
            display_command: "claude update".to_string(),
        }
    } else {
        UpdateOwner::ManualActionRequired {
            reason: "could not verify native, npm, Homebrew cask, or WinGet ownership for active Claude Code binary".to_string(),
            guidance: "Update Claude Code using the package manager that installed it, or run 'claude doctor' to inspect your installation.".to_string(),
        }
    }
}

fn collect_codex_signals(
    launcher_path: &Path,
    resolved_path: &Path,
    active_version: Option<&str>,
    env: &DiscoveryEnv,
    _runner: &dyn ProcessRunner,
    _cache: &mut PackageManagerCache,
) -> Vec<ManagerSignal> {
    let mut signals = Vec::new();

    if let Some(npm_sig) = detect_npm_ownership(
        launcher_path,
        resolved_path,
        active_version,
        "@openai/codex",
        env.is_windows,
    ) {
        signals.push(npm_sig);
    }

    for brew_sig in
        detect_homebrew_ownership(launcher_path, resolved_path, active_version, "codex", env)
    {
        if !signals.contains(&brew_sig) {
            signals.push(brew_sig);
        }
    }

    if let Some(standalone_sig) =
        detect_codex_standalone_ownership(launcher_path, resolved_path, active_version, env)
    {
        signals.push(standalone_sig);
    }

    if let Some(unsupported) =
        detect_unsupported_foreign_manager(launcher_path, resolved_path, false)
    {
        signals.push(ManagerSignal::UnsupportedManager {
            manager: unsupported,
        });
    }

    signals
}

fn collect_claude_signals(
    launcher_path: &Path,
    resolved_path: &Path,
    active_version: Option<&str>,
    env: &DiscoveryEnv,
    _runner: &dyn ProcessRunner,
    _cache: &mut PackageManagerCache,
) -> Vec<ManagerSignal> {
    let mut signals = Vec::new();

    if let Some(npm_sig) = detect_npm_ownership(
        launcher_path,
        resolved_path,
        active_version,
        "@anthropic-ai/claude-code",
        env.is_windows,
    ) {
        signals.push(npm_sig);
    }

    for brew_sig in
        detect_homebrew_ownership(launcher_path, resolved_path, active_version, "claude", env)
    {
        if !signals.contains(&brew_sig) {
            signals.push(brew_sig);
        }
    }

    if let Some(winget_sig) = detect_winget_claude_ownership(launcher_path, resolved_path) {
        signals.push(winget_sig);
    }

    if let Some(unsupported) =
        detect_unsupported_foreign_manager(launcher_path, resolved_path, false)
    {
        signals.push(ManagerSignal::UnsupportedManager {
            manager: unsupported,
        });
    }

    signals
}

fn has_contradictory_signals(signals: &[ManagerSignal]) -> bool {
    if signals.len() <= 1 {
        return false;
    }
    let first = &signals[0];
    signals.iter().skip(1).any(|s| s != first)
}

fn prefer_launcher_prefix(launcher_path: &Path, prefix: PathBuf, is_windows: bool) -> PathBuf {
    if let Some(bin_dir) = launcher_path.parent() {
        if bin_dir
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
            && let Some(launcher_prefix) = bin_dir.parent()
            && paths_refer_to_same_dir(launcher_prefix, &prefix, is_windows)
        {
            return launcher_prefix.to_path_buf();
        }
        if paths_refer_to_same_dir(bin_dir, &prefix, is_windows) {
            return bin_dir.to_path_buf();
        }
    }
    prefix
}

/// Checks if `launcher_path` or `resolved_path` is owned by an npm global installation
/// of `expected_pkg` (e.g. `@openai/codex` or `@anthropic-ai/claude-code`).
fn detect_npm_ownership(
    launcher_path: &Path,
    resolved_path: &Path,
    active_version: Option<&str>,
    expected_pkg: &str,
    is_windows: bool,
) -> Option<ManagerSignal> {
    if is_local_node_modules_bin_launcher(launcher_path) {
        return None;
    }
    let pkg_parts: Vec<&str> = expected_pkg.split('/').collect();

    for path in [resolved_path, launcher_path] {
        if let Some((prefix, pkg_root)) =
            extract_npm_prefix_from_node_modules(path, &pkg_parts, is_windows)
            && verify_package_json_name_and_version(&pkg_root, expected_pkg, active_version)
            && verify_package_json_entrypoint(
                &pkg_root,
                launcher_path,
                path,
                expected_pkg,
                is_windows,
            )
        {
            let clean_prefix = prefer_launcher_prefix(launcher_path, prefix, is_windows);
            return Some(ManagerSignal::Npm {
                prefix: clean_prefix,
                package: expected_pkg.to_string(),
            });
        }
    }

    // Check if launcher_path is in `<prefix>/bin/<cmd>` (Unix) or `<prefix>/<cmd>[.cmd]` (Windows)
    // with a matching `<prefix>/lib/node_modules/<expected_pkg>/package.json` or
    // `<prefix>/node_modules/<expected_pkg>/package.json` AND launcher is tied to that package.
    if let Some(bin_dir) = launcher_path.parent() {
        let candidate_prefixes = if bin_dir
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
        {
            vec![
                bin_dir.parent().map(Path::to_path_buf),
                Some(bin_dir.to_path_buf()),
            ]
        } else {
            vec![Some(bin_dir.to_path_buf())]
        };

        for prefix_opt in candidate_prefixes.into_iter().flatten() {
            let rel_bases = if is_windows {
                vec![
                    prefix_opt.join("lib").join("node_modules"),
                    prefix_opt.join("node_modules"),
                ]
            } else {
                vec![prefix_opt.join("lib").join("node_modules")]
            };
            for rel_base in rel_bases {
                let mut pkg_root = rel_base;
                for part in &pkg_parts {
                    pkg_root = pkg_root.join(part);
                }
                let canon_pkg_root = canonicalize_clean(&pkg_root);
                if verify_package_json_name_and_version(&pkg_root, expected_pkg, active_version) {
                    let resolved_in_pkg = (resolved_path.starts_with(&pkg_root)
                        || resolved_path.starts_with(&canon_pkg_root))
                        && verify_package_json_entrypoint(
                            &pkg_root,
                            launcher_path,
                            resolved_path,
                            expected_pkg,
                            is_windows,
                        );
                    let wrapper_in_pkg = launcher_targets_npm_pkg_root(
                        launcher_path,
                        &pkg_parts,
                        &prefix_opt,
                        &pkg_root,
                        expected_pkg,
                        is_windows,
                    );
                    if resolved_in_pkg || wrapper_in_pkg {
                        return Some(ManagerSignal::Npm {
                            prefix: prefix_opt,
                            package: expected_pkg.to_string(),
                        });
                    }
                }
            }
        }
    }

    None
}

fn is_local_node_modules_bin_launcher(launcher_path: &Path) -> bool {
    let Some(bin_dir) = launcher_path.parent() else {
        return false;
    };
    bin_dir
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(".bin"))
        && bin_dir
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n.eq_ignore_ascii_case("node_modules"))
}

fn extract_npm_prefix_from_node_modules(
    path: &Path,
    pkg_parts: &[&str],
    is_windows: bool,
) -> Option<(PathBuf, PathBuf)> {
    let components: Vec<OsString> = path
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    for (idx, comp) in components.iter().enumerate() {
        if comp.to_string_lossy().eq_ignore_ascii_case("node_modules")
            && idx + pkg_parts.len() < components.len()
        {
            let matches_pkg = pkg_parts.iter().enumerate().all(|(offset, expected)| {
                components[idx + 1 + offset]
                    .to_string_lossy()
                    .eq_ignore_ascii_case(expected)
            });
            if matches_pkg {
                let has_lib_parent = idx > 0
                    && components[idx - 1]
                        .to_string_lossy()
                        .eq_ignore_ascii_case("lib");
                if !is_windows && !has_lib_parent {
                    continue;
                }
                let prefix_end = if has_lib_parent { idx - 1 } else { idx };
                let prefix: PathBuf = components[..prefix_end].iter().collect();
                let pkg_root: PathBuf = components[..=idx + pkg_parts.len()].iter().collect();
                return Some((prefix, pkg_root));
            }
        }
    }
    None
}

fn verify_package_json_name_and_version(
    pkg_root: &Path,
    expected_pkg: &str,
    active_version: Option<&str>,
) -> bool {
    let pkg_json_path = pkg_root.join("package.json");
    let Ok(contents) = read_discovery_metadata(pkg_json_path) else {
        return false;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return false;
    };
    let name_matches = val
        .get("name")
        .and_then(|n| n.as_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(expected_pkg));
    if !name_matches {
        return false;
    }
    if let Some(active_ver) = active_version
        && let Some(pkg_ver) = val.get("version").and_then(|v| v.as_str())
    {
        let parsed_pkg_ver = parse_version_text(pkg_ver);
        if parsed_pkg_ver.as_deref() != Some(active_ver)
            && !pkg_ver.eq_ignore_ascii_case(active_ver)
        {
            return false;
        }
    }
    true
}

fn verify_package_json_entrypoint(
    pkg_root: &Path,
    launcher_path: &Path,
    target_in_pkg: &Path,
    expected_pkg: &str,
    is_windows: bool,
) -> bool {
    let pkg_json_path = pkg_root.join("package.json");
    let Ok(contents) = read_discovery_metadata(pkg_json_path) else {
        return false;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return false;
    };
    let launcher_stem = launcher_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let target_stem = target_in_pkg
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if let Some(bin_val) = val.get("bin") {
        if let Some(bin_obj) = bin_val.as_object() {
            let Some((_, entry_val)) = bin_obj
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(launcher_stem))
            else {
                return false;
            };
            let Some(rel_bin) = entry_val.as_str() else {
                return false;
            };
            return target_matches_declared_bin_path(pkg_root, target_in_pkg, rel_bin, is_windows);
        }
        if let Some(rel_bin) = bin_val.as_str() {
            let default_cmd = expected_pkg.rsplit('/').next().unwrap_or(expected_pkg);
            let declared_path = normalize_lexical_path(&pkg_root.join(rel_bin.replace('\\', "/")));
            let declared_stem = declared_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let cmd_matches = if expected_pkg.eq_ignore_ascii_case("@qoder-ai/qodercli") {
                launcher_stem.eq_ignore_ascii_case(default_cmd)
            } else {
                launcher_stem.eq_ignore_ascii_case(default_cmd)
                    || launcher_stem.eq_ignore_ascii_case(declared_stem)
            };
            return cmd_matches
                && target_matches_declared_bin_path(pkg_root, target_in_pkg, rel_bin, is_windows);
        }
        return false;
    }

    if expected_pkg.eq_ignore_ascii_case("@qoder-ai/qodercli") {
        return launcher_stem.eq_ignore_ascii_case("qodercli")
            && target_stem.eq_ignore_ascii_case("qodercli");
    }

    true
}

fn target_matches_declared_bin_path(
    pkg_root: &Path,
    target_in_pkg: &Path,
    rel_bin: &str,
    is_windows: bool,
) -> bool {
    let clean_rel = rel_bin.replace('\\', "/");
    let trimmed_rel = clean_rel.strip_prefix("./").unwrap_or(&clean_rel);
    if trimmed_rel.is_empty() {
        return false;
    }
    let declared = normalize_lexical_path(&pkg_root.join(trimmed_rel));
    let canon_pkg_declared =
        normalize_lexical_path(&canonicalize_clean(pkg_root).join(trimmed_rel));
    let norm_target = normalize_lexical_path(target_in_pkg);

    paths_refer_to_same_dir(&norm_target, &declared, is_windows)
        || paths_refer_to_same_dir(&norm_target, &canon_pkg_declared, is_windows)
}

fn package_json_has_prerelease(prefix: &Path, expected_pkg: &str) -> bool {
    let pkg_parts: Vec<&str> = expected_pkg.split('/').collect();
    for rel_base in [
        prefix.join("lib").join("node_modules"),
        prefix.join("node_modules"),
    ] {
        let mut pkg_root = rel_base;
        for part in &pkg_parts {
            pkg_root = pkg_root.join(part);
        }
        let pkg_json = pkg_root.join("package.json");
        if let Ok(contents) = read_discovery_metadata(pkg_json)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents)
            && let Some(ver) = val.get("version").and_then(|v| v.as_str())
            && is_non_default_channel_version(Some(ver))
        {
            return true;
        }
    }
    false
}

// Ownership evidence comes from small manifests/markers, never arbitrary-sized files.
fn read_discovery_metadata(path: impl AsRef<Path>) -> std::io::Result<String> {
    use std::io::{Error, ErrorKind, Read};
    const MAX_METADATA_BYTES: u64 = 1024 * 1024;
    let path = path.as_ref();
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_METADATA_BYTES {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "ownership metadata is not a regular file within the size limit",
        ));
    }
    let mut text = String::new();
    fs::File::open(path)?
        .take(MAX_METADATA_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_METADATA_BYTES {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "ownership metadata exceeds the size limit",
        ));
    }
    Ok(text)
}

fn read_text_wrapper_sample(launcher_path: &Path) -> Option<String> {
    use std::io::Read;
    let metadata = fs::metadata(launcher_path).ok()?;
    if !metadata.is_file() || metadata.len() > 8192 {
        return None;
    }
    let file = fs::File::open(launcher_path).ok()?;
    let mut bytes = Vec::with_capacity(8192);
    file.take(8193).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 8192 || bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn launcher_targets_npm_pkg_root(
    launcher_path: &Path,
    pkg_parts: &[&str],
    candidate_prefix: &Path,
    candidate_pkg_root: &Path,
    expected_pkg: &str,
    is_windows: bool,
) -> bool {
    let Some(exec_target) = extract_verified_wrapper_exec_target(launcher_path) else {
        return false;
    };
    let Some((target_prefix, target_pkg_root)) =
        extract_npm_prefix_from_node_modules(&exec_target, pkg_parts, is_windows)
    else {
        return false;
    };
    paths_refer_to_same_dir(&target_prefix, candidate_prefix, is_windows)
        && paths_refer_to_same_dir(&target_pkg_root, candidate_pkg_root, is_windows)
        && verify_package_json_entrypoint(
            candidate_pkg_root,
            launcher_path,
            &exec_target,
            expected_pkg,
            is_windows,
        )
}

#[derive(Default)]
struct WrapperBindings {
    basedir_bound_to_launcher: bool,
    dp0_bound_to_launcher: bool,
    prog_bound_to_node: bool,
}

fn extract_verified_wrapper_exec_target(launcher_path: &Path) -> Option<PathBuf> {
    let sample = read_text_wrapper_sample(launcher_path)?;
    let launcher_dir = launcher_path.parent()?;
    let mut bindings = WrapperBindings::default();
    let mut exec_target: Option<PathBuf> = None;

    for line in sample.lines() {
        let trimmed = line.trim();
        if is_wrapper_comment_or_empty_line(trimmed) {
            continue;
        }
        if trimmed == r#"basedir=$(dirname "$(echo "$0" | sed -e 's,\\,/,g')")"# {
            bindings.basedir_bound_to_launcher = true;
            continue;
        }
        if trimmed == r#"basedir=`cygpath -w "$basedir"`"# && bindings.basedir_bound_to_launcher {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("set dp0=%~dp0")
            || trimmed.eq_ignore_ascii_case("set \"dp0=%~dp0\"")
        {
            bindings.dp0_bound_to_launcher = true;
            continue;
        }
        if (trimmed.eq_ignore_ascii_case("set \"_prog=%dp0%\\node.exe\"")
            && bindings.dp0_bound_to_launcher)
            || trimmed.eq_ignore_ascii_case("set \"_prog=%~dp0\\node.exe\"")
            || trimmed.eq_ignore_ascii_case("set \"_prog=node\"")
        {
            bindings.prog_bound_to_node = true;
            continue;
        }
        if is_wrapper_preamble_line(trimmed) {
            continue;
        }
        let target = parse_single_exec_line(trimmed, launcher_dir, &bindings)?;
        if let Some(ref prev) = exec_target
            && prev != &target
        {
            return None;
        }
        exec_target = Some(target);
    }

    exec_target
}

fn is_wrapper_preamble_line(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "@echo off"
            | "echo off"
            | "@setlocal"
            | "setlocal"
            | "endlocal"
            | "goto start"
            | ":find_dp0"
            | "exit /b"
            | ":start"
            | "call :find_dp0"
            | "case `uname` in"
            | "*cygwin*|*mingw*|*msys*)"
            | "if command -v cygpath > /dev/null 2>&1; then"
            | ";;"
            | "esac"
            | "if [ -x \"$basedir/node\" ]; then"
            | "if [ -x \"$basedir/node.exe\" ]; then"
            | "else"
            | "fi"
            | "if exist \"%dp0%\\node.exe\" ("
            | "@if exist \"%~dp0\\node.exe\" ("
            | ") else ("
            | "set pathext=%pathext:;.js;=;%"
            | "@set pathext=%pathext:;.js;=;%"
            | ")"
    )
}

fn is_passthrough_arg(arg: &str) -> bool {
    matches!(arg, "$@" | "$*" | "%*")
}

fn is_node_interpreter(token: &str, bindings: &WrapperBindings) -> bool {
    let normalized = token.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    match lower.as_str() {
        "node" | "node.exe" | "%~dp0/node.exe" | "%~dp0node.exe" => true,
        "$basedir/node" | "$basedir/node.exe" | "${basedir}/node" | "${basedir}/node.exe" => {
            bindings.basedir_bound_to_launcher
        }
        "%dp0%/node.exe" | "%dp0%node.exe" => bindings.dp0_bound_to_launcher,
        "%_prog%" => bindings.prog_bound_to_node,
        _ => {
            !lower.contains('$')
                && !lower.contains('%')
                && (Path::new(token).is_absolute()
                    || normalized.starts_with('/')
                    || is_windows_drive_or_unc_path(&normalized))
                && (lower.ends_with("/bin/node") || lower.ends_with("/node.exe"))
        }
    }
}

fn strip_ascii_prefix_case_insensitive<'a>(s: &'a str, prefix_lower: &str) -> Option<&'a str> {
    let head = s.get(..prefix_lower.len())?;
    if head.eq_ignore_ascii_case(prefix_lower) {
        s.get(prefix_lower.len()..)
    } else {
        None
    }
}

fn parse_single_exec_line(
    trimmed: &str,
    launcher_dir: &Path,
    bindings: &WrapperBindings,
) -> Option<PathBuf> {
    let after_endlocal = strip_ascii_prefix_case_insensitive(
        trimmed,
        "endlocal & goto #_undefined_# 2>nul || title %comspec% &",
    )
    .map(str::trim_start)
    .unwrap_or(trimmed);

    let effective =
        strip_ascii_prefix_case_insensitive(after_endlocal, "set pathext=%pathext:;.js;=;% &")
            .or_else(|| {
                strip_ascii_prefix_case_insensitive(
                    after_endlocal,
                    "@set pathext=%pathext:;.js;=;% &",
                )
            })
            .map(str::trim_start)
            .unwrap_or(after_endlocal);

    let tokens = extract_command_tokens_checked(effective)?;
    let raw_target = match tokens.as_slice() {
        [cmd, target] if cmd == "exec" => target,
        [cmd, target, passthrough] if cmd == "exec" && is_passthrough_arg(passthrough) => target,
        [cmd, interp, target] if cmd == "exec" && is_node_interpreter(interp, bindings) => target,
        [cmd, interp, target, passthrough]
            if cmd == "exec"
                && is_node_interpreter(interp, bindings)
                && is_passthrough_arg(passthrough) =>
        {
            target
        }
        [cmd, target, passthrough]
            if cmd.eq_ignore_ascii_case("call") && is_passthrough_arg(passthrough) =>
        {
            target
        }
        [target, passthrough]
            if passthrough == "%*" && (target.contains('/') || target.contains('\\')) =>
        {
            target
        }
        [interp, target, passthrough]
            if passthrough == "%*" && is_node_interpreter(interp, bindings) =>
        {
            target
        }
        _ => return None,
    };
    if raw_target.starts_with('-') {
        return None;
    }
    resolve_wrapper_path_token(raw_target, launcher_dir, bindings)
}

fn is_wrapper_comment_or_empty_line(trimmed: &str) -> bool {
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("//")
        || trimmed.starts_with("::")
        || trimmed.eq_ignore_ascii_case("rem")
        || trimmed.eq_ignore_ascii_case("@rem")
    {
        return true;
    }
    let without_at = trimmed.strip_prefix('@').unwrap_or(trimmed).trim_start();
    without_at
        .get(..3)
        .is_some_and(|p| p.eq_ignore_ascii_case("rem"))
        && without_at
            .get(3..)
            .is_some_and(|r| r.starts_with(char::is_whitespace))
}

fn extract_command_tokens_checked(line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match in_quote {
            Some(q) => {
                if c == q {
                    in_quote = None;
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else if c == '\\' && q == '"' {
                    if let Some(&next_c) = chars.peek()
                        && matches!(next_c, '"' | '\\' | '$')
                    {
                        current.push(chars.next().unwrap());
                    } else {
                        current.push(c);
                    }
                } else {
                    current.push(c);
                }
            }
            None => {
                if matches!(c, ';' | '|' | '&' | '>' | '<' | '`') {
                    return None;
                } else if c == '"' || c == '\'' {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                    in_quote = Some(c);
                } else if c.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else if c == '\\' {
                    if let Some(&next_c) = chars.peek()
                        && (next_c.is_whitespace() || matches!(next_c, '"' | '\''))
                    {
                        current.push(chars.next().unwrap());
                    } else {
                        current.push(c);
                    }
                } else {
                    current.push(c);
                }
            }
        }
    }
    if in_quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Some(tokens)
}

fn is_windows_drive_or_unc_path(normalized: &str) -> bool {
    if normalized.starts_with("//") {
        return true;
    }
    let bytes = normalized.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

fn resolve_wrapper_path_token(
    raw_token: &str,
    launcher_dir: &Path,
    bindings: &WrapperBindings,
) -> Option<PathBuf> {
    let normalized = raw_token.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let dir_str = launcher_dir.to_string_lossy().replace('\\', "/");
    let dir_trimmed = dir_str.trim_end_matches('/');

    let launcher_prefixes: [(&str, bool); 8] = [
        ("$basedir/", bindings.basedir_bound_to_launcher),
        ("${basedir}/", bindings.basedir_bound_to_launcher),
        ("%dp0%/", bindings.dp0_bound_to_launcher),
        ("%dp0%", bindings.dp0_bound_to_launcher),
        ("%~dp0/", true),
        ("%~dp0", true),
        ("$(dirname \"$0\")/", true),
        ("$(dirname $0)/", true),
    ];

    for (prefix, is_bound) in launcher_prefixes {
        if lower.starts_with(prefix) {
            if !is_bound {
                return None;
            }
            let rest = normalized.get(prefix.len()..)?.trim_start_matches('/');
            if rest.is_empty() || rest.contains('$') || rest.contains('%') {
                return None;
            }
            let expanded = format!("{dir_trimmed}/{rest}");
            return Some(normalize_lexical_path(Path::new(&expanded)));
        }
    }

    if normalized.contains('$') || normalized.contains('%') {
        return None;
    }

    if Path::new(raw_token).is_absolute()
        || normalized.starts_with('/')
        || is_windows_drive_or_unc_path(&normalized)
    {
        Some(normalize_lexical_path(Path::new(&normalized)))
    } else {
        None
    }
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn signals_refer_to_same_owner(a: &ManagerSignal, b: &ManagerSignal, is_windows: bool) -> bool {
    match (a, b) {
        (
            ManagerSignal::Npm {
                prefix: p1,
                package: pkg1,
            },
            ManagerSignal::Npm {
                prefix: p2,
                package: pkg2,
            },
        ) => pkg1.eq_ignore_ascii_case(pkg2) && paths_refer_to_same_dir(p1, p2, is_windows),
        (
            ManagerSignal::HomebrewCask {
                brew_prefix: p1,
                cask_token: c1,
            },
            ManagerSignal::HomebrewCask {
                brew_prefix: p2,
                cask_token: c2,
            },
        ) => c1 == c2 && paths_refer_to_same_dir(p1, p2, is_windows),
        (
            ManagerSignal::HomebrewFormula {
                brew_prefix: p1,
                formula: f1,
            },
            ManagerSignal::HomebrewFormula {
                brew_prefix: p2,
                formula: f2,
            },
        ) => f1 == f2 && paths_refer_to_same_dir(p1, p2, is_windows),
        _ => a == b,
    }
}

fn push_unique_manager_signal(
    signals: &mut Vec<ManagerSignal>,
    candidate: ManagerSignal,
    is_windows: bool,
) {
    if !signals
        .iter()
        .any(|existing| signals_refer_to_same_owner(existing, &candidate, is_windows))
    {
        signals.push(candidate);
    }
}

/// Detects Homebrew Caskroom or Cellar ownership tied to `launcher_path` or `resolved_path`.
fn detect_homebrew_ownership(
    launcher_path: &Path,
    resolved_path: &Path,
    active_version: Option<&str>,
    agent_key: &str,
    env: &DiscoveryEnv,
) -> Vec<ManagerSignal> {
    let mut out = Vec::new();

    for path in [resolved_path, launcher_path] {
        if let Some((brew_prefix, cask_token)) = extract_homebrew_dir_token(path, "Caskroom") {
            let clean_prefix = prefer_launcher_prefix(launcher_path, brew_prefix, env.is_windows);
            let sig = ManagerSignal::HomebrewCask {
                brew_prefix: clean_prefix,
                cask_token,
            };
            push_unique_manager_signal(&mut out, sig, env.is_windows);
        }
        if let Some((brew_prefix, formula)) = extract_homebrew_dir_token(path, "Cellar") {
            let clean_prefix = prefer_launcher_prefix(launcher_path, brew_prefix, env.is_windows);
            let sig = ManagerSignal::HomebrewFormula {
                brew_prefix: clean_prefix,
                formula,
            };
            push_unique_manager_signal(&mut out, sig, env.is_windows);
        }
    }

    if !out.is_empty() {
        return out;
    }

    // If launcher_path is `<brew_prefix>/bin/<cmd>` (not directly resolved inside Caskroom/Cellar),
    // require explicit Homebrew wrapper evidence in `launcher_path` before attributing ownership.
    if let Some(bin_dir) = launcher_path.parent()
        && bin_dir
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
        && let Some(prefix) = bin_dir.parent()
        && launcher_references_homebrew(launcher_path, prefix, env.is_windows)
    {
        let caskroom = prefix.join("Caskroom");
        if caskroom.is_dir() {
            let matching_casks = find_matching_casks_in_dir(&caskroom, agent_key, active_version);
            for cask_token in matching_casks {
                out.push(ManagerSignal::HomebrewCask {
                    brew_prefix: prefix.to_path_buf(),
                    cask_token,
                });
            }
        }
        let cellar = prefix.join("Cellar");
        if cellar.is_dir() && cellar.join(agent_key).is_dir() {
            out.push(ManagerSignal::HomebrewFormula {
                brew_prefix: prefix.to_path_buf(),
                formula: agent_key.to_string(),
            });
        }
    }

    out
}

fn launcher_references_homebrew(
    launcher_path: &Path,
    candidate_prefix: &Path,
    is_windows: bool,
) -> bool {
    let Some(exec_target) = extract_verified_wrapper_exec_target(launcher_path) else {
        return false;
    };
    let Some((target_prefix, _)) = extract_homebrew_dir_token(&exec_target, "Caskroom")
        .or_else(|| extract_homebrew_dir_token(&exec_target, "Cellar"))
    else {
        return false;
    };
    paths_refer_to_same_dir(&target_prefix, candidate_prefix, is_windows)
}

fn extract_homebrew_dir_token(path: &Path, marker_dir: &str) -> Option<(PathBuf, String)> {
    let components: Vec<OsString> = path
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    for (idx, comp) in components.iter().enumerate() {
        if comp.to_string_lossy() == marker_dir && idx + 1 < components.len() {
            let prefix: PathBuf = components[..idx].iter().collect();
            let token = components[idx + 1].to_string_lossy().into_owned();
            if !token.is_empty() {
                return Some((prefix, token));
            }
        }
    }
    None
}

fn find_matching_casks_in_dir(
    caskroom: &Path,
    agent_key: &str,
    active_version: Option<&str>,
) -> Vec<String> {
    let Ok(entries) = fs::read_dir(caskroom) else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let cask_path = entry.path();
        if cask_path.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if cask_matches_agent(&name, agent_key)
                && cask_dir_matches_version(&cask_path, active_version)
            {
                matches.push(name);
            }
        }
    }
    matches.sort();
    matches
}

fn cask_dir_matches_version(cask_path: &Path, active_version: Option<&str>) -> bool {
    let Some(active_ver) = active_version else {
        return true;
    };
    if cask_path.join(active_ver).is_dir() || cask_path.join("latest").is_dir() {
        return true;
    }
    let Ok(entries) = fs::read_dir(cask_path) else {
        return false;
    };
    let mut has_version_subdirs = false;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') && entry.path().is_dir() {
            has_version_subdirs = true;
            if parse_version_text(&name).as_deref() == Some(active_ver) {
                return true;
            }
        }
    }
    !has_version_subdirs
}

fn cask_matches_agent(cask_name: &str, agent_key: &str) -> bool {
    if agent_key == "codex" {
        cask_name == "codex" || cask_name.starts_with("codex@")
    } else if agent_key == "claude" {
        cask_name == "claude-code" || cask_name.starts_with("claude-code@")
    } else {
        cask_name == agent_key
    }
}

fn find_verified_brew_binary(
    brew_prefix: &Path,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> Option<PathBuf> {
    // 1. Prefer the `brew` binary inside `<brew_prefix>/bin` (or matching `HOMEBREW_PREFIX/bin`) when present.
    let mut candidate_bins = vec![brew_prefix.join("bin")];
    if let Some(ref hb) = env.homebrew_prefix
        && paths_refer_to_same_dir(hb, brew_prefix, env.is_windows)
    {
        candidate_bins.push(hb.join("bin"));
    }
    for prefix_bin in candidate_bins {
        if let Some(prefix_brew) = find_executable_in_dir(&prefix_bin, "brew", env) {
            if env.is_windows && is_unsafe_windows_cmd_shim(&prefix_brew) {
                return None;
            }
            if let Some(reported_prefix) = cache.brew_prefix(&prefix_brew, runner)
                && !paths_refer_to_same_dir(&reported_prefix, brew_prefix, env.is_windows)
            {
                return None;
            }
            return Some(prefix_brew);
        }
    }

    // 2. Fall back to `brew` on PATH only if `brew --prefix` confirms it manages `brew_prefix`.
    if let Some((path_brew, _)) = resolve_executable_on_path("brew", env) {
        if env.is_windows && is_unsafe_windows_cmd_shim(&path_brew) {
            return None;
        }
        if let Some(reported_prefix) = cache.brew_prefix(&path_brew, runner)
            && paths_refer_to_same_dir(&reported_prefix, brew_prefix, env.is_windows)
        {
            return Some(path_brew);
        }
    }

    None
}

fn detect_winget_claude_ownership(
    launcher_path: &Path,
    resolved_path: &Path,
) -> Option<ManagerSignal> {
    for path in [resolved_path, launcher_path] {
        let normalized = path.to_string_lossy().replace('\\', "/");
        let lower = normalized.to_ascii_lowercase();
        if lower.contains("microsoft/winget/packages/anthropic.claudecode")
            || lower.contains("microsoft/winget/links/claude")
        {
            return Some(ManagerSignal::WinGet {
                package_id: "Anthropic.ClaudeCode".to_string(),
            });
        }
    }
    if has_adjacent_marker(launcher_path, resolved_path, ".winget-anthropic-claudecode") {
        return Some(ManagerSignal::WinGet {
            package_id: "Anthropic.ClaudeCode".to_string(),
        });
    }
    None
}

fn detect_codex_standalone_ownership(
    launcher_path: &Path,
    resolved_path: &Path,
    active_version: Option<&str>,
    env: &DiscoveryEnv,
) -> Option<ManagerSignal> {
    let bin_dir = launcher_path.parent()?.to_path_buf();

    // Case 1: resolved_path or launcher_path points into `<codex_home>/packages/standalone/...`
    for path in [resolved_path, launcher_path] {
        if let Some((codex_home, release_or_current_dir)) = extract_codex_standalone_root(path)
            && is_valid_codex_standalone_tree(&codex_home, &release_or_current_dir, active_version)
        {
            return Some(ManagerSignal::CodexStandalone {
                bin_dir,
                codex_home,
            });
        }
    }

    // Case 2: launcher_path is in the expected standalone bin dir AND has standalone launcher proof
    // (`launcher_references_codex_standalone` or Windows `rg.exe` companion) AND `<codex_home>`
    // has standalone release metadata matching `active_version`.
    if let Some(codex_home) = env.effective_codex_home()
        && has_matching_codex_standalone_layout(&codex_home, active_version)
    {
        let standalone_root = codex_home.join("packages").join("standalone");
        let canon_standalone = canonicalize_clean(&standalone_root);
        if resolved_path.starts_with(&canon_standalone)
            || resolved_path.starts_with(&standalone_root)
        {
            return Some(ManagerSignal::CodexStandalone {
                bin_dir,
                codex_home,
            });
        }
        let has_windows_companion = env.is_windows
            && bin_dir.join("codex.exe").is_file()
            && bin_dir.join("rg.exe").is_file();
        if let Some(expected_bin) = env.expected_codex_bin_dir()
            && paths_refer_to_same_dir(&bin_dir, &expected_bin, env.is_windows)
            && (launcher_references_codex_standalone(launcher_path, &codex_home, env.is_windows)
                || has_windows_companion)
        {
            return Some(ManagerSignal::CodexStandalone {
                bin_dir,
                codex_home,
            });
        }
    }

    None
}

fn launcher_references_codex_standalone(
    launcher_path: &Path,
    codex_home: &Path,
    is_windows: bool,
) -> bool {
    let Some(exec_target) = extract_verified_wrapper_exec_target(launcher_path) else {
        return false;
    };
    let Some((target_codex_home, _)) = extract_codex_standalone_root(&exec_target) else {
        return false;
    };
    paths_refer_to_same_dir(&target_codex_home, codex_home, is_windows)
}

fn extract_codex_standalone_root(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let components: Vec<OsString> = path
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    for idx in 0..components.len() {
        if components[idx]
            .to_string_lossy()
            .eq_ignore_ascii_case("packages")
            && idx + 1 < components.len()
            && components[idx + 1]
                .to_string_lossy()
                .eq_ignore_ascii_case("standalone")
        {
            let codex_home: PathBuf = components[..idx].iter().collect();
            let release_dir: PathBuf = if idx + 3 < components.len()
                && components[idx + 2]
                    .to_string_lossy()
                    .eq_ignore_ascii_case("releases")
            {
                components[..=idx + 3].iter().collect()
            } else if idx + 2 < components.len() {
                components[..=idx + 2].iter().collect()
            } else {
                components[..=idx + 1].iter().collect()
            };
            return Some((codex_home, release_dir));
        }
    }
    None
}

// Official Codex package targets, including GNU builds used by packaged daemons.
const CODEX_STANDALONE_TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
];

fn has_matching_codex_standalone_layout(codex_home: &Path, active_version: Option<&str>) -> bool {
    let standalone_root = codex_home.join("packages").join("standalone");
    let current_dir = standalone_root.join("current");
    if current_dir.is_dir()
        && is_valid_codex_standalone_tree(codex_home, &current_dir, active_version)
    {
        return true;
    }
    let releases_dir = standalone_root.join("releases");
    if let Some(active_ver) = active_version {
        let release_names = std::iter::once(active_ver.to_string()).chain(
            CODEX_STANDALONE_TARGETS
                .iter()
                .map(|target| format!("{active_ver}-{target}")),
        );
        for release_name in release_names {
            let ver_dir = releases_dir.join(release_name);
            if ver_dir.is_dir()
                && is_valid_codex_standalone_tree(codex_home, &ver_dir, active_version)
            {
                return true;
            }
        }
        return false;
    }
    if let Ok(entries) = fs::read_dir(&releases_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_valid_codex_standalone_tree(codex_home, &path, None) {
                return true;
            }
        }
    }
    false
}

fn is_valid_codex_standalone_tree(
    codex_home: &Path,
    release_dir: &Path,
    active_version: Option<&str>,
) -> bool {
    let standalone_root = codex_home.join("packages").join("standalone");
    if !standalone_root.is_dir() {
        return false;
    }
    let pkg_json = release_dir.join("codex-package.json");
    if pkg_json.is_file() {
        let Ok(contents) = read_discovery_metadata(&pkg_json) else {
            return false;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents) else {
            return false;
        };
        if let Some(active_ver) = active_version
            && let Some(pkg_ver) = val.get("version").and_then(|v| v.as_str())
            && parse_version_text(pkg_ver).as_deref() != Some(active_ver)
            && !pkg_ver.eq_ignore_ascii_case(active_ver)
        {
            return false;
        }
        return true;
    }
    release_dir.join("codex-path").join("rg").is_file()
        || release_dir.join("codex-path").join("rg.exe").is_file()
        || release_dir.join("codex-resources").join("rg").is_file()
        || release_dir.join("codex-resources").join("rg.exe").is_file()
        || standalone_root.join("auto-update-version").is_file()
        || standalone_root.join("selection-history").is_file()
}

fn codex_standalone_is_pinned_or_prerelease(
    codex_home: &Path,
    active_version: Option<&str>,
    launcher_path: &Path,
    resolved_path: &Path,
) -> bool {
    let standalone_root = codex_home.join("packages").join("standalone");
    let auto_update = standalone_root.join("auto-update-version");
    // Explicit --release installs remove this marker. Missing or unreadable
    // metadata must not opt a pinned installation back into the latest channel.
    let Ok(contents) = read_discovery_metadata(&auto_update) else {
        return true;
    };
    let release_name = contents.trim();
    let version = CODEX_STANDALONE_TARGETS
        .iter()
        .find_map(|target| release_name.strip_suffix(target)?.strip_suffix('-'))
        .unwrap_or(release_name);
    // Strip only a known platform suffix, retaining any actual prerelease suffix.
    if !is_valid_version_token(version)
        || is_non_default_channel_version(Some(version))
        || active_version != Some(version)
    {
        return true;
    }
    let current_dir = standalone_root.join("current");
    // Selection metadata must exist and agree with the executable actually invoked.
    let Ok(selected) = fs::canonicalize(&current_dir) else {
        return true;
    };
    let marker_release = standalone_root.join("releases").join(release_name);
    match fs::symlink_metadata(&marker_release) {
        Ok(_) => {
            if fs::canonicalize(&marker_release).ok().as_ref() != Some(&selected) {
                return true;
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            // Legacy/copied layouts may use a real current directory without releases/.
            if fs::canonicalize(&standalone_root)
                .ok()
                .map(|root| root.join("current"))
                != Some(selected.clone())
            {
                return true;
            }
        }
        Err(_) => return true,
    }
    let wrapper_target = extract_verified_wrapper_exec_target(launcher_path);
    for path in [
        Some(resolved_path),
        Some(launcher_path),
        wrapper_target.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some((_, active_dir)) = extract_codex_standalone_root(path)
            && fs::canonicalize(active_dir).ok().as_ref() != Some(&selected)
        {
            return true;
        }
    }
    // The selected directory is also the active directory for symlinks/wrappers.
    // Copied Windows launchers additionally require a successful matching probe above.
    let Ok(contents) = read_discovery_metadata(current_dir.join("codex-package.json")) else {
        return true;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return true;
    };
    if val.get("version").and_then(|v| v.as_str()) != Some(version) {
        return true;
    }
    if let Some(target) = val.get("target").and_then(|v| v.as_str())
        && release_name != version
        && release_name != format!("{version}-{target}")
    {
        return true;
    }
    if let Some(channel) = val.get("channel").and_then(|c| c.as_str())
        && !channel.eq_ignore_ascii_case("latest")
        && !channel.eq_ignore_ascii_case("stable")
    {
        return true;
    }
    false
}

fn is_proven_native_claude_install(
    launcher_path: &Path,
    resolved_path: &Path,
    env: &DiscoveryEnv,
) -> bool {
    for path in [resolved_path, launcher_path] {
        let normalized = path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if normalized.contains("/.local/share/claude/")
            || normalized.contains("/.claude/")
            || normalized.contains("/.local/bin/claude")
            || normalized.contains("/appdata/local/claude/")
            || normalized.contains("/appdata/local/programs/claude/")
        {
            return true;
        }
    }
    if let Some(ref home) = env.home_dir {
        let local_bin = home.join(".local").join("bin");
        if let Some(parent) = launcher_path.parent()
            && paths_refer_to_same_dir(parent, &local_bin, env.is_windows)
        {
            return true;
        }
    }
    has_adjacent_marker(launcher_path, resolved_path, ".claude-native")
}

fn detect_unsupported_foreign_manager(
    launcher_path: &Path,
    resolved_path: &Path,
    reject_npm_and_brew: bool,
) -> Option<String> {
    for path in [resolved_path, launcher_path] {
        let normalized = path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if normalized.contains("/.bun/") {
            return Some("bun".to_string());
        }
        if normalized.contains("/.pnpm/") || normalized.contains("/pnpm/") {
            return Some("pnpm".to_string());
        }
        if normalized.contains("/.yarn/") || normalized.contains("/yarn/") {
            return Some("yarn".to_string());
        }
        if normalized.contains("/nix/store/") {
            return Some("nix".to_string());
        }
        if normalized.contains("/snap/") {
            return Some("snap".to_string());
        }
        if normalized.contains("/flatpak/") {
            return Some("flatpak".to_string());
        }
        if normalized.contains("/scoop/apps/") || normalized.contains("/scoop/shims/") {
            return Some("scoop".to_string());
        }
        if normalized.contains("/programdata/chocolatey/")
            || normalized.contains("/chocolatey/lib/")
            || normalized.contains("/chocolatey/bin/")
        {
            return Some("chocolatey".to_string());
        }
        if !normalized.contains("microsoft/winget/packages/anthropic.claudecode")
            && !normalized.contains("microsoft/winget/links/claude")
            && !has_adjacent_marker(launcher_path, resolved_path, ".winget-anthropic-claudecode")
            && (normalized.contains("/microsoft/winget/packages/")
                || normalized.contains("/microsoft/winget/links/")
                || normalized.contains("/winget/packages/")
                || normalized.contains("/winget/links/"))
        {
            return Some("winget".to_string());
        }
        if normalized.starts_with("/usr/bin/")
            || normalized.starts_with("/usr/games/")
            || normalized.starts_with("/bin/")
            || normalized.starts_with("/sbin/")
        {
            return Some("system package manager".to_string());
        }
        if reject_npm_and_brew {
            if normalized.contains("/node_modules/") {
                return Some("npm".to_string());
            }
            if normalized.contains("/caskroom/") || normalized.contains("/cellar/") {
                return Some("homebrew".to_string());
            }
        }
    }
    None
}

fn has_adjacent_marker(launcher_path: &Path, resolved_path: &Path, marker_name: &str) -> bool {
    for path in [launcher_path, resolved_path] {
        if let Some(parent) = path.parent()
            && parent.join(marker_name).is_file()
        {
            return true;
        }
    }
    false
}

fn is_non_default_channel_version(version: Option<&str>) -> bool {
    let Some(ver) = version else {
        return false;
    };
    let trimmed = ver.trim().trim_start_matches(['v', 'V']);
    let without_build = trimmed.split('+').next().unwrap_or(trimmed);
    if let Some((core, prerelease)) = without_build.split_once('-')
        && !prerelease.is_empty()
        && core
            .split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && prerelease
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    {
        return true;
    }
    let lower = without_build.to_ascii_lowercase();
    lower.contains("-alpha")
        || lower.contains("-beta")
        || lower.contains("-canary")
        || lower.contains("-next")
        || lower.contains("-rc")
        || lower.contains("-dev")
        || lower.contains("-nightly")
}

pub fn paths_refer_to_same_dir(a: &Path, b: &Path, is_windows: bool) -> bool {
    let ca = canonicalize_clean(a);
    let cb = canonicalize_clean(b);
    if is_windows {
        let sa = ca
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        let sb = cb
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        sa == sb
    } else {
        ca == cb
    }
}
