---
name: hotpath_bump
description: Bump the hotpath version number across the workspace and related files. Updates crate versions in Cargo.toml files (exact patch version) and version references in the backend middleware, hotpath_init skill, and README (major.minor only). Use when the user wants to bump, bump the version, or release a new hotpath version.
allowed-tools: Bash, Read, Edit, Grep, Glob
---

# Bump hotpath Version

Bump the hotpath version number in every place it appears. The user provides the new version (e.g. `0.25.0`); if only `major.minor` is given, assume patch `0`.

Two formats are used:

- **Exact version** (`X.Y.Z`, e.g. `0.25.0`) - all Cargo.toml entries
- **Major.minor only** (`X.Y`, e.g. `0.25`) - all other references

## 1. Cargo.toml files - exact version `X.Y.Z`

Update the `version` field in the `[package]` section of each crate:

- `crates/hotpath/Cargo.toml`
- `crates/hotpath-macros/Cargo.toml`
- `crates/hotpath-meta/Cargo.toml`
- `crates/hotpath-macros-meta/Cargo.toml`

Update the `version` in the four `[workspace.dependencies]` entries in the root `Cargo.toml`:

```toml
hotpath-macros = { path = "./crates/hotpath-macros", version = "X.Y.Z" }
hotpath = { path = "./crates/hotpath", version = "X.Y.Z" }
hotpath-meta = { path = "./crates/hotpath-meta", version = "X.Y.Z" }
hotpath-macros-meta = { path = "./crates/hotpath-macros-meta", version = "X.Y.Z" }
```

All four crates always share the same version; never bump one without the others.

## 2. Other references - major.minor `X.Y`

- `../hotpath-backend/src/config/middleware.rs` (separate repo, sibling directory):

  ```rust
  const TEMPLATE_VARS: &[(&str, &str)] = &[("{{HOTPATH_VERSION}}", "X.Y")];
  ```

- `skills/hotpath_init/SKILL.md` - every version occurrence (`hotpath = "X.Y"`, `hotpath = { version = "X.Y", ... }`)
- `README.md` - every version occurrence (`cargo install hotpath --version '^X.Y'`, `hotpath = "X.Y"`)

## 3. Verify

Search for stragglers with the old version, excluding lockfiles, target dirs, and third-party code:

```bash
rg -n 'OLD_X\.OLD_Y' Cargo.toml README.md skills/ crates/hotpath/Cargo.toml crates/hotpath-macros/Cargo.toml crates/hotpath-meta/Cargo.toml crates/hotpath-macros-meta/Cargo.toml
rg -n 'OLD_X\.OLD_Y' ../hotpath-backend/src/config/middleware.rs
```

Ignore matches that are not hotpath version references (other dependencies may coincidentally share the number). Then run `cargo check` so `Cargo.lock` picks up the new versions.

## Rules

- Do not commit; leave changes for the user to review.
- Do not bump versions of test crates or any other dependency versions.
