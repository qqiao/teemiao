# AI Coding-Agent CLI Reference (`teemiao agents`)

`teemiao agents` discovers and updates supported AI coding-agent command-line tools resolved from the current process `PATH`. For general CLI options, `build-info`, and localization rules, see [CLI Reference (`docs/cli.md`)](cli.md).

## Commands

### `teemiao agents list`

```sh
teemiao agents list
```

Inspects `PATH` for the seven supported coding-agent CLIs in fixed catalog order (`codex`, `antigravity`, `opencode`, `kimi`, `cursor`, `claude-code`, `qoder`) and prints one summary row per detected agent to `stdout`:

```text
<Name> (<id>) | version: <version|unknown> | path: <launcher_path> | update: <method>
```

In Simplified Chinese (`zh-CN`):

```text
<Name> (<id>) | 版本: <version|未知> | 路径: <launcher_path> | 更新方式: <method>
```

- **Empty selection:** If no supported coding-agent CLI is found on `PATH`, prints `No supported AI coding-agent CLIs were found on PATH.` (`在 PATH 中未找到受支持的 AI 编程代理 CLI。`) to `stdout` and exits with code `0`.
- **Non-fatal probe warnings:** If an agent binary is identified on `PATH` but its `--version` probe times out, fails, exceeds 64 KiB, or returns unparseable text, Teemiao prints a warning to `stderr` and still lists the agent on `stdout` with `version: unknown` (`版本: 未知`).

### `teemiao agents update [AGENT] [--dry-run]`

```sh
teemiao agents update [AGENT] [--dry-run]
```

- **`[AGENT]` (optional, case-insensitive):** Canonical agent ID or alias (`codex`, `antigravity`/`agy`, `opencode`, `kimi`, `cursor`, `claude-code`/`claude`, `qoder`/`qodercli`).
  - When specified, only that agent is discovered and processed.
  - When omitted, Teemiao discovers all installed supported agents on `PATH` and processes them sequentially in catalog order.
- **`--dry-run`:** Runs discovery and read-only ownership checks, then prints the planned update command and target (or `manual action required` guidance) without executing updaters, downloading installers, or modifying installed files.

## Supported Agents Catalog

Agents are always listed and updated in this deterministic catalog order:

| # | Display Name | Canonical ID | CLI Alias | Candidate Executables on `PATH` (in preference order) | Verified Automated Update Paths |
|---|---|---|---|---|---|
| 1 | Codex | `codex` | — | `codex` | Global npm (`<npm> install -g --prefix <prefix> @openai/codex@latest`), Homebrew cask (`<brew> upgrade --cask <cask-token>`), Homebrew formula (`<brew> upgrade codex`), or official standalone installer (`https://chatgpt.com/codex/install.sh` on Unix / `https://chatgpt.com/codex/install.ps1` on Windows) |
| 2 | Antigravity | `antigravity` | `agy` | `agy` | Native self-update (`<launcher> update`, displayed as `agy update`) |
| 3 | OpenCode | `opencode` | — | `opencode` | Native self-update (`<launcher> upgrade`, displayed as `opencode upgrade`) |
| 4 | Kimi | `kimi` | — | `kimi` | Native self-update (`<launcher> upgrade --yes`, displayed as `kimi upgrade --yes`) |
| 5 | Cursor | `cursor` | — | `agent` (requires Cursor CLI identity proof), `cursor-agent` | Native self-update (`<launcher> update`, displayed as `agent update` or `cursor-agent update`) |
| 6 | Claude Code | `claude-code` | `claude` | `claude` | Native self-update (`<launcher> update`, displayed as `claude update`), global npm (`<npm> install -g --prefix <prefix> @anthropic-ai/claude-code@latest`), Homebrew cask (`<brew> upgrade --cask <cask-token>`), or WinGet (`<winget> upgrade --exact --id Anthropic.ClaudeCode --accept-source-agreements --accept-package-agreements --disable-interactivity`) |
| 7 | Qoder | `qoder` | `qodercli` | `qodercli`, `qoder` (requires Qoder CLI identity proof) | Global npm (`<npm> install -g --prefix <prefix> @qoder-ai/qodercli@latest`) or native self-update (`<launcher> update`, displayed as `qodercli update` or `qoder update`, when `--help` advertises `update`) |

## Ownership Resolution and Channel Preservation Rules

Teemiao never guesses a package-manager owner from the mere presence of `npm`, `brew`, or `winget` on `PATH`:

- **Package-managed and multi-method agents (`codex`, `claude-code`, `qoder`):** Package-managed update plans (`npm`, `Homebrew`, `WinGet`), Codex standalone installer plans, Claude Code native updates, and Qoder native updates require positive proof from the active launcher path, its symlink-resolved target, verified wrapper script bindings, local package metadata, or `--help` output.
- **Dedicated native self-updating CLIs (`antigravity`/`agy`, `opencode`, `kimi`, and identity-verified `cursor`):** Select their native self-update subcommand using agent-specific foreign/system-manager checks and reject unsafe Windows `.cmd` shims. Antigravity and Cursor reject npm (`node_modules`) and Homebrew (`Caskroom`/`Cellar`) paths. OpenCode and Kimi allow those paths and delegate updates to their own CLI; they additionally check for known legacy installation markers. These native paths do not require universal package-ownership proof.

### Per-Agent Ownership Matrix

1. **Codex (`codex`)**
   - **npm (`@openai/codex`):** Selected when the launcher or its verified wrapper/symlink target resolves inside a global npm prefix (`<prefix>/lib/node_modules/@openai/codex`, or `<prefix>/node_modules/@openai/codex` on Windows) with a matching `package.json` name, version, and `bin` entry point (`is_local_node_modules_bin_launcher` excludes project-local `node_modules/.bin` launchers from global npm ownership). Requires `npm` on `PATH`.
   - **Homebrew cask / formula:** Selected when the launcher or symlink/wrapper target resolves inside `<brew_prefix>/Caskroom/codex*` or `<brew_prefix>/Cellar/codex`, a matching `brew` binary for `<brew_prefix>` is verified via `brew --prefix`, and the token is present in `brew list --cask` or `brew list --formula`.
   - **Official standalone installer:** Selected when the launcher resolves into `<codex_home>/packages/standalone/...` (or resides in `CODEX_INSTALL_DIR` / `~/.local/bin` on Unix / `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin` on Windows alongside verified standalone metadata in `CODEX_HOME` or `~/.codex`).
     - Downloads the fixed platform script (`https://chatgpt.com/codex/install.sh` or `https://chatgpt.com/codex/install.ps1`) via `curl --fail --silent --show-error --location --proto =https --tlsv1.2 -o <script>` into a private temporary directory (`0700` on Unix).
     - Validates before execution that the downloaded script is a regular, non-symlink UTF-8 file containing `CODEX_INSTALL_DIR`, `CODEX_HOME`, and `installed successfully` (and starting with `#!/bin/sh` on Unix).
     - Executes `sh <script>` on Unix or `powershell`/`pwsh -NoProfile -NonInteractive -ExecutionPolicy Bypass -File <script>` on Windows with `CODEX_NON_INTERACTIVE=1`, `CODEX_INSTALL_DIR=<bin_dir>`, `CODEX_HOME=<codex_home>`, and `CI=1`.
     - Recognizes the installer's `<version>-<platform-target>` release directories and `auto-update-version` marker. The platform suffix does not indicate a prerelease. An explicit `--release <version>` installation removes this marker, so missing, unreadable, or malformed markers require manual action.
     - A stale marker that disagrees with the active version, current package metadata, or selected release directory also requires manual action. Repointing `current` to another build does not opt that build into automatic updates, even if its version number is unchanged. Both musl and GNU Linux package target suffixes are recognized.
     - Retains known non-fatal cleanup warnings in diagnostics without rejecting a successful same-version install. A successful exit, a matching active installation/version, and affirmative updater output are still required; warning-only output and actual failures do not confirm success.
   - **Manual action required:** Unproven standalone/package ownership (including project-local `node_modules/.bin` launchers on `PATH`), WinGet installations, prerelease npm versions, pinned/prerelease standalone channels (missing or invalid `auto-update-version`, a prerelease version, or a non-`latest`/`stable` channel in `codex-package.json`), `CODEX_INSTALL_DIR` mismatch against the active launcher directory, or contradictory signals across multiple managers.

2. **Antigravity (`antigravity` / `agy`)**
   - Only `agy` on `PATH` is inspected; `antigravity` desktop launchers are ignored.
   - Uses native `agy update` unless the binary resides under npm (`node_modules`), Homebrew (`Caskroom`/`Cellar`), or another foreign/system package manager, which reports `manual action required`.

3. **OpenCode (`opencode`)**
   - Uses native `opencode upgrade` unless legacy markers are detected (`legacy` in probe/version output or an adjacent `.opencode-legacy` file) or an unsupported foreign/system-manager path is detected. npm and Homebrew paths are allowed and delegated to OpenCode's updater. Never passes automatic cross-generation migration flags.

4. **Kimi (`kimi`)**
   - Uses native `kimi upgrade --yes` unless legacy markers are detected (`legacy` in probe/version output, an adjacent `.kimi-legacy` file, or wrapper scripts containing `kimi-legacy` or `no-self-upgrade`) or an unsupported foreign/system-manager path is detected. npm and Homebrew paths are allowed and delegated to Kimi's updater.
   - If `kimi upgrade --yes` prints manual upgrade instructions (such as `run uv tool upgrade` or `run pip install`), Teemiao classifies the outcome as `manual action required` even if the child exited with code `0`.

5. **Cursor (`cursor`)**
   - Prefers a verified `agent` binary on `PATH` over legacy `cursor-agent`; never inspects the `cursor` desktop binary.
   - A candidate named `agent` is accepted **only** if `--version` contains no conflicting non-Cursor product names and Cursor identity is proven by:
     - Cursor branding in `--version` (`anysphere`, `cursor.com`, `cursor agent`, `cursor-agent`, `cursor cli`, or `cursor <version>`),
     - Local Cursor path/package evidence (`/.cursor/bin/`, `/cursor.app/contents/resources/app/bin/`, `/programs/cursor/resources/app/bin/`, `/cursor-agent/`, adjacent `.cursor-agent`, or `package.json` for `@cursor/agent` / `@anysphere/cursor-agent` / `cursor-agent`), or
     - Cursor branding in a fallback `--help` probe.
   - Unverified generic `agent` binaries on `PATH` are ignored so `cursor-agent` later on `PATH` can still be discovered.
   - Uses native `agent update` or `cursor-agent update` unless managed by npm, Homebrew, or another foreign/system package manager.

6. **Claude Code (`claude-code` / `claude`)**
   - Checks package-manager ownership before native self-update:
     - **Global npm (`@anthropic-ai/claude-code`):** Verified prefix, `package.json`, and `bin` entry point -> `<npm> install -g --prefix <prefix> @anthropic-ai/claude-code@latest`.
     - **Homebrew cask (`claude-code` or `claude-code@<channel>`):** Verified `<brew_prefix>/Caskroom/<token>` and `brew list --cask` inventory -> `<brew> upgrade --cask <token>` (preserving channel-specific casks such as `claude-code@latest`). Homebrew formula installs (`Cellar/claude`) report `manual action required`.
     - **WinGet (`Anthropic.ClaudeCode`):** Verified WinGet package/link path (or `.winget-anthropic-claudecode` marker) and `winget list --exact --id Anthropic.ClaudeCode ...` inventory -> `<winget> upgrade --exact --id Anthropic.ClaudeCode --accept-source-agreements --accept-package-agreements --disable-interactivity`.
     - **Native (`claude update`):** Allowed only when no package-manager signal exists and native paths are proven (`/.local/share/claude/`, `/.claude/`, `/.local/bin/claude`, `~/.local/bin`, `/appdata/local/claude/`, `/appdata/local/programs/claude/`, or `.claude-native`).

7. **Qoder (`qoder` / `qodercli`)**
   - Checks `qodercli` on `PATH` before falling back to `qoder`.
   - Rejects macOS `/Qoder.app/` bundle paths (including wrapper targets) and any candidate whose `--version` or `--help` output contains desktop/IDE markers (`qoder desktop`, `qoder ide`, `qoder editor`, `qoder studio`, `desktop application`, `electron`, `--open-window`, `--new-window`, `--reuse-window`) or conflicting product banners/usage program names.
   - **Global npm (`@qoder-ai/qodercli`):** Verified prefix, `package.json`, and `bin` entry point -> `<npm> install -g --prefix <prefix> @qoder-ai/qodercli@latest` (without needing a `--help` probe when npm ownership is already proven).
   - **Native (`qodercli update` or `qoder update`):** Selected only when there is no Homebrew/WinGet/foreign/ambiguous-npm/wrapper signal and `--help` positively advertises the `update` command without negation. Never downloads or executes vendor installer scripts (`qoder.com/install` or `install.ps1`) and never migrates npm installations to native.

## Absent Tools vs. Unsupported Installations

Teemiao distinguishes between tools that are **absent** from `PATH` and installations that are **present on `PATH` but cannot be updated automatically**:

| Situation | `teemiao agents list` | `teemiao agents update` (bulk) | `teemiao agents update <agent>` (targeted) |
|---|---|---|---|
| **Absent tool** (not on `PATH`, non-executable file, broken symlink, desktop launcher, or unverified generic `agent`/`qoder` binary) | Omitted from output; exits `0`. | Skipped; never installed; does not cause failure. | Prints `Error: agent '<Name>' (<id>) is not installed on PATH. Teemiao only updates existing installations on PATH.` to `stderr` and exits `1`. |
| **Unsupported or ambiguous installation** (detected on `PATH`, e.g., project-local Codex `node_modules/.bin` launchers, installations owned by `pnpm`/`bun`/`yarn`/`nix`/`snap`/`flatpak`/`scoop`/`chocolatey`/system `/usr/bin`, or an unsupported channel, legacy installation, or unproven owner under the per-agent rules above) | Listed on `stdout` with `update: manual action required (<reason>)`; exits `0`. | Included in the sequence; prints section header and `manual action required (<reason>; guidance: <guidance>)` on `stderr` and in the `stdout` summary; continues to remaining agents and exits `1`. | Prints section header and `manual action required (<reason>; guidance: <guidance>)` on `stderr` and in the `stdout` summary without running an updater; exits `1`. |

## Output Streams and Formatting

### `teemiao agents list`

- **`stderr`:** Non-fatal version-probe warnings (`Warning: version probe for ...`), if any.
- **`stdout`:** One `<Name> (<id>) | version: ... | path: ... | update: ...` line per discovered agent, or the single empty-state line `No supported AI coding-agent CLIs were found on PATH.`.

### `teemiao agents update`

- **`stderr` (per-agent sections):**
  - Before each attempted agent's probe warnings or updater execution, Teemiao writes and flushes a section header:
    ```text
    --- [<index>/<total>] <Name> (<id>) ---
    ```
  - A blank line separates consecutive agent sections (`index > 1`).
  - Any discovery probe warnings for that agent are written immediately under its section header.
  - After the updater finishes (for non-`--dry-run` invocations), outcome error lines (`manual action required`, `unverified`, `failed`, `interrupted`) and captured vendor `stderr`/`stdout` diagnostics (`Diagnostics for <Name> (<id>): ...`) are written inside that agent's section, always terminated with a newline.
- **`stdout` (final summary):**
  - If bulk discovery found no agents on `PATH`, prints `No supported AI coding-agent CLIs were found on PATH; nothing to update.` (`在 PATH 中未找到受支持的 AI 编程代理 CLI；无需更新。`) with no section headers or summary heading.
  - Otherwise, after all attempted agents finish (or after an interruption stops the loop), prints a leading blank line, the localized heading `Update summary` (`更新摘要`), and one outcome line per attempted agent:
    - `DryRun`: `<Name> (<id>): dry-run (would run: <command>; target: <target>; current version: <version>)`
    - `Updated`: `<Name> (<id>): updated (<before> -> <after>; path: <path>)`
    - `UpToDate`: `<Name> (<id>): up to date (version: <version>; path: <path>)`
    - `ManualActionRequired`: `<Name> (<id>): manual action required (<reason>; guidance: <guidance>)`
    - `Unverified`: `<Name> (<id>): unverified (<reason>; path: <path>)`
    - `Failed`: `<Name> (<id>): failed (<reason>; path: <path>)`
    - `Interrupted`: `<Name> (<id>): interrupted (update cancelled before completion)`

### Post-Update Verification Rules

After an updater exits with code `0` (or WinGet's `0x8A15002B` "no applicable update" status), Teemiao re-discovers and re-probes the active executable on `PATH`:

1. **`updated`:** Reported only when the updater exited `0`, the active installation still resolves to the selected installation directory/owner on `PATH`, and either the post-update version changed (`before != after`) or the initial version was `unknown` and the updater output affirmatively confirmed an update to `after`.
2. **`up to date`:** Reported only when the active version before and after matches (or initial version was `unknown`) **and** the updater's output matches known affirmative up-to-date clauses for that owner without negation/error markers.
3. **`unverified`:** Reported (exiting `1`) when the updater exited `0` but the post-update version probe failed/returned `unknown`, or the version did not change and the updater output did not affirmatively confirm up-to-date status.

## Probe Safeguards and Security Boundaries

Every discovery, version, help, and package-manager inventory probe runs through `run_bounded_probe`:

- **Execution:** Invokes the resolved executable directly with an argument array (`Command::new(&program).args(&args)`) and `stdin` connected to `Stdio::null()`. Shell command strings are never constructed from paths or user arguments, and Windows `.cmd`/`.bat` shims whose paths contain `cmd.exe` metacharacters (`&`, `|`, `<`, `>`, `^`, `%`, `!`, CR/LF) are refused before spawn.
- **5-second deadline (`PROBE_TIMEOUT`):**
  - On Unix, timed children are placed in a dedicated process group (`process_group(0)`) and killed via both `child.kill()` and `/bin/kill -KILL -<pgid>` on timeout, including when descendant processes hold `stdout`/`stderr` pipes open.
  - On Windows, timed children are spawned with `CREATE_SUSPENDED`, assigned to a Win32 Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` before resuming the primary thread, and terminated via `TerminateJobObject` on timeout.
- **Stream capture caps:** Probe `stdout` and `stderr` are each capped at **64 KiB** (`PROBE_MAX_OUTPUT_BYTES`). Update command `stdout` and `stderr` are each capped at **256 KiB** (`UPDATE_MAX_OUTPUT_BYTES`).
- **Child environment overrides (`PROBE_ENV_OVERRIDES`):** Probe child processes receive `CI=1`, `DISABLE_AUTOUPDATER=1`, `NO_UPDATE_NOTIFIER=1`, `NPM_CONFIG_UPDATE_NOTIFIER=false`, `HOMEBREW_NO_AUTO_UPDATE=1`, `CODEX_DISABLE_AUTO_UPDATE=1`, and `OPENCODE_DISABLE_AUTOUPDATE=1` without modifying the parent process environment or user configuration files.
- **Important limitation (not a sandbox):** Running an external executable on `PATH` with `--version` or `--help` is **not** an OS-level sandbox or a universal no-network guarantee. Teemiao itself performs no network requests during `agents list` or `agents update --dry-run` and sets the environment variables above, but a third-party binary on `PATH` executes with the current user's permissions and could still ignore those variables or make network calls of its own.

## Exit Codes

| Exit Code | Meaning |
|---|---|
| `0` | `teemiao agents list` completed; bulk `teemiao agents update` found no installed agents (no-op); `--dry-run` produced verified update plans for all selected agents; or all selected agents finished as `updated` or `up to date`. |
| `1` | Named `[AGENT]` is not installed on `PATH`; or at least one selected agent resulted in `manual action required`, `unverified`, or `failed` (including during `--dry-run` when an installation requires manual action). |
| `2` | Missing subcommand (`teemiao agents`) or invalid CLI arguments / unknown agent identifier rejected by `clap` before any probe or updater runs. |
| `130` | Update process was interrupted (`SIGINT` / `SIGTERM` or child exit code `130` / `143`). In bulk mode, Teemiao stops immediately and does not start subsequent queued agents. |

## Troubleshooting

- **Installed agent does not appear in `teemiao agents list`:**
  - Verify that the CLI binary's directory (for example `~/.local/bin`, `~/.opencode/bin`, `~/.cursor/bin`, or your global `npm` bin directory) is present in the current process `PATH` and that the file has executable permissions (`chmod +x` on Unix).
  - Remember that Teemiao ignores desktop app launchers (`cursor`, `antigravity`, `/Applications/Qoder.app`) and unverified generic `agent` or `qoder` binaries that lack CLI identity evidence.
- **`manual action required` reported for an npm, Homebrew, or standalone installation:**
  - Check the `reason` and `guidance` printed on `stderr` and under `Update summary`.
  - Common causes include:
    - The active Codex binary on `PATH` comes from a **project-local `node_modules/.bin` directory** rather than a global npm prefix (`<prefix>/bin` on Unix or `<prefix>` on Windows). Teemiao still discovers and lists this launcher with `update: manual action required (...)` (`agents list` exits `0`), but `detect_npm_ownership` rejects it as a global npm install so `teemiao agents update codex [--dry-run]` reports `manual action required` and exits `1` instead of running `npm install -g`. This also affects bulk `teemiao agents update` when the project-local entry earlier on `PATH` shadows a global Codex installation. Other agents follow their per-agent ownership rules above; OpenCode and Kimi can select their native updater from a project-local path.
    - The installation is on a **prerelease or pinned channel** (such as `1.0.0-beta.1` or a standalone Codex installation without `auto-update-version`); Teemiao refuses to overwrite it with `@latest` / `stable`. A marker such as `0.167.0-aarch64-apple-darwin` identifies a stable release for that platform and permits automatic updates.
    - Multiple package managers or stale wrappers match the binary (`contradictory ownership metadata detected`). Remove duplicate wrappers or stale global installs so only one manager owns the active binary on `PATH`.
    - The owning package manager (`npm`, `brew`, or `winget`) is not on the current `PATH`.
    - For Qoder (`qoder` / `qodercli`), native self-update requires `--help` to advertise `update`, and Homebrew/WinGet Qoder installations must be upgraded directly with `brew` or `winget`.
- **`unverified` reported after an update:**
  - The updater exited with code `0`, but either the post-update `--version` probe failed/timed out or the version remained unchanged without a recognized up-to-date message from the updater. Run `<agent> --version` directly to inspect the installation.
