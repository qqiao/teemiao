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
- [ ] `teemiao agents list` and `teemiao agents update {qoder,qodercli,qoder-cli}` detect Qoder CLI `1.1.x` when `--version` prints bare `1.1.88` and `--help` wraps `"Qoder CLI - Defaults to interactive mode. Use -p/--print for non-interactive\noutput."` across lines, while still rejecting `Acme Qoder CLI`, `Acme CLI`, `Qoder CLI\nAcme CLI`, and `index-tool`.
- [ ] `.qoder/entry/qoder` (Unix) and `.qoder\entry\qoder.cmd` + `qoder-dispatcher.ps1` (Windows) resolve their underlying `qodercli` target in Qoder's exact order without executing shell/PowerShell code: standalone native targets yield `UpdateOwner::NativeSelfUpdate` invoking the verified `qodercli` binary (`qodercli update`); proven npm `@qoder-ai/qodercli` targets yield `UpdateOwner::NpmGlobal`; Homebrew (`.qodercli-install-resource` == `"homebrew-cask"` or Caskroom/Cellar), prerelease npm, foreign managers, and unverified `QODER_CLI_BIN` / wrapper targets yield `UpdateOwner::ManualActionRequired`; missing CLI targets / `"Qoder CLI is not installed"` (exit `127`) and `Qoder IDE.app` / `/usr/share/qoder-ide/` / `/remote-cli/` return `None` (absent).
- [ ] `updater_output_confirms_updated` recognizes `"Successfully updated Qoder CLI from <old> to <new>."` when initial version was `None`, while preserving `"Unable to determine installation method."` and incomplete-output rejections.

**Verification:**
- [ ] Focused RED -> GREEN unit and integration tests pass: `cargo test --locked agents` and `cargo test --locked --test agents_cli`
- [ ] Full suite, lint, and formatting pass: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked && cargo build --locked`
- [ ] Security and performance checks documented (bounded wrapper/marker file reads, zero extra subprocess probes when npm ownership is already proven, batch metacharacter validation preserved).

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
- [ ] Leader independently reviews and verifies `T002` and `T003` commits with `code-review-and-quality` and `doubt-driven-development`
- [ ] Both branches cleanly merged into `feat/qq-12-qoder-cli` and coder branches deleted
- [ ] Combined `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked` pass on `feat/qq-12-qoder-cli`

---

## Task T004: Prepare patch `0.2.4`, update release docs/changelog, and verify packaging

**Owner:** Coder (Wave 2 — sequential after Wave 1 checkpoint)

**Description:** Bump the root `teemiao` package version from `0.2.3` to `0.2.4` in `Cargo.toml` and `Cargo.lock` (preserving all dependency versions), add the `## [0.2.4] — Unreleased (Prepared)` section in `CHANGELOG.md`, align `README.md` (`qoder-cli` alias + `0.2.4` changelog reference) and `docs/cli.md` (`teemiao 0.2.4` output example), and update `tests/agents_cli.rs` to assert `teemiao 0.2.4` and `qoder-cli` in `agents update --help`.

**Acceptance criteria:**
- [ ] `Cargo.toml`, root `Cargo.lock` package entry, and `teemiao --version` all report `0.2.4`; `git diff Cargo.lock` modifies only the root `teemiao` version line.
- [ ] `CHANGELOG.md`, `README.md`, and `docs/cli.md` accurately describe `0.2.4` and `qoder-cli` without altering historical `0.2.2`/`0.2.3` records.
- [ ] `tests/agents_cli.rs` asserts `teemiao 0.2.4` on `--version` and `qoder-cli` visibility in `teemiao agents update --help`.

**Verification:**
- [ ] Full spec verification suite passes on a clean committed tree:
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
- [ ] All acceptance criteria in `spec.md` and `CONSTRAINTS.md` verified independently on the final `feat/qq-12-qoder-cli` commit.
- [ ] GitHub PR opened against `main` (without merging, tagging, or publishing) and reported on `QQ-12`; issue moved to `in_review`.

**Verification:**
- [ ] Complete `spec.md` command list + `cargo audit --deny warnings` (if installed) + `gh pr view` check.

**Dependencies:** `T004`

**Estimated scope:** Small (verification + PR creation)

---

## Task T006: Human Reviewer final acceptance

**Owner:** Qian Qiao (Reviewer)

**Description:** Review the delivered pull request and verification evidence on `QQ-12`.

**Acceptance criteria:**
- [ ] Explicit human acceptance or merge; `done` status remains human-owned.

**Dependencies:** `T005`
