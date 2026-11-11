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
- State: ready for dispatch; live state recorded on QQ-11.

## F05-02 — Independently review and integrate

- Deliverable: verified F05-01 revision integrated into PR #28's overall branch.
- Owner: Leader.
- Prerequisite: F05-01 handoff.
- Acceptance: inspect the exact coder diff using code-review-and-quality;
  independently challenge/check it with doubt-driven-development; verify its
  merge with the current overall tip; record approval, resulting SHA, and
  checks on QQ-11. Conflicts and implementation fixes return to the coder.
- Files: review artifacts/task state and PR description; coder owns all fixes.
- State: waiting for F05-01. Human acceptance follows issue `in_review`;
  default-branch merge and publication require separate direction.
