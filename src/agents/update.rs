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

//! Update plan selection, safe execution, Codex standalone installer verification,
//! and post-update active version verification.

use crate::agents::discovery::{
    DiscoveredAgent, DiscoveryEnv, PackageManagerCache, UpdateOwner, discover_agent,
    is_unsafe_windows_batch_command, paths_refer_to_same_dir, resolve_executable_on_path,
};
use crate::agents::{AgentId, ProcessError, ProcessOutput, ProcessRequest, ProcessRunner};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Fixed official HTTPS URL for the Unix (macOS / Linux) Codex standalone installer.
pub const CODEX_UNIX_INSTALLER_URL: &str = "https://chatgpt.com/codex/install.sh";

/// Fixed official HTTPS URL for the Windows PowerShell Codex standalone installer.
pub const CODEX_WINDOWS_INSTALLER_URL: &str = "https://chatgpt.com/codex/install.ps1";

/// Maximum bytes captured per stream (stdout/stderr) during an update execution.
pub const UPDATE_MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_INSTALLER_SCRIPT_BYTES: u64 = 1024 * 1024;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Immutable update execution plan selected for a discovered agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdatePlan {
    /// Direct execution of a resolved binary with an argument array and scoped child environment.
    Command {
        program: PathBuf,
        args: Vec<OsString>,
        env_overrides: Vec<(OsString, OsString)>,
        display_command: String,
        target_description: String,
    },
    /// Official Codex standalone installer workflow over verified HTTPS into a private temp file.
    CodexStandalone {
        installer_url: &'static str,
        bin_dir: PathBuf,
        codex_home: PathBuf,
        is_windows: bool,
        display_command: String,
        target_description: String,
    },
    /// Installation cannot be safely updated automatically.
    ManualActionRequired { reason: String, guidance: String },
}

/// Final outcome of processing one agent during `teemiao agents update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// Dry-run preview of a verified update plan (zero mutations performed).
    DryRun {
        command: String,
        target: String,
        current_version: Option<String>,
    },
    /// Active version changed and was verified by a post-update probe.
    Updated {
        before: String,
        after: String,
        path: PathBuf,
        diagnostics: Option<String>,
    },
    /// Updater succeeded and positively confirmed the installation is already current.
    UpToDate {
        version: String,
        path: PathBuf,
        diagnostics: Option<String>,
    },
    /// Installation requires manual intervention (ambiguous owner, unsupported manager, or manual-only updater).
    ManualActionRequired {
        reason: String,
        guidance: String,
        diagnostics: Option<String>,
    },
    /// Updater exited zero, but post-update version could not be verified or did not confirm up-to-date.
    Unverified {
        reason: String,
        path: PathBuf,
        diagnostics: Option<String>,
    },
    /// Updater process or installer validation failed.
    Failed {
        reason: String,
        path: PathBuf,
        diagnostics: Option<String>,
    },
    /// Update was interrupted/cancelled before completion.
    Interrupted { path: PathBuf },
}

impl UpdateOutcome {
    /// Returns true if this outcome represents a clean success (exit code 0).
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            UpdateOutcome::DryRun { .. }
                | UpdateOutcome::Updated { .. }
                | UpdateOutcome::UpToDate { .. }
        )
    }

    /// Returns true if the process was interrupted by a termination signal.
    pub fn is_interrupted(&self) -> bool {
        matches!(self, UpdateOutcome::Interrupted { .. })
    }
}

/// Result record pairing a discovered agent with its update outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentUpdateResult {
    pub id: AgentId,
    pub outcome: UpdateOutcome,
}

/// Pure selection of an `UpdatePlan` from a `DiscoveredAgent` record.
pub fn select_update_plan(agent: &DiscoveredAgent) -> UpdatePlan {
    match &agent.owner {
        UpdateOwner::NativeSelfUpdate {
            program,
            args,
            display_command,
        } => UpdatePlan::Command {
            program: program.clone(),
            args: args.iter().map(OsString::from).collect(),
            env_overrides: vec![(OsString::from("CI"), OsString::from("1"))],
            display_command: display_command.clone(),
            target_description: agent.launcher_path.display().to_string(),
        },
        UpdateOwner::NpmGlobal {
            npm_program,
            prefix,
            package,
        } => {
            let pkg_spec = format!("{package}@latest");
            let args = vec![
                OsString::from("install"),
                OsString::from("-g"),
                OsString::from("--prefix"),
                prefix.as_os_str().to_os_string(),
                OsString::from(&pkg_spec),
            ];
            let display_command = format!(
                "{} install -g --prefix {} {}",
                npm_program.display(),
                prefix.display(),
                pkg_spec
            );
            UpdatePlan::Command {
                program: npm_program.clone(),
                args,
                env_overrides: vec![
                    (
                        OsString::from("NPM_CONFIG_UPDATE_NOTIFIER"),
                        OsString::from("false"),
                    ),
                    (OsString::from("NPM_CONFIG_FUND"), OsString::from("false")),
                    (OsString::from("NPM_CONFIG_AUDIT"), OsString::from("false")),
                ],
                display_command,
                target_description: format!(
                    "{} (npm prefix {})",
                    agent.launcher_path.display(),
                    prefix.display()
                ),
            }
        }
        UpdateOwner::HomebrewCask {
            brew_program,
            cask_token,
        } => {
            let args = vec![
                OsString::from("upgrade"),
                OsString::from("--cask"),
                OsString::from(cask_token),
            ];
            let display_command =
                format!("{} upgrade --cask {}", brew_program.display(), cask_token);
            UpdatePlan::Command {
                program: brew_program.clone(),
                args,
                env_overrides: Vec::new(),
                display_command,
                target_description: format!(
                    "{} (Homebrew cask {})",
                    agent.launcher_path.display(),
                    cask_token
                ),
            }
        }
        UpdateOwner::HomebrewFormula {
            brew_program,
            formula,
        } => {
            let args = vec![OsString::from("upgrade"), OsString::from(formula)];
            let display_command = format!("{} upgrade {}", brew_program.display(), formula);
            UpdatePlan::Command {
                program: brew_program.clone(),
                args,
                env_overrides: Vec::new(),
                display_command,
                target_description: format!(
                    "{} (Homebrew formula {})",
                    agent.launcher_path.display(),
                    formula
                ),
            }
        }
        UpdateOwner::WinGet {
            winget_program,
            package_id,
        } => {
            let args = vec![
                OsString::from("upgrade"),
                OsString::from("--exact"),
                OsString::from("--id"),
                OsString::from(package_id),
                OsString::from("--accept-source-agreements"),
                OsString::from("--accept-package-agreements"),
                OsString::from("--disable-interactivity"),
            ];
            let display_command = format!(
                "{} upgrade --exact --id {} --accept-source-agreements --accept-package-agreements --disable-interactivity",
                winget_program.display(),
                package_id
            );
            UpdatePlan::Command {
                program: winget_program.clone(),
                args,
                env_overrides: Vec::new(),
                display_command,
                target_description: format!(
                    "{} (WinGet {})",
                    agent.launcher_path.display(),
                    package_id
                ),
            }
        }
        UpdateOwner::CodexStandalone {
            bin_dir,
            codex_home,
            is_windows,
        } => {
            let installer_url = if *is_windows {
                CODEX_WINDOWS_INSTALLER_URL
            } else {
                CODEX_UNIX_INSTALLER_URL
            };
            let display_command = if *is_windows {
                format!(
                    "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File <verified-temp-install.ps1> (from {installer_url})"
                )
            } else {
                format!("sh <verified-temp-install.sh> (from {installer_url})")
            };
            UpdatePlan::CodexStandalone {
                installer_url,
                bin_dir: bin_dir.clone(),
                codex_home: codex_home.clone(),
                is_windows: *is_windows,
                display_command,
                target_description: format!(
                    "{} (CODEX_INSTALL_DIR={}, CODEX_HOME={})",
                    agent.launcher_path.display(),
                    bin_dir.display(),
                    codex_home.display()
                ),
            }
        }
        UpdateOwner::ManualActionRequired { reason, guidance } => {
            UpdatePlan::ManualActionRequired {
                reason: reason.clone(),
                guidance: guidance.clone(),
            }
        }
    }
}

/// Executes or dry-runs an update for a single discovered agent and verifies the post-update state.
pub fn execute_agent_update(
    agent: &DiscoveredAgent,
    dry_run: bool,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> AgentUpdateResult {
    let plan = select_update_plan(agent);

    match plan {
        UpdatePlan::ManualActionRequired { reason, guidance } => AgentUpdateResult {
            id: agent.id,
            outcome: UpdateOutcome::ManualActionRequired {
                reason,
                guidance,
                diagnostics: None,
            },
        },
        UpdatePlan::Command {
            program,
            args,
            env_overrides,
            display_command,
            target_description,
        } => {
            if dry_run {
                return AgentUpdateResult {
                    id: agent.id,
                    outcome: UpdateOutcome::DryRun {
                        command: display_command,
                        target: target_description,
                        current_version: agent.version.clone(),
                    },
                };
            }

            if env.is_windows && is_unsafe_windows_batch_command(&program, &args) {
                return AgentUpdateResult {
                    id: agent.id,
                    outcome: UpdateOutcome::ManualActionRequired {
                        reason: "updater batch path or arguments on Windows contain unsafe shell metacharacters"
                            .to_string(),
                        guidance:
                            "Reinstall the updater or package manager in a path without cmd.exe metacharacters."
                                .to_string(),
                        diagnostics: None,
                    },
                };
            }

            let request = ProcessRequest {
                program,
                args,
                env_overrides,
                timeout: None,
                max_output_bytes: UPDATE_MAX_OUTPUT_BYTES,
            };

            match run_checked_update(&request, runner) {
                Ok(output) => {
                    let outcome = evaluate_post_update(agent, &output, env, runner, cache);
                    AgentUpdateResult {
                        id: agent.id,
                        outcome,
                    }
                }
                Err(err) => AgentUpdateResult {
                    id: agent.id,
                    outcome: UpdateOutcome::Failed {
                        reason: err.to_string(),
                        path: agent.launcher_path.clone(),
                        diagnostics: None,
                    },
                },
            }
        }
        UpdatePlan::CodexStandalone {
            installer_url,
            bin_dir,
            codex_home,
            is_windows: _,
            display_command,
            target_description,
        } => {
            if dry_run {
                return AgentUpdateResult {
                    id: agent.id,
                    outcome: UpdateOutcome::DryRun {
                        command: display_command,
                        target: target_description,
                        current_version: agent.version.clone(),
                    },
                };
            }

            let outcome = execute_codex_standalone_update(
                agent,
                installer_url,
                &bin_dir,
                &codex_home,
                env,
                runner,
                cache,
            );
            AgentUpdateResult {
                id: agent.id,
                outcome,
            }
        }
    }
}

fn execute_codex_standalone_update(
    agent: &DiscoveredAgent,
    installer_url: &'static str,
    bin_dir: &Path,
    codex_home: &Path,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> UpdateOutcome {
    let is_windows = env.is_windows;
    // Re-verify before downloading that the selected launcher still resides in `bin_dir`
    // and that no conflicting target directory is configured.
    let Some(launcher_parent) = agent.launcher_path.parent() else {
        return UpdateOutcome::ManualActionRequired {
            reason: "active Codex binary has no parent directory".to_string(),
            guidance: "Reinstall Codex using the official standalone installer.".to_string(),
            diagnostics: None,
        };
    };

    if !paths_refer_to_same_dir(launcher_parent, bin_dir, is_windows) {
        return UpdateOutcome::ManualActionRequired {
            reason: format!(
                "installer target directory ({}) does not match active Codex launcher directory ({})",
                bin_dir.display(),
                launcher_parent.display()
            ),
            guidance: "Update Codex manually so the active PATH installation is not shadowed."
                .to_string(),
            diagnostics: None,
        };
    }

    if !agent.launcher_path.is_file() {
        return UpdateOutcome::Failed {
            reason: format!(
                "active Codex executable ({}) disappeared before update could start",
                agent.launcher_path.display()
            ),
            path: agent.launcher_path.clone(),
            diagnostics: None,
        };
    }

    let Some((curl_bin, _)) = resolve_executable_on_path("curl", env) else {
        return UpdateOutcome::Failed {
            reason:
                "'curl' is required on PATH to download the official Codex standalone installer"
                    .to_string(),
            path: agent.launcher_path.clone(),
            diagnostics: None,
        };
    };

    let temp_guard = match TempInstallerDir::create() {
        Ok(guard) => guard,
        Err(err) => {
            return UpdateOutcome::Failed {
                reason: format!("failed to create private temporary directory: {err}"),
                path: agent.launcher_path.clone(),
                diagnostics: None,
            };
        }
    };

    let script_filename = if is_windows {
        "codex-install.ps1"
    } else {
        "codex-install.sh"
    };
    let script_path = temp_guard.dir.join(script_filename);

    let download_request = ProcessRequest {
        program: curl_bin,
        args: vec![
            // Must be first: prevent ~/.curlrc from weakening TLS or changing outputs.
            OsString::from("-q"),
            OsString::from("--fail"),
            OsString::from("--silent"),
            OsString::from("--show-error"),
            OsString::from("--location"),
            OsString::from("--proto"),
            OsString::from("=https"),
            OsString::from("--proto-redir"),
            OsString::from("=https"),
            OsString::from("--tlsv1.2"),
            OsString::from("--max-filesize"),
            OsString::from(MAX_INSTALLER_SCRIPT_BYTES.to_string()),
            OsString::from("--connect-timeout"),
            OsString::from("15"),
            OsString::from("--max-time"),
            OsString::from("120"),
            OsString::from("-o"),
            script_path.as_os_str().to_os_string(),
            OsString::from(installer_url),
        ],
        env_overrides: Vec::new(),
        timeout: Some(Duration::from_secs(125)),
        max_output_bytes: UPDATE_MAX_OUTPUT_BYTES,
    };

    let download_output = match run_checked_update(&download_request, runner) {
        Ok(out) => out,
        Err(err) => {
            return UpdateOutcome::Failed {
                reason: format!("failed to invoke curl for Codex installer download: {err}"),
                path: agent.launcher_path.clone(),
                diagnostics: None,
            };
        }
    };

    if download_output.interrupted {
        return UpdateOutcome::Interrupted {
            path: agent.launcher_path.clone(),
        };
    }

    let download_diag = format_process_diagnostics(&download_output);
    if !download_output.success {
        return UpdateOutcome::Failed {
            reason: format!("failed to download official Codex installer from {installer_url}"),
            path: agent.launcher_path.clone(),
            diagnostics: download_diag,
        };
    }

    if let Err(validation_err) = validate_downloaded_codex_installer(&script_path, is_windows) {
        return UpdateOutcome::Failed {
            reason: format!("downloaded Codex installer failed validation: {validation_err}"),
            path: agent.launcher_path.clone(),
            diagnostics: download_diag,
        };
    }

    let (interpreter, interpreter_args) =
        match resolve_codex_installer_interpreter(&script_path, is_windows, env) {
            Ok(pair) => pair,
            Err(reason) => {
                return UpdateOutcome::Failed {
                    reason,
                    path: agent.launcher_path.clone(),
                    diagnostics: download_diag,
                };
            }
        };

    let install_request = ProcessRequest {
        program: interpreter,
        args: interpreter_args,
        env_overrides: vec![
            (OsString::from("CODEX_NON_INTERACTIVE"), OsString::from("1")),
            (
                OsString::from("CODEX_INSTALL_DIR"),
                bin_dir.as_os_str().to_os_string(),
            ),
            (
                OsString::from("CODEX_HOME"),
                codex_home.as_os_str().to_os_string(),
            ),
            (OsString::from("CI"), OsString::from("1")),
        ],
        timeout: None,
        max_output_bytes: UPDATE_MAX_OUTPUT_BYTES,
    };

    match run_checked_update(&install_request, runner) {
        Ok(install_output) => evaluate_post_update(agent, &install_output, env, runner, cache),
        Err(err) => UpdateOutcome::Failed {
            reason: format!("failed to execute Codex standalone installer: {err}"),
            path: agent.launcher_path.clone(),
            diagnostics: download_diag,
        },
    }
}

fn run_checked_update(
    request: &ProcessRequest,
    runner: &dyn ProcessRunner,
) -> Result<ProcessOutput, ProcessError> {
    request.validate_batch_command()?;
    runner.run_update(request)
}

fn validate_downloaded_codex_installer(script_path: &Path, is_windows: bool) -> Result<(), String> {
    let sym_meta =
        fs::symlink_metadata(script_path).map_err(|e| format!("installer file missing ({e})"))?;
    if sym_meta.file_type().is_symlink() {
        return Err("refusing to execute symlinked temporary installer file".to_string());
    }
    if !sym_meta.is_file() {
        return Err("downloaded installer path is not a regular file".to_string());
    }

    if sym_meta.len() > MAX_INSTALLER_SCRIPT_BYTES {
        return Err("downloaded installer exceeds the 1 MiB size limit".to_string());
    }
    let file =
        fs::File::open(script_path).map_err(|e| format!("could not open installer ({e})"))?;
    // Bound the read too, even if the file grew after the metadata check.
    let mut bytes = Vec::new();
    file.take(MAX_INSTALLER_SCRIPT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("could not read installer ({e})"))?;
    if bytes.len() as u64 > MAX_INSTALLER_SCRIPT_BYTES {
        return Err("downloaded installer exceeds the 1 MiB size limit".to_string());
    }
    if bytes.is_empty() {
        return Err("downloaded installer file is empty".to_string());
    }
    let content = std::str::from_utf8(&bytes)
        .map_err(|_| "downloaded installer is not valid UTF-8 text".to_string())?;
    let trimmed = content.trim_start_matches('\u{feff}').trim();

    if is_windows {
        if !trimmed.contains("CODEX_INSTALL_DIR")
            || !trimmed.contains("CODEX_HOME")
            || !trimmed.contains("installed successfully")
        {
            return Err(
                "downloaded Windows installer is incomplete or missing expected Codex target markers"
                    .to_string(),
            );
        }
    } else {
        if !trimmed.starts_with("#!/bin/sh") {
            return Err("downloaded Unix installer does not start with '#!/bin/sh'".to_string());
        }
        if !trimmed.contains("CODEX_INSTALL_DIR")
            || !trimmed.contains("CODEX_HOME")
            || !trimmed.contains("installed successfully")
        {
            return Err(
                "downloaded Unix installer is incomplete or missing expected Codex target markers"
                    .to_string(),
            );
        }
    }

    Ok(())
}

fn resolve_codex_installer_interpreter(
    script_path: &Path,
    is_windows: bool,
    env: &DiscoveryEnv,
) -> Result<(PathBuf, Vec<OsString>), String> {
    if is_windows {
        let ps = resolve_executable_on_path("powershell", env)
            .or_else(|| resolve_executable_on_path("pwsh", env))
            .map(|(p, _)| p)
            .ok_or_else(|| {
                "'powershell' was not found on PATH to run the Codex Windows installer".to_string()
            })?;
        let args = vec![
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-ExecutionPolicy"),
            OsString::from("Bypass"),
            OsString::from("-File"),
            script_path.as_os_str().to_os_string(),
        ];
        Ok((ps, args))
    } else {
        let bin_sh = PathBuf::from("/bin/sh");
        let sh = bin_sh
            .is_file()
            .then_some(bin_sh)
            .or_else(|| resolve_executable_on_path("sh", env).map(|(p, _)| p))
            .ok_or_else(|| "'sh' was not found to run the Codex Unix installer".to_string())?;
        let args = vec![script_path.as_os_str().to_os_string()];
        Ok((sh, args))
    }
}

struct TempInstallerDir {
    dir: PathBuf,
}

impl TempInstallerDir {
    fn create() -> std::io::Result<Self> {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!("teemiao-codex-install-{pid}-{nanos}-{seq}"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new().mode(0o700).create(&dir)?;
        }
        #[cfg(not(unix))]
        {
            fs::create_dir(&dir)?;
        }

        Ok(Self { dir })
    }
}

impl Drop for TempInstallerDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const WINGET_UPDATE_NOT_APPLICABLE_EXIT_CODE: i32 = 0x8A15_002B_u32 as i32;

/// Evaluates updater output and re-probes the active installation on `PATH` to
/// classify the final update outcome according to FR-008 and FR-009.
fn evaluate_post_update(
    before_agent: &DiscoveredAgent,
    output: &ProcessOutput,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
) -> UpdateOutcome {
    if output.interrupted || matches!(output.status_code, Some(130 | 143)) {
        return UpdateOutcome::Interrupted {
            path: before_agent.launcher_path.clone(),
        };
    }

    let combined_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let diagnostics = format_process_diagnostics(output);

    if output.timed_out || output.stdout_truncated || output.stderr_truncated {
        return UpdateOutcome::Unverified {
            reason: "updater output is incomplete (execution/output collection timed out or output exceeded its size limit)".to_string(),
            path: before_agent.launcher_path.clone(),
            diagnostics,
        };
    }

    if let Some((reason, guidance)) = detect_manual_only_updater_output(&combined_output) {
        return UpdateOutcome::ManualActionRequired {
            reason,
            guidance,
            diagnostics,
        };
    }

    let is_winget_no_applicable_upgrade = matches!(before_agent.owner, UpdateOwner::WinGet { .. })
        && output.status_code == Some(WINGET_UPDATE_NOT_APPLICABLE_EXIT_CODE)
        && updater_output_confirms_up_to_date(
            &combined_output,
            before_agent.version.as_deref().unwrap_or(""),
            before_agent.id,
            &before_agent.owner,
        );

    if !output.success && !is_winget_no_applicable_upgrade {
        let status_label = output
            .status_code
            .map(|c| format!("updater exited with code {c}"))
            .unwrap_or_else(|| "updater terminated abnormally".to_string());
        return UpdateOutcome::Failed {
            reason: status_label,
            path: before_agent.launcher_path.clone(),
            diagnostics,
        };
    }

    // Re-resolve and re-probe the active executable on PATH because launchers or symlinks may change.
    let Some(after_agent) = discover_agent(before_agent.id, env, runner, cache) else {
        return UpdateOutcome::Failed {
            reason: "executable was no longer found on PATH after updater completed".to_string(),
            path: before_agent.launcher_path.clone(),
            diagnostics,
        };
    };

    if !rediscovered_matches_selected_installation(before_agent, &after_agent, env.is_windows) {
        return UpdateOutcome::Failed {
            reason: "selected installation executable was no longer found on PATH after updater completed".to_string(),
            path: before_agent.launcher_path.clone(),
            diagnostics,
        };
    }

    let Some(after_version) = after_agent.version else {
        return UpdateOutcome::Unverified {
            reason: "post-update version probe failed or returned an unreadable version"
                .to_string(),
            path: after_agent.launcher_path,
            diagnostics,
        };
    };

    match &before_agent.version {
        Some(before_version) if output.success && before_version != &after_version => {
            UpdateOutcome::Updated {
                before: before_version.clone(),
                after: after_version,
                path: after_agent.launcher_path,
                diagnostics,
            }
        }
        Some(before_version) if before_version == &after_version => {
            if updater_output_confirms_up_to_date(
                &combined_output,
                &after_version,
                before_agent.id,
                &before_agent.owner,
            ) {
                UpdateOutcome::UpToDate {
                    version: after_version,
                    path: after_agent.launcher_path,
                    diagnostics,
                }
            } else {
                UpdateOutcome::Unverified {
                    reason: "active version did not change and updater output did not confirm up-to-date status".to_string(),
                    path: after_agent.launcher_path,
                    diagnostics,
                }
            }
        }
        Some(_) => UpdateOutcome::Unverified {
            reason: "updater returned a non-zero no-op status while the active version changed"
                .to_string(),
            path: after_agent.launcher_path,
            diagnostics,
        },
        None => {
            if output.success
                && updater_output_confirms_updated(
                    &combined_output,
                    &after_version,
                    before_agent.id,
                    &before_agent.owner,
                )
            {
                UpdateOutcome::Updated {
                    before: "unknown".to_string(),
                    after: after_version,
                    path: after_agent.launcher_path,
                    diagnostics,
                }
            } else if updater_output_confirms_up_to_date(
                &combined_output,
                &after_version,
                before_agent.id,
                &before_agent.owner,
            ) {
                UpdateOutcome::UpToDate {
                    version: after_version,
                    path: after_agent.launcher_path,
                    diagnostics,
                }
            } else {
                UpdateOutcome::Unverified {
                    reason: "initial version was unknown and updater output did not confirm update outcome".to_string(),
                    path: after_agent.launcher_path,
                    diagnostics,
                }
            }
        }
    }
}

fn rediscovered_matches_selected_installation(
    before: &DiscoveredAgent,
    after: &DiscoveredAgent,
    is_windows: bool,
) -> bool {
    if paths_refer_to_same_dir(&before.launcher_path, &after.launcher_path, is_windows) {
        return true;
    }
    match (&before.owner, &after.owner) {
        (UpdateOwner::WinGet { package_id: a, .. }, UpdateOwner::WinGet { package_id: b, .. }) => {
            a.eq_ignore_ascii_case(b)
        }
        _ => {
            let Some(before_dir) = before.launcher_path.parent() else {
                return false;
            };
            let Some(after_dir) = after.launcher_path.parent() else {
                return false;
            };
            paths_refer_to_same_dir(before_dir, after_dir, is_windows)
                && before.launcher_path.exists()
        }
    }
}

fn detect_manual_only_updater_output(text: &str) -> Option<(String, String)> {
    let lower = text.to_ascii_lowercase();
    let manual_markers = [
        "please update manually",
        "update manually",
        "manual update required",
        "manual action required",
        "cannot auto-update",
        "auto-update is disabled",
        "auto-updates are disabled",
        "self-update is disabled",
        "self-update is not supported",
        "skipping self-update",
        "skipping auto-update",
        "managed externally",
        "managed by ",
        "managed installation",
        "no update applied",
        "use your package manager",
        "run pip install",
        "run uv tool upgrade",
        "installed via homebrew",
        "installed via apt",
        "installed via nix",
    ];
    for marker in manual_markers {
        if lower.contains(marker) {
            let first_line = text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("updater reported that manual action is required");
            return Some((
                "updater reported that automatic updating is unsupported for this installation"
                    .to_string(),
                first_line.to_string(),
            ));
        }
    }
    None
}

fn updater_output_has_negation_or_failure(text: &str, owner: &UpdateOwner) -> bool {
    let negative_markers = [
        "not up to date",
        "not up-to-date",
        "not already up to date",
        "not already up-to-date",
        "isn't up to date",
        "isn't already up to date",
        "is not up to date",
        "is not already up to date",
        "not on the latest",
        "not at the latest",
        "not running the latest",
        "not the latest",
        "update aborted",
        "upgrade aborted",
        "aborted",
        "aborting",
        "cancelled",
        "canceled",
        "update check failed",
        "check failed",
        "update failed",
        "upgrade failed",
        "failed to ",
        "failed:",
        "error:",
        "error ",
        "fatal:",
        "cannot ",
        "can't ",
        "could not ",
        "couldn't ",
        "unable to ",
    ];
    text.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        // The official Codex installer can complete successfully while keeping
        // old releases. Exempt only these complete cleanup warnings; callers
        // still require affirmative output and a verified active installation.
        if matches!(owner, UpdateOwner::CodexStandalone { .. })
            && matches!(
                lower.trim(),
                "warning: could not safely inspect old codex releases; skipped remaining cleanup."
                    | "warning: could not record installation selection; pruning will be skipped."
            )
        {
            return false;
        }
        negative_markers.iter().any(|m| lower.contains(m))
    })
}

fn line_has_negation_token(line_lower: &str) -> bool {
    for word in line_lower.split(|c: char| !c.is_ascii_alphabetic() && c != '\'') {
        if matches!(
            word,
            "not"
                | "never"
                | "isn't"
                | "aren't"
                | "wasn't"
                | "weren't"
                | "didn't"
                | "doesn't"
                | "won't"
                | "abort"
                | "aborted"
                | "aborting"
                | "cancel"
                | "cancelled"
                | "canceled"
                | "fail"
                | "failed"
                | "failure"
                | "error"
                | "fatal"
                | "checking"
                | "whether"
                | "if"
                | "when"
                | "while"
                | "verifying"
                | "querying"
                | "searching"
                | "waiting"
                | "trying"
                | "attempting"
        ) {
            return true;
        }
    }
    false
}

fn normalize_affirmative_status_clause(raw_line: &str, ver_lower: &str) -> Option<String> {
    let trimmed = raw_line.trim().to_ascii_lowercase();
    if trimmed.is_empty()
        || trimmed.ends_with("...")
        || trimmed.ends_with('…')
        || trimmed.ends_with('?')
        || trimmed.contains(';')
        || line_has_negation_token(&trimmed)
    {
        return None;
    }

    let mut clause = trimmed.as_str();
    for prefix in [
        "warning: ",
        "note: ",
        "info: ",
        "status: ",
        "==> ",
        "✔ ",
        "✓ ",
        "[ok] ",
        "success: ",
    ] {
        if let Some(rest) = clause.strip_prefix(prefix) {
            clause = rest.trim_start();
            break;
        }
    }

    clause = clause.trim_end_matches(['.', '!']).trim();
    for suffix in [format!(" ({ver_lower})"), format!(" (v{ver_lower})")] {
        if let Some(rest) = clause.strip_suffix(&suffix) {
            clause = rest.trim_end();
            break;
        }
    }

    if clause.is_empty() {
        None
    } else {
        Some(clause.to_string())
    }
}

fn agent_subject_aliases(agent_id: AgentId) -> &'static [&'static str] {
    match agent_id {
        AgentId::Codex => &["codex", "codex cli", "codex-cli", "@openai/codex"],
        AgentId::Antigravity => &["antigravity", "antigravity cli", "agy"],
        AgentId::OpenCode => &["opencode", "opencode cli"],
        AgentId::Kimi => &["kimi", "kimi cli", "kimi-cli"],
        AgentId::Cursor => &[
            "cursor",
            "cursor agent",
            "cursor-agent",
            "cursor cli",
            "agent",
        ],
        AgentId::ClaudeCode => &[
            "claude",
            "claude code",
            "claude-code",
            "claude cli",
            "@anthropic-ai/claude-code",
        ],
        AgentId::Qoder => &[
            "qoder",
            "qoder cli",
            "qoder-cli",
            "qodercli",
            "@qoder-ai/qodercli",
        ],
    }
}

fn matches_subject_and_predicate(
    clause: &str,
    subjects: &[&str],
    ver_lower: &str,
    predicates: &[&str],
) -> bool {
    let ver_space = format!("{ver_lower} ");
    let v_ver_space = format!("v{ver_lower} ");
    for subj in subjects {
        if let Some(after_subj) = clause.strip_prefix(subj)
            && let Some(rest) = after_subj.strip_prefix(' ')
        {
            let rest = rest.trim_start();
            let pred_candidate = rest
                .strip_prefix(&ver_space)
                .or_else(|| rest.strip_prefix(&v_ver_space))
                .unwrap_or(rest)
                .trim_start();
            if predicates.contains(&pred_candidate) {
                return true;
            }
        }
    }
    false
}

fn is_duration_clause(rest: &str) -> bool {
    !rest.is_empty()
        && rest
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'm' | 's' | ' '))
}

const NATIVE_AFFIRMATIVE_PREDICATES: &[&str] = &[
    "is already up to date",
    "is already up-to-date",
    "already up to date",
    "already up-to-date",
    "is up to date",
    "is up-to-date",
    "is already on the latest version",
    "already on the latest version",
    "is already on the latest",
    "already on the latest",
    "is already at the latest version",
    "already at the latest version",
    "is already at the latest",
    "already at the latest",
    "is already running the latest version",
    "already running the latest version",
    "is already running the latest",
    "already running the latest",
    "is the latest version",
    "is already the latest version",
];

const HOMEBREW_AFFIRMATIVE_PREDICATES: &[&str] = &[
    "is already installed and up-to-date",
    "is already installed and up to date",
    "already installed and up-to-date",
    "already installed and up to date",
    "is already installed",
    "already installed",
    "is up-to-date",
    "is up to date",
];

fn is_homebrew_not_upgrading_latest_installed(raw_line: &str, pkg_lower: &str) -> bool {
    let trimmed = raw_line.trim().to_ascii_lowercase();
    let clause = trimmed
        .strip_prefix("warning: ")
        .or_else(|| trimmed.strip_prefix("==> "))
        .unwrap_or(&trimmed)
        .trim_end_matches(['.', '!'])
        .trim();
    let s_plain = format!("not upgrading {pkg_lower}, the latest version is already installed");
    let s_quoted1 = format!("not upgrading '{pkg_lower}', the latest version is already installed");
    let s_quoted2 =
        format!("not upgrading \"{pkg_lower}\", the latest version is already installed");
    clause == s_plain || clause == s_quoted1 || clause == s_quoted2
}

fn updater_output_confirms_up_to_date(
    text: &str,
    active_version: &str,
    agent_id: AgentId,
    owner: &UpdateOwner,
) -> bool {
    if updater_output_has_negation_or_failure(text, owner) {
        return false;
    }

    let ver_lower = active_version.to_ascii_lowercase();
    for raw_line in text.lines() {
        match owner {
            UpdateOwner::HomebrewCask { cask_token, .. }
                if is_homebrew_not_upgrading_latest_installed(
                    raw_line,
                    &cask_token.to_ascii_lowercase(),
                ) =>
            {
                return true;
            }
            UpdateOwner::HomebrewFormula { formula, .. }
                if is_homebrew_not_upgrading_latest_installed(
                    raw_line,
                    &formula.to_ascii_lowercase(),
                ) =>
            {
                return true;
            }
            _ => {}
        }
        let Some(clause) = normalize_affirmative_status_clause(raw_line, &ver_lower) else {
            continue;
        };
        let matched = match owner {
            UpdateOwner::NativeSelfUpdate { .. } => {
                matches!(
                    clause.as_str(),
                    "already up to date"
                        | "already up-to-date"
                        | "up to date"
                        | "up-to-date"
                        | "already on the latest version"
                        | "already at the latest version"
                        | "already running the latest version"
                        | "no update available"
                        | "no updates available"
                ) || matches_subject_and_predicate(
                    &clause,
                    agent_subject_aliases(agent_id),
                    &ver_lower,
                    NATIVE_AFFIRMATIVE_PREDICATES,
                )
            }
            UpdateOwner::NpmGlobal { package, .. } => {
                let pkg_lower = package.to_ascii_lowercase();
                matches!(
                    clause.as_str(),
                    "up to date" | "up-to-date" | "changed 0 packages"
                ) || clause
                    .strip_prefix("up to date in ")
                    .is_some_and(is_duration_clause)
                    || clause
                        .strip_prefix("changed 0 packages in ")
                        .is_some_and(is_duration_clause)
                    || matches_subject_and_predicate(
                        &clause,
                        &[pkg_lower.as_str()],
                        &ver_lower,
                        NATIVE_AFFIRMATIVE_PREDICATES,
                    )
                    || matches_subject_and_predicate(
                        &clause,
                        agent_subject_aliases(agent_id),
                        &ver_lower,
                        NATIVE_AFFIRMATIVE_PREDICATES,
                    )
            }
            UpdateOwner::HomebrewCask { cask_token, .. } => {
                let pkg = cask_token.to_ascii_lowercase();
                let s1 = format!("cask '{pkg}'");
                let s2 = format!("cask \"{pkg}\"");
                let s3 = format!("cask {pkg}");
                let brew_subjects = [pkg.as_str(), s1.as_str(), s2.as_str(), s3.as_str()];
                matches_subject_and_predicate(
                    &clause,
                    &brew_subjects,
                    &ver_lower,
                    HOMEBREW_AFFIRMATIVE_PREDICATES,
                )
            }
            UpdateOwner::HomebrewFormula { formula, .. } => {
                let pkg = formula.to_ascii_lowercase();
                let s1 = format!("formula '{pkg}'");
                let s2 = format!("formula \"{pkg}\"");
                let s3 = format!("formula {pkg}");
                let brew_subjects = [pkg.as_str(), s1.as_str(), s2.as_str(), s3.as_str()];
                matches_subject_and_predicate(
                    &clause,
                    &brew_subjects,
                    &ver_lower,
                    HOMEBREW_AFFIRMATIVE_PREDICATES,
                )
            }
            UpdateOwner::WinGet { .. } => {
                matches!(
                    clause.as_str(),
                    "no available upgrade found"
                        | "no newer package versions are available"
                        | "no applicable update found"
                )
            }
            UpdateOwner::CodexStandalone { .. } => {
                matches!(
                    clause.as_str(),
                    "already up to date"
                        | "already up-to-date"
                        | "codex cli installed successfully"
                ) || matches_subject_and_predicate(
                    &clause,
                    &["codex", "codex cli", "codex-cli"],
                    &ver_lower,
                    &[
                        "installed successfully",
                        "is already installed",
                        "already installed",
                        "is already up to date",
                        "already up to date",
                        "is up to date",
                    ],
                )
            }
            UpdateOwner::ManualActionRequired { .. } => false,
        };
        if matched {
            return true;
        }
    }

    false
}

fn updater_output_confirms_updated(
    text: &str,
    active_version: &str,
    agent_id: AgentId,
    owner: &UpdateOwner,
) -> bool {
    if updater_output_has_negation_or_failure(text, owner) {
        return false;
    }
    let ver_lower = active_version.to_ascii_lowercase();
    let updated_to = format!("updated to {ver_lower}");
    let updated_to_v = format!("updated to v{ver_lower}");
    let upgraded_to = format!("upgraded to {ver_lower}");
    let upgraded_to_v = format!("upgraded to v{ver_lower}");
    let predicates = [
        updated_to.as_str(),
        updated_to_v.as_str(),
        upgraded_to.as_str(),
        upgraded_to_v.as_str(),
        "successfully updated",
        "successfully upgraded",
        "installed successfully",
    ];
    for raw_line in text.lines() {
        let Some(clause) = normalize_affirmative_status_clause(raw_line, &ver_lower) else {
            continue;
        };
        if predicates.contains(&clause.as_str())
            || matches_subject_and_predicate(
                &clause,
                agent_subject_aliases(agent_id),
                &ver_lower,
                &predicates,
            )
            || matches_updated_version_clause(&clause, agent_subject_aliases(agent_id), &ver_lower)
        {
            return true;
        }
    }
    false
}

fn matches_updated_version_clause(clause: &str, subjects: &[&str], ver_lower: &str) -> bool {
    let Some(after_verb) = clause
        .strip_prefix("successfully updated ")
        .or_else(|| clause.strip_prefix("successfully upgraded "))
        .or_else(|| clause.strip_prefix("updated "))
        .or_else(|| clause.strip_prefix("upgraded "))
    else {
        return false;
    };
    let after_verb = after_verb.trim_start();
    let target_v = format!("v{ver_lower}");
    let is_target = |tok: &str| tok == ver_lower || tok == target_v;

    let mut rest_candidates = Vec::new();
    for subj in subjects {
        if let Some(after_subj) = after_verb.strip_prefix(subj)
            && let Some(rest) = after_subj.strip_prefix(' ')
        {
            rest_candidates.push(rest.trim_start());
        }
    }
    if rest_candidates.is_empty()
        && (after_verb.starts_with("from ") || after_verb.starts_with("to "))
    {
        rest_candidates.push(after_verb);
    }

    rest_candidates.into_iter().any(|rest| {
        if let Some(from_rest) = rest.strip_prefix("from ") {
            if let Some((old_raw, new_raw)) = from_rest.split_once(" to ") {
                let old_tok = old_raw.trim();
                let old_ver = old_tok.strip_prefix('v').unwrap_or(old_tok);
                return super::discovery::is_valid_version_token(old_ver)
                    && is_target(new_raw.trim());
            }
            false
        } else if let Some(to_rest) = rest.strip_prefix("to ") {
            is_target(to_rest.trim())
        } else {
            false
        }
    })
}

fn format_process_diagnostics(output: &ProcessOutput) -> Option<String> {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match (stderr.is_empty(), stdout.is_empty()) {
        (true, true) => None,
        (false, true) => Some(stderr),
        (true, false) => Some(stdout),
        (false, false) => Some(format!("{stderr}\n{stdout}")),
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use crate::agents::RealProcessRunner;

    struct NeverRun;
    impl ProcessRunner for NeverRun {
        fn run_probe(&self, _: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
            panic!("unsafe request must not probe")
        }
        fn run_update(&self, _: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
            panic!("unsafe request must not update")
        }
    }

    #[test]
    fn security_every_update_request_checks_batch_arguments() {
        for program in [
            "curl.cmd",
            "powershell.bat",
            "pwsh.cmd",
            "npm.cmd",
            "winget.cmd",
        ] {
            for (path, arg) in [
                (PathBuf::from(program), "temp%USERNAME%/installer"),
                (PathBuf::from("Dir & Evil").join(program), "safe"),
            ] {
                let request = ProcessRequest {
                    program: path,
                    args: vec![arg.into()],
                    env_overrides: vec![],
                    timeout: None,
                    max_output_bytes: 1024,
                };
                assert!(run_checked_update(&request, &NeverRun).is_err());
                let Err(ProcessError::Io { source, .. }) =
                    RealProcessRunner::new().run_update(&request)
                else {
                    panic!("real runner accepted unsafe batch command");
                };
                assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
            }
        }
    }

    #[test]
    fn security_incomplete_updater_output_cannot_confirm_success() {
        let agent = DiscoveredAgent {
            id: AgentId::Codex,
            version: Some("0.167.0".into()),
            launcher_path: PathBuf::from("missing/codex"),
            resolved_path: PathBuf::from("missing/codex"),
            owner: UpdateOwner::WinGet {
                winget_program: PathBuf::from("winget.exe"),
                package_id: "OpenAI.Codex".into(),
            },
            warnings: vec![],
        };
        let env = DiscoveryEnv {
            path: OsString::new(),
            pathext: None,
            home_dir: None,
            local_app_data: None,
            codex_home: None,
            codex_install_dir: None,
            homebrew_prefix: None,
            qoder_cli_bin: None,
            is_windows: false,
        };
        for (timed_out, stdout_truncated, stderr_truncated) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let output = ProcessOutput {
                status_code: Some(0),
                success: !timed_out,
                interrupted: false,
                timed_out,
                stdout: b"Already up to date".to_vec(),
                stderr: vec![],
                stdout_truncated,
                stderr_truncated,
            };
            let outcome = evaluate_post_update(
                &agent,
                &output,
                &env,
                &NeverRun,
                &mut PackageManagerCache::new(),
            );
            assert!(
                matches!(outcome, UpdateOutcome::Unverified { ref reason, .. } if reason.contains("incomplete")),
                "{outcome:?}"
            );
        }
    }

    #[test]
    fn security_installer_size_is_bounded() {
        let temp = TempInstallerDir::create().unwrap();
        let path = temp.dir.join("installer.sh");
        let mut content =
            b"#!/bin/sh\n# CODEX_HOME CODEX_INSTALL_DIR installed successfully\n".to_vec();
        content.resize(1024 * 1024 + 1, b' ');
        fs::write(&path, &content).unwrap();
        assert!(validate_downloaded_codex_installer(&path, false).is_err());
        content.pop();
        fs::write(&path, &content).unwrap();
        assert!(validate_downloaded_codex_installer(&path, false).is_ok());
    }

    #[test]
    fn qoder_updater_output_confirms_from_old_to_new_update() {
        let owner = UpdateOwner::NativeSelfUpdate {
            program: std::path::PathBuf::from("/usr/local/bin/qodercli"),
            args: vec!["update".to_string()],
            display_command: "qodercli update".to_string(),
        };
        assert!(updater_output_confirms_updated(
            "Checking for updates...\nSuccessfully updated Qoder CLI from 1.1.87 to 1.1.88.",
            "1.1.88",
            AgentId::Qoder,
            &owner,
        ));
        assert!(updater_output_confirms_updated(
            "Successfully updated Qoder CLI to v1.1.88.",
            "1.1.88",
            AgentId::Qoder,
            &owner,
        ));
        assert!(!updater_output_confirms_updated(
            "Successfully updated Qoder CLI from 1.1.87 to 1.1.89.",
            "1.1.88",
            AgentId::Qoder,
            &owner,
        ));
        assert!(!updater_output_confirms_updated(
            "Successfully updated Codex CLI from 1.1.87 to 1.1.88.",
            "1.1.88",
            AgentId::Qoder,
            &owner,
        ));
        assert!(!updater_output_confirms_updated(
            "Unable to determine installation method.",
            "1.1.88",
            AgentId::Qoder,
            &owner,
        ));
        assert!(!updater_output_confirms_updated(
            "Successfully updated Qoder CLI from 1.1.87 to 1.1.88.\nUnable to determine installation method.",
            "1.1.88",
            AgentId::Qoder,
            &owner,
        ));
        assert!(!updater_output_confirms_updated(
            "Successfully updated Qoder CLI from 1.1.87 to 1.1.88...",
            "1.1.88",
            AgentId::Qoder,
            &owner,
        ));
    }
}
