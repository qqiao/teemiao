# Implementation plan: agents subcommand

**Status:** Approved by Qian Qiao on 2026-11-08; implementation authorized.

**Base:** `main` at `2bf8dbdadb546fef75af3d1ede1814b142e0471c`

**Branch:** `agent/leader/86bf5d8b0a06`

**Tracking:** One Multica issue, QQ-8, and one GitHub PR. Ordered checklist in
`tasks.md` in this directory, as required by the squad's workflow.

## Context and constraints

Rust 2024 binary; clap 4 derive commands; rust-i18n 3; thiserror 2. The existing
command tree and locale setup live in `src/main.rs`; localized strings live in
`locales/en.yml` and `locales/zh-CN.yml`. Current CI builds/tests only on Ubuntu.
Apply the root `CONSTRAINTS.md`. Do not add frameworks, a daemon, or a database.

## Architecture

- `src/agents/mod.rs`: clap argument types, fixed catalog/IDs, execution boundary,
  result types, orchestration, and localized presentation. Keep the module small;
  move process details into a focused module only when warranted by implementation.
- `src/agents/discovery.rs`: executable resolution, bounded probes, identity and
  ownership evidence. Return records instead of printing from helper functions.
- `src/agents/update.rs`: pure selection of update plans plus synchronous execution
  through an injectable process boundary. Return per-agent results to the caller.
- `tests/agents_cli.rs` and minimal shared fixtures: launch the built Teemiao binary
  with isolated `PATH` and home/config directories; fake tools record arguments and
  return controlled versions, errors, or delays.

Follow existing `Args`/`Subcommand` derive style and typed error propagation.
For example, mirror the existing command argument shape:

```rust
#[derive(Debug, Args)]
pub struct AgentsCommand {
    #[command(subcommand)]
    command: AgentsSubcommand,
}
```

Production errors must return a nonzero process status for the new command; do
not propagate the existing build-info branch's error-printing pattern into agents.
Move the existing test module below `main()` when editing that integration point.

The process boundary accepts executable paths, argument arrays, child-only
environment overrides, and probe/update mode. It must be easy to substitute in
unit tests. Use synchronous execution; six agents do not justify a new async
runtime. Prefer standard library facilities and existing dependencies. A small
cross-platform executable resolver, timeout/process helper, or HTTP/tempfile crate
may be introduced only with a documented need and reviewed lockfile changes.

## Execution decisions

1. Resolve an allowlisted CLI name using platform `PATH` semantics. Retain the
   launcher path separately from a symlink's resolved target. Resolve package
   ownership with local metadata and matching prefixes, not name coincidences.
2. Probe identity/version with deadlines and output limits. A version error does
   not erase a known installation. For generic `agent`, require Cursor branding
   or verified official installation metadata before considering it identified.
3. Select a native or exact package update plan. Cache read-only package-manager
   inventory per invocation instead of running identical inventory commands for
   each candidate. Detect contradictory ownership before mutating anything.
4. For Windows shims, use platform-safe process APIs or the owning executable
   directly; do not build `cmd /C` strings from paths/arguments. Adversarial path
   tests are required, including spaces and shell metacharacters.
5. For Codex standalone only, first validate official native ownership and exact
   destination. Fetch the fixed platform installer over verified HTTPS into a
   private temporary file, then invoke its fixed interpreter with separate
   arguments. Fail closed on a download error, unexpected destination, or inability
   to prove existing-installation targeting. Clean up the temporary file. Do not
   execute installers during automated tests or this development task.
6. Run selected updates one at a time. Let the existing vendor mechanisms enforce
   their policies and locks. No sudo, lock deletion, unrelated process termination,
   or alternate-installation retry. Cancellation stops later work and is distinct
   from an ordinary per-agent failure.
7. Probe the active command again. Distinguish a changed version, a positively
   confirmed up-to-date result, a failed update, manual action, and an unverified
   outcome. Never treat a zero exit plus an unrecognized message as proof of being
   current. Keep captured vendor output bounded and preserve useful diagnostics.

## Delivery order

T001 approval → T002 list → T003 native self-updaters → T004 Codex updates →
T005 Claude Code ownership/update paths → T006 bulk update and verification →
T007 documentation/platform CI → T008 leader review → T009 human review.

One coder implements sequentially to avoid parallel edits to the command tree
and update module. The leader owns specifications and changes to this plan.
The coder uses test-driven-development and may ask the leader to act as the
doubt-driven-development adversary. Before handoff, the coder runs
code-review-and-quality, code-simplification, security-and-hardening, and
performance-optimization reviews. Performance review checks bounded probes and
repeated work; it does not add speculative caching or benchmark infrastructure.

## Verification commands

```sh
cargo test --locked agents
cargo test --locked --test agents_cli
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
git diff --check
```

The focused commands apply after their tests exist. Run CLI smoke tests only
with the isolated test fixture environment; do not invoke real update commands.
Add Linux/macOS/Windows build/test jobs to the existing workflow; formatting and
Clippy can run once on Linux. Do not disable platform tests to obtain green CI.

## Risks and review decisions

| Risk | Mitigation / evidence |
|---|---|
| Wrong active install upgraded | Match ownership/prefix; test shadowing and multiple managers |
| Generic Cursor name collision | Require identity proof; negative fake `agent` test |
| Installer changes upstream | Fixed URL, verified download, target checks, isolated fixtures, explicit unsupported result |
| Legacy CLI migration changes user state | No automatic migration; explain manual action |
| Success status without an update | Re-probe and interpret only known updater outcomes |
| Broken CLI blocks listing | Probe deadline, output cap, child cleanup, visible warning |
| Platform support overstated | CI matrix plus explicit distinction between fake tests and real updater testing |

Final PR description must lead with the user behavior and explain PATH discovery,
supported update methods, channel preservation, and remaining manual cases.
Link the specification and record actual commands/results. Local verification
and a pushed PR are deliverable without waiting for externally running CI.
