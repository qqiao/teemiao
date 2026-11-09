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

    /// Qoder CLI (`qoder`, alias `qodercli`).
    #[value(name = "qoder", alias = "qodercli")]
    Qoder,
}

impl AgentId {
    /// Complete supported catalog in deterministic order.
    pub const ALL: [AgentId; 7] = [
        AgentId::Codex,
        AgentId::Antigravity,
        AgentId::OpenCode,
        AgentId::Kimi,
        AgentId::Cursor,
        AgentId::ClaudeCode,
        AgentId::Qoder,
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
            AgentId::Qoder => "qoder",
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
            AgentId::Qoder => "Qoder",
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
            AgentId::Qoder => &["qodercli", "qoder"],
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
            "qoder" | "qodercli" => Ok(AgentId::Qoder),
            other => Err(format!(
                "unknown agent '{other}' (expected codex, antigravity/agy, opencode, kimi, cursor, claude-code/claude, or qoder/qodercli)"
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
    #[cfg(test)]
    pre_attach_delay: Duration,
    #[cfg(test)]
    pre_attach_check_absent_path: Option<PathBuf>,
    #[cfg(test)]
    pre_attach_path_was_absent: Arc<AtomicBool>,
    #[cfg(all(windows, test))]
    force_job_assign_access_denied: bool,
}

impl RealProcessRunner {
    /// Creates a new `RealProcessRunner`.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            pre_attach_delay: Duration::ZERO,
            #[cfg(test)]
            pre_attach_check_absent_path: None,
            #[cfg(test)]
            pre_attach_path_was_absent: Arc::new(AtomicBool::new(false)),
            #[cfg(all(windows, test))]
            force_job_assign_access_denied: false,
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

        #[cfg(windows)]
        if request.timeout.is_some() {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(win_process_tree::CREATE_SUSPENDED);
        }

        for (k, v) in &request.env_overrides {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|source| ProcessError::Io {
            program: request.program.clone(),
            source,
        })?;

        #[cfg(test)]
        {
            if !self.pre_attach_delay.is_zero() {
                thread::sleep(self.pre_attach_delay);
            }
            if let Some(ref check_path) = self.pre_attach_check_absent_path {
                self.pre_attach_path_was_absent
                    .store(!check_path.exists(), Ordering::SeqCst);
            }
        }

        let child_group = self
            .attach_and_resume_child(&mut child, request.timeout.is_some())
            .map_err(|source| ProcessError::Io {
                program: request.program.clone(),
                source,
            })?;

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
                            terminate_timed_out_child(&mut child, &child_group);
                            let status = child.wait().map_err(|source| ProcessError::Io {
                                program: request.program.clone(),
                                source,
                            })?;
                            break status;
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(source) => {
                        terminate_timed_out_child(&mut child, &child_group);
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
                        terminate_timed_out_child(&mut child, &child_group);
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
                        terminate_timed_out_child(&mut child, &child_group);
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
    fn attach_and_resume_child(
        &self,
        child: &mut std::process::Child,
        has_timeout: bool,
    ) -> io::Result<ChildProcessGroup> {
        #[cfg(windows)]
        {
            let job_handle = if has_timeout {
                #[cfg(test)]
                let force_denied = self.force_job_assign_access_denied;
                #[cfg(not(test))]
                let force_denied = false;
                win_process_tree::attach_and_resume(child, force_denied)?
            } else {
                std::ptr::null_mut()
            };
            Ok(ChildProcessGroup { job_handle })
        }
        #[cfg(not(windows))]
        {
            let _ = (self, has_timeout);
            Ok(ChildProcessGroup {
                child_pid: child.id(),
            })
        }
    }
}

struct ChildProcessGroup {
    #[cfg(not(windows))]
    child_pid: u32,
    #[cfg(windows)]
    job_handle: *mut std::ffi::c_void,
}

#[cfg(windows)]
impl Drop for ChildProcessGroup {
    fn drop(&mut self) {
        win_process_tree::close_job(self.job_handle);
    }
}

fn terminate_timed_out_child(child: &mut std::process::Child, group: &ChildProcessGroup) {
    #[cfg(unix)]
    {
        let _ = child.kill();
        if group.child_pid > 1 {
            let neg_pgid = format!("-{}", group.child_pid);
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &neg_pgid])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    #[cfg(windows)]
    {
        win_process_tree::terminate_job_and_tree(child, group.job_handle);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = child.kill();
        let _ = group.child_pid;
    }
}

#[cfg(any(windows, test))]
mod win_process_tree {
    use std::collections::BTreeSet;
    use std::io;
    use std::time::{Duration, Instant};

    pub(super) const JOB_CLEANUP_WAIT_BUDGET: Duration = Duration::from_millis(500);

    pub(super) trait Win32JobBackend {
        fn create_job(&self) -> io::Result<usize>;
        fn set_kill_on_close(&self, job: usize) -> io::Result<()>;
        fn assign_process_to_job(&self, job: usize, process: usize) -> io::Result<()>;
        fn snapshot_threads(&self) -> io::Result<Vec<(u32, u32)>>;
        fn open_thread(&self, tid: u32) -> Option<usize>;
        fn thread_owner_pid(&self, thread_handle: usize) -> u32;
        fn is_thread_alive(&self, thread_handle: usize) -> bool;
        fn resume_thread(&self, thread_handle: usize) -> io::Result<u32>;
        fn terminate_job(&self, job: usize);
        fn query_job_process_ids(&self, job: usize) -> Vec<u32>;
        fn open_sync_process(&self, pid: u32) -> Option<usize>;
        fn is_process_in_job(&self, process_handle: usize, job: usize) -> bool;
        fn wait_process(&self, process_handle: usize, timeout_ms: u32);
        fn close_handle(&self, handle: usize);
        fn now(&self) -> Instant {
            Instant::now()
        }
    }

    pub(super) fn resume_suspended_process_with_backend<B: Win32JobBackend>(
        backend: &B,
        child_pid: u32,
    ) -> io::Result<()> {
        if child_pid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot resume process with PID 0",
            ));
        }
        let entries = backend.snapshot_threads()?;
        for (tid, owner_pid) in entries {
            if tid == 0 || owner_pid != child_pid {
                continue;
            }
            let Some(th) = backend.open_thread(tid) else {
                continue;
            };
            if backend.thread_owner_pid(th) != child_pid || !backend.is_thread_alive(th) {
                backend.close_handle(th);
                continue;
            }
            match backend.resume_thread(th) {
                Ok(prev_count) if prev_count > 0 => {
                    for _ in 1..prev_count {
                        if let Err(err) = backend.resume_thread(th) {
                            backend.close_handle(th);
                            return Err(err);
                        }
                    }
                    backend.close_handle(th);
                    return Ok(());
                }
                Ok(_) => {
                    backend.close_handle(th);
                }
                Err(err) => {
                    backend.close_handle(th);
                    return Err(err);
                }
            }
        }
        Err(io::Error::other(format!(
            "failed to locate and resume suspended primary thread for PID {child_pid}"
        )))
    }

    pub(super) fn setup_contained_process_with_backend<B: Win32JobBackend>(
        backend: &B,
        process_handle: usize,
        child_pid: u32,
    ) -> io::Result<usize> {
        let job = backend.create_job()?;
        if let Err(err) = backend.set_kill_on_close(job) {
            backend.close_handle(job);
            return Err(err);
        }
        if let Err(err) = backend.assign_process_to_job(job, process_handle) {
            backend.close_handle(job);
            return Err(err);
        }
        if let Err(err) = resume_suspended_process_with_backend(backend, child_pid) {
            backend.terminate_job(job);
            backend.close_handle(job);
            return Err(err);
        }
        Ok(job)
    }

    fn collect_verified_job_member_handles<B: Win32JobBackend>(
        backend: &B,
        job: usize,
        self_pid: u32,
        seen_pids: &mut BTreeSet<u32>,
    ) -> Vec<usize> {
        if job == 0 {
            return Vec::new();
        }
        let mut handles = Vec::new();
        for pid in backend.query_job_process_ids(job) {
            if pid == 0 || pid == self_pid || !seen_pids.insert(pid) {
                continue;
            }
            if let Some(h) = backend.open_sync_process(pid) {
                if backend.is_process_in_job(h, job) {
                    handles.push(h);
                } else {
                    backend.close_handle(h);
                }
            }
        }
        handles
    }

    pub(super) fn terminate_job_and_wait_with_backend<B: Win32JobBackend, K: FnOnce()>(
        backend: &B,
        job: usize,
        self_pid: u32,
        kill_root_child: K,
    ) {
        let mut seen_pids = BTreeSet::new();
        let mut handles =
            collect_verified_job_member_handles(backend, job, self_pid, &mut seen_pids);
        if job != 0 {
            backend.terminate_job(job);
        }
        kill_root_child();
        handles.extend(collect_verified_job_member_handles(
            backend,
            job,
            self_pid,
            &mut seen_pids,
        ));

        let deadline = backend.now() + JOB_CLEANUP_WAIT_BUDGET;
        for h in handles {
            let remaining_ms = deadline
                .saturating_duration_since(backend.now())
                .as_millis()
                .min(u32::MAX as u128) as u32;
            backend.wait_process(h, remaining_ms);
            backend.close_handle(h);
        }
    }

    #[cfg(windows)]
    pub(super) use os::{CREATE_SUSPENDED, attach_and_resume, close_job, terminate_job_and_tree};

    #[cfg(windows)]
    mod os {
        use super::{
            Win32JobBackend, setup_contained_process_with_backend,
            terminate_job_and_wait_with_backend,
        };
        use std::ffi::c_void;
        use std::io;
        use std::ptr;

        pub(in crate::agents) type Handle = *mut c_void;
        pub(in crate::agents) const CREATE_SUSPENDED: u32 = 0x0000_0004;
        const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
        const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
        const THREAD_SUSPEND_RESUME: u32 = 0x0002;
        const THREAD_QUERY_LIMITED_INFORMATION: u32 = 0x0800;
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const SYNCHRONIZE: u32 = 0x0010_0000;
        const WAIT_TIMEOUT: u32 = 0x0000_0102;
        const ERROR_MORE_DATA: i32 = 234;
        const JOB_OBJECT_BASIC_PROCESS_ID_LIST: i32 = 3;
        const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
        const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
        const MAX_TRACKED_JOB_PIDS: usize = 256;

        #[repr(C)]
        struct ThreadEntry32 {
            dw_size: u32,
            cnt_usage: u32,
            th32_thread_id: u32,
            th32_owner_process_id: u32,
            tp_base_pri: i32,
            tp_delta_pri: i32,
            dw_flags: u32,
        }

        #[repr(C)]
        struct JobObjectBasicLimitInformation {
            per_process_user_time_limit: i64,
            per_job_user_time_limit: i64,
            limit_flags: u32,
            minimum_working_set_size: usize,
            maximum_working_set_size: usize,
            active_process_limit: u32,
            affinity: usize,
            priority_class: u32,
            scheduling_class: u32,
        }

        #[repr(C)]
        struct IoCounters {
            read_operation_count: u64,
            write_operation_count: u64,
            other_operation_count: u64,
            read_transfer_count: u64,
            write_transfer_count: u64,
            other_transfer_count: u64,
        }

        #[repr(C)]
        struct JobObjectExtendedLimitInformation {
            basic_limit_information: JobObjectBasicLimitInformation,
            io_info: IoCounters,
            process_memory_limit: usize,
            job_memory_limit: usize,
            peak_process_memory_used: usize,
            peak_job_memory_used: usize,
        }

        #[repr(C)]
        struct JobObjectBasicProcessIdList {
            number_of_assigned_processes: u32,
            number_of_process_ids_in_list: u32,
            process_id_list: [usize; MAX_TRACKED_JOB_PIDS],
        }

        unsafe extern "system" {
            fn CreateJobObjectW(lp_job_attributes: *const c_void, lp_name: *const u16) -> Handle;
            fn SetInformationJobObject(
                h_job: Handle,
                job_object_information_class: i32,
                lp_job_object_information: *const c_void,
                cb_job_object_information_length: u32,
            ) -> i32;
            fn AssignProcessToJobObject(h_job: Handle, h_process: Handle) -> i32;
            fn TerminateJobObject(h_job: Handle, u_exit_code: u32) -> i32;
            fn QueryInformationJobObject(
                h_job: Handle,
                job_object_information_class: i32,
                lp_job_object_information: *mut c_void,
                cb_job_object_information_length: u32,
                lp_return_length: *mut u32,
            ) -> i32;
            fn IsProcessInJob(process_handle: Handle, job_handle: Handle, result: *mut i32) -> i32;
            fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> Handle;
            fn Thread32First(h_snapshot: Handle, lpte: *mut ThreadEntry32) -> i32;
            fn Thread32Next(h_snapshot: Handle, lpte: *mut ThreadEntry32) -> i32;
            fn OpenThread(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_thread_id: u32,
            ) -> Handle;
            fn GetProcessIdOfThread(thread: Handle) -> u32;
            fn ResumeThread(h_thread: Handle) -> u32;
            fn OpenProcess(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_process_id: u32,
            ) -> Handle;
            fn WaitForSingleObject(h_handle: Handle, dw_milliseconds: u32) -> u32;
            fn CloseHandle(h_object: Handle) -> i32;
            #[cfg(test)]
            fn GetCurrentProcess() -> Handle;
            #[cfg(test)]
            fn DuplicateHandle(
                h_source_process_handle: Handle,
                h_source_handle: Handle,
                h_target_process_handle: Handle,
                lp_target_handle: *mut Handle,
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_options: u32,
            ) -> i32;
        }

        struct OsWin32JobBackend {
            force_assign_access_denied: bool,
        }

        impl Win32JobBackend for OsWin32JobBackend {
            fn create_job(&self) -> io::Result<usize> {
                // SAFETY: Creates an unnamed Win32 Job Object with default security attributes.
                unsafe {
                    let job = CreateJobObjectW(ptr::null(), ptr::null());
                    if job.is_null() {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(job as usize)
                    }
                }
            }

            fn set_kill_on_close(&self, job: usize) -> io::Result<()> {
                // SAFETY: Configures `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` on a valid Job Object handle.
                unsafe {
                    let mut info: JobObjectExtendedLimitInformation = std::mem::zeroed();
                    info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                    let ok = SetInformationJobObject(
                        job as Handle,
                        JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                        (&raw const info).cast::<c_void>(),
                        std::mem::size_of::<JobObjectExtendedLimitInformation>() as u32,
                    );
                    if ok == 0 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                }
            }

            fn assign_process_to_job(&self, job: usize, process: usize) -> io::Result<()> {
                #[cfg(test)]
                if self.force_assign_access_denied {
                    // SAFETY: Duplicates the valid job handle with zero access rights (`dw_desired_access = 0`)
                    // so the kernel `AssignProcessToJobObject` call deterministically fails with `ERROR_ACCESS_DENIED`.
                    unsafe {
                        let current = GetCurrentProcess();
                        let mut restricted_job: Handle = ptr::null_mut();
                        if DuplicateHandle(
                            current,
                            job as Handle,
                            current,
                            &mut restricted_job,
                            0,
                            0,
                            0,
                        ) == 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        let rc = AssignProcessToJobObject(restricted_job, process as Handle);
                        let err = io::Error::last_os_error();
                        let _ = CloseHandle(restricted_job);
                        if rc == 0 {
                            return Err(err);
                        }
                    }
                }
                #[cfg(not(test))]
                let _ = self.force_assign_access_denied;

                // SAFETY: Assigns the valid suspended child process handle to the valid Job Object handle.
                unsafe {
                    if AssignProcessToJobObject(job as Handle, process as Handle) == 0 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                }
            }

            fn snapshot_threads(&self) -> io::Result<Vec<(u32, u32)>> {
                // SAFETY: Takes a thread snapshot and iterates with a properly initialized `ThreadEntry32`.
                unsafe {
                    let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
                    if snap.is_null() || snap == INVALID_HANDLE_VALUE {
                        return Err(io::Error::last_os_error());
                    }
                    let mut pairs = Vec::new();
                    let mut entry: ThreadEntry32 = std::mem::zeroed();
                    entry.dw_size = std::mem::size_of::<ThreadEntry32>() as u32;
                    if Thread32First(snap, &mut entry) != 0 {
                        loop {
                            pairs.push((entry.th32_thread_id, entry.th32_owner_process_id));
                            entry.dw_size = std::mem::size_of::<ThreadEntry32>() as u32;
                            if Thread32Next(snap, &mut entry) == 0 {
                                break;
                            }
                        }
                    }
                    let _ = CloseHandle(snap);
                    Ok(pairs)
                }
            }

            fn open_thread(&self, tid: u32) -> Option<usize> {
                // SAFETY: Opens a thread handle for suspend/resume, ownership verification, and liveness check.
                unsafe {
                    let h = OpenThread(
                        THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                        0,
                        tid,
                    );
                    if h.is_null() { None } else { Some(h as usize) }
                }
            }

            fn thread_owner_pid(&self, thread_handle: usize) -> u32 {
                // SAFETY: Queries the owning PID of a valid open thread handle.
                unsafe { GetProcessIdOfThread(thread_handle as Handle) }
            }

            fn is_thread_alive(&self, thread_handle: usize) -> bool {
                // SAFETY: Non-blocking wait checks whether the thread handle is still non-signaled.
                unsafe { WaitForSingleObject(thread_handle as Handle, 0) == WAIT_TIMEOUT }
            }

            fn resume_thread(&self, thread_handle: usize) -> io::Result<u32> {
                // SAFETY: Decrements the suspend count of a valid open thread handle.
                unsafe {
                    let prev = ResumeThread(thread_handle as Handle);
                    if prev == u32::MAX {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(prev)
                    }
                }
            }

            fn terminate_job(&self, job: usize) {
                if job != 0 {
                    // SAFETY: Terminates all processes associated with the valid Job Object handle.
                    unsafe {
                        let _ = TerminateJobObject(job as Handle, 1);
                    }
                }
            }

            fn query_job_process_ids(&self, job: usize) -> Vec<u32> {
                if job == 0 {
                    return Vec::new();
                }
                // SAFETY: Queries `JobObjectBasicProcessIdList` on a valid Job Object handle using a repr(C) buffer.
                unsafe {
                    let mut list: JobObjectBasicProcessIdList = std::mem::zeroed();
                    let ok = QueryInformationJobObject(
                        job as Handle,
                        JOB_OBJECT_BASIC_PROCESS_ID_LIST,
                        (&raw mut list).cast::<c_void>(),
                        std::mem::size_of::<JobObjectBasicProcessIdList>() as u32,
                        ptr::null_mut(),
                    );
                    if ok == 0 && io::Error::last_os_error().raw_os_error() != Some(ERROR_MORE_DATA)
                    {
                        return Vec::new();
                    }
                    let count =
                        (list.number_of_process_ids_in_list as usize).min(MAX_TRACKED_JOB_PIDS);
                    list.process_id_list[..count]
                        .iter()
                        .filter_map(|&raw| u32::try_from(raw).ok())
                        .collect()
                }
            }

            fn open_sync_process(&self, pid: u32) -> Option<usize> {
                // SAFETY: Opens a process handle strictly for synchronization and job-membership query (never termination).
                unsafe {
                    let h = OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if h.is_null() { None } else { Some(h as usize) }
                }
            }

            fn is_process_in_job(&self, process_handle: usize, job: usize) -> bool {
                // SAFETY: Verifies that `process_handle` is a member of `job` before waiting on it.
                unsafe {
                    let mut in_job: i32 = 0;
                    IsProcessInJob(process_handle as Handle, job as Handle, &mut in_job) != 0
                        && in_job != 0
                }
            }

            fn wait_process(&self, process_handle: usize, timeout_ms: u32) {
                // SAFETY: Waits for process rundown within the shared bounded timeout budget.
                unsafe {
                    let _ = WaitForSingleObject(process_handle as Handle, timeout_ms);
                }
            }

            fn close_handle(&self, handle: usize) {
                if handle != 0 {
                    // SAFETY: Closes a valid Win32 kernel handle.
                    unsafe {
                        let _ = CloseHandle(handle as Handle);
                    }
                }
            }
        }

        pub(in crate::agents) fn attach_and_resume(
            child: &mut std::process::Child,
            force_assign_access_denied: bool,
        ) -> io::Result<Handle> {
            use std::os::windows::io::AsRawHandle;
            let backend = OsWin32JobBackend {
                force_assign_access_denied,
            };
            let process_handle = child.as_raw_handle() as usize;
            let child_pid = child.id();
            match setup_contained_process_with_backend(&backend, process_handle, child_pid) {
                Ok(job) => Ok(job as Handle),
                Err(err) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    Err(err)
                }
            }
        }

        pub(in crate::agents) fn close_job(job_handle: Handle) {
            if !job_handle.is_null() {
                // SAFETY: Closes a valid Job Object handle created by `CreateJobObjectW`.
                unsafe {
                    let _ = CloseHandle(job_handle);
                }
            }
        }

        pub(in crate::agents) fn terminate_job_and_tree(
            child: &mut std::process::Child,
            job_handle: Handle,
        ) {
            let backend = OsWin32JobBackend {
                force_assign_access_denied: false,
            };
            let self_pid = std::process::id();
            terminate_job_and_wait_with_backend(&backend, job_handle as usize, self_pid, || {
                let _ = child.kill();
            });
        }
    }
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

    let total = targets.len();
    let mut results: Vec<AgentUpdateResult> = Vec::with_capacity(total);
    let mut was_interrupted = false;

    for (idx, agent) in targets.iter().enumerate() {
        if runner.is_cancelled() {
            was_interrupted = true;
            break;
        }

        if idx > 0 {
            let _ = writeln!(stderr);
        }
        let _ = writeln!(
            stderr,
            "{}",
            t!(
                "agents.update.section_header",
                index = idx + 1,
                total = total,
                name = agent.id.display_name(),
                id = agent.id.canonical_id()
            )
        );
        let _ = stderr.flush();

        for warning in &agent.warnings {
            let _ = writeln!(stderr, "{warning}");
        }
        if !agent.warnings.is_empty() {
            let _ = stderr.flush();
        }

        let res = execute_agent_update(agent, args.dry_run, env, runner, cache);
        let interrupted = res.outcome.is_interrupted();
        emit_update_diagnostics(&res, stderr);
        let _ = stderr.flush();
        results.push(res);

        if interrupted || runner.is_cancelled() {
            was_interrupted = true;
            break;
        }
    }

    if !results.is_empty() {
        let _ = writeln!(stdout);
        let _ = writeln!(stdout, "{}", t!("agents.update.summary_header"));
        for res in &results {
            let line = format_update_summary_line(res);
            let _ = writeln!(stdout, "{line}");
        }
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
        UpdateOutcome::Updated { diagnostics, .. }
        | UpdateOutcome::UpToDate { diagnostics, .. } => {
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
        UpdateOutcome::DryRun { .. } => {}
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
        assert_eq!("qoder".parse::<AgentId>(), Ok(AgentId::Qoder));
        assert_eq!("QODER".parse::<AgentId>(), Ok(AgentId::Qoder));
        assert_eq!("qodercli".parse::<AgentId>(), Ok(AgentId::Qoder));
        assert_eq!("QoderCli".parse::<AgentId>(), Ok(AgentId::Qoder));
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
        assert_eq!(
            parse_version_text("codex <v0.118.0>"),
            Some("0.118.0".to_string())
        );
        assert_eq!(
            parse_version_text("version='V1.2.3'"),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            parse_version_text("Version 1.2.3."),
            Some("1.2.3".to_string())
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

        // Non-executable entry named `codex` (file without +x on Unix; directory on non-Unix when is_windows=false)
        #[cfg(unix)]
        fs::write(bin.join("codex"), "not executable").unwrap();
        #[cfg(not(unix))]
        fs::create_dir_all(bin.join("codex")).unwrap();

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

        // Windows PATHEXT candidate filtering: extensionless `codex`, `codex.txt`, and desktop `.exe` launchers are skipped
        let bin_win = tmp.path().join("bin-win");
        fs::create_dir_all(&bin_win).unwrap();
        fs::write(bin_win.join("codex"), "extensionless not runnable on win").unwrap();
        fs::write(bin_win.join("codex.txt"), "non-PATHEXT extension").unwrap();
        fs::write(bin_win.join("cursor.exe"), "MZ").unwrap();
        fs::write(bin_win.join("antigravity.exe"), "MZ").unwrap();
        fs::write(bin_win.join("agent.exe"), "MZ").unwrap();

        let mut env_win = test_env(&[&bin_win], tmp.path());
        env_win.is_windows = true;
        env_win.pathext = Some(OsString::from(".EXE;.CMD"));
        let runner_win = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                assert_eq!(
                    name, "agent.exe",
                    "only PATHEXT-matching agent.exe should be probed"
                );
                Ok(ok_output("unrelated-agent 2.0.0"))
            },
            |_| panic!("list must never invoke update"),
        );
        let mut stdout_win = Vec::new();
        let mut stderr_win = Vec::new();
        let code_win = cmd.run_with_io(&env_win, &runner_win, &mut stdout_win, &mut stderr_win);
        assert_eq!(code_win, 0);
        assert!(stderr_win.is_empty());
        assert_eq!(runner_win.probe_calls().len(), 1);
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
            "#!/bin/sh\nbasedir=$(dirname \"$(echo \"$0\" | sed -e 's,\\\\,/,g')\")\nexec \"$basedir/../Caskroom/codex/0.10.0/codex\" \"$@\"\n",
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

        // 4. Negative test: project-local npm package (`<project>/node_modules/@openai/codex` via
        // `<project>/node_modules/.bin/codex`) on Unix must require manual action rather than
        // misclassifying `<project>` as a global npm prefix.
        let project_dir = tmp.path().join("my-project");
        let local_pkg_dir = project_dir
            .join("node_modules")
            .join("@openai")
            .join("codex");
        let local_dot_bin = project_dir.join("node_modules").join(".bin");
        fs::create_dir_all(local_pkg_dir.join("bin")).unwrap();
        fs::create_dir_all(&local_dot_bin).unwrap();
        fs::write(
            local_pkg_dir.join("package.json"),
            r#"{"name":"@openai/codex","version":"0.9.0"}"#,
        )
        .unwrap();
        let local_entry = local_pkg_dir.join("bin").join("codex.js");
        create_executable(&local_entry, "#!/bin/sh\n");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&local_entry, local_dot_bin.join("codex")).unwrap();
        #[cfg(not(unix))]
        create_executable(
            &local_dot_bin.join("codex"),
            &format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", local_entry.display()),
        );
        let env_local_npm = test_env(&[&local_dot_bin, &npm_bin], &home);
        let mut cache_local_npm = PackageManagerCache::new();
        let codex_local_npm = discover_agent(
            AgentId::Codex,
            &env_local_npm,
            &runner,
            &mut cache_local_npm,
        )
        .unwrap();
        assert!(
            matches!(
                codex_local_npm.owner,
                UpdateOwner::ManualActionRequired { .. }
            ),
            "project-local node_modules/.bin/codex must not be classified as NpmGlobal: {:?}",
            codex_local_npm.owner
        );
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
            "#!/bin/sh\nbasedir=$(dirname \"$(echo \"$0\" | sed -e 's,\\\\,/,g')\")\nexec \"$basedir/../Caskroom/claude-code@latest/1.0.20/claude\" \"$@\"\n",
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
            UpdatePlan::Command {
                args,
                env_overrides,
                ..
            } => {
                assert_eq!(
                    args,
                    vec![
                        OsString::from("upgrade"),
                        OsString::from("--cask"),
                        OsString::from("claude-code@latest")
                    ]
                );
                assert!(
                    env_overrides.is_empty(),
                    "Homebrew upgrade must not force HOMEBREW_NO_AUTO_UPDATE=1: {env_overrides:?}"
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

        // 2b. WinGet returning APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE (-1978335189 / 0x8A15002B)
        // with a positive no-newer-release message and verified active version reports `up to date` (exit 0),
        // whereas the same HRESULT without a no-newer-release message reports `failed` (exit 1).
        let winget_not_applicable_code: i32 = 0x8A15_002B_u32 as i32;
        let runner_winget_up_to_date = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                if name.starts_with("winget") {
                    Ok(ok_output("Anthropic.ClaudeCode 1.0.15"))
                } else {
                    Ok(ok_output("1.0.15 (Claude Code)"))
                }
            },
            move |_| {
                Ok(ProcessOutput {
                    status_code: Some(winget_not_applicable_code),
                    success: false,
                    interrupted: false,
                    timed_out: false,
                    stdout:
                        b"Found Claude Code [Anthropic.ClaudeCode]\nNo available upgrade found.\n"
                            .to_vec(),
                    stderr: Vec::new(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                })
            },
        );
        let cmd_wg = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: Some(AgentId::ClaudeCode),
                dry_run: false,
            }),
        };
        let mut wg_out = Vec::new();
        let mut wg_err = Vec::new();
        let wg_code = cmd_wg.run_with_io(
            &env_winget,
            &runner_winget_up_to_date,
            &mut wg_out,
            &mut wg_err,
        );
        assert_eq!(wg_code, 0, "expected exit 0 for WinGet no-op HRESULT");
        assert!(
            String::from_utf8(wg_out)
                .unwrap()
                .contains("up to date (version: 1.0.15"),
            "expected up to date summary for WinGet no-op HRESULT"
        );

        let runner_winget_incompat = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                if name.starts_with("winget") {
                    Ok(ok_output("Anthropic.ClaudeCode 1.0.15"))
                } else {
                    Ok(ok_output("1.0.15 (Claude Code)"))
                }
            },
            move |_| {
                Ok(ProcessOutput {
                    status_code: Some(winget_not_applicable_code),
                    success: false,
                    interrupted: false,
                    timed_out: false,
                    stdout: b"Package is not applicable to the current system.\n".to_vec(),
                    stderr: Vec::new(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                })
            },
        );
        let mut wg_fail_out = Vec::new();
        let mut wg_fail_err = Vec::new();
        let wg_fail_code = cmd_wg.run_with_io(
            &env_winget,
            &runner_winget_incompat,
            &mut wg_fail_out,
            &mut wg_fail_err,
        );
        assert_eq!(
            wg_fail_code, 1,
            "WinGet HRESULT without positive no-newer-release message must fail"
        );
        assert!(
            String::from_utf8(wg_fail_out).unwrap().contains("failed"),
            "expected failed summary when WinGet HRESULT is not a no-newer-release status"
        );
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
        assert!(out_str.starts_with("\nUpdate summary\n"));
        assert_eq!(out_str.lines().count(), 5);
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

        // 2. `cursor-tools/bin/agent` printing bare `1.0.0` without Cursor branding or metadata must be rejected,
        // even when `--help` contains a terminal option such as `--hide-cursor`
        let cursor_tools_bin = tmp.path().join("cursor-tools").join("bin");
        create_executable(&cursor_tools_bin.join("agent"), "#!/bin/sh\n");
        let env_tools = test_env(&[&cursor_tools_bin], &home);
        let runner_bare = FakeProcessRunner::new(
            |req| {
                if req.args.iter().any(|a| a == "--version") {
                    Ok(ok_output("1.0.0"))
                } else {
                    Ok(ok_output(
                        "Usage: agent [options]\n  --hide-cursor  Hide terminal cursor",
                    ))
                }
            },
            |_| unreachable!(),
        );
        let mut cache_tools = PackageManagerCache::new();
        assert!(
            discover_agent(AgentId::Cursor, &env_tools, &runner_bare, &mut cache_tools).is_none()
        );

        // 2b. Unrelated product banner mentioning `Cursor` alongside another product name (`Acme Cursor Agent 1.0.0`) must be rejected
        let runner_acme_cursor = FakeProcessRunner::new(
            |_| Ok(ok_output("Acme Cursor Agent 1.0.0")),
            |_| unreachable!(),
        );
        let mut cache_acme = PackageManagerCache::new();
        assert!(
            discover_agent(
                AgentId::Cursor,
                &env_tools,
                &runner_acme_cursor,
                &mut cache_acme
            )
            .is_none()
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
                "#!/bin/sh\n# ✨ 备注 env node @openai/codex\nexec \"{}\" \"$@\"\n",
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

        // 4. Wrapper that prints `stale` npm path in `printf` and `exec`s independent binary must be rejected
        let indep_bin = tmp.path().join("independent").join("codex");
        create_executable(&indep_bin, "#!/bin/sh\n");
        let stale_pkg_bin = stale_prefix
            .join("lib")
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("bin")
            .join("codex");
        create_executable(&stale_pkg_bin, "#!/bin/sh\n");
        create_executable(
            &stale_bin.join("codex"),
            &format!(
                "#!/bin/sh\nprintf 'Unused npm target: %s\\n' '{}' >&2\nexec '{}' \"$@\"\n",
                stale_pkg_bin.display(),
                indep_bin.display()
            ),
        );
        let mut cache_diag = PackageManagerCache::new();
        let codex_diag =
            discover_agent(AgentId::Codex, &env_stale, &runner_stale, &mut cache_diag).unwrap();
        assert!(matches!(
            codex_diag.owner,
            UpdateOwner::ManualActionRequired { .. }
        ));

        // 4b. Conditional wrapper (`if false; then exec ...`) and non-executed argument wrapper must be rejected,
        // while official npm `cmd-shim` POSIX wrapper pointing to `stale_pkg_bin` succeeds
        create_executable(
            &stale_bin.join("codex"),
            &format!(
                "#!/bin/sh\nif false; then\n  exec '{}' \"$@\"\nfi\nexec '{}' \"$@\"\n",
                stale_pkg_bin.display(),
                indep_bin.display()
            ),
        );
        let mut cache_cond = PackageManagerCache::new();
        let codex_cond =
            discover_agent(AgentId::Codex, &env_stale, &runner_stale, &mut cache_cond).unwrap();
        assert!(matches!(
            codex_cond.owner,
            UpdateOwner::ManualActionRequired { .. }
        ));

        create_executable(
            &stale_bin.join("codex"),
            &format!(
                "#!/bin/sh\nexec '{}' '{}' \"$@\"\n",
                indep_bin.display(),
                stale_pkg_bin.display()
            ),
        );
        let mut cache_arg = PackageManagerCache::new();
        let codex_arg =
            discover_agent(AgentId::Codex, &env_stale, &runner_stale, &mut cache_arg).unwrap();
        assert!(matches!(
            codex_arg.owner,
            UpdateOwner::ManualActionRequired { .. }
        ));

        create_executable(
            &stale_bin.join("codex"),
            "#!/bin/sh\nbasedir=$(dirname \"$(echo \"$0\" | sed -e 's,\\\\,/,g')\")\nif [ -x \"$basedir/node\" ]; then\n  exec \"$basedir/node\" \"$basedir/../lib/node_modules/@openai/codex/bin/codex\" \"$@\"\nelse\n  exec node \"$basedir/../lib/node_modules/@openai/codex/bin/codex\" \"$@\"\nfi\n",
        );
        let mut cache_shim = PackageManagerCache::new();
        let codex_shim =
            discover_agent(AgentId::Codex, &env_stale, &runner_stale, &mut cache_shim).unwrap();
        assert_eq!(
            codex_shim.owner,
            UpdateOwner::NpmGlobal {
                npm_program: stale_bin.join("npm"),
                prefix: stale_prefix.clone(),
                package: "@openai/codex".to_string(),
            }
        );

        // 4c. Reassigned `basedir` (`basedir=<active_prefix>/bin`) and unbound `$basedir` and bare working-directory-relative targets must be rejected
        create_executable(
            &stale_bin.join("codex"),
            &format!(
                "#!/bin/sh\nbasedir='{}'\nexec \"$basedir/../lib/node_modules/@openai/codex/bin/codex\" \"$@\"\n",
                active_prefix.join("bin").display()
            ),
        );
        let mut cache_reassigned = PackageManagerCache::new();
        let codex_reassigned = discover_agent(
            AgentId::Codex,
            &env_stale,
            &runner_stale,
            &mut cache_reassigned,
        )
        .unwrap();
        assert!(
            matches!(
                codex_reassigned.owner,
                UpdateOwner::ManualActionRequired { .. }
            ),
            "reassigned basedir pointing to active prefix must not authorize updating stale prefix: {:?}",
            codex_reassigned.owner
        );

        create_executable(
            &stale_bin.join("codex"),
            "#!/bin/sh\nexec \"../lib/node_modules/@openai/codex/bin/codex\" \"$@\"\n",
        );
        let mut cache_rel = PackageManagerCache::new();
        let codex_rel =
            discover_agent(AgentId::Codex, &env_stale, &runner_stale, &mut cache_rel).unwrap();
        assert!(
            matches!(codex_rel.owner, UpdateOwner::ManualActionRequired { .. }),
            "bare working-directory-relative wrapper target must not be resolved against launcher_dir: {:?}",
            codex_rel.owner
        );

        // 5. Negated Homebrew message "codex is not already installed" and check-start "Checking whether Antigravity is up to date..."
        let owner_prefix_str2 = owner_prefix.to_string_lossy().into_owned();
        let runner_neg_brew = FakeProcessRunner::new(
            move |req| {
                if req.args.iter().any(|a| a == "--prefix") {
                    Ok(ok_output(&owner_prefix_str2))
                } else if req.args.iter().any(|a| a == "list") {
                    Ok(ok_output("codex\n"))
                } else {
                    Ok(ok_output("codex 0.118.0"))
                }
            },
            |_| Ok(ok_output("codex is not already installed")),
        );
        let mut stdout_nb = Vec::new();
        let mut stderr_nb = Vec::new();
        let code_nb = cmd.run_with_io(&env, &runner_neg_brew, &mut stdout_nb, &mut stderr_nb);
        assert_eq!(code_nb, 1);
        assert!(String::from_utf8(stdout_nb).unwrap().contains("unverified"));

        let agy_bin_dir = tmp.path().join("agy-bin");
        create_executable(&agy_bin_dir.join("agy"), "#!/bin/sh\n");
        let env_agy = test_env(&[&agy_bin_dir], &home);
        let runner_agy_check = FakeProcessRunner::new(
            |_| Ok(ok_output("Antigravity CLI 1.0.0")),
            |_| Ok(ok_output("Checking whether Antigravity is up to date...")),
        );
        let cmd_agy = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: Some(AgentId::Antigravity),
                dry_run: false,
            }),
        };
        let mut stdout_ac = Vec::new();
        let mut stderr_ac = Vec::new();
        let code_ac =
            cmd_agy.run_with_io(&env_agy, &runner_agy_check, &mut stdout_ac, &mut stderr_ac);
        assert_eq!(code_ac, 1);
        assert!(String::from_utf8(stdout_ac).unwrap().contains("unverified"));

        // 5b. Homebrew's package-specific cask no-op ("Warning: Not upgrading codex, the latest version is already installed")
        // reports `up to date` (exit 0), whereas a mismatched package token remains `unverified` (exit 1).
        let owner_prefix_str3 = owner_prefix.to_string_lossy().into_owned();
        let runner_brew_noop_ok = FakeProcessRunner::new(
            move |req| {
                if req.args.iter().any(|a| a == "--prefix") {
                    Ok(ok_output(&owner_prefix_str3))
                } else if req.args.iter().any(|a| a == "list") {
                    Ok(ok_output("codex\n"))
                } else {
                    Ok(ok_output("codex 0.118.0"))
                }
            },
            |_| {
                Ok(ProcessOutput {
                    status_code: Some(0),
                    success: true,
                    interrupted: false,
                    timed_out: false,
                    stdout: Vec::new(),
                    stderr:
                        b"Warning: Not upgrading codex, the latest version is already installed\n"
                            .to_vec(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                })
            },
        );
        let mut stdout_noop_ok = Vec::new();
        let mut stderr_noop_ok = Vec::new();
        let code_noop_ok = cmd.run_with_io(
            &env,
            &runner_brew_noop_ok,
            &mut stdout_noop_ok,
            &mut stderr_noop_ok,
        );
        assert_eq!(code_noop_ok, 0);
        assert!(
            String::from_utf8(stdout_noop_ok)
                .unwrap()
                .contains("up to date (version: 0.118.0")
        );

        let owner_prefix_str4 = owner_prefix.to_string_lossy().into_owned();
        let runner_brew_noop_wrong_pkg = FakeProcessRunner::new(
            move |req| {
                if req.args.iter().any(|a| a == "--prefix") {
                    Ok(ok_output(&owner_prefix_str4))
                } else if req.args.iter().any(|a| a == "list") {
                    Ok(ok_output("codex\n"))
                } else {
                    Ok(ok_output("codex 0.118.0"))
                }
            },
            |_| {
                Ok(ok_output(
                    "Warning: Not upgrading other-cask, the latest version is already installed",
                ))
            },
        );
        let mut stdout_noop_wrong = Vec::new();
        let mut stderr_noop_wrong = Vec::new();
        let code_noop_wrong = cmd.run_with_io(
            &env,
            &runner_brew_noop_wrong_pkg,
            &mut stdout_noop_wrong,
            &mut stderr_noop_wrong,
        );
        assert_eq!(code_noop_wrong, 1);
        assert!(
            String::from_utf8(stdout_noop_wrong)
                .unwrap()
                .contains("unverified")
        );

        // 6. Successful update preserves nonempty updater diagnostics on stderr (FR-009).
        let agy_versions = Mutex::new(VecDeque::from(vec![
            "Antigravity CLI 1.0.0",
            "Antigravity CLI 1.1.0",
        ]));
        let runner_agy_diag = FakeProcessRunner::new(
            move |_| {
                let v = agy_versions
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or("Antigravity CLI 1.1.0");
                Ok(ok_output(v))
            },
            |_| {
                Ok(ProcessOutput {
                    status_code: Some(0),
                    success: true,
                    interrupted: false,
                    timed_out: false,
                    stdout: b"Updated Antigravity to 1.1.0\n".to_vec(),
                    stderr: b"Warning: restart your shell to complete setup\n".to_vec(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                })
            },
        );
        let mut stdout_diag = Vec::new();
        let mut stderr_diag = Vec::new();
        let code_diag = cmd_agy.run_with_io(
            &env_agy,
            &runner_agy_diag,
            &mut stdout_diag,
            &mut stderr_diag,
        );
        assert_eq!(code_diag, 0);
        assert!(
            String::from_utf8(stdout_diag)
                .unwrap()
                .contains("updated (1.0.0 -> 1.1.0")
        );
        let stderr_diag_str = String::from_utf8(stderr_diag).unwrap();
        assert!(
            stderr_diag_str.contains("Warning: restart your shell to complete setup"),
            "successful update must preserve vendor diagnostics on stderr: {stderr_diag_str}"
        );

        // 7. Deletion of the selected PATH executable when a shadowed copy exists in a later PATH
        // directory must report `failed` (exit 1), even if the shadowed copy has a different version.
        let primary_agy_dir = tmp.path().join("primary-agy-bin");
        let shadowed_agy_dir = tmp.path().join("shadowed-agy-bin");
        let primary_agy_bin = primary_agy_dir.join("agy");
        let shadowed_agy_bin = shadowed_agy_dir.join("agy");
        create_executable(&primary_agy_bin, "#!/bin/sh\n");
        create_executable(&shadowed_agy_bin, "#!/bin/sh\n");
        let env_shadow = test_env(&[&primary_agy_dir, &shadowed_agy_dir], &home);
        let primary_for_probe = primary_agy_bin.clone();
        let primary_for_update = primary_agy_bin.clone();
        let runner_delete_shadow = FakeProcessRunner::new(
            move |req| {
                if req.program == primary_for_probe {
                    Ok(ok_output("Antigravity CLI 1.1.0"))
                } else {
                    Ok(ok_output("Antigravity CLI 1.0.0"))
                }
            },
            move |_| {
                let _ = fs::remove_file(&primary_for_update);
                Ok(ok_output("Removed primary binary"))
            },
        );
        let mut stdout_shadow = Vec::new();
        let mut stderr_shadow = Vec::new();
        let code_shadow = cmd_agy.run_with_io(
            &env_shadow,
            &runner_delete_shadow,
            &mut stdout_shadow,
            &mut stderr_shadow,
        );
        assert_eq!(
            code_shadow, 1,
            "deleting primary binary and falling through to shadowed copy must fail"
        );
        let stdout_shadow_str = String::from_utf8(stdout_shadow).unwrap();
        assert!(
            stdout_shadow_str.contains("failed"),
            "expected failed outcome when primary binary disappears and falls back to shadowed copy: {stdout_shadow_str}"
        );
    }

    #[test]
    fn review_current_npm_cmd_shim_retains_npm_ownership() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("npm-win");
        let pkg_dir = prefix.join("node_modules").join("@openai").join("codex");
        fs::create_dir_all(pkg_dir.join("bin")).unwrap();
        fs::write(
            pkg_dir.join("package.json"),
            r#"{"name":"@openai/codex","version":"0.118.0"}"#,
        )
        .unwrap();
        fs::write(pkg_dir.join("bin").join("codex.js"), "// js entry").unwrap();
        fs::write(
            prefix.join("codex.cmd"),
            concat!(
                "@ECHO off\r\n",
                "GOTO start\r\n",
                ":find_dp0\r\n",
                "SET dp0=%~dp0\r\n",
                "EXIT /b\r\n",
                ":start\r\n",
                "SETLOCAL\r\n",
                "CALL :find_dp0\r\n",
                "\r\n",
                "IF EXIST \"%dp0%\\node.exe\" (\r\n",
                "  SET \"_prog=%dp0%\\node.exe\"\r\n",
                ") ELSE (\r\n",
                "  SET \"_prog=node\"\r\n",
                "  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n",
                ")\r\n",
                "\r\n",
                "endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & set PATHEXT=%PATHEXT:;.JS;=;% & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n",
            ),
        )
        .unwrap();
        fs::write(prefix.join("npm.cmd"), "@echo off\r\n").unwrap();

        let env = DiscoveryEnv {
            path: OsString::from(prefix.as_os_str()),
            pathext: Some(OsString::from(".CMD;.EXE")),
            home_dir: Some(tmp.path().join("home")),
            local_app_data: Some(tmp.path().join("local")),
            codex_home: None,
            codex_install_dir: None,
            homebrew_prefix: None,
            is_windows: true,
        };
        let runner =
            FakeProcessRunner::new(|_| Ok(ok_output("codex-cli 0.118.0")), |_| unreachable!());
        let mut cache = PackageManagerCache::new();
        let codex = discover_agent(AgentId::Codex, &env, &runner, &mut cache).unwrap();

        assert_eq!(
            codex.owner,
            UpdateOwner::NpmGlobal {
                npm_program: prefix.join("npm.cmd"),
                prefix: prefix.clone(),
                package: "@openai/codex".to_string(),
            }
        );

        // Negative: Windows .cmd shim that reassigns `dp0` or uses a bare working-directory-relative path must be rejected
        fs::write(
            prefix.join("codex.cmd"),
            concat!(
                "@ECHO off\r\n",
                "SET dp0=C:\\other\\prefix\\\r\n",
                "SET \"_prog=%dp0%\\node.exe\"\r\n",
                "endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & set PATHEXT=%PATHEXT:;.JS;=;% & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n",
            ),
        )
        .unwrap();
        let mut cache_bad_dp0 = PackageManagerCache::new();
        let codex_bad_dp0 =
            discover_agent(AgentId::Codex, &env, &runner, &mut cache_bad_dp0).unwrap();
        assert!(matches!(
            codex_bad_dp0.owner,
            UpdateOwner::ManualActionRequired { .. }
        ));

        fs::write(
            prefix.join("codex.cmd"),
            "@ECHO off\r\nnode \"node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n",
        )
        .unwrap();
        let mut cache_rel_cmd = PackageManagerCache::new();
        let codex_rel_cmd =
            discover_agent(AgentId::Codex, &env, &runner, &mut cache_rel_cmd).unwrap();
        assert!(matches!(
            codex_rel_cmd.owner,
            UpdateOwner::ManualActionRequired { .. }
        ));
    }

    struct MockWin32JobBackend {
        events: Mutex<Vec<String>>,
        create_job_res: io::Result<usize>,
        set_kill_res: io::Result<()>,
        assign_res: io::Result<()>,
        threads: io::Result<Vec<(u32, u32)>>,
        thread_details: std::collections::HashMap<u32, (usize, u32, bool, io::Result<u32>)>,
        job_pids_before_kill: Vec<u32>,
        job_pids_after_kill: Vec<u32>,
        in_job_pids: std::collections::BTreeSet<u32>,
        terminated: AtomicBool,
        now_base: Instant,
        elapsed_ms: AtomicUsize,
        wait_step_ms: usize,
        waited_handles: Mutex<Vec<(usize, u32)>>,
        closed_handles: Mutex<Vec<usize>>,
    }

    impl MockWin32JobBackend {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                create_job_res: Ok(777),
                set_kill_res: Ok(()),
                assign_res: Ok(()),
                threads: Ok(vec![(501, 100)]),
                thread_details: std::collections::HashMap::from([(501, (901, 100, true, Ok(1)))]),
                job_pids_before_kill: Vec::new(),
                job_pids_after_kill: Vec::new(),
                in_job_pids: std::collections::BTreeSet::new(),
                terminated: AtomicBool::new(false),
                now_base: Instant::now(),
                elapsed_ms: AtomicUsize::new(0),
                wait_step_ms: 0,
                waited_handles: Mutex::new(Vec::new()),
                closed_handles: Mutex::new(Vec::new()),
            }
        }

        fn clone_io_res<T: Copy>(res: &io::Result<T>) -> io::Result<T> {
            match res {
                Ok(v) => Ok(*v),
                Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
            }
        }
    }

    impl win_process_tree::Win32JobBackend for MockWin32JobBackend {
        fn create_job(&self) -> io::Result<usize> {
            self.events.lock().unwrap().push("create_job".to_string());
            Self::clone_io_res(&self.create_job_res)
        }

        fn set_kill_on_close(&self, job: usize) -> io::Result<()> {
            self.events
                .lock()
                .unwrap()
                .push(format!("set_kill_on_close:{job}"));
            Self::clone_io_res(&self.set_kill_res)
        }

        fn assign_process_to_job(&self, job: usize, process: usize) -> io::Result<()> {
            self.events
                .lock()
                .unwrap()
                .push(format!("assign_process:{job}:{process}"));
            Self::clone_io_res(&self.assign_res)
        }

        fn snapshot_threads(&self) -> io::Result<Vec<(u32, u32)>> {
            self.events
                .lock()
                .unwrap()
                .push("snapshot_threads".to_string());
            match &self.threads {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
            }
        }

        fn open_thread(&self, tid: u32) -> Option<usize> {
            self.thread_details.get(&tid).map(|d| d.0)
        }

        fn thread_owner_pid(&self, thread_handle: usize) -> u32 {
            self.thread_details
                .values()
                .find(|d| d.0 == thread_handle)
                .map(|d| d.1)
                .unwrap_or(0)
        }

        fn is_thread_alive(&self, thread_handle: usize) -> bool {
            self.thread_details
                .values()
                .find(|d| d.0 == thread_handle)
                .map(|d| d.2)
                .unwrap_or(false)
        }

        fn resume_thread(&self, thread_handle: usize) -> io::Result<u32> {
            self.events
                .lock()
                .unwrap()
                .push(format!("resume_thread:{thread_handle}"));
            let Some(detail) = self.thread_details.values().find(|d| d.0 == thread_handle) else {
                return Err(io::Error::new(io::ErrorKind::NotFound, "unknown thread"));
            };
            Self::clone_io_res(&detail.3)
        }

        fn terminate_job(&self, job: usize) {
            self.terminated.store(true, Ordering::SeqCst);
            self.events
                .lock()
                .unwrap()
                .push(format!("terminate_job:{job}"));
        }

        fn query_job_process_ids(&self, _job: usize) -> Vec<u32> {
            if self.terminated.load(Ordering::SeqCst) {
                self.job_pids_after_kill.clone()
            } else {
                self.job_pids_before_kill.clone()
            }
        }

        fn open_sync_process(&self, pid: u32) -> Option<usize> {
            self.events
                .lock()
                .unwrap()
                .push(format!("open_sync_process:{pid}"));
            Some(pid as usize)
        }

        fn is_process_in_job(&self, process_handle: usize, _job: usize) -> bool {
            self.in_job_pids.contains(&(process_handle as u32))
        }

        fn wait_process(&self, process_handle: usize, timeout_ms: u32) {
            self.waited_handles
                .lock()
                .unwrap()
                .push((process_handle, timeout_ms));
            self.elapsed_ms
                .fetch_add(self.wait_step_ms, Ordering::SeqCst);
        }

        fn close_handle(&self, handle: usize) {
            self.closed_handles.lock().unwrap().push(handle);
        }

        fn now(&self) -> Instant {
            self.now_base + Duration::from_millis(self.elapsed_ms.load(Ordering::SeqCst) as u64)
        }
    }

    #[test]
    fn windows_job_containment_and_cleanup_state_machine() {
        // 1. Containment setup assigns process to Job Object strictly BEFORE resuming thread,
        // skipping stale/terminated/unowned threads in the snapshot.
        let mut backend_ok = MockWin32JobBackend::new();
        backend_ok.threads = Ok(vec![
            (400, 999), // different owner PID in snapshot
            (401, 100), // owner PID changed / mismatched GetProcessIdOfThread
            (402, 100), // already terminated thread
            (403, 100), // non-suspended thread (prev suspend count 0)
            (501, 100), // valid suspended primary thread (prev suspend count 1)
        ]);
        backend_ok.thread_details = std::collections::HashMap::from([
            (401, (801, 222, true, Ok(1))),
            (402, (802, 100, false, Ok(1))),
            (403, (803, 100, true, Ok(0))),
            (501, (901, 100, true, Ok(1))),
        ]);

        let job = win_process_tree::setup_contained_process_with_backend(&backend_ok, 42, 100)
            .expect("setup must succeed");
        assert_eq!(job, 777);
        assert_eq!(
            *backend_ok.events.lock().unwrap(),
            vec![
                "create_job",
                "set_kill_on_close:777",
                "assign_process:777:42",
                "snapshot_threads",
                "resume_thread:803",
                "resume_thread:901",
            ],
            "job assignment must precede thread resumption"
        );
        assert_eq!(
            *backend_ok.closed_handles.lock().unwrap(),
            vec![801, 802, 803, 901],
            "all inspected thread handles must be closed"
        );

        // 2. Setup failures abort before child execution and clean up handles.
        let mut fail_create = MockWin32JobBackend::new();
        fail_create.create_job_res = Err(io::Error::from_raw_os_error(5));
        assert!(
            win_process_tree::setup_contained_process_with_backend(&fail_create, 42, 100).is_err()
        );
        assert_eq!(*fail_create.events.lock().unwrap(), vec!["create_job"]);

        let mut fail_limit = MockWin32JobBackend::new();
        fail_limit.set_kill_res = Err(io::Error::from_raw_os_error(5));
        assert!(
            win_process_tree::setup_contained_process_with_backend(&fail_limit, 42, 100).is_err()
        );
        assert_eq!(*fail_limit.closed_handles.lock().unwrap(), vec![777]);

        let mut fail_assign = MockWin32JobBackend::new();
        fail_assign.assign_res = Err(io::Error::from_raw_os_error(5));
        assert!(
            win_process_tree::setup_contained_process_with_backend(&fail_assign, 42, 100).is_err()
        );
        assert_eq!(
            *fail_assign.events.lock().unwrap(),
            vec![
                "create_job",
                "set_kill_on_close:777",
                "assign_process:777:42"
            ],
            "thread must never be resumed when job assignment fails"
        );
        assert_eq!(*fail_assign.closed_handles.lock().unwrap(), vec![777]);

        let mut fail_resume = MockWin32JobBackend::new();
        fail_resume.threads = Ok(vec![]);
        assert!(
            win_process_tree::setup_contained_process_with_backend(&fail_resume, 42, 100).is_err()
        );
        assert!(
            fail_resume.terminated.load(Ordering::SeqCst),
            "job must be terminated if resuming the primary thread fails"
        );
        assert_eq!(*fail_resume.closed_handles.lock().unwrap(), vec![777]);

        // 3. Timeout cleanup uses verified job membership, includes early grandchild 300 whose
        // intermediate parent 200 exited, rejects unrelated PID 200 via IsProcessInJob, and shares
        // a single 500ms cleanup deadline across all handles.
        let mut cleanup_backend = MockWin32JobBackend::new();
        cleanup_backend.job_pids_before_kill = vec![100, 200, 300];
        cleanup_backend.job_pids_after_kill = vec![100, 300, 400];
        cleanup_backend.in_job_pids = std::collections::BTreeSet::from([100, 300, 400]);
        cleanup_backend.wait_step_ms = 200;

        let mut root_killed = false;
        win_process_tree::terminate_job_and_wait_with_backend(&cleanup_backend, 777, 9999, || {
            root_killed = true;
        });
        assert!(root_killed);
        assert!(cleanup_backend.terminated.load(Ordering::SeqCst));
        let waited = cleanup_backend.waited_handles.lock().unwrap().clone();
        assert_eq!(
            waited,
            vec![(100, 500), (300, 300), (400, 100)],
            "only verified job members [100, 300, 400] may be waited on, using a single shared 500ms budget (unrelated PID 200 excluded)"
        );
        assert_eq!(
            *cleanup_backend.closed_handles.lock().unwrap(),
            vec![200, 100, 300, 400],
            "unverified handle 200 must be closed immediately and verified handles closed after waiting"
        );
    }

    const HELPER_PROBE_TREE_RS: &str = r#"
use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::Duration;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    match mode {
        "--sentinel" => {
            if let Some(p) = args.get(2) {
                let _ = fs::write(p, format!("{}", std::process::id()));
            }
            thread::sleep(Duration::from_secs(20));
            ExitCode::from(0)
        }
        "--holder" => {
            if let Some(p) = args.get(2) {
                let _ = fs::write(p, format!("{}", std::process::id()));
            }
            println!("holder-piped-stdout");
            let _ = std::io::stdout().flush();
            eprintln!("holder-piped-stderr");
            let _ = std::io::stderr().flush();
            thread::sleep(Duration::from_secs(20));
            ExitCode::from(0)
        }
        "--bridge" => {
            let exe = env::current_exe().expect("current_exe");
            let bridge_pid_file = args.get(2).map(String::as_str).unwrap_or("");
            let holder_pid_file = args.get(3).map(String::as_str).unwrap_or("");
            if !bridge_pid_file.is_empty() {
                let _ = fs::write(bridge_pid_file, format!("{}", std::process::id()));
            }
            let _ = Command::new(&exe)
                .arg("--holder")
                .arg(holder_pid_file)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn holder");
            for _ in 0..200 {
                if fs::read_to_string(holder_pid_file)
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
                {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            ExitCode::from(0)
        }
        "--probe" => {
            let exe = env::current_exe().expect("current_exe");
            let parent_pid_file = args.get(2).map(String::as_str).unwrap_or("");
            let bridge_pid_file = args.get(3).map(String::as_str).unwrap_or("");
            let holder_pid_file = args.get(4).map(String::as_str).unwrap_or("");
            if !parent_pid_file.is_empty() {
                let _ = fs::write(parent_pid_file, format!("{}", std::process::id()));
            }
            println!("probe-piped-stdout");
            let _ = std::io::stdout().flush();
            eprintln!("probe-piped-stderr");
            let _ = std::io::stderr().flush();
            let _ = Command::new(&exe)
                .arg("--bridge")
                .arg(bridge_pid_file)
                .arg(holder_pid_file)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status();
            thread::sleep(Duration::from_secs(20));
            ExitCode::from(0)
        }
        _ => {
            if let Ok(marker) = env::var("TEEMIAO_UNIT_PROBE_MARKER_FILE") {
                let _ = fs::write(marker, format!("{}", std::process::id()));
            }
            println!("Antigravity CLI 1.0.0");
            ExitCode::from(0)
        }
    }
}
"#;

    fn compiled_unit_probe_tree_binary() -> &'static std::path::Path {
        static BIN: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        BIN.get_or_init(|| {
            let pid = std::process::id();
            let dir = std::env::temp_dir().join(format!("teemiao-unit-probe-tree-{pid}"));
            fs::create_dir_all(&dir).expect("create unit probe helper dir");
            let src = dir.join("probe_tree.rs");
            let bin = dir.join(if cfg!(windows) {
                "probe_tree.exe"
            } else {
                "probe_tree"
            });
            fs::write(&src, HELPER_PROBE_TREE_RS).expect("write probe_tree.rs");
            let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
            let status = Command::new(rustc)
                .arg(&src)
                .arg("-O")
                .arg("-o")
                .arg(&bin)
                .status()
                .expect("compile probe_tree.rs");
            assert!(status.success(), "rustc failed on probe_tree.rs");
            bin
        })
    }

    fn is_unit_test_process_alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            for _ in 0..10 {
                let status = Command::new("/bin/kill")
                    .args(["-0", &pid.to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                if !status.is_ok_and(|s| s.success()) {
                    return false;
                }
                thread::sleep(Duration::from_millis(10));
            }
            true
        }
        #[cfg(windows)]
        {
            const SYNCHRONIZE: u32 = 0x0010_0000;
            const WAIT_TIMEOUT: u32 = 0x0000_0102;
            unsafe extern "system" {
                fn OpenProcess(
                    dw_desired_access: u32,
                    b_inherit_handle: i32,
                    dw_process_id: u32,
                ) -> *mut std::ffi::c_void;
                fn WaitForSingleObject(
                    h_handle: *mut std::ffi::c_void,
                    dw_milliseconds: u32,
                ) -> u32;
                fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
            }
            // SAFETY: Queries whether `pid` is still active using a non-blocking wait on a `SYNCHRONIZE` handle.
            unsafe {
                let handle = OpenProcess(SYNCHRONIZE, 0, pid);
                if handle.is_null() {
                    return false;
                }
                let rc = WaitForSingleObject(handle, 0);
                let _ = CloseHandle(handle);
                rc == WAIT_TIMEOUT
            }
        }
    }

    fn kill_unit_test_process_best_effort(pid: u32) {
        #[cfg(unix)]
        {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &pid.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    #[test]
    fn real_process_runner_containment_early_descendant_and_setup_failure() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let helper = compiled_unit_probe_tree_binary();

        let sentinel_pid_file = tmp.path().join("sentinel.pid");
        let parent_pid_file = tmp.path().join("parent.pid");
        let bridge_pid_file = tmp.path().join("bridge.pid");
        let holder_pid_file = tmp.path().join("holder.pid");

        let mut sentinel = Command::new(helper)
            .arg("--sentinel")
            .arg(&sentinel_pid_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn unrelated sentinel");
        let sentinel_pid = sentinel.id();

        let mut runner = RealProcessRunner::new();
        runner.pre_attach_delay = Duration::from_millis(300);
        runner.pre_attach_check_absent_path = Some(parent_pid_file.clone());

        let req = ProcessRequest {
            program: helper.to_path_buf(),
            args: vec![
                OsString::from("--probe"),
                parent_pid_file.as_os_str().to_os_string(),
                bridge_pid_file.as_os_str().to_os_string(),
                holder_pid_file.as_os_str().to_os_string(),
            ],
            env_overrides: Vec::new(),
            timeout: Some(Duration::from_millis(900)),
            max_output_bytes: 65536,
        };

        let out = runner
            .run_probe(&req)
            .expect("run_probe must return output");
        assert!(out.timed_out, "probe must time out");

        #[cfg(windows)]
        assert!(
            runner.pre_attach_path_was_absent.load(Ordering::SeqCst),
            "on Windows, CREATE_SUSPENDED must prevent child execution during the 300ms pre-attach scheduling gap"
        );

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stdout.contains("probe-piped-stdout") && stdout.contains("holder-piped-stdout"),
            "piped stdout from both parent and early grandchild must be retained: {stdout}"
        );
        assert!(
            stderr.contains("probe-piped-stderr") && stderr.contains("holder-piped-stderr"),
            "piped stderr from both parent and early grandchild must be retained: {stderr}"
        );

        let parent_pid: u32 = fs::read_to_string(&parent_pid_file)
            .expect("parent pid file")
            .trim()
            .parse()
            .unwrap();
        let bridge_pid: u32 = fs::read_to_string(&bridge_pid_file)
            .expect("bridge pid file")
            .trim()
            .parse()
            .unwrap();
        let holder_pid: u32 = fs::read_to_string(&holder_pid_file)
            .expect("holder pid file")
            .trim()
            .parse()
            .unwrap();

        let parent_alive = is_unit_test_process_alive(parent_pid);
        let bridge_alive = is_unit_test_process_alive(bridge_pid);
        let holder_alive = is_unit_test_process_alive(holder_pid);
        let sentinel_alive = is_unit_test_process_alive(sentinel_pid);

        let _ = sentinel.kill();
        let _ = sentinel.wait();
        if parent_alive {
            kill_unit_test_process_best_effort(parent_pid);
        }
        if bridge_alive {
            kill_unit_test_process_best_effort(bridge_pid);
        }
        if holder_alive {
            kill_unit_test_process_best_effort(holder_pid);
        }

        assert!(
            !bridge_alive,
            "intermediate bridge process {bridge_pid} must have exited before timeout"
        );
        assert!(
            !parent_alive,
            "parent probe process {parent_pid} survived production timeout cleanup"
        );
        assert!(
            !holder_alive,
            "early grandchild pipe-holder {holder_pid} (with exited intermediate parent) survived production timeout cleanup"
        );
        assert!(
            sentinel_alive,
            "unrelated sentinel process {sentinel_pid} must remain alive across timeout cleanup"
        );

        #[cfg(windows)]
        {
            let setup_fail_marker = tmp.path().join("setup-fail-never-ran.pid");
            let mut fail_runner = RealProcessRunner::new();
            fail_runner.force_job_assign_access_denied = true;

            let fail_req = ProcessRequest {
                program: helper.to_path_buf(),
                args: vec![
                    OsString::from("--probe"),
                    setup_fail_marker.as_os_str().to_os_string(),
                    bridge_pid_file.as_os_str().to_os_string(),
                    holder_pid_file.as_os_str().to_os_string(),
                ],
                env_overrides: Vec::new(),
                timeout: Some(Duration::from_secs(5)),
                max_output_bytes: 65536,
            };
            let err = fail_runner
                .run_probe(&fail_req)
                .expect_err("job setup failure must return ProcessError::Io");
            assert!(
                !setup_fail_marker.exists(),
                "suspended child must be killed and reaped before executing any instructions when job setup fails: {err}"
            );

            let bin_dir = tmp.path().join("win-setup-fail-bin");
            fs::create_dir_all(&bin_dir).unwrap();
            let agy_exe = bin_dir.join("agy.exe");
            fs::copy(helper, &agy_exe).unwrap();
            let env = DiscoveryEnv {
                path: OsString::from(bin_dir.as_os_str()),
                pathext: Some(OsString::from(".EXE")),
                home_dir: Some(tmp.path().join("home")),
                local_app_data: Some(tmp.path().join("local")),
                codex_home: None,
                codex_install_dir: None,
                homebrew_prefix: None,
                is_windows: true,
            };
            let cmd = AgentsCommand {
                command: AgentsSubcommand::List,
            };
            let mut list_stdout = Vec::new();
            let mut list_stderr = Vec::new();
            let code = cmd.run_with_io(&env, &fail_runner, &mut list_stdout, &mut list_stderr);
            assert_eq!(code, 0);
            let list_out = String::from_utf8(list_stdout).unwrap();
            let list_err = String::from_utf8(list_stderr).unwrap();
            assert!(
                list_out.contains("Antigravity (antigravity) | version: unknown"),
                "list output must degrade to version unknown on job setup error: {list_out}"
            );
            assert!(
                list_err.contains("version probe for Antigravity")
                    && list_err.contains("failed (failed to execute"),
                "stderr must report probe failure when job setup fails: {list_err}"
            );
        }
    }

    #[derive(Clone, Default)]
    struct SharedRecordingWriter {
        state: Arc<Mutex<(Vec<u8>, usize)>>,
    }

    impl SharedRecordingWriter {
        fn snapshot(&self) -> (String, usize) {
            let guard = self.state.lock().unwrap();
            (String::from_utf8_lossy(&guard.0).into_owned(), guard.1)
        }
    }

    impl Write for SharedRecordingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut guard = self.state.lock().unwrap();
            guard.0.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            let mut guard = self.state.lock().unwrap();
            guard.1 += 1;
            Ok(())
        }
    }

    #[test]
    fn update_output_separates_sections_flushes_headers_before_updater_and_localizes_summary() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        let bin = home.join(".local").join("bin");
        create_executable(&bin.join("agy"), "#!/bin/sh\n");
        create_executable(&bin.join("opencode"), "#!/bin/sh\n");
        create_executable(&bin.join("kimi"), "#!/bin/sh\n");

        let env = test_env(&[&bin], &home);
        let shared_stderr = SharedRecordingWriter::default();
        let stderr_in_runner = shared_stderr.clone();
        let observed_pre_update: Arc<Mutex<Vec<(String, String, usize)>>> =
            Arc::new(Mutex::new(Vec::new()));
        let observed_clone = Arc::clone(&observed_pre_update);

        let agy_versions = Mutex::new(VecDeque::from(vec![
            "Antigravity CLI 1.0.0",
            "Antigravity CLI 1.1.0",
        ]));
        let runner = FakeProcessRunner::new(
            move |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                match name.as_ref() {
                    "agy" => {
                        let v = agy_versions
                            .lock()
                            .unwrap()
                            .pop_front()
                            .unwrap_or("Antigravity CLI 1.1.0");
                        Ok(ok_output(v))
                    }
                    "opencode" => Ok(ok_output("opencode 0.5.0")),
                    "kimi" => Ok(ok_output("kimi 0.4.0")),
                    _ => unreachable!(),
                }
            },
            move |req| {
                let name = req
                    .program
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                let (snap, flushes) = stderr_in_runner.snapshot();
                observed_clone
                    .lock()
                    .unwrap()
                    .push((name.clone(), snap, flushes));

                match name.as_str() {
                    "agy" => Ok(ProcessOutput {
                        status_code: Some(0),
                        success: true,
                        interrupted: false,
                        timed_out: false,
                        stdout: b"agy step 1\nagy step 2 without newline".to_vec(),
                        stderr: b"agy warn line 1\nagy warn line 2".to_vec(),
                        stdout_truncated: false,
                        stderr_truncated: false,
                    }),
                    "opencode" => Ok(ProcessOutput {
                        status_code: Some(1),
                        success: false,
                        interrupted: false,
                        timed_out: false,
                        stdout: b"opencode stdout detail".to_vec(),
                        stderr: b"opencode error line 1\nopencode error line 2".to_vec(),
                        stdout_truncated: false,
                        stderr_truncated: false,
                    }),
                    "kimi" => Ok(ProcessOutput {
                        status_code: Some(0),
                        success: true,
                        interrupted: false,
                        timed_out: false,
                        stdout: b"kimi is already up to date".to_vec(),
                        stderr: Vec::new(),
                        stdout_truncated: false,
                        stderr_truncated: false,
                    }),
                    _ => unreachable!(),
                }
            },
        );

        let cmd = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: None,
                dry_run: false,
            }),
        };
        let mut stdout = Vec::new();
        let mut stderr_writer = shared_stderr.clone();
        let code = cmd.run_with_io(&env, &runner, &mut stdout, &mut stderr_writer);
        assert_eq!(code, 1);

        let observations = observed_pre_update.lock().unwrap().clone();
        assert_eq!(observations.len(), 3);
        // 1. Before `agy` updater runs, its header is already written and flushed to stderr.
        assert_eq!(
            observations[0].1,
            "--- [1/3] Antigravity (antigravity) ---\n"
        );
        assert!(observations[0].2 >= 1);

        // 2. Before `opencode` updater runs, `agy` diagnostics ended with a newline, followed by a blank line and header 2/3.
        assert!(
            observations[1]
                .1
                .starts_with("--- [1/3] Antigravity (antigravity) ---\nDiagnostics for Antigravity (antigravity): agy warn line 1\nagy warn line 2\nagy step 1\nagy step 2 without newline\n\n--- [2/3] OpenCode (opencode) ---\n"),
            "unexpected stderr before opencode updater: {:?}",
            observations[1].1
        );
        assert!(observations[1].2 > observations[0].2);

        // 3. Before `kimi` updater runs, `opencode` failure and multiline diagnostics are confined to section 2/3, followed by a blank line and header 3/3.
        assert!(
            observations[2]
                .1
                .ends_with("opencode error line 1\nopencode error line 2\nopencode stdout detail\n\n--- [3/3] Kimi (kimi) ---\n"),
            "unexpected stderr before kimi updater: {:?}",
            observations[2].1
        );

        let (final_stderr, _) = shared_stderr.snapshot();
        assert!(final_stderr.ends_with('\n'));
        assert!(!final_stderr.contains("Update summary"));

        let final_stdout = String::from_utf8(stdout).unwrap();
        assert!(
            final_stdout.starts_with("\nUpdate summary\nAntigravity (antigravity): updated"),
            "expected leading blank line and 'Update summary' on stdout, got: {final_stdout:?}"
        );
        let stdout_lines: Vec<&str> = final_stdout.lines().collect();
        assert_eq!(stdout_lines[0], "");
        assert_eq!(stdout_lines[1], "Update summary");
        assert!(stdout_lines[2].starts_with("Antigravity (antigravity): updated (1.0.0 -> 1.1.0;"));
        assert!(stdout_lines[3].starts_with("OpenCode (opencode): failed"));
        assert!(stdout_lines[4].starts_with("Kimi (kimi): up to date (version: 0.4.0;"));

        // Cancellation on agent 1 of 3 must not print headers for agents 2 or 3
        let runner_cancel = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                Ok(ok_output(&format!("{name} 1.0.0")))
            },
            |_| {
                Ok(ProcessOutput {
                    status_code: Some(130),
                    success: false,
                    interrupted: true,
                    timed_out: false,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                })
            },
        );
        let mut cancel_out = Vec::new();
        let mut cancel_err = Vec::new();
        let cancel_code = cmd.run_with_io(&env, &runner_cancel, &mut cancel_out, &mut cancel_err);
        assert_eq!(cancel_code, 130);
        let cancel_err_str = String::from_utf8(cancel_err).unwrap();
        assert!(cancel_err_str.contains("--- [1/3] Antigravity (antigravity) ---"));
        assert!(!cancel_err_str.contains("--- [2/3]"));
        assert!(!cancel_err_str.contains("--- [3/3]"));

        // Single-agent dry-run still renders a `[1/1]` section header and a separated summary block,
        // and Simplified Chinese (`zh-CN`) localization keys are verified without mutating global locale across parallel unit tests.
        let single_dry_cmd = AgentsCommand {
            command: AgentsSubcommand::Update(UpdateArgs {
                agent: Some(AgentId::Antigravity),
                dry_run: true,
            }),
        };
        let mut single_out = Vec::new();
        let mut single_err = Vec::new();
        let single_code =
            single_dry_cmd.run_with_io(&env, &runner, &mut single_out, &mut single_err);
        assert_eq!(single_code, 0);
        let single_err_str = String::from_utf8(single_err).unwrap();
        let single_out_str = String::from_utf8(single_out).unwrap();
        assert_eq!(single_err_str, "--- [1/1] Antigravity (antigravity) ---\n");
        assert!(
            single_out_str.starts_with("\nUpdate summary\nAntigravity (antigravity): dry-run"),
            "expected single-agent summary header and dry-run row: {single_out_str:?}"
        );
        assert_eq!(
            t!(
                "agents.update.section_header",
                locale = "zh-CN",
                index = 1,
                total = 1,
                name = "Antigravity",
                id = "antigravity"
            )
            .as_ref(),
            "--- [1/1] Antigravity (antigravity) ---"
        );
        assert_eq!(
            t!("agents.update.summary_header", locale = "zh-CN").as_ref(),
            "更新摘要"
        );
        assert_eq!(
            t!(
                "agents.update.dry_run_plan",
                locale = "zh-CN",
                name = "Antigravity",
                id = "antigravity",
                command = "agy update",
                target = "/bin/agy",
                version = "1.0.0"
            )
            .as_ref(),
            "Antigravity (antigravity): 演练模式 (将运行: agy update; 目标: /bin/agy; 当前版本: 1.0.0)"
        );
    }

    #[test]
    fn qoder_discovery_precedence_desktop_rejection_native_and_npm_ownership() {
        rust_i18n::set_locale("en");
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");

        // 1. Catalog order places Qoder 7th after Claude Code; `qodercli` is preferred over `qoder`
        let bin_primary = tmp.path().join("bin-primary");
        let bin_secondary = tmp.path().join("bin-secondary");
        create_executable(&bin_primary.join("claude"), "#!/bin/sh\n");
        create_executable(&bin_primary.join("qoder"), "#!/bin/sh\n");
        create_executable(&bin_secondary.join("qodercli"), "#!/bin/sh\n");
        let env_both = test_env(&[&bin_primary, &bin_secondary], &home);
        let runner_both = FakeProcessRunner::new(
            |req| {
                let name = req.program.file_name().unwrap().to_string_lossy();
                let is_help = req.args.iter().any(|a| a == "--help");
                match (name.as_ref(), is_help) {
                    ("claude", false) => Ok(ok_output("2.1.0 (Claude Code)")),
                    ("qodercli", false) => Ok(ok_output("qodercli 0.2.1")),
                    ("qodercli", true) => Ok(ok_output(
                        "Qoder CLI\nUsage: qodercli [command]\nCommands:\n  update  Update Qoder CLI",
                    )),
                    ("qoder", false) => Ok(ok_output("Qoder CLI 0.1.9")),
                    ("qoder", true) => Ok(ok_output(
                        "Qoder CLI\nUsage: qoder [command]\nCommands:\n  update  Update Qoder CLI",
                    )),
                    _ => unreachable!(),
                }
            },
            |_| unreachable!(),
        );
        let mut cache = PackageManagerCache::new();
        let all = discover_all_agents(&env_both, &runner_both, &mut cache);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, AgentId::ClaudeCode);
        assert_eq!(all[1].id, AgentId::Qoder);
        assert_eq!(all[1].launcher_path, bin_secondary.join("qodercli"));
        assert_eq!(all[1].version.as_deref(), Some("0.2.1"));
        assert_eq!(
            all[1].owner,
            UpdateOwner::NativeSelfUpdate {
                program: bin_secondary.join("qodercli"),
                args: vec!["update".to_string()],
                display_command: "qodercli update".to_string(),
            }
        );

        // 2. Fallback to verified `qoder` when `qodercli` is absent or unverified
        let bin_fallback = tmp.path().join("bin-fallback");
        create_executable(&bin_fallback.join("qoder"), "#!/bin/sh\n");
        let env_fallback = test_env(&[&bin_fallback], &home);
        let runner_fallback = FakeProcessRunner::new(
            |req| {
                if req.args.iter().any(|a| a == "--help") {
                    Ok(ok_output(
                        "Qoder CLI\nUsage: qoder [command]\nCommands:\n  update  Update Qoder CLI",
                    ))
                } else {
                    Ok(ok_output("0.2.0"))
                }
            },
            |_| unreachable!(),
        );
        let mut cache_fb = PackageManagerCache::new();
        let qoder_fb = discover_agent(
            AgentId::Qoder,
            &env_fallback,
            &runner_fallback,
            &mut cache_fb,
        )
        .unwrap();
        assert_eq!(qoder_fb.launcher_path, bin_fallback.join("qoder"));
        assert_eq!(qoder_fb.version.as_deref(), Some("0.2.0"));
        assert_eq!(
            qoder_fb.owner,
            UpdateOwner::NativeSelfUpdate {
                program: bin_fallback.join("qoder"),
                args: vec!["update".to_string()],
                display_command: "qoder update".to_string(),
            }
        );

        // 3. Reject desktop/IDE `qoder`, bare product/version `qoder` without CLI help/update evidence, and conflicting banners
        for (ver_out, help_out) in [
            ("Qoder Desktop 1.2.0", "Usage: qoder --open-window"),
            ("Qoder IDE 1.2.0", "Qoder IDE"),
            ("Qoder 1.2.0", "Usage: qoder [file]"),
            ("1.2.0", ""),
            ("Datadog Agent 7.50.0", "Datadog agent"),
            ("Acme Qoder CLI 1.0.0", "Commands:\n  update  Update"),
        ] {
            let runner_reject = FakeProcessRunner::new(
                move |req| {
                    if req.args.iter().any(|a| a == "--help") {
                        Ok(ok_output(help_out))
                    } else {
                        Ok(ok_output(ver_out))
                    }
                },
                |_| unreachable!(),
            );
            let mut cache_rej = PackageManagerCache::new();
            assert!(
                discover_agent(
                    AgentId::Qoder,
                    &env_fallback,
                    &runner_reject,
                    &mut cache_rej
                )
                .is_none(),
                "expected qoder candidate with version={ver_out:?} help={help_out:?} to be rejected"
            );
        }

        // 4. Qoder CLI without `update` interface in `--help` (and not proven npm) is discovered with ManualActionRequired
        let runner_no_update_cmd = FakeProcessRunner::new(
            |req| {
                if req.args.iter().any(|a| a == "--help") {
                    Ok(ok_output("Qoder CLI\nUsage: qoder [options]\n  --help"))
                } else {
                    Ok(ok_output("Qoder CLI 0.2.0"))
                }
            },
            |_| unreachable!(),
        );
        let mut cache_manual = PackageManagerCache::new();
        let qoder_manual = discover_agent(
            AgentId::Qoder,
            &env_fallback,
            &runner_no_update_cmd,
            &mut cache_manual,
        )
        .unwrap();
        assert!(matches!(
            qoder_manual.owner,
            UpdateOwner::ManualActionRequired { .. }
        ));

        // 5. Proven npm `@qoder-ai/qodercli` ownership (without needing `--help` probe) and missing-npm manual fallback
        let npm_prefix = tmp.path().join("npm-qoder");
        let npm_bin = npm_prefix.join("bin");
        let pkg_dir = npm_prefix
            .join("lib")
            .join("node_modules")
            .join("@qoder-ai")
            .join("qodercli");
        fs::create_dir_all(pkg_dir.join("bin")).unwrap();
        fs::write(
            pkg_dir.join("package.json"),
            r#"{"name":"@qoder-ai/qodercli","version":"0.2.0"}"#,
        )
        .unwrap();
        create_executable(
            &npm_bin.join("qodercli"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                pkg_dir.join("bin").join("qodercli.js").display()
            ),
        );
        create_executable(&npm_bin.join("npm"), "#!/bin/sh\n");

        let env_npm = test_env(&[&npm_bin], &home);
        let runner_npm = FakeProcessRunner::new(
            |req| {
                assert!(
                    !req.args.iter().any(|a| a == "--help"),
                    "proven npm @qoder-ai/qodercli should not need a --help probe"
                );
                Ok(ok_output("qodercli 0.2.0"))
            },
            |_| unreachable!(),
        );
        let mut cache_npm = PackageManagerCache::new();
        let qoder_npm =
            discover_agent(AgentId::Qoder, &env_npm, &runner_npm, &mut cache_npm).unwrap();
        assert_eq!(
            qoder_npm.owner,
            UpdateOwner::NpmGlobal {
                npm_program: npm_bin.join("npm"),
                prefix: npm_prefix.clone(),
                package: "@qoder-ai/qodercli".to_string(),
            }
        );

        // Removing `npm` from PATH must yield ManualActionRequired and never fall back to native self-update
        fs::remove_file(npm_bin.join("npm")).unwrap();
        let runner_missing_npm = FakeProcessRunner::new(
            |req| {
                if req.args.iter().any(|a| a == "--help") {
                    Ok(ok_output(
                        "Qoder CLI\nUsage: qodercli [command]\nCommands:\n  update  Update Qoder CLI",
                    ))
                } else {
                    Ok(ok_output("qodercli 0.2.0"))
                }
            },
            |_| unreachable!(),
        );
        let mut cache_missing_npm = PackageManagerCache::new();
        let qoder_missing_npm = discover_agent(
            AgentId::Qoder,
            &env_npm,
            &runner_missing_npm,
            &mut cache_missing_npm,
        )
        .unwrap();
        assert!(
            matches!(
                qoder_missing_npm.owner,
                UpdateOwner::ManualActionRequired { .. }
            ),
            "npm-owned Qoder without npm on PATH must not fall back to native update: {:?}",
            qoder_missing_npm.owner
        );

        // 6. Wrong-prefix wrapper in `stale/bin/qodercli` pointing to `active` must yield ManualActionRequired (neither stale npm nor native update)
        let stale_prefix = tmp.path().join("npm-qoder-stale");
        let stale_bin = stale_prefix.join("bin");
        let stale_pkg_dir = stale_prefix
            .join("lib")
            .join("node_modules")
            .join("@qoder-ai")
            .join("qodercli");
        fs::create_dir_all(&stale_bin).unwrap();
        fs::create_dir_all(&stale_pkg_dir).unwrap();
        fs::write(
            stale_pkg_dir.join("package.json"),
            r#"{"name":"@qoder-ai/qodercli","version":"0.2.0"}"#,
        )
        .unwrap();
        create_executable(
            &stale_bin.join("qodercli"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                pkg_dir.join("bin").join("qodercli.js").display()
            ),
        );
        create_executable(&stale_bin.join("npm"), "#!/bin/sh\n");
        let env_stale = test_env(&[&stale_bin], &home);
        let mut cache_stale = PackageManagerCache::new();
        let qoder_stale = discover_agent(
            AgentId::Qoder,
            &env_stale,
            &runner_missing_npm,
            &mut cache_stale,
        )
        .unwrap();
        assert!(
            matches!(qoder_stale.owner, UpdateOwner::ManualActionRequired { .. }),
            "wrong-prefix wrapper must not authorize stale npm update or native self-update: {:?}",
            qoder_stale.owner
        );

        // 7. Unsupported manager (.bun) must yield ManualActionRequired even if --help has `update`
        let bun_bin = home.join(".bun").join("bin");
        create_executable(&bun_bin.join("qodercli"), "#!/bin/sh\n");
        let env_bun = test_env(&[&bun_bin], &home);
        let mut cache_bun = PackageManagerCache::new();
        let qoder_bun = discover_agent(
            AgentId::Qoder,
            &env_bun,
            &runner_missing_npm,
            &mut cache_bun,
        )
        .unwrap();
        assert!(
            matches!(qoder_bun.owner, UpdateOwner::ManualActionRequired { ref reason, .. } if reason.contains("bun")),
            "expected bun-managed Qoder to require manual action: {:?}",
            qoder_bun.owner
        );

        // 8. Windows .cmd shim discovery: valid npm .cmd shim and unsafe .cmd path metacharacter guard
        let win_prefix = tmp.path().join("win_npm_qoder");
        let win_pkg = win_prefix
            .join("node_modules")
            .join("@qoder-ai")
            .join("qodercli");
        fs::create_dir_all(win_pkg.join("bin")).unwrap();
        fs::write(
            win_pkg.join("package.json"),
            r#"{"name":"@qoder-ai/qodercli","version":"0.2.0"}"#,
        )
        .unwrap();
        fs::write(
            win_prefix.join("qodercli.cmd"),
            "@echo off\r\n\"%~dp0\\node_modules\\@qoder-ai\\qodercli\\bin\\qodercli.exe\" %*\r\n",
        )
        .unwrap();
        fs::write(win_prefix.join("npm.cmd"), "@echo off\r\n").unwrap();
        let env_win = DiscoveryEnv {
            path: OsString::from(win_prefix.to_string_lossy().as_ref()),
            pathext: Some(OsString::from(".COM;.EXE;.BAT;.CMD")),
            home_dir: Some(home.clone()),
            local_app_data: None,
            codex_home: None,
            codex_install_dir: None,
            homebrew_prefix: None,
            is_windows: true,
        };
        let mut cache_win = PackageManagerCache::new();
        let qoder_win =
            discover_agent(AgentId::Qoder, &env_win, &runner_npm, &mut cache_win).unwrap();
        assert_eq!(
            qoder_win.owner,
            UpdateOwner::NpmGlobal {
                npm_program: win_prefix.join("npm.cmd"),
                prefix: win_prefix.clone(),
                package: "@qoder-ai/qodercli".to_string(),
            }
        );

        let unsafe_win_dir = tmp.path().join("win&unsafe");
        fs::create_dir_all(&unsafe_win_dir).unwrap();
        fs::write(unsafe_win_dir.join("qodercli.cmd"), "@echo off\r\n").unwrap();
        let env_unsafe_win = DiscoveryEnv {
            path: OsString::from(unsafe_win_dir.to_string_lossy().as_ref()),
            pathext: Some(OsString::from(".COM;.EXE;.BAT;.CMD")),
            home_dir: Some(home.clone()),
            local_app_data: None,
            codex_home: None,
            codex_install_dir: None,
            homebrew_prefix: None,
            is_windows: true,
        };
        let runner_must_not_run = FakeProcessRunner::new(|_| unreachable!(), |_| unreachable!());
        let mut cache_unsafe = PackageManagerCache::new();
        let qoder_unsafe = discover_agent(
            AgentId::Qoder,
            &env_unsafe_win,
            &runner_must_not_run,
            &mut cache_unsafe,
        )
        .unwrap();
        assert_eq!(qoder_unsafe.version, None);
        assert!(matches!(
            qoder_unsafe.owner,
            UpdateOwner::ManualActionRequired { .. }
        ));

        // 9. Preserve ownership through wrappers: cross-prefix Homebrew Cellar/Caskroom, foreign manager,
        // and unresolved forwarding wrappers must remain ManualActionRequired and never permit native update
        let wrapper_bin = tmp.path().join("wrapper").join("bin");
        let separate_cellar_bin = tmp
            .path()
            .join("separate-brew")
            .join("Cellar")
            .join("qodercli")
            .join("0.9.0")
            .join("bin")
            .join("qodercli");
        create_executable(&separate_cellar_bin, "#!/bin/sh\n");
        create_executable(
            &wrapper_bin.join("qodercli"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                separate_cellar_bin.display()
            ),
        );
        let env_wrapped_brew = test_env(&[&wrapper_bin], &home);
        let runner_affirmative = FakeProcessRunner::new(
            |req| {
                if req.args.iter().any(|a| a == "--help") {
                    Ok(ok_output(
                        "Qoder CLI\nCommands:\n  update  Update Qoder CLI",
                    ))
                } else {
                    Ok(ok_output("Qoder CLI 0.9.0"))
                }
            },
            |_| panic!("managed wrapper must never invoke updater"),
        );
        let mut cache_wb = PackageManagerCache::new();
        let qoder_wb = discover_agent(
            AgentId::Qoder,
            &env_wrapped_brew,
            &runner_affirmative,
            &mut cache_wb,
        )
        .unwrap();
        assert!(
            matches!(
                qoder_wb.owner,
                UpdateOwner::ManualActionRequired { ref reason, .. } if reason.contains("Homebrew")
            ),
            "cross-prefix Homebrew wrapper must require manual action: {:?}",
            qoder_wb.owner
        );

        let unverified_wrapper_bin = tmp.path().join("unverified-wrapper").join("bin");
        let mystery_target = tmp.path().join("mystery- target").join("qodercli");
        create_executable(&mystery_target, "#!/bin/sh\n");
        create_executable(
            &unverified_wrapper_bin.join("qodercli"),
            &format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", mystery_target.display()),
        );
        let env_mystery = test_env(&[&unverified_wrapper_bin], &home);
        let mut cache_mystery = PackageManagerCache::new();
        let qoder_mystery = discover_agent(
            AgentId::Qoder,
            &env_mystery,
            &runner_affirmative,
            &mut cache_mystery,
        )
        .unwrap();
        assert!(
            matches!(
                qoder_mystery.owner,
                UpdateOwner::ManualActionRequired { .. }
            ),
            "forwarding wrapper to unresolved external target must remain manual: {:?}",
            qoder_mystery.owner
        );

        // 10. Directory names (.qodercli, @qoder-ai/qodercli) and nearby unlinked package.json
        // cannot establish CLI identity for shared `qoder` name
        let dot_qodercli_root = tmp.path().join(".qodercli");
        let dot_qodercli_bin = dot_qodercli_root.join("bin");
        create_executable(&dot_qodercli_bin.join("qoder"), "#!/bin/sh\n");
        fs::write(
            dot_qodercli_root.join("package.json"),
            r#"{"name":"@qoder-ai/qodercli","version":"1.2.3","bin":{"qodercli":"bin/qodercli.js"}}"#,
        )
        .unwrap();
        let env_dir_hint = test_env(&[&dot_qodercli_bin], &home);
        let runner_index_tool = FakeProcessRunner::new(
            |req| {
                if req.args.iter().any(|a| a == "--help") {
                    Ok(ok_output(
                        "Usage: index-tool\nCommands:\n  update  Rebuild the document index",
                    ))
                } else {
                    Ok(ok_output("1.2.3"))
                }
            },
            |_| panic!("unrelated tool in .qodercli/bin must never be updated"),
        );
        let mut cache_dir_hint = PackageManagerCache::new();
        assert!(
            discover_agent(
                AgentId::Qoder,
                &env_dir_hint,
                &runner_index_tool,
                &mut cache_dir_hint
            )
            .is_none(),
            "shared `qoder` under .qodercli/bin with nearby package.json and index-tool help must be rejected"
        );

        // Unbound `qoder` inside node_modules/@qoder-ai/qodercli/bin/qoder when package.json `bin` only binds `qodercli` is rejected
        let unbound_npm_prefix = tmp.path().join("npm-unbound-qoder");
        let unbound_npm_bin = unbound_npm_prefix.join("bin");
        let unbound_pkg_dir = unbound_npm_prefix
            .join("lib")
            .join("node_modules")
            .join("@qoder-ai")
            .join("qodercli");
        fs::create_dir_all(unbound_pkg_dir.join("bin")).unwrap();
        fs::write(
            unbound_pkg_dir.join("package.json"),
            r#"{"name":"@qoder-ai/qodercli","version":"1.2.3","bin":{"qodercli":"bin/qodercli.js"}}"#,
        )
        .unwrap();
        create_executable(
            &unbound_npm_bin.join("qoder"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                unbound_pkg_dir.join("bin").join("qoder").display()
            ),
        );
        let env_unbound_npm = test_env(&[&unbound_npm_bin], &home);
        let mut cache_unbound = PackageManagerCache::new();
        assert!(
            discover_agent(
                AgentId::Qoder,
                &env_unbound_npm,
                &runner_index_tool,
                &mut cache_unbound
            )
            .is_none(),
            "shared `qoder` not matching package.json `bin` entry point must be rejected"
        );

        // Positive entry-point-bound `qoder` in package.json `bin` succeeds as NpmGlobal
        fs::write(
            unbound_pkg_dir.join("package.json"),
            r#"{"name":"@qoder-ai/qodercli","version":"1.2.3","bin":{"qoder":"bin/qoder.js","qodercli":"bin/qodercli.js"}}"#,
        )
        .unwrap();
        create_executable(
            &unbound_npm_bin.join("qoder"),
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                unbound_pkg_dir.join("bin").join("qoder.js").display()
            ),
        );
        create_executable(&unbound_npm_bin.join("npm"), "#!/bin/sh\n");
        let runner_bound_qoder =
            FakeProcessRunner::new(|_| Ok(ok_output("1.2.3")), |_| unreachable!());
        let mut cache_bound = PackageManagerCache::new();
        let qoder_bound = discover_agent(
            AgentId::Qoder,
            &env_unbound_npm,
            &runner_bound_qoder,
            &mut cache_bound,
        )
        .unwrap();
        assert_eq!(
            qoder_bound.owner,
            UpdateOwner::NpmGlobal {
                npm_program: unbound_npm_bin.join("npm"),
                prefix: unbound_npm_prefix,
                package: "@qoder-ai/qodercli".to_string(),
            }
        );

        // 11. Negated update help text and non-zero help probe exit must not authorize native update
        for (help_exit, help_text) in [
            (
                0,
                "Qoder CLI\nDo not run qodercli update; use your package manager.",
            ),
            (0, "Qoder CLI\nqodercli update is disabled by policy"),
            (
                0,
                "Qoder CLI\nCommands:\n  update  Update Qoder CLI\nNote: Do not run qodercli update; use your package manager.",
            ),
            (1, "Qoder CLI\nError: qodercli update is disabled by policy"),
            (1, "Qoder CLI\nCommands:\n  update  Update Qoder CLI"),
        ] {
            let runner_neg_help = FakeProcessRunner::new(
                move |req| {
                    if req.args.iter().any(|a| a == "--help") {
                        if help_exit == 0 {
                            Ok(ok_output(help_text))
                        } else {
                            Ok(ProcessOutput {
                                status_code: Some(help_exit),
                                success: false,
                                interrupted: false,
                                timed_out: false,
                                stdout: help_text.as_bytes().to_vec(),
                                stderr: Vec::new(),
                                stdout_truncated: false,
                                stderr_truncated: false,
                            })
                        }
                    } else {
                        Ok(ok_output("Qoder CLI 0.9.0"))
                    }
                },
                |_| unreachable!(),
            );
            let mut cache_nh = PackageManagerCache::new();
            let qoder_nh =
                discover_agent(AgentId::Qoder, &env_both, &runner_neg_help, &mut cache_nh).unwrap();
            assert!(
                matches!(qoder_nh.owner, UpdateOwner::ManualActionRequired { .. }),
                "expected ManualActionRequired for help_exit={help_exit} help_text={help_text:?}, got {:?}",
                qoder_nh.owner
            );
        }
    }
}
