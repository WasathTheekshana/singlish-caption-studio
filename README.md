<p align="center">
  <img src="icons/icon.png" alt="Singlish Caption Studio logo" width="112">
</p>

<h1 align="center">Singlish Caption Studio</h1>

<p align="center">
  Captions for videos where Sinhala and English are mixed in the same sentence,<br>
  written straight into your CapCut project.
</p>

<p align="center">
  <a href="https://github.com/WasathTheekshana/singlish-caption-studio/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/WasathTheekshana/singlish-caption-studio/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/WasathTheekshana/singlish-caption-studio/actions/workflows/release.yml"><img alt="Release build" src="https://github.com/WasathTheekshana/singlish-caption-studio/actions/workflows/release.yml/badge.svg"></a>
  <a href="https://github.com/WasathTheekshana/singlish-caption-studio/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/WasathTheekshana/singlish-caption-studio?display_name=tag&sort=semver"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/github/license/WasathTheekshana/singlish-caption-studio"></a>
</p>

<p align="center">
  <a href="https://github.com/WasathTheekshana/singlish-caption-studio/stargazers"><img alt="GitHub stars" src="https://img.shields.io/github/stars/WasathTheekshana/singlish-caption-studio?logo=github"></a>
  <a href="https://github.com/WasathTheekshana/singlish-caption-studio/forks"><img alt="GitHub forks" src="https://img.shields.io/github/forks/WasathTheekshana/singlish-caption-studio?logo=github"></a>
  <a href="https://github.com/WasathTheekshana/singlish-caption-studio/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/WasathTheekshana/singlish-caption-studio/total"></a>
  <a href="https://github.com/WasathTheekshana/singlish-caption-studio/issues"><img alt="Open issues" src="https://img.shields.io/github/issues/WasathTheekshana/singlish-caption-studio"></a>
  <img alt="Platform: Windows" src="https://img.shields.io/badge/platform-Windows-0078D6">
  <img alt="Built with Rust" src="https://img.shields.io/badge/built%20with-Rust-b7410e">
  <a href="https://buymeacoffee.com/wasath"><img alt="Buy me a coffee" src="https://img.shields.io/badge/buy%20me%20a%20coffee-support-FFDD00?logo=buymeacoffee&logoColor=black"></a>
</p>

If you speak Sinhala and drop English words in as you go ("මේ video එක බලලා යන්න"), most caption tools stumble. They either don't know Sinhala, or they spell the English words in Sinhala letters. This app listens to your clips, puts the English words back in English, and gives each language its own font and size, so the captions look the way you would have made them by hand.

It is a small Windows app. You pick a CapCut project, press one button, and the captions appear as a normal text track you can edit in CapCut.

## What it does

- Transcribes only the parts of your clips that are actually on the timeline.
- Works with projects that have many clips, trims and speed changes.
- Turns English words that came back in Sinhala letters into English, using a list you can edit.
- Applies your own Sinhala font and English font, sizes, color and position.
- Shows how much audio each run used, per project and per clip.
- Backs up the project file before every change, and can restore it.

## Download

Get the installer from the [Releases](https://github.com/WasathTheekshana/singlish-caption-studio/releases/latest) page: `Singlish Caption Studio_x.y.z_x64-setup.exe`.

It installs for your user only, so it does not ask for administrator rights.

The installer is not code signed yet, so Windows SmartScreen may warn you the first time. Each release also has a `SHA256SUMS.txt` file so you can check the download, and the build is attested by GitHub. To check it:

```
gh attestation verify "Singlish Caption Studio_0.1.0_x64-setup.exe" --repo WasathTheekshana/singlish-caption-studio
```

## First run

A short setup asks for three things.

1. **CapCut.** The app looks for CapCut on your PC and tells you which version it found.
2. **ElevenLabs key.** Transcription is done by [ElevenLabs](https://elevenlabs.io), so you need your own account and API key. Create a key, allow only the Speech to Text permission, and set a credit limit so it cannot do anything else. Paste it into the app. It is checked for free and then stored in Windows Credential Manager.
3. **Fonts.** Choose a Sinhala font and an English font.

## Using it

1. Make your edit in CapCut, then close CapCut completely (check the tray icon).
2. Open Singlish Caption Studio and pick the project.
3. Tick the clips that contain speech. Quiet background clips are left unticked.
4. Tick the box that allows the audio to be sent to ElevenLabs, then press **Generate captions**.
5. Open the project in CapCut. The captions are on a track called "Auto captions (SI+EN)".

Running it again replaces that track, so you can tweak the glossary or the style and try again.

### The glossary

ElevenLabs often writes English words in Sinhala letters, for example `වීඩියෝ` for "video". The Words tab in Settings lets you map those spellings to the English you want, and fix Sinhala spellings you prefer. Add entries as you notice them.

## Usage and cost

ElevenLabs charges for speech to text by the amount of audio, not by tokens. The Usage tab shows what the app has sent: seconds of audio, requests and words, for each project and each clip. Answers that come from the local cache are free and are marked as such. If you enter your price per minute, it also shows a rough estimate. Check your ElevenLabs plan for the real rate.

## Limits

- Windows only. Tested with CapCut 9.6. Other versions may store projects differently, and the app warns you when it finds one it has not been tested with.
- Only video clips are captioned. Separate voice-over or music tracks are not.
- Reversed clips are skipped.
- Where two ticked clips play at the same time, the earlier caption is cut short so captions never overlap.
- Font files that hold several fonts (`.ttc`) may not be applied by CapCut. The app marks them.
- Accuracy depends on ElevenLabs. Read through the captions before you publish.
- Your project folder is edited in place (with a backup). CapCut's file format is not documented, so use this at your own risk.

## Build from source

You need Windows, Rust 1.82 or newer, and the Visual Studio Build Tools with the C++ workload.

```
cargo install tauri-cli --version "^2" --locked
cargo test
cargo tauri build --bundles nsis
```

The installer ends up in `target/release/bundle/nsis/`.

A few tests decode a real video. Point `SCS_TEST_VIDEO` at any short clip with speech to run them. Without it they skip.

```
set SCS_TEST_VIDEO=C:\path\to\clip.mov
cargo test
```

There is a short tour of the code in [docs/architecture.md](docs/architecture.md).

## Privacy

The audio of the clips you choose goes to ElevenLabs. Nothing else leaves your computer, and the app has no analytics. Read [PRIVACY.md](PRIVACY.md) for the details.

## Contributing

Bug reports, ideas and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md). Security problems should be reported privately, see [SECURITY.md](SECURITY.md).

## Contributors

Thank you to everyone who has helped build this.

<a href="https://github.com/WasathTheekshana/singlish-caption-studio/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=WasathTheekshana/singlish-caption-studio" alt="Contributors">
</a>

## Star history

<a href="https://star-history.com/#WasathTheekshana/singlish-caption-studio&Date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=WasathTheekshana/singlish-caption-studio&type=Date&theme=dark">
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=WasathTheekshana/singlish-caption-studio&type=Date">
    <img alt="Star history chart" src="https://api.star-history.com/svg?repos=WasathTheekshana/singlish-caption-studio&type=Date">
  </picture>
</a>

If the app saves you time, a star on the repository helps other people find it.

## License

[MIT](LICENSE).

This is an independent project. It is not affiliated with or endorsed by CapCut, ByteDance or ElevenLabs. Those names belong to their owners.

## About the developer

I'm Wasath. I make videos in Sinhala with a lot of English mixed in, and fixing captions by hand was eating my evenings. I built this for myself first and I'm sharing it in case it saves you the same time.

If it helps you and you want to say thanks, you can [buy me a coffee](https://buymeacoffee.com/wasath).
