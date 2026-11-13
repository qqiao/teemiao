# Changelog

All notable changes to `teemiao` are documented in this file.

## [0.2.4] — Unreleased (Prepared)

### Added

- Accept `qoder-cli` alongside `qoder` and `qodercli` as case-insensitive identifiers for `teemiao agents update`, retaining the canonical `Qoder (qoder)` output label.

### Fixed

- Detect Qoder CLI when Commander.js soft-wraps its `--help` description, while continuing to reject conflicting product banners and usage names.
- Resolve verified Qoder command dispatchers (`~/.qoder/entry/qoder` on Unix and `%USERPROFILE%\.qoder\entry\qoder.cmd` on Windows) to the underlying `qodercli` binary. Standalone updates run that binary directly; proven global npm installations retain their npm update path, while Homebrew, foreign managers, and ambiguous ownership require manual action.
- Recognize `Successfully updated Qoder CLI from <old> to <new>.` as confirmation of an update when the initial version was unknown, preserving failure, negation, and post-update version checks.

### Security

- Inspect Qoder dispatchers statically with bounded reads, without executing shell or PowerShell code; reject oversized dispatchers and symlinked or invalid `.qodercli-install-resource` markers while preserving Windows batch-argument protections.
- Exclude IDE-only dispatchers without an installed CLI, Qoder IDE bundles, and remote CLI launchers; keep unverified overrides and contradictory ownership evidence on the manual-action path.

### Documentation

- Align the [README](README.md), [CLI reference](docs/cli.md), [agent reference](docs/agents.md), and English/Simplified Chinese help with Qoder dispatcher behavior, all three identifiers, and version `0.2.4`.

### Packaging

- Prepare patch version `0.2.4` in `Cargo.toml` and the root `Cargo.lock` package entry without changing dependency versions. Publication and release tagging remain pending maintainer approval.

## [0.2.3] — Unreleased (Prepared)

### Fixed

- Allow stable standalone Codex updates when the official installer records a platform suffix in its release name and update marker; continue requiring manual action for pinned, prerelease, or unreadable channel metadata.
- Recognize GNU Linux package target suffixes and reject stale update markers after the selected Codex release changes, including replacement builds reporting the same version.
- Recognize successful standalone Codex updates that emit known non-fatal cleanup warnings while retaining the warnings in diagnostics and preserving failure and version-verification checks.

### Security

- Validate Windows batch-command arguments at every subprocess boundary, including WinGet probes and Codex installer tools.
- Require a verified Codex version and agreement between the active launcher, current selection, manifest, and automatic-update marker.
- Isolate installer downloads from curl configuration, restrict redirects to HTTPS, bound download time and script size, and prefer the system POSIX shell.
- Bound post-exit output collection, reject incomplete updater evidence, and terminate timed Unix process groups without spawning an external kill command.
- Bound ownership metadata reads and reject oversized wrapper files instead of accepting incomplete evidence.
- Upgrade `rust-i18n` from `3.1.5` to `4.2.4` (`rust-i18n = "4.2.4"`) to remove unmaintained `unsafe-libyaml 0.2.11` (`RUSTSEC-2024-0449`) and deprecated `serde_yaml` across all dependency edges while preserving English and Simplified Chinese YAML catalogs and CLI localization behavior.
- Harden `.github/workflows/rust.yml` with `contents: read` permissions, commit-pinned `actions/checkout` (`persist-credentials: false`), and a bounded `cargo-audit` lockfile job enforcing `cargo audit --deny warnings`; document the repository security review and dependency supply-chain audit in [`docs/security-review.md`](docs/security-review.md).

### Packaging

- Prepare patch version `0.2.3`, updating only the `rust-i18n` localization dependency subgraph to `4.2.4` and declaring the already-locked `libc` directly on Unix for process-group termination. Publication and release tagging remain pending maintainer approval.

## [0.2.2] — Unreleased (Prepared)

> **Status:** Prepared in repository; not yet published to [crates.io](https://crates.io/crates/teemiao) or tagged as a GitHub release until maintainer release approval.

### Documentation

- Rewrote `README.md` with installation instructions (`cargo install teemiao --locked` and source build), verified CLI usage examples, implementation status, documentation index, preserved section headings, and `Apache License 2.0` licensing text.
- Added [`docs/cli.md`](docs/cli.md) covering global flags (`-v`/`-q`, `-h`/`--help`, `-V`/`--version`), locale detection and fallback (`LC_ALL`, `LC_MESSAGES`, `LANG`, `en`/`zh-CN`), `teemiao build-info [FILE]` behavior, JSON schema (`revision`, `build_time`), prerequisites, and current error/exit-code behavior, plus the unimplemented `teemiao config-template` status.
- Added [`docs/agents.md`](docs/agents.md) covering all seven supported coding-agent CLIs (`codex`, `antigravity`/`agy`, `opencode`, `kimi`, `cursor`, `claude-code`/`claude`, `qoder`/`qodercli`), executable precedence, verified installation ownership/channel update paths, `--dry-run`, sequential bulk execution, output stream separation (`stderr` diagnostics vs. `stdout` summary), exit codes (`0`, `1`, `2`, `130`), probe bounds (`5s` / `64 KiB` / `PROBE_ENV_OVERRIDES`), non-sandbox boundaries, and troubleshooting.
- Added [`CONTRIBUTING.md`](CONTRIBUTING.md) with repository layout, development prerequisites, CI parity and local verification commands, fake-tool safety rules, and release preparation checklist.
- Added [`spec/documentation-review/audit.md`](spec/documentation-review/audit.md) recording the repository-wide documentation audit and verification evidence.
- Refined CLI help text in `locales/en.yml` and `locales/zh-CN.yml` and Rust doc comments in `src/main.rs` and `src/build_info/mod.rs` to match current runtime behavior.

### Packaging

- Prepared package version `0.2.2` in `Cargo.toml` and `Cargo.lock` without modifying dependency versions.
- Added `repository`, `documentation`, `readme`, `keywords`, and `categories` metadata and refined `description` in `Cargo.toml` so `cargo package --list --locked` and `cargo publish --dry-run --locked` succeed without missing-metadata warnings.

## Earlier Releases (`<= 0.2.1`)

Detailed changelog entries were not maintained in-tree prior to `0.2.2`. Refer to commit history and Git tags in <https://github.com/qqiao/teemiao> for changes shipped in `0.2.1` and earlier versions.
