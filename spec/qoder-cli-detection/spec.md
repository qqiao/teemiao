# Feature specification: Qoder CLI dispatcher detection, alias support, and patch 0.2.4

**Issue:** QQ-12 — Add Qoder support

**Status:** Draft — awaiting Reviewer approval (`T001`)

**Date:** 2026-11-13

**Baseline:** `main` (`feat/qq-12-qoder-cli` base) at `1ec92c726c934a12f29c5c01e401dd9c90e34520`, version `0.2.3`

## Objective

Fix false-negative Qoder CLI discovery and wrapper-ownership rejection on real Qoder CLI `1.1.x` installations (including `~/.qoder/entry/qoder` and Windows `%USERPROFILE%\.qoder\entry\qoder.cmd` command dispatchers and Commander.js soft-wrapped `--help` preambles), add the hyphenated `qoder-cli` CLI value alias to `teemiao agents update`, update localized help and user documentation, and prepare patch version `0.2.4` on `feat/qq-12-qoder-cli` for a pull request to `main`. Existing `CONSTRAINTS.md` rules apply unchanged.

## Surface assumptions

1. Canonical agent ID remains `qoder` (display name `Qoder`, 7th in catalog order), preserving backward compatibility with `teemiao 0.2.1`–`0.2.3`; `qoder-cli` is added as a case-insensitive CLI/parser alias alongside `qoder` and `qodercli`.
2. Upstream Qoder CLI `1.1.x` installs a command dispatcher (`~/.qoder/entry/qoder` on Unix; `%USERPROFILE%\.qoder\entry\qoder.cmd` + `qoder-dispatcher.ps1` on Windows; `bundle/qoder-npm-dispatcher.cjs` for npm) onto `PATH` that delegates CLI invocations (`--version`, `--help`, `update`, etc.) to an underlying `qodercli` binary while routing IDE subcommands (`ide`, `chat`, `serve-web`, `tunnel`) to Qoder IDE.
3. When the `~/.qoder/entry` dispatcher exists on `PATH` without an installed `qodercli` binary (IDE-only stub, exiting `127` with `"Qoder CLI is not installed"`), `teemiao` must treat Qoder CLI as **not installed** rather than reporting a false positive.
4. Patch bump target is `0.2.4` (from baseline `0.2.3`), modifying only the root `teemiao` version in `Cargo.toml` and `Cargo.lock` without changing locked dependency versions.

## User stories and acceptance scenarios

- **US1 (P1) — Detect active Qoder CLI installations reliably.** A user with Qoder CLI installed via the official standalone installer (`~/.qoder/entry/qoder` dispatching to `~/.local/bin/qodercli` or `~/.qoder/bin/qodercli/qodercli`, or `%USERPROFILE%\.qoder\entry\qoder.cmd` dispatching to `%USERPROFILE%\.qoder\bin\qodercli\qodercli.exe`), global npm (`@qoder-ai/qodercli`), or Homebrew runs `teemiao agents list` and sees `Qoder (qoder)` with its parsed version, active `PATH` launcher path, and verified update method, while IDE-only stubs and unrelated `qoder` binaries remain excluded.
- **US2 (P1) — Update Qoder CLI safely via any documented alias.** A user runs `teemiao agents update qoder`, `teemiao agents update qodercli`, or `teemiao agents update qoder-cli` (with or without `--dry-run`). Standalone native installations execute the verified native `update` command without shell/PowerShell wrapper ambiguity; proven global npm `@qoder-ai/qodercli` installations update via `npm install -g --prefix <prefix> @qoder-ai/qodercli@latest`; Homebrew, prerelease npm, or foreign package-manager installations report `manual action required` with actionable guidance.
- **US3 (P2) — Consistent documentation and patch `0.2.4`.** Localized CLI help (`en` and `zh-CN`), `docs/agents.md`, `docs/cli.md`, `README.md`, `CHANGELOG.md`, `Cargo.toml`, and `Cargo.lock` accurately reflect `qoder`/`qodercli`/`qoder-cli`, Qoder dispatcher behavior, and version `0.2.4`.

## Requirements

- **FR-001 — Multi-line `--help` preamble parsing:** `src/agents/discovery.rs` must evaluate soft-wrapped continuation lines within a `--help` preamble paragraph (such as Commander.js wrapping `Qoder CLI - Defaults to interactive mode. Use -p/--print for non-interactive\noutput.` at 80 columns when `stdout` is piped) as continuation prose of that paragraph rather than rejecting wrapped trailing lines as unbranded conflicting product banners. Conflicting product titles (`Acme Qoder CLI`, `Acme CLI`, `Acme Code CLI ...`, or a second conflicting banner line such as `Qoder CLI\nAcme CLI`) must still be rejected.
- **FR-002 — Official Qoder command-dispatcher verification and target resolution:** When `qoder` on `PATH` is Qoder's command dispatcher (`<home>/.qoder/entry/qoder` or `<home>/.qoder-cn/entry/qoder` on Unix; `<home>\.qoder\entry\qoder.cmd` paired with `qoder-dispatcher.ps1` on Windows, plus `QODER_CONFIG_DIR` if applicable), discovery must:
  1. Verify the dispatcher file has bounded size and expected Qoder CLI dispatcher markers (not an arbitrary user shell wrapper).
  2. Deterministically resolve the underlying `qodercli` CLI executable using Qoder's dispatcher precedence without executing shell code:
     - Windows: `QODER_CLI_BIN` (when set in `DiscoveryEnv`) -> `qodercli` on `PATH`/`PATHEXT` (excluding dispatcher shims) -> `<home>\.qoder\bin\qodercli\qodercli.exe`.
     - Unix: `qodercli` on `PATH` (excluding dispatcher scripts) -> `<home>/.local/bin/qodercli` -> `<home>/.qoder/bin/qodercli/qodercli`.
  3. Reject the candidate (`None`) when no underlying CLI target exists or when `--version` / `--help` reports that Qoder CLI is not installed (for example `"Qoder CLI is not installed"` / exit `127`). Never treat `"Qoder CLI is not installed"` as positive `version_has_qoder_cli_branding` evidence.
  4. Exclude macOS `Qoder IDE.app` as well as `Qoder.app`, Linux `/usr/share/qoder-ide/` and `/usr/share/qoder/`, and IDE `/remote-cli/` launchers from CLI discovery.
- **FR-003 — Underlying CLI ownership and update plan safety:** When Qoder is discovered through a verified Qoder command dispatcher, `determine_qoder_owner` must inspect the resolved underlying `qodercli` executable (and its canonical path, adjacent `.qodercli-install-resource` marker when present, and wrapper/symlink chain):
  - Proven global npm `@qoder-ai/qodercli` -> `UpdateOwner::NpmGlobal` (preserving prerelease manual guard and missing/unsafe-npm checks).
  - Homebrew Cask/Formula ownership or `.qodercli-install-resource` == `"homebrew-cask"` -> `UpdateOwner::ManualActionRequired` with `brew upgrade --cask qodercli` (or formula) guidance.
  - Foreign managers (`pnpm`, `yarn`, `bun`, `nix`, `scoop`, `chocolatey`, `winget`) or unverified `QODER_CLI_BIN` overrides / ambiguous npm metadata -> `UpdateOwner::ManualActionRequired`.
  - Verified standalone native binary (`.qodercli-install-resource` == `"curl-bash"` or native binary with verified `update` subcommand and no conflicting manager evidence) -> `UpdateOwner::NativeSelfUpdate` whose `program` is the verified underlying `qodercli` executable (`args: ["update"]`), avoiding shell/PowerShell wrapper indirection during update while keeping `DiscoveredAgent.launcher_path` as the active `PATH` launcher.
- **FR-004 — CLI alias `qoder-cli`:** Extend `AgentId::Qoder` clap value aliases and `AgentId::from_str` to accept `"qoder-cli"` (case-insensitive) in addition to `"qoder"` and `"qodercli"`. Keep `AgentId::Qoder.canonical_id()` == `"qoder"` and `cli_candidates()` == `&["qodercli", "qoder"]` unless a verified `qoder-cli` binary candidate is needed (do not break existing `qodercli` -> `qoder` precedence). Update the `FromStr` unknown-agent error message to mention `qoder/qodercli/qoder-cli`.
- **FR-005 — Native Qoder update outcome confirmation:** Extend `updater_output_confirms_updated` in `src/agents/update.rs` so upstream Qoder CLI's native update message (`"Successfully updated Qoder CLI from <old> to <new>."`) also confirms `UpdateOutcome::Updated` when `before_agent.version` was `None`, while keeping all negation, `"Unable to determine installation method."`, and incomplete-output guards intact.
- **FR-006 — Documentation, localization, and patch `0.2.4`:** Update `locales/en.yml`, `locales/zh-CN.yml`, `docs/agents.md`, `docs/cli.md`, `README.md`, and `CHANGELOG.md` to document `qoder-cli`, Qoder dispatcher discovery/ownership behavior, and release `0.2.4`. Bump root `version` in `Cargo.toml` and `Cargo.lock` from `0.2.3` to `0.2.4` without modifying dependency versions.

## API and interface contracts

### External CLI contract

```text
teemiao agents list
  -> stdout row (when installed):
     Qoder (qoder) | version: <ver|unknown> | path: <active-PATH-launcher> | update: <verified-method>

teemiao agents update [qoder | qodercli | qoder-cli] [--dry-run]
  -> accepts all three identifiers case-insensitively; canonical output label remains `Qoder (qoder)`.
  -> exit codes unchanged: 0 (success / up-to-date / clean dry-run), 1 (manual action / failed / unverified), 2 (CLI argument error), 130 (interrupted).
```

### Internal module contract (`src/agents/`)

- `AgentId::Qoder`:
  - `#[value(name = "qoder", aliases = ["qodercli", "qoder-cli"])]`
  - `FromStr`: `"qoder" | "qodercli" | "qoder-cli" => Ok(AgentId::Qoder)`
  - `canonical_id() -> "qoder"`, `display_name() -> "Qoder"`, `cli_candidates() -> &["qodercli", "qoder"]`
- `DiscoveryEnv` (`src/agents/discovery.rs`):
  - Retain existing fields for test compatibility or add `pub qoder_cli_bin: Option<PathBuf>` populated in `DiscoveryEnv::from_process()` from `QODER_CLI_BIN` (updating crate-internal `DiscoveryEnv` struct literals in `src/agents/mod.rs` and `src/agents/update.rs`).
- `UpdateOwner::NativeSelfUpdate`:
  - For direct `qodercli` / `qoder` standalone binaries: `program = launcher_path`, `display_command = "qodercli update"` or `"qoder update"`.
  - For verified `.qoder/entry` dispatchers backed by a standalone `qodercli` binary: `program = resolved_cli_binary_path`, `args = vec!["update".into()]`, `display_command = "qodercli update"`, while `DiscoveredAgent.launcher_path` remains the `.qoder/entry` launcher on `PATH`.

## Tech stack

- Rust `2024` edition (`rustc` / `cargo`), `clap 4.6.0`, `rust-i18n 4.2.4`, `serde` / `serde_json`, `thiserror 2.0.18`, Unix `libc 0.2`. No new crate dependencies allowed without user approval.

## Commands

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo metadata --locked --no-deps --format-version 1
cargo run --locked -- --version
cargo run --locked -- agents update --help
cargo package --list --locked
cargo publish --dry-run --locked
git diff --check
```

## Project structure

```text
src/main.rs              -> CLI entrypoint, locale normalization, localization parity unit tests
src/agents/mod.rs        -> AgentId catalog/aliases, process runner, orchestration, unit tests
src/agents/discovery.rs  -> PATH discovery, bounded probes, Qoder dispatcher & ownership verification
src/agents/update.rs     -> Update plan construction, post-update verification, security unit tests
tests/agents_cli.rs      -> End-to-end isolated fake-agent integration tests
tests/support/mod.rs     -> Disposable TestSandbox and cross-platform fake-executable builders
locales/{en,zh-CN}.yml   -> English and Simplified Chinese CLI messages
docs/{agents,cli}.md     -> Detailed CLI and coding-agent reference guides
```

## Code style

Follow existing Rust 2024 idioms, `rustfmt` defaults, explicit helper functions without `unwrap`/`expect` in production paths, and table-driven unit/integration tests:

```rust
/// Qoder CLI (`qoder`, aliases `qodercli`, `qoder-cli`).
#[value(name = "qoder", aliases = ["qodercli", "qoder-cli"])]
Qoder,
```

## Testing strategy

- **Unit tests (`src/agents/mod.rs`, `src/agents/discovery.rs`, `src/agents/update.rs`, `src/main.rs`):** Test `AgentId` alias parsing, wrapped Commander `--help` preambles (80-col wrap + negative conflicting banners), Qoder dispatcher target resolution on Unix and simulated Windows (`DiscoveryEnv { is_windows: true, .. }`), `.qodercli-install-resource` markers (`curl-bash`, `homebrew-cask`, `npm`), `"Qoder CLI is not installed"` rejection, `Qoder IDE.app` bundle exclusion, and `Successfully updated Qoder CLI from X to Y.` matching.
- **Process integration tests (`tests/agents_cli.rs`):** Reproduce Qian Qiao's exact `PATH` layout (`~/.qoder/entry/qoder` first on `PATH`, with standalone `~/.qoder/bin/qodercli/qodercli`, `~/.local/bin/qodercli`, npm `@qoder-ai/qodercli`, Homebrew cask, and IDE-only missing-CLI exit `127` fixtures) across `agents list`, `agents update qoder-cli --dry-run`, and `agents update qoder-cli`.

## Boundaries

- **Always:** Use TDD (write failing test first, observe expected failure, implement, refactor green); keep probes bounded (`5s`, `64 KiB`, `PROBE_ENV_OVERRIDES`); use isolated temporary directories and fake executables only; preserve `en` and `zh-CN` parity.
- **Ask first:** Adding or upgrading Cargo dependencies, changing `CONSTRAINTS.md` or CI workflows, or altering any non-Qoder agent's ownership contract.
- **Never:** Execute, install, update, or modify real coding agents on the host machine; execute shell scripts during ownership inspection; weaken conflicting-banner or Windows `.cmd` metacharacter protections; publish to crates.io, push tags, or merge into `main`.

## Success criteria

1. `teemiao agents list` detects Qoder CLI in both direct `qodercli` and `~/.qoder/entry/qoder` (and Windows `.qoder\entry\qoder.cmd`) layouts when `qodercli --help` wraps its 84-character description across multiple lines.
2. IDE-only `.qoder/entry/qoder` dispatchers without an installed `qodercli` binary (exit `127` / `"Qoder CLI is not installed"`), `Qoder IDE.app` / `Qoder.app` bundles, and non-Qoder tools named `qoder` are excluded with zero updater executions.
3. `teemiao agents update qoder-cli` (and `qoder` / `qodercli`) selects the verified native binary or exact npm prefix, and reports `manual action required` for Homebrew, prerelease npm, foreign managers, or unverified wrappers.
4. All verification commands pass with zero warnings, `teemiao --version` prints `teemiao 0.2.4`, and `git diff Cargo.lock` changes only the root `teemiao` package version.

## Open questions

None.
