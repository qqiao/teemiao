# Feature specification: documentation and package metadata

**Issue:** QQ-10 — Documentation review

**Status:** Independent review accepted; awaiting human acceptance (T007)

Qian Qiao approved this specification, plan, and task package in QQ-10 comment
`01a1c666-6298-7a14-bd34-e3bff5f26445` on 2026-11-10 (UTC):
"Approved, please proceed". Final acceptance remains T007.

**Date:** 2026-11-11

**Baseline:** `main` at `bae5920599a572e3ee728a5f75bce6279536ad2d`, version `0.2.1`

## Objective

Give users accurate installation and command documentation, give contributors
reproducible validation and release instructions, and remove Cargo's missing
package-information warning. Deliver one patch release preparation, `0.2.2`,
on a branch from `main`. The existing approved `CONSTRAINTS.md` applies unchanged.

## User stories and acceptance scenarios

- **US1 (P1) — Use the CLI confidently.** A new user can install Teemiao, discover
  supported commands, generate the documented JSON file, preview an agent update,
  and understand errors and limitations using linked documentation alone.
- **US2 (P1) — Identify the package.** Cargo exposes accurate repository,
  documentation, README, license, and discovery metadata. Packaging no longer
  reports missing human-readable package information; its archive builds.
- **US3 (P2) — Contribute and prepare a patch.** A contributor can reproduce the
  required checks and a publish dry-run. The manifest, lockfile's root package,
  release note, and CLI version all identify `0.2.2`.

## Requirements

- **FR-001 — Audit coverage:** Inventory every tracked documentation surface:
  README, agent instructions, constraints, historical `spec/` documents, Rust
  documentation comments, localized help, CI instructions, and bundled skill
  documentation. Record each surface as corrected, accurate, historical, or
  externally maintained, with evidence. Comprehensive review does not require
  rewriting accurate material or modifying bundled third-party workflows.
- **FR-002 — Entry point:** Make README a useful introduction with Cargo and
  source installation, actual command examples, a documentation index, current
  implementation status, repository/issues/contribution links, and the correct
  name “Apache License 2.0.” Preserve the existing LICENSE text.
- **FR-003 — Build information:** Document positional syntax
  `teemiao build-info [FILE]`, the default `./build_info.json`, repository-root
  invocation, valid HEAD requirement, existing-parent-directory requirement,
  file overwrite behavior, and exactly the `revision` and `build_time` fields.
  Explain short revision and Unix seconds; do not promise stdout JSON, `--out`,
  parent-repository discovery, or extra metadata. Document the current limitation
  that generation errors print to stderr while the process returns success.
- **FR-004 — Agent reference:** Check all seven agents, aliases, executable
  preferences, supported owner/channel paths, dry-run, sequential bulk behavior,
  output streams, exit codes, empty selection, troubleshooting, and probe limits
  against source and isolated tests. Describe Teemiao's safeguards precisely;
  probing an external executable is not a sandbox or universal no-network
  guarantee. Distinguish unsupported installations from absent tools.
- **FR-005 — Other CLI surfaces:** Document verbosity, locale precedence
  (`LC_ALL`, `LC_MESSAGES`, `LANG`), available English/Simplified Chinese strings,
  and fallback behavior as implemented. Explicitly identify `config-template`
  as unfinished and currently panicking when invoked. Correct misleading
  project/help prose in both locales together where needed, without changing
  command behavior, parsing, error handling, or updater execution.
- **FR-006 — Maintainer documentation:** Add actual contributing guidance with
  structure, local prerequisites, existing CI/platform checks, fake-tool safety,
  and a release-preparation checklist. Record a concise `0.2.2` changelog entry
  for documentation and package metadata; do not reconstruct unsupported old
  release history or claim the new version is already published.
- **FR-007 — Manifest:** Improve the description; add repository, documentation,
  explicit README, relevant valid keywords, and categories. Keep `Apache-2.0`.
  Use the existing repository/README as the documentation destination; do not
  invent a website, docs.rs library page, author details, or an untested MSRV.
  Omit redundant homepage metadata when no dedicated homepage exists.
- **FR-008 — Patch:** After documentation checks, change only the root package
  version from `0.2.1` to `0.2.2` in Cargo.toml and Cargo.lock. Retain dependency
  resolutions. Reconcile current version references, preserving historical ones.
- **FR-009 — Verification:** Meet the commands and evidence below. Check links
  and examples, including how README links work on the registry. Inspect packaged
  files for source, locale assets, README, LICENSE, and linked user guides.
  Run publish validation with `--dry-run`; do not upload, tag, or merge.

## Technical context and style

Rust 2024 binary with clap 4.6, rust-i18n 3, and Cargo.lock. `src/main.rs` defines
CLI/locale routing; `src/build_info/mod.rs` writes JSON; `src/agents/` handles
discovery and updates. `locales/` contains help text. Unit tests live in source;
`tests/agents_cli.rs` uses fixtures in `tests/support/mod.rs`.

Use concise Markdown, working links, fenced examples, and existing Rust comment
style. For example, a documented invocation must match its actual positional API:

```sh
teemiao build-info build_info.json
teemiao agents update qoder --dry-run
```

## Verification and success criteria

Run from the repository root after implementation:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
cargo doc --locked --no-deps
cargo metadata --locked --no-deps --format-version 1
cargo run --locked -- --version
cargo run --locked -- --help
cargo run --locked -- build-info --help
cargo run --locked -- agents update --help
cargo package --list --locked
cargo publish --dry-run --locked
git diff --check
```

Package/publish checks run after committing the intended changes so no dirty-tree
bypass is necessary. No `--no-verify` or metadata-warning suppression is allowed.
Use isolated temporary Git repositories and fake executables to validate examples;
never run updates against the user's installed agents. Verify both help locales.
Do not add tests that merely duplicate prose; use existing checks and focused
executable examples. Any separately approved behavior change requires a failing
regression test first.

Success means all FRs have evidence, no missing-metadata warning remains, package
verification builds the archive, required checks pass, version output is
`teemiao 0.2.2`, dependencies are unchanged, and one PR targets `main`. Report
environmental failures honestly; CI pending is not a local test failure.

## Boundaries and approval

- Always preserve quality constraints, safety checks, localization, and behavior.
- Return behavior fixes, dependency/CI changes, unsupported metadata claims, and
  design changes to the leader. Existing bugs are documented, not silently fixed.
- Never weaken tests, execute real agent installers/updaters, change credentials,
  invent historical approvals, publish the crate, push a release tag, or merge.

Implementation approval is recorded above. The leader's independent review is
complete, with verification recorded in tasks.md. Final human acceptance remains
T007; publication, release tags, and merge are outside this delivery's scope.
