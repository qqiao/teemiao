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

//! Cross-platform isolated test harness and fake CLI tool builder for `agents_cli` tests.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SANDBOX_SEQ: AtomicUsize = AtomicUsize::new(0);
static FAKE_TOOL_BIN: OnceLock<PathBuf> = OnceLock::new();

const FAKE_TOOL_RS_SOURCE: &str = r#"
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::Duration;

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().collect();
    let arg0 = raw_args.first().cloned().unwrap_or_default();
    let args = &raw_args[1..];

    if args.first().map(String::as_str) == Some("--internal-hold-pipes-ms") {
        if let Some(pid_file) = args.get(2) {
            if !pid_file.is_empty() {
                let _ = fs::write(pid_file, format!("{}", std::process::id()));
            }
        }
        let ms = args
            .get(1)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(7000);
        thread::sleep(Duration::from_millis(ms));
        return ExitCode::from(0);
    }

    let exe_path = env::current_exe().unwrap_or_else(|_| PathBuf::from(&arg0));
    let arg0_path = PathBuf::from(&arg0);

    let tool_name = arg0_path
        .file_stem()
        .or_else(|| exe_path.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_ascii_lowercase();

    if let Ok(log_file) = env::var("TEEMIAO_TEST_LOG_FILE") {
        let ci = env::var("CI").unwrap_or_default();
        let dis_auto = env::var("DISABLE_AUTOUPDATER").unwrap_or_default();
        let codex_ni = env::var("CODEX_NON_INTERACTIVE").unwrap_or_default();
        let codex_dir = env::var("CODEX_INSTALL_DIR").unwrap_or_default();
        let codex_home = env::var("CODEX_HOME").unwrap_or_default();
        let line = format!(
            "TOOL={tool_name}|ARGS={}|CI={ci}|DISABLE_AUTOUPDATER={dis_auto}|CODEX_NON_INTERACTIVE={codex_ni}|CODEX_INSTALL_DIR={codex_dir}|CODEX_HOME={codex_home}\n",
            args.join(" ")
        );
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    let cfg = load_cfg(&arg0_path, &exe_path, &tool_name);

    let is_version = args.iter().any(|a| a == "--version");
    let is_help = args.iter().any(|a| a == "--help");
    let is_list = args.iter().any(|a| a == "list");
    let is_prefix = args.iter().any(|a| a == "--prefix") && !args.iter().any(|a| a == "install");

    if is_version {
        if let Some(p_file) = cfg.get("parent_pid_file") {
            if !p_file.is_empty() {
                let _ = fs::write(p_file, format!("{}", std::process::id()));
            }
        }
        if let Some(hold_ms) = cfg.get("spawn_hold_pipes_ms") {
            let pid_file = cfg.get("hold_pid_file").map(String::as_str).unwrap_or("");
            let _ = Command::new(&exe_path)
                .arg("--internal-hold-pipes-ms")
                .arg(hold_ms)
                .arg(pid_file)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn();
        }
        if let Some(ms) = cfg.get("version_sleep_ms").and_then(|v| v.parse::<u64>().ok()) {
            thread::sleep(Duration::from_millis(ms));
        }
        if let Some(spam) = cfg.get("version_spam_bytes").and_then(|v| v.parse::<usize>().ok()) {
            let buf = vec![b'V'; spam];
            let _ = std::io::stdout().write_all(&buf);
            let _ = std::io::stdout().flush();
        }
        if let Some(out) = cfg.get("version_stdout") {
            println!("{out}");
        }
        if let Some(err) = cfg.get("version_stderr") {
            eprintln!("{err}");
        }
        let code = cfg
            .get("version_exit")
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(0);
        return ExitCode::from(code);
    }

    if is_help {
        if let Some(out) = cfg.get("help_stdout") {
            println!("{out}");
        }
        let code = cfg
            .get("help_exit")
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(0);
        return ExitCode::from(code);
    }

    if is_list {
        if let Some(out) = cfg.get("list_stdout") {
            for item in out.split(',') {
                println!("{item}");
            }
        }
        let code = cfg
            .get("list_exit")
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(0);
        return ExitCode::from(code);
    }

    if is_prefix {
        if let Some(out) = cfg.get("prefix_stdout") {
            println!("{out}");
        }
        return ExitCode::from(0);
    }

    // Handle curl `-o <dest>` download simulation
    if tool_name == "curl" {
        if let Some(pos) = args.iter().position(|a| a == "-o") {
            if let Some(dest) = args.get(pos + 1) {
                if let Some(src) = cfg.get("curl_payload_file") {
                    if let Ok(bytes) = fs::read(src) {
                        let _ = fs::write(dest, bytes);
                    }
                }
            }
        }
    }

    if let Some(del_path) = cfg.get("delete_on_update") {
        if fs::remove_file(del_path).is_err() {
            let _ = fs::rename(del_path, format!("{del_path}.removed"));
        }
    }

    if let Some(next_ver) = cfg.get("next_version_stdout") {
        let target_cfg = cfg
            .get("target_cfg_path")
            .map(PathBuf::from)
            .or_else(|| find_cfg_path(&arg0_path, &exe_path, &tool_name));
        if let Some(cfg_path) = target_cfg {
            update_cfg_version(&cfg_path, next_ver);
        }
    }

    if let Some(out) = cfg.get("update_stdout") {
        println!("{out}");
    }
    if let Some(err) = cfg.get("update_stderr") {
        eprintln!("{err}");
    }
    let code = cfg
        .get("update_exit")
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0);
    ExitCode::from(code)
}

fn find_cfg_path(arg0_path: &Path, exe_path: &Path, tool_name: &str) -> Option<PathBuf> {
    let cfg_name = format!(".fake_{tool_name}.cfg");
    let mut candidates = vec![arg0_path.to_path_buf(), exe_path.to_path_buf()];
    if let Ok(canon_arg0) = fs::canonicalize(arg0_path) {
        candidates.push(canon_arg0);
    }
    if let Ok(canon_exe) = fs::canonicalize(exe_path) {
        candidates.push(canon_exe);
    }
    for candidate in candidates {
        if let Some(parent) = candidate.parent() {
            let p = parent.join(&cfg_name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn load_cfg(arg0_path: &Path, exe_path: &Path, tool_name: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(path) = find_cfg_path(arg0_path, exe_path, tool_name) else {
        return map;
    };
    let Ok(text) = fs::read_to_string(path) else {
        return map;
    };
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

fn update_cfg_version(cfg_path: &Path, next_ver: &str) {
    let Ok(text) = fs::read_to_string(cfg_path) else {
        return;
    };
    let mut lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in text.lines() {
        if line.starts_with("version_stdout=") {
            lines.push(format!("version_stdout={next_ver}"));
            found = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        lines.push(format!("version_stdout={next_ver}"));
    }
    let _ = fs::write(cfg_path, lines.join("\n") + "\n");
}
"#;

fn compiled_fake_tool_binary() -> &'static Path {
    FAKE_TOOL_BIN.get_or_init(|| {
        let pid = std::process::id();
        let out_dir = std::env::temp_dir().join(format!("teemiao-fake-tool-build-v2-{pid}"));
        fs::create_dir_all(&out_dir).expect("create fake tool build dir");
        let src_path = out_dir.join("fake_tool.rs");
        let bin_name = if cfg!(windows) {
            "fake_tool.exe"
        } else {
            "fake_tool"
        };
        let bin_path = out_dir.join(bin_name);
        fs::write(&src_path, FAKE_TOOL_RS_SOURCE).expect("write fake_tool.rs");

        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
        let status = Command::new(rustc)
            .arg(&src_path)
            .arg("-O")
            .arg("-o")
            .arg(&bin_path)
            .status()
            .expect("invoke rustc to compile fake_tool");
        assert!(status.success(), "rustc failed to compile fake_tool.rs");
        bin_path
    })
}

/// Isolated test environment with its own temporary directory, home, bin dirs, and invocation log.
pub struct TestSandbox {
    pub root: PathBuf,
    pub home: PathBuf,
    pub bin_dir: PathBuf,
    pub log_file: PathBuf,
}

impl TestSandbox {
    pub fn new() -> Self {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = SANDBOX_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("teemiao-cli-test-{pid}-{nanos}-{seq}"));
        let home = root.join("home");
        let bin_dir = if cfg!(windows) {
            home.join("AppData")
                .join("Local")
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
        } else {
            home.join(".local").join("bin")
        };
        let log_file = root.join("invocations.log");
        fs::create_dir_all(&bin_dir).expect("create sandbox bin_dir");
        Self {
            root,
            home,
            bin_dir,
            log_file,
        }
    }

    /// Installs a compiled fake executable in `dir` named `name` (with `.exe` on Windows)
    /// and writes its `.fake_<name>.cfg` configuration file.
    pub fn install_fake_tool_in(
        &self,
        dir: &Path,
        name: &str,
        config: &[(&str, &str)],
    ) -> (PathBuf, PathBuf) {
        fs::create_dir_all(dir).expect("create tool dir");
        let file_name = if cfg!(windows) && !name.ends_with(".exe") && !name.ends_with(".cmd") {
            format!("{name}.exe")
        } else {
            name.to_string()
        };
        let dst = dir.join(&file_name);
        fs::copy(compiled_fake_tool_binary(), &dst).expect("copy compiled fake_tool");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dst).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dst, perms).unwrap();
        }
        let stem = Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(name)
            .to_ascii_lowercase();
        if cfg!(windows) && dir == self.bin_dir && stem == "claude" {
            let _ = fs::write(dir.join(".claude-native"), "native");
        }
        let cfg_path = dir.join(format!(".fake_{stem}.cfg"));
        let mut map = BTreeMap::new();
        for &(k, v) in config {
            map.insert(k, v);
        }
        let mut content = String::new();
        for (k, v) in map {
            content.push_str(&format!("{k}={v}\n"));
        }
        fs::write(&cfg_path, content).expect("write fake tool config");
        (dst, cfg_path)
    }

    pub fn install_fake_tool(&self, name: &str, config: &[(&str, &str)]) -> (PathBuf, PathBuf) {
        self.install_fake_tool_in(&self.bin_dir, name, config)
    }

    /// Runs the compiled `teemiao` binary with `args`, isolated `PATH`, and isolated `HOME`.
    pub fn run_teemiao(
        &self,
        args: &[&str],
        path_dirs: &[&Path],
        extra_env: &[(&str, &str)],
    ) -> Output {
        let teemiao_bin = env!("CARGO_BIN_EXE_teemiao");
        let joined_path = std::env::join_paths(path_dirs).expect("join PATH");
        let mut cmd = Command::new(teemiao_bin);
        cmd.args(args)
            .env_clear()
            .env("PATH", joined_path)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("LOCALAPPDATA", self.home.join("AppData").join("Local"))
            .env("TEEMIAO_TEST_LOG_FILE", &self.log_file)
            .env("LC_ALL", "en_US.UTF-8")
            .env("LANG", "en_US.UTF-8");

        if cfg!(windows) {
            if let Some(sys_root) = std::env::var_os("SystemRoot") {
                cmd.env("SystemRoot", sys_root);
            }
            cmd.env("PATHEXT", ".COM;.EXE;.BAT;.CMD;.PS1");
        }

        for &(k, v) in extra_env {
            cmd.env(k, v);
        }

        cmd.output().expect("spawn teemiao binary")
    }

    /// Reads all recorded fake tool invocations from `invocations.log`.
    pub fn invocations(&self) -> Vec<String> {
        let Ok(content) = fs::read_to_string(&self.log_file) else {
            return Vec::new();
        };
        content.lines().map(ToOwned::to_owned).collect()
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
