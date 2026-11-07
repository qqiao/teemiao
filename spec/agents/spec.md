# Feature specification: agents subcommand

**Issue:** QQ-8 — agents subcommand

**Status:** Approved for implementation by Qian Qiao on 2026-11-08

**Date:** 2026-11-08

## Objective and confirmed scope

A developer can discover installed coding-agent CLIs and update either one agent
or every detected agent through Teemiao. The confirmed catalog is Codex,
Antigravity, OpenCode, Kimi, Cursor, and Claude Code. All six refer to their
command-line tools. Desktop applications and editor extensions are excluded.

## Approved defaults

- Support macOS, Linux (including WSL), and native Windows where the upstream
  agent supports that platform.
- Discover the active installation available through the current process's
  `PATH`. Do not scan disks or load shell startup files. Explain this boundary in
  help and documentation, including how to expose an installation missing from
  `PATH`.
- Update to the newest version available through the installation's existing
  manager and release channel. Do not change channels, replace a package-managed
  installation with a native one, or perform a legacy-product migration.
- Unsupported or ambiguous installations remain visible and get a reason and
  manual guidance. They never count as successful updates.
- Add `update --dry-run` so the selected commands and targets are reviewable
  without changing installed software. JSON output and arbitrary custom agents
  are outside this release.

## User stories and acceptance scenarios

### US1 — List installed agents (P1)

As a developer, I can see which supported CLIs this shell can use.

1. Given any subset of the six supported agents on `PATH`, when I run
   `teemiao agents list`, then each detected agent appears once in stable catalog
   order with its name, version (or `unknown`), executable path, and update method.
2. Given none are installed, the command prints a clear empty-state message and
   succeeds. An unreadable version from an otherwise identified agent produces
   an explicit warning instead of hiding that installation.
3. Given a desktop launcher named `cursor` or `antigravity`, or an unrelated
   executable named `agent`, none is mistaken for the requested coding-agent CLI.

### US2 — Update one installed agent (P1)

As a developer, I can update one named CLI without changing another tool.

1. Given a supported, identifiable installation, running
   `teemiao agents update codex` updates that installation using the verified
   method and reports the result and versions before/after when available.
2. Given an unknown identifier, invalid syntax is rejected before any updater
   runs. Given a known but absent agent, the command fails clearly without
   installing it.
3. Given a missing manager, unknown ownership, blocked policy, or updater
   failure, the command fails with actionable context and does not try a different
   installer as a fallback.
4. Given `--dry-run`, detection and read-only ownership probes may run, but no
   updater, installer download, or configuration write occurs.

### US3 — Update all installed agents (P1)

As a developer, I can request updates for every detected supported CLI in one run.

1. With no name, `teemiao agents update` processes detected agents sequentially in
   catalog order. Uninstalled agents are skipped without being installed.
2. If one update fails or needs manual action, later agents are still attempted;
   the final summary lists all results and exits nonzero.
3. If no supported CLI is installed, the command is a successful no-op. An
   interruption stops the sequence and does not start later updates.

## Functional requirements

- **FR-001:** Extend the existing clap command tree with `agents list` and
  `agents update [AGENT] [--dry-run]`. `agents` without a subcommand shows help.
- **FR-002:** Accept the canonical identifiers `codex`, `antigravity`, `opencode`,
  `kimi`, `cursor`, and `claude-code`; allow `agy` and `claude` as convenience
  aliases. IDs and aliases are fixed, case-insensitive values.
- **FR-003:** Detect `codex`, `agy`, `opencode`, `kimi`, Cursor's `agent` with
  `cursor-agent` compatibility, and `claude`. Resolve platform executable rules,
  quoted/spaced paths, symlinks, and Windows executable/shim suffixes. For a
  repeated executable name, respect `PATH` order. Prefer verified `agent` over
  its legacy Cursor alias; reject an unverified generic `agent` candidate.
- **FR-004:** Keep both the selected launcher path and resolved ownership
  evidence. A package-manager executable merely existing on `PATH` is not proof
  that it owns an agent. If multiple managers or wrappers prevent proving the
  active installation's owner, require manual action rather than guessing.
- **FR-005:** List performs no update/version-registry requests and never starts
  an interactive agent session. Probe only documented version/help or local
  package metadata. Suppress vendor background updating in probe children where
  supported, without persisting environment/config changes. Bound each child to
  five seconds and each captured stream to 64 KiB; reap children on timeout.
  Do not claim that arbitrary third-party executables are sandboxed or cannot
  themselves contact a network.
- **FR-006:** Update plans consist of a resolved program, argument list, and
  narrowly scoped child environment. Resolve the active package prefix before
  executing managed updates. Never launch an unrelated `agent` executable's
  updater, run a package manager's global upgrade-all operation, or invoke sudo.
- **FR-007:** Use documented native updaters for Antigravity, OpenCode, current
  Kimi, Cursor, and native Claude Code. Codex supports npm, Homebrew, and its
  official standalone installer. See `research.md` for the support matrix and
  `plan.md` for execution constraints. Legacy Kimi and arbitrary wrappers are
  detected but require manual guidance when safe same-installation updating
  cannot be established.
- **FR-008:** Re-resolve and probe the active executable after an update because
  launchers or symlinks may change. Report `updated` only when the new active
  version is verified. Report `up to date` only with a known successful updater
  outcome; do not infer it from unchanged or unreadable version text alone.
  Otherwise report `unverified` or `manual action required` and exit nonzero.
- **FR-009:** Preserve native updater diagnostics and show one final outcome per
  detected agent. Route diagnostics to stderr and list/summary output to stdout.
  Exit 0 for successful/up-to-date results or an empty selection, 1 for failed,
  unsupported, or unverified results, and 2 for clap usage errors. Preserve
  normal platform interrupt behavior; cancellation must not report success.
- **FR-010:** Download an official installer only for a proven native Codex
  installation. Use the fixed official HTTPS URL, a private temporary file,
  validated download success, and a fixed interpreter with argument arrays.
  Never interpolate user input into a shell/PowerShell expression, accept a
  user-provided installer URL, disable TLS verification, or pipe a partial
  download into an interpreter. Verify the installer targets the selected
  installation before launch; otherwise report manual action required.
- **FR-011:** Add English and Simplified Chinese help/status/error messages using
  existing rust-i18n conventions. Keep existing commands and locale behavior.
- **FR-012:** Update README with examples, identifiers, support boundaries,
  exit codes, and troubleshooting. Open one GitHub PR describing behavior,
  tests, platform evidence, and material limits after implementation and review.

## Edge cases

Cover missing/empty `PATH`; non-executable files; broken symlinks; duplicate
installations; unbranded Cursor candidates; Windows `.cmd` paths and metacharacters;
unknown/invalid version output; oversized or hanging probes; an updater that
returns zero but only prints manual instructions; a disappearing executable;
read-only install locations; package-manager locks; network failures; and
interruptions. Teemiao does not remove vendor locks, stop unrelated processes,
or roll back a failed vendor update.

## Success criteria

- **SC-001:** Fixtures cover all six agents; absent tools, desktop launchers, and
  unrelated executables do not produce installed-agent false positives.
- **SC-002:** Every supported update method has a positive argument/ownership test
  and a negative test that proves an incorrect owner cannot trigger an update.
- **SC-003:** Dry-run launches zero mutation commands. Bulk-update tests verify
  continuation, complete summaries, nonzero partial-failure status, and no-op
  empty selection.
- **SC-004:** All existing 20 tests remain; new unit/process tests cover the
  requirements above. Formatting, compilation, and strict Clippy pass.
- **SC-005:** CI validates Linux, macOS, and Windows with fake tools. The PR
  distinguishes simulated updater coverage from actual end-user update testing.

## Non-goals

Installing absent agents; updating desktop apps, plugins, or models; background
scheduling; remote machine management; system-wide disk enumeration; managing
every shadowed copy; automatic legacy migrations; changing release channels;
credential handling; or downloading arbitrary executables from user input.

## Approval

Qian Qiao approved this package with "LGTM" in QQ-8 comment
`01a1b7a7-3876-7273-a84b-19ab22057240`, including the platform, `PATH` discovery,
channel-preservation, unsupported-installation behavior, and dry-run defaults.
The coder stage is authorized; scope changes return to the leader for
specification updates.
