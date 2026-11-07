# Agent update research

Checked 2026-11-08. These are upstream facts used to design Teemiao, not evidence
that a real agent was updated. No installed agent was modified during research.
The coder must validate supported flags/ownership against installed-version
fixtures; a documentation change must not be covered up by inventing a command.

| Agent | CLI candidates | Proposed automated update coverage |
|---|---|---|
| Codex | `codex` | Proven global npm package: upgrade only `@openai/codex`; proven Homebrew cask: upgrade only `codex`; proven standalone install: official platform installer with the existing target preserved. |
| Antigravity | `agy` | Native `agy update`. The desktop launcher `antigravity` is not a substitute. |
| OpenCode | `opencode` | `opencode upgrade`, retaining detected method/version lineage. Do not silently use migration flags. |
| Kimi | `kimi` | Current CLI: `kimi upgrade --yes`, which supports native and several global JS managers. Identify legacy installations and give manual guidance when the supported upgrade path differs. |
| Cursor | `agent`, legacy `cursor-agent` | Verified Cursor executable: `update`; require Cursor identity evidence for the generic name. |
| Claude Code | `claude` | Native `claude update`; proven npm package upgrade; exact installed Homebrew cask; proven WinGet package upgrade. Other system-managed installations receive manual guidance in this release. |

## Primary sources

- [Codex CLI](https://learn.chatgpt.com/docs/codex/cli): installation tabs document
  npm, Homebrew, and standalone scripts for Unix/Windows. The Markdown page was
  retrieved to read all tabs. The current Unix installer was inspected as text:
  it uses an install directory and standalone package tree, and supports
  noninteractive execution. Native ownership and target preservation must be
  established before reusing it; filenames alone are insufficient.
- [Antigravity installation](https://antigravity.google/docs/getting-started?tab=cli)
  documents `agy` on macOS/Linux/Windows;
  [its CLI changelog guide](https://antigravity.google/docs/cli/commands/changelog/)
  explicitly names `agy update`.
- [OpenCode commands](https://opencode.ai/docs/cli/commands/) documents the
  `upgrade` command. Older and newer product generations must not be silently
  conflated; preserve the active install's supported update mechanism.
- [Kimi command reference](https://moonshotai.github.io/kimi-code/en/reference/kimi-command)
  documents noninteractive upgrade and notes that unsupported installation
  methods may only print manual instructions. A zero process exit alone is
  therefore not enough to establish an update succeeded.
- [Cursor CLI installation](https://cursor.com/docs/cli/installation) documents
  the current `agent` command and its `update` subcommand, including native
  Windows installation. [Older official documentation](https://docs.cursor.com/en/cli/installation)
  uses `cursor-agent`. Compatibility must be tested rather than assuming the
  generic command name uniquely identifies Cursor.
- [Claude Code setup](https://code.claude.com/docs/en/setup) distinguishes native,
  npm, Homebrew, WinGet, and Linux package-manager installations. Homebrew cask
  names can encode different channels. A native self-update command must not
  stand in for a package-manager update; some managed cases report success
  without applying an update.

## Design conclusions

Discovery, installation ownership, update execution, and outcome verification are
separate decisions. Test each boundary with fake executables and local metadata.
Do not scrape prose to guess a manager, migrate deprecated products automatically,
or advertise global-latest guarantees that the existing channel cannot provide.

Real network downloads and vendor updater side effects remain upstream
dependencies. Tests can establish our target selection, invocation, and reporting;
they cannot prove that every upstream release will install successfully.
