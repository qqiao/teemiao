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
    is_unsafe_windows_cmd_shim, paths_refer_to_same_dir, resolve_executable_on_path,
};
use crate::agents::{AgentId, ProcessOutput, ProcessRequest, ProcessRunner};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Fixed official HTTPS URL for the Unix (macOS / Linux) Codex standalone installer.
pub const CODEX_UNIX_INSTALLER_URL: &str = "https://chatgpt.com/codex/install.sh";

/// Fixed official HTTPS URL for the Windows PowerShell Codex standalone installer.
pub const CODEX_WINDOWS_INSTALLER_URL: &str = "https://chatgpt.com/codex/install.ps1";

/// Maximum bytes captured per stream (stdout/stderr) during an update execution.
pub const UPDATE_MAX_OUTPUT_BYTES: usize = 256 * 1024;

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
                env_overrides: vec![(
                    OsString::from("HOMEBREW_NO_AUTO_UPDATE"),
                    OsString::from("1"),
                )],
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
                env_overrides: vec![(
                    OsString::from("HOMEBREW_NO_AUTO_UPDATE"),
                    OsString::from("1"),
                )],
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

            if env.is_windows && is_unsafe_windows_cmd_shim(&program) {
                return AgentUpdateResult {
                    id: agent.id,
                    outcome: UpdateOutcome::ManualActionRequired {
                        reason: "updater shim path on Windows contains unsafe shell metacharacters"
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

            match runner.run_update(&request) {
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
            OsString::from("--fail"),
            OsString::from("--silent"),
            OsString::from("--show-error"),
            OsString::from("--location"),
            OsString::from("--proto"),
            OsString::from("=https"),
            OsString::from("--tlsv1.2"),
            OsString::from("-o"),
            script_path.as_os_str().to_os_string(),
            OsString::from(installer_url),
        ],
        env_overrides: Vec::new(),
        timeout: None,
        max_output_bytes: UPDATE_MAX_OUTPUT_BYTES,
    };

    let download_output = match runner.run_update(&download_request) {
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

    match runner.run_update(&install_request) {
        Ok(install_output) => evaluate_post_update(agent, &install_output, env, runner, cache),
        Err(err) => UpdateOutcome::Failed {
            reason: format!("failed to execute Codex standalone installer: {err}"),
            path: agent.launcher_path.clone(),
            diagnostics: download_diag,
        },
    }
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

    let bytes = fs::read(script_path).map_err(|e| format!("could not read installer ({e})"))?;
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
        let sh = resolve_executable_on_path("sh", env)
            .map(|(p, _)| p)
            .or_else(|| {
                let bin_sh = PathBuf::from("/bin/sh");
                bin_sh.is_file().then_some(bin_sh)
            })
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

    if let Some((reason, guidance)) = detect_manual_only_updater_output(&combined_output) {
        return UpdateOutcome::ManualActionRequired {
            reason,
            guidance,
            diagnostics,
        };
    }

    if !output.success {
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

    let Some(after_version) = after_agent.version else {
        return UpdateOutcome::Unverified {
            reason: "post-update version probe failed or returned an unreadable version"
                .to_string(),
            path: after_agent.launcher_path,
            diagnostics,
        };
    };

    match &before_agent.version {
        Some(before_version) if before_version != &after_version => UpdateOutcome::Updated {
            before: before_version.clone(),
            after: after_version,
            path: after_agent.launcher_path,
            diagnostics,
        },
        Some(_same_version) => {
            if updater_output_confirms_up_to_date(
                &combined_output,
                &after_version,
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
        None => {
            if updater_output_confirms_updated(&combined_output, &after_version) {
                UpdateOutcome::Updated {
                    before: "unknown".to_string(),
                    after: after_version,
                    path: after_agent.launcher_path,
                    diagnostics,
                }
            } else if updater_output_confirms_up_to_date(
                &combined_output,
                &after_version,
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

fn updater_output_has_negation_or_failure(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
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
    negative_markers.iter().any(|m| lower.contains(m))
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
        ) {
            return true;
        }
    }
    false
}

fn line_mentions_token(line_lower: &str, token_lower: &str) -> bool {
    if token_lower.is_empty() {
        return false;
    }
    for (idx, _) in line_lower.match_indices(token_lower) {
        let before_ok = idx == 0
            || !line_lower[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '@'));
        let after_idx = idx + token_lower.len();
        let after_ok = after_idx == line_lower.len()
            || !line_lower[after_idx..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '@'));
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn line_has_unrelated_up_to_date_subject(line_lower: &str) -> bool {
    let unrelated_subjects = [
        "dependencies",
        "dependency",
        "submodules",
        "submodule",
        "cache",
        "configuration",
        "config",
        "database",
        "plugins",
        "plugin",
        "extensions",
        "extension",
    ];
    unrelated_subjects.iter().any(|subj| {
        line_lower.starts_with(subj)
            || line_lower.contains(&format!("{subj} are up to date"))
            || line_lower.contains(&format!("{subj} is up to date"))
            || line_lower.contains(&format!("{subj} is already"))
            || line_lower.contains(&format!("warning: {subj} "))
    })
}

fn updater_output_confirms_up_to_date(
    text: &str,
    active_version: &str,
    owner: &UpdateOwner,
) -> bool {
    if updater_output_has_negation_or_failure(text) {
        return false;
    }

    let ver_lower = active_version.to_ascii_lowercase();
    for raw_line in text.lines() {
        let line = raw_line.trim().to_ascii_lowercase();
        if line.is_empty() || line_has_unrelated_up_to_date_subject(&line) {
            continue;
        }
        let matched = match owner {
            UpdateOwner::NativeSelfUpdate { .. } => {
                const NATIVE_MARKERS: [&str; 10] = [
                    "already up to date",
                    "already up-to-date",
                    "is up to date",
                    "is up-to-date",
                    "already on the latest",
                    "already at the latest",
                    "already running the latest",
                    "is the latest version",
                    "no update available",
                    "no updates available",
                ];
                !line_has_negation_token(&line) && NATIVE_MARKERS.iter().any(|m| line.contains(m))
            }
            UpdateOwner::NpmGlobal { .. } => {
                !line_has_negation_token(&line)
                    && (line.contains("up to date")
                        || line.contains("up-to-date")
                        || line.contains("changed 0 packages"))
            }
            UpdateOwner::HomebrewCask { cask_token, .. } => {
                let cask_lower = cask_token.to_ascii_lowercase();
                line_mentions_token(&line, &cask_lower)
                    && (line.contains(&format!("{cask_lower} {ver_lower} is already installed"))
                        || line.contains("already installed and up-to-date")
                        || line.contains("already installed")
                        || line.contains("up-to-date")
                        || line.contains("up to date")
                        || line.contains("not upgraded"))
            }
            UpdateOwner::HomebrewFormula { formula, .. } => {
                let formula_lower = formula.to_ascii_lowercase();
                line_mentions_token(&line, &formula_lower)
                    && (line.contains(&format!("{formula_lower} {ver_lower} already installed"))
                        || line.contains("already installed and up-to-date")
                        || line.contains("already installed")
                        || line.contains("up-to-date")
                        || line.contains("up to date")
                        || line.contains("not upgraded"))
            }
            UpdateOwner::WinGet { .. } => {
                !line_has_negation_token(&line)
                    && (line.contains("no available upgrade found")
                        || line.contains("no newer package versions are available"))
            }
            UpdateOwner::CodexStandalone { .. } => {
                !line_has_negation_token(&line)
                    && (line.contains(&format!("codex cli {ver_lower} installed successfully"))
                        || (line_mentions_token(&line, "codex")
                            && (line.contains("already up to date")
                                || line.contains("already installed")))
                        || line.starts_with("already up to date"))
            }
            UpdateOwner::ManualActionRequired { .. } => false,
        };
        if matched {
            return true;
        }
    }

    false
}

fn updater_output_confirms_updated(text: &str, active_version: &str) -> bool {
    if updater_output_has_negation_or_failure(text) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    let ver_lower = active_version.to_ascii_lowercase();
    lower.contains(&format!("updated to {ver_lower}"))
        || lower.contains(&format!("upgraded to {ver_lower}"))
        || lower.contains(&format!("codex cli {ver_lower} installed successfully"))
        || lower.contains("successfully updated")
        || lower.contains("successfully upgraded")
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
