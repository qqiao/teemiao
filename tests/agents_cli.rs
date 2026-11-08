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

//! End-to-end process integration tests for `teemiao agents list` and `teemiao agents update`.

mod support;

use std::fs;
use std::path::Path;
use support::TestSandbox;

fn setup_codex_standalone(
    sb: &TestSandbox,
    version: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let release_dir = sb
        .home
        .join(".codex")
        .join("packages")
        .join("standalone")
        .join("releases")
        .join(version);
    fs::create_dir_all(&release_dir).unwrap();
    fs::write(
        release_dir.join("codex-package.json"),
        format!(r#"{{"version":"{version}","channel":"stable"}}"#),
    )
    .unwrap();
    #[cfg(unix)]
    {
        let release_bin_dir = release_dir.join("bin");
        let (target_bin, cfg_path) = sb.install_fake_tool_in(
            &release_bin_dir,
            "codex",
            &[("version_stdout", &format!("codex-cli {version}"))],
        );
        let launcher_path = sb.bin_dir.join("codex");
        let _ = fs::remove_file(&launcher_path);
        std::os::unix::fs::symlink(&target_bin, &launcher_path).unwrap();
        (launcher_path, cfg_path)
    }
    #[cfg(not(unix))]
    {
        fs::write(sb.bin_dir.join("rg.exe"), "MZ").unwrap();
        sb.install_fake_tool(
            "codex",
            &[("version_stdout", &format!("codex-cli {version}"))],
        )
    }
}

#[test]
fn agents_without_subcommand_displays_help_and_exits_2() {
    let sb = TestSandbox::new();
    let out = sb.run_teemiao(&["agents"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(2));
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("list"),
        "expected 'list' in help output: {combined}"
    );
    assert!(
        combined.contains("update"),
        "expected 'update' in help output: {combined}"
    );
    assert!(sb.invocations().is_empty());
}

#[test]
fn agents_update_unknown_identifier_rejected_by_clap_before_any_probe() {
    let sb = TestSandbox::new();
    sb.install_fake_tool("agy", &[("version_stdout", "Antigravity 1.0.0")]);
    let out = sb.run_teemiao(&["agents", "update", "unknown-agent"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown-agent") || stderr.contains("invalid value"),
        "expected invalid value message on stderr: {stderr}"
    );
    assert!(
        sb.invocations().is_empty(),
        "no tool should be invoked when clap rejects an unknown agent identifier"
    );
}

#[test]
fn agents_list_discovers_all_six_supported_agents_in_catalog_order() {
    let sb = TestSandbox::new();
    setup_codex_standalone(&sb, "0.118.0");
    sb.install_fake_tool("agy", &[("version_stdout", "Antigravity CLI 1.4.2")]);
    sb.install_fake_tool("opencode", &[("version_stdout", "opencode v0.6.1")]);
    sb.install_fake_tool("kimi", &[("version_stdout", "kimi, version 0.5.3")]);
    sb.install_fake_tool("agent", &[("version_stdout", "Cursor Agent CLI 0.48.0")]);
    sb.install_fake_tool("claude", &[("version_stdout", "2.1.4 (Claude Code)")]);

    let out = sb.run_teemiao(&["agents", "list"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);

    let codex_pos = stdout.find("Codex (codex)").expect("Codex in output");
    let agy_pos = stdout
        .find("Antigravity (antigravity)")
        .expect("Antigravity in output");
    let opencode_pos = stdout
        .find("OpenCode (opencode)")
        .expect("OpenCode in output");
    let kimi_pos = stdout.find("Kimi (kimi)").expect("Kimi in output");
    let cursor_pos = stdout.find("Cursor (cursor)").expect("Cursor in output");
    let claude_pos = stdout
        .find("Claude Code (claude-code)")
        .expect("Claude Code in output");

    assert!(codex_pos < agy_pos);
    assert!(agy_pos < opencode_pos);
    assert!(opencode_pos < kimi_pos);
    assert!(kimi_pos < cursor_pos);
    assert!(cursor_pos < claude_pos);

    assert!(stdout.contains("0.118.0"));
    assert!(stdout.contains("1.4.2"));
    assert!(stdout.contains("0.6.1"));
    assert!(stdout.contains("0.5.3"));
    assert!(stdout.contains("0.48.0"));
    assert!(stdout.contains("2.1.4"));
    assert!(stdout.contains("agy update"));
    assert!(stdout.contains("opencode upgrade"));
    assert!(stdout.contains("kimi upgrade --yes"));
    assert!(stdout.contains("agent update"));
    assert!(stdout.contains("claude update"));
}

#[test]
fn agents_list_empty_path_and_false_positive_rejection() {
    let sb = TestSandbox::new();
    // Desktop launchers (`cursor`, `antigravity`) and an unverified `agent` binary must not be reported.
    sb.install_fake_tool("cursor", &[("version_stdout", "Cursor Desktop 0.45.0")]);
    sb.install_fake_tool(
        "antigravity",
        &[("version_stdout", "Antigravity Desktop 2.0.0")],
    );
    sb.install_fake_tool(
        "agent",
        &[
            ("version_stdout", "Datadog Agent 7.50.0"),
            ("help_stdout", "Datadog monitoring agent"),
        ],
    );

    let out = sb.run_teemiao(&["agents", "list"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("No supported AI coding-agent CLIs were found on PATH."),
        "expected empty state message, got: {stdout}"
    );

    let invocations = sb.invocations();
    assert!(
        !invocations.iter().any(|l| l.starts_with("TOOL=cursor|")),
        "desktop 'cursor' binary must never be probed: {invocations:?}"
    );
    assert!(
        !invocations
            .iter()
            .any(|l| l.starts_with("TOOL=antigravity|")),
        "desktop 'antigravity' binary must never be probed: {invocations:?}"
    );
}

#[test]
fn agents_list_cursor_falls_back_to_cursor_agent_when_agent_is_unverified() {
    let sb = TestSandbox::new();
    sb.install_fake_tool(
        "agent",
        &[
            ("version_stdout", "Generic Agent 1.0.0"),
            ("help_stdout", "Unrelated tool"),
        ],
    );
    sb.install_fake_tool("cursor-agent", &[("version_stdout", "0.44.1")]);

    let out = sb.run_teemiao(&["agents", "list"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Cursor (cursor)"));
    assert!(stdout.contains("0.44.1"));
    assert!(stdout.contains("cursor-agent update"));
}

#[test]
fn agents_list_respects_path_precedence_and_handles_spaces_and_symlinks() {
    let sb = TestSandbox::new();
    let dir_first = sb.root.join("My Tools (Primary)").join("bin");
    let dir_second = sb.root.join("secondary_bin");
    fs::create_dir_all(&dir_first).unwrap();
    fs::create_dir_all(&dir_second).unwrap();

    #[cfg(unix)]
    {
        let real_dir = sb.root.join("real_agy_store");
        let (real_agy, _) =
            sb.install_fake_tool_in(&real_dir, "agy", &[("version_stdout", "Antigravity 1.2.3")]);
        std::os::unix::fs::symlink(&real_agy, dir_first.join("agy")).unwrap();
    }
    #[cfg(not(unix))]
    {
        sb.install_fake_tool_in(
            &dir_first,
            "agy",
            &[("version_stdout", "Antigravity 1.2.3")],
        );
    }

    sb.install_fake_tool_in(
        &dir_second,
        "agy",
        &[("version_stdout", "Antigravity 9.9.9")],
    );

    let out = sb.run_teemiao(&["agents", "list"], &[&dir_first, &dir_second], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("1.2.3"),
        "expected first PATH version: {stdout}"
    );
    assert!(
        !stdout.contains("9.9.9"),
        "shadowed PATH entry must not be reported: {stdout}"
    );
    assert!(
        stdout.contains("My Tools (Primary)"),
        "expected path with spaces and parentheses in output: {stdout}"
    );
}

#[test]
fn agents_list_bounded_probe_timeout_oversized_nonzero_and_probe_env_suppression() {
    let sb = TestSandbox::new();
    sb.install_fake_tool(
        "agy",
        &[
            ("version_sleep_ms", "6200"),
            ("version_stdout", "Antigravity 1.0.0"),
        ],
    );
    sb.install_fake_tool("opencode", &[("version_spam_bytes", "100000")]);
    sb.install_fake_tool("kimi", &[("version_exit", "2")]);
    sb.install_fake_tool("claude", &[("version_stdout", "2.1.0")]);

    let out = sb.run_teemiao(&["agents", "list"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(stdout.contains("Antigravity (antigravity) | version: unknown"));
    assert!(stdout.contains("OpenCode (opencode) | version: unknown"));
    assert!(stdout.contains("Kimi (kimi) | version: unknown"));
    assert!(stdout.contains("Claude Code (claude-code) | version: 2.1.0"));

    assert!(
        stderr.contains("timed out after 5s"),
        "expected timeout warning on stderr: {stderr}"
    );
    assert!(
        stderr.contains("exceeded 64 KiB limit"),
        "expected oversized warning on stderr: {stderr}"
    );
    assert!(
        stderr.contains("exit code 2"),
        "expected nonzero exit warning on stderr: {stderr}"
    );

    let invocations = sb.invocations();
    assert!(
        invocations
            .iter()
            .all(|line| line.contains("CI=1|DISABLE_AUTOUPDATER=1")),
        "every probe child must receive CI=1 and DISABLE_AUTOUPDATER=1: {invocations:?}"
    );
}

#[test]
fn agents_update_native_self_updaters_positive_and_up_to_date() {
    let sb = TestSandbox::new();
    sb.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("next_version_stdout", "Antigravity 1.1.0"),
            ("update_stdout", "Updated Antigravity to 1.1.0"),
        ],
    );
    sb.install_fake_tool(
        "opencode",
        &[
            ("version_stdout", "opencode 0.5.0"),
            ("update_stdout", "opencode is already up to date"),
        ],
    );
    sb.install_fake_tool(
        "kimi",
        &[
            ("version_stdout", "kimi 0.4.0"),
            ("next_version_stdout", "kimi 0.5.0"),
            ("update_stdout", "Upgraded kimi to 0.5.0"),
        ],
    );
    sb.install_fake_tool(
        "agent",
        &[
            ("version_stdout", "Cursor Agent CLI 0.45.0"),
            ("next_version_stdout", "Cursor Agent CLI 0.46.0"),
            ("update_stdout", "Updated Cursor CLI"),
        ],
    );

    // 1. Test `agy` alias
    let out_agy = sb.run_teemiao(&["agents", "update", "agy"], &[&sb.bin_dir], &[]);
    assert_eq!(out_agy.status.code(), Some(0));
    let stdout_agy = String::from_utf8_lossy(&out_agy.stdout);
    assert!(
        stdout_agy.contains("updated (1.0.0 -> 1.1.0;"),
        "expected agy version transition: {stdout_agy}"
    );

    // 2. Test `opencode` up-to-date
    let out_oc = sb.run_teemiao(&["agents", "update", "opencode"], &[&sb.bin_dir], &[]);
    assert_eq!(out_oc.status.code(), Some(0));
    let stdout_oc = String::from_utf8_lossy(&out_oc.stdout);
    assert!(
        stdout_oc.contains("up to date (version: 0.5.0;"),
        "expected opencode up-to-date status: {stdout_oc}"
    );

    // 3. Test `kimi` non-interactive `kimi upgrade --yes`
    let out_kimi = sb.run_teemiao(&["agents", "update", "kimi"], &[&sb.bin_dir], &[]);
    assert_eq!(out_kimi.status.code(), Some(0));
    let stdout_kimi = String::from_utf8_lossy(&out_kimi.stdout);
    assert!(
        stdout_kimi.contains("updated (0.4.0 -> 0.5.0;"),
        "expected kimi version transition: {stdout_kimi}"
    );

    // 4. Test `cursor` via verified `agent update`
    let out_cursor = sb.run_teemiao(&["agents", "update", "cursor"], &[&sb.bin_dir], &[]);
    assert_eq!(out_cursor.status.code(), Some(0));
    let stdout_cursor = String::from_utf8_lossy(&out_cursor.stdout);
    assert!(
        stdout_cursor.contains("updated (0.45.0 -> 0.46.0;"),
        "expected cursor version transition: {stdout_cursor}"
    );

    let invocations = sb.invocations();
    assert!(
        invocations
            .iter()
            .any(|l| l.starts_with("TOOL=agy|ARGS=update|"))
    );
    assert!(
        invocations
            .iter()
            .any(|l| l.starts_with("TOOL=opencode|ARGS=upgrade|"))
    );
    assert!(
        invocations
            .iter()
            .any(|l| l.starts_with("TOOL=kimi|ARGS=upgrade --yes|"))
    );
    assert!(
        invocations
            .iter()
            .any(|l| l.starts_with("TOOL=agent|ARGS=update|"))
    );
}

#[test]
fn agents_update_native_negative_legacy_manual_unverified_and_missing_binary() {
    let sb = TestSandbox::new();

    // 1. Legacy Kimi installation blocked before running updater
    sb.install_fake_tool("kimi", &[("version_stdout", "kimi 0.1.0-legacy")]);
    let out_kimi = sb.run_teemiao(&["agents", "update", "kimi"], &[&sb.bin_dir], &[]);
    assert_eq!(out_kimi.status.code(), Some(1));
    let stdout_kimi = String::from_utf8_lossy(&out_kimi.stdout);
    assert!(stdout_kimi.contains("manual action required"));
    assert!(
        !sb.invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=kimi|ARGS=upgrade")),
        "legacy kimi must not invoke 'kimi upgrade --yes'"
    );

    // 2. Native updater exits 0 but prints manual-update guidance
    sb.install_fake_tool(
        "opencode",
        &[
            ("version_stdout", "opencode 0.5.0"),
            (
                "update_stdout",
                "Self-update is disabled. Please update manually via your package manager.",
            ),
        ],
    );
    let out_oc = sb.run_teemiao(&["agents", "update", "opencode"], &[&sb.bin_dir], &[]);
    assert_eq!(out_oc.status.code(), Some(1));
    let stdout_oc = String::from_utf8_lossy(&out_oc.stdout);
    assert!(
        stdout_oc.contains("manual action required"),
        "zero-exit manual instruction must be reported as manual action required: {stdout_oc}"
    );

    // 3. Native updater exits 0 with unrecognized output and unchanged version -> unverified
    sb.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("update_stdout", "completed check"),
        ],
    );
    let out_agy = sb.run_teemiao(&["agents", "update", "antigravity"], &[&sb.bin_dir], &[]);
    assert_eq!(out_agy.status.code(), Some(1));
    let stdout_agy = String::from_utf8_lossy(&out_agy.stdout);
    assert!(
        stdout_agy.contains("unverified"),
        "unchanged version without up-to-date confirmation must be unverified: {stdout_agy}"
    );

    // 4. Native updater deletes binary during update -> failed
    let (claude_bin, _) = sb.install_fake_tool("claude", &[("version_stdout", "2.0.0")]);
    sb.install_fake_tool(
        "claude",
        &[
            ("version_stdout", "2.0.0"),
            ("delete_on_update", &claude_bin.to_string_lossy()),
            ("update_stdout", "Updated"),
        ],
    );
    let out_claude = sb.run_teemiao(&["agents", "update", "claude-code"], &[&sb.bin_dir], &[]);
    assert_eq!(out_claude.status.code(), Some(1));
    let stdout_claude = String::from_utf8_lossy(&out_claude.stdout);
    assert!(
        stdout_claude.contains("failed"),
        "disappearing binary after update must be reported as failed: {stdout_claude}"
    );
}

#[test]
fn agents_update_codex_npm_global_positive() {
    let sb = TestSandbox::new();
    let npm_prefix = sb.root.join("npm-global");
    let npm_bin_dir = npm_prefix.join("bin");
    let pkg_dir = npm_prefix
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

    let (pkg_codex, codex_cfg) = sb.install_fake_tool_in(
        &pkg_dir.join("bin"),
        "codex",
        &[("version_stdout", "codex-cli 0.118.0")],
    );
    fs::create_dir_all(&npm_bin_dir).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&pkg_codex, npm_bin_dir.join("codex")).unwrap();
    #[cfg(not(unix))]
    {
        fs::write(
            npm_bin_dir.join("codex.cmd"),
            format!(
                "@echo off\r\nREM @openai/codex node_modules\r\n\"{}\" %*\r\n",
                pkg_codex.display()
            ),
        )
        .unwrap();
    }
    sb.install_fake_tool_in(
        &npm_bin_dir,
        "npm",
        &[
            ("next_version_stdout", "codex-cli 0.119.0"),
            ("target_cfg_path", &codex_cfg.to_string_lossy()),
            ("update_stdout", "added 1 package"),
        ],
    );

    let out = sb.run_teemiao(&["agents", "update", "codex"], &[&npm_bin_dir], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("updated (0.118.0 -> 0.119.0;"),
        "expected npm codex update: {stdout}"
    );

    let invocations = sb.invocations();
    let expected_npm_args = format!(
        "install -g --prefix {} @openai/codex@latest",
        npm_prefix.display()
    );
    assert!(
        invocations
            .iter()
            .any(|l| l.starts_with(&format!("TOOL=npm|ARGS={expected_npm_args}|"))),
        "expected npm install command in invocations: {invocations:?}"
    );
}

#[test]
fn agents_update_codex_homebrew_cask_positive() {
    let sb = TestSandbox::new();
    let brew_prefix = sb.root.join("homebrew");
    let brew_bin_dir = brew_prefix.join("bin");
    let cask_dir = brew_prefix.join("Caskroom").join("codex").join("0.118.0");
    fs::create_dir_all(&cask_dir).unwrap();
    fs::create_dir_all(&brew_bin_dir).unwrap();

    let (cask_codex, codex_cfg) = sb.install_fake_tool_in(
        &cask_dir,
        "codex",
        &[("version_stdout", "codex-cli 0.118.0")],
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(&cask_codex, brew_bin_dir.join("codex")).unwrap();
    #[cfg(not(unix))]
    {
        fs::write(
            brew_bin_dir.join("codex.cmd"),
            format!(
                "@echo off\r\nREM Caskroom\\codex\r\n\"{}\" %*\r\n",
                cask_codex.display()
            ),
        )
        .unwrap();
    }
    sb.install_fake_tool_in(
        &brew_bin_dir,
        "brew",
        &[
            ("prefix_stdout", &brew_prefix.to_string_lossy()),
            ("list_stdout", "codex"),
            ("next_version_stdout", "codex-cli 0.119.0"),
            ("target_cfg_path", &codex_cfg.to_string_lossy()),
            ("update_stdout", "==> Upgrading 1 outdated package: codex"),
        ],
    );

    let out = sb.run_teemiao(&["agents", "update", "codex"], &[&brew_bin_dir], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("updated (0.118.0 -> 0.119.0;"),
        "expected brew codex update: {stdout}"
    );
    assert!(
        sb.invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=brew|ARGS=upgrade --cask codex|")),
        "expected 'brew upgrade --cask codex' in invocations: {:?}",
        sb.invocations()
    );
}

#[test]
fn agents_update_codex_standalone_positive_and_negative_validation() {
    let sb = TestSandbox::new();
    let (_, codex_cfg) = setup_codex_standalone(&sb, "0.118.0");

    // 1. Positive standalone update with valid installer script payload
    let valid_payload = sb.root.join("valid_install_payload.txt");
    if cfg!(windows) {
        fs::write(
            &valid_payload,
            "# Codex CLI standalone installer\n$ErrorActionPreference = 'Stop'\n# CODEX_INSTALL_DIR CODEX_HOME\nWrite-Host 'Codex CLI installed successfully'\n",
        )
        .unwrap();
    } else {
        fs::write(
            &valid_payload,
            "#!/bin/sh\n# Codex CLI standalone installer\n# CODEX_INSTALL_DIR CODEX_HOME\necho 'Codex CLI installed successfully'\nexit 0\n",
        )
        .unwrap();
    }

    sb.install_fake_tool(
        "curl",
        &[("curl_payload_file", &valid_payload.to_string_lossy())],
    );
    let shell_tool = if cfg!(windows) { "powershell" } else { "sh" };
    sb.install_fake_tool(
        shell_tool,
        &[
            ("next_version_stdout", "codex-cli 0.119.0"),
            ("target_cfg_path", &codex_cfg.to_string_lossy()),
            ("update_stdout", "Installed Codex 0.119.0"),
        ],
    );

    let out = sb.run_teemiao(&["agents", "update", "codex"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("updated (0.118.0 -> 0.119.0;"),
        "expected standalone codex update: {stdout}"
    );

    let invocations = sb.invocations();
    assert!(
        invocations.iter().any(|l| {
            l.starts_with("TOOL=curl|")
                && l.contains("--proto =https --tlsv1.2 -o")
                && l.contains("https://chatgpt.com/codex/install.")
        }),
        "expected curl download from official HTTPS URL: {invocations:?}"
    );
    assert!(
        invocations.iter().any(|l| {
            l.starts_with(&format!("TOOL={shell_tool}|"))
                && l.contains("CODEX_NON_INTERACTIVE=1")
                && l.contains(&format!("CODEX_INSTALL_DIR={}", sb.bin_dir.display()))
        }),
        "expected shell execution with CODEX_NON_INTERACTIVE=1 and CODEX_INSTALL_DIR: {invocations:?}"
    );

    // 2. Negative standalone test: invalid/partial payload rejected before shell execution
    let sb_bad = TestSandbox::new();
    setup_codex_standalone(&sb_bad, "0.118.0");
    let invalid_payload = sb_bad.root.join("invalid_payload.txt");
    fs::write(&invalid_payload, "<html>502 Bad Gateway</html>").unwrap();
    sb_bad.install_fake_tool(
        "curl",
        &[("curl_payload_file", &invalid_payload.to_string_lossy())],
    );
    sb_bad.install_fake_tool(shell_tool, &[("update_stdout", "should not run")]);

    let out_bad = sb_bad.run_teemiao(&["agents", "update", "codex"], &[&sb_bad.bin_dir], &[]);
    assert_eq!(out_bad.status.code(), Some(1));
    let stdout_bad = String::from_utf8_lossy(&out_bad.stdout);
    assert!(stdout_bad.contains("failed"));
    assert!(
        !sb_bad
            .invocations()
            .iter()
            .any(|l| l.starts_with(&format!("TOOL={shell_tool}|"))),
        "shell interpreter must never be invoked when downloaded installer fails validation"
    );

    // 3. Negative standalone test: CODEX_INSTALL_DIR points to a different directory
    let sb_mismatch = TestSandbox::new();
    setup_codex_standalone(&sb_mismatch, "0.118.0");
    let other_dir = sb_mismatch.root.join("other_bin");
    fs::create_dir_all(&other_dir).unwrap();
    let out_mismatch = sb_mismatch.run_teemiao(
        &["agents", "update", "codex"],
        &[&sb_mismatch.bin_dir],
        &[("CODEX_INSTALL_DIR", &other_dir.to_string_lossy())],
    );
    assert_eq!(out_mismatch.status.code(), Some(1));
    let stdout_mismatch = String::from_utf8_lossy(&out_mismatch.stdout);
    assert!(stdout_mismatch.contains("manual action required"));
    assert!(
        !sb_mismatch
            .invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=curl|")),
        "curl must not run when CODEX_INSTALL_DIR mismatches active launcher dir"
    );
}

#[test]
fn agents_update_codex_unowned_binary_does_not_trigger_npm_or_brew() {
    let sb = TestSandbox::new();
    let custom_dir = sb.root.join("custom_bin");
    sb.install_fake_tool_in(
        &custom_dir,
        "codex",
        &[("version_stdout", "codex-cli 0.118.0")],
    );
    sb.install_fake_tool_in(&custom_dir, "npm", &[("update_stdout", "npm ran")]);
    sb.install_fake_tool_in(&custom_dir, "brew", &[("list_stdout", "codex")]);

    let out = sb.run_teemiao(&["agents", "update", "codex"], &[&custom_dir], &[]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("manual action required"));
    assert!(
        !sb.invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=npm|") || l.starts_with("TOOL=brew|")),
        "unowned codex binary must not trigger npm or brew: {:?}",
        sb.invocations()
    );
}

#[test]
fn agents_update_claude_code_managed_and_negative_ownership_boundaries() {
    // 1. Homebrew channel cask `claude-code@latest` preserves channel token and never runs `claude update`
    let sb_brew = TestSandbox::new();
    let brew_prefix = sb_brew.root.join("homebrew");
    let brew_bin_dir = brew_prefix.join("bin");
    let cask_dir = brew_prefix
        .join("Caskroom")
        .join("claude-code@latest")
        .join("2.1.0");
    fs::create_dir_all(&cask_dir).unwrap();
    fs::create_dir_all(&brew_bin_dir).unwrap();

    let (cask_claude, claude_cfg) = sb_brew.install_fake_tool_in(
        &cask_dir,
        "claude",
        &[("version_stdout", "2.1.0 (Claude Code)")],
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(&cask_claude, brew_bin_dir.join("claude")).unwrap();
    #[cfg(not(unix))]
    {
        fs::write(
            brew_bin_dir.join("claude.cmd"),
            format!(
                "@echo off\r\nREM Caskroom\\claude-code@latest\r\n\"{}\" %*\r\n",
                cask_claude.display()
            ),
        )
        .unwrap();
    }
    sb_brew.install_fake_tool_in(
        &brew_bin_dir,
        "brew",
        &[
            ("prefix_stdout", &brew_prefix.to_string_lossy()),
            ("list_stdout", "claude-code@latest"),
            ("next_version_stdout", "2.2.0 (Claude Code)"),
            ("target_cfg_path", &claude_cfg.to_string_lossy()),
            ("update_stdout", "Upgraded claude-code@latest"),
        ],
    );

    let out_brew = sb_brew.run_teemiao(&["agents", "update", "claude"], &[&brew_bin_dir], &[]);
    assert_eq!(out_brew.status.code(), Some(0));
    let stdout_brew = String::from_utf8_lossy(&out_brew.stdout);
    assert!(stdout_brew.contains("updated (2.1.0 -> 2.2.0;"));
    let brew_invocations = sb_brew.invocations();
    assert!(
        brew_invocations
            .iter()
            .any(|l| l.starts_with("TOOL=brew|ARGS=upgrade --cask claude-code@latest|")),
        "expected brew upgrade --cask claude-code@latest: {brew_invocations:?}"
    );
    assert!(
        !brew_invocations
            .iter()
            .any(|l| l.starts_with("TOOL=claude|ARGS=update|")),
        "managed Claude Code must never run 'claude update': {brew_invocations:?}"
    );

    // 2. WinGet-managed Claude Code runs `winget upgrade --exact --id Anthropic.ClaudeCode ...`
    let sb_winget = TestSandbox::new();
    let winget_dir = sb_winget
        .home
        .join("AppData")
        .join("Local")
        .join("Microsoft")
        .join("WinGet")
        .join("Packages")
        .join("Anthropic.ClaudeCode_Microsoft.Winget.Source_8wekyb3d8bbwe");
    let (_, wg_claude_cfg) = sb_winget.install_fake_tool_in(
        &winget_dir,
        "claude",
        &[("version_stdout", "2.1.0 (Claude Code)")],
    );
    sb_winget.install_fake_tool_in(
        &winget_dir,
        "winget",
        &[
            ("list_stdout", "Anthropic.ClaudeCode 2.1.0"),
            ("next_version_stdout", "2.2.0 (Claude Code)"),
            ("target_cfg_path", &wg_claude_cfg.to_string_lossy()),
            ("update_stdout", "Successfully installed"),
        ],
    );

    let out_winget =
        sb_winget.run_teemiao(&["agents", "update", "claude-code"], &[&winget_dir], &[]);
    assert_eq!(out_winget.status.code(), Some(0));
    let stdout_winget = String::from_utf8_lossy(&out_winget.stdout);
    assert!(stdout_winget.contains("updated (2.1.0 -> 2.2.0;"));
    assert!(
        sb_winget.invocations().iter().any(|l| {
            l.starts_with(
                "TOOL=winget|ARGS=upgrade --exact --id Anthropic.ClaudeCode --accept-source-agreements --accept-package-agreements --disable-interactivity|",
            )
        }),
        "expected winget upgrade invocation: {:?}",
        sb_winget.invocations()
    );

    // 3. Negative test: Homebrew-owned Claude Code when `brew` is missing -> manual action required (never `claude update`)
    let sb_missing_mgr = TestSandbox::new();
    let missing_brew_prefix = sb_missing_mgr.root.join("homebrew");
    let missing_brew_bin = missing_brew_prefix.join("bin");
    let missing_cask_dir = missing_brew_prefix
        .join("Caskroom")
        .join("claude-code")
        .join("2.1.0");
    fs::create_dir_all(&missing_cask_dir).unwrap();
    fs::create_dir_all(&missing_brew_bin).unwrap();
    let (missing_cask_claude, _) = sb_missing_mgr.install_fake_tool_in(
        &missing_cask_dir,
        "claude",
        &[("version_stdout", "2.1.0 (Claude Code)")],
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(&missing_cask_claude, missing_brew_bin.join("claude")).unwrap();
    #[cfg(not(unix))]
    {
        fs::write(
            missing_brew_bin.join("claude.cmd"),
            format!(
                "@echo off\r\nREM Caskroom\\claude-code\r\n\"{}\" %*\r\n",
                missing_cask_claude.display()
            ),
        )
        .unwrap();
    }

    let out_missing =
        sb_missing_mgr.run_teemiao(&["agents", "update", "claude"], &[&missing_brew_bin], &[]);
    assert_eq!(out_missing.status.code(), Some(1));
    let stdout_missing = String::from_utf8_lossy(&out_missing.stdout);
    assert!(stdout_missing.contains("manual action required"));
    assert!(
        !sb_missing_mgr
            .invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=claude|ARGS=update|")),
        "missing package manager must never fall back to 'claude update'"
    );

    // 4. Negative test: managed no-op success message cannot mask failure even if updater exits 0
    let sb_noop = TestSandbox::new();
    sb_noop.install_fake_tool(
        "claude",
        &[
            ("version_stdout", "2.1.0 (Claude Code)"),
            (
                "update_stdout",
                "Managed by package manager; skipping self-update (already up to date)",
            ),
        ],
    );
    let out_noop = sb_noop.run_teemiao(&["agents", "update", "claude"], &[&sb_noop.bin_dir], &[]);
    assert_eq!(out_noop.status.code(), Some(1));
    let stdout_noop = String::from_utf8_lossy(&out_noop.stdout);
    assert!(
        stdout_noop.contains("manual action required"),
        "managed no-op message must not report success: {stdout_noop}"
    );
}

#[test]
fn agents_update_dry_run_bulk_continuation_empty_and_cancellation() {
    // 1. Empty PATH bulk update exits 0
    let sb_empty = TestSandbox::new();
    let out_empty = sb_empty.run_teemiao(&["agents", "update"], &[&sb_empty.bin_dir], &[]);
    assert_eq!(out_empty.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out_empty.stdout)
            .contains("No supported AI coding-agent CLIs were found on PATH; nothing to update.")
    );

    // 2. Dry-run single and bulk update execute zero mutation commands
    let sb_dry = TestSandbox::new();
    sb_dry.install_fake_tool("agy", &[("version_stdout", "Antigravity 1.0.0")]);
    sb_dry.install_fake_tool("kimi", &[("version_stdout", "kimi 0.4.0")]);

    let out_dry_all =
        sb_dry.run_teemiao(&["agents", "update", "--dry-run"], &[&sb_dry.bin_dir], &[]);
    assert_eq!(out_dry_all.status.code(), Some(0));
    let stdout_dry = String::from_utf8_lossy(&out_dry_all.stdout);
    assert!(stdout_dry.contains("dry-run"));
    assert!(stdout_dry.contains("agy update"));
    assert!(stdout_dry.contains("kimi upgrade --yes"));
    assert!(
        sb_dry
            .invocations()
            .iter()
            .all(|l| l.contains("ARGS=--version|")),
        "--dry-run must only run version probes, never update commands: {:?}",
        sb_dry.invocations()
    );

    // 3. Bulk update continues past single-agent failure and exits 1 with complete summary
    let sb_bulk = TestSandbox::new();
    sb_bulk.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("next_version_stdout", "Antigravity 1.1.0"),
            ("update_stdout", "Updated"),
        ],
    );
    sb_bulk.install_fake_tool(
        "opencode",
        &[
            ("version_stdout", "opencode 0.5.0"),
            ("update_stderr", "network error"),
            ("update_exit", "1"),
        ],
    );
    sb_bulk.install_fake_tool(
        "kimi",
        &[
            ("version_stdout", "kimi 0.4.0"),
            ("next_version_stdout", "kimi 0.5.0"),
            ("update_stdout", "Updated"),
        ],
    );

    let out_bulk = sb_bulk.run_teemiao(&["agents", "update"], &[&sb_bulk.bin_dir], &[]);
    assert_eq!(out_bulk.status.code(), Some(1));
    let stdout_bulk = String::from_utf8_lossy(&out_bulk.stdout);
    assert!(stdout_bulk.contains("Antigravity (antigravity): updated (1.0.0 -> 1.1.0;"));
    assert!(stdout_bulk.contains("OpenCode (opencode): failed"));
    assert!(stdout_bulk.contains("Kimi (kimi): updated (0.4.0 -> 0.5.0;"));

    // 4. Bulk update stops immediately when an updater returns exit code 130 (SIGINT cancellation)
    let sb_int = TestSandbox::new();
    sb_int.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("update_exit", "130"),
        ],
    );
    sb_int.install_fake_tool(
        "kimi",
        &[
            ("version_stdout", "kimi 0.4.0"),
            ("next_version_stdout", "kimi 0.5.0"),
        ],
    );

    let out_int = sb_int.run_teemiao(&["agents", "update"], &[&sb_int.bin_dir], &[]);
    assert_eq!(out_int.status.code(), Some(130));
    assert!(
        !sb_int
            .invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=kimi|ARGS=upgrade")),
        "subsequent agent updates must not start after cancellation (exit 130)"
    );
}

#[test]
fn agents_localized_zh_cn_output() {
    let sb = TestSandbox::new();
    sb.install_fake_tool("agy", &[("version_stdout", "Antigravity 1.0.0")]);

    let out = sb.run_teemiao(
        &["agents", "list"],
        &[&sb.bin_dir],
        &[("LC_ALL", "zh_CN.UTF-8"), ("LANG", "zh_CN.UTF-8")],
    );
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("版本:") && stdout.contains("更新方式:"),
        "expected Simplified Chinese labels in output: {stdout}"
    );
}

#[test]
fn agents_list_handles_empty_and_nonexistent_path_entries() {
    let sb = TestSandbox::new();
    let missing_dir = sb.root.join("does_not_exist");
    sb.install_fake_tool("claude", &[("version_stdout", "2.1.0 (Claude Code)")]);

    let out = sb.run_teemiao(
        &["agents", "list"],
        &[Path::new(""), &missing_dir, &sb.bin_dir],
        &[],
    );
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Claude Code (claude-code)"));
    assert!(stdout.contains("2.1.0"));
}

#[test]
fn agents_cursor_rejects_cursor_prefixed_dir_conflicting_banner_and_loose_package_json() {
    // 1. `cursor-tools/bin/agent` printing `Datadog Agent 7.50.0` must be rejected
    let sb = TestSandbox::new();
    let cursor_tools_bin = sb.root.join("cursor-tools").join("bin");
    sb.install_fake_tool_in(
        &cursor_tools_bin,
        "agent",
        &[
            ("version_stdout", "Datadog Agent 7.50.0"),
            ("help_stdout", "Datadog monitoring agent"),
            ("update_stdout", "WRONG_AGENT_UPDATED"),
        ],
    );
    let out = sb.run_teemiao(&["agents", "update", "cursor"], &[&cursor_tools_bin], &[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not installed"),
        "expected not installed for non-Cursor agent in cursor-tools/bin: {stderr}"
    );
    assert!(
        !sb.invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=agent|ARGS=update|")),
        "must never run 'agent update' on non-Cursor binary: {:?}",
        sb.invocations()
    );

    // 2. Conflicting product banner (`Datadog Agent 7.50.0`) inside `.cursor/bin/agent` must still be rejected
    let sb_conflict = TestSandbox::new();
    let dot_cursor_bin = sb_conflict.home.join(".cursor").join("bin");
    sb_conflict.install_fake_tool_in(
        &dot_cursor_bin,
        "agent",
        &[
            ("version_stdout", "Datadog Agent 7.50.0"),
            ("help_stdout", "Datadog monitoring agent"),
            ("update_stdout", "WRONG_AGENT_UPDATED"),
        ],
    );
    let out_conflict =
        sb_conflict.run_teemiao(&["agents", "update", "cursor"], &[&dot_cursor_bin], &[]);
    assert_eq!(out_conflict.status.code(), Some(1));
    assert!(
        !sb_conflict
            .invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=agent|ARGS=update|")),
        "conflicting product banner in .cursor/bin must never be executed for update"
    );

    // 3. Adjacent `package.json` mentioning `"cursor"` only in description (not `"name"`) must be rejected
    let sb_pkg = TestSandbox::new();
    let pkg_bin = sb_pkg.root.join("custom-agent").join("bin");
    fs::create_dir_all(&pkg_bin).unwrap();
    fs::write(
        sb_pkg.root.join("custom-agent").join("package.json"),
        r#"{"name":"acme-agent","description":"moves terminal cursor","version":"1.2.0"}"#,
    )
    .unwrap();
    sb_pkg.install_fake_tool_in(
        &pkg_bin,
        "agent",
        &[
            ("version_stdout", "1.2.0"),
            ("help_stdout", "Usage: agent [options]"),
        ],
    );
    let out_pkg = sb_pkg.run_teemiao(&["agents", "list"], &[&pkg_bin], &[]);
    assert_eq!(out_pkg.status.code(), Some(0));
    let stdout_pkg = String::from_utf8_lossy(&out_pkg.stdout);
    assert!(
        stdout_pkg.contains("No supported AI coding-agent CLIs were found on PATH."),
        "incidental 'cursor' substring in package.json description must not authorize agent: {stdout_pkg}"
    );
}

#[test]
fn agents_homebrew_multi_prefix_selects_owning_brew_executable_and_rejects_mismatch() {
    let sb = TestSandbox::new();
    let other_brew_prefix = sb.root.join("other-brew");
    let other_brew_bin = other_brew_prefix.join("bin");
    let owner_brew_prefix = sb.root.join("owner-brew");
    let owner_brew_bin = owner_brew_prefix.join("bin");
    let cask_dir = owner_brew_prefix
        .join("Caskroom")
        .join("codex")
        .join("0.118.0");
    fs::create_dir_all(&cask_dir).unwrap();
    fs::create_dir_all(&owner_brew_bin).unwrap();

    let (cask_codex, codex_cfg) = sb.install_fake_tool_in(
        &cask_dir,
        "codex",
        &[("version_stdout", "codex-cli 0.118.0")],
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(&cask_codex, owner_brew_bin.join("codex")).unwrap();
    #[cfg(not(unix))]
    {
        fs::write(
            owner_brew_bin.join("codex.cmd"),
            format!(
                "@echo off\r\nREM Caskroom\\codex\r\n\"{}\" %*\r\n",
                cask_codex.display()
            ),
        )
        .unwrap();
    }

    // Install `brew` in `other_brew_bin` (which comes FIRST on PATH) and in `owner_brew_bin`
    sb.install_fake_tool_in(
        &other_brew_bin,
        "brew",
        &[
            ("prefix_stdout", &other_brew_prefix.to_string_lossy()),
            ("update_stderr", "wrong brew prefix used"),
            ("update_exit", "1"),
        ],
    );
    let (owner_brew_exe, _) = sb.install_fake_tool_in(
        &owner_brew_bin,
        "brew",
        &[
            ("prefix_stdout", &owner_brew_prefix.to_string_lossy()),
            ("list_stdout", "codex"),
            ("next_version_stdout", "codex-cli 0.119.0"),
            ("target_cfg_path", &codex_cfg.to_string_lossy()),
            ("update_stdout", "==> Upgrading 1 outdated package: codex"),
        ],
    );

    let out = sb.run_teemiao(
        &["agents", "update", "codex", "--dry-run"],
        &[&other_brew_bin, &owner_brew_bin],
        &[],
    );
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&owner_brew_exe.display().to_string()),
        "expected dry-run plan to select owner_brew_bin/brew ({}), got: {stdout}",
        owner_brew_exe.display()
    );
    assert!(
        !stdout.contains(&other_brew_bin.display().to_string()),
        "dry-run plan must not select other-brew/bin/brew: {stdout}"
    );
}

#[test]
fn agents_rejects_unrelated_launchers_with_nearby_npm_standalone_or_homebrew_metadata() {
    // 1. Unrelated regular executable in `prefix/bin/codex` with stale `prefix/lib/node_modules/@openai/codex/package.json`
    let sb_npm = TestSandbox::new();
    let npm_prefix = sb_npm.root.join("usr-local");
    let npm_bin = npm_prefix.join("bin");
    let pkg_dir = npm_prefix
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
    sb_npm.install_fake_tool_in(
        &npm_bin,
        "codex",
        &[("version_stdout", "codex-cli 0.118.0")],
    );
    sb_npm.install_fake_tool_in(&npm_bin, "npm", &[("update_stdout", "added 1 package")]);

    let out_npm = sb_npm.run_teemiao(&["agents", "update", "codex"], &[&npm_bin], &[]);
    assert_eq!(out_npm.status.code(), Some(1));
    let stdout_npm = String::from_utf8_lossy(&out_npm.stdout);
    assert!(
        stdout_npm.contains("manual action required"),
        "unrelated regular binary in prefix/bin must not be treated as npm-owned: {stdout_npm}"
    );
    assert!(
        !sb_npm
            .invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=npm|")),
        "npm must not be executed for unrelated regular binary in prefix/bin"
    );

    // 2. Unrelated `~/.local/bin/codex` (0.118.0) with stale `.codex/packages/standalone/releases/0.117.0/codex-package.json`
    let sb_st = TestSandbox::new();
    let stale_release = sb_st
        .home
        .join(".codex")
        .join("packages")
        .join("standalone")
        .join("releases")
        .join("0.117.0");
    fs::create_dir_all(&stale_release).unwrap();
    fs::write(
        stale_release.join("codex-package.json"),
        r#"{"version":"0.117.0","channel":"stable"}"#,
    )
    .unwrap();
    sb_st.install_fake_tool("codex", &[("version_stdout", "codex-cli 0.118.0")]);
    sb_st.install_fake_tool("curl", &[("update_stdout", "curl ran")]);

    let out_st = sb_st.run_teemiao(&["agents", "update", "codex"], &[&sb_st.bin_dir], &[]);
    assert_eq!(out_st.status.code(), Some(1));
    let stdout_st = String::from_utf8_lossy(&out_st.stdout);
    assert!(
        stdout_st.contains("manual action required"),
        "unrelated binary in ~/.local/bin with stale standalone metadata must report manual action required: {stdout_st}"
    );
    assert!(
        !sb_st
            .invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=curl|")),
        "standalone installer must not run for unrelated ~/.local/bin/codex"
    );

    // 3. Unrelated regular executable in `homebrew/bin/codex` with nearby `Caskroom/codex`
    let sb_brew = TestSandbox::new();
    let brew_prefix = sb_brew.root.join("homebrew");
    let brew_bin = brew_prefix.join("bin");
    let cask_dir = brew_prefix.join("Caskroom").join("codex").join("0.117.0");
    fs::create_dir_all(&cask_dir).unwrap();
    sb_brew.install_fake_tool_in(
        &brew_bin,
        "codex",
        &[("version_stdout", "codex-cli 0.118.0")],
    );
    sb_brew.install_fake_tool_in(
        &brew_bin,
        "brew",
        &[
            ("prefix_stdout", &brew_prefix.to_string_lossy()),
            ("list_stdout", "codex"),
        ],
    );

    let out_brew = sb_brew.run_teemiao(&["agents", "update", "codex"], &[&brew_bin], &[]);
    assert_eq!(out_brew.status.code(), Some(1));
    let stdout_brew = String::from_utf8_lossy(&out_brew.stdout);
    assert!(
        stdout_brew.contains("manual action required"),
        "unrelated regular binary in homebrew/bin must not be treated as Homebrew-owned: {stdout_brew}"
    );
}

#[test]
fn agents_probe_timeout_bounds_pipe_holding_descendant_processes() {
    let sb = TestSandbox::new();
    let pid_file = sb.root.join("grandchild.pid");
    sb.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("spawn_hold_pipes_ms", "8000"),
            ("hold_pid_file", &pid_file.to_string_lossy()),
        ],
    );

    let start = std::time::Instant::now();
    let out = sb.run_teemiao(&["agents", "list"], &[&sb.bin_dir], &[]);
    let elapsed = start.elapsed();

    assert_eq!(out.status.code(), Some(0));
    assert!(
        elapsed < std::time::Duration::from_millis(6500),
        "probe with pipe-holding child must be bounded by the 5s deadline, took {elapsed:?}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("Antigravity (antigravity) | version: unknown"),
        "timed-out pipe-holding probe must report version unknown: {stdout}"
    );
    assert!(
        stderr.contains("timed out after 5s"),
        "timed-out pipe-holding probe must emit timeout warning: {stderr}"
    );
}

#[test]
fn agents_update_rejects_negated_error_or_unrelated_up_to_date_messages() {
    let sb = TestSandbox::new();

    // 1. "Not up to date. Update check failed." must be unverified (exit 1)
    sb.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("update_stdout", "Not up to date. Update check failed."),
        ],
    );
    let out_neg = sb.run_teemiao(&["agents", "update", "agy"], &[&sb.bin_dir], &[]);
    assert_eq!(out_neg.status.code(), Some(1));
    let stdout_neg = String::from_utf8_lossy(&out_neg.stdout);
    assert!(
        stdout_neg.contains("unverified"),
        "negated 'Not up to date' output must be reported as unverified: {stdout_neg}"
    );

    // 2. Unrelated subject "Dependencies are up to date" must be unverified (exit 1)
    sb.install_fake_tool(
        "opencode",
        &[
            ("version_stdout", "opencode 0.5.0"),
            ("update_stdout", "Dependencies are up to date."),
        ],
    );
    let out_dep = sb.run_teemiao(&["agents", "update", "opencode"], &[&sb.bin_dir], &[]);
    assert_eq!(out_dep.status.code(), Some(1));
    let stdout_dep = String::from_utf8_lossy(&out_dep.stdout);
    assert!(
        stdout_dep.contains("unverified"),
        "'Dependencies are up to date' must be reported as unverified: {stdout_dep}"
    );
}
