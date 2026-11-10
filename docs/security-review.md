# Security review — 0.2.3

Reviewed on 2026-11-11 for QQ-11 / PR #28, starting from `2591a48`.
The review covered all application modules, tests and fixtures, dependency
manifests, localization inputs, and the build workflow. It is a source review
with isolated reproductions, not a penetration-test certification.

## Trust boundaries

Teemiao is a local CLI running as its caller. Assets are the caller's installed
tools, selected update channels, filesystem, credentials inherited by vendor
processes, and the accuracy of update results. Inputs cross boundaries through
CLI arguments, PATH and environment, executable wrappers and package metadata,
subprocess output, Git repositories, and downloaded installer scripts.

The application has no HTTP server, login/session system, database, telemetry,
model-service calls, or file-upload API. Web authentication, CORS, and cookie
controls therefore do not apply. `build-info` uses gix and JSON serialization;
its output path is explicitly chosen by the caller and may overwrite a file.
The existing `config-template` placeholder still panics as documented; it
accepts no template or network input and is not a completed feature.

## Findings and disposition

| Finding | Assessment and remediation |
| --- | --- |
| F-01: Windows batch validation gaps | Fixed missing checks for downloader/interpreter requests, WinGet inventory probes, and batch arguments such as npm prefixes. Every request validates both the batch program path and arguments before dispatch, with a second check in the real runner. Tests cover control/expansion characters, quotes, safe spaces/parentheses, and native executable arguments. The audit established a validation bypass, not demonstrated arbitrary execution on a live Windows kernel. Rust also performs its own version-dependent batch escaping. |
| F-02: Standalone selection ambiguity | Fixed. A failed/unknown version probe, missing/broken current selection, invalid current manifest, or a launcher/wrapper pointing at a different release requires manual action. Canonical paths bind the marked release, current selection, and active target. Legacy real-current-directory layouts require matching metadata and version evidence. |
| F-03: Installer transport and resource limits | Fixed ambient curl configuration, redirect scheme, unbounded script read, and interpreter precedence. Curl receives `-q` first, HTTPS-only transfer/redirect flags, a 1 MiB maximum, 15-second connection and 120-second transfer limits, and a 125-second process deadline. Validation independently bounds reads. Unix prefers `/bin/sh`; Windows retains noninteractive, no-profile PowerShell with a file argument. |
| F-04: Inherited output pipes and timeout cleanup | Fixed indefinite post-exit joins: output draining has a one-second limit even when the updater has no execution deadline. Incomplete output cannot authorize success. Unix timed process groups use `libc::kill` instead of launching `/bin/kill`. Normal update signal behavior is preserved. |
| Additional ownership/output evidence gaps | Fixed accepting a wrapper's first 8 KiB while ignoring later commands, whole-binary reads for Kimi wrapper detection, unbounded ownership metadata reads, and accepting truncated updater evidence. Manifests/markers are capped at 1 MiB; wrappers must fit completely within 8 KiB. |
| CI token/action supply chain | A prepared patch adds `contents: read`, disables persisted checkout credentials, pins checkout to upstream `11d5960a326750d5838078e36cf38b85af677262`, and adds a separate lockfile-audit job. **Not applied:** GitHub rejected the workflow write because the workspace OAuth credential lacks `workflow` scope. The patch is attached to QQ-11 for an authorized maintainer. Existing CI remains in place. |
| F-05: Unmaintained YAML dependency | Deferred maintenance risk, detailed below. No known vulnerability was reported by the lockfile audit. |

The subprocess policy follows Rust's [Windows argument-splitting guidance](https://doc.rust-lang.org/std/process/index.html#windows-argument-splitting).
Download flags follow the [curl manual](https://curl.se/docs/manpage.html).

## Dependency audit and deferral

`cargo-audit 0.23.0` inspected 239 locked packages against RustSec database commit
`28d73fc95e5972af3403225a217def57dcaba3c4`, updated
2026-11-10 14:45:03 +01:00 (1,446 advisories): **zero known vulnerabilities**,
one informational unmaintained warning.

[RUSTSEC-2024-0449](https://rustsec.org/advisories/RUSTSEC-2024-0449.html)
affects `unsafe-libyaml 0.2.11`, reached through `serde_yaml` and `rust-i18n`'s
macro/support dependencies. The application compiles its tracked locale YAML
into static lookup tables; no application runtime path parses user-supplied
YAML. The build dependency remains relevant when reviewing locale changes.
There is no patched release for this maintenance advisory. Replacing the
localization stack is deferred to avoid an unrelated migration in this patch.
Maintainers should review upstream alternatives by **2026-12-11**, or before
the next release if earlier. No advisory suppression was added.

The only dependency declaration added is Unix `libc`, already present in the
lockfile; no dependency version or checksum changed.

## Verification and limits

Seven focused security tests cover batch dispatch, active Codex selection,
oversized ownership evidence, inherited pipes, installer-size boundaries, and
incomplete update output. The original four reported behavioral failures and
the wrapper/output findings were reproduced before their fixes. The full
suite passes on macOS arm64; Windows resolution and Job Object logic also have
simulation tests. Native Linux/Windows results come from the PR's CI matrix,
not from macOS simulation.

Pattern scans of tracked source and all local Git history found no private-key
headers or common GitHub, AWS, and service-token formats. These scans do not
establish the absence of every possible secret. No real agent updates, model
calls, or live installer execution were used for testing.

Remaining trust and compatibility limits:

- PATH executables, the upstream HTTPS installer, CA/proxy configuration, and
  files writable by the caller remain trusted. This CLI is not a sandbox for
  malicious local programs, and script marker checks are not signatures.
- Curl older than 8.4.0 may not stop an unknown-length response at the download
  size threshold. Independent validation still rejects oversized scripts and
  caps memory reads; the transfer/process deadlines bound duration.
- A detached helper may retain a pipe until it exits. Teemiao returns an
  unverified result after the drain limit; reader threads can remain until
  pipe closure or CLI exit. It does not kill persistent updater helpers.
- This review does not address a malicious same-user process racing filesystem
  checks, OS compromise, or unknown upstream vulnerabilities. Re-run the
  lockfile audit and platform checks before publication.
