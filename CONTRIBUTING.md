# Contributing

## Workflow

`main` is protected: all changes land through pull requests that pass CI.
PRs are **squash-merged**, and the PR title becomes the commit message on
`main`. Merged branches are deleted automatically.

## Conventional Commits

PR titles (and therefore commits on `main`) follow
[Conventional Commits](https://www.conventionalcommits.org/). The
`Conventions` workflow enforces this on PR titles and on every commit in
a PR, using the rules in `committed.toml`. jj has no commit hooks, so run
`just lint-commits` to check your commits locally before pushing.

```
<type>(optional scope): <description>
```

| Type | Use for | Release effect |
|------|---------|----------------|
| `feat` | New gameplay or features | Minor bump (patch while < 1.0) |
| `fix` | Bug fixes | Patch bump |
| `perf` | Performance improvements | Patch bump |
| `docs`, `refactor`, `test`, `build`, `ci`, `chore`, `style` | Everything else | None |

Add `!` after the type (e.g. `feat!: ...`) or a `BREAKING CHANGE:` footer for
breaking changes, which bump the minor version while below 1.0.

## Releases

[release-please](https://github.com/googleapis/release-please) keeps a release
PR open on `main` that collects unreleased changes, bumps the version in
`Cargo.toml` and updates `CHANGELOG.md`. Edit that PR to curate the release
notes, then merge it to publish. Merging tags the release, and the `Release`
workflow builds Linux, Windows and macOS binaries and attaches them to it.

To check that every platform builds without releasing, run the `Release`
workflow manually from the Actions tab; the builds are saved as workflow
artifacts.

## Local checks

Run `just ci` in the dev shell before opening a PR; it runs the same blocking
checks as CI.

## Coverage reports

Every CI run shows a coverage summary on its run page and uploads the full
HTML report as the `coverage` artifact. To also publish the report from `main`
to GitHub Pages, enable Pages with "GitHub Actions" as the source and set the
repository variable `PUBLISH_COVERAGE` to `true`.
