# builddiag — Checks

This document describes builddiag's check surface at the level reviewers care about:
- What each check validates
- What it means when it fails
- How to fix it
- How it behaves under profiles

**Canonical details (codes, exact wording, links) live in the explain registry.**
Use:
- `builddiag explain <check_id>`
- `builddiag explain <code>`
- `builddiag list-checks` — show all checks with profile severities

## Check Overview (15 total)

| Module | Check ID | Description |
|--------|----------|-------------|
| rust | `rust.msrv_defined` | MSRV is explicitly defined |
| rust | `rust.msrv_consistent` | All members have consistent MSRV |
| rust | `rust.toolchain_pinning` | Toolchain pinned to specific version |
| rust | `rust.toolchain_msrv_relation` | Toolchain matches MSRV policy |
| workspace | `workspace.resolver_v2` | Workspace uses resolver v2 |
| workspace | `workspace.edition_consistent` | Consistent edition across members |
| workspace | `workspace.member_ordering` | Members sorted alphabetically |
| deps | `deps.wildcard_version` | No wildcard version specs |
| deps | `deps.path_missing_version` | Path deps have versions |
| deps | `deps.workspace_inheritance` | Suggests workspace deps |
| deps | `deps.lockfile_present` | Cargo.lock for binaries |
| tools | `tools.checksums_file_exists` | Checksums file exists |
| tools | `tools.checksums_format` | Checksums are well-formed |
| tools | `tools.checksums_coverage` | All tools have checksums |
| tools | `tools.checksums_verify_local` | Local files match checksums |

## Profiles: Default Posture

- `oss`: safe for strangers; does not fail because your repo lacks your conventions
- `team`: practical gating
- `strict`: CI/release discipline

> Rule of thumb: "missing convention file" is skip in `oss`, but "present and malformed" is always a real signal.

---

## rust.msrv_defined

**What it checks**
- The repo declares an MSRV via `package.rust-version` or `workspace.package.rust-version`.

**Codes:** `missing_msrv`, `invalid_msrv_defined`

**Why it matters**
- Without MSRV, reproducibility and dependency upgrades become guesswork; contributors can't know what "supported" means.

**How to fix**
- Add `rust-version = "1.xx"` either:
  - `[workspace.package] rust-version = "…"` (preferred for workspaces), or
  - `[package] rust-version = "…"`

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | warn     |
| team    | warn     |
| strict  | error    |

---

## rust.msrv_consistent

**What it checks**
- All publishable crates in the workspace are consistent with the declared MSRV source-of-truth.
- Flags drift (missing MSRV in a member when the workspace declares one, or mismatch values).

**Codes:** `invalid_msrv`, `missing_member_msrv`, `invalid_member_msrv`, `msrv_mismatch`

**Why it matters**
- MSRV drift is a silent footgun: CI "works" until a contributor uses a different toolchain or you publish.

**How to fix**
- Prefer one source of truth:
  - `[workspace.package] rust-version = "…"`
- Remove per-crate overrides unless they are intentional and documented.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | error    |
| team    | error    |
| strict  | error    |

---

## rust.toolchain_pinning

**What it checks**
- Whether the toolchain file pins to a specific toolchain version (vs `stable`, `nightly`, etc.) when a toolchain file exists.

**Codes:** `missing_toolchain`, `nightly_disallowed`, `unpinned_channel`, `invalid_toolchain_version`

**Why it matters**
- Pinning is the simplest hedge against "Rust updated and CI changed".

**How to fix**
- In `rust-toolchain.toml`, set:
  - `channel = "1.xx.y"` (or similar pinned version)
- If you intentionally want `stable`, treat that as a repo decision and encode it via profile/config.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | info     |
| team    | warn     |
| strict  | error    |

---

## rust.toolchain_msrv_relation

**What it checks**
- Ensures the pinned toolchain and declared MSRV are not contradictory (e.g., pinned toolchain older than MSRV, or missing one side).

**Codes:** `toolchain_msrv_mismatch`

**Why it matters**
- A declared MSRV that can't be built with the pinned toolchain is self-contradictory.

**How to fix**
- Make MSRV and toolchain consistent:
  - Pin toolchain >= MSRV
  - Or adjust MSRV to match reality (but do that intentionally)
- Configure `policy.toolchain.relation_to_msrv = "at_least"` to allow newer toolchains.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | warn     |
| team    | error    |
| strict  | error    |

---

## workspace.resolver_v2

**What it checks**
- Workspace resolver is set to v2 (`resolver = "2"`), especially for edition 2021+ workspaces.

**Codes:** `resolver_not_v2`

**Why it matters**
- Resolver v2 prevents a class of feature-unification surprises.

**How to fix**
Add to root `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = [ ... ]
```

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | info     |
| team    | warn     |
| strict  | error    |

---

## workspace.edition_consistent

**What it checks**
- All workspace members use the same Rust edition.

**Codes:** `invalid_workspace_edition`, `missing_member_edition`, `invalid_member_edition`, `edition_mismatch`

**Why it matters**
- Inconsistent editions across crates can cause confusing behavior differences.

**How to fix**
- Ensure all crates either inherit from `workspace.package.edition` or explicitly set the same edition.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | warn     |
| team    | error    |
| strict  | error    |

---

## workspace.member_ordering

**What it checks**
- Workspace members in `[workspace.members]` are sorted alphabetically.

**Codes:** `members_not_sorted`

**Why it matters**
- Sorted members improve readability and reduce merge conflicts.

**How to fix**
- Sort the members array alphabetically in Cargo.toml.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | info     |
| team    | info     |
| strict  | error    |

---

## deps.wildcard_version

**What it checks**
- Dependencies do not use wildcard version specifications (`"*"`).

**Codes:** `wildcard_version`

**Why it matters**
- Wildcard versions are fragile and can cause unexpected breakage.

**How to fix**
- Replace `foo = "*"` with a specific version like `foo = "1.0"`.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | info     |
| team    | warn     |
| strict  | error    |

---

## deps.path_missing_version

**What it checks**
- Path dependencies also specify a version.

**Codes:** `path_missing_version`

**Why it matters**
- Path-only dependencies cannot be published to crates.io.

**How to fix**
- Add a version field: `foo = { path = "../foo", version = "0.1" }`.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | info     |
| team    | warn     |
| strict  | error    |

---

## deps.workspace_inheritance

**What it checks**
- Suggests using workspace dependency inheritance when a dependency is defined in `workspace.dependencies`.

**Codes:** `missing_workspace_inheritance`

**Why it matters**
- Workspace inheritance reduces duplication and ensures consistent versions.

**How to fix**
- Use `foo.workspace = true` instead of duplicating the version.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | info     |
| team    | warn     |
| strict  | error    |

> **Note:** This is a suggestion check; it identifies opportunities for improvement rather than errors.

---

## deps.lockfile_present

**What it checks**
- `Cargo.lock` exists for binary crates.

**Codes:** `missing_lockfile_for_binary`, `unexpected_lockfile_for_library`

**Why it matters**
- A lockfile ensures reproducible builds for applications.

**How to fix**
- Run `cargo build` to generate `Cargo.lock` and commit it to version control.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | info     |
| team    | warn     |
| strict  | error    |

---

## tools.checksums_file_exists

**What it checks**
- A checksums file exists (e.g., `scripts/tools.sha256`).

**Codes:** `missing_checksums`

**Why it matters**
- Checksums files contain SHA256 hashes for tool binaries to verify integrity.

**How to fix**
- Create `scripts/tools.sha256` with checksums in the format: `<sha256hash>  <filepath>`

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | **skip** |
| team    | warn     |
| strict  | error    |

> **Note:** All `tools.*` checks are skipped in `oss` profile since they represent optional conventions.

---

## tools.checksums_format

**What it checks**
- Checksum entries are parseable and well-formed (64-character hex SHA256 hashes).

**Codes:** `invalid_hash`, `missing_path`, `duplicate_path`

**Why it matters**
- Malformed inputs are a risk if the file exists.

**How to fix**
- Ensure each line follows the format: `<64-char-sha256>  <filepath>`
- Generate with: `sha256sum <file>`

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | skip     |
| team    | warn     |
| strict  | error    |

---

## tools.checksums_coverage

**What it checks**
- The checksums file covers the expected tool set (as defined by repo policy).

**Codes:** `missing_checksum`, `unexpected_checksum`

**Why it matters**
- Ensures the declared manifest is coherent.

**How to fix**
- Add missing checksums for all files listed in `scripts/tools.toml`.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | skip     |
| team    | warn     |
| strict  | error    |

---

## tools.checksums_verify_local

**What it checks**
- Local tool files match their recorded checksums.

**Codes:** `missing_tool_file`, `hash_mismatch`

**Why it matters**
- Detects tampering or corruption of tool binaries.

**How to fix**
- Re-download or regenerate tools with mismatched checksums, then update `scripts/tools.sha256`.

**Profile defaults**
| Profile | Severity |
|---------|----------|
| oss     | skip     |
| team    | warn     |
| strict  | error    |

> **Note:** Local binary verification is **machine truth** and may belong in env-check. builddiag should not hash local binaries by default.
