# Tasks: agent command improvements

**Issue:** QQ-9. All implementation tasks await T001. Specs and plans are owned
by the leader; the coder may append completion evidence to this checklist.

- [ ] **T001 — Approve the review package** (Qian Qiao; HumanReviewer).
  Review `spec.md`, `plan.md`, this checklist, and the existing CONSTRAINTS.md.
  Acceptance: explicit approval or requested revisions, including Qoder coverage,
  native/npm ownership boundaries, presentation, and `0.2.1` version target.
  Verify: approval recorded on QQ-9 before coder dispatch. Dependency: none.

- [ ] **T002 [US1] — Separate each agent's update output** (Coder; medium).
  Add failing output-order tests, then header/newline/summary rendering at the
  current orchestration boundary with both locale keys. Include dry-run and
  single-agent output; retain final result rows and stderr/stdout split.
  Acceptance: each attempted agent's diagnostics are confined to a named block;
  summary is visibly separate; empty/cancel/failure behavior remains correct.
  Verify: `cargo test --locked agents` and
  `cargo test --locked --test agents_cli`; inspect fake multiline and no-final-
  newline output and assert the header is written before the updater is invoked.
  Files: `src/agents/mod.rs`, `locales/en.yml`, `locales/zh-CN.yml`,
  `tests/agents_cli.rs`; `tests/support/mod.rs` only if fixtures need extension.
  Dependency: T001. Requirements: FR-001–FR-003.

- [ ] **T003 [US2] — Add Qoder CLI management** (Coder; medium).
  Use test-driven-development to extend the catalog, alias parsing, discovery,
  verified native/npm ownership, and result matching as one functional slice.
  Acceptance: Qoder lists once after existing agents; targeted/bulk/dry-run
  flows use the selected executable or exact owning npm prefix; unsafe or
  ambiguous cases cannot run updates or report false success.
  Verify: focused unit/process tests covering both executable names and their
  precedence, CLI-vs-desktop identity, absent Qoder, unknown versions, verified
  native update, npm success/wrong prefix/missing manager, unsupported manager,
  zero dry-run mutations, partial failure, and unchanged/negated success text.
  Cover Windows shim/path cases with the existing cross-platform harness.
  Files: `src/agents/mod.rs`, `src/agents/discovery.rs`, `src/agents/update.rs`,
  `tests/agents_cli.rs`, optionally `tests/support/mod.rs`.
  Dependency: T002 (shared files). Requirements: FR-004–FR-007.

- [ ] **Checkpoint — Behavior and constraint review** (Coder/Leader).
  Both slices pass focused tests. No changed ownership safeguards, skipped tests,
  new dependencies, probe-limit regressions, or unapproved specification changes.
  Bring uncertain native identity evidence to the leader before proceeding.
  Dependency: T003.

- [ ] **T004 [US3] — Document support and bump the patch version** (Coder; medium).
  Add Qoder examples/update boundaries and output explanation to README and both
  localized help texts. Change Teemiao to `0.2.1` in manifest/root lock entry.
  Acceptance: documented behavior matches tests; all three version surfaces
  agree; dependency versions are unchanged.
  Verify: all delivery commands in `spec.md`; review lockfile diff and
  `cargo run --locked -- --version`. Record local OS, exact commands/results,
  fake-output samples, and CI status without claiming unrun platforms passed.
  Complete code-review-and-quality, code-simplification, security-and-hardening,
  and performance-optimization reviews and fix findings before reporting done.
  Files: `README.md`, both locale files, `Cargo.toml`, `Cargo.lock`.
  Dependency: behavior checkpoint. Requirements: FR-003, FR-008.

- [ ] **T005 — Independent adversarial review and PR delivery** (Leader/Coder).
  Leader reviews implementation, tests, and security against every requirement
  and independently runs appropriate checks. Coder addresses findings with the
  same TDD/review standards; repeat until no material findings remain.
  Acceptance: one PR to `main` with exact reviewed revision, behavior changes,
  tests, platform evidence, and limitations. No merge or release publication.
  Verify: leader review and PR link recorded on QQ-9. Dependency: T004.

- [ ] **T006 — Human acceptance** (Qian Qiao; HumanReviewer).
  Leader requests final review after validating the complete delivery and sets
  parent issue to `in_review`. Address feedback until explicitly accepted.
  Verify: acceptance on QQ-9; `done` stays human-owned. Dependency: T005.
