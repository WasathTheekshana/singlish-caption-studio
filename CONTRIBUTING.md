# Contributing

Thanks for wanting to help. This is a small project, so there is no heavy process. Open an issue or a pull request and we will sort it out.

## Reporting a bug

Please include:

- your Singlish Caption Studio version (Settings, then About) and your CapCut version;
- what you did, what you expected, and what happened;
- the message the app showed, word for word.

Please do not attach project files, videos or your API key. If a project is needed to reproduce the problem, describe its shape instead (how many clips, which tracks, any speed changes).

## Ideas

Open an issue before starting anything large, so we can agree on the direction first. Things on my own list: more CapCut versions, voice-over tracks, caption style presets for TikTok, Reels and Shorts, and word by word highlighting.

## Setting up

You need Windows, Rust 1.82 or newer, and the Visual Studio Build Tools with the C++ workload. Fork the repository first, then clone your fork:

```
git clone https://github.com/YOUR-USERNAME/singlish-caption-studio.git
cd singlish-caption-studio
cargo test
cargo install tauri-cli --version "^2" --locked
cargo tauri build --bundles nsis
```

The interface is plain HTML, CSS and JavaScript in `ui/`. There is no bundler and no npm. Keep it that way: it keeps the supply chain small and the content security policy strict.

## Before you open a pull request

Run these and make sure they pass:

```
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

Some tests decode a real video. Set `SCS_TEST_VIDEO` to a short clip with speech to run them, otherwise they skip.

Also:

- Add a test for new behavior, or for the bug you fixed.
- Keep functions small and name things plainly. Comments should explain why, not repeat the code.
- No `unwrap()` outside tests. Clippy will tell you.
- Do not add network calls. The only place that talks to the internet is `src/eleven.rs`, and it only talks to ElevenLabs.
- If you add a command the interface can call, list it in `build.rs` and in `capabilities/main.json`, and validate every argument in Rust.
- Text shown to people should be short and say what to do next. Avoid jargon.

## Commit messages and pull request titles

This project uses [Conventional Commits](https://www.conventionalcommits.org/). The messages are not just history: they decide the next version number and write the release notes, so nobody edits the version or the changelog by hand. [docs/releasing.md](docs/releasing.md) explains the whole flow.

The form is `type(scope): short summary`.

| Type | Use it for | Next release |
| --- | --- | --- |
| `feat` | A new feature | minor |
| `fix` | A bug fix | patch |
| `perf` | A speed or memory improvement | patch |
| `docs`, `test`, `refactor`, `style`, `build`, `ci`, `chore` | Everything that users do not notice | none by itself |
| `revert` | Undoing an earlier commit | patch |

Put a `!` after the type for a breaking change (`feat!: change the settings file format`), or add a `BREAKING CHANGE:` line in the body. That is a major release, or a minor one while the version is below 1.0.

A few examples:

```
feat(ui): add a usage tab
fix: keep captions from overlapping
fix(audio): handle files without sound
build(deps): bump sha2 from 0.10.9 to 0.11.1
docs: explain how releases work
```

Keep the first line to 72 characters and leave off the final period. The body is for what changed and why.

**Pull requests are squash merged, so the pull request title is the commit message.** A check on every pull request makes sure the title follows the form. You can edit the title at any time and the check runs again.

To get the same check on your own commits, turn on the hook and the template once per clone:

```
git config core.hooksPath tools/hooks
git config commit.template .gitmessage
```

## Security

Do not report vulnerabilities in public issues. See [SECURITY.md](SECURITY.md).

## License

By contributing you agree that your work is released under the [MIT License](LICENSE).
