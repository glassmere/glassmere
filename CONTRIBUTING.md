# Contributing

## Branching model

Glassmere uses a release-based flow. There is no `develop` branch: `main` is the single source of truth and holds released code only.

```
main ──●──────────────────●────────────────●──►
       │                  ▲                ▲
       ├─ release/v0.2.0 ─┤                │
       │   ├─ feature/path-styles  → PR    │
       │   └─ fix/grid-gap         → PR    │
       │                                   │
       └─ hotfix/yanked-dependency ── PR ──┘   (then merged into every open release/*)
```

| Branch | Created from | Merges into | Use |
|--------|--------------|-------------|-----|
| `main` | | | Released code. Protected, no direct pushes. Tagged on every release. |
| `release/vX.Y.Z` | `main` | `main` | One release cycle. Feature and fix branches for that version target it. |
| `feature/<description>` | `release/*` | the same `release/*` | New functionality. |
| `fix/<description>` | `release/*` | the same `release/*` | A bug found while the release is in development. |
| `hotfix/<description>` | `main` | `main` | A bug in a published version that cannot wait for the next release. |

Branch names are `<type>/<kebab-case-description>`, with the description at most about 40 characters, for example `feature/windows-menu-bar` or `fix/caption-first-hover`.

Breaking changes go only into a release branch for a new minor version (`release/v0.2.0`). A patch release branch (`release/v0.1.3`) takes additive, compatible changes only; `crates/glassmere/tests/api.rs` may show additions only.

### Propagation

Everything merged into `main` (a release or a hotfix) is merged into every open `release/*` branch. Everything merged into an older release branch is merged forward into the newer ones:

```
main ──► every open release/*
release/v0.1.3 ──► release/v0.2.0
```

Propagation goes through a pull request too, from a branch named `merge/<source>-into-<target>` (for example `merge/main-into-release-v0.2.0`) that carries any conflict resolution.

## Tags

| Tag | Points at | Set |
|-----|-----------|-----|
| `vX.Y.Z` | the release commit on `main` | when the release is merged into `main` |
| `latest` | the version published on crates.io | after `cargo publish` succeeds |
| `rollback` | the previously published version | just before `latest` moves |

```bash
git tag v0.2.0
git tag -f rollback "$(git rev-list -n 1 latest)"
git tag -f latest
git push origin v0.2.0
git push origin -f latest rollback
```

## Pull requests

All changes go through pull requests, including propagation merges.

- `feature/*` and `fix/*` target their `release/*` branch; `hotfix/*` and a finished `release/*` target `main`.
- Title: a short description in the imperative, for example `Add a native Windows menu bar`.
- CI (`.github/workflows/ci.yml`) must pass, and no review comment may be left unresolved.
- Squash-merge feature and fix branches. Merge a release into `main`, and `main` into release branches, with a merge commit so history is kept.
- Every feature updates or adds an example that shows it.

## Commit messages

```
<type>: <Subject>

<optional body explaining why>
```

| Type | Use for |
|------|---------|
| `feat` | New feature |
| `fix` | Bug fix |
| `refactor` | Restructuring without a behavior change |
| `test` | Tests |
| `docs` | Documentation |
| `style` | Formatting only |
| `chore` | Build, CI, dependency updates |
| `perf` | Performance |

The subject is at most 72 characters, capitalized, in the imperative mood and without a trailing period.

## Workflows

Start a release:

```bash
git switch main && git pull
git switch -c release/v0.3.0
git push -u origin release/v0.3.0
```

Then point `.github/dependabot.yml` at the open release branches through a hotfix to `main` (Dependabot reads its configuration from the default branch only).

Work on a feature:

```bash
git switch release/v0.3.0 && git pull
git switch -c feature/offline-cache
git push -u origin feature/offline-cache
# open a PR into release/v0.3.0
```

Apply a hotfix:

```bash
git switch main && git pull
git switch -c hotfix/clipboard-crash
git push -u origin hotfix/clipboard-crash
# open a PR into main, then a merge/main-into-release-v0.3.0 PR for each open release
```

Finish a release: open a PR from `release/vX.Y.Z` into `main`, merge it with a merge commit, tag it, publish, move `latest` and `rollback`, and propagate `main` into the remaining release branches.
