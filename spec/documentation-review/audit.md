# Documentation Review Audit & Verification Record

- **Feature**: `documentation-review` (`QQ-10`)
- **Baseline**: `main` at `bae5920599a572e3ee728a5f75bce6279536ad2d` (`0.2.1`)
- **Target Version**: `0.2.2`
- **Status**: Implemented (awaiting independent Leader re-review `T006` and Human acceptance `T007`)

---

## 1. Tracked Documentation Surface Inventory (`FR-001`)

Every tracked documentation surface in the repository (`git ls-files`) was audited against the `0.2.1` baseline (`bae5920599a572e3ee728a5f75bce6279536ad2d`), current source code, CLI runtime behavior, and test suite.

| Surface | Disposition | Baseline Findings & Evidence |
| --- | --- | --- |
| `README.md` | `corrected` | At baseline `bae5920`, `README.md` already documented `teemiao agents` (`list`, `update [--dry-run]`, the 7-agent table, discovery/safety notes, exit codes `0`/`1`/`2`/`130`, and troubleshooting), but had six concrete gaps/inaccuracies: (1) `### Build Information Generator (build-info)` claimed `"Additional contextual build data"` without documenting positional `[FILE]` syntax (`teemiao build-info [FILE]`), the default `./build_info.json` path, the exact 2-field JSON schema (`revision` string + `build_time` Unix seconds `i64`), the `cwd` Git repository / valid `HEAD` / existing parent-directory requirements, overwrite behavior, or the current limitation that generation errors print to `stderr` while returning exit code `0`; (2) it omitted global `-v`/`-q` verbosity flags, locale resolution precedence (`LC_ALL`, `LC_MESSAGES`, `LANG`) with `en`/`zh-CN` catalogs and `"en"` fallback, and the unfinished `teemiao config-template` subcommand (`todo!("config template")` panics when invoked); (3) its bounded-probe summary mentioned only `CI=1` and `DISABLE_AUTOUPDATER=1` without clarifying that probing an external binary on `PATH` is not an OS sandbox or universal no-network guarantee; (4) `### Installation` showed only `cargo install teemiao` without source build instructions; (5) `## Contributing` referenced contributing guidelines that did not exist in the repository; and (6) `## License` named `"Apache Public License 2.0 (APL-2)"` instead of `"Apache License 2.0"` (`Apache-2.0`) and lacked repository, issue-tracker, and `LICENSE` links. Rewrote `README.md` as a concise entry point with a `## Documentation Index` linking `docs/cli.md`, `docs/agents.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, and `CONSTRAINTS.md`, while preserving all 14 existing section headings/anchors from `bae5920` (`#teemiao`, `#features`, `#build-information-generator-build-info`, `#ai-coding-agent-cli-manager-agents`, `#supported-agents-and-update-methods`, `#discovery-and-safety-boundaries`, `#current-status`, `#getting-started`, `#installation`, `#usage`, `#exit-codes-teemiao-agents`, `#troubleshooting`, `#contributing`, `#license`). |
| `docs/cli.md` | `corrected` (created) | Added CLI reference covering `teemiao [OPTIONS] <COMMAND>`, global `-v`/`-q` verbosity mapping (`Warn` default, `Info`, `Debug`, `Trace`, `Error`, `Off`), locale resolution precedence (`LC_ALL` -> `LC_MESSAGES` -> `LANG`), POSIX locale normalization (`parse_locale_string` in `src/main.rs:103-122`), `en`/`zh-CN` catalogs and `"en"` fallback, `teemiao build-info [FILE]` positional syntax, `./build_info.json` default, `cwd` Git repository requirement (`gix::open`), valid `HEAD` commit requirement, existing parent directory requirement, overwrite behavior, exact 2-field JSON schema (`revision` string + `build_time` Unix seconds `i64`), current limitation that `build-info` errors print to `stderr` while returning exit code `0` (`src/main.rs:156-161`), and `teemiao config-template` unimplemented panic status (`todo!("config template")` at `src/main.rs:162-164`). |
| `docs/agents.md` | `corrected` (created) | Added reference for `teemiao agents list` and `teemiao agents update [AGENT] [--dry-run]`: all 7 supported agents in catalog order (`codex`, `antigravity`/`agy`, `opencode`, `kimi`, `cursor`, `claude-code`/`claude`, `qoder`/`qodercli`), executable lookup order (`src/agents/mod.rs:107-179`), Cursor `agent` vs. `cursor-agent` disambiguation (`src/agents/discovery.rs:281-319`), verified installation ownership and release-channel preservation (`npm` global prefix, `Homebrew` cask/formula, `WinGet`, native updaters, Codex standalone installer), absent vs. unsupported installations (`manual action required`), project-local `node_modules/.bin` discovery vs. update behavior (`agents list` discovers and lists project-local `node_modules/.bin` executables on `PATH` with `update: manual action required (...)` and exits `0`, whereas `detect_npm_ownership` / `is_local_node_modules_bin_launcher` at `src/agents/discovery.rs:2939`, `:3027-3038` excludes them from global npm ownership so `agents update [<agent>] [--dry-run]` reports `manual action required` and exits `1`), `--dry-run` guarantees, sequential bulk execution in catalog order, `stderr` section headers (`--- [i/n] Name (id) ---`) vs. `stdout` `Update summary`, exit codes (`0`, `1`, `2`, `130`), bounded probe execution (`5s` timeout, `64 KiB` per stream, `PROBE_ENV_OVERRIDES`), non-sandbox boundary for external binaries, and troubleshooting. |
| `CONTRIBUTING.md` | `corrected` (created) | Added contributor and maintainer guide covering repository layout, Rust Edition 2024 toolchain prerequisites, development safety rules aligned with `CONSTRAINTS.md`, CI parity with `.github/workflows/rust.yml` (`ubuntu-latest`, `macos-latest`, `windows-latest`), local verification commands, fake-tool test safety rules (`tests/support/mod.rs`), and the release-preparation checklist. |
| `CHANGELOG.md` | `corrected` (created) | Added concise `0.2.2` entry describing the documentation review, CLI help/doc refinements, and Cargo manifest metadata updates without reconstructing unverified release history for `<= 0.2.1` or claiming `0.2.2` is already published. |
| `Cargo.toml` | `corrected` | At baseline `bae5920`, `cargo package --list --locked` printed `warning: manifest has no documentation, homepage or repository`. Bumped root package `version` from `"0.2.1"` to `"0.2.2"`; refined `description`; added `repository`, `documentation`, `readme`, `keywords`, and `categories` (`FR-007`, `FR-008`). Retained `license = "Apache-2.0"` and all dependency specifications unchanged. |
| `Cargo.lock` | `corrected` | Updated only the root `teemiao` package `version` from `"0.2.1"` to `"0.2.2"` (`Cargo.lock:2215`) without changing any dependency versions or checksums (`FR-008`). |
| `locales/en.yml` | `corrected` | Updated `commands.build_info.long_about` to name the exact `revision` and `build_time` JSON fields instead of open-ended `"such as..."`; updated `commands.config_template.about` and `long_about` to state `(not implemented)` and that invoking it panics; updated `commands.agents.list.long_about` to describe bounded local version/help probes and package metadata without claiming OS-level network sandboxing; aligned `agents.update.agent.long_help` with `zh-CN.yml` by specifying `"sequentially in catalog order"`. |
| `locales/zh-CN.yml` | `corrected` | Applied paired Simplified Chinese updates for `commands.build_info.long_about`, `commands.config_template.about`, `commands.config_template.long_about`, and `commands.agents.list.long_about` matching `locales/en.yml`. |
| `src/main.rs` | `corrected` | Updated `Commands::ConfigTemplate` doc comment (`src/main.rs:73`) to note `(currently not implemented; invoking it panics)`. Verified crate-level and locale-detection rustdoc comments (`src/main.rs:15-140`) match implementation and pass `cargo doc --locked --no-deps`. |
| `src/build_info/mod.rs` | `corrected` | Updated `BuildInfoCommand` rustdoc comment (`src/build_info/mod.rs:52-58`) to describe the exact `revision` and `build_time` JSON fields and repository/parent-directory prerequisites. |
| `src/agents/mod.rs` | `accurate` | Audited module and type docstrings (`AgentId`, `AgentsCommand`, `AgentsSubcommand`, `UpdateArgs`, `AgentSpec`, `CATALOG`) against `clap` attributes and catalog behavior; all 7 agents, aliases, and value help strings are accurate. |
| `src/agents/discovery.rs` | `accurate` | Audited module and function docstrings (`PROBE_TIMEOUT`, `MAX_PROBE_BYTES`, `PROBE_ENV_OVERRIDES`, ownership classification, `is_local_node_modules_bin_launcher`, Cursor disambiguation, Codex/Claude/OpenCode/Kimi/Qoder detection) against runtime implementation. |
| `src/agents/update.rs` | `accurate` | Audited module and function docstrings (sequential updates, `--dry-run`, exit code precedence `130` > `1` > `0`, post-update verification) against runtime implementation. |
| `tests/agents_cli.rs` | `corrected` | Updated CLI version assertion at `tests/agents_cli.rs:86` from `"teemiao 0.2.1"` to `"teemiao 0.2.2"` (`FR-008`). All 28 integration tests pass. |
| `tests/support/mod.rs` | `accurate` | Audited test sandbox documentation (`TestSandbox`, fake executable generation, isolated `HOME`/`APPDATA`/`LOCALAPPDATA` environment overrides). |
| `LICENSE` | `accurate` | Standard Apache License, Version 2.0 text; matches `Cargo.toml` (`license = "Apache-2.0"`), source file headers, and `README.md` (`Apache License 2.0`). |
| `CONSTRAINTS.md` | `accurate` | Quality constraints approved for `QQ-8` by Qian Qiao on 2026-11-08 (`01a1b7a7-3876-7273-a84b-19ab22057240`), defining `## Floor` (no new lint suppressions, ignored/deleted tests, swallowed errors, or unfinished production branches; no credentials/env dumps/user config in source or diagnostics; argument arrays and validated agent IDs without shell command construction; isolated fake executables and fake package-manager records without mutating real agents; preserving CLI behavior and English/Chinese localization), `## Delivery checks` (`cargo fmt --check`, `cargo build --locked`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, fake-process safety tests, and macOS/Linux/Windows CI reporting rules), and `## Baseline` measurements at `2bf8dbdadb546fef75af3d1ede1814b142e0471c` plus the 5s / 64 KiB probe limits. Preserved unchanged and linked from `README.md` and `CONTRIBUTING.md`. |
| `AGENTS.md` / `CLAUDE.md` | `accurate` | Tracked repository agent instruction files (`AGENTS.md` contains `Read CONSTRAINTS.md before writing code. Do not weaken it to make a change pass.`, and `CLAUDE.md` is a symlink to `AGENTS.md`). Preserved unchanged. |
| `.github/workflows/rust.yml` | `accurate` | CI workflow running `build`, `test`, `clippy` (`-D warnings`), and `fmt` (`--check`) across `ubuntu-latest`, `macos-latest`, and `windows-latest` (`fail-fast: false`). Documented accurately in `CONTRIBUTING.md`. |
| `spec/agents/spec.md`, `spec/agents/plan.md`, `spec/agents/research.md`, `spec/agents/tasks.md` | `historical` | Historical feature specification, plan, upstream vendor research (`research.md`, checked 2026-11-08 for the initial 6 agents), and task checklist for `QQ-8` (`0.2.0`). `spec.md` and `plan.md` record Qian Qiao's 2026-11-08 approval; `tasks.md` has `T001`–`T008` checked (`[x]`, including `T001` human design approval in comment `01a1b7a7-3876-7273-a84b-19ab22057240` and `T008` independent leader review of `07a2ff4` / PR #25) and `T009` (`Human review of the final delivery`) unchecked (`[ ]`). Preserved unmodified. |
| `spec/agent-improvements/spec.md`, `spec/agent-improvements/plan.md`, `spec/agent-improvements/tasks.md` | `historical` (reconciled by Leader in `4558261`) | Historical feature package for `QQ-9` (`0.2.1`, merged in `bae5920599a572e3ee728a5f75bce6279536ad2d` via PR #26). At baseline `bae5920`, `spec.md:5` and `plan.md:5` still carried draft status headers (`Draft; awaiting Qian Qiao's explicit approval...` / `Draft; implementation is not yet authorized`), and `tasks.md` had `T001`–`T004` (plus `Checkpoint`) checked (`[x]`) while `T005` (`Independent adversarial review and PR delivery`) and `T006` (`Human acceptance`) were unchecked (`[ ]`). Reconciled by the Leader in commit `4558261ab763dfb9184e5db79da41ff345247e80` with authoritative `QQ-9` review/acceptance comments (`01a1c40f-1860-71a9-8b78-0e02491b28e3`, `01a1c65b-36aa-7116-ac56-db89f71e68e2`, `01a1c66c-7fd1-7e8d-8022-77eead15020b`). |
| `spec/documentation-review/spec.md`, `spec/documentation-review/plan.md`, `spec/documentation-review/tasks.md`, `spec/documentation-review/audit.md` | `corrected` | Active feature package for `QQ-10`. Leader commit `4558261ab763dfb9184e5db79da41ff345247e80` recorded `T001` approval (`01a1c666-6298-7a14-bd34-e3bff5f26445`) and `T006` first-review findings; `audit.md` and `tasks.md` record the Coder's implementation and verification evidence. |
| `.agents/skills/**` & `skills-lock.json` | `externally maintained` | Bundled workflow skill definitions (`33` files under `.agents/skills/`) and `skills-lock.json` from `addyosmani/agent-skills`; reviewed as externally maintained workflow resources and retained unmodified. |

---

## 2. Historical Specification Status Summary (`FR-001`)

1. **`QQ-8` (`spec/agents/{spec.md,plan.md,research.md,tasks.md}`)**:
   - `spec.md` and `plan.md` record Qian Qiao's 2026-11-08 approval (`01a1b7a7-3876-7273-a84b-19ab22057240`).
   - `research.md` records upstream CLI update research from 2026-11-08.
   - `tasks.md` has `T001`–`T008` checked (`[x]`) and `T009` (`Human review of the final delivery`) unchecked (`[ ]`); left unchanged as historical record.
2. **`QQ-9` (`spec/agent-improvements/{spec.md,plan.md,tasks.md}`)**:
   - At baseline `bae5920599a572e3ee728a5f75bce6279536ad2d`, `spec.md:5` and `plan.md:5` still carried `Draft` status headers and `tasks.md` had `T001`–`T004` (and `Checkpoint`) checked (`[x]`) with `T005` and `T006` unchecked (`[ ]`).
   - Left unmodified by the Coder and reconciled by the Leader in commit `4558261ab763dfb9184e5db79da41ff345247e80` citing `QQ-9` comments `01a1c40f-1860-71a9-8b78-0e02491b28e3` (`T005` leader review of `a974292`) and `01a1c65b-36aa-7116-ac56-db89f71e68e2` / `01a1c66c-7fd1-7e8d-8022-77eead15020b` (`T006` human merge confirmation and closure authorization).

---

## 3. User Story & Requirement Traceability (`US1`–`US3`, `FR-001`–`FR-009`)

| Story / Requirement | Implementation Artifact(s) | Verification Evidence |
| --- | --- | --- |
| **US1 (P1)** — Use the CLI confidently | `README.md`, `docs/cli.md`, `docs/agents.md`, `locales/en.yml`, `locales/zh-CN.yml` | Verified installation commands, `--help` in `en_US.UTF-8` and `zh_CN.UTF-8`, disposable `build-info` runs, and isolated fake-agent `list` / `update --dry-run` checks |
| **US2 (P1)** — Identify the package | `Cargo.toml`, `README.md`, `LICENSE` | Verified via `cargo metadata --locked --no-deps --format-version 1`, `cargo package --list --locked`, and `cargo publish --dry-run --locked --registry crates-io` (zero missing-metadata warnings; archive builds and compiles) |
| **US3 (P2)** — Contribute and prepare a patch | `CONTRIBUTING.md`, `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, `tests/agents_cli.rs:86` | Verified `teemiao 0.2.2` across `Cargo.toml`, `Cargo.lock:2215`, `CHANGELOG.md`, and `cargo run --locked -- --version`, plus all 13 verification commands in `spec.md` |
| **FR-001** (Audit coverage) | `spec/documentation-review/audit.md` (Sections 1 & 2) | Verified against `git ls-files` and baseline `bae5920599a572e3ee728a5f75bce6279536ad2d` |
| **FR-002** (`README.md` entry point, install, examples, index, status, links, preserved headings, `Apache License 2.0`) | `README.md` | Verified all 14 preserved section headings from `bae5920`, relative links, and CLI examples |
| **FR-003** (`build-info` positional `[FILE]`, `./build_info.json` default, `cwd` Git repo + valid `HEAD`, existing parent dir, overwrite, 2-field JSON schema `revision` + `build_time`, error `stderr` + exit code `0`) | `docs/cli.md`, `README.md`, `locales/en.yml`, `locales/zh-CN.yml`, `src/build_info/mod.rs` | Verified in disposable Git repo (`mktemp -d`) and non-repo / missing-parent cases |
| **FR-004** (`agents` 7-agent catalog, binary precedence, verified ownership/channels, `--dry-run`, sequential order, `stderr`/`stdout` split, exit codes `0`/`1`/`2`/`130`, empty selection, troubleshooting, `5s`/`64 KiB`/`PROBE_ENV_OVERRIDES`, non-sandbox boundary, absent vs. `manual action required`, project-local `node_modules/.bin` discovery vs. update behavior) | `docs/agents.md`, `README.md`, `locales/en.yml`, `locales/zh-CN.yml` | Verified against `src/agents/{mod.rs,discovery.rs,update.rs}`, `tests/agents_cli.rs`, and isolated fake-tool sandboxes (including project-local `node_modules/.bin/codex` fixture) |
| **FR-005** (Global `-v`/`-q`, locale precedence `LC_ALL` -> `LC_MESSAGES` -> `LANG`, `en`/`zh-CN` fallback to `"en"`, `config-template` unimplemented panic, paired locale updates) | `docs/cli.md`, `locales/en.yml`, `locales/zh-CN.yml`, `src/main.rs` | Verified `--help` in `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8` across all subcommands |
| **FR-006** (`CONTRIBUTING.md` and `CHANGELOG.md` for `0.2.2`) | `CONTRIBUTING.md`, `CHANGELOG.md` | Verified links, commands, and unpublished `0.2.2` status note |
| **FR-007** (`Cargo.toml` metadata: `description`, `repository`, `documentation`, `readme`, `keywords`, `categories`) | `Cargo.toml` | Verified zero missing-metadata warnings in `cargo package --list --locked` and `cargo publish --dry-run --locked --registry crates-io` |
| **FR-008** (Patch version `0.2.2` in `Cargo.toml`, `Cargo.lock`, and active version assertions without dependency changes) | `Cargo.toml`, `Cargo.lock`, `tests/agents_cli.rs:86` | Verified via `cargo run --locked -- --version` (`teemiao 0.2.2`) and `git diff bae5920..HEAD -- Cargo.lock` |
| **FR-009** (Verification commands, isolated examples, both locales, packaged assets, self-review gates) | `spec/documentation-review/audit.md` (Sections 4 & 5), `spec/documentation-review/tasks.md` | All required checks pass |

---

## 4. Command & Runtime Verification Evidence (`FR-009`)

Executed locally on macOS (`Darwin arm64`, `rustc 1.99.1`, `cargo 1.99.1`):

1. `cargo fmt --check` -> exit `0`
2. `cargo clippy --locked --all-targets -- -D warnings` -> exit `0` (0 warnings)
3. `cargo test --locked` -> exit `0`:
   - Unit tests (`src/main.rs`, `src/agents/*`): `39 passed; 0 failed; 0 ignored`
   - Integration tests (`tests/agents_cli.rs`): `28 passed; 0 failed; 0 ignored` (including `agents_without_subcommand_displays_help_and_exits_2` asserting `teemiao 0.2.2`)
4. `cargo build --locked` -> exit `0`
5. `cargo doc --locked --no-deps` -> exit `0` (0 rustdoc warnings)
6. `cargo metadata --locked --no-deps --format-version 1` -> exit `0` (`"name":"teemiao","version":"0.2.2"`)
7. `cargo run --locked -- --version` -> exit `0` (`teemiao 0.2.2`)
8. `cargo run --locked -- --help` -> exit `0` (verified in both `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8`)
9. `cargo run --locked -- build-info --help` -> exit `0` (verified in both `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8`)
10. `cargo run --locked -- agents update --help` -> exit `0` (verified in both `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8`; also verified `config-template --help`, `agents --help`, and `agents list --help`)
11. `cargo package --list --locked` -> exit `0` (69 packaged files including `Cargo.toml`, `Cargo.lock`, `LICENSE`, `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `docs/cli.md`, `docs/agents.md`, `locales/en.yml`, `locales/zh-CN.yml`, `src/**`, and `tests/**`; 0 missing-metadata warnings)
12. `cargo publish --dry-run --locked` -> on this host, `~/.cargo/config.toml` replaces `crates-io` with the non-remote-registry mirror `aliyun`, so bare `cargo publish --dry-run --locked` exits `101` with `error: crates-io is replaced with non-remote-registry source registry aliyun; include --registry crates-io to use crates.io`. Running `cargo publish --dry-run --locked --registry crates-io` exits `0`, packaging 69 files (`1.1MiB` / `314.6KiB` compressed) and verifying/compiling `target/package/teemiao-0.2.2` cleanly with 0 missing-metadata warnings (`warning: aborting upload due to dry run`).
13. `git diff --check` -> exit `0` (no whitespace or conflict marker issues)

### Isolated Disposable Sandbox Verification

- **`build-info` default & custom paths**: In a temporary Git repository (`mktemp -d`) with an initial commit, `teemiao build-info` wrote `./build_info.json` with keys `revision` (short hash) and `build_time` (`i64` Unix seconds), and `teemiao -v build-info custom/build_info.json` logged `INFO` progress to `stderr` and wrote `custom/build_info.json`.
- **`build-info` error and exit code behavior**: Running `teemiao build-info missing_parent/build_info.json` printed `Error generating build info: Failed to write to output file: No such file or directory (os error 2)` to `stderr` and exited with code `0`. Running `teemiao build-info` in a non-Git directory printed `Error generating build info: Failed to open repository: ... does not appear to be a git repository` to `stderr` and exited with code `0`.
- **Fake-agent `agents list` & `agents update --dry-run` in `en_US.UTF-8` and `zh_CN.UTF-8`**:
  - Empty `PATH`: printed `No supported AI coding-agent CLIs were found on PATH.` / `No supported AI coding-agent CLIs were found on PATH; nothing to update.` on `stdout` and exited `0`.
  - Fake `agy` script on isolated `PATH`: `agents list` reported `Antigravity (antigravity) | version: 1.4.2 | path: .../agy | update: native (agy update)` (`更新方式: 原生自更新 (agy update)` in `zh_CN.UTF-8`), and `agents update agy --dry-run` wrote `--- [1/1] Antigravity (antigravity) ---` to `stderr` and `Update summary` / `更新摘要` to `stdout` with exit code `0`.
  - Project-local `node_modules/.bin/codex` fixture on isolated `PATH` (`project/node_modules/.bin/codex` -> `../@openai/codex/bin/codex.js` with `project/node_modules/@openai/codex/package.json`):
    - `teemiao agents list` discovered the launcher and printed `Codex (codex) | version: 0.118.0 | path: .../project/node_modules/.bin/codex | update: manual action required (could not verify installation owner (not a proven npm, Homebrew, or standalone Codex installation))` on `stdout` with exit code `0`.
    - `teemiao agents update codex --dry-run` wrote `--- [1/1] Codex (codex) ---` and `manual action required (...)` to `stderr` and `Update summary` on `stdout`, exiting with code `1`.

---

## 5. Self-Review Gates (`FR-009`)

### 5.1 `test-driven-development` & `doubt-driven-development`
- Updated `tests/agents_cli.rs:86` to expect `"teemiao 0.2.2"` first and confirmed the test failed (`left: "teemiao 0.2.1"`, `right: "teemiao 0.2.2"`) before bumping `Cargo.toml` and `Cargo.lock` to `0.2.2` and re-running to green.
- Verified every claim in `README.md`, `docs/cli.md`, `docs/agents.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, and `audit.md` against `bae5920599a572e3ee728a5f75bce6279536ad2d`, current source code (`src/main.rs`, `src/build_info/mod.rs`, `src/agents/{mod.rs,discovery.rs,update.rs}`), and live CLI execution in disposable sandboxes (including the project-local `node_modules/.bin/codex` fixture).

### 5.2 `code-review-and-quality`
- Verified all 30 relative links and section fragments across `README.md`, `docs/cli.md`, `docs/agents.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `CONSTRAINTS.md`, `LICENSE`, and `spec/documentation-review/audit.md`, and preserved all 14 section headings from `bae5920:README.md`.
- Confirmed `locales/en.yml` and `locales/zh-CN.yml` remain in lockstep across all message keys.

### 5.3 `code-simplification`
- Kept `README.md` concise as an entry point while placing detailed command, agent-matrix, and maintainer documentation in dedicated files (`docs/cli.md`, `docs/agents.md`, `CONTRIBUTING.md`) to avoid duplicating large reference tables.

### 5.4 `security-and-hardening`
- Explicitly documented that `teemiao` is not an OS sandbox and cannot prevent a binary on `PATH` from performing network or filesystem actions when invoked with `--version` or `--help`, while documenting the bounded probe controls (`5s` timeout, `64 KiB` stream cap, `PROBE_ENV_OVERRIDES`).
- All verification used isolated fake scripts in temporary directories (`mktemp -d`); no real agent installers or self-updaters were executed.

### 5.5 `performance-optimization`
- Confirmed zero runtime overhead was introduced: Rust code edits were limited to doc comments and localized static help strings, and the `Cargo.lock` dependency graph remained unchanged.
