# Teemiao

Teemiao (`teemiao`) is a Rust command-line tool for generating Git build metadata files (`build-info`) and discovering or updating installed AI coding-agent CLIs on `PATH` (`agents`).

- **Repository:** <https://github.com/qqiao/teemiao>
- **Issue Tracker:** <https://github.com/qqiao/teemiao/issues>
- **License:** [Apache License 2.0](LICENSE)

## Documentation Index

| Document | Contents |
|---|---|
| [CLI Reference (`docs/cli.md`)](docs/cli.md) | Command synopsis, verbosity flags (`-v`/`-q`), `build-info` positional syntax, JSON schema (`revision`, `build_time`), exit-status limitations, `config-template` status, and `en` / `zh-CN` localization rules. |
| [AI Coding-Agent CLI Reference (`docs/agents.md`)](docs/agents.md) | Supported agent catalog (`codex`, `antigravity`/`agy`, `opencode`, `kimi`, `cursor`, `claude-code`/`claude`, `qoder`/`qodercli`/`qoder-cli`), ownership and channel verification, `--dry-run`, output streams, probe limits, exit codes, and troubleshooting. |
| [Contributing Guide (`CONTRIBUTING.md`)](CONTRIBUTING.md) | Repository layout, local prerequisites, isolated fake-tool testing rules, required verification commands, CI matrix, and release-preparation checklist. |
| [Changelog (`CHANGELOG.md`)](CHANGELOG.md) | Prepared release notes for `0.2.4`. |
| [Quality Constraints (`CONSTRAINTS.md`)](CONSTRAINTS.md) | Mandatory quality, safety, process-containment, and verification floor. |

## Features

### Build Information Generator (`build-info`)

Reads the Git repository in the current working directory and writes a pretty-printed JSON file containing:

- `revision` — Shortened `HEAD` commit hash (`String`)
- `build_time` — UTC Unix timestamp in seconds (`i64`)

See [`docs/cli.md#build-information-generator-teemiao-build-info`](docs/cli.md#build-information-generator-teemiao-build-info) for the exact JSON schema, repository-root and parent-directory requirements, overwrite behavior, and error/exit-code semantics.

### AI Coding-Agent CLI Manager (`agents`)

Discovers and updates supported AI coding-agent CLIs resolved from the current process `PATH` without touching desktop applications, IDE extensions, or unverified installations:

- **`teemiao agents list`** — Inspects `PATH` in deterministic catalog order (`codex`, `antigravity`, `opencode`, `kimi`, `cursor`, `claude-code`, `qoder`) and reports each detected CLI's version, executable path, and verified update method.
- **`teemiao agents update [AGENT] [--dry-run]`** — Previews (`--dry-run`) or executes updates for one named agent or all detected agents sequentially, preserving the active installation owner and release channel.

#### Supported Agents and Update Methods

Teemiao supports seven coding-agent CLIs in catalog order: **Codex** (`codex`), **Antigravity** (`antigravity` / `agy`), **OpenCode** (`opencode`), **Kimi** (`kimi`), **Cursor** (`cursor`, via verified `agent` or `cursor-agent`), **Claude Code** (`claude-code` / `claude`), and **Qoder** (`qoder` / `qodercli` / `qoder-cli`). Qoder accepts all three identifiers case-insensitively; `qoder-cli` is an argument alias, and output keeps the canonical label `Qoder (qoder)`.

For the full catalog table, candidate executable precedence, and per-manager update commands (`native`, `npm`, `Homebrew`, `WinGet`, and `Codex` standalone installer), see [`docs/agents.md#supported-agents-catalog`](docs/agents.md#supported-agents-catalog).

#### Discovery and Safety Boundaries

- **Active `PATH` launcher discovery:** Discovery starts with runnable launchers on the current process `PATH`; desktop app bundles and editor extensions are excluded. Verified Qoder dispatchers (`~/.qoder/entry/qoder` on Unix or `%USERPROFILE%\.qoder\entry\qoder.cmd` on Windows) are inspected statically to resolve the underlying `qodercli` binary, including known install locations outside `PATH`. Standalone updates run that verified binary directly; IDE-only dispatchers without an installed CLI are excluded.
- **Verified update selection and channel preservation:** Package-managed updates (`npm`, `Homebrew`, `WinGet`), Codex standalone updates, and Claude Code / Qoder native updates require positive local path, package, or `--help` proof. Dedicated self-updating CLIs use agent-specific checks: Antigravity and Cursor reject npm/Homebrew paths, while OpenCode and Kimi allow those paths and reject known legacy installations. Unsupported ownership or channel combinations report `manual action required`; see the per-agent rules in the guide.
- **Bounded probes (not an OS sandbox):** Discovery probes use a 5-second timeout, a 64 KiB per-stream capture cap, and child environment overrides (`CI=1`, `DISABLE_AUTOUPDATER=1`, etc.), but executing an external binary on `PATH` is not a sandbox or a universal no-network guarantee.

See [`docs/agents.md#ownership-resolution-and-channel-preservation-rules`](docs/agents.md#ownership-resolution-and-channel-preservation-rules) and [`docs/agents.md#probe-safeguards-and-security-boundaries`](docs/agents.md#probe-safeguards-and-security-boundaries) for complete details.

## Current Status

| Subcommand | Status | Notes |
|---|---|---|
| `teemiao build-info [FILE]` | Implemented | Requires invocation from a Git repository root with a valid `HEAD` commit and an existing output parent directory. Generation errors print to `stderr` while the process currently exits `0` ([details](docs/cli.md#operational-requirements-and-limitations)). |
| `teemiao agents list` | Implemented | Inspects `PATH` and prints detected agents to `stdout` (`0` exit code). |
| `teemiao agents update [AGENT] [--dry-run]` | Implemented | Updates one or all detected agents on `PATH` with separated `stderr` diagnostics and a `stdout` summary ([details](docs/agents.md#output-streams-and-formatting)). |
| `teemiao config-template` | **Unfinished (panics)** | Placeholder subcommand; running `teemiao config-template` executes `todo!("config template")` and panics ([details](docs/cli.md#unfinished-command-teemiao-config-template)). |

## Getting Started

### Installation

Install the published binary from [crates.io](https://crates.io/crates/teemiao) with Cargo:

```sh
cargo install teemiao --locked
```

Or build and install from a source checkout (requires a Rust toolchain supporting the 2024 edition):

```sh
git clone https://github.com/qqiao/teemiao.git
cd teemiao
cargo install --path . --locked
```

### Usage

Write build metadata to the default `./build_info.json` or an explicit file path (from a Git repository root):

```sh
teemiao build-info
teemiao build-info build_info.json
```

List installed AI coding-agent CLIs discovered on `PATH`:

```sh
teemiao agents list
```

Preview planned update commands without modifying installed software:

```sh
teemiao agents update --dry-run
teemiao agents update codex --dry-run
teemiao agents update qoder --dry-run
teemiao agents update qoder-cli --dry-run
```

Update a single coding-agent CLI (by canonical ID or alias) or all detected coding-agent CLIs on `PATH`:

```sh
teemiao agents update antigravity
teemiao agents update agy
teemiao agents update claude
teemiao agents update qoder
teemiao agents update qodercli
teemiao agents update qoder-cli
teemiao agents update
```

Display localized help in English or Simplified Chinese (`LC_ALL` -> `LC_MESSAGES` -> `LANG`):

```sh
LC_ALL=en_US.UTF-8 teemiao --help
LC_ALL=zh_CN.UTF-8 teemiao --help
```

#### Exit Codes (`teemiao agents`)

- `0` — `agents list` completed, bulk `agents update` found no installed agents, `--dry-run` succeeded for all selected agents, or all selected updates finished as `updated` / `up to date`.
- `1` — Named agent is not installed on `PATH`, or one or more selected agents resulted in `manual action required`, `unverified`, or `failed`.
- `2` — Invalid CLI arguments or unknown agent identifier rejected by `clap`.
- `130` — Update interrupted (`SIGINT` / `SIGTERM`); remaining queued agents are not started.

See [`docs/cli.md#exit-codes-summary`](docs/cli.md#exit-codes-summary) and [`docs/agents.md#exit-codes`](docs/agents.md#exit-codes) for full exit-code tables.

#### Troubleshooting

For missing agents on `PATH`, `manual action required` guidance, and `unverified` outcomes, see [`docs/agents.md#troubleshooting`](docs/agents.md#troubleshooting).

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) and [CONSTRAINTS.md](CONSTRAINTS.md) before making changes, and open issues or pull requests at <https://github.com/qqiao/teemiao/issues>.

## License

This project is licensed under the **Apache License 2.0** (`Apache-2.0`). See the [LICENSE](LICENSE) file for the full license text.
