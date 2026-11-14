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
    let target = if cfg!(windows) {
        "x86_64-pc-windows-msvc"
    } else {
        "aarch64-apple-darwin"
    };
    setup_codex_standalone_target(sb, version, target)
}

fn setup_codex_standalone_target(
    sb: &TestSandbox,
    version: &str,
    target: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let release_name = if target.is_empty() {
        version.to_string()
    } else {
        format!("{version}-{target}")
    };
    let standalone = sb.home.join(".codex/packages/standalone");
    let release_dir = if cfg!(unix) {
        standalone.join("releases").join(&release_name)
    } else {
        standalone.join("current")
    };
    fs::create_dir_all(&release_dir).unwrap();
    fs::write(
        release_dir.join("codex-package.json"),
        format!(r#"{{"layoutVersion":1,"version":"{version}","target":"{target}"}}"#),
    )
    .unwrap();
    fs::write(standalone.join("auto-update-version"), &release_name).unwrap();
    #[cfg(unix)]
    {
        let current = standalone.join("current");
        if fs::symlink_metadata(&current).is_ok() {
            fs::remove_file(&current).unwrap();
        }
        std::os::unix::fs::symlink(&release_dir, current).unwrap();
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

fn write_unix_installer_fixture(sb: &TestSandbox, payload: &std::path::Path, tool: &str) {
    if cfg!(unix) {
        let executable = sb
            .bin_dir
            .join(tool)
            .to_string_lossy()
            .replace('\'', "'\"'\"'");
        fs::write(payload, format!("#!/bin/sh\n# CODEX_HOME CODEX_INSTALL_DIR installed successfully\nexec '{executable}' \"$@\"\n")).unwrap();
    }
}

#[test]
fn codex_standalone_release_markers_preserve_update_channels() {
    for (version, marker, automatic) in [
        ("0.167.0", Some("0.167.0-aarch64-apple-darwin\n"), true),
        ("0.167.0", Some("0.167.0-x86_64-apple-darwin"), true),
        ("0.167.0", Some("0.167.0-aarch64-unknown-linux-musl"), true),
        ("0.167.0", Some("0.167.0-x86_64-unknown-linux-musl"), true),
        ("0.167.0", Some("0.167.0-aarch64-unknown-linux-gnu"), true),
        ("0.167.0", Some("0.167.0-x86_64-unknown-linux-gnu"), true),
        ("0.167.0", Some("0.167.0-x86_64-pc-windows-msvc"), true),
        ("0.167.0", Some("0.167.0-aarch64-pc-windows-msvc"), true),
        ("0.167.0", Some("0.167.0"), true),
        ("0.167.0", Some("0.166.0-aarch64-apple-darwin"), false),
        ("0.167.0", None, false),
        ("0.167.0", Some(""), false),
        ("0.167.0", Some("not-a-version"), false),
        ("0.167.0", Some("0.167.0-unknown-platform"), false),
        (
            "0.167.0",
            Some("0.167.0-beta.1-aarch64-apple-darwin"),
            false,
        ),
        (
            "0.167.0-preview.1",
            Some("0.167.0-aarch64-apple-darwin"),
            false,
        ),
    ] {
        let sb = TestSandbox::new();
        if automatic {
            let suffix = marker
                .unwrap()
                .trim()
                .strip_prefix(version)
                .unwrap()
                .trim_start_matches('-');
            setup_codex_standalone_target(&sb, version, suffix);
        } else {
            setup_codex_standalone(&sb, version);
        }
        let marker_path = sb
            .home
            .join(".codex/packages/standalone/auto-update-version");
        if let Some(marker) = marker {
            fs::write(marker_path, marker).unwrap();
        } else {
            fs::remove_file(marker_path).unwrap();
        }
        let out = sb.run_teemiao(
            &["agents", "update", "codex", "--dry-run"],
            &[&sb.bin_dir],
            &[],
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(if automatic { 0 } else { 1 }),
            "{version}, {marker:?}: {stdout}"
        );
        assert_eq!(
            stdout.contains("manual action required"),
            !automatic,
            "{marker:?}: {stdout}"
        );
        assert!(
            sb.invocations()
                .iter()
                .all(|line| line.starts_with("TOOL=codex|")),
            "dry-run must only probe Codex: {:?}",
            sb.invocations()
        );
    }
}

#[test]
fn codex_standalone_current_manifest_must_match_update_marker() {
    for (version, target, channel, automatic) in [
        ("0.167.0", "aarch64-apple-darwin", "stable", true),
        ("0.166.0", "aarch64-apple-darwin", "stable", false),
        ("0.167.0", "x86_64-apple-darwin", "stable", false),
        ("0.167.0", "aarch64-apple-darwin", "beta", false),
    ] {
        let sb = TestSandbox::new();
        setup_codex_standalone_target(&sb, "0.167.0", "aarch64-apple-darwin");
        let standalone = sb.home.join(".codex/packages/standalone");
        fs::write(
            standalone.join("auto-update-version"),
            "0.167.0-aarch64-apple-darwin",
        )
        .unwrap();
        fs::create_dir_all(standalone.join("current")).unwrap();
        fs::write(
            standalone.join("current/codex-package.json"),
            format!(r#"{{"version":"{version}","target":"{target}","channel":"{channel}"}}"#),
        )
        .unwrap();
        let out = sb.run_teemiao(
            &["agents", "update", "codex", "--dry-run"],
            &[&sb.bin_dir],
            &[],
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(if automatic { 0 } else { 1 }),
            "{version}, {target}, {channel}: {stdout}"
        );
        assert_eq!(
            stdout.contains("manual action required"),
            !automatic,
            "{stdout}"
        );
        assert!(
            sb.invocations()
                .iter()
                .all(|line| line.starts_with("TOOL=codex|"))
        );
    }
}

#[test]
#[cfg(unix)]
fn codex_standalone_repointed_current_requires_matching_marker() {
    let sb = TestSandbox::new();
    let (launcher, _) = setup_codex_standalone(&sb, "0.167.0");
    let original = fs::canonicalize(&launcher)
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let standalone = sb.home.join(".codex/packages/standalone");
    let selected = standalone.join("current");
    let local_release = standalone.join("releases/local-custom-aarch64-apple-darwin");
    sb.install_fake_tool_in(
        &local_release.join("bin"),
        "codex",
        &[("version_stdout", "codex-cli 0.167.0")],
    );
    fs::copy(
        original.join("codex-package.json"),
        local_release.join("codex-package.json"),
    )
    .unwrap();
    fs::remove_file(&launcher).unwrap();
    std::os::unix::fs::symlink(selected.join("bin/codex"), &launcher).unwrap();
    fs::remove_file(&selected).unwrap();
    for (release, expected_code) in [(&original, 0), (&local_release, 1)] {
        std::os::unix::fs::symlink(release, &selected).unwrap();
        let out = sb.run_teemiao(
            &["agents", "update", "codex", "--dry-run"],
            &[&sb.bin_dir],
            &[],
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(expected_code),
            "{release:?}: {stdout}"
        );
        assert_eq!(
            stdout.contains("manual action required"),
            expected_code == 1,
            "{stdout}"
        );
        fs::remove_file(&selected).unwrap();
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

    let ver_out = sb.run_teemiao(&["--version"], &[&sb.bin_dir], &[]);
    assert_eq!(ver_out.status.code(), Some(0));
    let ver_stdout = String::from_utf8_lossy(&ver_out.stdout);
    assert_eq!(ver_stdout.trim(), "teemiao 0.2.4");

    let update_help = sb.run_teemiao(&["agents", "update", "--help"], &[&sb.bin_dir], &[]);
    assert_eq!(update_help.status.code(), Some(0));
    let help_stdout = String::from_utf8_lossy(&update_help.stdout);
    assert!(
        help_stdout.contains("qoder")
            && help_stdout.contains("qodercli")
            && help_stdout.contains("qoder-cli"),
        "expected qoder/qodercli/qoder-cli in agents update --help: {help_stdout}"
    );
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
fn codex_cleanup_warning_exemption_does_not_apply_to_native_updaters() {
    let sb = TestSandbox::new();
    sb.install_fake_tool(
        "kimi",
        &[
            ("version_stdout", "kimi 1.0.0"),
            ("update_stdout", "kimi is already up to date"),
            (
                "update_stderr",
                "WARNING: Could not safely inspect old Codex releases; skipped remaining cleanup.",
            ),
        ],
    );
    let out = sb.run_teemiao(&["agents", "update", "kimi"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("unverified"));
}

#[test]
fn codex_standalone_cleanup_warnings_require_verified_success() {
    let success = "Codex CLI 0.167.0 installed successfully.";
    let cleanup =
        "WARNING: Could not safely inspect old Codex releases; skipped remaining cleanup.";
    let selection = "WARNING: Could not record installation selection; pruning will be skipped.";
    for (initial_known, output, diagnostics, exit, after, expected, code) in [
        (
            true,
            success,
            cleanup,
            "0",
            "codex-cli 0.167.0",
            "up to date",
            0,
        ),
        (
            true,
            success,
            selection,
            "0",
            "codex-cli 0.167.0",
            "up to date",
            0,
        ),
        (
            false,
            success,
            cleanup,
            "0",
            "codex-cli 0.167.0",
            "manual action required",
            1,
        ),
        (true, "", cleanup, "0", "codex-cli 0.167.0", "unverified", 1),
        (
            true,
            "Codex CLI 0.168.0 installed successfully.",
            cleanup,
            "0",
            "codex-cli 0.167.0",
            "unverified",
            1,
        ),
        (
            true,
            success,
            cleanup,
            "1",
            "codex-cli 0.167.0",
            "failed",
            1,
        ),
        (
            true,
            success,
            "WARNING: Could not safely inspect old Codex releases; skipped remaining cleanup.<NL>Error: update failed",
            "0",
            "codex-cli 0.167.0",
            "unverified",
            1,
        ),
        (
            false,
            success,
            "Error: update failed",
            "0",
            "codex-cli 0.167.0",
            "manual action required",
            1,
        ),
        (
            true,
            success,
            "WARNING: Could not install Codex.",
            "0",
            "codex-cli 0.167.0",
            "unverified",
            1,
        ),
        (
            true,
            "Codex CLI 0.167.0 not installed successfully.",
            cleanup,
            "0",
            "codex-cli 0.167.0",
            "unverified",
            1,
        ),
        (true, success, cleanup, "0", "unreadable", "unverified", 1),
    ] {
        let sb = TestSandbox::new();
        let (_, codex_cfg) = setup_codex_standalone(&sb, "0.167.0");
        if !initial_known {
            fs::write(&codex_cfg, "version_stdout=unreadable\n").unwrap();
        }
        let payload = sb.root.join("installer.txt");
        fs::write(&payload, if cfg!(windows) {
            "# CODEX_INSTALL_DIR CODEX_HOME\nWrite-Host 'Codex CLI installed successfully'\n"
        } else {
            "#!/bin/sh\n# CODEX_INSTALL_DIR CODEX_HOME\necho 'Codex CLI installed successfully'\n"
        }).unwrap();
        sb.install_fake_tool("curl", &[("curl_payload_file", &payload.to_string_lossy())]);
        let shell = if cfg!(windows) {
            "powershell"
        } else {
            "fixture-installer"
        };
        sb.install_fake_tool(
            shell,
            &[
                ("update_stdout", output),
                ("update_stderr", diagnostics),
                ("update_exit", exit),
                ("next_version_stdout", after),
                ("target_cfg_path", &codex_cfg.to_string_lossy()),
            ],
        );
        write_unix_installer_fixture(&sb, &payload, shell);
        let out = sb.run_teemiao(&["agents", "update", "codex"], &[&sb.bin_dir], &[]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.code(),
            Some(code),
            "{output}, {diagnostics}, {after}: {stdout}\n{stderr}"
        );
        assert!(stdout.contains(expected), "expected {expected}: {stdout}");
        if initial_known {
            assert!(
                stderr.contains(&diagnostics.replace("<NL>", "\n")),
                "diagnostics must be retained: {stderr}"
            );
        } else {
            assert!(
                sb.invocations()
                    .iter()
                    .all(|line| line.starts_with("TOOL=codex|")),
                "an unknown active version must prevent installer execution"
            );
        }
        assert!(
            !stdout.contains("WARNING:"),
            "warnings belong on stderr: {stdout}"
        );
    }
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
    let shell_tool = if cfg!(windows) {
        "powershell"
    } else {
        "fixture-installer"
    };
    sb.install_fake_tool(
        shell_tool,
        &[
            ("next_version_stdout", "codex-cli 0.119.0"),
            ("target_cfg_path", &codex_cfg.to_string_lossy()),
            ("update_stdout", "Installed Codex 0.119.0"),
        ],
    );

    write_unix_installer_fixture(&sb, &valid_payload, shell_tool);
    #[cfg(unix)]
    sb.install_fake_tool(
        "sh",
        &[
            ("update_exit", "1"),
            ("update_stdout", "PATH sh must not run"),
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
                && l.contains("ARGS=-q ")
                && l.contains("--proto =https --proto-redir =https --tlsv1.2")
                && l.contains("--max-filesize 1048576")
                && l.contains("--connect-timeout 15 --max-time 120")
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
            ("help_stdout", "Usage: agent [--hide-cursor]"),
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

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        for _ in 0..10 {
            let status = std::process::Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if !status.is_ok_and(|s| s.success()) {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
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
            fn WaitForSingleObject(h_handle: *mut std::ffi::c_void, dw_milliseconds: u32) -> u32;
            fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
        }
        // SAFETY: Standard Win32 kernel32 process query using a valid process ID and closing the handle immediately.
        unsafe {
            let handle = OpenProcess(SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                return false;
            }
            let wait_rc = WaitForSingleObject(handle, 0);
            let _ = CloseHandle(handle);
            wait_rc == WAIT_TIMEOUT
        }
    }
}

fn kill_process_best_effort(pid: u32) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

struct PidFileCleanupGuard(std::path::PathBuf);

impl Drop for PidFileCleanupGuard {
    fn drop(&mut self) {
        if let Ok(text) = fs::read_to_string(&self.0)
            && let Some(line) = text.trim().lines().next()
            && let Ok(pid) = line.trim().parse::<u32>()
        {
            kill_process_best_effort(pid);
        }
    }
}

#[test]
fn agents_probe_timeout_bounds_pipe_holding_descendant_processes() {
    // Case 1: Parent exits immediately while its spawned descendant keeps stdout/stderr pipes open
    let sb = TestSandbox::new();
    let pid_file = sb.root.join("grandchild-immediate.pid");
    let _cleanup_immediate = PidFileCleanupGuard(pid_file.clone());
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
        "probe with pipe-holding child (parent exited immediately) must be bounded by the 5s deadline, took {elapsed:?}"
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

    let grandchild_pid: u32 = fs::read_to_string(&pid_file)
        .expect("grandchild pid file must be written")
        .trim()
        .parse()
        .expect("valid grandchild pid");
    let alive_after_immediate = is_process_alive(grandchild_pid);
    if alive_after_immediate {
        kill_process_best_effort(grandchild_pid);
    }
    assert!(
        !alive_after_immediate,
        "descendant PID {grandchild_pid} (parent exited immediately) survived production timeout cleanup"
    );
}

#[test]
fn agents_probe_timeout_terminates_running_parent_and_descendant() {
    // Parent is still running at the 5s deadline, intermediate bridge child exited early after
    // spawning a pipe-holding grandchild, and an unrelated sentinel process runs concurrently.
    let sb_running = TestSandbox::new();
    let sentinel_pid_file = sb_running.root.join("unrelated-sentinel.pid");
    let parent_pid_file = sb_running.root.join("parent-running.pid");
    let bridge_pid_file = sb_running.root.join("bridge-exited.pid");
    let pid_file_running = sb_running.root.join("grandchild-running.pid");
    let _cleanup_sentinel = PidFileCleanupGuard(sentinel_pid_file.clone());
    let _cleanup_parent = PidFileCleanupGuard(parent_pid_file.clone());
    let _cleanup_bridge = PidFileCleanupGuard(bridge_pid_file.clone());
    let _cleanup_running = PidFileCleanupGuard(pid_file_running.clone());

    let mut sentinel_child = std::process::Command::new(support::compiled_fake_tool_binary())
        .arg("--internal-hold-pipes-ms")
        .arg("15000")
        .arg(&sentinel_pid_file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn unrelated sentinel process");
    let sentinel_pid = sentinel_child.id();

    sb_running.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("parent_pid_file", &parent_pid_file.to_string_lossy()),
            ("bridge_pid_file", &bridge_pid_file.to_string_lossy()),
            ("version_sleep_ms", "8000"),
            ("spawn_bridge_hold_pipes_ms", "8000"),
            ("hold_pid_file", &pid_file_running.to_string_lossy()),
        ],
    );

    let start_running = std::time::Instant::now();
    let out_running = sb_running.run_teemiao(&["agents", "list"], &[&sb_running.bin_dir], &[]);
    let elapsed_running = start_running.elapsed();

    assert_eq!(out_running.status.code(), Some(0));
    assert!(
        elapsed_running < std::time::Duration::from_millis(6500),
        "probe with still-running parent and pipe-holding descendant must be bounded by the 5s deadline, took {elapsed_running:?}"
    );
    let stdout_running = String::from_utf8_lossy(&out_running.stdout);
    let stderr_running = String::from_utf8_lossy(&out_running.stderr);
    assert!(
        stdout_running.contains("Antigravity (antigravity) | version: unknown"),
        "timed-out still-running probe must report version unknown: {stdout_running}"
    );
    assert!(
        stderr_running.contains("timed out after 5s"),
        "timed-out still-running probe must emit timeout warning: {stderr_running}"
    );

    let parent_pid: u32 = fs::read_to_string(&parent_pid_file)
        .expect("parent-running pid file must be written")
        .trim()
        .parse()
        .expect("valid parent-running pid");
    let bridge_pid: u32 = fs::read_to_string(&bridge_pid_file)
        .expect("bridge-exited pid file must be written")
        .trim()
        .parse()
        .expect("valid bridge-exited pid");
    let grandchild_running_pid: u32 = fs::read_to_string(&pid_file_running)
        .expect("grandchild-running pid file must be written")
        .trim()
        .parse()
        .expect("valid grandchild-running pid");

    let parent_alive = is_process_alive(parent_pid);
    let bridge_alive = is_process_alive(bridge_pid);
    let grandchild_alive = is_process_alive(grandchild_running_pid);
    let sentinel_alive = is_process_alive(sentinel_pid);

    let _ = sentinel_child.kill();
    let _ = sentinel_child.wait();
    if parent_alive {
        kill_process_best_effort(parent_pid);
    }
    if bridge_alive {
        kill_process_best_effort(bridge_pid);
    }
    if grandchild_alive {
        kill_process_best_effort(grandchild_running_pid);
    }
    assert!(
        !bridge_alive,
        "intermediate bridge PID {bridge_pid} must have exited before timeout cleanup"
    );
    assert!(
        !parent_alive,
        "still-running parent PID {parent_pid} survived production timeout cleanup"
    );
    assert!(
        !grandchild_alive,
        "descendant PID {grandchild_running_pid} (with exited intermediate parent) survived production timeout cleanup"
    );
    assert!(
        sentinel_alive,
        "unrelated sentinel PID {sentinel_pid} must remain alive across timeout cleanup"
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

    // 3. "Not already up to date; update aborted." must be unverified (exit 1)
    sb.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("update_stdout", "Not already up to date; update aborted."),
        ],
    );
    let out_abort = sb.run_teemiao(&["agents", "update", "antigravity"], &[&sb.bin_dir], &[]);
    assert_eq!(out_abort.status.code(), Some(1));
    let stdout_abort = String::from_utf8_lossy(&out_abort.stdout);
    assert!(
        stdout_abort.contains("unverified"),
        "'Not already up to date; update aborted.' must be reported as unverified: {stdout_abort}"
    );

    // 4. Homebrew updater printing "Warning: ripgrep 14.1.0 is already installed" for Codex must be unverified (exit 1)
    let sb_brew = TestSandbox::new();
    let brew_prefix = sb_brew.root.join("brew");
    let brew_bin = brew_prefix.join("bin");
    let cask_dir = brew_prefix.join("Caskroom").join("codex").join("0.118.0");
    fs::create_dir_all(&cask_dir).unwrap();
    fs::create_dir_all(&brew_bin).unwrap();
    let (cask_codex, _) = sb_brew.install_fake_tool_in(
        &cask_dir,
        "codex",
        &[("version_stdout", "codex-cli 0.118.0")],
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(&cask_codex, brew_bin.join("codex")).unwrap();
    #[cfg(not(unix))]
    {
        fs::write(
            brew_bin.join("codex.cmd"),
            format!("@echo off\r\n\"{}\" %*\r\n", cask_codex.display()),
        )
        .unwrap();
    }
    sb_brew.install_fake_tool_in(
        &brew_bin,
        "brew",
        &[
            ("prefix_stdout", &brew_prefix.to_string_lossy()),
            ("list_stdout", "codex"),
            (
                "update_stdout",
                "Warning: ripgrep 14.1.0 is already installed",
            ),
        ],
    );
    let out_brew = sb_brew.run_teemiao(&["agents", "update", "codex"], &[&brew_bin], &[]);
    assert_eq!(out_brew.status.code(), Some(1));
    let stdout_brew = String::from_utf8_lossy(&out_brew.stdout);
    assert!(
        stdout_brew.contains("unverified"),
        "foreign 'ripgrep is already installed' message must not report Codex up to date: {stdout_brew}"
    );

    // 5. Homebrew updater printing negated "codex is not already installed" must be unverified (exit 1)
    let sb_brew_neg = TestSandbox::new();
    let brew_prefix_neg = sb_brew_neg.root.join("homebrew");
    let brew_bin_neg = brew_prefix_neg.join("bin");
    let cask_dir_neg = brew_prefix_neg
        .join("Caskroom")
        .join("codex")
        .join("0.118.0");
    fs::create_dir_all(&cask_dir_neg).unwrap();
    let (cask_codex_neg, _) = sb_brew_neg.install_fake_tool_in(
        &cask_dir_neg,
        "codex",
        &[("version_stdout", "codex-cli 0.118.0")],
    );
    fs::create_dir_all(&brew_bin_neg).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&cask_codex_neg, brew_bin_neg.join("codex")).unwrap();
    #[cfg(not(unix))]
    fs::write(
        brew_bin_neg.join("codex.cmd"),
        format!("@echo off\r\n\"{}\" %*\r\n", cask_codex_neg.display()),
    )
    .unwrap();
    let prefix_neg_str = brew_prefix_neg.to_string_lossy().into_owned();
    sb_brew_neg.install_fake_tool_in(
        &brew_bin_neg,
        "brew",
        &[
            ("prefix_stdout", &prefix_neg_str),
            ("list_stdout", "codex"),
            ("update_stdout", "codex is not already installed"),
        ],
    );
    let out_brew_neg =
        sb_brew_neg.run_teemiao(&["agents", "update", "codex"], &[&brew_bin_neg], &[]);
    assert_eq!(out_brew_neg.status.code(), Some(1));
    let stdout_brew_neg = String::from_utf8_lossy(&out_brew_neg.stdout);
    assert!(
        stdout_brew_neg.contains("unverified"),
        "negated Homebrew 'codex is not already installed' must not report Codex up to date: {stdout_brew_neg}"
    );

    // 6. Native updater printing progress/check-start "Checking whether Antigravity is up to date..." must be unverified (exit 1)
    let sb_check = TestSandbox::new();
    sb_check.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity CLI 1.0.0"),
            (
                "update_stdout",
                "Checking whether Antigravity is up to date...",
            ),
        ],
    );
    let out_check = sb_check.run_teemiao(
        &["agents", "update", "antigravity"],
        &[&sb_check.bin_dir],
        &[],
    );
    assert_eq!(out_check.status.code(), Some(1));
    let stdout_check = String::from_utf8_lossy(&out_check.stdout);
    assert!(
        stdout_check.contains("unverified"),
        "check-start message 'Checking whether Antigravity is up to date...' must be unverified: {stdout_check}"
    );
}

#[test]
fn agents_cursor_accepts_date_hash_versions_and_prefers_verified_primary_over_older_alias() {
    let sb = TestSandbox::new();
    #[cfg(unix)]
    {
        let version_dir = sb
            .home
            .join(".local")
            .join("share")
            .join("cursor-agent")
            .join("versions")
            .join("2026.11.05-86047df");
        fs::create_dir_all(&version_dir).unwrap();
        let (target_agent, _) = sb.install_fake_tool_in(
            &version_dir,
            "agent",
            &[
                ("version_stdout", "2026.11.05-86047df"),
                ("help_stdout", "Start the Cursor Agent"),
            ],
        );
        let link_path = sb.bin_dir.join("agent");
        let _ = fs::remove_file(&link_path);
        std::os::unix::fs::symlink(&target_agent, &link_path).unwrap();
    }
    #[cfg(not(unix))]
    {
        sb.install_fake_tool(
            "agent",
            &[
                ("version_stdout", "2026.11.05-86047df"),
                ("help_stdout", "Start the Cursor Agent"),
            ],
        );
    }

    // Without alias: `agent` with date/hash version `2026.11.05-86047df` is verified
    let out1 = sb.run_teemiao(
        &["agents", "update", "cursor", "--dry-run"],
        &[&sb.bin_dir],
        &[],
    );
    assert_eq!(out1.status.code(), Some(0));
    let stdout1 = String::from_utf8_lossy(&out1.stdout);
    assert!(
        stdout1.contains("would run: agent update") && stdout1.contains("2026.11.05-86047df"),
        "expected verified primary 'agent update' with 2026.11.05-86047df, got: {stdout1}"
    );

    // With older `cursor-agent` alias also present in `bin_dir`: primary `agent` still wins
    sb.install_fake_tool(
        "cursor-agent",
        &[("version_stdout", "Cursor Agent 2026.01.01-old")],
    );
    let out2 = sb.run_teemiao(
        &["agents", "update", "cursor", "--dry-run"],
        &[&sb.bin_dir],
        &[],
    );
    assert_eq!(out2.status.code(), Some(0));
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    assert!(
        stdout2.contains("would run: agent update") && stdout2.contains("2026.11.05-86047df"),
        "verified primary 'agent' must take precedence over older 'cursor-agent': {stdout2}"
    );
}

#[test]
fn agents_npm_wrapper_rejects_other_prefix_target_and_generic_node_comment() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // 1. Wrapper in `stale/bin/codex` executing `active/lib/node_modules/@openai/codex/bin/codex`
        let sb = TestSandbox::new();
        let active_prefix = sb.root.join("active");
        let stale_prefix = sb.root.join("stale");
        for prefix in [&active_prefix, &stale_prefix] {
            let pkg_dir = prefix
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
        let active_bin_dir = active_prefix
            .join("lib")
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("bin");
        let (active_codex, _) = sb.install_fake_tool_in(
            &active_bin_dir,
            "codex",
            &[("version_stdout", "codex-cli 0.118.0")],
        );
        let stale_bin = stale_prefix.join("bin");
        fs::create_dir_all(&stale_bin).unwrap();
        let wrapper_path = stale_bin.join("codex");
        fs::write(
            &wrapper_path,
            format!("#!/bin/sh\nexec '{}' \"$@\"\n", active_codex.display()),
        )
        .unwrap();
        let mut perms = fs::metadata(&wrapper_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&wrapper_path, perms).unwrap();
        sb.install_fake_tool_in(&stale_bin, "npm", &[("update_stdout", "added 1 package")]);

        let out = sb.run_teemiao(
            &["agents", "update", "codex", "--dry-run"],
            &[&stale_bin],
            &[],
        );
        assert_eq!(out.status.code(), Some(1));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("manual action required"),
            "wrapper in stale/bin targeting active/lib/node_modules must not authorize updating stale prefix: {stdout}"
        );

        // 2. Unrelated wrapper in `prefix/bin/codex` with `# unrelated wrapper: env node` comment
        let sb2 = TestSandbox::new();
        let prefix2 = sb2.root.join("prefix");
        let pkg2 = prefix2
            .join("lib")
            .join("node_modules")
            .join("@openai")
            .join("codex");
        fs::create_dir_all(&pkg2).unwrap();
        fs::write(
            pkg2.join("package.json"),
            r#"{"name":"@openai/codex","version":"0.118.0"}"#,
        )
        .unwrap();
        let bin2 = prefix2.join("bin");
        fs::create_dir_all(&bin2).unwrap();
        let wrapper2 = bin2.join("codex");
        fs::write(
            &wrapper2,
            "#!/bin/sh\n# unrelated wrapper: env node\necho \"codex-cli 0.118.0\"\n",
        )
        .unwrap();
        let mut perms2 = fs::metadata(&wrapper2).unwrap().permissions();
        perms2.set_mode(0o755);
        fs::set_permissions(&wrapper2, perms2).unwrap();
        sb2.install_fake_tool_in(&bin2, "npm", &[("update_stdout", "added 1 package")]);

        let out2 = sb2.run_teemiao(&["agents", "update", "codex", "--dry-run"], &[&bin2], &[]);
        assert_eq!(out2.status.code(), Some(1));
        let stdout2 = String::from_utf8_lossy(&out2.stdout);
        assert!(
            stdout2.contains("manual action required"),
            "unrelated wrapper with 'env node' comment must not authorize npm update: {stdout2}"
        );

        // 3. Wrapper that prints npm path in diagnostic `printf` and `exec`s independent binary
        let sb3 = TestSandbox::new();
        let stale3 = sb3.root.join("stale");
        let pkg3 = stale3
            .join("lib")
            .join("node_modules")
            .join("@openai")
            .join("codex");
        fs::create_dir_all(&pkg3).unwrap();
        fs::write(
            pkg3.join("package.json"),
            r#"{"name":"@openai/codex","version":"0.118.0"}"#,
        )
        .unwrap();
        let (stale_target3, _) = sb3.install_fake_tool_in(
            &pkg3.join("bin"),
            "codex",
            &[("version_stdout", "codex-cli 0.118.0")],
        );
        let (indep_target3, _) = sb3.install_fake_tool_in(
            &sb3.root.join("independent"),
            "codex",
            &[("version_stdout", "codex-cli 0.118.0")],
        );
        let bin3 = stale3.join("bin");
        fs::create_dir_all(&bin3).unwrap();
        let wrapper3 = bin3.join("codex");
        fs::write(
            &wrapper3,
            format!(
                "#!/bin/sh\nprintf 'Unused npm target: %s\\n' '{}' >&2\nexec '{}' \"$@\"\n",
                stale_target3.display(),
                indep_target3.display()
            ),
        )
        .unwrap();
        let mut perms3 = fs::metadata(&wrapper3).unwrap().permissions();
        perms3.set_mode(0o755);
        fs::set_permissions(&wrapper3, perms3).unwrap();
        sb3.install_fake_tool_in(&bin3, "npm", &[("update_stdout", "added 1 package")]);

        let out3 = sb3.run_teemiao(&["agents", "update", "codex", "--dry-run"], &[&bin3], &[]);
        assert_eq!(out3.status.code(), Some(1));
        let stdout3 = String::from_utf8_lossy(&out3.stdout);
        assert!(
            stdout3.contains("manual action required"),
            "wrapper that only prints npm path before exec'ing independent binary must require manual action: {stdout3}"
        );

        // 4. Reassigned `basedir` pointing to `active/bin` while wrapper lives in `stale/bin` must require manual action
        let wrapper4 = stale_bin.join("codex");
        fs::write(
            &wrapper4,
            format!(
                "#!/bin/sh\nbasedir='{}'\nexec \"$basedir/../lib/node_modules/@openai/codex/bin/codex\" \"$@\"\n",
                active_prefix.join("bin").display()
            ),
        )
        .unwrap();
        let mut perms4 = fs::metadata(&wrapper4).unwrap().permissions();
        perms4.set_mode(0o755);
        fs::set_permissions(&wrapper4, perms4).unwrap();
        let out4 = sb.run_teemiao(
            &["agents", "update", "codex", "--dry-run"],
            &[&stale_bin],
            &[],
        );
        assert_eq!(out4.status.code(), Some(1));
        let stdout4 = String::from_utf8_lossy(&out4.stdout);
        assert!(
            stdout4.contains("manual action required"),
            "wrapper with reassigned basedir must not authorize updating stale prefix: {stdout4}"
        );

        // 5. Bare working-directory-relative target (`exec "../lib/node_modules/@openai/codex/bin/codex" "$@"`) must require manual action
        fs::write(
            &wrapper4,
            "#!/bin/sh\nexec \"../lib/node_modules/@openai/codex/bin/codex\" \"$@\"\n",
        )
        .unwrap();
        let mut perms5 = fs::metadata(&wrapper4).unwrap().permissions();
        perms5.set_mode(0o755);
        fs::set_permissions(&wrapper4, perms5).unwrap();
        let out5 = sb.run_teemiao(
            &["agents", "update", "codex", "--dry-run"],
            &[&stale_bin],
            &[],
        );
        assert_eq!(out5.status.code(), Some(1));
        let stdout5 = String::from_utf8_lossy(&out5.stdout);
        assert!(
            stdout5.contains("manual action required"),
            "wrapper with bare working-directory-relative target must not authorize updating stale prefix: {stdout5}"
        );
    }
    #[cfg(not(unix))]
    {
        let sb = TestSandbox::new();
        let active_prefix = sb.root.join("active");
        let stale_prefix = sb.root.join("stale");
        for prefix in [&active_prefix, &stale_prefix] {
            let pkg_dir = prefix
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
        let active_bin_dir = active_prefix
            .join("lib")
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("bin");
        let (active_codex, _) = sb.install_fake_tool_in(
            &active_bin_dir,
            "codex",
            &[("version_stdout", "codex-cli 0.118.0")],
        );
        let stale_bin = stale_prefix.join("bin");
        fs::create_dir_all(&stale_bin).unwrap();
        fs::write(
            stale_bin.join("codex.cmd"),
            format!("@echo off\r\n\"{}\" %*\r\n", active_codex.display()),
        )
        .unwrap();
        sb.install_fake_tool_in(&stale_bin, "npm", &[("update_stdout", "added 1 package")]);

        let out = sb.run_teemiao(
            &["agents", "update", "codex", "--dry-run"],
            &[&stale_bin],
            &[],
        );
        assert_eq!(out.status.code(), Some(1));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("manual action required"),
            "Windows .cmd wrapper in stale/bin targeting active/lib/node_modules must not authorize updating stale prefix: {stdout}"
        );

        // Reassigned `dp0` in Windows .cmd wrapper must also require manual action
        fs::write(
            stale_bin.join("codex.cmd"),
            format!(
                "@echo off\r\nSET dp0={}\r\n\"%dp0%\\lib\\node_modules\\@openai\\codex\\bin\\codex.exe\" %*\r\n",
                active_prefix.display()
            ),
        )
        .unwrap();
        let out_dp0 = sb.run_teemiao(
            &["agents", "update", "codex", "--dry-run"],
            &[&stale_bin],
            &[],
        );
        assert_eq!(out_dp0.status.code(), Some(1));
        let stdout_dp0 = String::from_utf8_lossy(&out_dp0.stdout);
        assert!(
            stdout_dp0.contains("manual action required"),
            "Windows .cmd wrapper with reassigned dp0 must not authorize updating stale prefix: {stdout_dp0}"
        );
    }
}

#[test]
fn agents_update_separates_multiline_and_no_newline_sections_flushes_header_before_updater_and_localizes_summary()
 {
    let sb = TestSandbox::new();
    let stderr_file = sb.root.join("live_stderr.log");
    let agy_snap_file = sb.root.join("agy_observed_stderr.txt");
    let kimi_snap_file = sb.root.join("kimi_observed_stderr.txt");

    sb.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("next_version_stdout", "Antigravity 1.1.0"),
            (
                "update_stdout",
                "Downloading Antigravity 1.1.0<NL>Applying update without final newline",
            ),
            ("update_stderr", "agy note line 1<NL>agy note line 2"),
            ("update_no_trailing_newline", "1"),
            ("observe_stderr_file", &stderr_file.to_string_lossy()),
            ("observe_stderr_out_file", &agy_snap_file.to_string_lossy()),
        ],
    );
    sb.install_fake_tool(
        "kimi",
        &[
            ("version_stdout", "kimi 0.4.0"),
            ("next_version_stdout", "kimi 0.5.0"),
            ("update_stdout", "Upgraded kimi to 0.5.0"),
            ("observe_stderr_file", &stderr_file.to_string_lossy()),
            ("observe_stderr_out_file", &kimi_snap_file.to_string_lossy()),
        ],
    );

    let out =
        sb.run_teemiao_with_stderr_file(&["agents", "update"], &[&sb.bin_dir], &[], &stderr_file);
    assert_eq!(out.status.code(), Some(0));

    let agy_observed = fs::read_to_string(&agy_snap_file).expect("agy observed stderr snapshot");
    assert_eq!(
        agy_observed, "--- [1/2] Antigravity (antigravity) ---\n",
        "section header [1/2] must be flushed to stderr before agy updater executes"
    );

    let kimi_observed = fs::read_to_string(&kimi_snap_file).expect("kimi observed stderr snapshot");
    assert!(
        kimi_observed.starts_with("--- [1/2] Antigravity (antigravity) ---\nDiagnostics for Antigravity (antigravity): agy note line 1\nagy note line 2\nDownloading Antigravity 1.1.0\nApplying update without final newline\n\n--- [2/2] Kimi (kimi) ---\n"),
        "section [1/2] multiline/no-newline diagnostics must finish with a newline and blank line before section [2/2] header is flushed: {kimi_observed:?}"
    );

    let final_stderr = fs::read_to_string(&stderr_file).expect("read final stderr");
    assert!(final_stderr.ends_with('\n'));
    assert!(!final_stderr.contains("Update summary"));

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("\nUpdate summary\nAntigravity (antigravity): updated (1.0.0 -> 1.1.0;"),
        "expected separated Update summary on stdout, got: {stdout:?}"
    );
    assert!(stdout.contains("\nKimi (kimi): updated (0.4.0 -> 0.5.0;"));

    // Verify zh-CN localization for section header and summary heading
    let out_zh = sb.run_teemiao(
        &["agents", "update", "agy", "--dry-run"],
        &[&sb.bin_dir],
        &[("LC_ALL", "zh_CN.UTF-8"), ("LANG", "zh_CN.UTF-8")],
    );
    assert_eq!(out_zh.status.code(), Some(0));
    let stderr_zh = String::from_utf8_lossy(&out_zh.stderr);
    let stdout_zh = String::from_utf8_lossy(&out_zh.stdout);
    assert_eq!(stderr_zh, "--- [1/1] Antigravity (antigravity) ---\n");
    assert!(
        stdout_zh.starts_with("\n更新摘要\nAntigravity (antigravity): 演练模式"),
        "expected zh-CN summary heading on stdout: {stdout_zh:?}"
    );
}

#[test]
fn agents_qoder_catalog_order_discovery_precedence_desktop_rejection_and_unknown_version() {
    let sb = TestSandbox::new();
    setup_codex_standalone(&sb, "0.118.0");
    sb.install_fake_tool("agy", &[("version_stdout", "Antigravity CLI 1.4.2")]);
    sb.install_fake_tool("opencode", &[("version_stdout", "opencode v0.6.1")]);
    sb.install_fake_tool("kimi", &[("version_stdout", "kimi, version 0.5.3")]);
    sb.install_fake_tool("agent", &[("version_stdout", "Cursor Agent CLI 0.48.0")]);
    sb.install_fake_tool("claude", &[("version_stdout", "2.1.4 (Claude Code)")]);
    sb.install_fake_tool(
        "qodercli",
        &[
            ("version_stdout", "qodercli 0.9.1"),
            (
                "help_stdout",
                "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
        ],
    );
    sb.install_fake_tool(
        "qoder",
        &[
            ("version_stdout", "Qoder CLI 0.8.0"),
            (
                "help_stdout",
                "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
        ],
    );

    let out = sb.run_teemiao(&["agents", "list"], &[&sb.bin_dir], &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);

    let claude_pos = stdout
        .find("Claude Code (claude-code)")
        .expect("Claude Code in output");
    let qoder_pos = stdout.find("Qoder (qoder)").expect("Qoder in output");
    assert!(
        claude_pos < qoder_pos,
        "Qoder must appear after Claude Code in catalog order: {stdout}"
    );
    assert_eq!(
        stdout.matches("Qoder (qoder)").count(),
        1,
        "Qoder must only be listed once when both qodercli and qoder exist on PATH: {stdout}"
    );
    assert!(
        stdout.contains("0.9.1"),
        "qodercli must take precedence over qoder: {stdout}"
    );
    assert!(
        !stdout.contains("0.8.0"),
        "shadowed qoder version must not be listed: {stdout}"
    );
    assert!(stdout.contains("qodercli update"));

    // Fallback to verified `qoder` when `qodercli` is desktop/unverified, plus rejecting bare/desktop `qoder`
    let sb_fallback = TestSandbox::new();
    sb_fallback.install_fake_tool(
        "qodercli",
        &[
            ("version_stdout", "Qoder Desktop 2.0.0"),
            ("help_stdout", "Qoder Desktop IDE --new-window"),
        ],
    );
    sb_fallback.install_fake_tool(
        "qoder",
        &[
            ("version_stdout", "Qoder 0.8.4"),
            (
                "help_stdout",
                "Qoder CLI coding agent<NL>Commands:<NL>  update  Self-update Qoder CLI",
            ),
        ],
    );
    let out_fb = sb_fallback.run_teemiao(&["agents", "list"], &[&sb_fallback.bin_dir], &[]);
    assert_eq!(out_fb.status.code(), Some(0));
    let stdout_fb = String::from_utf8_lossy(&out_fb.stdout);
    assert!(stdout_fb.contains("Qoder (qoder) | version: 0.8.4"));
    assert!(stdout_fb.contains("qoder update"));

    // Desktop / bare `qoder` without positive CLI evidence is rejected
    let sb_reject = TestSandbox::new();
    sb_reject.install_fake_tool(
        "qoder",
        &[
            ("version_stdout", "Qoder 2.0.0"),
            ("help_stdout", "Usage: qoder [file]"),
        ],
    );
    let out_rej = sb_reject.run_teemiao(&["agents", "list"], &[&sb_reject.bin_dir], &[]);
    assert_eq!(out_rej.status.code(), Some(0));
    let stdout_rej = String::from_utf8_lossy(&out_rej.stdout);
    assert!(
        stdout_rej.contains("No supported AI coding-agent CLIs were found on PATH."),
        "bare product/version qoder without CLI evidence must be rejected: {stdout_rej}"
    );

    // Absent Qoder targeted update fails cleanly without section header or summary header
    let out_absent = sb_reject.run_teemiao(
        &["agents", "update", "qodercli"],
        &[&sb_reject.bin_dir],
        &[],
    );
    assert_eq!(out_absent.status.code(), Some(1));
    let stderr_absent = String::from_utf8_lossy(&out_absent.stderr);
    let stdout_absent = String::from_utf8_lossy(&out_absent.stdout);
    assert!(stderr_absent.contains("Error: agent 'Qoder' (qoder) is not installed on PATH."));
    assert!(!stderr_absent.contains("--- ["));
    assert!(stdout_absent.is_empty());

    // Unknown version warning when `qodercli --version` exits non-zero
    let sb_unknown = TestSandbox::new();
    sb_unknown.install_fake_tool(
        "qodercli",
        &[
            ("version_exit", "2"),
            (
                "help_stdout",
                "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
        ],
    );
    let out_unk = sb_unknown.run_teemiao(&["agents", "list"], &[&sb_unknown.bin_dir], &[]);
    assert_eq!(out_unk.status.code(), Some(0));
    let stdout_unk = String::from_utf8_lossy(&out_unk.stdout);
    let stderr_unk = String::from_utf8_lossy(&out_unk.stderr);
    assert!(stdout_unk.contains("Qoder (qoder) | version: unknown"));
    assert!(stderr_unk.contains("Warning: version probe for Qoder"));
}

#[test]
fn agents_qoder_update_native_npm_dry_run_partial_failure_and_unverified_guards() {
    // 1. Verified native `qodercli` dry-run and real update via both `qoder` and `qodercli` identifiers
    let sb = TestSandbox::new();
    sb.install_fake_tool(
        "qodercli",
        &[
            ("version_stdout", "qodercli 0.9.0"),
            ("next_version_stdout", "qodercli 0.9.1"),
            (
                "help_stdout",
                "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
            ("update_stdout", "Updated Qoder CLI to 0.9.1"),
        ],
    );

    let out_dry = sb.run_teemiao(
        &["agents", "update", "qodercli", "--dry-run"],
        &[&sb.bin_dir],
        &[],
    );
    assert_eq!(out_dry.status.code(), Some(0));
    assert!(
        !sb.invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=qodercli|ARGS=update|")),
        "dry-run must not invoke qodercli update"
    );
    let stdout_dry = String::from_utf8_lossy(&out_dry.stdout);
    assert!(
        stdout_dry.contains("Qoder (qoder): dry-run (would run: qodercli update;"),
        "expected Qoder dry-run plan: {stdout_dry}"
    );
    assert!(stdout_dry.contains("current version: 0.9.0)"));

    let out_up = sb.run_teemiao(&["agents", "update", "qoder"], &[&sb.bin_dir], &[]);
    assert_eq!(out_up.status.code(), Some(0));
    let stdout_up = String::from_utf8_lossy(&out_up.stdout);
    assert!(
        stdout_up.contains("Qoder (qoder): updated (0.9.0 -> 0.9.1; path:"),
        "expected Qoder native update transition: {stdout_up}"
    );

    // 2. Unchanged version with negated / foreign / non-confirming output -> unverified (exit 1),
    // and partial failure in multi-agent batch continues to remaining agents
    let sb_unv = TestSandbox::new();
    sb_unv.install_fake_tool(
        "qoder",
        &[
            ("version_stdout", "Qoder CLI 0.9.1"),
            ("next_version_stdout", "Qoder CLI 0.9.1"),
            (
                "help_stdout",
                "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
            ("update_stdout", "Qoder CLI is not up to date"),
        ],
    );
    sb_unv.install_fake_tool(
        "agy",
        &[
            ("version_stdout", "Antigravity 1.0.0"),
            ("next_version_stdout", "Antigravity 1.1.0"),
            ("update_stdout", "Updated Antigravity to 1.1.0"),
        ],
    );
    let out_batch = sb_unv.run_teemiao(&["agents", "update"], &[&sb_unv.bin_dir], &[]);
    assert_eq!(out_batch.status.code(), Some(1));
    let stdout_batch = String::from_utf8_lossy(&out_batch.stdout);
    let stderr_batch = String::from_utf8_lossy(&out_batch.stderr);
    assert!(stderr_batch.contains("--- [1/2] Antigravity (antigravity) ---"));
    assert!(stderr_batch.contains("--- [2/2] Qoder (qoder) ---"));
    assert!(stdout_batch.contains("Antigravity (antigravity): updated (1.0.0 -> 1.1.0;"));
    assert!(stdout_batch.contains("Qoder (qoder): unverified"));

    // 3. Proven npm `@qoder-ai/qodercli` update, wrong-prefix wrapper rejection, and missing npm
    let sb_npm = TestSandbox::new();
    let npm_prefix = sb_npm.root.join("npm_global");
    let npm_bin = npm_prefix.join("bin");
    let pkg_dir = npm_prefix
        .join("lib")
        .join("node_modules")
        .join("@qoder-ai")
        .join("qodercli");
    let pkg_bin = pkg_dir.join("bin");
    fs::create_dir_all(&npm_bin).unwrap();
    fs::create_dir_all(&pkg_bin).unwrap();
    fs::write(
        pkg_dir.join("package.json"),
        r#"{"name":"@qoder-ai/qodercli","version":"0.9.0"}"#,
    )
    .unwrap();

    let (real_qodercli, qodercli_cfg) = sb_npm.install_fake_tool_in(
        &pkg_bin,
        "qodercli",
        &[
            ("version_stdout", "0.9.0"),
            (
                "help_stdout",
                "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
        ],
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_qodercli, npm_bin.join("qodercli")).unwrap();
    }
    #[cfg(not(unix))]
    {
        fs::write(
            npm_bin.join("qodercli.cmd"),
            format!("@echo off\r\n\"{}\" %*\r\n", real_qodercli.display()),
        )
        .unwrap();
    }

    // Missing `npm` on PATH must require manual action and NEVER fall back to native `qodercli update`
    let out_no_npm = sb_npm.run_teemiao(&["agents", "update", "qoder"], &[&npm_bin], &[]);
    assert_eq!(out_no_npm.status.code(), Some(1));
    let stdout_no_npm = String::from_utf8_lossy(&out_no_npm.stdout);
    assert!(
        stdout_no_npm.contains("manual action required"),
        "npm-owned Qoder without npm on PATH must require manual action: {stdout_no_npm}"
    );
    assert!(
        !sb_npm
            .invocations()
            .iter()
            .any(|l| l.starts_with("TOOL=qodercli|ARGS=update|")),
        "missing npm must never fall back to native qodercli update"
    );

    // Install fake `npm` in `npm_bin` and verify proven npm update
    sb_npm.install_fake_tool_in(
        &npm_bin,
        "npm",
        &[
            ("next_version_stdout", "0.9.2"),
            ("target_cfg_path", &qodercli_cfg.to_string_lossy()),
            ("update_stdout", "added 1 package"),
        ],
    );
    let out_npm_up = sb_npm.run_teemiao(&["agents", "update", "qoder"], &[&npm_bin], &[]);
    assert_eq!(out_npm_up.status.code(), Some(0));
    let stdout_npm_up = String::from_utf8_lossy(&out_npm_up.stdout);
    assert!(
        stdout_npm_up.contains("Qoder (qoder): updated (0.9.0 -> 0.9.2; path:"),
        "expected proven npm global update for Qoder: {stdout_npm_up}"
    );
    assert!(
        sb_npm
            .invocations()
            .iter()
            .any(|l| l.contains("TOOL=npm|") && l.contains("@qoder-ai/qodercli@latest")),
        "expected npm install -g --prefix <prefix> @qoder-ai/qodercli@latest invocation"
    );

    // 4. Cross-prefix Homebrew wrapper (`wrapper/bin/qodercli` -> `separate-brew/Cellar/qodercli/0.9.0/bin/qodercli`)
    // must require manual action and perform zero updater invocations
    let sb_brew_wrap = TestSandbox::new();
    let wrapper_bin = sb_brew_wrap.root.join("wrapper").join("bin");
    let cellar_bin = sb_brew_wrap
        .root
        .join("separate-brew")
        .join("Cellar")
        .join("qodercli")
        .join("0.9.0")
        .join("bin");
    fs::create_dir_all(&wrapper_bin).unwrap();
    let (cellar_qodercli, _) = sb_brew_wrap.install_fake_tool_in(
        &cellar_bin,
        "qodercli",
        &[
            ("version_stdout", "Qoder CLI 0.9.0"),
            (
                "help_stdout",
                "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
            ("update_stdout", "Qoder CLI is up to date"),
        ],
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let launcher = wrapper_bin.join("qodercli");
        fs::write(
            &launcher,
            format!("#!/bin/sh\nexec '{}' \"$@\"\n", cellar_qodercli.display()),
        )
        .unwrap();
        let mut perms = fs::metadata(&launcher).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&launcher, perms).unwrap();
    }
    #[cfg(not(unix))]
    {
        fs::write(
            wrapper_bin.join("qodercli.cmd"),
            format!("@echo off\r\n\"{}\" %*\r\n", cellar_qodercli.display()),
        )
        .unwrap();
    }
    let out_brew_wrap =
        sb_brew_wrap.run_teemiao(&["agents", "update", "qoder"], &[&wrapper_bin], &[]);
    assert_eq!(out_brew_wrap.status.code(), Some(1));
    let stdout_brew_wrap = String::from_utf8_lossy(&out_brew_wrap.stdout);
    assert!(
        stdout_brew_wrap.contains("manual action required"),
        "cross-prefix Homebrew wrapper must require manual action: {stdout_brew_wrap}"
    );
    assert!(
        !sb_brew_wrap
            .invocations()
            .iter()
            .any(|l| l.contains("|ARGS=update|")),
        "cross-prefix Homebrew wrapper must perform zero updater invocations: {:?}",
        sb_brew_wrap.invocations()
    );

    // 5. Negated help text and failed help probe must require manual action and perform zero updater invocations
    for (help_exit, help_text) in [
        (
            "0",
            "Qoder CLI<NL>Do not run qodercli update; use your package manager.",
        ),
        (
            "1",
            "Qoder CLI<NL>Error: qodercli update is disabled by policy",
        ),
    ] {
        let sb_neg = TestSandbox::new();
        sb_neg.install_fake_tool(
            "qodercli",
            &[
                ("version_stdout", "Qoder CLI 0.9.0"),
                ("help_stdout", help_text),
                ("help_exit", help_exit),
                ("update_stdout", "Qoder CLI is up to date"),
            ],
        );
        let out_neg = sb_neg.run_teemiao(&["agents", "update", "qoder"], &[&sb_neg.bin_dir], &[]);
        assert_eq!(out_neg.status.code(), Some(1));
        let stdout_neg = String::from_utf8_lossy(&out_neg.stdout);
        assert!(
            stdout_neg.contains("manual action required"),
            "expected manual action required for help_exit={help_exit} help={help_text}: {stdout_neg}"
        );
        assert!(
            !sb_neg
                .invocations()
                .iter()
                .any(|l| l.contains("|ARGS=update|")),
            "negated or failed help probe must perform zero updater invocations: {:?}",
            sb_neg.invocations()
        );
    }

    // 6. Directory name `.qodercli/bin` and nearby `package.json` cannot establish CLI identity for shared `qoder`
    let sb_dir_id = TestSandbox::new();
    let dot_qodercli = sb_dir_id.root.join(".qodercli");
    let dot_qodercli_bin = dot_qodercli.join("bin");
    fs::create_dir_all(&dot_qodercli_bin).unwrap();
    fs::write(
        dot_qodercli.join("package.json"),
        r#"{"name":"@qoder-ai/qodercli","version":"1.2.3"}"#,
    )
    .unwrap();
    sb_dir_id.install_fake_tool_in(
        &dot_qodercli_bin,
        "qoder",
        &[
            ("version_stdout", "1.2.3"),
            (
                "help_stdout",
                "Usage: index-tool<NL>Commands:<NL>  update  Rebuild the document index",
            ),
            ("update_stdout", "Qoder CLI is up to date"),
        ],
    );
    let out_dir_list = sb_dir_id.run_teemiao(&["agents", "list"], &[&dot_qodercli_bin], &[]);
    assert_eq!(out_dir_list.status.code(), Some(0));
    let stdout_dir_list = String::from_utf8_lossy(&out_dir_list.stdout);
    assert!(
        stdout_dir_list.contains("No supported AI coding-agent CLIs were found on PATH."),
        "shared qoder under .qodercli/bin with index-tool help must not be discovered: {stdout_dir_list}"
    );
    let out_dir_up =
        sb_dir_id.run_teemiao(&["agents", "update", "qoder"], &[&dot_qodercli_bin], &[]);
    assert_eq!(out_dir_up.status.code(), Some(1));
    let stderr_dir_up = String::from_utf8_lossy(&out_dir_up.stderr);
    assert!(
        stderr_dir_up.contains("Error: agent 'Qoder' (qoder) is not installed on PATH."),
        "shared qoder under .qodercli/bin must be rejected before update: {stderr_dir_up}"
    );
    assert!(
        !sb_dir_id
            .invocations()
            .iter()
            .any(|l| l.contains("|ARGS=update|")),
        "rejected qoder candidate must perform zero updater invocations: {:?}",
        sb_dir_id.invocations()
    );

    // 7. Exact npm entry-point binding (`"bin": {"qoder": "bin/qoder.js"}`):
    // extensionless `bin/qoder` and `.exe` `bin/qoder.exe` siblings must NOT match `bin/qoder.js`,
    // while the exact declared `bin/qoder.js` entry point succeeds via npm update.
    for target_name in ["qoder", "qoder.exe", "qoder.js"] {
        let sb_ep = TestSandbox::new();
        let ep_prefix = sb_ep.root.join("npm");
        let ep_bin = ep_prefix.join("bin");
        let ep_pkg = ep_prefix
            .join("lib")
            .join("node_modules")
            .join("@qoder-ai")
            .join("qodercli");
        let ep_pkg_bin = ep_pkg.join("bin");
        fs::create_dir_all(&ep_bin).unwrap();
        fs::create_dir_all(&ep_pkg_bin).unwrap();
        fs::write(
            ep_pkg.join("package.json"),
            r#"{"name":"@qoder-ai/qodercli","version":"1.2.3","bin":{"qoder":"bin/qoder.js"}}"#,
        )
        .unwrap();

        let target_path = ep_pkg_bin.join(target_name);
        let (installed_bin, _) = sb_ep.install_fake_tool_in(
            &ep_pkg_bin,
            "qoder",
            &[
                ("version_stdout", "1.2.3"),
                ("help_stdout", "Usage: index-tool"),
                ("update_stdout", "Native update should never run"),
            ],
        );
        if installed_bin != target_path {
            fs::copy(&installed_bin, &target_path).unwrap();
        }
        let declared_js = ep_pkg_bin.join("qoder.js");
        if !declared_js.exists() {
            fs::copy(&installed_bin, &declared_js).unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target_path, ep_bin.join("qoder")).unwrap();
        }
        #[cfg(not(unix))]
        {
            sb_ep.install_fake_tool_in(
                &ep_bin,
                "node",
                &[
                    ("version_stdout", "1.2.3"),
                    ("help_stdout", "Usage: index-tool"),
                ],
            );
            fs::write(
                ep_bin.join("qoder.cmd"),
                format!(
                    "@echo off\r\n\"%~dp0\\node.exe\" \"{}\" %*\r\n",
                    target_path.display()
                ),
            )
            .unwrap();
        }
        sb_ep.install_fake_tool_in(
            &ep_bin,
            "npm",
            &[("update_stdout", "Qoder CLI is up to date")],
        );

        let out_ep = sb_ep.run_teemiao(&["agents", "update", "qoder"], &[&ep_bin], &[]);
        let ep_invocations = sb_ep.invocations();
        if target_name == "qoder.js" {
            assert_eq!(
                out_ep.status.code(),
                Some(0),
                "exact declared entry point `bin/qoder.js` should succeed: stdout={} stderr={}",
                String::from_utf8_lossy(&out_ep.stdout),
                String::from_utf8_lossy(&out_ep.stderr)
            );
            assert!(
                ep_invocations
                    .iter()
                    .any(|l| l.starts_with("TOOL=npm|") && l.contains("@qoder-ai/qodercli@latest")),
                "exact declared entry point must invoke npm update: {ep_invocations:?}"
            );
            assert!(
                !ep_invocations.iter().any(|l| l.contains("--help")),
                "proven npm entry point should not require a --help probe: {ep_invocations:?}"
            );
        } else {
            assert_eq!(
                out_ep.status.code(),
                Some(1),
                "mismatched sibling `{target_name}` must be rejected: stdout={} stderr={}",
                String::from_utf8_lossy(&out_ep.stdout),
                String::from_utf8_lossy(&out_ep.stderr)
            );
            assert!(
                !ep_invocations
                    .iter()
                    .any(|l| l.starts_with("TOOL=npm|") || l.contains("|ARGS=update|")),
                "mismatched sibling `{target_name}` must perform zero npm/native update calls: {ep_invocations:?}"
            );
            assert!(
                ep_invocations.iter().any(|l| l.contains("--help")),
                "mismatched sibling `{target_name}` must fall back to --help probe and be rejected: {ep_invocations:?}"
            );
        }
    }

    // 8. Architecture banner (`darwin-arm64` / `linux x64`) as well as multiline `Usage:` and description paragraphs
    // in `--help` are accepted, while conflicting CLI branding in `--help` (`Acme Qoder CLI` for `qoder`/`qodercli`,
    // `Acme CLI` for `qodercli`) is rejected with zero updater invocations
    for help_layout in [
        "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
        "Qoder CLI<NL>Usage:<NL>  qodercli [command]<NL>Commands:<NL>  update  Update Qoder CLI",
        "Qoder CLI<NL>Usage:<NL>  qodercli update<NL>Commands:<NL>  update  Update Qoder CLI",
        "Qoder CLI<NL>An AI coding assistant for your terminal.<NL>Usage: qodercli [command]<NL>Commands:<NL>  update  Update Qoder CLI",
    ] {
        let sb_arch = TestSandbox::new();
        sb_arch.install_fake_tool(
            "qodercli",
            &[
                ("version_stdout", "Qoder CLI 0.9.1 (darwin-arm64)"),
                ("help_stdout", help_layout),
                ("update_stdout", "Qoder CLI is up to date"),
            ],
        );
        let out_list_arch = sb_arch.run_teemiao(&["agents", "list"], &[&sb_arch.bin_dir], &[]);
        assert_eq!(out_list_arch.status.code(), Some(0));
        assert!(
            String::from_utf8_lossy(&out_list_arch.stdout).contains("Qoder (qoder)"),
            "expected Qoder to be listed for help layout `{help_layout}`, stdout={}",
            String::from_utf8_lossy(&out_list_arch.stdout)
        );
        let out_arch =
            sb_arch.run_teemiao(&["agents", "update", "qoder"], &[&sb_arch.bin_dir], &[]);
        assert_eq!(
            out_arch.status.code(),
            Some(0),
            "expected update to succeed for help layout `{help_layout}`, stderr={}",
            String::from_utf8_lossy(&out_arch.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out_arch.stdout)
                .contains("Qoder (qoder): up to date (version: 0.9.1;"),
            "expected darwin-arm64 banner + help layout `{help_layout}` to be accepted: {}",
            String::from_utf8_lossy(&out_arch.stdout)
        );
        assert!(
            sb_arch
                .invocations()
                .iter()
                .any(|l| l.contains("|ARGS=update|")),
            "expected native updater invocation for help layout `{help_layout}`: {:?}",
            sb_arch.invocations()
        );
    }

    for (bin_name, ver_out, conflicting_help) in [
        (
            "qoder",
            "1.2.3",
            "Acme Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
        ),
        (
            "qodercli",
            "Qoder CLI 0.9.0",
            "Acme Qoder CLI<NL>Usage: qodercli [command]<NL>Commands:<NL>  update  Update Qoder CLI",
        ),
        (
            "qodercli",
            "1.2.3",
            "Acme CLI<NL>Commands:<NL>  update  Update CLI",
        ),
        (
            "qodercli",
            "1.2.3",
            "Acme Code CLI helps you build projects.<NL>Usage: qodercli [command]<NL>Commands:<NL>  update  Update Qoder CLI",
        ),
        (
            "qodercli",
            "1.2.3",
            "Qoder CLI<NL>Usage:<NL>  index-tool [command]<NL>Commands:<NL>  update  Update Qoder CLI",
        ),
        (
            "qodercli",
            "1.2.3",
            "Qoder CLI<NL>Usage:<NL>  index-tool update<NL>Commands:<NL>  update  Update Qoder CLI",
        ),
        (
            "qodercli",
            "1.2.3",
            "Qoder CLI<NL>Usage:<NL>  index-tool --help<NL>Commands:<NL>  update  Update Qoder CLI",
        ),
        (
            "qodercli",
            "1.2.3",
            "Qoder CLI<NL>Usage:<NL>  qodercli [command]<NL>  index-tool update<NL>Commands:<NL>  update  Update Qoder CLI",
        ),
    ] {
        let sb_conf = TestSandbox::new();
        sb_conf.install_fake_tool(
            bin_name,
            &[
                ("version_stdout", ver_out),
                ("help_stdout", conflicting_help),
                ("update_stdout", "Updated Acme CLI"),
            ],
        );
        let out_conf_list = sb_conf.run_teemiao(&["agents", "list"], &[&sb_conf.bin_dir], &[]);
        assert!(
            !String::from_utf8_lossy(&out_conf_list.stdout).contains("Qoder (qoder)"),
            "conflicting help branding `{conflicting_help}` must not list Qoder: {}",
            String::from_utf8_lossy(&out_conf_list.stdout)
        );
        let out_conf =
            sb_conf.run_teemiao(&["agents", "update", "qoder"], &[&sb_conf.bin_dir], &[]);
        assert_eq!(out_conf.status.code(), Some(1));
        assert!(
            !sb_conf
                .invocations()
                .iter()
                .any(|l| l.contains("|ARGS=update|")),
            "conflicting help branding `{conflicting_help}` must perform zero updater calls: {:?}",
            sb_conf.invocations()
        );
    }

    // 9. Windows package-manager paths (WinGet, Scoop, Chocolatey) and wrapper targets must require manual action
    // and perform zero updater invocations
    for rel_mgr_dir in [
        "AppData/Local/Microsoft/WinGet/Packages/Qoder.QoderCLI_8wekyb3d8bbwe",
        "scoop/apps/qodercli/current",
        "ProgramData/chocolatey/lib/qodercli/tools",
    ] {
        let sb_wm = TestSandbox::new();
        let mgr_dir = sb_wm.root.join(rel_mgr_dir);
        sb_wm.install_fake_tool_in(
            &mgr_dir,
            "qodercli",
            &[
                ("version_stdout", "Qoder CLI 0.9.0"),
                (
                    "help_stdout",
                    "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
                ),
                ("update_stdout", "Qoder CLI is up to date"),
            ],
        );
        let out_wm = sb_wm.run_teemiao(&["agents", "update", "qoder"], &[&mgr_dir], &[]);
        assert_eq!(out_wm.status.code(), Some(1));
        let stdout_wm = String::from_utf8_lossy(&out_wm.stdout);
        assert!(
            stdout_wm.contains("manual action required"),
            "expected manual action required for `{rel_mgr_dir}`: {stdout_wm}"
        );
        assert!(
            !sb_wm
                .invocations()
                .iter()
                .any(|l| l.contains("|ARGS=update|")),
            "managed directory `{rel_mgr_dir}` must perform zero updater invocations: {:?}",
            sb_wm.invocations()
        );
    }

    // 10. SemVer prerelease `0.9.0-preview.1` (both in `--version` and in `package.json` when `--version` is unreadable)
    // requires manual action and performs zero `npm` update calls
    for probe_ver in ["qodercli 0.9.0-preview.1", "qodercli"] {
        let sb_pre = TestSandbox::new();
        let pre_prefix = sb_pre.root.join("npm_pre");
        let pre_bin = pre_prefix.join("bin");
        let pre_pkg = pre_prefix
            .join("lib")
            .join("node_modules")
            .join("@qoder-ai")
            .join("qodercli");
        let pre_pkg_bin = pre_pkg.join("bin");
        fs::create_dir_all(&pre_bin).unwrap();
        fs::create_dir_all(&pre_pkg_bin).unwrap();
        fs::write(
            pre_pkg.join("package.json"),
            r#"{"name":"@qoder-ai/qodercli","version":"0.9.0-preview.1"}"#,
        )
        .unwrap();
        let (real_pre_qodercli, _) = sb_pre.install_fake_tool_in(
            &pre_pkg_bin,
            "qodercli",
            &[
                ("version_stdout", probe_ver),
                (
                    "help_stdout",
                    "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
                ),
            ],
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real_pre_qodercli, pre_bin.join("qodercli")).unwrap();
        }
        #[cfg(not(unix))]
        {
            fs::write(
                pre_bin.join("qodercli.cmd"),
                format!("@echo off\r\n\"{}\" %*\r\n", real_pre_qodercli.display()),
            )
            .unwrap();
        }
        sb_pre.install_fake_tool_in(&pre_bin, "npm", &[("update_stdout", "added 1 package")]);
        let out_pre = sb_pre.run_teemiao(&["agents", "update", "qoder"], &[&pre_bin], &[]);
        assert_eq!(out_pre.status.code(), Some(1));
        let stdout_pre = String::from_utf8_lossy(&out_pre.stdout);
        assert!(
            stdout_pre.contains("manual action required") && stdout_pre.contains("prerelease"),
            "expected prerelease manual action for `{probe_ver}`: {stdout_pre}"
        );
        assert!(
            !sb_pre
                .invocations()
                .iter()
                .any(|l| l.starts_with("TOOL=npm|") || l.contains("|ARGS=update|")),
            "prerelease `{probe_ver}` must perform zero npm/native update calls: {:?}",
            sb_pre.invocations()
        );
    }
}

fn run_teemiao_with_clean_locale_env(
    sb: &TestSandbox,
    args: &[&str],
    locale_env: &[(&str, &str)],
    cwd: Option<&Path>,
) -> std::process::Output {
    let teemiao_bin = env!("CARGO_BIN_EXE_teemiao");
    let joined_path = std::env::join_paths([&sb.bin_dir]).expect("join sandbox PATH");
    let mut cmd = std::process::Command::new(teemiao_bin);
    cmd.args(args)
        .current_dir(cwd.unwrap_or(&sb.root))
        .env_clear()
        .env("PATH", joined_path)
        .env("HOME", &sb.home)
        .env("USERPROFILE", &sb.home)
        .env("LOCALAPPDATA", sb.home.join("AppData").join("Local"))
        .env("TEEMIAO_TEST_LOG_FILE", &sb.log_file);

    if cfg!(windows) {
        if let Some(sys_root) = std::env::var_os("SystemRoot") {
            cmd.env("SystemRoot", sys_root);
        }
        cmd.env("PATHEXT", ".COM;.EXE;.BAT;.CMD;.PS1");
    }

    for &(k, v) in locale_env {
        cmd.env(k, v);
    }

    cmd.output().expect("spawn isolated teemiao process")
}

#[test]
fn cli_localization_parity_help_messages_precedence_and_fallback() {
    let sb_empty = TestSandbox::new();
    let en_env = [("LC_ALL", "en_US.UTF-8"), ("LANG", "en_US.UTF-8")];
    let zh_env = [("LC_ALL", "zh_CN.UTF-8"), ("LANG", "zh_CN.UTF-8")];

    // 1. Root, build-info, config-template, and agents help parity (including multiline, backticks, and quotes)
    let root_help_en = run_teemiao_with_clean_locale_env(&sb_empty, &["--help"], &en_env, None);
    let root_help_zh = run_teemiao_with_clean_locale_env(&sb_empty, &["--help"], &zh_env, None);
    assert_eq!(root_help_en.status.code(), Some(0));
    assert_eq!(root_help_zh.status.code(), Some(0));
    let root_en_str = String::from_utf8(root_help_en.stdout).unwrap();
    let root_zh_str = String::from_utf8(root_help_zh.stdout).unwrap();
    assert!(
        root_en_str
            .contains("Teemiao is a set of convenient tools for building other applications.")
            && root_en_str.contains("Generate build information metadata in JSON format.")
            && root_en_str.contains("Generate configuration from template (not implemented).")
            && root_en_str.contains("Discover and update installed AI coding-agent CLIs on PATH."),
        "unexpected English root --help: {root_en_str}"
    );
    assert!(
        root_zh_str.contains("Teemiao 是一组用于构建其他应用程序的便捷工具。")
            && root_zh_str.contains("以 JSON 格式生成构建信息元数据。")
            && root_zh_str.contains("从模板生成配置（尚未实现）。")
            && root_zh_str.contains("发现并更新 PATH 中已安装的 AI 编程代理命令行工具。"),
        "unexpected zh-CN root --help: {root_zh_str}"
    );

    let bi_help_en =
        run_teemiao_with_clean_locale_env(&sb_empty, &["build-info", "--help"], &en_env, None);
    let bi_help_zh =
        run_teemiao_with_clean_locale_env(&sb_empty, &["build-info", "--help"], &zh_env, None);
    let bi_en_str = String::from_utf8(bi_help_en.stdout).unwrap();
    let bi_zh_str = String::from_utf8(bi_help_zh.stdout).unwrap();
    assert!(
        bi_en_str.contains("shortened HEAD git revision (`revision`) and UTC Unix timestamp in seconds (`build_time`).")
            && bi_en_str.contains("If not specified, defaults to `./build_info.json` in the current working directory."),
        "unexpected English build-info --help: {bi_en_str}"
    );
    assert!(
        bi_zh_str.contains(
            "缩短后的 HEAD git 修订版本（`revision`）和 UTC Unix 秒级时间戳（`build_time`）。"
        ) && bi_zh_str.contains("如果未指定，默认为当前工作目录下的 `./build_info.json`。"),
        "unexpected zh-CN build-info --help: {bi_zh_str}"
    );

    let agents_help_en =
        run_teemiao_with_clean_locale_env(&sb_empty, &["agents", "--help"], &en_env, None);
    let agents_help_zh =
        run_teemiao_with_clean_locale_env(&sb_empty, &["agents", "--help"], &zh_env, None);
    let agents_en_str = String::from_utf8(agents_help_en.stdout).unwrap();
    let agents_zh_str = String::from_utf8(agents_help_zh.stdout).unwrap();
    assert!(
        agents_en_str.contains(
            "Discover and update active command-line coding agents (Codex, Antigravity, OpenCode, Kimi, Cursor, Claude Code, and Qoder) resolved from the current process PATH.\n\nOnly active CLI installations on PATH are inspected;"
        ),
        "expected multiline English agents --help: {agents_en_str}"
    );
    assert!(
        agents_zh_str.contains(
            "从当前进程的 PATH 中发现并更新活动的命令行编程代理（Codex、Antigravity、OpenCode、Kimi、Cursor、Claude Code 和 Qoder）。\n\n仅检查 PATH 中的命令行安装，不包含桌面应用和编辑器扩展。"
        ),
        "expected multiline zh-CN agents --help: {agents_zh_str}"
    );

    let update_help_en = run_teemiao_with_clean_locale_env(
        &sb_empty,
        &["agents", "update", "--help"],
        &en_env,
        None,
    );
    let update_help_zh = run_teemiao_with_clean_locale_env(
        &sb_empty,
        &["agents", "update", "--help"],
        &zh_env,
        None,
    );
    let update_en_str = String::from_utf8(update_help_en.stdout).unwrap();
    let update_zh_str = String::from_utf8(update_help_zh.stdout).unwrap();
    assert!(
        update_en_str.contains("'--- [<index>/<total>] <Name> (<id>) ---'")
            && update_en_str.contains("'Update summary'")
            && update_en_str.contains("\n\nUse --dry-run to preview the selected update command"),
        "unexpected English agents update --help: {update_en_str}"
    );
    assert!(
        update_zh_str.contains("'--- [<index>/<total>] <Name> (<id>) ---'")
            && update_zh_str.contains("“更新摘要”")
            && update_zh_str.contains("\n\n使用 --dry-run 可预览选定的更新命令")
            && update_zh_str.contains("可选的要更新的代理标识符或别名。")
            && update_zh_str.contains("运行发现和只读归属检查"),
        "unexpected zh-CN agents update --help: {update_zh_str}"
    );

    // 2. Locale precedence (`LC_ALL` > `LC_MESSAGES` > `LANG`) and fallback (`fr_FR`, `zh_TW`, `C`, `POSIX`, unset)
    for fallback_env in [
        &[][..],
        &[("LC_ALL", "C")][..],
        &[("LC_ALL", "POSIX.UTF-8")][..],
        &[("LC_ALL", "fr_FR.UTF-8"), ("LANG", "fr_FR.UTF-8")][..],
        &[("LC_ALL", "zh_TW.UTF-8"), ("LANG", "zh_TW.UTF-8")][..],
        &[
            ("LC_ALL", ""),
            ("LC_MESSAGES", "zh_CN.UTF-8"),
            ("LANG", "zh_CN.UTF-8"),
        ][..],
    ] {
        let fb_root = run_teemiao_with_clean_locale_env(&sb_empty, &["--help"], fallback_env, None);
        assert_eq!(
            String::from_utf8(fb_root.stdout).unwrap(),
            root_en_str,
            "fallback --help must match English byte-for-byte for env {fallback_env:?}"
        );
        let fb_update = run_teemiao_with_clean_locale_env(
            &sb_empty,
            &["agents", "update", "--help"],
            fallback_env,
            None,
        );
        assert_eq!(
            String::from_utf8(fb_update.stdout).unwrap(),
            update_en_str,
            "fallback agents update --help must match English byte-for-byte for env {fallback_env:?}"
        );
    }

    for zh_precedence_env in [
        &[("LC_MESSAGES", "zh_cn"), ("LANG", "fr_FR.UTF-8")][..],
        &[("LANG", "zh_CN.UTF-8")][..],
    ] {
        let zh_prec =
            run_teemiao_with_clean_locale_env(&sb_empty, &["--help"], zh_precedence_env, None);
        assert_eq!(
            String::from_utf8(zh_prec.stdout).unwrap(),
            root_zh_str,
            "zh-CN precedence --help mismatch for env {zh_precedence_env:?}"
        );
    }

    // 3. Empty PATH, not-installed error, build-info error, and fake-agent list/dry-run parity
    let empty_list_en =
        run_teemiao_with_clean_locale_env(&sb_empty, &["agents", "list"], &en_env, None);
    let empty_list_zh =
        run_teemiao_with_clean_locale_env(&sb_empty, &["agents", "list"], &zh_env, None);
    assert_eq!(
        String::from_utf8(empty_list_en.stdout).unwrap(),
        "No supported AI coding-agent CLIs were found on PATH.\n"
    );
    assert_eq!(
        String::from_utf8(empty_list_zh.stdout).unwrap(),
        "在 PATH 中未找到受支持的 AI 编程代理 CLI。\n"
    );

    let missing_en = run_teemiao_with_clean_locale_env(
        &sb_empty,
        &["agents", "update", "codex", "--dry-run"],
        &en_env,
        None,
    );
    let missing_zh = run_teemiao_with_clean_locale_env(
        &sb_empty,
        &["agents", "update", "codex", "--dry-run"],
        &zh_env,
        None,
    );
    assert_eq!(missing_en.status.code(), Some(1));
    assert_eq!(missing_zh.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(missing_en.stderr).unwrap(),
        "Error: agent 'Codex' (codex) is not installed on PATH. Teemiao only updates existing installations on PATH.\n"
    );
    assert_eq!(
        String::from_utf8(missing_zh.stderr).unwrap(),
        "错误：代理 'Codex' (codex) 未安装在 PATH 中。Teemiao 仅更新 PATH 中已有的安装。\n"
    );

    let non_git_dir = sb_empty.root.join("not_a_git_repo");
    fs::create_dir_all(&non_git_dir).unwrap();
    let bi_err_en = run_teemiao_with_clean_locale_env(
        &sb_empty,
        &["build-info", "out.json"],
        &en_env,
        Some(&non_git_dir),
    );
    let bi_err_zh = run_teemiao_with_clean_locale_env(
        &sb_empty,
        &["build-info", "out.json"],
        &zh_env,
        Some(&non_git_dir),
    );
    assert_eq!(bi_err_en.status.code(), Some(0));
    assert_eq!(bi_err_zh.status.code(), Some(0));
    assert!(
        String::from_utf8(bi_err_en.stderr)
            .unwrap()
            .starts_with("Error generating build info: Failed to open repository:"),
        "unexpected English build-info error"
    );
    assert!(
        String::from_utf8(bi_err_zh.stderr)
            .unwrap()
            .starts_with("生成构建信息时出错：Failed to open repository:"),
        "unexpected zh-CN build-info error"
    );

    let sb_fake = TestSandbox::new();
    let (agy_path, _) =
        sb_fake.install_fake_tool("agy", &[("version_stdout", "Antigravity CLI 1.4.2")]);
    let list_en = run_teemiao_with_clean_locale_env(&sb_fake, &["agents", "list"], &en_env, None);
    let list_zh = run_teemiao_with_clean_locale_env(&sb_fake, &["agents", "list"], &zh_env, None);
    let list_fr = run_teemiao_with_clean_locale_env(
        &sb_fake,
        &["agents", "list"],
        &[("LC_ALL", "fr_FR.UTF-8")],
        None,
    );
    let list_en_out = String::from_utf8(list_en.stdout).unwrap();
    let list_zh_out = String::from_utf8(list_zh.stdout).unwrap();
    assert_eq!(
        list_en_out,
        format!(
            "Antigravity (antigravity) | version: 1.4.2 | path: {} | update: native (agy update)\n",
            agy_path.display()
        )
    );
    assert_eq!(
        list_zh_out,
        format!(
            "Antigravity (antigravity) | 版本: 1.4.2 | 路径: {} | 更新方式: 原生自更新 (agy update)\n",
            agy_path.display()
        )
    );
    assert_eq!(String::from_utf8(list_fr.stdout).unwrap(), list_en_out);
}

#[cfg(unix)]
fn write_executable_dispatcher(path: &std::path::Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

#[cfg(unix)]
#[test]
fn qoder_entry_dispatcher_wrapped_commander_alias_and_updater_cli() {
    let dispatcher_script = concat!(
        "#!/bin/sh\n",
        "# Qoder CLI dispatcher\n",
        "CLI_BIN=\"$(command -v qodercli 2>/dev/null || true)\"\n",
        "if [ -z \"$CLI_BIN\" ] && [ -x \"$HOME/.local/bin/qodercli\" ]; then CLI_BIN=\"$HOME/.local/bin/qodercli\"; fi\n",
        "if [ -z \"$CLI_BIN\" ] && [ -x \"$HOME/.qoder/bin/qodercli/qodercli\" ]; then CLI_BIN=\"$HOME/.qoder/bin/qodercli/qodercli\"; fi\n",
        "if [ -z \"$CLI_BIN\" ]; then echo \"Qoder CLI is not installed.\" >&2; exit 127; fi\n",
        "exec \"$CLI_BIN\" \"$@\"\n",
    );

    // 1. Qian Qiao's exact PATH setup: only `~/.qoder/entry` on PATH, wrapped Commander `--help`,
    //    standalone `~/.qoder/bin/qodercli/qodercli` (`curl-bash`), `qoder-cli` alias, and
    //    `"Successfully updated Qoder CLI from ... to ..."` updater confirmation.
    let sb = TestSandbox::new();
    let entry_dir = sb.home.join(".qoder").join("entry");
    let entry_qoder = entry_dir.join("qoder");
    write_executable_dispatcher(&entry_qoder, dispatcher_script);

    let qoder_bin_dir = sb.home.join(".qoder").join("bin").join("qodercli");
    fs::create_dir_all(&qoder_bin_dir).unwrap();
    let (qoder_bin, _) = sb.install_fake_tool_in(
        &qoder_bin_dir,
        "qodercli",
        &[
            ("version_stdout", "1.1.87"),
            (
                "help_stdout",
                "Qoder CLI - Defaults to interactive mode. Use -p/--print for non-interactive<NL>output.<NL><NL>Usage: qodercli [options] [command]<NL><NL>Commands:<NL>  update         Update Qoder CLI",
            ),
            (
                "update_stdout",
                "Successfully updated Qoder CLI from 1.1.87 to 1.1.88.",
            ),
            ("next_version_stdout", "1.1.88"),
        ],
    );
    fs::write(
        qoder_bin_dir.join(".qodercli-install-resource"),
        "curl-bash\n",
    )
    .unwrap();

    let list_out = sb.run_teemiao(&["agents", "list"], &[&entry_dir], &[]);
    assert_eq!(list_out.status.code(), Some(0));
    let list_stdout = String::from_utf8_lossy(&list_out.stdout);
    assert!(
        list_stdout.contains("Qoder (qoder) | version: 1.1.87")
            && list_stdout.contains(&format!("path: {}", entry_qoder.display()))
            && list_stdout.contains("update: native (qodercli update)"),
        "unexpected list output: {list_stdout}"
    );

    let dry_out = sb.run_teemiao(
        &["agents", "update", "QoDeR-ClI", "--dry-run"],
        &[&entry_dir],
        &[],
    );
    assert_eq!(dry_out.status.code(), Some(0));
    let dry_stdout = String::from_utf8_lossy(&dry_out.stdout);
    assert!(
        dry_stdout.contains("dry-run") && dry_stdout.contains("qodercli update"),
        "unexpected dry-run output: {dry_stdout}"
    );
    assert!(
        !sb.invocations()
            .iter()
            .any(|line| line.contains("|ARGS=update|")),
        "dry-run must not execute qodercli update: {:?}",
        sb.invocations()
    );

    let upd_out = sb.run_teemiao(&["agents", "update", "qoder-cli"], &[&entry_dir], &[]);
    assert_eq!(
        upd_out.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&upd_out.stdout),
        String::from_utf8_lossy(&upd_out.stderr)
    );
    let upd_stdout = String::from_utf8_lossy(&upd_out.stdout);
    assert!(
        upd_stdout.contains("updated") && upd_stdout.contains("1.1.87 -> 1.1.88"),
        "unexpected update output: {upd_stdout}"
    );
    assert!(
        sb.invocations()
            .iter()
            .any(|line| line.starts_with("TOOL=qodercli|ARGS=update|")),
        "expected verified qodercli binary to execute directly: {:?}",
        sb.invocations()
    );

    // 2. Initial unknown version (`None`) + `"Successfully updated Qoder CLI from ... to ..."` vs
    //    `"Unable to determine installation method."`.
    let sb_unk = TestSandbox::new();
    let entry_unk = sb_unk.home.join(".qoder").join("entry");
    write_executable_dispatcher(&entry_unk.join("qoder"), dispatcher_script);
    let bin_unk = sb_unk.home.join(".qoder").join("bin").join("qodercli");
    fs::create_dir_all(&bin_unk).unwrap();
    sb_unk.install_fake_tool_in(
        &bin_unk,
        "qodercli",
        &[
            ("version_stdout", "Qoder CLI"),
            (
                "help_stdout",
                "Qoder CLI - Defaults to interactive mode. Use -p/--print for non-interactive<NL>output.<NL><NL>Commands:<NL>  update         Update Qoder CLI",
            ),
            (
                "update_stdout",
                "Successfully updated Qoder CLI from 1.1.87 to 1.1.88.",
            ),
            ("next_version_stdout", "1.1.88"),
        ],
    );
    let upd_unk_ok = sb_unk.run_teemiao(&["agents", "update", "qoder-cli"], &[&entry_unk], &[]);
    assert_eq!(upd_unk_ok.status.code(), Some(0));
    let stdout_unk_ok = String::from_utf8_lossy(&upd_unk_ok.stdout);
    assert!(
        stdout_unk_ok.contains("updated") && stdout_unk_ok.contains("unknown -> 1.1.88"),
        "unexpected unknown->1.1.88 output: {stdout_unk_ok}"
    );

    let sb_unable = TestSandbox::new();
    let entry_unable = sb_unable.home.join(".qoder").join("entry");
    write_executable_dispatcher(&entry_unable.join("qoder"), dispatcher_script);
    let bin_unable = sb_unable.home.join(".qoder").join("bin").join("qodercli");
    fs::create_dir_all(&bin_unable).unwrap();
    sb_unable.install_fake_tool_in(
        &bin_unable,
        "qodercli",
        &[
            ("version_stdout", "Qoder CLI"),
            (
                "help_stdout",
                "Qoder CLI - Defaults to interactive mode.<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
            ("update_stdout", "Unable to determine installation method."),
            ("next_version_stdout", "1.1.88"),
        ],
    );
    let upd_unable =
        sb_unable.run_teemiao(&["agents", "update", "qoder-cli"], &[&entry_unable], &[]);
    assert_eq!(upd_unable.status.code(), Some(1));

    // 3. Precedence (`~/.local/bin/qodercli` beats `~/.qoder/bin/qodercli/qodercli`) &
    //    Homebrew `.qodercli-install-resource == "homebrew-cask"` -> manual action required.
    let local_bin_dir = sb.home.join(".local").join("bin");
    fs::create_dir_all(&local_bin_dir).unwrap();
    sb.install_fake_tool_in(
        &local_bin_dir,
        "qodercli",
        &[
            ("version_stdout", "2.0.0"),
            (
                "help_stdout",
                "Qoder CLI - Defaults to interactive mode.<NL>Commands:<NL>  update  Update Qoder CLI",
            ),
        ],
    );
    let list_local = sb.run_teemiao(&["agents", "list"], &[&entry_dir], &[]);
    let list_local_stdout = String::from_utf8_lossy(&list_local.stdout);
    assert!(
        list_local_stdout.contains("version: 2.0.0")
            && list_local_stdout.contains(&format!("path: {}", entry_qoder.display())),
        "expected ~/.local/bin/qodercli precedence: {list_local_stdout}"
    );

    fs::write(
        local_bin_dir.join(".qodercli-install-resource"),
        "homebrew-cask\n",
    )
    .unwrap();
    let cask_upd = sb.run_teemiao(&["agents", "update", "qoder-cli"], &[&entry_dir], &[]);
    assert_eq!(cask_upd.status.code(), Some(1));
    let cask_stdout = String::from_utf8_lossy(&cask_upd.stdout);
    assert!(
        cask_stdout.contains("manual action required")
            && cask_stdout.contains("brew upgrade --cask qodercli"),
        "expected Homebrew cask manual action: {cask_stdout}"
    );
    let _ = fs::remove_file(local_bin_dir.join(".qodercli-install-resource"));
    let _ = fs::remove_file(local_bin_dir.join("qodercli"));

    // 4. Global npm `@qoder-ai/qodercli` target via `.qoder/entry/qoder` -> `~/.local/bin/qodercli`
    //    must route to NpmGlobal without running `--help`.
    let sb_npm = TestSandbox::new();
    let entry_npm = sb_npm.home.join(".qoder").join("entry");
    write_executable_dispatcher(&entry_npm.join("qoder"), dispatcher_script);
    let npm_prefix = sb_npm.root.join("npm-global");
    let npm_bin = npm_prefix.join("bin");
    let npm_pkg = npm_prefix
        .join("lib")
        .join("node_modules")
        .join("@qoder-ai")
        .join("qodercli");
    let npm_pkg_bin = npm_pkg.join("bin");
    fs::create_dir_all(&npm_bin).unwrap();
    fs::create_dir_all(&npm_pkg_bin).unwrap();
    sb_npm.install_fake_tool_in(&npm_bin, "npm", &[("update_stdout", "added 1 package")]);
    let (pkg_cli, _) =
        sb_npm.install_fake_tool_in(&npm_pkg_bin, "qodercli", &[("version_stdout", "1.1.87")]);
    fs::write(
        npm_pkg.join("package.json"),
        r#"{"name":"@qoder-ai/qodercli","version":"1.1.87","bin":{"qodercli":"bin/qodercli"}}"#,
    )
    .unwrap();
    let npm_local_bin = sb_npm.home.join(".local").join("bin");
    fs::create_dir_all(&npm_local_bin).unwrap();
    std::os::unix::fs::symlink(&pkg_cli, npm_local_bin.join("qodercli")).unwrap();
    let list_npm = sb_npm.run_teemiao(&["agents", "list"], &[&entry_npm, &npm_bin], &[]);
    assert_eq!(list_npm.status.code(), Some(0));
    let list_npm_stdout = String::from_utf8_lossy(&list_npm.stdout);
    assert!(
        list_npm_stdout.contains("update: npm (@qoder-ai/qodercli in "),
        "expected npm ownership via dispatcher: {list_npm_stdout}"
    );
    assert!(
        !sb_npm
            .invocations()
            .iter()
            .any(|line| line.contains("|ARGS=--help|")),
        "npm ownership should not invoke --help: {:?}",
        sb_npm.invocations()
    );

    // 5. IDE-only stub (no qodercli installed), `Qoder IDE.app`, `/usr/share/qoder-ide/`,
    //    `/remote-cli/`, and conflicting banners must not discover Qoder.
    let sb_missing = TestSandbox::new();
    let entry_missing = sb_missing.home.join(".qoder").join("entry");
    write_executable_dispatcher(&entry_missing.join("qoder"), dispatcher_script);
    let list_missing = sb_missing.run_teemiao(&["agents", "list"], &[&entry_missing], &[]);
    assert_eq!(list_missing.status.code(), Some(0));
    let missing_stdout = String::from_utf8_lossy(&list_missing.stdout);
    assert!(
        !missing_stdout.contains("Qoder (qoder)"),
        "missing CLI target must not discover Qoder: {missing_stdout}"
    );

    for subpath in [
        "Qoder IDE.app/Contents/Resources/app/bin",
        "usr/share/qoder-ide/bin",
        ".qoder-server/bin/abc/bin/remote-cli",
    ] {
        let sb_ide = TestSandbox::new();
        let ide_bin = sb_ide.root.join(subpath);
        fs::create_dir_all(&ide_bin).unwrap();
        sb_ide.install_fake_tool_in(
            &ide_bin,
            "qodercli",
            &[
                ("version_stdout", "1.1.87"),
                (
                    "help_stdout",
                    "Qoder CLI<NL>Commands:<NL>  update  Update Qoder CLI",
                ),
            ],
        );
        let ide_list = sb_ide.run_teemiao(&["agents", "list"], &[&ide_bin], &[]);
        assert_eq!(ide_list.status.code(), Some(0));
        assert!(
            !String::from_utf8_lossy(&ide_list.stdout).contains("Qoder (qoder)"),
            "IDE/remote-cli path `{subpath}` must be excluded"
        );
        assert!(
            sb_ide.invocations().is_empty(),
            "IDE/remote-cli path `{subpath}` must be excluded before spawning: {:?}",
            sb_ide.invocations()
        );
    }

    for conflicting_help in [
        "Acme Qoder CLI<NL>Commands:<NL>  update  Update",
        "Qoder CLI<NL>Acme CLI<NL>Commands:<NL>  update  Update",
        "index-tool<NL>Commands:<NL>  update  Update",
    ] {
        let sb_conf = TestSandbox::new();
        sb_conf.install_fake_tool(
            "qodercli",
            &[
                ("version_stdout", "1.1.87"),
                ("help_stdout", conflicting_help),
            ],
        );
        let conf_list = sb_conf.run_teemiao(&["agents", "list"], &[&sb_conf.bin_dir], &[]);
        assert_eq!(conf_list.status.code(), Some(0));
        assert!(
            !String::from_utf8_lossy(&conf_list.stdout).contains("Qoder (qoder)"),
            "conflicting help `{conflicting_help}` must not discover Qoder"
        );
    }

    // 6. `.qodercli-install-resource == "curl-bash"` with unavailable `--help` and initial
    //    `version: None` authorizes `qodercli update` and confirms `unknown -> 1.1.88`.
    let sb_cb_err = TestSandbox::new();
    let entry_cb_err = sb_cb_err.home.join(".qoder").join("entry");
    write_executable_dispatcher(&entry_cb_err.join("qoder"), dispatcher_script);
    let bin_cb_err = sb_cb_err.home.join(".qoder").join("bin").join("qodercli");
    fs::create_dir_all(&bin_cb_err).unwrap();
    sb_cb_err.install_fake_tool_in(
        &bin_cb_err,
        "qodercli",
        &[("version_exit", "2"), ("help_exit", "2")],
    );
    fs::write(bin_cb_err.join(".qodercli-install-resource"), "curl-bash\n").unwrap();
    let cb_list = sb_cb_err.run_teemiao(&["agents", "list"], &[&entry_cb_err], &[]);
    assert_eq!(cb_list.status.code(), Some(0));
    let cb_list_stdout = String::from_utf8_lossy(&cb_list.stdout);
    assert!(
        cb_list_stdout.contains("Qoder (qoder) | version: unknown")
            && cb_list_stdout.contains("update: native (qodercli update)"),
        "expected curl-bash with failed probes to report NativeSelfUpdate: {cb_list_stdout}"
    );
    let cb_dry = sb_cb_err.run_teemiao(
        &["agents", "update", "qoder-cli", "--dry-run"],
        &[&entry_cb_err],
        &[],
    );
    assert_eq!(cb_dry.status.code(), Some(0));

    let sb_cb_up = TestSandbox::new();
    let entry_cb_up = sb_cb_up.home.join(".qoder").join("entry");
    write_executable_dispatcher(&entry_cb_up.join("qoder"), dispatcher_script);
    let bin_cb_up = sb_cb_up.home.join(".qoder").join("bin").join("qodercli");
    fs::create_dir_all(&bin_cb_up).unwrap();
    sb_cb_up.install_fake_tool_in(
        &bin_cb_up,
        "qodercli",
        &[
            ("version_stdout", "Qoder CLI"),
            ("help_exit", "2"),
            (
                "update_stdout",
                "Successfully updated Qoder CLI from 1.1.87 to 1.1.88.",
            ),
            ("next_version_stdout", "Qoder CLI 1.1.88"),
        ],
    );
    fs::write(bin_cb_up.join(".qodercli-install-resource"), "curl-bash\n").unwrap();
    let cb_up = sb_cb_up.run_teemiao(&["agents", "update", "qoder-cli"], &[&entry_cb_up], &[]);
    assert_eq!(cb_up.status.code(), Some(0));
    let cb_up_stdout = String::from_utf8_lossy(&cb_up.stdout);
    assert!(
        cb_up_stdout.contains("updated") && cb_up_stdout.contains("unknown -> 1.1.88"),
        "expected curl-bash unknown -> 1.1.88 update output: {cb_up_stdout}"
    );

    let _ = qoder_bin;
}
