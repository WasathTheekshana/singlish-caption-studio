# How releases work

Releases are automatic and follow the commit messages. Nobody edits a version number or the changelog by hand.

## The short version

1. Open a pull request with a title like `fix: keep captions from overlapping`.
2. Squash merge it, so the title becomes the commit message.
3. A bot keeps one pull request open called `chore(main): release singlish-caption-studio x.y.z`. It holds the next version number and the changelog, rebuilt every time something lands on `main`.
4. When you want to ship, merge that pull request. The tag and the GitHub release appear, and the installer is built and attached, along with `SHA256SUMS.txt` and a build attestation. Tags include the package name, like `singlish-caption-studio-v0.1.0`.

The tool behind this is [release-please](https://github.com/googleapis/release-please). Its settings are in `release-please-config.json`, and the workflow is `.github/workflows/release.yml`.

## What decides the version

| Commit type | Next release | Example while below 1.0 | Example after 1.0 |
| --- | --- | --- | --- |
| `fix`, `perf`, `revert` | patch | 0.1.0 to 0.1.1 | 1.0.0 to 1.0.1 |
| `feat` | minor | 0.1.0 to 0.2.0 | 1.0.0 to 1.1.0 |
| `!` after the type, or a `BREAKING CHANGE:` line | major | 0.1.0 to 0.2.0 | 1.0.0 to 2.0.0 |
| `docs`, `test`, `refactor`, `style`, `build`, `ci`, `chore` | none by itself | | |

While the version starts with 0, a breaking change bumps the minor number instead of the major one. That is the usual rule for software that has not reached 1.0.

If several commits land before a release, the biggest one wins. Types that do not release by themselves are still included in the next release when one happens, they just do not appear in the changelog.

A dependency update that fixes a security problem should be titled `fix(deps): ...` instead of the usual `build(deps): ...`, so it triggers a release.

## The first release

Nothing has been released yet, so the first release PR appears after the first commit that counts, a `feat` or a `fix`. The settings start the very first release at 0.1.0 (the `initial-version` option in `release-please-config.json`).

To start it on purpose, push an empty commit:

```
git commit --allow-empty -m "feat: first public release"
git push
```

Then wait for the pull request called `chore(main): release singlish-caption-studio 0.1.0`. Look at the version in its title before you merge it. If it is not 0.1.0, close it and use the `Release-As` footer described below.

The notes for the first release will be short. The commits from before conventional commits were adopted do not follow the format, so they are ignored, and the history is not rewritten.

## Choosing a version yourself

Add a `Release-As` footer to any commit. This is how you go to 1.0.0:

```
git commit --allow-empty -m "chore: release 1.0.0" -m "Release-As: 1.0.0"
```

## One-time repository settings

The automation needs these, and only the repository owner can change them.

1. **Settings, Actions, General, Workflow permissions.** Tick "Allow GitHub Actions to create and approve pull requests". Without it the release pull request cannot be opened.
2. **Settings, General, Pull Requests.** Allow squash merging, and set its default message to "Pull request title". Turn off the other merge types if you want the history to stay clean. A merge commit would put the title in a message nobody checks.
3. Optional: protect `main` and require the checks `Format, lint, test`, `Dependency audit` and `Conventional commit title`.

## Good to know

- **No checks run on the release pull request.** GitHub does not start workflows for pull requests opened with the built-in token. The release PR only changes version files and the changelog, and the build job runs the whole test suite before anything is published. If you protect `main` with required checks, you will need to merge the release PR as an administrator, or switch the workflow to a personal access token.
- **The build starts from a clean slate.** Release builds use no caches, run the tests, and build from the tag. Every action used is pinned to a commit.
- **If the build fails after the tag exists,** the GitHub release is there without an installer. If the failure was a hiccup (a network error, for example), use "Re-run failed jobs" on the run. If it needed a change to the workflow or the code, push the fix, then open Actions, Release, "Run workflow", and type the tag (like `singlish-caption-studio-v0.1.0`). That builds the tagged code with the current workflow and attaches the installer to the existing release. A plain re-run would not work for that, because it uses the workflow file from the original run.
- **To try a build without releasing,** use "Run workflow" on the Release workflow and leave the tag empty. It builds the installer and keeps it as a downloadable artifact on the run.
- **Commits from before this system** (the first 17) do not follow the format. They are ignored when the notes are written, and the history is left as it is.
- **The version lives in two files**, `Cargo.toml` and `tauri.conf.json`, and release-please updates both. The build checks that they agree with the tag and stops if they do not.
