# Specification: remove F-05 through the upstream localization upgrade

Issue: QQ-11. Baseline: `d3845b877543a60335124249c79c79e1f5a34d23`.
Overall branch: `agent/leader/3d7d54ef74f3`, PR #28 targeting `main`.

## Authorization and objective

Qian Qiao selected option 1 with "go with option 1" in comment
`01a1c98b-5452-7db6-b7fe-7ad6b567e778` on 2026-11-11. The preceding options audit
(`01a1c97a-af08-73e6-8c4f-83ec9d8a6f9d`) described the upstream upgrade,
strict dependency audit, localization parity, packaging, and performance checks.
These documents record that selected scope; they do not claim the reviewer
previously reviewed this later documentation commit.

Remove `unsafe-libyaml` / RUSTSEC-2024-0449 from Teemiao's dependency graph while
preserving its CLI and English/Simplified Chinese translations. Integrate into
the existing open PR, retaining the already prepared package version 0.2.3.

## Requirements and acceptance

- **FR-01 — Upstream dependency:** Replace `rust-i18n` 3.1.5 with 4.2.4 through
  crates.io. Use `rust-i18n = "4.2.4"` as the compatible version requirement so
  the manifest cannot resolve to an older 4.x release predating the parser fix.
  Update only this dependency subgraph. Both `serde_yaml` and `unsafe-libyaml`
  must disappear from Cargo.lock and all normal, build, and proc-macro edges.
- **FR-02 — Behavior preservation:** Retain the YAML catalogs, CLI flags, output,
  exit behavior, and locale precedence/fallback. Verify English and Simplified
  Chinese help, interpolation, punctuation, and multiline strings, including
  English fallback for unsupported locales. Establish expected values from the
  existing catalogs and executable; do not assume the audit's inconsistent
  59/60 key counts are an acceptance threshold. New parity tests must not race
  on process-global locale state.
- **FR-03 — Audit:** `cargo audit --deny warnings` must pass with a current
  advisory database. Apply that command to the existing CI audit job, retaining
  its tool pin, timeout, permissions, and three-platform build matrix. No
  advisory suppressions or other weakened checks.
- **FR-04 — Documentation:** Close F-05 in `docs/security-review.md` only after
  checking removal and audit results. Update directly affected contributing
  instructions and the 0.2.3 changelog. Record actual dependency counts and
  advisory freshness; preserve historical specifications.
- **FR-05 — Delivery:** Required Rust, workflow, localization, and package checks
  pass on the submitted commit. Record security review of added dependencies
  and before/after build/runtime measurements. Report external CI honestly when
  pending. The leader reviews the exact coder SHA before integration.

## Context, style, and interface contract

Rust 2024 binary; Cargo.toml/Cargo.lock control dependencies; `src/main.rs`
initializes rust-i18n and normalizes locales. Catalogs are `locales/en.yml` and
`locales/zh-CN.yml`; tests live in source and `tests/`, with fake tools in
`tests/support/mod.rs`. CI is `.github/workflows/rust.yml`.

No public API change is intended. Existing calls such as
`rust_i18n::i18n!("locales", fallback = "en")` and `t!(...)` retain their
observable behavior. Follow existing Rust naming and rustfmt conventions.
Bring any required contract or behavior change to the leader before proceeding.

## Verification

Use the existing test harness for parity checks and fake-agent smoke tests.
Record the baseline strict-audit failure caused by F-05, then its passing result
after the upgrade. Parity tests are expected to pass on both versions; do not
fabricate a failing behavior test for an unchanged behavior. Any actual behavior
fix follows a focused failing test, implementation, and passing regression test.

```sh
cargo tree --locked -e all -i unsafe-libyaml
cargo tree --locked -e all -i serde_yaml
cargo audit --deny warnings
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo metadata --locked --no-deps --format-version 1
actionlint .github/workflows/rust.yml
cargo package --list --locked
cargo publish --dry-run --locked
git diff --check
```

After removal, each inverse tree query is expected to report that its package
is absent; distinguish that from an unrelated command or registry failure.
Run packaging on a committed clean tree without dirty-tree/verification bypasses.
Use `--registry crates-io` for publish dry-run if required by local mirror setup.

## Boundaries

Always follow CONSTRAINTS.md, inspect lockfile additions, and validate both
locales. Keep developer-machine data and secrets out of evidence. Use isolated
fake tools for agent behavior/performance checks.

Return scope or contract changes to the leader. Do not migrate locale formats,
fork libraries, refresh unrelated dependencies, weaken quality gates, run real
agent updates, publish a crate, tag a release, or merge to `main`.

## Source references

- [rust-i18n 4.2.4](https://github.com/longbridge/rust-i18n/releases/tag/v4.2.4)
- [Upstream YAML parser replacement](https://github.com/longbridge/rust-i18n/pull/144)
- [4.0 migration notes](https://github.com/longbridge/rust-i18n/releases/tag/v4.0.0)

No unresolved product decision is required within the selected option.
