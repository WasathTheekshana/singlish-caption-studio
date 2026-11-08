# Security policy

## Reporting a problem

Please report security problems privately, not in a public issue.

Use GitHub's private reporting: open the [Security tab](https://github.com/WasathTheekshana/singlish-caption-studio/security) of this repository and choose **Report a vulnerability**, or go straight to the [report form](https://github.com/WasathTheekshana/singlish-caption-studio/security/advisories/new). Include what you found, how to reproduce it, and which version you tested. I will answer as soon as I can, usually within a few days, and I will credit you in the release notes if you want that.

If you cannot use private reporting, open a public issue that says only that you have a security report, with no details, and I will arrange a private channel.

## Supported versions

Only the latest release gets security fixes.

## What the app is careful about

- **Your API key** is stored in Windows Credential Manager and is only ever read to be placed in a request header to `api.elevenlabs.io`. It is not written to disk, logged, or sent to the user interface.
- **The window** loads only local files. Its content security policy blocks every outside address, the window refuses to navigate anywhere except the app's own page, and release builds have no developer tools.
- **The interface never inserts text as HTML.** Everything that comes from a project file or from ElevenLabs is shown as plain text.
- **Commands** from the interface to the backend are limited to a fixed list, and each one checks its input. The interface never supplies a file path. Projects are picked by their CapCut folder name from CapCut's own list, and clips by their id.
- **Project files are treated as untrusted.** Media paths found inside a project must be plain local paths to known media types. Network paths such as `\\server\share` are refused without being opened, because opening one would leak the user's Windows login hash. Windows device names such as `CON.mov` are refused. Files above a size limit, and projects with an absurd number of clips, are refused.
- **A damaged media file cannot crash the app.** Decoding runs behind a guard that turns a decoder failure into an ordinary error message.
- **Network use** is one fixed HTTPS address. Redirects are turned off, so the API key header can never be forwarded to another address. Calls have timeouts, and replies have size limits. A reply that is not exactly what was expected is treated as a failure, including when checking a key.
- **Edits to your project** are checked in memory first, backed up, and written atomically. If writing the second copy of a draft fails, the first is put back. Only one job can touch a project at a time, and a restore refuses a backup that is not valid JSON.
- **Updates are verified before they are installed.** The app contains the public half of a signing key. An update is installed only if its signature matches that key, and only if the signature names the exact version being offered, so a tampered update list cannot hand out an older build. The update list and the download must both use HTTPS. The installer is never run without you pressing the button, and never while a caption run is in progress. The page cannot call the updater directly, only the app's own fixed commands.
- **The signing key** is kept out of the repository. It lives in GitHub Actions secrets and is available to exactly one workflow step, which signs the finished installer. The step that compiles the app, where hundreds of dependencies run build scripts, never has it. The release build uses no caches, and the build tool is pinned to an exact version.
- **Dependencies** are checked against the RustSec advisory database in CI, and the GitHub Actions used for builds are pinned to exact commits. Release builds use no caches and start from a clean state.

## Known limits

- The Windows installer is not code signed yet, so SmartScreen warns on first run. Verify downloads with the `SHA256SUMS.txt` file and the build attestation on the release page.
- The app edits CapCut project files in place. A backup is made first, but nothing about CapCut's file format is guaranteed.
- If the update signing key were ever lost or leaked, installed apps could not be moved to a build signed with a new key automatically. They would need a fresh manual install. The key is backed up for that reason, and a leak would be handled by publishing a new release and warning users.
- The first version that has updates built in has to be installed by hand once. Older versions do not know how to update themselves.
- The API key is held in ordinary memory while a request is being made. It is not wiped afterwards.
- `cargo audit` reports two warnings for crates that only matter on other operating systems (an unmaintained macro helper and a Linux-only GUI library). They are not part of the Windows build.
