# Documentation Review Audit & Verification Record

- **Feature**: `documentation-review` (`QQ-10`)
- **Target Version**: `0.2.2`
- **Status**: Implemented (awaiting independent Leader review `T006` and Human approval `T007`)

---

## 1. Tracked Documentation Surface Inventory (`FR-001`, `SC-001`)

Every tracked documentation surface in the repository was audited against the current codebase, CLI runtime behavior, and test suite.

| Surface | Disposition | Findings & Evidence |
| --- | --- | --- |
| `README.md` | `corrected` | Replaced outdated `0.1.0` documentation (`--out`/`-o` flags that do not exist, 5-field `build_info.json` schema with unimplemented `version`/`build_timestamp`/`git_branch`/`git_dirty` fields, unimplemented `TEEMIAO_LOG`/`TEEMIAO_CONFIG`/`~/.config/teemiao/config.toml` claims, non-existent `CONTRIBUTING.md`/`CHANGELOG.md` links, `cargo test --all-features` and `cargo run --example basic_usage` commands that had no targets, and complete omission of `teemiao agents`). Rewrote as a concise entry point with `cargo install teemiao` and source build instructions, real CLI examples, current implementation status, documentation index (`docs/cli.md`, `docs/agents.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `CONSTRAINTS.md`), preserved legacy anchors (`#usage`, `#build-information-generation`, `#command-line-options`, `#configuration`), and `Apache License 2.0`. |
| `docs/cli.md` | `corrected` (created) | Added complete CLI reference covering `teemiao [OPTIONS] <COMMAND>`, global `-v`/`-q` verbosity mapping (`Warn` default, `Info`, `Debug`, `Trace`, `Error`, `Off`), locale resolution precedence (`LC_ALL` -> `LC_MESSAGES` -> `LANG`), POSIX locale normalization (`parse_locale_string` in `src/main.rs:103-122`), `en`/`zh-CN` catalogs and `"en"` fallback, `teemiao build-info [FILE]` positional syntax, `./build_info.json` default, `cwd` Git repository requirement (`gix::open`), valid `HEAD` commit requirement, existing parent directory requirement, overwrite behavior, exact 2-field JSON schema (`revision` string + `build_time` Unix seconds `i64`), current limitation that `build-info` errors print to `stderr` while returning exit code `0` (`src/main.rs:156-161`), and `teemiao config-template` unimplemented panic status (`todo!("config template")` at `src/main.rs:162-164`). |
| `docs/agents.md` | `corrected` (created) | Added complete reference for `teemiao agents list` and `teemiao agents update [AGENT] [--dry-run]`: all 7 supported agents in catalog order (`codex`, `antigravity`/`agy`, `opencode`, `kimi`, `cursor`, `claude-code`/`claude`, `qoder`/`qodercli`), executable lookup order (`src/agents/mod.rs:107-179`), Cursor `agent` vs. `cursor-agent` disambiguation (`src/agents/discovery.rs:281-319`), verified installation ownership and release-channel preservation (`npm`, `Homebrew` cask/formula, `WinGet`, native updaters, standalone installers), absent vs. unsupported installations (`manual action required`), `--dry-run` guarantees, sequential bulk execution in catalog order, `stderr` section headers (`--- [i/n] Name (id) ---`) vs. `stdout` `Update summary`, exit codes (`0`, `1`, `2`, `130`), bounded probe execution (`5s` timeout, `64 KiB` per stream, `PROBE_ENV_OVERRIDES`), non-sandbox boundary for external binaries, and troubleshooting. |
| `CONTRIBUTING.md` | `corrected` (created) | Added maintainer and contributor guide covering repository layout, Rust Edition 2024 toolchain prerequisites, CI parity with `.github/workflows/rust.yml` (`ubuntu-latest`, `macos-latest`, `windows-latest`), local verification commands, fake-tool test safety rules (`tests/support/mod.rs`), and the `0.2.2` release preparation checklist. |
| `CHANGELOG.md` | `corrected` (created) | Added concise `0.2.2` entry describing the documentation review, CLI help/doc refinements, and Cargo manifest metadata updates without inventing detailed release history for `<= 0.2.1` or claiming `0.2.2` is already published. |
| `Cargo.toml` | `corrected` | Bumped root package `version` from `"0.2.1"` to `"0.2.2"`; refined `description`; added `repository`, `documentation`, `readme`, `keywords`, and `categories` (`FR-007`, `FR-008`). Verified zero missing-metadata warnings from `cargo package --list --locked` and `cargo publish --dry-run --locked`. Dependency versions were not modified. |
| `Cargo.lock` | `corrected` | Updated root `teemiao` package `version` from `"0.2.1"` to `"0.2.2"` (`Cargo.lock:2215`) without changing any dependency versions or checksums (`FR-008`). |
| `locales/en.yml` | `corrected` | Updated `commands.build_info.long_about` to name the exact `revision` and `build_time` JSON fields instead of open-ended `"such as..."`; updated `commands.config_template.about` and `long_about` to state `(not implemented)` and that invoking it panics; updated `commands.agents.list.long_about` to describe bounded local version/help probes and package metadata without claiming OS-level network sandboxing; aligned `agents.update.agent.long_help` with `zh-CN.yml` by specifying `"sequentially in catalog order"`. |
| `locales/zh-CN.yml` | `corrected` | Applied paired Simplified Chinese updates for `commands.build_info.long_about`, `commands.config_template.about`, `commands.config_template.long_about`, and `commands.agents.list.long_about` matching `locales/en.yml`. |
| `src/main.rs` | `corrected` | Updated `Commands::ConfigTemplate` doc comment (`src/main.rs:73`) to note `(currently not implemented; invoking it panics)`. Verified crate-level and locale-detection rustdoc comments (`src/main.rs:15-140`) match implementation and pass `cargo doc --locked --no-deps`. |
| `src/build_info/mod.rs` | `corrected` | Updated `BuildInfoCommand` rustdoc comment (`src/build_info/mod.rs:52-58`) to describe the exact `revision` and `build_time` JSON fields and repository/parent-directory prerequisites. |
| `src/agents/mod.rs` | `accurate` | Audited module and type docstrings (`AgentId`, `AgentsCommand`, `AgentsSubcommand`, `UpdateArgs`, `AgentSpec`, `CATALOG`) against `clap` attributes and catalog behavior; all 7 agents, aliases, and value help strings are accurate. |
| `src/agents/discovery.rs` | `accurate` | Audited module and function docstrings (`PROBE_TIMEOUT`, `MAX_PROBE_BYTES`, `PROBE_ENV_OVERRIDES`, ownership classification, Cursor disambiguation, Codex/Claude/OpenCode/Kimi/Qoder detection) against runtime implementation. |
| `src/agents/update.rs` | `accurate` | Audited module and function docstrings (sequential updates, `--dry-run`, exit code precedence `130` > `1` > `0`, post-update verification) against runtime implementation. |
| `tests/agents_cli.rs` | `corrected` | Updated CLI version assertion at `tests/agents_cli.rs:86` from `"teemiao 0.2.1"` to `"teemiao 0.2.2"` (`FR-008`). All 28 integration tests pass. |
| `tests/support/mod.rs` | `accurate` | Audited test sandbox documentation (`TestSandbox`, fake executable generation, isolated `HOME`/`APPDATA`/`LOCALAPPDATA` environment overrides). |
| `LICENSE` | `accurate` | Standard Apache License, Version 2.0 text; matches `Cargo.toml` (`license = "Apache-2.0"`), source file headers, and `README.md` (`Apache License 2.0`). |
| `CONSTRAINTS.md` | `accurate` | Repository-level engineering constraints (SOLID, CLI UX, i18n, dependencies, git hygiene); linked from `README.md` and `CONTRIBUTING.md`. |
| `AGENTS.md` / `CLAUDE.md` | `externally maintained` | Squad workflow instructions (`CLAUDE.md` is a symlink to `AGENTS.md`); governed by `CONSTRAINTS.md` and not modified by product documentation changes. |
| `.github/workflows/rust.yml` | `accurate` | CI workflow running `build`, `test` (`ubuntu-latest`, `macos-latest`, `windows-latest`), `clippy` (`-D warnings`), and `fmt` (`--check`). Documented accurately in `CONTRIBUTING.md`. |
| `spec/agents/{spec.md,plan.md,tasks.md}` | `historical` | Historical feature specification package for initial `teemiao agents` implementation (`0.2.0`). Preserved unmodified as historical engineering record. Note for Leader: `spec/agents/tasks.md` has unchecked `T001` (Human), `T011` (Leader), and `T012` (Human) checkboxes from the historical workflow. |
| `spec/agent-improvements/{spec.md,plan.md,tasks.md}` | `historical` | Historical feature specification package for `0.2.1` agent improvements. Preserved unmodified per `FR-001` and `plan.md`. Flagged below in Section 2 for Leader annotation (`spec.md` and `plan.md` still carry `Status: Draft (awaiting human review)`, and `tasks.md` has unchecked `T001`, `T008`, `T009`). |
| `spec/documentation-review/{spec.md,plan.md,tasks.md,audit.md}` | `corrected` | Active feature package for `QQ-10`. Added `audit.md` and updated `tasks.md` with implementation and verification evidence for `T002`–`T005`. |
| `.agents/skills/*` & `skills-lock.json` | `externally maintained` | Installed agent skill definitions and lockfile; read-only reference material, untouched. |

---

## 2. Historical Specification Flags for Leader Annotation (`FR-001`)

Per `spec/documentation-review/spec.md` (`FR-001`) and `spec/documentation-review/plan.md`, historical specification files were **not** modified by the Coder. The following stale status fields and unchecked historical human/leader tasks are flagged for the Leader:

1. `spec/agent-improvements/spec.md:5` — Header still reads `**Status**: Draft (awaiting human review)` even though the `0.2.1` implementation was merged to `main` in commit `bae5920599a572e3ee728a5f75bce6279536ad2d`.
2. `spec/agent-improvements/plan.md:5` — Header still reads `**Status**: Draft (awaiting human review)`.
3. `spec/agent-improvements/tasks.md` — Historical tasks `T001` (Human approval of spec/plan/tasks), `T008` (Leader independent review), and `T009` (Human final approval) remain unchecked (`[ ]`) even though `T002`–`T007` are `[x]` and merged.
4. `spec/agents/tasks.md` — Historical tasks `T001` (Human approval), `T011` (Leader review), and `T012` (Human final approval) remain unchecked (`[ ]`) while `T002`–`T010` are `[x]` and merged.

---

## 3. Requirement & Success Criteria Traceability (`FR-001`–`FR-009`, `SC-001`–`SC-005`)

| Requirement / Criterion | Implementation Artifact(s) | Verification Status |
| --- | --- | --- |
| **FR-001** (Audit coverage) | `spec/documentation-review/audit.md` (Sections 1 & 2) | Verified against `git ls-files` |
| **FR-002** (`README.md` entry point, install, examples, index, status, links, legacy anchors, `Apache License 2.0`) | `README.md` | Verified anchors, links, and CLI examples |
| **FR-003** (`build-info` syntax, `./build_info.json` default, `cwd` Git repo + valid `HEAD`, existing parent dir, overwrite, 2-field JSON schema, error `stderr` + exit code `0`) | `docs/cli.md`, `README.md`, `locales/en.yml`, `locales/zh-CN.yml`, `src/build_info/mod.rs` | Verified in disposable Git repo (`mktemp -d`) and non-repo / missing-parent cases |
| **FR-004** (`agents` 7-agent catalog, binary precedence, verified ownership/channels, `--dry-run`, sequential order, `stderr`/`stdout` split, exit codes `0`/`1`/`2`/`130`, empty selection, troubleshooting, `5s`/`64 KiB`/`PROBE_ENV_OVERRIDES`, non-sandbox boundary, absent vs. `manual action required`) | `docs/agents.md`, `README.md`, `locales/en.yml`, `locales/zh-CN.yml` | Verified against `src/agents/{mod.rs,discovery.rs,update.rs}`, `tests/agents_cli.rs`, and isolated fake-tool sandbox |
| **FR-005** (Global `-v`/`-q`, locale precedence `LC_ALL` -> `LC_MESSAGES` -> `LANG`, `en`/`zh-CN` fallback to `"en"`, `config-template` unimplemented panic, paired locale updates) | `docs/cli.md`, `locales/en.yml`, `locales/zh-CN.yml`, `src/main.rs` | Verified `--help` in `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8` across all subcommands |
| **FR-006** (`CONTRIBUTING.md` and `CHANGELOG.md` for `0.2.2`) | `CONTRIBUTING.md`, `CHANGELOG.md` | Verified links, commands, and unpublished `0.2.2` status note |
| **FR-007** (`Cargo.toml` metadata: `description`, `repository`, `documentation`, `readme`, `keywords`, `categories`) | `Cargo.toml` | Verified zero missing-metadata warnings in `cargo package --list --locked` and `cargo publish --dry-run --locked` |
| **FR-008** (Patch version `0.2.2` in `Cargo.toml`, `Cargo.lock`, and active version assertions without dependency changes) | `Cargo.toml`, `Cargo.lock`, `tests/agents_cli.rs:86` | Verified via `cargo run --locked -- --version` (`teemiao 0.2.2`) and `git diff Cargo.lock` |
| **FR-009** (Verification commands, isolated examples, both locales, self-review gates) | `spec/documentation-review/audit.md` (Sections 4 & 5), `spec/documentation-review/tasks.md` | All 13 required commands exit `0` |
| **SC-001**–**SC-005** | All updated files | Verified end-to-end |

---

## 4. Command & Runtime Verification Evidence (`FR-009`, `SC-005`)

All required commands from `spec/documentation-review/spec.md` were executed on the branch:

1. `cargo fmt --check` -> exit `0`
2. `cargo clippy --locked --all-targets -- -D warnings` -> exit `0` (0 warnings)
3. `cargo test --locked` -> exit `0`:
   - Unit tests (`src/main.rs`, `src/agents/*`): `39 passed; 0 failed`
   - Integration tests (`tests/agents_cli.rs`): `28 passed; 0 failed` (including `agents_without_subcommand_displays_help_and_exits_2` asserting `teemiao 0.2.2`)
4. `cargo build --locked` -> exit `0`
5. `cargo doc --locked --no-deps` -> exit `0` (0 rustdoc warnings)
6. `cargo metadata --locked --no-deps --format-version 1` -> exit `0` (`"name":"teemiao","version":"0.2.2"`)
7. `cargo run --locked -- --version` -> exit `0` (`teemiao 0.2.2`)
8. `cargo run --locked -- --help` -> exit `0` (verified in both `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8`)
9. `cargo run --locked -- build-info --help` -> exit `0` (verified in both `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8`)
10. `cargo run --locked -- agents update --help` -> exit `0` (verified in both `LC_ALL=en_US.UTF-8` and `LC_ALL=zh_CN.UTF-8`; also verified `config-template --help`, `agents --help`, and `agents list --help`)
11. `cargo package --list --locked` -> exit `0` (69 packaged files listed; 0 missing-metadata warnings)
12. `cargo publish --dry-run --locked` -> when a local Cargo `[source.crates-io]` mirror replacement (`aliyun`) is active in `~/.cargo/config.toml`, bare `cargo publish --dry-run --locked` exits `101` asking for `--registry crates-io`; running `cargo publish --dry-run --locked --registry crates-io` (and `cargo package --locked`) exits `0`, packaging 69 files (`1.1MiB` / `314.6KiB` compressed) and compiling `target/package/teemiao-0.2.2` cleanly with 0 missing-metadata warnings
13. `git diff --check` -> exit `0` (no whitespace or conflict marker issues)

### Isolated Disposable Sandbox Verification

- **`build-info` default & custom paths**: In a temporary Git repository (`mktemp -d`) with an initial commit, `teemiao build-info` wrote `./build_info.json` with keys `revision` (short hash) and `build_time` (`i64` Unix seconds), and `teemiao -v build-info custom/build_info.json` logged `INFO` progress to `stderr` and wrote `custom/build_info.json`.
- **`build-info` error and exit code behavior**: Running `teemiao build-info missing_parent/build_info.json` printed `Error generating build info: Failed to write to output file: No such file or directory (os error 2)` to `stderr` and exited with code `0`. Running `teemiao build-info` in a non-Git directory printed `Error generating build info: Failed to open repository: ... does not appear to be a git repository` to `stderr` and exited with code `0`.
- **Fake-agent `agents list` & `agents update --dry-run` in `en_US.UTF-8` and `zh_CN.UTF-8`**:
  - Empty `PATH`: printed `No supported AI coding-agent CLIs were found on PATH.` / `No supported AI coding-agent CLIs were found on PATH; nothing to update.` on `stdout` and exited `0`.
  - Fake `agy` script on isolated `PATH`: `agents list` reported `Antigravity (antigravity) | version: 1.4.2 | path: .../agy | update: native (agy update)` (`更新方式: 原生自更新 (agy update)` in `zh_CN.UTF-8`), and `agents update agy --dry-run` wrote `--- [1/1] Antigravity (antigravity) ---` to `stderr` and `Update summary` / `更新摘要` to `stdout` with exit code `0`.

---

## 5. Self-Review Gates (`FR-009`)

### 5.1 `test-driven-development` & `doubt-driven-development`
- Updated `tests/agents_cli.rs:86` to expect `"teemiao 0.2.2"` first and confirmed the test failed (`left: "teemiao 0.2.1"`, `right: "teemiao 0.2.2"`) before bumping `Cargo.toml` and `Cargo.lock` to `0.2.2` and re-running to green.
- Challenged every claim in `README.md`, `docs/cli.md`, `docs/agents.md`, `CONTRIBUTING.md`, and `CHANGELOG.md` against source code (`src/main.rs`, `src/build_info/mod.rs`, `src/agents/{mod.rs,discovery.rs,update.rs}`) and live CLI execution in disposable sandboxes.

### 5.2 `code-review-and-quality`
- Verified all internal Markdown links (`docs/cli.md`, `docs/agents.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `CONSTRAINTS.md`, `LICENSE`, `spec/documentation-review/audit.md`) and preserved legacy `README.md` section anchors (`#usage`, `#build-information-generation`, `#command-line-options`, `#configuration`).
- Confirmed `locales/en.yml` and `locales/zh-CN.yml` remain in lockstep across all message keys.

### 5.3 `code-simplification`
- Kept `README.md` concise as an entry point while placing detailed command, agent-matrix, and maintainer documentation in dedicated files (`docs/cli.md`, `docs/agents.md`, `CONTRIBUTING.md`) to avoid duplicating large reference tables.

### 5.4 `security-and-hardening`
- Explicitly documented that `teemiao` is not an OS sandbox and cannot prevent a binary on `PATH` from performing network or filesystem actions when invoked with `--version` or `--help`, while documenting the bounded probe controls (`5s` timeout, `64 KiB` stream cap, `PROBE_ENV_OVERRIDES`).
- All verification used isolated fake scripts in temporary directories (`mktemp -d`); no real agent installers or self-updaters were executed.

### 5.5 `performance-optimization`
- Confirmed zero runtime overhead was introduced: Rust code edits were limited to doc comments and localized static help strings, and `Cargo.lock` dependency graph remained unchanged.
