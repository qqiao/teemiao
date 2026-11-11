# Tasks: F-05 upstream dependency upgrade

Tracking: QQ-11. Scope authorized by Qian Qiao's option-1 selection in comment
`01a1c98b-5452-7db6-b7fe-7ad6b567e778`. See `spec.md` and `plan.md`.

## F05-01 — Remove F-05 with rust-i18n 4.2.4

- Deliverable: one reviewable commit series removing the unmaintained YAML
  dependency while preserving localization and enforcing a clean CI audit.
- Owner: 吗喽; branch `work/qq-11-f05-01-malou` in an isolated checkout/worktree.
- Base: the documentation commit containing this task package on the overall
  branch; its exact SHA is supplied in the issue handoff.
- Prerequisites: selected option 1, existing CI-01 integration at `d3845b8`, and
  the documented task package. No competing implementation assignment.
- Likely files: Cargo.toml, Cargo.lock, .github/workflows/rust.yml,
  docs/security-review.md, CONTRIBUTING.md, CHANGELOG.md, and focused parity
  tests in the established harness. This is one coupled dependency-remediation
  slice; it does not include a localization redesign.
- Acceptance: FR-01 through FR-05 pass; old YAML crates are absent; strict audit
  has no vulnerabilities/warnings; both locales retain behavior; packaging
  succeeds; new dependency/security and performance evidence is supplied.
- Verification: all commands and evidence in `spec.md` and `plan.md`.
- Handoff: accessible branch, exact SHA, baseline failing strict audit and
  passing updated audit, parity/regression results, security and performance
  checks, actual platform/toolchain, and any unresolved limitations.
- State: complete. Submitted and independently approved at
  `108a8ee7a7b417ebffb26331219ecefc6e27eb6b`; integrated by fast-forward from
  `47b5ac29a87029e308f493b975a5b82836c5fe30`.

## F05-02 — Independently review and integrate

- Deliverable: verified F05-01 revision integrated into PR #28's overall branch.
- Owner: Leader.
- Prerequisite: F05-01 handoff.
- Acceptance: inspect the exact coder diff using code-review-and-quality;
  independently challenge/check it with doubt-driven-development; verify its
  merge with the current overall tip; record approval, resulting SHA, and
  checks on QQ-11. Conflicts and implementation fixes return to the coder.
- Files: review artifacts/task state and PR description; coder owns all fixes.
- State: complete. Review and local verification passed on the exact F05-01
  commit. Human acceptance follows issue `in_review`; default-branch merge and
  publication require separate direction.

## Independent review evidence — 2026-11-11

Leader reviewed correctness, readability, architecture, security, and performance
at `108a8ee7a7b417ebffb26331219ecefc6e27eb6b`. No blocking findings. The large test
diff is predominantly explicit bilingual expected values; production Rust code
and locale catalogs are unchanged. Existing tests also cover the parameterized
Chinese dry-run message, beyond the newly added localization checks.

Checks run independently on macOS arm64 with Rust 1.99.1:

- Full suite: 48 unit and 34 integration tests pass; no failures or ignored tests.
- 112 baseline/candidate CLI comparisons (8 locale configurations, 14 safe
  commands) match exit status, stdout, and stderr byte-for-byte with isolated
  empty PATH/HOME. Existing fake-agent tests pass.
- Adversarial mutation: temporarily changing the fallback from English to
  Chinese makes the new localization test fail for `fr-FR`; restoring the
  source makes it pass again. The review worktree is restored to the exact SHA.
- Baseline strict audit fails on RUSTSEC-2024-0449; the upgraded strict audit
  passes with 243 packages, zero vulnerabilities, and zero warnings against
  1,446 RustSec advisories. Both old YAML packages are absent from all edges.
- Lockfile comparison confirms 232 shared packages are entirely unchanged.
  Added package licenses/build scripts were inspected, both new parser crates
  forbid unsafe code, and rust-i18n's build script is unchanged. The new parser
  dependencies are absent from rust-i18n's normal non-proc-macro target tree.
- Formatting, strict Clippy, build, strict rustdoc, metadata inspection,
  checksum-verified actionlint 1.7.12, and diff hygiene pass.
- Clean-tree `cargo publish --dry-run --locked --registry crates-io` packages
  73 files and successfully compiles the archive; upload is skipped as expected.

Coder-supplied performance measurements were reviewed, not independently
repeated: clean release build median 22.14s to 21.23s, release binary size +1.59%,
and CLI startup differences within observed noise. The small size increase is
accepted; no runtime optimization or performance threshold change was made.

The leader's adversarial checks are independent of the author but do not add a
separate fresh-context reviewer. Cross-model review was skipped in this
non-interactive run. Native Linux/Windows results belong to the PR CI run after
integration and are not inferred from local macOS checks.
