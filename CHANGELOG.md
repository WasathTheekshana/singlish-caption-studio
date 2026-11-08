# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-11-08

First public release.

### Added

- Captions for CapCut projects, written as a normal text track named "Auto captions (SI+EN)".
- Separate font and size for Sinhala and English, plus color and height on screen.
- Support for projects with many clips, trims and speed changes. Pick which clips to caption.
- Only the audio that is on the timeline is transcribed. Nearby cuts of one file share a request.
- A glossary that turns English words written in Sinhala letters back into English, and fixes Sinhala spellings.
- A usage log: seconds of audio, requests and words, per project and per clip, with an optional cost estimate from your own price per minute.
- First-run setup for CapCut, the ElevenLabs key and fonts.
- The API key is stored in Windows Credential Manager.
- Backups before every change, and a button to restore the latest one.
- Per-user installer that needs no administrator rights.
- A window that cannot be maximized, and a layout that uses extra height when you make it taller.

### Security

- Project media paths must be plain local paths to known media types. Network paths and Windows device names are refused without being opened.
- Project files above a size limit, and projects with an absurd number of clips, are refused.
- Strict content security policy, a fixed list of backend commands, and the window can only show the app's own page.
- Redirects are off for the one network call, so the API key header cannot be forwarded.
- A damaged media file is reported as an error and does not crash the app.
- Restore and caption runs share one lock. A failed write rolls back, a damaged backup is refused, and old backups are pruned (the first and the newest ten are kept).
- Release builds run without caches, and every GitHub Action is pinned to a commit.
