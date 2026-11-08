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
- Update `CHANGELOG.md` for anything a user would notice.

## Security

Do not report vulnerabilities in public issues. See [SECURITY.md](SECURITY.md).

## License

By contributing you agree that your work is released under the [MIT License](LICENSE).
