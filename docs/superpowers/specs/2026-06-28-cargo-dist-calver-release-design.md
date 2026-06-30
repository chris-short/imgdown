# cargo-dist + CalVer Release Pipeline Design

**Date:** 2026-06-28
**Status:** Approved

## Goal

Replace imgdown's source-build Homebrew formula and ad-hoc release workflow with a cargo-dist pipeline that publishes pre-built native binaries for all major platforms. End users install imgdown via `brew install chris-short/imgdown/imgdown` without Rust or any build toolchain.

## Versioning

CalVer: `0.YYYYMMDD.run_number` (e.g., `0.20260628.42`). Auto-generated on every push to `main`. Matches the chkcerts pattern. Git tag is `v0.YYYYMMDD.run_number`; Cargo.toml version is the same without the `v` prefix.

## Release Tool

**cargo-dist** — the canonical Rust tool for distributing cross-compiled binaries with Homebrew tap support. Handles cross-compilation, GitHub Release creation, tarball packaging, SHA256 computation, and Homebrew formula generation automatically.

## Target Platforms

- `aarch64-apple-darwin` (macOS Apple Silicon)
- `x86_64-apple-darwin` (macOS Intel)
- `x86_64-unknown-linux-gnu` (Linux x86_64)
- `aarch64-unknown-linux-gnu` (Linux ARM64)

Windows is excluded — no demand, adds complexity.

## Components

### 1. homebrew-imgdown repo (new)

Create `chris-short/homebrew-imgdown` as an empty GitHub repo. cargo-dist pushes the generated formula here on every release. Users tap it via `brew tap chris-short/imgdown`.

### 2. HOMEBREW_TAP_GITHUB_TOKEN secret

A GitHub PAT with `repo` scope on `homebrew-imgdown` must be added as a secret named `HOMEBREW_TAP_GITHUB_TOKEN` in the `imgdown` repo. Same pattern as chkcerts.

### 3. cargo-dist config in Cargo.toml

Add `[package.metadata.dist]` to `Cargo.toml` specifying targets, the tap repo, and installer type (homebrew). cargo-dist reads this to know what to build and where to push the formula.

### 4. tag.yml workflow (new)

Triggers on: `push` to `main` (excluding pushes that only touch `Cargo.toml`/`Cargo.lock` to avoid re-triggering on version bump commits — enforced via `[skip ci]` in commit message).

Steps:
1. Compute CalVer version string and tag
2. Update `Cargo.toml` version field via sed
3. Regenerate `Cargo.lock` via `cargo update -p imgdown`
4. Commit with message `chore: bump version to X.Y.Z [skip ci]`
5. Create and push the git tag
6. Push the commit to main

GITHUB_TOKEN pushes do not re-trigger the same workflow, and `[skip ci]` prevents `rust.yml` from running on the bump commit.

### 5. release.yml workflow (cargo-dist generated)

Triggers on: `push: tags: ["v[0-9]+.*"]`

cargo-dist generates this file via `cargo dist init`. It:
- Builds native binaries on macOS runners for darwin targets
- Builds Linux binaries on ubuntu runners
- Creates a GitHub Release with tarballs
- Pushes updated formula to `homebrew-imgdown` using `HOMEBREW_TAP_GITHUB_TOKEN`

### 6. Delete main.yml

The existing `.github/workflows/main.yml` (semver-tag-triggered, single-arch, no tap update) is removed entirely.

### 7. rust.yml unchanged

The existing CI workflow (`cargo build`, `cargo test`, `cargo audit`) stays as-is.

### 8. homebrew-tap cleanup

Delete `Formula/imgdown.rb` from `chris-short/homebrew-tap`.

### 9. README update

Replace current Homebrew install instructions with:
```
brew install chris-short/imgdown/imgdown
```

## Data Flow

```
push to main
  → tag.yml: bump Cargo.toml, commit [skip ci], push tag
    → release.yml (cargo-dist): build binaries per target
      → GitHub Release: tarballs uploaded
      → homebrew-imgdown: formula updated with URLs + SHA256
```

## What End Users See

```sh
brew install chris-short/imgdown/imgdown
# Downloads pre-built binary for their platform, no Rust required
```

## Out of Scope

- Windows releases
- cargo-dist installer types other than homebrew (shell scripts, msi, etc.)
- Submitting to homebrew/core
