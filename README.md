# Teemiao

Teemiao is a versatile toolkit designed to streamline application development
workflows.

## Features

### Build Information Generator (`build-info`)

Automatically generates structured metadata about your build process in JSON
format. The generated information includes:

- Build timestamp
- Git revision hash
- Additional contextual build data

This feature enables better traceability and version management for your
deployments.

### AI Coding-Agent CLI Manager (`agents`)

Discovers and updates supported AI coding-agent command-line tools resolved from
the current process `PATH` without touching desktop applications, IDE extensions,
or unowned installations:

- **`teemiao agents list`** — Inspects `PATH` in catalog order and reports each
  detected coding-agent CLI's canonical identifier, version, executable path, and
  verified update method without performing network update checks.
- **`teemiao agents update [AGENT] [--dry-run]`** — Updates one specified agent
  (or every detected agent sequentially when `AGENT` is omitted) using the
  verified installation owner and preserving the active release channel. Each
  attempted agent's diagnostics are grouped under a
  `--- [<index>/<total>] <Name> (<id>) ---` header flushed to `stderr` before the
  updater runs, and final per-agent outcomes are printed under a separated
  `Update summary` (`更新摘要` in `zh-CN`) heading on `stdout`.

#### Supported Agents and Update Methods

| Display Name | Canonical ID | CLI Aliases | Probed Executables on `PATH` | Supported Automated Update Paths |
|---|---|---|---|---|
| Codex | `codex` | — | `codex` | `npm install -g --prefix <prefix> @openai/codex@latest`, `brew upgrade --cask codex` (or formula when formula-owned), or official standalone installer (`https://chatgpt.com/codex/install.sh` on Unix / `install.ps1` on Windows) |
| Antigravity | `antigravity` | `agy` | `agy` | `agy update` |
| OpenCode | `opencode` | — | `opencode` | `opencode upgrade` |
| Kimi | `kimi` | — | `kimi` | `kimi upgrade --yes` |
| Cursor | `cursor` | — | `agent` (verified Cursor identity), `cursor-agent` | `agent update` or `cursor-agent update` |
| Claude Code | `claude-code` | `claude` | `claude` | `claude update` (native), `npm install -g --prefix <prefix> @anthropic-ai/claude-code@latest`, `brew upgrade --cask <cask-token>`, or `winget upgrade --exact --id Anthropic.ClaudeCode ...` |
| Qoder | `qoder` | `qodercli` | `qodercli`, `qoder` (verified Qoder CLI identity) | `qodercli update` / `qoder update` (verified native CLI) or `npm install -g --prefix <prefix> @qoder-ai/qodercli@latest` |

#### Discovery and Safety Boundaries

- **`PATH`-only active CLI discovery:** Only runnable executables on the current
  process `PATH` are inspected. Desktop applications (`cursor`, `antigravity`,
  `Qoder Desktop` / `Qoder IDE`) and editor extensions are never launched as
  managed CLIs. Generic `agent` binaries on `PATH` must pass Cursor CLI
  identity/metadata verification before being classified as Cursor, and Qoder
  prefers `qodercli` before falling back to a `qoder` binary that provides
  corroborating Qoder CLI evidence beyond a bare product/version string.
- **No implicit new installations:** Updating an absent tool exits with code `1`
  and never installs a new agent. Bulk updates (`teemiao agents update`) only act
  on agents currently discovered on `PATH`.
- **Ownership and channel preservation:** Package-managed installations (`npm`,
  Homebrew, WinGet) are matched to the active executable's resolved path and
  prefix. Installations managed by unsupported package managers (such as system
  `apt`/`dnf`/`pacman`, `pnpm`, `bun`, `scoop`, `choco`, or `nix`), ambiguous
  multi-manager installations, legacy migrations, or pinned/prerelease channels
  that cannot be preserved automatically are reported as `manual action required`
  with actionable guidance (never executing vendor installer scripts for Qoder or
  migrating an npm-owned installation to native self-update).
- **Separated per-agent update output:** During `teemiao agents update`, each
  attempted agent's section header (`--- [<index>/<total>] <Name> (<id>) ---`) is
  flushed to `stderr` before any probe warnings or updater output, blank lines
  separate consecutive agent sections, and `stdout` prints a separated
  `Update summary` (`更新摘要`) heading followed by one outcome row per attempted
  agent.
- **Bounded probes:** Version/identity probes run with `CI=1` and
  `DISABLE_AUTOUPDATER=1`, a 5-second timeout, and a 64 KiB per-stream output cap
  so hung or noisy tools do not block discovery of other agents.

## Current Status

The project currently provides Git build-info generation (`build-info`) and AI
coding-agent CLI discovery and updates (`agents`). Support for additional version
control systems and configuration templates is planned for future releases.

## Getting Started

### Installation

You can install Teemiao using Cargo:

```bash
cargo install teemiao
```

### Usage

Generate build information for your project:

```bash
teemiao build-info
```

List installed AI coding-agent CLIs on `PATH`:

```bash
teemiao agents list
```

Preview planned update commands without modifying installed software:

```bash
teemiao agents update --dry-run
teemiao agents update codex --dry-run
teemiao agents update qoder --dry-run
```

Update a single coding-agent CLI (by canonical ID or alias) or all detected
coding-agent CLIs on `PATH`:

```bash
teemiao agents update antigravity
teemiao agents update agy
teemiao agents update claude
teemiao agents update qoder
teemiao agents update qodercli
teemiao agents update
```

#### Exit Codes (`teemiao agents`)

- `0` — Discovery succeeded, `--dry-run` completed for installable plans, or all
  selected agent updates were verified as `updated` or `up to date`.
- `1` — One or more selected agents were not installed on `PATH`, required
  manual action, failed to update, or could not be verified after the updater
  exited.
- `2` — Invalid CLI arguments or unknown agent identifier (rejected by `clap`
  before any updater runs).
- `130` — Update interrupted/cancelled (such as `SIGINT` / `Ctrl-C`); subsequent
  queued updates are not started.

#### Troubleshooting

- **Installed agent does not appear in `teemiao agents list`:** Ensure the
  directory containing the agent's CLI executable (for example `~/.local/bin`,
  `~/.opencode/bin`, or your global `npm` bin directory) is exported on `PATH` in
  your shell profile.
- **`manual action required` reported during `teemiao agents update`:** Read the
  reported reason and guidance on `stderr`. Teemiao refuses to cross package
  managers or overwrite pinned/prerelease channels when ownership cannot be
  safely verified.

## Contributing

We welcome contributions! Please see our contributing guidelines for more
details.

## License

This project is under Apache Public License 2.0 (APL-2). For detailed
information, please consult the LICENSE file.

---

> **Note**: This project is under active development. For feature requests or bug reports, please open an issue in our repository.
