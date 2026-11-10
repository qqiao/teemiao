# Implementation plan: documentation and package metadata

**Issue:** QQ-10

**Status:** Independent review accepted; awaiting human acceptance (T007)

Approval: Qian Qiao's QQ-10 comment `01a1c666-6298-7a14-bd34-e3bff5f26445`
(2026-11-10 UTC). Delivery: [PR #27](https://github.com/qqiao/teemiao/pull/27),
`agent/agent/ef1841502b54` targeting `main`.

**Branch:** `agent/leader/c8e152b1d511`, created from `main`

**Tracking:** One issue and one PR; ordered checklist in `tasks.md` here

## Baseline findings

The leader inspected main at `bae5920599a572e3ee728a5f75bce6279536ad2d`.
`cargo package --list --locked` succeeds but prints:

```text
warning: manifest has no documentation, homepage or repository
```

README points to nonexistent contributing guidance and calls the license
“Apache Public License 2.0 (APL-2).” Build-info options/schema/error semantics
and locale/verbosity behavior are missing. Source exposes an unfinished
`config-template`. Historical QQ-9 spec/plan headers still say draft although
their task checklist records completed work; historical status must be reconciled
only against authoritative approval/review evidence, by the leader.

## Structure and ownership

Keep README concise. Put detailed build-info, localization, status, and error
reference in `docs/cli.md`; agent catalog, owner paths, output, and troubleshooting
in `docs/agents.md`. Add `CONTRIBUTING.md` for development/release checks and
`CHANGELOG.md` for the requested patch. Link these from README. Avoid duplicate
agent tables that can drift. Preserve useful old README anchors or provide
navigation at their previous sections when reorganizing.

Audit coverage is recorded in `spec/documentation-review/audit.md`, created by
the coder as implementation evidence. Include these groups:

| Surface | Review disposition |
|---|---|
| README and new user/maintainer guides | Correct and verify against CLI/source |
| Cargo metadata, LICENSE, release references | Correct metadata/name/version; preserve license text |
| Rust module/item docs and locale help | Inspect all modules; fix inaccurate prose only |
| AGENTS.md, CLAUDE.md, CONSTRAINTS.md, CI | Confirm guidance matches required checks; preserve approved bar |
| `spec/agents/` and `spec/agent-improvements/` | Preserve historical requirements; flag stale status for leader correction with evidence |
| `.agents/skills/**`, skills-lock.json | Review as externally maintained workflow resources; retain upstream text |

The leader owns all spec/plan/task changes and any historical-status annotations.
The coder can append audit and completion evidence; neither may fabricate an old
approval or mark human-review tasks complete merely because code was merged.

## Execution order and decisions

`T001 approval -> T002 CLI docs -> T003 maintainer/help audit -> T004 metadata
-> T005 patch and validation -> T006 leader review -> T007 human acceptance`.

One coder works sequentially. Record discovered inaccuracies before correcting
them and map the verification evidence to each requirement. Use
test-driven-development for executable behavior changes if separately approved;
for this prose/metadata scope use source comparison, existing tests, CLI smoke
checks, and Cargo's package verifier without inventing prose-mirroring tests.

Candidate package fields for T004:

```toml
description = "Developer CLI for Git build metadata and installed AI coding-agent updates."
repository = "https://github.com/qqiao/teemiao"
documentation = "https://github.com/qqiao/teemiao#readme"
readme = "README.md"
keywords = ["cli", "build-info", "git", "agents", "developer-tools"]
categories = ["command-line-utilities", "development-tools"]
```

Validate category slugs against the registry before applying them. No dedicated
homepage is established; leave that optional field unset. Keep package contents
unchanged except for the added documentation unless a concrete packaging problem
requires a leader-reviewed adjustment. In particular, retain `locales/`, which is
needed at build time. Do not regenerate dependency versions during the patch bump.

## Verification checkpoints

After T002/T003, inspect documentation links and compare examples with `--help`,
source, and isolated fixtures. Exercise build-info in a disposable repository
with a commit and a writable output path; check JSON keys/types, default/custom
paths, overwrite behavior, and error reporting without relying on its exit status.
Validate locale-specific help and agent dry-runs only with fake executables.

After T005, run the complete spec command list on the committed branch. Inspect
the package list and publish dry-run output rather than only their exit codes.
Check metadata values through Cargo JSON and root version through CLI/lockfile.
Record actual platform/toolchain, commands, counts, and limitations. Do not wait
for external CI unless the issue explicitly requests its result.

Before handoff the coder performs code-review-and-quality, code-simplification,
security-and-hardening, and performance-optimization reviews, focused on accurate
claims, safe examples, packaging assets, and absence of runtime changes. The
leader can serve as the doubt-driven-development adversary during implementation
and independently reviews the final diff, tests, and security evidence. Findings
return to the coder until resolved; human acceptance follows leader validation.

## Risks and source evidence

| Risk | Mitigation |
|---|---|
| Documentation promises behavior code lacks | Confirm exact flags, output, exit semantics, and unfinished commands |
| Examples mutate installed software | Use disposable Git repos and fake agent fixtures |
| Reorganized links break on crates.io | Check repository and registry link destinations; use canonical absolute links where needed |
| Metadata hides warning without helping users | Meaningful URLs/description; no suppression; inspect Cargo output |
| Package misses embedded locale files | Inspect file list and let Cargo compile the archive |
| Historical records are rewritten as current policy | Retain history; leader annotates only with linked evidence |

Official references checked 2026-11-11:

- [Cargo manifest metadata](https://doc.rust-lang.org/cargo/reference/manifest.html#package-metadata):
  repository/documentation/README fields, keyword/category validation, and guidance
  against redundant homepage URLs.
- [Cargo package](https://doc.rust-lang.org/cargo/commands/cargo-package.html):
  package listing and archive build verification.
- [Cargo publish](https://doc.rust-lang.org/cargo/commands/cargo-publish.html):
  `--dry-run` performs validation without uploading.
