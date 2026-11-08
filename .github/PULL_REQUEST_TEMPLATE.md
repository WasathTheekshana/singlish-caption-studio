## What this changes

<!-- One or two sentences. Link the issue if there is one. -->

## Title

The title of this pull request becomes the commit message, and the commit messages decide the next version number. It has to look like `type(scope): short summary`, for example `fix: keep captions from overlapping`. A check runs on it, and the contributing guide has the list of types.

## Type of change

Tick one. It tells the reviewer what to expect, and the title prefix is what actually sets the version.

- [ ] Bug fix. Title starts with `fix:`. Next release is a patch (0.1.0 to 0.1.1).
- [ ] New feature. Title starts with `feat:`. Next release is a minor (0.1.0 to 0.2.0).
- [ ] Performance. Title starts with `perf:`. Next release is a patch.
- [ ] Breaking change. Title has a `!`, like `feat!:`, or the description has a `BREAKING CHANGE:` line. Next release is a major (a minor while the version is below 1.0).
- [ ] No release needed. Title starts with `docs:`, `test:`, `refactor:`, `style:`, `build:`, `ci:` or `chore:`.

### If it breaks something

<!-- What stops working, and what people need to do. Delete this section if nothing breaks. -->

## How I tested it

<!-- What you ran, and what you saw. -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass
- [ ] I added or updated a test
- [ ] I did not add any network calls outside `src/eleven.rs`
- [ ] No keys, project files or personal paths are included
