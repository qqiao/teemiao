# Feature specification: agent command improvements

**Issue:** QQ-9 — Agent sub command improvement

**Status:** Draft; awaiting Qian Qiao's explicit approval of specification, plan, and tasks

**Date:** 2026-11-09

**Baseline:** `main` at `0bdd79f1082a2fb528ef32a67789fc2fa6f8d55f` (Teemiao `0.2.0`)

## Objective and scope

Make each agent's update output easy to distinguish, add Qoder CLI to the
existing agent manager, and deliver the requested patch bump to `0.2.1`.
Qoder support means discovery, targeted and bulk updates, and dry-run parity.
The existing `CONSTRAINTS.md` applies unchanged. This extends the existing
`spec/agents/` contract; its completed work and historical approvals stay intact.

## User stories and acceptance scenarios

### US1 — Read each update independently (P1)

Given several installed agents with multiline updater output, running
`teemiao agents update` shows a separate, named section for each attempted agent.
Warnings and diagnostics sit inside their agent's section. A clearly labelled
final summary follows, with one outcome row per attempted agent. A failed update
still permits later agents to run; interruption stops the sequence.

### US2 — Manage Qoder alongside existing agents (P1)

Given an identifiable Qoder CLI on `PATH`, `agents list` shows one Qoder row,
`agents update qoder --dry-run` previews the exact command, and `agents update
qoder` updates that selected installation. Omitting the identifier includes Qoder
in the existing sequential bulk flow. A desktop launcher or unrelated executable
must not become a Qoder CLI update target. An absent Qoder is not installed.

### US3 — Identify the patch version (P2)

After the behavior changes pass verification, the manifest, root lockfile package
entry, and `teemiao --version` agree on `0.2.1`.

## Functional requirements

- **FR-001:** Before each attempted agent's warnings or updater invocation, write
  and flush a plain-text header to stderr containing its display name, canonical
  identifier, and position in the selected sequence. Use a blank line between
  sections. Apply this to one-agent, bulk, and dry-run updates, including manual
  action and failure cases. Do not print headers for agents never attempted.
- **FR-002:** Keep vendor diagnostics on stderr and result rows on stdout. Finish
  the diagnostic section with a newline even if vendor output lacks one; do not
  discard multiline details. Add a localized summary heading, separated by a
  blank line, on stdout when there are results. Keep existing result row content,
  exit codes, sequential order, output bounds, and interruption behavior.
  Empty selection retains its existing no-op message without empty sections.
- **FR-003:** Section labels and the summary heading have English and Simplified
  Chinese translations. Plain redirected output remains readable without ANSI
  colors, cursor control, terminal-width assumptions, or new flags.
- **FR-004:** Add canonical identifier `qoder`, convenience alias `qodercli`, and
  display name `Qoder`. Append it after the existing six catalog entries. Resolve
  `qodercli` first, then a verified `qoder` CLI fallback, retaining the selected
  launcher. Preserve PATH order within each name and report Qoder only once.
  The preference accommodates the binary currently shipped by the installer.
- **FR-005:** Use bounded, read-only version/help probes or verified local package
  metadata to establish Qoder CLI identity. The shared `qoder` name requires CLI
  evidence, not merely a version number or product-name substring: corroborate
  its CLI help/update interface or its exact CLI package entry point. Reject
  desktop/IDE launchers and conflicting identity evidence. Unknown ownership
  yields manual guidance rather than guessing an update command.
- **FR-006:** For an identified native Qoder CLI with no contradictory package
  ownership, invoke the selected launcher with the single argument `update`.
  For a proven npm installation, reuse the verified npm executable and prefix
  with `install -g --prefix <prefix> @qoder-ai/qodercli@latest`. Presence of npm
  alone is insufficient ownership evidence. Unsupported managers, ambiguous
  wrappers, and channels/pins that cannot be preserved require manual action.
  Never migrate npm installations to native or execute an installation script.
- **FR-007:** Qoder uses existing post-update re-discovery and result verification.
  A zero exit code alone, a generic dependency message, or an unchanged/unreadable
  version cannot prove success. Dry-run performs zero update mutations. Missing
  Qoder, failures, manual action, and interruption retain existing exit semantics.
- **FR-008:** Update README and localized help with Qoder identifiers, supported
  paths, examples, and output separation. Bump only the Teemiao package version
  from `0.2.0` to `0.2.1` in Cargo.toml and Cargo.lock after behavior verification;
  avoid dependency refreshes. The requested bump does not include publishing,
  tagging, or merging a release.

Illustrative English presentation (vendor text shown as placeholders):

```text
--- [1/2] Antigravity (antigravity) ---
Antigravity (antigravity): <vendor diagnostics>

--- [2/2] Qoder (qoder) ---
Qoder (qoder): <vendor diagnostics>

Update summary
Antigravity (antigravity): <existing outcome row>
Qoder (qoder): <existing outcome row>
```

Headers/diagnostics are stderr; summary heading/rows are stdout. The example
illustrates a combined terminal view; independent stream collectors control their
own merge order. No live-streaming redesign is required for this presentation fix.

## Technical context and verification

Rust 2024, clap 4.6, rust-i18n 3, and existing synchronous process execution.
`src/agents/mod.rs` owns catalog/orchestration/presentation;
`discovery.rs` owns identity/ownership; `update.rs` owns plans/result verification.
Locales live in `locales/`; fake executable integration tests in
`tests/agents_cli.rs` with `tests/support/mod.rs`. Follow existing typed enums,
argument arrays, and localization calls, for example:

```rust
AgentId::ClaudeCode => "claude-code",
```

Add behavior tests before production changes, using only isolated fake tools.
Cover multiline/no-trailing-newline diagnostics, silent tools, partial failure,
dry-run, empty selection, cancellation, both locales, both Qoder executable names,
desktop/name collisions, npm prefix ownership, unknown versions, and aliases.
Retain all existing behavioral assertions except intentional presentation/catalog
expectations. Run focused tests, then these delivery checks:

```sh
cargo test --locked agents
cargo test --locked --test agents_cli
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
cargo run --locked -- --version
git diff --check
```

## Boundaries and success criteria

- Always follow `CONSTRAINTS.md`, preserve existing ownership checks and probe
  limits, and record actual local/platform validation evidence.
- Return scope changes, new dependencies, and changes to this design to the leader.
- Never update real installed agents during development, alter credentials or
  user settings, weaken tests, or claim platforms passed without evidence.
- Success: automated assertions prove section boundaries and ordering, Qoder
  discovery/update safety, all existing regressions pass, and version is `0.2.1`.
  One PR targets `main`; leader reviews it before final human acceptance.

## Approval

Proposed defaults are CLI-only Qoder support across list/update/dry-run, the
executable preference above, plain-text separation, and the requested patch bump.
No additional product question is needed to review this draft. Implementation
awaits explicit approval of this package under the squad's AGENTS.md workflow.
