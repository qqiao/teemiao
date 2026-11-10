# Tasks: documentation and package metadata

**Issue:** QQ-10. All implementation tasks await T001. The leader owns this
package; the coder may append evidence without changing requirements or sequence.

- [ ] **T001 — Approve the complete review package** (Qian Qiao).
  Review spec.md, plan.md, this checklist, and existing CONSTRAINTS.md.
  Acceptance: explicit approval of scope, behavior-preservation boundary, and
  `0.2.2` target. Verify: approval comment on QQ-10. Dependency: none.

- [ ] **T002 [US1] — Document the actual user workflows** (Coder; medium).
  Audit README against CLI/source; produce linked CLI and agents references.
  Acceptance: FR-002–FR-005 covered, including build-info limitations, unfinished
  command, locale/verbosity, and complete agent behavior. Preserve useful links.
  Files: README.md, docs/cli.md, docs/agents.md, audit.md here.
  Verify: help in both locales, isolated build-info examples, fake-agent fixtures,
  source comparisons, and documentation links. Dependency: T001.

- [ ] **T003 [US1/US3] — Complete contributor guidance and documentation audit** (Coder).
  Add contributor/release guidance. Inspect all Rust docs, locale help, history,
  constraints, CI, and bundled skills; record dispositions and evidence in audit.md.
  Acceptance: FR-001/FR-005/FR-006 coverage; no stale product claims or unsupported
  approval/platform claims. Bring historical corrections to the leader.
  Files: CONTRIBUTING.md, audit.md, and inaccurate comments/help only. Make each
  correction slice at most five files; pair both locale files for help edits.
  Verify: commands match CI/constraints, help remains localized, Rust doc build,
  reviewed diff proves unchanged runtime behavior. Dependency: T002.

- [ ] **Checkpoint — Documentation coverage and examples** (Coder/Leader).
  All tracked documentation groups have a disposition; limitations are explicit;
  links and examples are checked. Leader resolves any design/history questions.
  Dependency: T003.

- [ ] **T004 [US2] — Supply useful Cargo package metadata** (Coder; small).
  Apply verified fields from plan.md, retain license/dependencies, check categories.
  Acceptance: FR-007; Cargo metadata contains meaningful values and the reproduced
  missing-information warning is absent without suppression.
  Files: Cargo.toml, audit.md.
  Verify: cargo metadata and cargo package --list; review packaged assets.
  Dependency: documentation checkpoint.

- [ ] **T005 [US3] — Prepare and verify patch 0.2.2** (Coder; medium).
  Bump the package version, write concise release notes, and run the full checks.
  Acceptance: FR-008/FR-009; CLI/manifest/lockfile agree; only root package version
  changes in the lockfile; publish dry-run verifies the archive without uploading.
  Files: Cargo.toml, Cargo.lock, CHANGELOG.md, audit.md; tasks.md evidence only.
  Verify: complete spec command list after committing intended changes; record
  results and four required self-reviews. Dependency: T004.

- [ ] **T006 — Independent leader review and PR delivery** (Leader/Coder).
  Leader reviews requirements, diff, tests, safety, packaging, and audit coverage;
  leader corrects historical design status only when supported by evidence.
  Coder resolves findings to the same standards. Create/update one PR to main.
  Acceptance: no unresolved material findings; exact reviewed commit and actual
  verification results recorded. Verify: reviewed diff, targeted reproductions,
  PR and issue evidence. Dependency: T005.

- [ ] **T007 — Human acceptance** (Qian Qiao).
  Review the leader-validated PR and address feedback before closure.
  Acceptance: explicit final acceptance; done remains human-owned.
  Verify: QQ-10 review discussion. Dependency: T006.
