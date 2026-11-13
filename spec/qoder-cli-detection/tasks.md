# Tasks: Qoder CLI dispatcher detection, alias support, and patch 0.2.4

**Issue:** QQ-12. Leader owns `spec/qoder-cli-detection/`; coders implement approved tasks on isolated branches from `feat/qq-12-qoder-cli` and submit branch + SHA for Leader review and merge.

---

## Task T001: Approve specification, plan, and task breakdown

**Owner:** Qian Qiao (Reviewer)

**Description:** Review [`spec.md`](spec.md), [`plan.md`](plan.md), and this task breakdown for requirements accuracy, CLI/internal contracts, acceptance criteria, task sizing, and parallel dependency structure.

**Acceptance criteria:**
- [x] Explicit approval recorded on `QQ-12` (or requested revisions addressed and re-approved) before any coder task is dispatched.

**Verification:**
- [x] Approval comment (`01a1d430-db07-70ed-a981-4c203afc2b29`) and approved spec commit SHA (`b4fb6b3c6107d9f36ff1ee00c7b4e2006209bf9e`) recorded on `QQ-12`.

**Dependencies:** None

**Files likely touched:**
- `spec/qoder-cli-detection/spec.md`
- `spec/qoder-cli-detection/plan.md`
- `spec/qoder-cli-detection/tasks.md`

**Estimated scope:** Small (review only)

---

## Task T002: Fix Qoder CLI discovery, dispatcher ownership, `qoder-cli` alias, and update matching

**Owner:** Coder (eligible in Wave 1 — parallel with `T003`)

**Description:** Using `test-driven-development`, `security-and-hardening`, and `performance-optimization`, fix Qoder CLI discovery for soft-wrapped multi-line `--help` preambles, implement safe static target resolution and ownership verification for Qoder's `.qoder/entry` command dispatchers (Unix & Windows) and `.qodercli-install-resource` markers, exclude IDE-only stubs (`"Qoder CLI is not installed"` / exit `127`) and `Qoder IDE.app` / Linux IDE / `remote-cli` paths, add the `qoder-cli` alias to `AgentId::Qoder`, and recognize `"Successfully updated Qoder CLI from <old> to <new>."` in `updater_output_confirms_updated`. Do **not** modify `locales/`, `docs/`, `README.md`, `Cargo.toml`, or the `"teemiao 0.2.3"` version assertion in `tests/agents_cli.rs` (owned by `T003`/`T004`).

**Acceptance criteria:**
- [x] `teemiao agents list` and `teemiao agents update {qoder,qodercli,qoder-cli}` detect Qoder CLI `1.1.x` when `--version` prints bare `1.1.88` and `--help` wraps `"Qoder CLI - Defaults to interactive mode. Use -p/--print for non-interactive\noutput."` across lines, while still rejecting `Acme Qoder CLI`, `Acme CLI`, `Qoder CLI\nAcme CLI`, and `index-tool`.
- [x] `.qoder/entry/qoder` (Unix) and `.qoder\entry\qoder.cmd` + `qoder-dispatcher.ps1` (Windows) resolve their underlying `qodercli` target in Qoder's exact order without executing shell/PowerShell code: standalone native targets yield `UpdateOwner::NativeSelfUpdate` invoking the verified `qodercli` binary (`qodercli update`); proven npm `@qoder-ai/qodercli` targets yield `UpdateOwner::NpmGlobal`; Homebrew (`.qodercli-install-resource` == `"homebrew-cask"` or Caskroom/Cellar), prerelease npm, foreign managers, and unverified `QODER_CLI_BIN` / wrapper targets yield `UpdateOwner::ManualActionRequired`; missing CLI targets / `"Qoder CLI is not installed"` (exit `127`) and `Qoder IDE.app` / `/usr/share/qoder-ide/` / `/remote-cli/` return `None` (absent).
- [x] `updater_output_confirms_updated` recognizes `"Successfully updated Qoder CLI from <old> to <new>."` when initial version was `None`, while preserving `"Unable to determine installation method."` and incomplete-output rejections.

**Verification:**
- [x] Focused RED -> GREEN unit and integration tests pass: `cargo test --locked agents` and `cargo test --locked --test agents_cli` (verified on `d560b212621a84a159563bfcd66c73d46afe10f2` and merged at `782b75d67cb4bbe3f8649eee927a9cc6d720ce9c`)
- [x] Full suite, lint, and formatting pass: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked && cargo build --locked`
- [x] Security and performance checks documented (bounded wrapper/marker file reads, zero extra subprocess probes when npm ownership is already proven, batch metacharacter validation preserved).

**Dependencies:** `T001`

**Files likely touched:**
- `src/agents/mod.rs`
- `src/agents/discovery.rs`
- `src/agents/update.rs`
- `tests/agents_cli.rs`
- `tests/support/mod.rs` (only if fixture helpers are needed)

**Estimated scope:** Medium (4–5 files)

---

## Task T003: Update localized CLI help catalogs and agent reference documentation

**Owner:** Coder (eligible in Wave 1 — parallel with `T002`)

**Description:** Update English and Simplified Chinese CLI help strings (`locales/en.yml` and `locales/zh-CN.yml`) and `docs/agents.md` to document the `qoder-cli` alias alongside `qoder`/`qodercli`, Qoder's `.qoder/entry` command-dispatcher discovery and underlying `qodercli` ownership resolution, `.qodercli-install-resource` / Homebrew / npm / standalone boundaries, and IDE-only stub exclusion. Do **not** touch `src/`, `tests/`, `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `README.md`, or `docs/cli.md`.

**Acceptance criteria:**
- [x] `locales/en.yml` and `locales/zh-CN.yml` list `qoder`/`qodercli`/`qoder-cli` consistently in `agents.update.agent.help` and `agents.update.agent.long_help` while maintaining full localization parity.
- [x] `docs/agents.md` accurately documents `qoder-cli` alias support, `.qoder/entry/qoder` (and Windows `.qoder\entry\qoder.cmd`) dispatcher resolution, direct underlying `qodercli update` execution for standalone installs, manual-action boundaries, and troubleshooting when only Qoder IDE's dispatcher stub is present.
- [x] All relative Markdown links and anchors in `docs/agents.md` remain valid.

**Verification:**
- [x] Tests pass (including `src/main.rs` localization parity tests and `tests/agents_cli.rs`): `cargo test --locked` (verified on `d794c52702a88ab36dcb425c88a71c21d2e0178a` and merged at `390b68a4c5cfdee1c3471f261f67011a5b799a4f`)
- [x] Build, lint, docs, and formatting pass: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps && git diff --check`

**Dependencies:** `T001`

**Files likely touched:**
- `locales/en.yml`
- `locales/zh-CN.yml`
- `docs/agents.md`

**Estimated scope:** Small/Medium (3 files)

---

## Checkpoint: After Tasks T002–T003 (Wave 1 integration)
- [x] Leader independently reviews and verifies `T002` and `T003` commits with `code-review-and-quality` and `doubt-driven-development`
- [x] Both branches cleanly merged into `feat/qq-12-qoder-cli` and coder branches deleted
- [x] Combined `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked` pass on `feat/qq-12-qoder-cli`

---

## Task T004: Prepare patch `0.2.4`, update release docs/changelog, and verify packaging

**Owner:** Coder (Wave 2 — sequential after Wave 1 checkpoint)

**Description:** Bump the root `teemiao` package version from `0.2.3` to `0.2.4` in `Cargo.toml` and `Cargo.lock` (preserving all dependency versions), add the `## [0.2.4] — Unreleased (Prepared)` section in `CHANGELOG.md`, align `README.md` (`qoder-cli` alias + `0.2.4` changelog reference) and `docs/cli.md` (`teemiao 0.2.4` output example), and update `tests/agents_cli.rs` to assert `teemiao 0.2.4` and `qoder-cli` in `agents update --help`.

**Acceptance criteria:**
- [x] `Cargo.toml`, root `Cargo.lock` package entry, and `teemiao --version` all report `0.2.4`; `git diff Cargo.lock` modifies only the root `teemiao` version line.
- [x] `CHANGELOG.md`, `README.md`, and `docs/cli.md` accurately describe `0.2.4` and `qoder-cli` without altering historical `0.2.2`/`0.2.3` records.
- [x] `tests/agents_cli.rs` asserts `teemiao 0.2.4` on `--version` and `qoder-cli` visibility in `teemiao agents update --help`.

**Verification:**
- [x] Full spec verification suite passes on a clean committed tree (verified on `1c420359c15014f6bded3bc2bd9f164e24436ad5` and merged at `87293edd29c0222bde572786542b4a0a2e155c8f`):
  `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked && cargo build --locked && RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps && cargo metadata --locked --no-deps --format-version 1 && cargo run --locked -- --version && cargo run --locked -- agents update --help && cargo package --list --locked && cargo publish --dry-run --locked && git diff --check`

**Dependencies:** `T002`, `T003` (Wave 1 checkpoint)

**Files likely touched:**
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `docs/cli.md`
- `tests/agents_cli.rs`

**Estimated scope:** Medium (6 tightly coupled version/release files)

---

## Task T005: Independent Leader verification and GitHub PR delivery

**Owner:** Leader

**Description:** Review and verify `T004`, merge into `feat/qq-12-qoder-cli`, delete the coder branch, run `doubt-driven-development` end-to-end verification across the integrated feature on `feat/qq-12-qoder-cli`, push to `origin/feat/qq-12-qoder-cli`, and open one GitHub PR targeting `main` (`QQ-12: ...`).

**Acceptance criteria:**
- [x] All acceptance criteria in `spec.md` and `CONSTRAINTS.md` verified independently on the final `feat/qq-12-qoder-cli` commit.
- [x] GitHub PR opened against `main` (without merging, tagging, or publishing) and reported on `QQ-12`; issue moved to `in_review`.

**Verification:**
- [x] Complete `spec.md` command list + `cargo audit --deny warnings` (if installed) + `gh pr view` check.

**Dependencies:** `T004`

**Estimated scope:** Small (verification + PR creation)

---

## Task T007: Address PR #29 Copilot review findings and Windows test portability

**Owner:** Coder (Wave 3 — single cohesive assignment because all 5 findings touch coupled discovery/dispatcher functions in `src/agents/discovery.rs` and Qoder unit tests in `src/agents/mod.rs`)

**Description:** Using `test-driven-development`, `security-and-hardening`, and `performance-optimization`, resolve all five PR #29 Copilot review findings without altering external CLI or internal `spec.md` contracts:
1. **Structural non-comment dispatcher verification (`src/agents/discovery.rs:689–740`, PR thread `PRRT_kwDOM79GKM63DK7C`):** Harden `verify_unix_qoder_dispatcher` and `verify_windows_qoder_dispatcher` so incidental marker substrings inside comments (`#`, `REM`, `@REM`, `::`) or `echo`/`printf`/`Write-Host` output cannot classify an arbitrary user script under `.qoder*/entry` as Qoder's official dispatcher:
   - **Unix (`verify_unix_qoder_dispatcher`):** Require `#!` shebang, `extract_verified_wrapper_exec_target(script_path).is_none()`, at least one non-comment, non-`echo`/`printf` line resolving `qodercli` (`command -v qodercli`, `type -p qodercli`, `/.local/bin/qodercli`, `/.qoder/bin/qodercli`, or `/.qoder-cn/bin/qodercli`), and at least one non-comment variable-dispatched argument-forwarding execution line (`exec "$` ... `"$@"`). Must accept both upstream `@qoder-ai/qodercli@1.1.x` `J6s.sh` (`_find_cli` with `type -P qodercli` + `_run_cli` with `exec "$cli" "$@"`) and existing test fixtures (`CLI_BIN="$(command -v qodercli ...)"` + `exec "$CLI_BIN" "$@"`).
   - **Windows (`verify_windows_qoder_dispatcher`):** Strip UTF-8 BOM (`\u{feff}`) and batch comments (`REM` / `@REM` / `::`); require `qoder.cmd` to invoke `powershell` / `powershell.exe` / `pwsh` with `-File` targeting `%~dp0qoder-dispatcher.ps1` (either directly on the invocation line or via `set "DISPATCHER=%~dp0qoder-dispatcher.ps1"` + `-File "%DISPATCHER%"`, matching both upstream `J6s.cmd` and existing test fixtures) and forwarding `%*`. Strip UTF-8 BOM and `#` comments (and `<# ... #>` comment lines) in `qoder-dispatcher.ps1`; require non-comment `qodercli` lookup (`$env:qoder_cli_bin`, `$env:qodercn_cli_bin`, `get-command qodercli`, or `.qoder/bin/qodercli/qodercli.exe` / `.qoder-cn/bin/qodercli/qodercli.exe`) plus non-comment variable call forwarding CLI arguments (`& $` with `@args`, `@argv`, or `$args`, matching both upstream `J6s.ps1` `& $exe @argv` and existing test fixtures `& $cliBin @args`).
2. **Positive compiled-binary header check for external `.qoder*/entry` symlinks (`src/agents/discovery.rs:775–782`, PR thread `PRRT_kwDOM79GKM63DK7P`):** Replace `read_qoder_dispatcher_text(resolved_path).is_none()` in `classify_qoder_entry_dispatcher` with a bounded 4-byte native-binary magic check helper (`is_compiled_native_binary(resolved_path)`: regular file starting with ELF `\x7fELF`, Mach-O 32/64/universal magics `0xFEEDFACE`/`0xCEFAEDFE`/`0xFEEDFACF`/`0xCFFAEDFE`/`0xCAFEBABE`/`0xBEBAFECA`/`0xCAFEBABF`/`0xBFBAFECA`, or Windows PE `MZ`). Oversized (`>8 KiB`) shell scripts, invalid-UTF-8 scripts, empty files, or unreadable symlink targets outside `entry/` must remain `QoderEntryStatus::Invalid` with zero spawned probes.
3. **Cross-platform Windows CI test gating (`src/agents/mod.rs:5547–5796`, PR thread `PRRT_kwDOM79GKM63DK7h`):** Gate the Unix-symlink-dependent subcases (Steps 7–10 in `qoder_entry_dispatcher_unix_precedence_and_target_ownership` and any new Unix-symlink tests) with `#[cfg(unix)]` (with zero unused-variable/import warnings under both `cfg(unix)` and `cfg(not(unix))`) so `Rust/build (windows-latest)` passes cleanly.
4. **Reject `--help` exit code `127` (`src/agents/discovery.rs:1104–1110`, PR thread `PRRT_kwDOM79GKM63DK7y`):** Add `|| help_out.status_code == Some(127)` to the immediate `return None` guard on `--help` probe output in `verify_qoder_target_candidate`, matching `--version` exit `127` handling.
5. **Honor validated `.qodercli-install-resource` when `--version` prints bare `1.1.x` and `--help` is unavailable (`src/agents/discovery.rs:1118–1131`, PR thread `PRRT_kwDOM79GKM63DK8I`):** Compute validated `detect_qoder_install_resource(&target_launcher_path, &target_resolved_path, &wrapper_targets, env.is_windows)` (`matches!(..., Ok(Some(_)))`) and accept it (when `!has_conflicting_non_qoder_product(&version_text)`) in `is_verified_cli` so real Qoder CLI installations returning bare `1.1.87` on `--version` are still recognized when `--help` times out, truncates, errors, or exits non-zero (non-127, non-conflicting), while preserving `help_negated_update`, conflicting-product/banner rejections, and `curl-bash` / `homebrew-cask` / `npm` / invalid-marker ownership boundaries.

**Acceptance criteria:**
- [ ] Spoofed `.qoder/entry/qoder`, `.qoder\entry\qoder.cmd`, and `qoder-dispatcher.ps1` files whose markers appear only in comments (`#`, `REM`, `::`) or `echo` lines are rejected (`None`, zero target probes), while both exact upstream `@qoder-ai/qodercli@1.1.88` templates (`J6s.sh`, `J6s.cmd` with `set "DISPATCHER=%~dp0qoder-dispatcher.ps1"` + `powershell.exe ... -File "%DISPATCHER%" -- %*`, BOM-prefixed `J6s.ps1` with `& $exe @argv`) and existing test fixtures pass verification.
- [ ] A `.qoder/entry/qoder` symlink pointing outside `entry/` to an oversized (`>8 KiB`) `#!` script or invalid-UTF-8 script fails closed (`None`, zero probes), whereas a symlink to a compiled native binary (ELF/Mach-O/PE magic) falls through to `NotDispatcher` probing.
- [ ] `qoder_entry_dispatcher_unix_precedence_and_target_ownership` and all new unit tests in `src/agents/mod.rs` compile with zero warnings and pass under both `cfg(unix)` and `cfg(windows)`.
- [ ] A `--help` probe exiting `127` (even without `"not installed"` wording and even when `--version` succeeded or `curl-bash` marker is present) returns `None`.
- [ ] When `--version` outputs bare `1.1.87` (exit `0`) and `--help` fails/times out (non-127, non-conflicting), candidates with a valid `.qodercli-install-resource` marker are discovered with the expected owner (`curl-bash` -> `NativeSelfUpdate`, `homebrew-cask` -> `ManualActionRequired`, `npm` without proven prefix -> `ManualActionRequired`, negated `--help` -> `ManualActionRequired`), while unbranded bare `1.1.87` candidates without a valid marker or `--help` evidence still return `None`.

**Verification:**
- [ ] Focused RED -> GREEN unit and CLI tests pass: `cargo test --locked agents` and `cargo test --locked --test agents_cli`
- [ ] Full spec verification suite passes with zero warnings:
  `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked && cargo build --locked && RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps && cargo metadata --locked --no-deps --format-version 1 && cargo package --list --locked && cargo publish --dry-run --locked && git diff --check`

**Dependencies:** `T005`

**Files likely touched:**
- `src/agents/discovery.rs`
- `src/agents/mod.rs`
- `tests/agents_cli.rs` (optional if CLI-level coverage is added)

**Estimated scope:** Small/Medium (2–3 files)

---

## Task T008: Leader verification, merge, push, and PR #29 conversation resolution

**Owner:** Leader

**Description:** Independently review and verify `T007` (`code-review-and-quality` + `doubt-driven-development`), merge into `feat/qq-12-qoder-cli`, conditionally delete the `T007` coder branch, push `origin/feat/qq-12-qoder-cli` to update PR #29, post concise technical replies referencing the fixing commit SHA on each of the five PR #29 review comments, resolve all five PR #29 review threads via GitHub GraphQL, update `QQ-12` tracking, and return `QQ-12` to `in_review`.

**Acceptance criteria:**
- [ ] `T007` verified independently (including full 11-command `spec.md` suite and `cargo audit --deny warnings`), merged into `feat/qq-12-qoder-cli`, and coder branch cleaned up locally and on `origin`.
- [ ] All 5 Copilot review threads on PR #29 (`PRRT_kwDOM79GKM63DK7C`, `PRRT_kwDOM79GKM63DK7P`, `PRRT_kwDOM79GKM63DK7h`, `PRRT_kwDOM79GKM63DK7y`, `PRRT_kwDOM79GKM63DK8I`) replied to and marked `isResolved: true`.
- [ ] Updated head pushed to `origin/feat/qq-12-qoder-cli`, `QQ-12` moved to `in_review`, and Reviewer notified within 5 short lines.

**Dependencies:** `T007`

**Estimated scope:** Small (review, merge, push, PR thread resolution)

---

## Task T006: Human Reviewer final acceptance

**Owner:** Qian Qiao (Reviewer)

**Description:** Review the delivered pull request and verification evidence on `QQ-12`.

**Acceptance criteria:**
- [ ] Explicit human acceptance or merge; `done` status remains human-owned.

**Dependencies:** `T008`
