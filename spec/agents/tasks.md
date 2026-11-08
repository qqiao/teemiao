# Tasks: agents subcommand

T001 is approved; implementation may proceed. Each implementation task uses a failing
behavior test, the smallest implementation, and a passing verification run.
Do not alter the specification independently; ask the leader about conflicts.

- [x] **T001 — Human review of the complete design** (Qian Qiao).
  Review `spec.md`, `research.md`, `plan.md`, this checklist, and `CONSTRAINTS.md`.
  Acceptance: explicitly approve implementation or identify changes, especially
  platforms, PATH-only active-install discovery, and release-channel preservation.
  Verification: Qian Qiao replied "LGTM" on 2026-11-08 in QQ-8 comment
  `01a1b7a7-3876-7273-a84b-19ab22057240`. Dependency: none.

- [x] **T002 [US1] — List the six supported CLI agents** (Coder; medium).
  Add the clap group, catalog, executable discovery, bounded version/identity
  probes, and localized output. Keep tests alongside the implementation. Move the
  existing test module below `main()` to resolve baseline Clippy.
  Acceptance: all six fixtures listed; absent/duplicate/desktop/unrelated cases
  handled; hanging/oversized/failed probes do not block or hide other agents.
  Files: `src/main.rs`, `src/agents/mod.rs`, `src/agents/discovery.rs`, both locales.
  Verify: `cargo test --locked agents`, full tests, format, strict Clippy.
  Verification: Passed 12 unit tests in `src/agents/mod.rs` and process tests in
  `tests/agents_cli.rs` covering all 6 agents, `PATH` precedence, symlinks, desktop
  and generic `agent` rejection, and 5s/64 KiB bounded probes with `CI=1` and
  `DISABLE_AUTOUPDATER=1`. Dependency: T001.

- [x] **T003 [US2] — Update verified native self-updating agents** (Coder; medium).
  Add immutable update plans, dry-run, and real process-test fixtures for
  Antigravity, OpenCode, current Kimi, and Cursor. Recognize unsupported legacy
  variants; never start a general agent session as an update fallback.
  Acceptance: exact updater/arguments; dry-run causes zero mutations; failed or
  manual-only output cannot be reported as a successful update.
  Files: `src/agents/mod.rs`, `src/agents/update.rs`, `tests/agents_cli.rs`, both locales.
  Verify: focused unit tests and `cargo test --locked --test agents_cli` with fake
  tools, including Windows shim and path-metacharacter cases.
  Verification: Passed unit and process tests for `agy update`, `opencode upgrade`,
  `kimi upgrade --yes`, `agent update`, and `cursor-agent update`, legacy Kimi
  rejection, zero-exit manual-only and unverified outcomes, and Windows `.cmd`
  metacharacter rejection. Dependency: T002.

- [x] **T004 [US2] — Update the active Codex installation** (Coder; medium).
  Implement proven npm/Homebrew ownership and standalone installer selection.
  Add installer download/target validation behind a fakeable boundary; inspect
  both official platform installers without executing them during development.
  Acceptance: manager/prefix and native destination remain unchanged; partial
  downloads and ambiguous ownership launch no installer; post-update discovery
  selects the active updated launcher. No implicit new installation.
  Files: discovery/update modules, process tests, and Cargo manifest/lockfile only
  if a small dependency is justified in the implementation review.
  Verify: Codex ownership, prefix, download-failure, target-mismatch, and version
  verification tests; full tests and strict Clippy.
  Verification: Inspected official `install.sh` and `install.ps1` as text without
  executing them; verified npm (`--prefix`), Homebrew cask (`codex`), and standalone
  installer (`CODEX_NON_INTERACTIVE=1`, `CODEX_INSTALL_DIR`, `CODEX_HOME`, private
  temp dir, header/marker validation) with positive and negative tests; zero new
  Cargo dependencies added. Dependency: T003.

- [x] **T005 [US2] — Update Claude Code through its actual owner** (Coder; medium).
  Support native, npm, exact Homebrew cask, and proven WinGet installations.
  Preserve channels and report other managed installs as manual action required.
  Acceptance: the matching manager alone runs; manager detection is tied to the
  selected executable; managed no-op success messages cannot mask failure.
  Files: discovery/update modules, process tests, both locales.
  Verify: native/npm/Homebrew/WinGet fixtures plus wrong-owner and unavailable
  manager tests, including channel-specific casks.
  Verification: Passed unit and process tests for native `claude update`, npm
  `@anthropic-ai/claude-code@latest`, Homebrew channel cask `claude-code@latest`,
  WinGet `Anthropic.ClaudeCode`, missing manager refusal, system-managed refusal,
  and managed no-op output detection. Dependency: T004.

- [x] **T006 [US3] — Finish bulk updates and outcome reporting** (Coder; medium).
  Run the complete detected set sequentially; produce version/result summaries,
  correct exit codes, and cancellation handling. Cover all single/all dry-runs.
  Acceptance: one failure does not skip later tools; no absent tool is installed;
  interruptions stop later updates; unverified/manual results exit nonzero.
  Files: command/update modules, process tests, both locales.
  Verify: empty/all/single/partial-failure/zero-exit-manual/cancel/version-change
  process tests and full delivery checks.
  Verification: Passed unit and process tests for `--dry-run` (single and bulk),
  empty `PATH` no-op (`0`), partial-failure continuation (`1`), cancellation stop
  (`130`), and `en`/`zh-CN` localized output. Dependency: T005.

- [x] **T007 — Document and validate platform support** (Coder; medium).
  Update README and the existing CI workflow with Linux/macOS/Windows tests.
  Add minimal shared fixtures if required; keep real installers out of all tests.
  Acceptance: examples and limits match behavior; both locales include new keys;
  CI exercises supported executable and ownership paths without external agents.
  Files: `README.md`, `.github/workflows/rust.yml`, `tests/agents_cli.rs`, optional
  `tests/support/mod.rs`, `spec/agents/tasks.md` for checked-off evidence only.
  Verify: all commands in `plan.md`; record local platform and outstanding CI.
  Run the four mandated self-reviews and report findings/resolutions.
  Verification: Updated `README.md`, `.github/workflows/rust.yml`,
  `tests/agents_cli.rs`, `tests/support/mod.rs`, and both locales (`en`, `zh-CN`);
  verified all `plan.md` commands locally on macOS (`darwin`/`aarch64`) and
  completed all 4 self-reviews (`code-review-and-quality`, `code-simplification`,
  `security-and-hardening`, `performance-optimization`). Passed 37 unit + 25
  integration tests (62 total), formatting, strict Clippy for host plus
  `x86_64-pc-windows-gnu` and `x86_64-unknown-linux-gnu`, and validated the
  Linux/macOS/Windows GitHub Actions matrix on PR #25 (including Windows `.cmd`
  npm shim parsing, wrapper variable-binding validation, suspended Windows
  process creation followed by Job Object assignment before resumption,
  descendant termination across piped output, and cross-platform
  non-executable candidate filtering).
  Dependency: T006.

- [x] **T008 — Independent leader review and GitHub PR** (Leader/Coder).
  Leader reviews code, test quality, security, and requirements coverage. Coder
  fixes findings under the same standards; repeat until the leader accepts.
  Open one PR with the requested description, specification links, validation,
  limitations, and issue reference; report the URL on QQ-8. Do not merge.
  Acceptance: no unresolved material findings; exact reviewed commit identified;
  clear evidence for each success criterion. Dependency: T007.
  Verification: Leader independently accepted implementation `07a2ff4` on
  2026-11-08 after reviewing R4/R6-A and R4/R6-B, running all 62 local tests and
  delivery checks, and inspecting native CI run `42466380585` (62/62 on each
  platform). Inverting the job-membership guard fails the deterministic cleanup
  test; restoring it passes. Final review removed the unused Windows PID field
  instead of suppressing its lint; host and Windows-target strict Clippy, all
  62 local tests, formatting, build, and patch checks pass after that adjustment.
  PR #25 targets `main` and records the final revision and current CI evidence:
  https://github.com/qqiao/teemiao/pull/25. No unresolved material findings.

- [ ] **T009 — Human review of the final delivery** (Qian Qiao).
  Acceptance: leader-validated PR delivered and issue moved to `in_review`;
  address human feedback before requesting closure. `done` remains human-owned.
  Verification: PR and review discussion on QQ-8. Dependency: T008.
