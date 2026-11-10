# Changelog

All notable changes to `teemiao` are documented in this file.

## [0.2.3] — Unreleased (Prepared)

### Fixed

- Allow stable standalone Codex updates when the official installer records a platform suffix in its release name and update marker; continue requiring manual action for pinned, prerelease, or unreadable channel metadata.
- Recognize GNU Linux package target suffixes and reject stale update markers after the selected Codex release changes, including replacement builds reporting the same version.
- Recognize successful standalone Codex updates that emit known non-fatal cleanup warnings while retaining the warnings in diagnostics and preserving failure and version-verification checks.

### Packaging

- Prepare patch version `0.2.3` without changing dependency versions. Publication and release tagging remain pending maintainer approval.

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
