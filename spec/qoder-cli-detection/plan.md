# Implementation plan: Qoder CLI dispatcher detection, alias support, and patch 0.2.4

**Issue:** QQ-12

**Status:** Approved by Qian Qiao on 2026-11-13 in `QQ-12` comment `01a1d430-db07-70ed-a981-4c203afc2b29` (approved spec commit `b4fb6b3c6107d9f36ff1ee00c7b4e2006209bf9e`)

**Overall task branch:** `feat/qq-12-qoder-cli` (from `main` at `1ec92c726c934a12f29c5c01e401dd9c90e34520`)

**Tracking:** Single parent issue (`QQ-12`) and single PR to `main`; task checklist in [`tasks.md`](tasks.md) and squad board in `QQ-12` description.

## Overview

Investigating the `QQ-12` reproduction on `teemiao 0.2.3` (`which qoder` -> `~/.qoder/entry/qoder`, Qoder missing from `teemiao agents list`) against `src/agents/discovery.rs` and upstream `@qoder-ai/qodercli@1.1.88` identified two concrete defects plus three hardening/UX gaps:

1. **Discovery false rejection on wrapped `--help` preamble (`src/agents/discovery.rs:742, 1010–1059, 1245–1279`):** `qodercli` / `qoder --version` outputs bare `1.1.88`, triggering `--help` over a non-TTY pipe. Commander.js wraps `"Qoder CLI - Defaults to interactive mode. Use -p/--print for non-interactive output."` at 80 columns, placing `"output."` on line 2 of the preamble. `parse_help_lines` emits `"output."` as a standalone `HelpLine::Preamble`, which `preamble_line_has_conflicting_banner` rejects as a non-Qoder product banner — dropping both `qodercli` and `qoder`.
2. **Dispatcher wrapper ownership rejection (`src/agents/discovery.rs:1537–1576, 1769–1777`):** Upstream Qoder CLI `1.1.x` installs `~/.qoder/entry/qoder` (Unix bash script) and `%USERPROFILE%\.qoder\entry\qoder.cmd` + `qoder-dispatcher.ps1` (Windows) and prepends `.qoder/entry` to `PATH`. `determine_qoder_owner` treats the dispatcher's `exec "$cli" "$@"` / `%*` forwarding as an unverified wrapper (`ManualActionRequired`) instead of resolving the underlying `qodercli` target (`PATH`, `~/.local/bin/qodercli`, `~/.qoder/bin/qodercli/qodercli[.exe]`) and verifying its ownership.
3. **IDE-only dispatcher stub & renamed bundle guards (`src/agents/discovery.rs:827–851, 910–916`):** When Qoder IDE installs `.qoder/entry/qoder` without Qoder CLI, `qoder --version` exits `127` with `"Qoder CLI is not installed..."` (containing the substring `"qoder cli"`), and macOS Qoder IDE now also ships as `Qoder IDE.app` (Linux `/usr/share/qoder-ide/`). Both must be explicitly excluded from CLI detection.
4. **CLI alias & native update confirmation (`src/agents/mod.rs:66, 139`; `src/agents/update.rs:1340–1379`):** Add `qoder-cli` to `AgentId::Qoder` clap aliases and `FromStr`, and recognize `"Successfully updated Qoder CLI from <old> to <new>."` in `updater_output_confirms_updated` when initial version was unknown.

## Architecture decisions

1. **Paragraph-aware preamble banner validation:** Track contiguous preamble lines between blank lines, `Usage:` blocks, and section headers. When a preamble paragraph opens with a validated Qoder banner-and-description separator (` - `, ` — `, ` – `, `: `) or prose sentence that soft-wraps across lines, validate wrapped continuation lines with `description_has_conflicting_brand` rather than `is_conflicting_banner_segment`, while still rejecting distinct conflicting banner lines (`Qoder CLI\nAcme CLI`, `Acme Qoder CLI`, `Acme CLI`).
2. **Static, zero-execution dispatcher resolution:** Recognize Qoder's official `.qoder/entry` (and `.qoder-cn/entry`) dispatcher scripts by path structure + bounded text inspection (`read_text_wrapper_sample`), never by executing shell or PowerShell code. Resolve the underlying `qodercli` target purely via `DiscoveryEnv` filesystem/PATH lookups in Qoder's exact order (`QODER_CLI_BIN` on Windows -> `qodercli` on `PATH` excluding dispatcher shims -> `~/.local/bin/qodercli` on Unix -> `~/.qoder/bin/qodercli/qodercli[.exe]`).
3. **Direct binary execution for native updates:** When `.qoder/entry/qoder` dispatches to a verified standalone `qodercli` binary, set `UpdateOwner::NativeSelfUpdate.program` to that verified `qodercli` binary (`display_command: "qodercli update"`) while keeping `DiscoveredAgent.launcher_path` as `.qoder/entry/qoder`. This eliminates shell/PowerShell wrapper indirection and Windows `.cmd` batch risks during `agents update`, while post-update rediscovery still matches `launcher_path`.
4. **Install-source marker integration:** Read `.qodercli-install-resource` adjacent to the canonical `qodercli` binary (bounded read) as an additional ownership signal: `"homebrew-cask"` -> `ManualActionRequired` (`brew upgrade --cask qodercli`), `"npm"` without verified npm prefix -> `ManualActionRequired` (ambiguous npm), `"curl-bash"` -> compatible with `NativeSelfUpdate` when no package-manager signal contradicts it.
5. **Parallel task slicing across two coders:** `spec.md` settles the exact CLI alias (`qoder-cli`) and dispatcher contract up front so Wave 1 runs **two completely file-disjoint tasks in parallel**:
   - `T002` owns Rust implementation & behavioral tests (`src/agents/{mod,discovery,update}.rs`, `tests/agents_cli.rs` [keeping version `0.2.3` assertion until `T004`]).
   - `T003` owns localized help catalogs & agent reference docs (`locales/{en,zh-CN}.yml`, `docs/agents.md`).
   After Leader reviews, verifies, and merges both `T002` and `T003`, `T004` performs the atomic `0.2.4` version bump, changelog, `README.md`/`docs/cli.md` alignment, and clean-tree `cargo publish --dry-run --locked`.

## Dependency graph and execution order

```text
T001 (Reviewer spec/plan/tasks approval)
  ├──> T002 (Core Qoder discovery, dispatcher ownership, qoder-cli alias & tests)  [Coder A — parallel]
  └──> T003 (Localized help catalogs & docs/agents.md updates)                     [Coder B — parallel]
         │
         ▼ (Checkpoint 1: Leader review, independent verification & merge of T002 + T003)
       T004 (Patch 0.2.4 bump, CHANGELOG/README/docs/cli.md & packaging dry-run)   [Coder]
         │
         ▼ (Checkpoint 2: Full suite + packaging checks)
       T005 (Leader doubt-driven final verification & GitHub PR to main)
         │
         ▼
       T006 (Human acceptance on QQ-12)
```

## Task list summary

### Phase 1: Specification review
- [x] `T001`: Reviewer (`Qian Qiao`) approves `spec.md`, `plan.md`, and `tasks.md` (`01a1d430-db07-70ed-a981-4c203afc2b29` on `b4fb6b3c6107d9f36ff1ee00c7b4e2006209bf9e`).

### Phase 2: Parallel core implementation and documentation (Wave 1)
- [x] `T002`: Fix Qoder CLI discovery, `.qoder/entry` dispatcher ownership resolution, `qoder-cli` alias, and update outcome matching (`src/agents/mod.rs`, `src/agents/discovery.rs`, `src/agents/update.rs`, `tests/agents_cli.rs`) — verified `d560b21` and merged in `782b75d`.
- [x] `T003`: Update `en`/`zh-CN` CLI help catalogs and `docs/agents.md` for `qoder-cli` and `.qoder/entry` dispatcher behavior (`locales/en.yml`, `locales/zh-CN.yml`, `docs/agents.md`) — verified `d794c52` and merged in `390b68a`.

### Checkpoint: Wave 1 integration
- [x] Both `T002` and `T003` independently reviewed, verified with `doubt-driven-development`, and merged into `feat/qq-12-qoder-cli`; `cargo test --locked` (55 unit + 35 integration tests) and `cargo clippy --locked --all-targets -- -D warnings` pass.

### Phase 3: Release preparation and PR delivery (Wave 2)
- [x] `T004`: Bump version to `0.2.4` in `Cargo.toml` and `Cargo.lock`, update `CHANGELOG.md`, `README.md`, `docs/cli.md`, and `tests/agents_cli.rs` version/help assertions, and validate `cargo package --list --locked` + `cargo publish --dry-run --locked` — verified `1c42035` and merged in `87293ed`.
- [x] `T005`: Leader final `doubt-driven-development` verification on `feat/qq-12-qoder-cli` tip and GitHub PR creation (`QQ-12: ...`).
- [ ] `T006`: Human Reviewer final acceptance.

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Relaxing wrapped preamble check admits foreign tools named `qoder` | High | Only continuation lines of a validated Qoder paragraph are treated as description prose; `description_has_conflicting_brand` and usage-stem checks remain enforced and tested against `Acme CLI` / `Acme Qoder CLI` / `index-tool`. |
| IDE-only `~/.qoder/entry/qoder` stub falsely detected as Qoder CLI | High | Require an existing resolved `qodercli` target for `.qoder/entry` dispatchers and reject `"is not installed"` / exit `127` outputs in `version_has_qoder_cli_branding`. |
| Windows `.qoder\entry\qoder.cmd` executes PowerShell during update or trips batch metacharacter checks | High | Resolve the underlying `qodercli.exe` statically and execute `qodercli.exe update` directly for `NativeSelfUpdate`; preserve `is_unsafe_windows_cmd_shim` / `validate_batch_command` at every probe/update boundary. |
| Merge conflicts between parallel coders in Wave 1 | Medium | `T002` and `T003` touch strictly disjoint file sets; existing `src/main.rs` and `tests/agents_cli.rs` help tests pass both before and after `T003`. |
| `Cargo.lock` drifts transitive dependencies during `0.2.4` bump | Medium | Edit only the root `teemiao` `version` entry and verify with `git diff Cargo.lock` + `--locked` Cargo commands. |

## Open questions

None.
