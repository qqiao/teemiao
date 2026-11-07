# Quality constraints

Proposed for QQ-8, 2026-11-08. Implementation awaits the human's review of
`spec/agents/`. These constraints apply to the feature diff; they do not authorize
unrelated cleanup or changes to the user's installed agents during development.

## Floor

- No new lint suppressions, ignored/deleted tests, swallowed errors, or unfinished
  production branches. Do not weaken these rules to make a check pass.
- Keep credentials, environment dumps, and user configuration out of source and
  diagnostics. Never modify an agent's authentication or project settings.
- Use argument arrays and validated agent identifiers. Do not construct shell
  command strings from user input or discovered paths.
- Tests use isolated fake executables and fake package-manager records. They
  never install, update, or uninstall real agents or call model services.
- Preserve the existing CLI behavior and English/Chinese localization.

## Delivery checks

| Dimension | Required result | Command / evidence |
|---|---|---|
| Formatting | No differences | `cargo fmt --check` |
| Compilation | Successful build | `cargo build --locked` |
| Regression and feature tests | Zero failing or newly ignored tests | `cargo test --locked` |
| Lint | Zero warnings | `cargo clippy --locked --all-targets -- -D warnings` |
| Patch hygiene | No whitespace errors or unrelated changes | `git diff --check` and reviewed diff |
| Process safety | Probes bounded; update arguments and targets verified | Fake-process tests mapped to FR-004–FR-010 |
| Platforms | Tests/build succeed on macOS, Linux, Windows | Proposed CI matrix; record each actual result |

Run focused tests during each implementation slice, then all delivery checks at
handoff. Report CI as pending when it is still running; do not claim a platform
was validated from compilation on another platform alone.

## Baseline

Measured on macOS at `2bf8dbdadb546fef75af3d1ede1814b142e0471c`:

- `cargo test --locked`: 20 passed, 0 failed, 0 ignored.
- `cargo fmt --check`: passed.
- Strict Clippy: one existing `items_after_test_module` error at
  `src/main.rs:133` because `main()` follows the tests. Move the test module to
  the end while integrating the new command; do not suppress the lint.
- Coverage and cold-start performance have not been measured. No invented
  percentage or timing baseline is claimed. UI accessibility budgets do not
  apply to this command-line feature.

The proposed probe deadline is five seconds per child; captured probe output is
limited to 64 KiB per stream. Tests must demonstrate timeout and output-limit
handling. These are new feature requirements, not measured baseline results.
