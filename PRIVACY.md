# Privacy policy

Last updated: 2026-11-08

Singlish Caption Studio runs on your own computer. I do not run any server for it, I do not collect anything about you, and the app has no analytics, tracking or crash reporting.

This page says exactly what the app does with your data.

## What leaves your computer

When you press **Generate captions**, the app sends the audio of the clips you ticked to ElevenLabs so it can be turned into text. The request contains:

- the audio, cut from your source files (only the parts used on the timeline, plus a fraction of a second on each side), as a mono WAV file;
- your ElevenLabs API key, in a request header, so ElevenLabs knows whose account to use;
- the model name and the language hint.

It goes to `api.elevenlabs.io` over HTTPS and nowhere else. ElevenLabs handles that audio under its own terms and privacy policy, which you can read at https://elevenlabs.io/privacy. I have no access to it. How long ElevenLabs keeps audio and transcripts is up to them and your account settings.

The app asks for your permission first. Nothing is sent unless the consent box in the main window is ticked, and audio that was already transcribed is read from a local cache instead of being sent again.

Two buttons open a web page in your browser (the ElevenLabs API keys page and the Buy Me a Coffee page). They only open those two fixed addresses.

## What stays on your computer

| What | Where | Notes |
| --- | --- | --- |
| ElevenLabs API key | Windows Credential Manager | Encrypted by Windows for your user account. Never written to a file, never shown in the app after you save it. |
| Settings (fonts, sizes, color, position, price) | `%APPDATA%\Singlish Caption Studio\config.json` | |
| Glossary | `%APPDATA%\Singlish Caption Studio\glossary.json` | |
| Usage log | `%APPDATA%\Singlish Caption Studio\usage.json` | Seconds, request counts, word counts and clip names. No audio and no caption text. The newest 200 runs are kept. |
| Transcript cache | `%LOCALAPPDATA%\Singlish Caption Studio Data\cache` | The words and timings ElevenLabs returned, so the same audio is never paid for twice. |
| Project backups | Inside your CapCut project folder, next to `draft_content.json` | Named `draft_content.json.bak-<number>`. |
| Browser storage | The app's own web view | Remembers whether you ticked the consent box. |

Your video files are never copied, moved or uploaded as files. Only decoded audio from the ticked clips is sent.

## Deleting your data

- Remove the key in the app under Settings, then Account. You can also delete the "Singlish Caption Studio" entry in Windows Credential Manager.
- Clear the usage log in Settings, then Usage.
- Delete the two folders in the table above to remove settings, glossary and cache.
- Delete the `.bak` files in a project folder if you no longer need the backups.
- Uninstalling the app removes the program but leaves your settings, so delete the folders as well if you want everything gone.

To delete what ElevenLabs holds, use your ElevenLabs account.

## Children

The app is not aimed at children and does not knowingly collect anything from anyone.

## Changes

If this policy changes, the new version will be in this repository with a new date at the top, and the change will be mentioned in the changelog.

## Questions

Open an issue in this repository. Please do not post your API key, project files or anything private there.
