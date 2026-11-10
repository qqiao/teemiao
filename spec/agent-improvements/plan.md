# Implementation plan: agent command improvements

**Issue:** QQ-9

**Status:** Historical; implementation reviewed, merged, and accepted

See the acceptance evidence in [spec.md](spec.md). This plan records the design
delivered in [PR #26](https://github.com/qqiao/teemiao/pull/26), merged as
`bae5920599a572e3ee728a5f75bce6279536ad2d`; it is not pending implementation.

**Branch:** `agent/leader/dde0f9b81f6e`, based on `main`

**Tracking:** One issue, one eventual PR; ordered work is in `tasks.md` here.

## Design and execution order

This is an extension of the existing agent-management capability. Two behavior
slices share its catalog and orchestration; the version bump follows both.

`T001 approval -> T002 output separation -> T003 Qoder -> T004 documentation/version
-> T005 leader review -> T006 human acceptance`.

1. Add localized per-agent framing at `run_update`'s existing sequential boundary,
   before warnings and `execute_agent_update`. Flush the header before invoking
   the updater. Keep the existing bounded capture and diagnostic formatter.
   Add summary framing after the loop without changing outcome classification.
2. Extend `AgentId`, `ALL`, display/canonical IDs, CLI candidates, clap aliases,
   and `FromStr` together. Reuse discovery and update abstractions. Keep original
   six-agent order and add Qoder last.
3. Verify both executable-name cases. Prefer the first verified `qodercli`; use
   verified `qoder` only as fallback. PATH resolution, symlink behavior, Windows
   suffix/shim validation, and post-update resolution follow existing helpers.
   Probe help only when necessary for identity/native updater verification;
   apply existing deadlines/output caps and do not persist settings.
4. Tie npm ownership to the resolved `@qoder-ai/qodercli` entry point and matching
   prefix using existing metadata/wrapper validation. Check managed ownership
   before allowing native self-update. For native binaries require positive CLI
   evidence including the update interface and no contradictory ownership.
   Do not infer native ownership merely because npm is missing. Ambiguous or
   unsupported installations remain visible with manual guidance.
5. Reuse `UpdateOwner::NativeSelfUpdate`, `UpdateOwner::NpmGlobal`, and current
   plan execution. Add Qoder subject aliases to outcome matching without
   loosening its negation/foreign-message protections. Prefer version-change
   verification; only add unchanged-version success patterns backed by evidence.
6. Document behavior, then bump the root package version and its lockfile entry.
   Keep dependency versions unchanged. Run all specified checks and deliver
   implementation evidence for independent leader review.

One coder works sequentially because the slices share files. The leader owns
specification/planning and scope decisions. The coder must use
test-driven-development, can use the leader as a doubt-driven-development
adversary, and must complete code-review-and-quality, code-simplification,
security-and-hardening, and performance-optimization reviews before handoff.
The performance review checks unnecessary probes and bounded work; no new
benchmark framework is needed.

## Source evidence and limits

Checked 2026-11-09 without running installers or installed agents:

- [Qoder installation and upgrade](https://docs.qoder.com/cli/installation):
  documents native installation, `qoder --version`, `qoder update`, and the
  compatibility npm package `@qoder-ai/qodercli` with an npm upgrade path.
- [Qoder command reference](https://docs.qoder.com/cli/cli-reference): documents
  noninteractive `--version`/`--help`, `update`, and CLI-specific options.
- The official [Unix installer](https://qoder.com/install) and
  [PowerShell installer](https://qoder.com/install.ps1), read as text, still name
  the extracted binary `qodercli` and delegate installation to that executable.
  This is why both names need coverage; do not assume qodercli is obsolete.
- [Qoder settings reference](https://docs.qoder.com/cli/settings-reference):
  documents auto-update configuration. It does not establish that the existing
  generic `DISABLE_AUTOUPDATER` environment flag is honored by Qoder. Use only
  version/help probes, retain existing child safeguards, do not write settings,
  and do not claim arbitrary vendor binaries are sandboxed or network-free.

Installer scripts delegate placement to the binary, so they do not establish
universal native installation directories. Do not invent path markers or assert
real updater verification from fake fixtures. If CLI/native identity cannot be
proved within these constraints, return the concrete gap to the leader rather
than broadening detection or changing the specification silently.

## Risks and verification checkpoints

| Risk | Required evidence |
|---|---|
| New framing still mixes diagnostics | Multiline and missing-newline fixtures; assert block membership/order and stream destinations |
| `qoder` is an IDE launcher | Desktop/foreign negative fixtures; require CLI identity before any update |
| Wrong Qoder installation updated | Two-name precedence, shadowing, wrong-prefix metadata, and exact argument tests |
| npm silently migrates to native | Proven npm fixture uses npm only; no installer or native fallback |
| False success | Changed-version positive and unchanged/negated/foreign-output negative tests |
| Platform-specific regression | Existing Linux/macOS/Windows CI matrix; distinguish local from pending CI results |

After T002 and T003, run focused tests and check the diff against CONSTRAINTS.md.
After T004 run the complete command list in `spec.md`. Update test expectations
for the seventh catalog entry without weakening existing six-agent assertions.
Version smoke tests do not invoke agent discovery. Record test counts and actual
commands. CI runs externally; do not wait for it unless explicitly requested.

At handoff provide commit/branch, PR URL, validation results, output samples from
isolated fixtures, and any material limitation. The leader performs code/test/
security review, sends findings back to the coder, and repeats until satisfied.
Only then does the parent move to `in_review` for human acceptance.
