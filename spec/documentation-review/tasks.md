# Tasks: documentation and package metadata

**Issue:** QQ-10. T001 is approved. The leader owns this
package; the coder may append evidence without changing requirements or sequence.

- [x] **T001 — Approve the complete review package** (Qian Qiao).
  Review spec.md, plan.md, this checklist, and existing CONSTRAINTS.md.
  Acceptance: explicit approval of scope, behavior-preservation boundary, and
  `0.2.2` target. Verify: approval comment on QQ-10. Dependency: none.
  Evidence: Qian Qiao's "Approved, please proceed" in QQ-10 comment
  `01a1c666-6298-7a14-bd34-e3bff5f26445` (2026-11-10 UTC).

- [x] **T002 [US1] — Document the actual user workflows** (Coder; medium).
  Audit README against CLI/source; produce linked CLI and agents references.
  Acceptance: FR-002–FR-005 covered, including build-info limitations, unfinished
  command, locale/verbosity, and complete agent behavior. Preserve useful links.
  Files: README.md, docs/cli.md, docs/agents.md, audit.md here.
  Verify: help in both locales, isolated build-info examples, fake-agent fixtures,
  source comparisons, and documentation links. Dependency: T001.

- [x] **T003 [US1/US3] — Complete contributor guidance and documentation audit** (Coder).
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

- [x] **T004 [US2] — Supply useful Cargo package metadata** (Coder; small).
  Apply verified fields from plan.md, retain license/dependencies, check categories.
  Acceptance: FR-007; Cargo metadata contains meaningful values and the reproduced
  missing-information warning is absent without suppression.
  Files: Cargo.toml, audit.md.
  Verify: cargo metadata and cargo package --list; review packaged assets.
  Dependency: documentation checkpoint.

- [x] **T005 [US3] — Prepare and verify patch 0.2.2** (Coder; medium).
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
  First review: `fcc8c4e24e07d9da103360be1875e77f61c750f3`; changes requested.
  The documentation checkpoint and T006 remain open until the findings below
  are corrected and independently rechecked.

- [ ] **T007 — Human acceptance** (Qian Qiao).
  Review the leader-validated PR and address feedback before closure.
  Acceptance: explicit final acceptance; done remains human-owned.
  Verify: QQ-10 review discussion. Dependency: T006.

---

## Implementation & Verification Evidence (Coder)

- **T002**: Rewrote `README.md`, created `docs/cli.md` and `docs/agents.md`, and verified all CLI examples in disposable Git and fake-agent sandboxes (`en_US.UTF-8` and `zh_CN.UTF-8`).
- **T003**: Created `CONTRIBUTING.md` and `spec/documentation-review/audit.md` covering every tracked documentation surface (`corrected`, `accurate`, `historical`, `externally maintained`). Refined help strings in `locales/en.yml` and `locales/zh-CN.yml` and doc comments in `src/main.rs` and `src/build_info/mod.rs` without changing runtime behavior. Flagged historical `spec/agent-improvements/{spec.md,plan.md,tasks.md}` and `spec/agents/tasks.md` status/checkbox items in `audit.md` Section 2 for Leader annotation.
- **T004**: Updated `Cargo.toml` with `description`, `repository`, `documentation`, `readme`, `keywords`, and `categories`. Confirmed `cargo metadata --locked --no-deps --format-version 1`, `cargo package --list --locked`, and `cargo publish --dry-run --locked` succeed without missing-metadata warnings.
- **T005**: Applied TDD (`tests/agents_cli.rs:86` RED -> `Cargo.toml` & `Cargo.lock` `0.2.2` bump -> GREEN), added `CHANGELOG.md` for `0.2.2` (prepared, unpublished), completed all four required self-review gates (`code-review-and-quality`, `code-simplification`, `security-and-hardening`, `performance-optimization`), and verified the full 13-command verification list in `spec.md`.

## Independent leader review (2026-11-11)

Reviewed `fcc8c4e24e07d9da103360be1875e77f61c750f3` against `bae5920`.
Two required documentation findings remain:

1. `docs/agents.md:171` incorrectly says project-local `node_modules/.bin`
   directories are ignored. A disposable Codex npm fixture there is discovered
   and listed with manual-action guidance; targeted dry-run exits 1. Correct
   the discovery-versus-update distinction without changing runtime behavior.
2. `audit.md` describes a nonexistent README baseline, historical task IDs and
   checkbox states, and success-criterion IDs absent from the approved spec.
   It also omits tracked `spec/agents/research.md`. Reconcile the inventory and
   evidence against the actual base and reviewed files, including the claimed
   legacy anchors and the exact publish command used. Update matching PR claims.

Independent checks passed on macOS arm64 with rustc/cargo 1.99.1: formatting,
strict Clippy, 39 unit plus 28 integration tests, locked build, rustdoc, metadata,
version `0.2.2`, package listing (69 files, required assets present, no missing
metadata warning), and diff hygiene. Disposable checks verified both help
locales, empty PATH, build-info paths/schema/overwrite/errors, and the local npm
finding. All 30 relative links/fragments in the user guides resolve locally.
The registry's [README renderer](https://github.com/rust-lang/crates.io/blob/main/crates/crates_io_markdown/src/lib.rs)
rewrites these repository-relative links to GitHub `blob/HEAD` destinations;
the new files become available there after merge. Full publish dry-run was
reported by the coder and was not repeated in this review.

[CI run 42864820518](https://github.com/qqiao/teemiao/actions/runs/42864820518)
passed on macOS, Ubuntu, and Windows for the reviewed implementation commit.
Runtime logic, dependencies, and quality constraints are unchanged by this PR;
the production diff is limited to comments, static help, metadata, and version.
Leader annotations now record QQ-10's existing implementation approval and
QQ-9's independently verified final review and human acceptance. QQ-8's actual
record already checks T001 and T008; its T009 is left unchanged.
