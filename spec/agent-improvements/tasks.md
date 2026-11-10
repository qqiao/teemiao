# Tasks: agent command improvements

**Issue:** QQ-9. Historical checklist; implementation and final acceptance are
complete. Leader reconciliation below cites the issue's recorded review and
human acceptance; merge alone is not treated as approval evidence.

- [x] **T001 — Approve the review package** (Qian Qiao; HumanReviewer).
  Review `spec.md`, `plan.md`, this checklist, and the existing CONSTRAINTS.md.
  Acceptance: explicit approval or requested revisions, including Qoder coverage,
  native/npm ownership boundaries, presentation, and `0.2.1` version target.
  Verify: approval recorded on QQ-9 before coder dispatch. Dependency: none.

- [x] **T002 [US1] — Separate each agent's update output** (Coder; medium).
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

- [x] **T003 [US2] — Add Qoder CLI management** (Coder; medium).
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

- [x] **Checkpoint — Behavior and constraint review** (Coder/Leader).
  Both slices pass focused tests. No changed ownership safeguards, skipped tests,
  new dependencies, probe-limit regressions, or unapproved specification changes.
  Bring uncertain native identity evidence to the leader before proceeding.
  Dependency: T003.

- [x] **T004 [US3] — Document support and bump the patch version** (Coder; medium).
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

- [x] **T005 — Independent adversarial review and PR delivery** (Leader/Coder).
  Leader reviews implementation, tests, and security against every requirement
  and independently runs appropriate checks. Coder addresses findings with the
  same TDD/review standards; repeat until no material findings remain.
  Acceptance: one PR to `main` with exact reviewed revision, behavior changes,
  tests, platform evidence, and limitations. No merge or release publication.
  Verify: leader review and PR link recorded on QQ-9. Dependency: T004.
  Evidence: final independent review passed for `a97429204b908b127885a490c570e116b50dcd55`
  in QQ-9 comment `01a1c40f-1860-71a9-8b78-0e02491b28e3` (2026-11-10 UTC),
  with all reported findings resolved and 39 unit plus 28 process tests passing.

- [x] **T006 — Human acceptance** (Qian Qiao; HumanReviewer).
  Leader requests final review after validating the complete delivery and sets
  parent issue to `in_review`. Address feedback until explicitly accepted.
  Verify: acceptance on QQ-9; `done` stays human-owned. Dependency: T005.
  Evidence: Qian Qiao confirmed the PR merge in QQ-9 comment
  `01a1c65b-36aa-7116-ac56-db89f71e68e2` and wrote "done, you may now close
  this issue" in `01a1c66c-7fd1-7e8d-8022-77eead15020b` (2026-11-10 UTC).

## Implementation & Verification Evidence (T002–T004)

- **Local Environment:** `Darwin 27.2.0 arm64` (macOS), `rustc 1.99.1`, `cargo 1.99.1`.
- **Commands & Results (local macOS; Linux/Windows covered by cross-platform unit harness & CI):**
  - `cargo test --locked agents` -> passed (`19` unit tests in `src/agents/mod.rs` + `28` integration tests in `tests/agents_cli.rs`).
  - `cargo test --locked --test agents_cli` -> passed (`28` process integration tests).
  - `cargo fmt --check` -> passed (`0` diffs).
  - `cargo clippy --locked --all-targets -- -D warnings` -> passed (`0` warnings).
  - `cargo test --locked` -> passed (`39` unit tests + `28` integration tests, `0` failed, `0` ignored).
  - `cargo build --locked` -> passed.
  - `cargo run --locked -- --version` -> `teemiao 0.2.1`.
  - `git diff --check` -> passed (`0` whitespace/formatting errors).
  - `git diff Cargo.lock` -> only root `teemiao` package version changed from `0.2.0` to `0.2.1`; all dependency versions unchanged.
- **Fake-Fixture Output Samples:**
  - `stderr` (multi-agent update with multiline/no-trailing-newline diagnostics):
    ```text
    --- [1/2] Antigravity (antigravity) ---
    Diagnostics for Antigravity (antigravity): agy note line 1
    agy note line 2
    Downloading Antigravity 1.1.0
    Applying update without final newline

    --- [2/2] Kimi (kimi) ---
    Diagnostics for Kimi (kimi): Upgraded kimi to 0.5.0
    ```
  - `stdout` (`en` and `zh-CN` separated summary):
    ```text

    Update summary
    Antigravity (antigravity): updated (1.0.0 -> 1.1.0; path: /tmp/.../bin/agy)
    Kimi (kimi): updated (0.4.0 -> 0.5.0; path: /tmp/.../bin/kimi)
    ```
    ```text

    更新摘要
    Antigravity (antigravity): 演练模式 (将运行: agy update; 目标: /tmp/.../bin/agy; 当前版本: 1.0.0)
    ```
  - Qoder discovery & update (`en`):
    ```text
    Qoder (qoder) | version: 0.9.1 | path: /tmp/.../bin/qodercli | update: native (qodercli update)
    ```
    ```text
    --- [1/1] Qoder (qoder) ---
    Diagnostics for Qoder (qoder): Updated Qoder CLI to 0.9.1
    ```
    ```text

    Update summary
    Qoder (qoder): updated (0.9.0 -> 0.9.1; path: /tmp/.../bin/qodercli)
    ```
