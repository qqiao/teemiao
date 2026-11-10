# CLI Reference

This document covers Teemiao's command-line interface, global options, `build-info` behavior and JSON schema, localization rules, exit codes, and current implementation status. For AI coding-agent discovery and update details, see [AI Coding-Agent CLI Reference (`docs/agents.md`)](agents.md).

## Synopsis

```sh
teemiao [OPTIONS] <COMMAND>
```

Running `teemiao` with no arguments prints help to `stderr` and exits with status code `2`.

### Subcommand Status Overview

| Command | Status | Summary |
|---|---|---|
| `teemiao build-info [FILE]` | Implemented | Writes Git `HEAD` short revision and UTC build timestamp JSON to `FILE` (default `./build_info.json`). |
| `teemiao agents list` | Implemented | Lists supported AI coding-agent CLIs discovered on `PATH`. See [`docs/agents.md`](agents.md). |
| `teemiao agents update [AGENT] [--dry-run]` | Implemented | Previews or runs updates for one or all detected AI coding-agent CLIs on `PATH`. See [`docs/agents.md`](agents.md). |
| `teemiao config-template` | **Unfinished (panics)** | Placeholder subcommand; invoking `teemiao config-template` executes `todo!("config template")` and panics. |

## Global Options

| Option | Description |
|---|---|
| `-v`, `--verbose`... | Increase log verbosity (`-v` = `INFO`, `-vv` = `DEBUG`, `-vvv` = `TRACE`). |
| `-q`, `--quiet`... | Decrease log verbosity (`-q` = `ERROR` only, `-qq` = logging disabled). |
| `-h`, `--help` | Print short (`-h`) or long (`--help`) localized help information. |
| `-V`, `--version` | Print `teemiao <version>` (`teemiao 0.2.2`) to `stdout`. |

### Logging and Verbosity

Teemiao initializes `env_logger` from `clap-verbosity-flag` using `WarnLevel` as the default filter:

- **Default (no flags):** `WARN` and `ERROR` log records are enabled.
- **`-v` (`INFO`):** Logs high-level `build-info` progress to `stderr` (`Generating build info...`, `Build info successfully written to ...`).
- **`-vv` (`DEBUG`):** Adds resolved output file path logging (`Output file: ...`).
- **`-vvv` (`TRACE`):** Adds step-by-step working-directory, Git repository open, `HEAD` lookup, short revision, and serialized struct traces.
- **`-q` (`ERROR`) / `-qq` (`OFF`):** Suppresses `log` crate records on `stderr`.

> **Note:** Verbosity flags only filter `log` records emitted by `build-info`. Direct `stderr` error messages (such as `Error generating build info: ...`) and all `teemiao agents` output on `stdout` and `stderr` are written directly to standard streams rather than through `env_logger`.

## Build Information Generator (`teemiao build-info`)

### Syntax

```sh
teemiao [OPTIONS] build-info [FILE]
```

- `[FILE]` is an **optional positional argument** (not a `--out` or `-o` flag).
- When `[FILE]` is omitted, Teemiao writes to `./build_info.json` in the current working directory.

### Examples

Write to the default `./build_info.json` in the current Git repository root:

```sh
teemiao build-info
```

Write to a custom file path (whose parent directory already exists) with `INFO` logging enabled:

```sh
teemiao -v build-info target/build_info.json
```

### JSON Output Schema

`teemiao build-info` writes a pretty-printed UTF-8 JSON object containing **exactly two fields**:

```json
{
  "revision": "bae5920",
  "build_time": 1794326400
}
```

| Field | JSON Type | Rust Type | Value |
|---|---|---|---|
| `revision` | `string` | `String` | Shortened Git commit hash of `HEAD` in the current working directory's repository (`head.id().shorten()`). |
| `build_time` | `number` (integer) | `i64` | Current UTC Unix timestamp in seconds since `1970-01-01T00:00:00Z` (`chrono::Utc::now().timestamp()`). |

No additional fields (such as branch names, tags, compiler versions, or dirty-tree flags) are emitted, and JSON is never written to `stdout`.

### Operational Requirements and Limitations

1. **Repository-root invocation (`cwd`):** Teemiao opens the Git repository at the canonicalized current working directory using `gix::open(&cwd)`. It does **not** search parent directories (`gix::discover` is not used). Run `teemiao build-info` from the directory containing `.git`.
2. **Valid `HEAD` commit required:** The repository's `HEAD` reference must resolve to an existing commit ID. Running in a newly initialized repository with an unborn `HEAD` fails with `Head ID not found` (or a reference lookup error).
3. **Existing parent directory required:** Teemiao opens `[FILE]` with `std::fs::File::create`. If the parent directory of `[FILE]` does not exist, file creation fails with an I/O error; missing parent directories are not created automatically.
4. **Overwrite behavior:** If `[FILE]` already exists, `std::fs::File::create` truncates and overwrites it.
5. **Exit code `0` on generation errors (current limitation):** In `src/main.rs`, if `build_info.run()` returns an error, Teemiao prints `Error generating build info: <error>` (`生成构建信息时出错：<error>` in `zh-CN`) to `stderr` and **still returns exit code `0` (`ExitCode::SUCCESS`)**. Because a failure during repository opening or `HEAD` resolution returns before `std::fs::File::create` truncates an existing destination file, build scripts and CI pipelines should remove any pre-existing target file (or write to a fresh path) before invoking `teemiao build-info`, then verify that the newly written JSON file exists and is valid (and that `stderr` contains no error output) rather than relying on the process exit code or file existence alone.

## Unfinished Command (`teemiao config-template`)

`teemiao config-template` is registered in the CLI parser as a placeholder for future configuration-template generation, but its execution branch in `src/main.rs` is `todo!("config template")`.

- Running `teemiao config-template --help` safely displays help text and exits with `0`.
- Running `teemiao config-template` **panics** with `not yet implemented: config template` and exits with status `101`. Do not invoke `teemiao config-template` in production or automation scripts.

## Localization (`en` and `zh-CN`)

Teemiao detects the user's locale from environment variables before `clap` parses command-line arguments so that `--help` output and command messages use the active locale.

### Environment Variable Precedence

`detect_locale()` inspects the following variables in order, using the first variable that is set in the environment:

1. `LC_ALL`
2. `LC_MESSAGES`
3. `LANG`

If an earlier variable in this list is set to an empty string (for example `LC_ALL=""`), `std::env::var` succeeds with `""` and does **not** fall through to `LC_MESSAGES` or `LANG`; `parse_locale_string("")` then returns `"en"`.

### Locale Normalization and Fallback

`parse_locale_string()` normalizes the raw value as follows:

1. Strips encoding suffixes (everything from `.` onward, such as `.UTF-8` or `.gb2312`) and modifiers (everything from `@` onward, such as `@euro` or `@latin`).
2. Treats `""`, `"C"`, and `"POSIX"` (including `"C.UTF-8"` and `"POSIX.UTF-8"`) as `"en"`.
3. Replaces `_` with `-` and normalizes casing to `<lowercase-language>-<UPPERCASE-REGION>` (for example, `zh_CN.UTF-8` and `zh-cn` both normalize to `"zh-CN"`, and `en_US.UTF-8` normalizes to `"en-US"`).
4. Sets the locale via `rust_i18n::set_locale(&locale)` with `fallback = "en"`.

### Bundled Catalogs and Translation Coverage

| Catalog | File | Activation Examples |
|---|---|---|
| English (default & fallback) | `locales/en.yml` | `LC_ALL=en_US.UTF-8`, `LC_ALL=C`, unset environment, or any locale without a dedicated catalog (e.g. `fr_FR.UTF-8`, `zh_TW.UTF-8`). |
| Simplified Chinese | `locales/zh-CN.yml` | `LC_ALL=zh_CN.UTF-8`, `LC_MESSAGES=zh_CN`, `LANG=zh-CN`. |

Preview localized help in either language:

```sh
LC_ALL=en_US.UTF-8 teemiao --help
LC_ALL=zh_CN.UTF-8 teemiao --help
LC_ALL=zh_CN.UTF-8 teemiao agents update --help
```

**What is localized:**
- Subcommand and argument help descriptions (`--help` and `-h`).
- `build-info` log messages and the top-level `Error generating build info: ...` prefix.
- `agents list` and `agents update` section headers, summary headers, outcome templates, empty-state messages, not-installed errors, and probe warning templates.

**What remains in English:**
- `clap`'s built-in headings (`Usage:`, `Commands:`, `Arguments:`, `Options:`) and argument-validation error prefixes.
- Underlying `gix`, `std::io`, and `serde_json` error details interpolated into `%{error}`.
- Internal ownership/update `reason` and `guidance` strings interpolated into `%{reason}` and `%{guidance}` in `teemiao agents`.

## Exit Codes Summary

| Command / Condition | Exit Code | Notes |
|---|---|---|
| `--help`, `-h`, `--version`, `-V` | `0` | Printed to `stdout`. |
| Missing required subcommand or invalid CLI arguments | `2` | Rejected by `clap` on `stderr`. |
| `teemiao build-info [FILE]` (success **or** runtime error) | `0` | Runtime errors print to `stderr` (`Error generating build info: ...`) but return `ExitCode::SUCCESS`. |
| `teemiao config-template` | `101` | Panics with `not yet implemented: config template`. |
| `teemiao agents list` | `0` | Always exits `0` (including when no agents are found or when a version probe warns). |
| `teemiao agents update ...` (all selected agents `updated`, `up to date`, or `dry-run` plan; or empty bulk selection) | `0` | See [`docs/agents.md`](agents.md#exit-codes). |
| `teemiao agents update ...` (named agent not installed, `manual action required`, `unverified`, or `failed`) | `1` | See [`docs/agents.md`](agents.md#exit-codes). |
| `teemiao agents update ...` (interrupted by `SIGINT`/`SIGTERM` or child exit `130`/`143`) | `130` | Remaining queued agents are not started. |
