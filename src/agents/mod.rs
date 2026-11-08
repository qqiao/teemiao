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

//! Discover and update installed AI coding-agent CLIs on `PATH`.

pub mod discovery;
pub mod update;

use crate::agents::discovery::{
    DiscoveredAgent, DiscoveryEnv, PackageManagerCache, discover_agent, discover_all_agents,
};
use crate::agents::update::{AgentUpdateResult, UpdateOutcome, execute_agent_update};
use clap::{Args, Subcommand, ValueEnum};
use rust_i18n::t;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Fixed catalog of supported coding-agent CLIs in deterministic presentation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub enum AgentId {
    /// OpenAI Codex CLI (`codex`).
    #[value(name = "codex")]
    Codex,

    /// Google Antigravity CLI (`antigravity`, alias `agy`).
    #[value(name = "antigravity", alias = "agy")]
    Antigravity,

    /// OpenCode CLI (`opencode`).
    #[value(name = "opencode")]
    OpenCode,

    /// Moonshot Kimi CLI (`kimi`).
    #[value(name = "kimi")]
    Kimi,

    /// Cursor CLI (`cursor`, detected via verified `agent` or `cursor-agent`).
    #[value(name = "cursor")]
    Cursor,

    /// Anthropic Claude Code CLI (`claude-code`, alias `claude`).
    #[value(name = "claude-code", alias = "claude")]
    ClaudeCode,
}

impl AgentId {
    /// Complete supported catalog in deterministic order.
    pub const ALL: [AgentId; 6] = [
        AgentId::Codex,
        AgentId::Antigravity,
        AgentId::OpenCode,
        AgentId::Kimi,
        AgentId::Cursor,
        AgentId::ClaudeCode,
    ];

    /// Canonical identifier used in CLI arguments and output.
    pub const fn canonical_id(self) -> &'static str {
        match self {
            AgentId::Codex => "codex",
            AgentId::Antigravity => "antigravity",
            AgentId::OpenCode => "opencode",
            AgentId::Kimi => "kimi",
            AgentId::Cursor => "cursor",
            AgentId::ClaudeCode => "claude-code",
        }
    }

    /// Human-readable product name.
    pub const fn display_name(self) -> &'static str {
        match self {
            AgentId::Codex => "Codex",
            AgentId::Antigravity => "Antigravity",
            AgentId::OpenCode => "OpenCode",
            AgentId::Kimi => "Kimi",
            AgentId::Cursor => "Cursor",
            AgentId::ClaudeCode => "Claude Code",
        }
    }

    /// Allowlisted CLI executable names searched on `PATH` for this agent.
    pub const fn cli_candidates(self) -> &'static [&'static str] {
        match self {
            AgentId::Codex => &["codex"],
            AgentId::Antigravity => &["agy"],
            AgentId::OpenCode => &["opencode"],
            AgentId::Kimi => &["kimi"],
            AgentId::Cursor => &["agent", "cursor-agent"],
            AgentId::ClaudeCode => &["claude"],
        }
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_id())
    }
}

impl FromStr for AgentId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "codex" => Ok(AgentId::Codex),
            "antigravity" | "agy" => Ok(AgentId::Antigravity),
            "opencode" => Ok(AgentId::OpenCode),
            "kimi" => Ok(AgentId::Kimi),
            "cursor" => Ok(AgentId::Cursor),
            "claude-code" | "claude" => Ok(AgentId::ClaudeCode),
            other => Err(format!(
                "unknown agent '{other}' (expected codex, antigravity/agy, opencode, kimi, cursor, or claude-code/claude)"
            )),
        }
    }
}

/// Top-level `teemiao agents` command group.
#[derive(Debug, Args)]
#[command(
    subcommand_required = true,
    arg_required_else_help = true,
    about = t!("commands.agents.about"),
    long_about = t!("commands.agents.long_about")
)]
pub struct AgentsCommand {
    /// Subcommand to execute (`list` or `update`).
    #[command(subcommand)]
    pub command: AgentsSubcommand,
}

/// Subcommands under `teemiao agents`.
#[derive(Debug, Subcommand)]
pub enum AgentsSubcommand {
    /// List installed coding-agent CLIs discovered on `PATH`.
    #[command(
        about = t!("commands.agents.list.about"),
        long_about = t!("commands.agents.list.long_about")
    )]
    List,

    /// Update one installed coding-agent CLI or all detected coding-agent CLIs on `PATH`.
    #[command(
        about = t!("commands.agents.update.about"),
        long_about = t!("commands.agents.update.long_about")
    )]
    Update(UpdateArgs),
}

/// Arguments for `teemiao agents update [AGENT] [--dry-run]`.
#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Optional canonical agent identifier or alias to update.
    #[arg(
        value_enum,
        ignore_case = true,
        value_name = "AGENT",
        help = t!("agents.update.agent.help"),
        long_help = t!("agents.update.agent.long_help")
    )]
    pub agent: Option<AgentId>,

    /// Preview selected update commands and targets without modifying installed software.
    #[arg(
        long,
        help = t!("agents.update.dry_run.help"),
        long_help = t!("agents.update.dry_run.long_help")
    )]
    pub dry_run: bool,
}

/// Specification for spawning a probe or updater child process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRequest {
    /// Resolved executable path.
    pub program: PathBuf,
    /// Argument array passed directly without shell interpolation.
    pub args: Vec<OsString>,
    /// Scoped environment variable overrides applied only to the child process.
    pub env_overrides: Vec<(OsString, OsString)>,
    /// Optional execution deadline; child is killed and reaped on timeout.
    pub timeout: Option<Duration>,
    /// Maximum bytes retained for stdout and stderr respectively.
    pub max_output_bytes: usize,
}

/// Captured result from executing a child process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    /// Numeric exit code if the process exited normally.
    pub status_code: Option<i32>,
    /// Whether the process exited with code 0.
    pub success: bool,
    /// Whether the process was terminated by an interrupt/termination signal.
    pub interrupted: bool,
    /// Whether the process exceeded its execution deadline and was killed.
    pub timed_out: bool,
    /// Captured standard output (bounded by `max_output_bytes`).
    pub stdout: Vec<u8>,
    /// Captured standard error (bounded by `max_output_bytes`).
    pub stderr: Vec<u8>,
    /// True if stdout exceeded `max_output_bytes`.
    pub stdout_truncated: bool,
    /// True if stderr exceeded `max_output_bytes`.
    pub stderr_truncated: bool,
}

/// Error returned when a child process cannot be spawned or monitored.
#[derive(Debug, Error)]
pub enum ProcessError {
    /// Failed to spawn or wait on the child process.
    #[error("failed to execute '{program}': {source}")]
    Io {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Injectable execution boundary for discovery probes and update commands.
pub trait ProcessRunner {
    /// Executes a read-only discovery/version/inventory probe.
    fn run_probe(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError>;

    /// Executes an update or installer command.
    fn run_update(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError>;

    /// Returns true if an external cancellation signal has been observed.
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Production synchronous process runner using `std::process::Command` with
/// bounded stream readers and deadline enforcement.
#[derive(Debug, Default)]
pub struct RealProcessRunner {
    cancelled: Arc<AtomicBool>,
}

impl RealProcessRunner {
    /// Creates a new `RealProcessRunner`.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Default)]
struct BoundedStreamBuffer {
    bytes: Vec<u8>,
    truncated: bool,
}

impl ProcessRunner for RealProcessRunner {
    fn run_probe(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        self.execute_bounded(request)
    }

    fn run_update(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        self.execute_bounded(request)
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl RealProcessRunner {
    fn execute_bounded(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        let mut cmd = Command::new(&request.program);
        cmd.args(&request.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(unix)]
        if request.timeout.is_some() {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        for (k, v) in &request.env_overrides {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|source| ProcessError::Io {
            program: request.program.clone(),
            source,
        })?;
        let child_pid = child.id();

        let stdout_buf = Arc::new(Mutex::new(BoundedStreamBuffer::default()));
        let stderr_buf = Arc::new(Mutex::new(BoundedStreamBuffer::default()));
        let reader_state = Arc::new((Mutex::new(0usize), Condvar::new()));
        let mut expected_readers = 0usize;

        let stdout_handle = child.stdout.take().map(|out| {
            expected_readers += 1;
            spawn_bounded_reader(
                out,
                Arc::clone(&stdout_buf),
                request.max_output_bytes,
                Arc::clone(&reader_state),
            )
        });
        let stderr_handle = child.stderr.take().map(|err| {
            expected_readers += 1;
            spawn_bounded_reader(
                err,
                Arc::clone(&stderr_buf),
                request.max_output_bytes,
                Arc::clone(&reader_state),
            )
        });

        let mut timed_out = false;

        let status = if let Some(limit) = request.timeout {
            let start = Instant::now();
            let exit_status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        if start.elapsed() >= limit {
                            timed_out = true;
                            terminate_timed_out_child(&mut child, child_pid);
                            let status = child.wait().map_err(|source| ProcessError::Io {
                                program: request.program.clone(),
                                source,
                            })?;
                            break status;
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(source) => {
                        terminate_timed_out_child(&mut child, child_pid);
                        let _ = child.wait();
                        return Err(ProcessError::Io {
                            program: request.program.clone(),
                            source,
                        });
                    }
                }
            };

            if !timed_out && expected_readers > 0 {
                let (lock, cvar) = &*reader_state;
                let mut done = lock.lock().unwrap_or_else(|e| e.into_inner());
                while *done < expected_readers {
                    let elapsed = start.elapsed();
                    if elapsed >= limit {
                        timed_out = true;
                        drop(done);
                        terminate_timed_out_child(&mut child, child_pid);
                        break;
                    }
                    let remaining = limit - elapsed;
                    let (next_done, wait_res) = cvar
                        .wait_timeout(done, remaining)
                        .unwrap_or_else(|e| e.into_inner());
                    done = next_done;
                    if wait_res.timed_out() && *done < expected_readers {
                        timed_out = true;
                        drop(done);
                        terminate_timed_out_child(&mut child, child_pid);
                        break;
                    }
                }
            }

            exit_status
        } else {
            child.wait().map_err(|source| ProcessError::Io {
                program: request.program.clone(),
                source,
            })?
        };

        if !timed_out {
            if let Some(h) = stdout_handle {
                let _ = h.join();
            }
            if let Some(h) = stderr_handle {
                let _ = h.join();
            }
        } else {
            // Give reader threads a brief moment to flush buffered bytes after killing the
            // process group; never block indefinitely if an external handle remains open.
            thread::sleep(Duration::from_millis(20));
        }

        let (stdout, stdout_truncated) = {
            let mut guard = stdout_buf.lock().unwrap_or_else(|e| e.into_inner());
            (std::mem::take(&mut guard.bytes), guard.truncated)
        };
        let (stderr, stderr_truncated) = {
            let mut guard = stderr_buf.lock().unwrap_or_else(|e| e.into_inner());
            (std::mem::take(&mut guard.bytes), guard.truncated)
        };

        let status_code = status.code();
        let interrupted = !timed_out && is_status_interrupted(&status);
        if interrupted {
            self.cancelled.store(true, Ordering::SeqCst);
        }

        Ok(ProcessOutput {
            status_code,
            success: !timed_out && !interrupted && status.success(),
            interrupted,
            timed_out,
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        })
    }
}

fn terminate_timed_out_child(child: &mut std::process::Child, child_pid: u32) {
    let _ = child.kill();
    #[cfg(unix)]
    if child_pid > 1 {
        let neg_pgid = format!("-{child_pid}");
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &neg_pgid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(unix))]
    let _ = child_pid;
}

fn spawn_bounded_reader<R: Read + Send + 'static>(
    mut reader: R,
    buffer: Arc<Mutex<BoundedStreamBuffer>>,
    max_bytes: usize,
    completion: Arc<(Mutex<usize>, Condvar)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let mut guard = buffer.lock().unwrap_or_else(|e| e.into_inner());
                    if guard.bytes.len() < max_bytes {
                        let remaining = max_bytes - guard.bytes.len();
                        if n <= remaining {
                            guard.bytes.extend_from_slice(&chunk[..n]);
                        } else {
                            guard.bytes.extend_from_slice(&chunk[..remaining]);
                            guard.truncated = true;
                        }
                    } else {
                        guard.truncated = true;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let (lock, cvar) = &*completion;
        let mut done = lock.lock().unwrap_or_else(|e| e.into_inner());
        *done += 1;
        cvar.notify_all();
    })
}

fn is_status_interrupted(status: &std::process::ExitStatus) -> bool {
    if matches!(status.code(), Some(130 | 143)) {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if matches!(status.signal(), Some(2 | 15)) {
            return true;
        }
    }
    false
}

impl AgentsCommand {
    /// Executes the `agents` subcommand using the real process environment and standard streams.
    pub fn run(&self) -> ExitCode {
        let env = DiscoveryEnv::from_process();
        let runner = RealProcessRunner::new();
        let mut stdout = io::stdout();
        let mut stderr = io::stderr();
        let code = self.run_with_io(&env, &runner, &mut stdout, &mut stderr);
        ExitCode::from(code)
    }

    /// Executes the `agents` subcommand against an injected environment, runner, and output streams.
    pub fn run_with_io(
        &self,
        env: &DiscoveryEnv,
        runner: &dyn ProcessRunner,
        stdout: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> u8 {
        let mut cache = PackageManagerCache::new();
        match &self.command {
            AgentsSubcommand::List => run_list(env, runner, &mut cache, stdout, stderr),
            AgentsSubcommand::Update(args) => {
                run_update(args, env, runner, &mut cache, stdout, stderr)
            }
        }
    }
}

fn run_list(
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    let discovered = discover_all_agents(env, runner, cache);

    if discovered.is_empty() {
        let _ = writeln!(stdout, "{}", t!("agents.list.empty"));
        return 0;
    }

    for agent in &discovered {
        for warning in &agent.warnings {
            let _ = writeln!(stderr, "{warning}");
        }
        let line = t!(
            "agents.list.row",
            name = agent.id.display_name(),
            id = agent.id.canonical_id(),
            version = agent.display_version(),
            path = agent.launcher_path.display(),
            method = agent.owner.display_method()
        );
        let _ = writeln!(stdout, "{line}");
    }

    0
}

fn run_update(
    args: &UpdateArgs,
    env: &DiscoveryEnv,
    runner: &dyn ProcessRunner,
    cache: &mut PackageManagerCache,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    let targets: Vec<DiscoveredAgent> = match args.agent {
        Some(target_id) => match discover_agent(target_id, env, runner, cache) {
            Some(agent) => vec![agent],
            None => {
                let _ = writeln!(
                    stderr,
                    "{}",
                    t!(
                        "agents.update.not_installed",
                        name = target_id.display_name(),
                        id = target_id.canonical_id()
                    )
                );
                return 1;
            }
        },
        None => {
            let all = discover_all_agents(env, runner, cache);
            if all.is_empty() {
                let _ = writeln!(stdout, "{}", t!("agents.update.empty"));
                return 0;
            }
            all
        }
    };

    let mut results: Vec<AgentUpdateResult> = Vec::with_capacity(targets.len());
    let mut was_interrupted = false;

    for agent in &targets {
        if runner.is_cancelled() {
            was_interrupted = true;
            break;
        }

        for warning in &agent.warnings {
            let _ = writeln!(stderr, "{warning}");
        }

        let res = execute_agent_update(agent, args.dry_run, env, runner, cache);
        let interrupted = res.outcome.is_interrupted();
        emit_update_diagnostics(&res, stderr);
        results.push(res);

        if interrupted || runner.is_cancelled() {
            was_interrupted = true;
            break;
        }
    }

    for res in &results {
        let line = format_update_summary_line(res);
        let _ = writeln!(stdout, "{line}");
    }

    if was_interrupted {
        130
    } else if results.iter().all(|r| r.outcome.is_success()) {
        0
    } else {
        1
    }
}

fn emit_update_diagnostics(res: &AgentUpdateResult, stderr: &mut dyn Write) {
    match &res.outcome {
        UpdateOutcome::ManualActionRequired {
            reason,
            guidance,
            diagnostics,
        } => {
            let _ = writeln!(
                stderr,
                "{}",
                t!(
                    "agents.update.outcome.manual",
                    name = res.id.display_name(),
                    id = res.id.canonical_id(),
                    reason = reason,
                    guidance = guidance
                )
            );
            if let Some(diag) = diagnostics {
                let _ = writeln!(
                    stderr,
                    "{}",
                    t!(
                        "agents.update.diagnostic",
                        name = res.id.display_name(),
                        id = res.id.canonical_id(),
                        detail = diag
                    )
                );
            }
        }
        UpdateOutcome::Unverified {
            reason,
            path,
            diagnostics,
        } => {
            let _ = writeln!(
                stderr,
                "{}",
                t!(
                    "agents.update.outcome.unverified",
                    name = res.id.display_name(),
                    id = res.id.canonical_id(),
                    reason = reason,
                    path = path.display()
                )
            );
            if let Some(diag) = diagnostics {
                let _ = writeln!(
                    stderr,
                    "{}",
                    t!(
                        "agents.update.diagnostic",
                        name = res.id.display_name(),
                        id = res.id.canonical_id(),
                        detail = diag
                    )
                );
            }
        }
        UpdateOutcome::Failed {
            reason,
            path,
            diagnostics,
        } => {
            let _ = writeln!(
                stderr,
                "{}",
                t!(
                    "agents.update.outcome.failed",
                    name = res.id.display_name(),
                    id = res.id.canonical_id(),
                    reason = reason,
                    path = path.display()
                )
            );
            if let Some(diag) = diagnostics {
                let _ = writeln!(
                    stderr,
                    "{}",
                    t!(
                        "agents.update.diagnostic",
                        name = res.id.display_name(),
                        id = res.id.canonical_id(),
                        detail = diag
                    )
                );
            }
        }
        UpdateOutcome::Interrupted { .. } => {
            let _ = writeln!(
                stderr,
                "{}",
                t!(
                    "agents.update.outcome.interrupted",
                    name = res.id.display_name(),
                    id = res.id.canonical_id()
                )
            );
        }
        UpdateOutcome::DryRun { .. }
        | UpdateOutcome::Updated { .. }
        | UpdateOutcome::UpToDate { .. } => {}
    }
}

fn format_update_summary_line(res: &AgentUpdateResult) -> String {
    let name = res.id.display_name();
    let id = res.id.canonical_id();
    match &res.outcome {
        UpdateOutcome::DryRun {
            command,
            target,
            current_version,
        } => {
            let ver = current_version
                .clone()
                .unwrap_or_else(|| t!("agents.version_unknown").to_string());
            t!(
                "agents.update.dry_run_plan",
                name = name,
                id = id,
                command = command,
                target = target,
                version = ver
            )
            .to_string()
        }
        UpdateOutcome::Updated {
            before,
            after,
            path,
            ..
        } => t!(
            "agents.update.outcome.updated",
            name = name,
            id = id,
            before = before,
            after = after,
            path = path.display()
        )
        .to_string(),
        UpdateOutcome::UpToDate { version, path, .. } => t!(
            "agents.update.outcome.up_to_date",
            name = name,
            id = id,
            version = version,
            path = path.display()
        )
        .to_string(),
        UpdateOutcome::ManualActionRequired {
            reason, guidance, ..
        } => t!(
            "agents.update.outcome.manual",
            name = name,
            id = id,
            reason = reason,
            guidance = guidance
        )
        .to_string(),
        UpdateOutcome::Unverified { reason, path, .. } => t!(
            "agents.update.outcome.unverified",
            name = name,
            id = id,
            reason = reason,
            path = path.display()
        )
        .to_string(),
        UpdateOutcome::Failed { reason, path, .. } => t!(
            "agents.update.outcome.failed",
            name = name,
            id = id,
            reason = reason,
            path = path.display()
        )
        .to_string(),
        UpdateOutcome::Interrupted { .. } => {
            t!("agents.update.outcome.interrupted", name = name, id = id).to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::discovery::{
        DiscoveryEnv, UpdateOwner, parse_version_text, split_path_entries,
    };
    use crate::agents::update::{
        CODEX_UNIX_INSTALLER_URL, CODEX_WINDOWS_INSTALLER_URL, UpdatePlan, select_update_plan,
    };
    use std::collections::VecDeque;
    use std::ffi::OsStr;
    use std::fs;
    use std::sync::atomic::AtomicUsize;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> io::Result<Self> {
            let pid = std::process::id();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("teemiao-unit-test-{pid}-{nanos}-{seq}"));
            fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    type HandlerFn =
        Box<dyn Fn(&ProcessRequest) -> Result<ProcessOutput, ProcessError> + Send + Sync>;

    struct FakeProcessRunner {
        probe_calls: Mutex<Vec<ProcessRequest>>,
        update_calls: Mutex<Vec<ProcessRequest>>,
        probe_handler: HandlerFn,
        update_handler: HandlerFn,
        cancelled: AtomicBool,
    }

    impl FakeProcessRunner {
        fn new<P, U>(probe_handler: P, update_handler: U) -> Self
        where
            P: Fn(&ProcessRequest) -> Result<ProcessOutput, ProcessError> + Send + Sync + 'static,
            U: Fn(&ProcessRequest) -> Result<ProcessOutput, ProcessError> + Send + Sync + 'static,
        {
            Self {
                probe_calls: Mutex::new(Vec::new()),
                update_calls: Mutex::new(Vec::new()),
                probe_handler: Box::new(probe_handler),
                update_handler: Box::new(update_handler),
                cancelled: AtomicBool::new(false),
            }
        }

        fn probe_calls(&self) -> Vec<ProcessRequest> {
            self.probe_calls.lock().unwrap().clone()
        }

        fn update_calls(&self) -> Vec<ProcessRequest> {
            self.update_calls.lock().unwrap().clone()
        }
    }

    impl ProcessRunner for FakeProcessRunner {
        fn run_probe(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
            self.probe_calls.lock().unwrap().push(request.clone());
            (self.probe_handler)(request)
        }

        fn run_update(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
            self.update_calls.lock().unwrap().push(request.clone());
            let out = (self.update_handler)(request)?;
            if out.interrupted {
                self.cancelled.store(true, Ordering::SeqCst);
            }
            Ok(out)
        }

        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::SeqCst)
        }
    }

    fn ok_output(stdout: &str) -> ProcessOutput {
        ProcessOutput {
            status_code: Some(0),
            success: true,
            interrupted: false,
            timed_out: false,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    fn err_output(code: i32, stderr: &str) -> ProcessOutput {
        ProcessOutput {
            status_code: Some(code),
            success: false,
            interrupted: false,
            timed_out: false,
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    fn create_executable(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).unwrap();
        }
    }

    fn test_env(bin_dirs: &[&std::path::Path], home: &std::path::Path) -> DiscoveryEnv {
        let joined = std::env::join_paths(bin_dirs).unwrap();
        DiscoveryEnv {
            path: joined,
            pathext: None,
            home_dir: Some(home.to_path_buf()),
            local_app_data: Some(home.join("AppData").join("Local")),
            codex_home: None,
            codex_install_dir: None,
            homebrew_prefix: None,
            is_windows: false,
        }
    }

    #[test]
    fn agent_id_parsing_and_aliases_are_case_insensitive() {
        assert_eq!("codex".parse::<AgentId>(), Ok(AgentId::Codex));
        assert_eq!("CODEX".parse::<AgentId>(), Ok(AgentId::Codex));
        assert_eq!("antigravity".parse::<AgentId>(), Ok(AgentId::Antigravity));
        assert_eq!("agy".parse::<AgentId>(), Ok(AgentId::Antigravity));
        assert_eq!("AGY".parse::<AgentId>(), Ok(AgentId::Antigravity));
        assert_eq!("opencode".parse::<AgentId>(), Ok(AgentId::OpenCode));
        assert_eq!("OpenCode".parse::<AgentId>(), Ok(AgentId::OpenCode));
        assert_eq!("kimi".parse::<AgentId>(), Ok(AgentId::Kimi));
        assert_eq!("cursor".parse::<AgentId>(), Ok(AgentId::Cursor));
        assert_eq!("claude-code".parse::<AgentId>(), Ok(AgentId::ClaudeCode));
        assert_eq!("claude".parse::<AgentId>(), Ok(AgentId::ClaudeCode));
        assert_eq!("CLAUDE".parse::<AgentId>(), Ok(AgentId::ClaudeCode));
        assert!("unknown-agent".parse::<AgentId>().is_err());
    }

    #[test]
    fn parse_version_text_extracts_clean_semver_tokens() {
        assert_eq!(
            parse_version_text("codex-cli 0.10.2\n"),
            Some("0.10.2".to_string())
        );
        assert_eq!(
            parse_version_text("Antigravity CLI v1.4.0"),
            Some("1.4.0".to_string())
        );
        assert_eq!(
            parse_version_text("1.0.29 (Claude Code)"),
            Some("1.0.29".to_string())
        );
        assert_eq!(
            parse_version_text("rust-v0.11.0-alpha.1"),
            Some("0.11.0-alpha.1".to_string())
        );
        assert_eq!(parse_version_text("no version here"), None);
    }

    #[test]
    fn list_all_six_agents_in_catalog_order() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        let local_bin = home.join(".local").join("bin");
        fs::create_dir_all(&local_bin).unwrap();

        // Set up standalone Codex layout
        let codex_release = home
            .join(".codex")
            .join("packages")
            .join("standalone")
            .join("current");
        fs::create_dir_all(codex_release.join("bin")).unwrap();
        fs::write(
            codex_release.join("codex-package.json"),
            r#"{"version":"0.10.0","target":"aarch64-apple-darwin","entrypoint":"bin/codex"}"#,
        )
        .unwrap();
        create_executable(
            &local_bin.join("codex"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                codex_release.join("bin").join("codex").display()
            ),
        );
        create_executable(&local_bin.join("agy"), "#!/bin/sh\n");
        create_executable(&local_bin.join("opencode"), "#!/bin/sh\n");
        create_executable(&local_bin.join("kimi"), "#!/bin/sh\n");
        create_executable(&local_bin.join("agent"), "#!/bin/sh\n");
        create_executable(&local_bin.join("claude"), "#!/bin/sh\n");

        let env = test_env(&[&local_bin], &home);
        let runner = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                let out = match name.as_ref() {
                    "codex" => "codex 0.10.0",
                    "agy" => "Antigravity CLI 1.2.3",
                    "opencode" => "opencode 0.3.1",
                    "kimi" => "kimi, version 0.5.0",
                    "agent" => "Cursor Agent CLI 0.48.2",
                    "claude" => "1.0.18 (Claude Code)",
                    _ => "unknown",
                };
                Ok(ok_output(out))
            },
            |_| panic!("list must never invoke update runner"),
        );

        let cmd = AgentsCommand {
            command: AgentsSubcommand::List,
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = cmd.run_with_io(&env, &runner, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(runner.probe_calls().len(), 6);

        let out_str = String::from_utf8(stdout).unwrap();
        let lines: Vec<&str> = out_str.lines().collect();
        assert_eq!(lines.len(), 6);
        assert!(lines[0].starts_with("Codex (codex) | version: 0.10.0"));
        assert!(lines[1].starts_with("Antigravity (antigravity) | version: 1.2.3"));
        assert!(lines[2].starts_with("OpenCode (opencode) | version: 0.3.1"));
        assert!(lines[3].starts_with("Kimi (kimi) | version: 0.5.0"));
        assert!(lines[4].starts_with("Cursor (cursor) | version: 0.48.2"));
        assert!(lines[5].starts_with("Claude Code (claude-code) | version: 1.0.18"));
    }

    #[test]
    fn list_empty_state_and_rejects_desktop_or_unverified_candidates() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&bin).unwrap();

        // Desktop app launchers (`cursor`, `antigravity`) and an unverified `agent`
        create_executable(&bin.join("cursor"), "#!/bin/sh\n");
        create_executable(&bin.join("antigravity"), "#!/bin/sh\n");
        create_executable(&bin.join("agent"), "#!/bin/sh\n");

        // Non-executable file named `codex`
        fs::write(bin.join("codex"), "not executable").unwrap();

        // Broken symlink named `agy`
        #[cfg(unix)]
        std::os::unix::fs::symlink(bin.join("nonexistent-target"), bin.join("agy")).unwrap();

        let env = test_env(&[&bin], tmp.path());
        let runner = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                assert_eq!(name, "agent", "only generic agent should even be probed");
                Ok(ok_output("unrelated-agent 2.0.0"))
            },
            |_| panic!("list must never invoke update"),
        );

        let cmd = AgentsCommand {
            command: AgentsSubcommand::List,
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = cmd.run_with_io(&env, &runner, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(runner.probe_calls().len(), 1);
        let out_str = String::from_utf8(stdout).unwrap();
        assert!(out_str.contains("No supported AI coding-agent CLIs were found on PATH."));
    }

    #[test]
    fn cursor_falls_back_to_cursor_agent_when_generic_agent_is_unrelated() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        create_executable(&bin.join("agent"), "#!/bin/sh\n");
        create_executable(&bin.join("cursor-agent"), "#!/bin/sh\n");

        let env = test_env(&[&bin], tmp.path());
        let runner = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                match name.as_ref() {
                    "agent" => Ok(ok_output("generic-task-agent 1.0.0")),
                    "cursor-agent" => Ok(ok_output("0.45.1")),
                    _ => unreachable!(),
                }
            },
            |_| unreachable!(),
        );

        let mut cache = PackageManagerCache::new();
        let cursor = discover_agent(AgentId::Cursor, &env, &runner, &mut cache).unwrap();
        assert_eq!(cursor.launcher_path, bin.join("cursor-agent"));
        assert_eq!(cursor.version.as_deref(), Some("0.45.1"));
        assert_eq!(
            cursor.owner,
            UpdateOwner::NativeSelfUpdate {
                program: bin.join("cursor-agent"),
                args: vec!["update".to_string()],
                display_command: "cursor-agent update".to_string(),
            }
        );
    }

    #[test]
    fn timeout_oversized_and_failed_probes_emit_warnings_without_hiding_agents() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        create_executable(&bin.join("agy"), "#!/bin/sh\n");
        create_executable(&bin.join("opencode"), "#!/bin/sh\n");
        create_executable(&bin.join("kimi"), "#!/bin/sh\n");
        create_executable(&bin.join("claude"), "#!/bin/sh\n");

        let env = test_env(&[&bin], tmp.path());
        let runner = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                match name.as_ref() {
                    "agy" => Ok(ProcessOutput {
                        status_code: None,
                        success: false,
                        interrupted: false,
                        timed_out: true,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        stdout_truncated: false,
                        stderr_truncated: false,
                    }),
                    "opencode" => Ok(ProcessOutput {
                        status_code: Some(0),
                        success: true,
                        interrupted: false,
                        timed_out: false,
                        stdout: vec![b'x'; 65_536],
                        stderr: Vec::new(),
                        stdout_truncated: true,
                        stderr_truncated: false,
                    }),
                    "kimi" => Ok(err_output(1, " crash ")),
                    "claude" => Ok(ok_output("not-a-semver-output")),
                    _ => unreachable!(),
                }
            },
            |_| unreachable!(),
        );

        let cmd = AgentsCommand {
            command: AgentsSubcommand::List,
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = cmd.run_with_io(&env, &runner, &mut stdout, &mut stderr);
        assert_eq!(code, 0);

        let out_str = String::from_utf8(stdout).unwrap();
        let err_str = String::from_utf8(stderr).unwrap();
        assert_eq!(out_str.lines().count(), 4);
        assert!(out_str.contains("Antigravity (antigravity) | version: unknown"));
        assert!(out_str.contains("OpenCode (opencode) | version: unknown"));
        assert!(out_str.contains("Kimi (kimi) | version: unknown"));
        assert!(out_str.contains("Claude Code (claude-code) | version: unknown"));

        assert!(err_str.contains("timed out after 5s"));
        assert!(err_str.contains("exceeded 64 KiB limit"));
        assert!(err_str.contains("exit code 1"));
        assert!(err_str.contains("could not parse version"));
    }

    #[test]
    fn codex_npm_homebrew_standalone_and_wrong_owner_plans() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");

        // 1. Proven npm global install
        let npm_prefix = tmp.path().join("npm-global");
        let npm_bin = npm_prefix.join("bin");
        let npm_pkg_dir = npm_prefix
            .join("lib")
            .join("node_modules")
            .join("@openai")
            .join("codex");
        fs::create_dir_all(&npm_pkg_dir).unwrap();
        fs::write(
            npm_pkg_dir.join("package.json"),
            r#"{"name":"@openai/codex","version":"0.9.0"}"#,
        )
        .unwrap();
        create_executable(
            &npm_bin.join("codex"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                npm_pkg_dir.join("bin").join("codex.js").display()
            ),
        );
        create_executable(&npm_bin.join("npm"), "#!/bin/sh\n");

        let env_npm = test_env(&[&npm_bin], &home);
        let runner = FakeProcessRunner::new(|_| Ok(ok_output("codex 0.9.0")), |_| unreachable!());
        let mut cache = PackageManagerCache::new();
        let codex_npm = discover_agent(AgentId::Codex, &env_npm, &runner, &mut cache).unwrap();
        let plan_npm = select_update_plan(&codex_npm);
        assert_eq!(
            plan_npm,
            UpdatePlan::Command {
                program: npm_bin.join("npm"),
                args: vec![
                    OsString::from("install"),
                    OsString::from("-g"),
                    OsString::from("--prefix"),
                    npm_prefix.as_os_str().to_os_string(),
                    OsString::from("@openai/codex@latest"),
                ],
                env_overrides: vec![
                    (
                        OsString::from("NPM_CONFIG_UPDATE_NOTIFIER"),
                        OsString::from("false")
                    ),
                    (OsString::from("NPM_CONFIG_FUND"), OsString::from("false")),
                    (OsString::from("NPM_CONFIG_AUDIT"), OsString::from("false")),
                ],
                display_command: format!(
                    "{} install -g --prefix {} @openai/codex@latest",
                    npm_bin.join("npm").display(),
                    npm_prefix.display()
                ),
                target_description: format!(
                    "{} (npm prefix {})",
                    npm_bin.join("codex").display(),
                    npm_prefix.display()
                ),
            }
        );

        // 2. Proven Homebrew cask install
        let brew_prefix = tmp.path().join("homebrew");
        let brew_bin_dir = brew_prefix.join("bin");
        let cask_dir = brew_prefix.join("Caskroom").join("codex").join("0.10.0");
        fs::create_dir_all(&cask_dir).unwrap();
        create_executable(&cask_dir.join("codex"), "#!/bin/sh\n");
        create_executable(&brew_bin_dir.join("brew"), "#!/bin/sh\n");
        #[cfg(unix)]
        {
            fs::create_dir_all(&brew_bin_dir).unwrap();
            std::os::unix::fs::symlink(cask_dir.join("codex"), brew_bin_dir.join("codex")).unwrap();
        }
        #[cfg(not(unix))]
        create_executable(
            &brew_bin_dir.join("codex"),
            "#!/bin/sh\nexec \"$basedir/../Caskroom/codex/0.10.0/codex\" \"$@\"\n",
        );

        let env_brew = test_env(&[&brew_bin_dir], &home);
        let runner_brew = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                if name == "brew" {
                    Ok(ok_output("codex\n"))
                } else {
                    Ok(ok_output("codex 0.10.0"))
                }
            },
            |_| unreachable!(),
        );
        let mut cache_brew = PackageManagerCache::new();
        let codex_brew =
            discover_agent(AgentId::Codex, &env_brew, &runner_brew, &mut cache_brew).unwrap();
        match select_update_plan(&codex_brew) {
            UpdatePlan::Command { program, args, .. } => {
                assert_eq!(program, brew_bin_dir.join("brew"));
                assert_eq!(
                    args,
                    vec![
                        OsString::from("upgrade"),
                        OsString::from("--cask"),
                        OsString::from("codex")
                    ]
                );
            }
            other => panic!("expected Homebrew cask plan, got {other:?}"),
        }

        // 3. Negative test: unverified loose `codex` binary in a directory where `npm` and `brew`
        // also exist on PATH must NOT guess npm, brew, or standalone installer!
        let loose_bin = tmp.path().join("loose-bin");
        create_executable(&loose_bin.join("codex"), "#!/bin/sh\n");
        let env_loose = test_env(&[&loose_bin, &npm_bin, &brew_bin_dir], &home);
        let mut cache_loose = PackageManagerCache::new();
        let codex_loose =
            discover_agent(AgentId::Codex, &env_loose, &runner, &mut cache_loose).unwrap();
        assert!(matches!(
            select_update_plan(&codex_loose),
            UpdatePlan::ManualActionRequired { .. }
        ));
    }

    #[test]
    fn codex_standalone_validates_download_and_rejects_partial_installer() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        let local_bin = home.join(".local").join("bin");
        let codex_home = home.join(".codex");
        let current_dir = codex_home
            .join("packages")
            .join("standalone")
            .join("current");
        fs::create_dir_all(&current_dir).unwrap();
        fs::write(
            current_dir.join("codex-package.json"),
            r#"{"version":"0.10.0","target":"aarch64-apple-darwin","entrypoint":"bin/codex"}"#,
        )
        .unwrap();
        create_executable(
            &local_bin.join("codex"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                current_dir.join("bin").join("codex").display()
            ),
        );
        create_executable(&local_bin.join("curl"), "#!/bin/sh\n");
        create_executable(&local_bin.join("sh"), "#!/bin/sh\n");

        let env = test_env(&[&local_bin], &home);

        // Partial/truncated installer script must be rejected before invoking `sh`!
        let runner_partial = FakeProcessRunner::new(
            |_| Ok(ok_output("codex 0.10.0")),
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                assert_eq!(name, "curl", "sh must not be invoked on partial download");
                let out_idx = req.args.iter().position(|a| a == "-o").unwrap();
                let target_file = PathBuf::from(&req.args[out_idx + 1]);
                assert_eq!(
                    req.args.last().unwrap().to_string_lossy(),
                    CODEX_UNIX_INSTALLER_URL
                );
                fs::write(&target_file, "#!/bin/sh\n# truncated partial script").unwrap();
                Ok(ok_output(""))
            },
        );

        let cmd = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: Some(AgentId::Codex),
                dry_run: false,
            }),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = cmd.run_with_io(&env, &runner_partial, &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert_eq!(runner_partial.update_calls().len(), 1);
        let err_str = String::from_utf8(stderr).unwrap();
        assert!(err_str.contains("failed validation"));

        // Valid installer script executes `sh <temp_script>` with CODEX_NON_INTERACTIVE=1 and verifies new version
        let probe_versions = Mutex::new(VecDeque::from(vec!["codex 0.10.0", "codex 0.11.0"]));
        let runner_valid = FakeProcessRunner::new(
            move |_| {
                let ver = probe_versions
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or("codex 0.11.0");
                Ok(ok_output(ver))
            },
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                if name == "curl" {
                    let out_idx = req.args.iter().position(|a| a == "-o").unwrap();
                    let target_file = PathBuf::from(&req.args[out_idx + 1]);
                    fs::write(
                        &target_file,
                        "#!/bin/sh\n# CODEX_INSTALL_DIR CODEX_HOME\necho 'Codex CLI 0.11.0 installed successfully.'\n",
                    )
                    .unwrap();
                    Ok(ok_output(""))
                } else if name == "sh" {
                    assert!(
                        req.env_overrides.contains(&(
                            OsString::from("CODEX_NON_INTERACTIVE"),
                            OsString::from("1")
                        ))
                    );
                    Ok(ok_output("Codex CLI 0.11.0 installed successfully."))
                } else {
                    panic!("unexpected update program: {name}");
                }
            },
        );

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = cmd.run_with_io(&env, &runner_valid, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(runner_valid.update_calls().len(), 2);
        let out_str = String::from_utf8(stdout).unwrap();
        assert!(out_str.contains("Codex (codex): updated (0.10.0 -> 0.11.0"));
    }

    #[test]
    fn claude_code_supports_native_npm_channel_cask_and_winget_owners() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");

        // 1. Homebrew channel-specific cask `claude-code@latest`
        let brew_prefix = tmp.path().join("brew");
        let brew_bin = brew_prefix.join("bin");
        let cask_dir = brew_prefix
            .join("Caskroom")
            .join("claude-code@latest")
            .join("1.0.20");
        fs::create_dir_all(&cask_dir).unwrap();
        create_executable(&cask_dir.join("claude"), "#!/bin/sh\n");
        create_executable(&brew_bin.join("brew"), "#!/bin/sh\n");
        #[cfg(unix)]
        {
            fs::create_dir_all(&brew_bin).unwrap();
            std::os::unix::fs::symlink(cask_dir.join("claude"), brew_bin.join("claude")).unwrap();
        }
        #[cfg(not(unix))]
        create_executable(
            &brew_bin.join("claude"),
            "#!/bin/sh\nexec \"$basedir/../Caskroom/claude-code@latest/1.0.20/claude\" \"$@\"\n",
        );

        let env_brew = test_env(&[&brew_bin], &home);
        let runner_brew = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                if name == "brew" {
                    Ok(ok_output("claude-code@latest\n"))
                } else {
                    Ok(ok_output("1.0.20 (Claude Code)"))
                }
            },
            |_| unreachable!(),
        );
        let mut cache = PackageManagerCache::new();
        let claude_brew =
            discover_agent(AgentId::ClaudeCode, &env_brew, &runner_brew, &mut cache).unwrap();
        match select_update_plan(&claude_brew) {
            UpdatePlan::Command { args, .. } => {
                assert_eq!(
                    args,
                    vec![
                        OsString::from("upgrade"),
                        OsString::from("--cask"),
                        OsString::from("claude-code@latest")
                    ]
                );
            }
            other => panic!("expected Homebrew cask plan, got {other:?}"),
        }

        // 2. WinGet installation
        let winget_pkg_dir = tmp
            .path()
            .join("Microsoft")
            .join("WinGet")
            .join("Packages")
            .join("Anthropic.ClaudeCode_Microsoft.Winget.Source_8wekyb3d8bbwe");
        create_executable(&winget_pkg_dir.join("claude.exe"), "MZ");
        create_executable(&winget_pkg_dir.join("winget.exe"), "MZ");
        let mut env_winget = test_env(&[&winget_pkg_dir], &home);
        env_winget.is_windows = true;

        let runner_winget = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                if name.starts_with("winget") {
                    Ok(ok_output("Anthropic.ClaudeCode 1.0.15"))
                } else {
                    Ok(ok_output("1.0.15 (Claude Code)"))
                }
            },
            |_| unreachable!(),
        );
        let mut cache_wg = PackageManagerCache::new();
        let claude_wg = discover_agent(
            AgentId::ClaudeCode,
            &env_winget,
            &runner_winget,
            &mut cache_wg,
        )
        .unwrap();
        match select_update_plan(&claude_wg) {
            UpdatePlan::Command { program, args, .. } => {
                assert_eq!(program, winget_pkg_dir.join("winget.exe"));
                assert_eq!(
                    args,
                    vec![
                        OsString::from("upgrade"),
                        OsString::from("--exact"),
                        OsString::from("--id"),
                        OsString::from("Anthropic.ClaudeCode"),
                        OsString::from("--accept-source-agreements"),
                        OsString::from("--accept-package-agreements"),
                        OsString::from("--disable-interactivity"),
                    ]
                );
            }
            other => panic!("expected WinGet plan, got {other:?}"),
        }
    }

    #[test]
    fn dry_run_performs_zero_mutations_and_bulk_update_handles_partial_failure_and_cancel() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        let bin = home.join(".local").join("bin");
        create_executable(&bin.join("agy"), "#!/bin/sh\n");
        create_executable(&bin.join("opencode"), "#!/bin/sh\n");
        create_executable(&bin.join("kimi"), "#!/bin/sh\n");

        let env = test_env(&[&bin], &home);
        let runner_dry = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                Ok(ok_output(&format!("{name} 1.0.0")))
            },
            |_| panic!("dry-run must never execute update commands"),
        );

        let dry_cmd = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: None,
                dry_run: true,
            }),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = dry_cmd.run_with_io(&env, &runner_dry, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(runner_dry.update_calls().is_empty());
        let out_str = String::from_utf8(stdout).unwrap();
        assert_eq!(out_str.lines().count(), 3);
        assert!(out_str.contains("Antigravity (antigravity): dry-run (would run: agy update"));
        assert!(out_str.contains("OpenCode (opencode): dry-run (would run: opencode upgrade"));
        assert!(out_str.contains("Kimi (kimi): dry-run (would run: kimi upgrade --yes"));

        // Bulk update: agy fails (exit 1), opencode prints zero-exit manual instructions, kimi is already up to date
        let runner_partial = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                Ok(ok_output(&format!("{name} 1.0.0")))
            },
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                match name.as_ref() {
                    "agy" => Ok(err_output(1, "network connection refused")),
                    "opencode" => Ok(ok_output(
                        "Please update manually using your package manager.",
                    )),
                    "kimi" => Ok(ok_output("kimi is already up to date (1.0.0)")),
                    _ => unreachable!(),
                }
            },
        );

        let bulk_cmd = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: None,
                dry_run: false,
            }),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = bulk_cmd.run_with_io(&env, &runner_partial, &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert_eq!(runner_partial.update_calls().len(), 3);
        let out_str = String::from_utf8(stdout).unwrap();
        assert!(out_str.contains("Antigravity (antigravity): failed"));
        assert!(out_str.contains("OpenCode (opencode): manual action required"));
        assert!(out_str.contains("Kimi (kimi): up to date"));

        // Cancellation / interruption stops sequence immediately before later agents run
        let runner_cancel = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                Ok(ok_output(&format!("{name} 1.0.0")))
            },
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                assert_eq!(
                    name, "agy",
                    "later agents must not start after interruption"
                );
                Ok(ProcessOutput {
                    status_code: Some(130),
                    success: false,
                    interrupted: true,
                    timed_out: false,
                    stdout: Vec::new(),
                    stderr: b"Interrupted".to_vec(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                })
            },
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = bulk_cmd.run_with_io(&env, &runner_cancel, &mut stdout, &mut stderr);
        assert_eq!(code, 130);
        assert_eq!(runner_cancel.update_calls().len(), 1);
        let out_str = String::from_utf8(stdout).unwrap();
        assert!(out_str.contains("Antigravity (antigravity): interrupted"));
        assert!(!out_str.contains("OpenCode"));
    }

    #[test]
    fn windows_path_splitting_and_cmd_metacharacter_rejection() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let safe_dir = tmp.path().join("Program Files (x86)").join("Safe Agent");
        let unsafe_dir = tmp.path().join("Dir & Evil");
        create_executable(&safe_dir.join("agy.exe"), "MZ");
        create_executable(&unsafe_dir.join("opencode.cmd"), "@echo off\r\n");

        let raw_path = format!("\"{}\";\"{}\"", safe_dir.display(), unsafe_dir.display());
        let entries = split_path_entries(OsStr::new(&raw_path), true);
        assert_eq!(entries, vec![safe_dir.clone(), unsafe_dir.clone()]);

        let env = DiscoveryEnv {
            path: OsString::from(raw_path),
            pathext: Some(OsString::from(".EXE;.CMD")),
            home_dir: Some(tmp.path().to_path_buf()),
            local_app_data: Some(tmp.path().to_path_buf()),
            codex_home: None,
            codex_install_dir: None,
            homebrew_prefix: None,
            is_windows: true,
        };

        let runner = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                assert_eq!(
                    name, "agy.exe",
                    "unsafe .cmd shim with '&' must never be spawned"
                );
                Ok(ok_output("1.5.0"))
            },
            |_| panic!("unsafe .cmd shim must not be executed"),
        );

        let mut cache = PackageManagerCache::new();
        let all = discover_all_agents(&env, &runner, &mut cache);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, AgentId::Antigravity);
        assert_eq!(all[0].version.as_deref(), Some("1.5.0"));
        assert_eq!(all[1].id, AgentId::OpenCode);
        assert_eq!(all[1].version, None);
        assert!(matches!(
            all[1].owner,
            UpdateOwner::ManualActionRequired { .. }
        ));
    }

    #[test]
    fn windows_codex_standalone_uses_powershell_with_file_argument() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let local_app_data = tmp.path().join("AppData").join("Local");
        let win_bin = local_app_data
            .join("Programs")
            .join("OpenAI")
            .join("Codex")
            .join("bin");
        create_executable(&win_bin.join("codex.exe"), "MZ");
        create_executable(&win_bin.join("rg.exe"), "MZ");
        create_executable(&win_bin.join("curl.exe"), "MZ");
        create_executable(&win_bin.join("powershell.exe"), "MZ");
        let win_standalone_dir = tmp
            .path()
            .join(".codex")
            .join("packages")
            .join("standalone")
            .join("current");
        fs::create_dir_all(&win_standalone_dir).unwrap();
        fs::write(
            win_standalone_dir.join("codex-package.json"),
            r#"{"version":"0.10.0","channel":"stable"}"#,
        )
        .unwrap();

        let env = DiscoveryEnv {
            path: OsString::from(win_bin.as_os_str()),
            pathext: Some(OsString::from(".EXE;.CMD")),
            home_dir: Some(tmp.path().to_path_buf()),
            local_app_data: Some(local_app_data),
            codex_home: Some(tmp.path().join(".codex")),
            codex_install_dir: None,
            homebrew_prefix: None,
            is_windows: true,
        };

        let versions = Mutex::new(VecDeque::from(vec!["codex 0.10.0", "codex 0.11.0"]));
        let runner = FakeProcessRunner::new(
            move |_| {
                let v = versions
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or("codex 0.11.0");
                Ok(ok_output(v))
            },
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                if name == "curl.exe" {
                    let out_idx = req.args.iter().position(|a| a == "-o").unwrap();
                    let script_file = PathBuf::from(&req.args[out_idx + 1]);
                    assert_eq!(
                        req.args.last().unwrap().to_string_lossy(),
                        CODEX_WINDOWS_INSTALLER_URL
                    );
                    fs::write(
                        &script_file,
                        "# CODEX_INSTALL_DIR CODEX_HOME\r\nWrite-Host 'Codex CLI 0.11.0 installed successfully.'\r\n",
                    )
                    .unwrap();
                    Ok(ok_output(""))
                } else if name == "powershell.exe" {
                    assert_eq!(req.args[0], OsString::from("-NoProfile"));
                    assert_eq!(req.args[1], OsString::from("-NonInteractive"));
                    assert_eq!(req.args[2], OsString::from("-ExecutionPolicy"));
                    assert_eq!(req.args[3], OsString::from("Bypass"));
                    assert_eq!(req.args[4], OsString::from("-File"));
                    Ok(ok_output("Codex CLI 0.11.0 installed successfully."))
                } else {
                    panic!("unexpected program: {name}");
                }
            },
        );

        let cmd = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: Some(AgentId::Codex),
                dry_run: false,
            }),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = cmd.run_with_io(&env, &runner, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        let out_str = String::from_utf8(stdout).unwrap();
        assert!(out_str.contains("Codex (codex): updated (0.10.0 -> 0.11.0"));
    }

    #[test]
    fn cursor_rejects_conflicting_banner_even_in_cursor_dirs_and_accepts_parsed_package_json() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");

        // 1. `.cursor/bin/agent` printing `Datadog Agent 7.50.0` must be rejected
        let dot_cursor_bin = home.join(".cursor").join("bin");
        create_executable(&dot_cursor_bin.join("agent"), "#!/bin/sh\n");
        let env_conflict = test_env(&[&dot_cursor_bin], &home);
        let runner_conflict = FakeProcessRunner::new(
            |_| Ok(ok_output("Datadog Agent 7.50.0")),
            |_| unreachable!(),
        );
        let mut cache = PackageManagerCache::new();
        assert!(
            discover_agent(AgentId::Cursor, &env_conflict, &runner_conflict, &mut cache).is_none()
        );

        // 2. `cursor-tools/bin/agent` printing bare `1.0.0` without Cursor branding or metadata must be rejected
        let cursor_tools_bin = tmp.path().join("cursor-tools").join("bin");
        create_executable(&cursor_tools_bin.join("agent"), "#!/bin/sh\n");
        let env_tools = test_env(&[&cursor_tools_bin], &home);
        let runner_bare = FakeProcessRunner::new(
            |req| {
                if req.args.iter().any(|a| a == "--version") {
                    Ok(ok_output("1.0.0"))
                } else {
                    Ok(ok_output("Usage: agent [options]"))
                }
            },
            |_| unreachable!(),
        );
        let mut cache_tools = PackageManagerCache::new();
        assert!(
            discover_agent(AgentId::Cursor, &env_tools, &runner_bare, &mut cache_tools).is_none()
        );

        // 3. Adjacent `package.json` with `"name": "@cursor/agent"` and bare date/hash version `2026.11.05-86047df` is accepted
        let cursor_pkg_dir = tmp.path().join("verified-pkg");
        let cursor_pkg_bin = cursor_pkg_dir.join("bin");
        create_executable(&cursor_pkg_bin.join("agent"), "#!/bin/sh\n");
        fs::write(
            cursor_pkg_dir.join("package.json"),
            r#"{"name":"@cursor/agent","version":"2026.11.05-86047df"}"#,
        )
        .unwrap();
        let env_pkg = test_env(&[&cursor_pkg_bin], &home);
        let runner_pkg =
            FakeProcessRunner::new(|_| Ok(ok_output("2026.11.05-86047df")), |_| unreachable!());
        let mut cache_pkg = PackageManagerCache::new();
        let discovered =
            discover_agent(AgentId::Cursor, &env_pkg, &runner_pkg, &mut cache_pkg).unwrap();
        assert_eq!(discovered.version.as_deref(), Some("2026.11.05-86047df"));
    }

    #[test]
    fn homebrew_multi_prefix_and_negative_updater_messages_are_verified() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");

        // 1. When `other_brew/bin` precedes `owner_brew/bin` on PATH, `owner_brew/bin/brew` is selected
        let other_prefix = tmp.path().join("other-brew");
        let other_bin = other_prefix.join("bin");
        let owner_prefix = tmp.path().join("owner-brew");
        let owner_bin = owner_prefix.join("bin");
        let cask_dir = owner_prefix.join("Caskroom").join("codex").join("0.118.0");
        fs::create_dir_all(&cask_dir).unwrap();
        create_executable(&other_bin.join("brew"), "#!/bin/sh\n");
        create_executable(&owner_bin.join("brew"), "#!/bin/sh\n");
        create_executable(
            &owner_bin.join("codex"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                cask_dir.join("codex").display()
            ),
        );

        let other_prefix_str = other_prefix.to_string_lossy().into_owned();
        let owner_prefix_str = owner_prefix.to_string_lossy().into_owned();
        let env = test_env(&[&other_bin, &owner_bin], &home);
        let runner = FakeProcessRunner::new(
            move |req| {
                let prog = req.program.to_string_lossy();
                if req.args.iter().any(|a| a == "--prefix") {
                    if prog.contains("other-brew") {
                        Ok(ok_output(&other_prefix_str))
                    } else {
                        Ok(ok_output(&owner_prefix_str))
                    }
                } else if req.args.iter().any(|a| a == "list") {
                    Ok(ok_output("codex\n"))
                } else {
                    Ok(ok_output("codex 0.118.0"))
                }
            },
            |_| Ok(ok_output("Warning: ripgrep 14.1.0 is already installed")),
        );

        let mut cache = PackageManagerCache::new();
        let codex = discover_agent(AgentId::Codex, &env, &runner, &mut cache).unwrap();
        assert_eq!(
            codex.owner,
            UpdateOwner::HomebrewCask {
                brew_program: owner_bin.join("brew"),
                cask_token: "codex".to_string(),
            }
        );

        // 2. Foreign Homebrew message "Warning: ripgrep 14.1.0 is already installed" must produce Unverified (exit 1)
        let cmd = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: Some(AgentId::Codex),
                dry_run: false,
            }),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = cmd.run_with_io(&env, &runner, &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        let out_str = String::from_utf8(stdout).unwrap();
        assert!(out_str.contains("unverified"));

        // 3. Cross-prefix npm wrapper with non-ASCII comments must not authorize updating stale prefix
        let stale_prefix = tmp.path().join("stale-npm");
        let active_prefix = tmp.path().join("active-npm");
        for p in [&stale_prefix, &active_prefix] {
            let pkg_dir = p
                .join("lib")
                .join("node_modules")
                .join("@openai")
                .join("codex");
            fs::create_dir_all(&pkg_dir).unwrap();
            fs::write(
                pkg_dir.join("package.json"),
                r#"{"name":"@openai/codex","version":"0.118.0"}"#,
            )
            .unwrap();
        }
        let active_target = active_prefix
            .join("lib")
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("bin")
            .join("codex");
        create_executable(&active_target, "#!/bin/sh\n");
        let stale_bin = stale_prefix.join("bin");
        create_executable(&stale_bin.join("npm"), "#!/bin/sh\n");
        create_executable(
            &stale_bin.join("codex"),
            &format!(
                "#!/bin/sh\n✨ 备注 env node @openai/codex\nexec \"{}\" \"$@\"\n",
                active_target.display()
            ),
        );
        let env_stale = test_env(&[&stale_bin], &home);
        let runner_stale =
            FakeProcessRunner::new(|_| Ok(ok_output("codex-cli 0.118.0")), |_| unreachable!());
        let mut cache_stale = PackageManagerCache::new();
        let codex_stale =
            discover_agent(AgentId::Codex, &env_stale, &runner_stale, &mut cache_stale).unwrap();
        assert!(matches!(
            codex_stale.owner,
            UpdateOwner::ManualActionRequired { .. }
        ));
    }
}
