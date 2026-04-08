---
mode: agent
description: Bump the version in Cargo.toml, commit, tag, and push to trigger a new GitHub release.
---

# Generate New GitHub Release

You are automating a release for the TaskPilot project. Follow these steps exactly:

## Step 1 — Determine Current Version

Read the `version` field from `Cargo.toml` in the repo root. Parse it as `MAJOR.MINOR.PATCH` (semver).

## Step 2 — Ask for Bump Type

Ask the user which version bump to apply:
- **patch** (default) — e.g. `0.1.0` → `0.1.1`
- **minor** — e.g. `0.1.0` → `0.2.0`
- **major** — e.g. `0.1.0` → `1.0.0`

Default to **patch** if the user doesn't specify.

## Step 3 — Bump the Version

Update the `version = "..."` line in `Cargo.toml` to the new version.

## Step 4 — Update Cargo.lock

Run `cargo check` to regenerate `Cargo.lock` with the new version.

## Step 5 — Commit

Stage `Cargo.toml` and `Cargo.lock`, then create a commit:

```
git add Cargo.toml Cargo.lock
git commit -m "release: v<NEW_VERSION>"
```

## Step 6 — Tag

Create an annotated git tag:

```
git tag -a "v<NEW_VERSION>" -m "Release v<NEW_VERSION>"
```

## Step 7 — Push

Push the commit and the tag to `origin`:

```
git push origin HEAD
git push origin "v<NEW_VERSION>"
```

This triggers the `release.yml` GitHub Actions workflow, which builds release binaries and creates a GitHub Release automatically.

## Step 8 — Confirm

Tell the user:
- The new version number
- That the tag has been pushed
- That the release workflow should now be running (link: `https://github.com/rmoreirao/taskpilot/actions/workflows/release.yml`)
