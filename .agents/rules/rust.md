# Teemiao Project — Agent Rules

## Project Overview

Teemiao is a Rust CLI toolkit that streamlines application development workflows.
It is licensed under Apache-2.0 and authored by Qian Qiao.

## Rust Edition

All Rust code in this project **must** target the **2024 edition**. The `Cargo.toml`
already specifies `edition = "2024"` — never downgrade this. When writing code,
use idioms and features available in the 2024 edition (e.g. `gen` blocks, precise
capturing in `impl Trait`, etc.) where appropriate.

## Documentation

All code must be **thoroughly documented**:

- Every public item (`pub fn`, `pub struct`, `pub enum`, `pub trait`, `pub mod`,
  `pub type`, `pub const`, `pub static`) **must** have a doc comment (`///` or `//!`).
- Every module file must start with a module-level doc comment (`//!`) describing
  the module's purpose.
- Doc comments should follow Rust conventions:
  - Start with a single-line summary sentence.
  - Optionally include a blank doc-comment line followed by a more detailed
    explanation.
  - Use `# Examples`, `# Errors`, `# Panics`, and `# Safety` sections where
    applicable.
- Struct and enum fields should also have doc comments.
- Non-trivial private functions and internal logic should include regular comments
  (`//`) explaining the reasoning when it is not obvious.

## Linting — Use `cargo clippy`, Not `cargo check`

- **Always** run `cargo clippy` instead of `cargo check` when verifying code.
- **All** Clippy lint warnings and errors **must** be resolved before considering
  a change complete.
- When fixing lint issues, prefer addressing the root cause over suppressing with
  `#[allow(...)]`. Only use allow attributes when there is a justified reason, and
  always include a comment explaining why.
- The command to run is:
  ```
  cargo clippy --all-targets --all-features -- -D warnings
  ```

## License Headers

Every Rust source file must begin with the Apache-2.0 copyright header:

```
// Copyright 2026 Qian Qiao
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
```

This header must appear **before** any module doc comments or code.

## Error Handling

- Use `thiserror` for defining error types.
- Each module that can fail should define its own error enum.
- Error variants must have doc comments and descriptive `#[error("...")]` messages.
- Propagate errors with `?` — avoid `.unwrap()` and `.expect()` in non-test code
  unless the invariant is provably safe and documented.

## Internationalisation (i18n)

- This project uses `rust-i18n` with locale files in the `locales/` directory.
- User-facing strings (CLI help text, error messages shown to the user) should use
  `t!("key")` macro lookups rather than hard-coded English strings.
- The fallback locale is `"en"`.

## Code Style & Conventions

- Follow standard Rust formatting (`rustfmt` defaults).
- Use `clap` with derive macros for CLI argument parsing.
- Prefer structured logging via the `log` crate (`trace!`, `debug!`, `info!`,
  `warn!`, `error!`).
- Group imports in this order, separated by blank lines:
  1. `crate::` / `self::` / `super::` imports
  2. External crate imports
  3. `std` library imports
- Keep modules focused — one responsibility per module.

## Testing

- Place unit tests in a `#[cfg(test)] mod tests` block at the bottom of each file.
- Test function names should be descriptive of what they verify.
- Use `cargo test` to run the full test suite and ensure all tests pass.

## Dependencies

- Only add dependencies when truly necessary.
- Prefer well-maintained, widely-used crates from the Rust ecosystem.
- Keep `Cargo.toml` dependencies organized and avoid duplicates.

## Build & CI

- The GitHub Actions workflow is at `.github/workflows/rust.yml`.
- The release profile enables LTO and symbol stripping for optimised binaries.
- Always verify that changes build successfully and pass both `cargo test` and
  `cargo clippy` before finalising.
