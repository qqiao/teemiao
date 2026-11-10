# Contributing to Teemiao

Thank you for contributing to Teemiao! Before writing code or documentation, read [CONSTRAINTS.md](CONSTRAINTS.md), which defines the mandatory quality, security, and testing floor for this repository.

## Repository Structure

| Path | Purpose |
|---|---|
| `src/main.rs` | CLI entry point, `clap` command router, `env_logger` initialization, and POSIX/BCP-47 locale detection (`LC_ALL`, `LC_MESSAGES`, `LANG`). |
| `src/build_info/mod.rs` | `teemiao build-info [FILE]` implementation (`BuildInfoCommand`, `BuildInfo`, `BuildInfoError`) using `gix` and `chrono`. |
| `src/agents/mod.rs` | `teemiao agents` command definitions, fixed 7-agent catalog (`AgentId`), bounded `ProcessRunner` / `RealProcessRunner` (Unix process groups and Windows Job Objects), and localized list/update output formatting. |
| `src/agents/discovery.rs` | `PATH` executable resolution, bounded `--version`/`--help` probes (`5s`, `64 KiB`), Cursor/Qoder CLI identity verification, and package-manager ownership detection (`npm`, Homebrew, WinGet, Codex standalone). |
| `src/agents/update.rs` | Immutable `UpdatePlan` selection, Codex standalone installer verification, updater execution (`256 KiB` capture limit), and post-update re-discovery/version verification. |
| `locales/en.yml`, `locales/zh-CN.yml` | English and Simplified Chinese `rust-i18n` message catalogs embedded at compile time. |
| `tests/agents_cli.rs`, `tests/support/mod.rs` | End-to-end process integration tests and isolated `TestSandbox` / compiled fake CLI tool harness. |
| `docs/cli.md`, `docs/agents.md` | User-facing CLI and AI coding-agent reference guides. |
| `spec/` | Feature specifications, implementation plans, task checklists, and audit records (`spec/agents/`, `spec/agent-improvements/`, `spec/documentation-review/`). |
| `.agents/skills/`, `skills-lock.json` | Bundled workflow skills (externally maintained upstream resources from `addyosmani/agent-skills`). |
| `.github/workflows/rust.yml` | GitHub Actions workflow running across `ubuntu-latest`, `macos-latest`, and `windows-latest`. |

## Local Prerequisites

- **Rust toolchain:** A current stable Rust toolchain (`rustc`, `cargo`) with **Rust 2024 edition** support (`edition = "2024"` in `Cargo.toml`), plus `rustfmt` and `clippy`:
  ```sh
  rustup component add rustfmt clippy
  ```
  *(Note: No separate minimum supported Rust version (MSRV) is pinned in `Cargo.toml` beyond `edition = "2024"`.)*
- **Git:** Required for version control and exercising `teemiao build-info` in disposable test repositories.

## Development Workflow and Safety Rules

1. **Follow `CONSTRAINTS.md` without exception:**
   - Do not add lint suppressions (`#[allow(...)]`), ignore or delete existing tests, swallow errors, or introduce unfinished production branches.
   - Use argument arrays (`Command::new(...).args(...)`) and validated identifiers; never construct shell command strings from user input or discovered paths.
   - Keep credentials, environment dumps, and user configuration out of source code and diagnostics.
2. **Isolated fake-tool testing only (never mutate real agents):**
   - Never run `teemiao agents update` (without `--dry-run` or an isolated fake `PATH`) or vendor installers against a developer's or user's real installed coding agents.
   - All `agents` tests and manual CLI smoke checks must run inside an isolated temporary directory (`TestSandbox` in `tests/support/mod.rs` or a disposable temp directory with `PATH` and `HOME` overridden to fake executables).
   - Validate `teemiao build-info` in a disposable temporary Git repository with a committed `HEAD` rather than overwriting files in an active workspace.
3. **Keep `en` and `zh-CN` locales paired:**
   - Whenever you add or update a help or message key, update `locales/en.yml` and `locales/zh-CN.yml` together and verify `--help` output under both `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8`.
4. **Test-Driven Development (TDD):**
   - For any behavioral change or bug fix, write a failing unit or integration test first, implement the minimal fix, and verify the test passes alongside the full suite.

## Required Verification Commands

Run these checks from the repository root before opening or updating a pull request:

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
git diff --check
```

During iterative `agents` work, you can also run focused test targets:

```sh
cargo test --locked agents
cargo test --locked --test agents_cli
```

## Continuous Integration (CI) Matrix

`.github/workflows/rust.yml` runs on pushes and pull requests to `main` across a three-OS matrix (`fail-fast: false`):

- `ubuntu-latest` (Linux)
- `macos-latest` (macOS)
- `windows-latest` (Windows)

Each job runs:
1. `rustup component add rustfmt clippy`
2. `cargo fmt --check`
3. `cargo clippy --locked --all-targets -- -D warnings`
4. `cargo build --locked --verbose`
5. `cargo test --locked --verbose`

For dependency security, install `cargo-audit 0.23.0` with `cargo install cargo-audit --version 0.23.0 --locked`, then run `cargo audit` against the project lockfile. The unmaintained localization dependency and a prepared CI hardening patch are documented in [`docs/security-review.md`](docs/security-review.md). The workflow patch still requires a maintainer credential with GitHub's `workflow` permission; the existing CI matrix does not yet run the audit automatically.

When reporting verification results on an issue or pull request, state the local OS/architecture and toolchain you actually ran, and report external GitHub Actions runs accurately (e.g., pending vs. completed per platform). Never claim a platform passed based on compilation on another OS alone.

## Release-Preparation Checklist

When preparing a version bump (such as a patch release on a branch from `main`):

1. **Update only the root package version:**
   - Bump `version` under `[package]` in `Cargo.toml`.
   - Update only the root `name = "teemiao"` package entry in `Cargo.lock` (do not run a broad `cargo update` that refreshes transitive dependency versions unless a dependency change was separately approved).
   - Reconcile current version assertions (such as `--version` checks in `tests/agents_cli.rs` and current version references in docs), while preserving historical version references in `spec/` records.
2. **Update `CHANGELOG.md`:**
   - Add a concise section for the new version summarizing user-visible and package-metadata changes without inventing unverified historical release notes or claiming the crate is already published on `crates.io`.
3. **Commit intended changes before packaging checks:**
   - `cargo package` and `cargo publish --dry-run` require a clean working tree. Do **not** pass `--allow-dirty` or `--no-verify`.
4. **Run the complete release verification suite:**
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
   *(If your local `~/.cargo/config.toml` replaces `crates-io` with a mirror via `replace-with`, pass `--registry crates-io` to `cargo publish --dry-run --locked --registry crates-io` or run `cargo package --locked` so Cargo verifies the packaged archive against `crates.io`.)*
5. **Inspect packaged assets and warnings:**
   - Confirm `cargo package --list --locked` and `cargo publish --dry-run --locked` emit no missing-metadata warnings and include `Cargo.toml`, `Cargo.lock`, `LICENSE`, `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `docs/`, `locales/`, `src/`, and `tests/`.
6. **Human-owned publication and merge:**
   - Open or update a pull request targeting `main`. Do not run `cargo publish` without `--dry-run`, do not push release tags, and do not merge the pull request unless explicitly instructed by the repository maintainer.
