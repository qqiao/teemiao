# Implementation plan: F-05 upstream dependency upgrade

The selected scope and authorization are recorded in `spec.md`. Task definitions
are in `tasks.md`; assignment, branch, blockers, and review results are tracked
on QQ-11. The leader owns these documents and integration into the established
overall branch `agent/leader/3d7d54ef74f3` / PR #28.

## Execution and dependencies

`Approved option 1 -> F05-01 implementation -> F05-02 independent review/integration`.

Use one coder for F05-01: dependency removal, the strict CI audit, and accurate
security documentation must land together. A second implementation assignment
would overlap the lockfile or prematurely claim F-05 is closed.

1. On an isolated coder branch, record baseline audit, localization behavior,
   build time, release size, and safe CLI startup measurements.
2. Add meaningful parity coverage where missing. Update the intended dependency
   subgraph to rust-i18n 4.2.4 and inspect the resulting graph and lockfile diff.
3. Enforce strict auditing in CI; validate the workflow and update current docs
   with measured evidence. Retain package version 0.2.3.
4. Run the spec's verification commands, commit, verify packaging from the clean
   tree, push the coder branch, and hand off its exact SHA and check results.
5. The leader reviews and independently verifies that SHA against the current
   overall branch, returns defects/conflicts to the coder, and integrates only
   the reviewed revision. Update the existing PR description after integration.

## Risks and checks

| Risk | Check |
| --- | --- |
| Major-version dependency changes translation behavior | Compare both locales, interpolation, multiline help, and fallback with baseline; check upstream migration notes |
| Lockfile retains the advisory through another edge | Inspect all-edge dependency trees and strict audit |
| New dependencies add licensing or build-script exposure | Inspect each added package, its license, provenance, and build scripts before accepting |
| Locale tests interfere through global state | Prefer isolated CLI processes or explicit per-call locale selection |
| Broad lockfile churn obscures the intended change | Use a targeted update and explain every unrelated-looking dependency delta |
| Packaged crate resolves differently | Verify the archive with publish dry-run and the declared minimum version |
| Build/startup regressions | Compare like-for-like baseline and upgraded measurements; explain material differences |

No performance optimization is requested. Measure clean build time, release
binary size, and safe CLI startup on the available host; do not substitute a
host measurement for Linux/Windows evidence. Preserve cross-platform CI and
report any unavailable measurements as limitations. Do not wait for CI unless
the user explicitly asks for its result.
