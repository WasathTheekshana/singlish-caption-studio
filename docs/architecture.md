# How the code is organized

The app is a [Tauri 2](https://tauri.app) program. The window shows plain HTML, CSS and JavaScript from `ui/`, and everything else is Rust in `src/`.

I tried a pure Rust interface first, but Sinhala needs proper text shaping to look right (vowel signs move around), and a web view does that out of the box. That is the only reason there is a web view.

## The flow of one run

```
pick project -> list clips -> tick clips -> Generate
   |
   v
read the project file          draft.rs
   |
   v
plan requests                  pipeline.rs   (merge back-to-back cuts, group nearby cuts of one file)
   |
   v
for each request:
   cache hit?  ------------->  pipeline.rs   (cache lives in %LOCALAPPDATA%)
   no: cut the audio range     audio.rs      (decode, mix to mono, resample to 16 kHz)
       ask ElevenLabs          eleven.rs
   |
   v
map words onto the timeline    draft.rs      (trim, speed, clip offsets)
restore English words          text.rs       (glossary)
group into caption lines       text.rs
trim overlaps                  draft.rs
   |
   v
write both copies of the draft draft.rs      (validate in memory, back up, write atomically)
record usage                   usage.rs
```

## Modules

| File | What it does |
| --- | --- |
| `src/main.rs` | The commands the interface can call. Validates every argument. |
| `src/pipeline.rs` | The run described above. Takes the transcriber as an argument, so tests run the whole flow without a network. |
| `src/draft.rs` | Reads and edits CapCut's `draft_content.json`: finds clips, builds caption segments, backups and restore. |
| `src/text.rs` | Splits a caption into Sinhala and English runs, applies the glossary, groups words into lines. |
| `src/audio.rs` | Pure Rust audio extraction (symphonia) with its own windowed-sinc resampler. No ffmpeg. |
| `src/eleven.rs` | The only network code. One address, timeouts, size limits. |
| `src/secrets.rs` | The API key in Windows Credential Manager. |
| `src/store.rs` | Settings, glossary and cache locations. Atomic writes and size-limited reads. |
| `src/srt.rs` | Writes the captions as a `.srt` file in Documents, with a safe file name built from the project title. |
| `src/usage.rs` | The local usage log. |
| `src/updater.rs` | Checks GitHub for a newer release and installs it. A thin, tested wrapper around the Tauri updater plugin, which the page cannot call directly. |
| `src/fonts.rs` | Installed fonts, read from the registry. |
| `src/validate.rs` | Input checks for project names and style settings. |

## Things that are easy to get wrong

- **CapCut stores two copies of a draft.** One is `draft_content.json` in the project folder, the other is under `Timelines/<id>/`. Both must be written.
- **Style ranges count characters.** CapCut stores the font for each stretch of a caption as a character range, so Sinhala and English runs must add up to exactly the caption length. `draft.rs` checks that before writing.
- **A track cannot hold overlapping segments.** Captions from overlapping clips are trimmed so they never overlap.
- **Source time is not timeline time.** A clip can start part way into its file and play at a different speed. `map_to_timeline` handles both.
- **Media paths in a project file are untrusted.** See `media_path_ok` in `draft.rs`.

## Tests

`cargo test` runs everything. Most tests are plain unit tests. A few more are worth knowing about:

- `tests/fixtures/golden.json` holds expected results for the text and draft code, taken from an early prototype of this tool. If you change how captions are built on purpose, update it deliberately.
- The end to end tests in `pipeline.rs` build a small fake CapCut project in a temp folder and run the whole flow with a fake transcriber.
- Tests that need a real video read its path from `SCS_TEST_VIDEO` and skip when it is not set.

## Interface rules

- Everything the backend sends is inserted with `textContent`, never as HTML.
- The content security policy in `tauri.conf.json` allows only local files and the app's own IPC channel.
- The page never sends a file path. It sends a CapCut folder name or a clip id, and the backend checks it against what CapCut itself lists.
