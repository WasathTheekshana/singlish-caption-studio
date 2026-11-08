//! The one-click job. For every selected clip it transcribes only the part of the source file
//! that is on the timeline (cached), maps the words onto the timeline, restores English words
//! and groups them into captions. Then everything is written to the project in one safe step.

use std::{
    cell::RefCell,
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    audio,
    draft::{self, Clip},
    eleven, fonts, secrets, srt, store,
    text::{group_tokens, Glossary, Grouping, Token},
    usage::{self, ClipUsage, RequestUsage},
};

pub const LANGUAGE: &str = "sin";
/// Extra audio around a clip so words on its edges are heard whole.
const PAD_S: f64 = 0.3;
/// Clips cut from one file with gaps no longer than this are transcribed in a single
/// request: fewer calls, and the recognizer hears the surrounding speech.
const MERGE_GAP_S: f64 = 2.0;

/// The words of one source-file range, plus what the request cost in audio.
pub struct Fetched {
    /// Tokens in source-file seconds.
    pub tokens: Vec<Token>,
    pub cached: bool,
    /// Length of the audio in the request, in seconds.
    pub audio_s: f64,
}

/// Gets the words of one source-file range.
pub type Transcriber<'a> = &'a dyn Fn(&Clip, f64, f64) -> Result<Fetched, String>;

/// Saves the captions as an .srt file and returns the file's name.
pub type SrtSaver<'a> = &'a dyn Fn(&[Token]) -> Result<String, String>;

pub struct Deps<'a> {
    pub transcribe: Transcriber<'a>,
    pub style: draft::Style,
    pub glossary: Glossary,
    /// False only in tests, where no real CapCut is involved.
    pub check_capcut: bool,
    /// Where to report usage. Tests leave this empty or collect the runs.
    pub record: Option<&'a dyn Fn(usage::Run)>,
    /// How captions are split: the most words in one box, and where the limits are.
    pub grouping: Grouping,
    /// Saves the captions as an .srt file and returns its name. Tests leave this empty.
    pub save_srt: Option<SrtSaver<'a>>,
}

pub fn resolve_style(
    cfg: &store::AppConfig,
    installed: &[fonts::Font],
) -> Result<draft::Style, String> {
    let s = &cfg.style;
    let find = |label: &str, name: &str| {
        fonts::find(installed, name).ok_or_else(|| {
            format!(
                "The {label} font \"{name}\" is not installed. Choose another in Settings, Style."
            )
        })
    };
    Ok(draft::Style {
        si_font_path: find("Sinhala", &s.sinhala.font)?.path.clone(),
        en_font_path: find("English", &s.english.font)?.path.clone(),
        si_size: s.sinhala.size,
        en_size: s.english.size,
        en_bold: s.english.bold,
        color: s.color,
        position_y: s.position_y,
    })
}

fn file_key(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|_| "Could not open the video file.")?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|_| "Could not read the video file.")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn ms(x: f64) -> i64 {
    (x * 1000.0).round() as i64
}

fn cache_path(video_key: &str, start: f64, end: f64) -> Result<PathBuf, String> {
    Ok(store::cache_dir()?.join(format!(
        "{video_key}_{}_{}_{LANGUAGE}_{}.json",
        ms(start),
        ms(end),
        eleven::MODEL
    )))
}

/// Cached words, and the audio length stored with them when there is one.
fn load_cached(p: &Path) -> Option<(Vec<Token>, Option<f64>)> {
    let v: Value = serde_json::from_str(&fs::read_to_string(p).ok()?).ok()?;
    let words = v["words"].as_array()?;
    let tokens = words
        .iter()
        .filter_map(|w| {
            Some(Token {
                text: w["text"].as_str()?.to_string(),
                start: w["start"].as_f64()?,
                end: w["end"].as_f64()?,
            })
        })
        .collect();
    Some((tokens, v["audio_s"].as_f64()))
}

fn save_cache(p: &Path, tokens: &[Token], audio_s: f64) -> Result<(), String> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|_| "Could not create the cache folder.")?;
    }
    let words: Vec<Value> = tokens
        .iter()
        .map(|t| json!({"text": t.text, "start": t.start, "end": t.end}))
        .collect();
    store::write_atomic(
        p,
        &json!({"model": eleven::MODEL, "language": LANGUAGE, "audio_s": audio_s, "words": words})
            .to_string(),
    )
}

/// The real transcriber: cache first, then (only with consent) extract audio and call ElevenLabs.
fn cloud_fetch(
    allow_cloud: bool,
    hashes: &RefCell<HashMap<String, String>>,
    clip: &Clip,
    start: f64,
    end: f64,
) -> Result<Fetched, String> {
    let key = {
        let mut memo = hashes.borrow_mut();
        match memo.get(&clip.path) {
            Some(k) => k.clone(),
            None => {
                let k = file_key(Path::new(&clip.path))?;
                memo.insert(clip.path.clone(), k.clone());
                k
            }
        }
    };
    let cache = cache_path(&key, start, end)?;
    if let Some((tokens, audio_s)) = load_cached(&cache) {
        return Ok(Fetched {
            tokens,
            cached: true,
            audio_s: audio_s.unwrap_or(end - start),
        });
    }
    if !allow_cloud {
        return Err(
            "consent_required: this will send the video's audio to ElevenLabs. Allow it in the app first."
                .into(),
        );
    }
    let api_key = secrets::get()?.ok_or("No ElevenLabs key is saved yet. Add it in Settings.")?;
    let wav = audio::extract_wav(Path::new(&clip.path), Some((start, end)))?;
    let reply = eleven::transcribe(&api_key, &wav, Some(LANGUAGE))?;
    let tokens: Vec<Token> = reply
        .words
        .into_iter()
        .map(|t| Token {
            text: t.text,
            start: t.start + start,
            end: t.end + start,
        })
        .collect();
    let audio_s = reply.audio_s.unwrap_or(end - start);
    let _ = save_cache(&cache, &tokens, audio_s); // a cache write failure is not fatal
    Ok(Fetched {
        tokens,
        cached: false,
        audio_s,
    })
}

/// One request: a span of one source file, and the clips (indices) that use it.
#[derive(Debug, PartialEq)]
struct Fetch {
    path: String,
    lo: f64,
    hi: f64,
    clips: Vec<usize>,
}

fn plan_fetches(clips: &[Clip]) -> Vec<Fetch> {
    let mut out: Vec<Fetch> = Vec::new();
    for (i, c) in clips.iter().enumerate() {
        let (lo, hi) = (c.src_start, c.src_start + c.src_dur);
        match out
            .iter_mut()
            .find(|f| f.path == c.path && lo <= f.hi + MERGE_GAP_S && hi >= f.lo - MERGE_GAP_S)
        {
            Some(f) => {
                f.lo = f.lo.min(lo);
                f.hi = f.hi.max(hi);
                f.clips.push(i);
            }
            None => out.push(Fetch {
                path: c.path.clone(),
                lo,
                hi,
                clips: vec![i],
            }),
        }
    }
    out
}

fn stage(emit: &dyn Fn(Value), name: &str, message: &str) {
    emit(json!({"event": "stage", "name": name, "message": message}));
}

/// A clip whose file simply has no sound is skipped; anything else is a real failure.
fn is_silent_error(e: &str) -> bool {
    e.starts_with("This video has no audio") || e.starts_with("No audio was found")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Runs the whole job, reporting progress through `emit`. Always ends with `done` or `error`.
/// `selected` holds the segment ids to caption; `None` means the sensible default.
pub fn run(
    project: &Path,
    title: &str,
    allow_cloud: bool,
    selected: Option<&[String]>,
    emit: &dyn Fn(Value),
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let installed = fonts::installed();
        let cfg = store::load_config(&installed);
        let style = resolve_style(&cfg, &installed)?; // fail early, before any paid work
        let hashes = RefCell::new(HashMap::new());
        let fetch = |c: &Clip, s: f64, e: f64| cloud_fetch(allow_cloud, &hashes, c, s, e);
        let save = |run: usage::Run| {
            let _ = usage::record(run);
        };
        let save_srt = |captions: &[Token]| srt::save(title, captions);
        let deps = Deps {
            transcribe: &fetch,
            style,
            glossary: Glossary::new(&store::load_glossary()),
            check_capcut: true,
            record: Some(&save),
            grouping: Grouping::with_max_words(cfg.style.max_words),
            save_srt: Some(&save_srt),
        };
        run_with(project, selected, emit, &deps)
    }))
    .unwrap_or_else(|_| {
        Err(
            "Something unexpected went wrong. If the project looks wrong, use Restore backup."
                .to_string(),
        )
    });
    match result {
        Ok((n, skipped)) => emit(json!({"event": "done", "segments": n, "skipped": skipped})),
        Err(message) => emit(json!({"event": "error", "message": message})),
    }
}

/// Returns (captions written, clips skipped for having no sound).
pub fn run_with(
    project: &Path,
    selected: Option<&[String]>,
    emit: &dyn Fn(Value),
    deps: &Deps,
) -> Result<(usize, usize), String> {
    stage(emit, "transcribe", "Reading your project");
    let info = draft::inspect(project)?;
    if deps.check_capcut && info.capcut_running {
        return Err(
            "CapCut is running. Close it completely (check the tray icon), then try again.".into(),
        );
    }
    if let Some(p) = info.problems.first() {
        return Err(p.clone());
    }
    let chosen: Vec<Clip> = info
        .clips
        .iter()
        .filter(|c| selected.map_or_else(|| c.default_selected(), |ids| ids.contains(&c.id)))
        .cloned()
        .collect();
    if chosen.is_empty() {
        return Err("No clips are selected. Tick the clips that contain speech.".into());
    }
    if let Some(bad) = chosen.iter().find(|c| !c.file_ok) {
        return Err(format!(
            "The video file \"{}\" was not found. Was it moved or deleted?",
            bad.name
        ));
    }
    let clips = draft::merge_adjacent(&chosen);
    let fetches = plan_fetches(&clips);

    // Step 1: ask for the words, one request per planned span. Usage is kept even if a later request fails.
    let mut requests: Vec<RequestUsage> = Vec::new();
    let mut words: Vec<Option<(Vec<Token>, usize)>> = Vec::with_capacity(fetches.len());
    let mut skipped = 0usize;
    let mut failure: Option<String> = None;
    for (i, f) in fetches.iter().enumerate() {
        let msg = if fetches.len() == 1 {
            "Transcribing speech".to_string()
        } else {
            format!("Transcribing part {} of {}", i + 1, fetches.len())
        };
        stage(emit, "transcribe", &msg);
        let (start, end) = ((f.lo - PAD_S).max(0.0), f.hi + PAD_S);
        match (deps.transcribe)(&clips[f.clips[0]], start, end) {
            Ok(got) => {
                requests.push(RequestUsage {
                    audio_s: got.audio_s,
                    cached: got.cached,
                    words: got.tokens.len(),
                    chars: got.tokens.iter().map(|t| t.text.chars().count()).sum(),
                });
                words.push(Some((got.tokens, requests.len())));
            }
            Err(e) if is_silent_error(&e) => {
                skipped += f.clips.len();
                words.push(None);
            }
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    let project_name = project
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let record = |clips: Vec<ClipUsage>| {
        if let (Some(rec), false) = (deps.record, requests.is_empty()) {
            rec(usage::Run {
                when: now_secs(),
                project: project_name.clone(),
                requests: requests.clone(),
                clips,
            });
        }
    };
    if let Some(e) = failure {
        record(Vec::new()); // earlier requests were still paid for
        return Err(e);
    }

    // Step 2: build captions clip by clip, so a caption never straddles a cut.
    let mut captions = Vec::new();
    let mut clip_usage = Vec::new();
    for (fi, f) in fetches.iter().enumerate() {
        let Some((tokens, request)) = &words[fi] else {
            continue;
        };
        for &ci in &f.clips {
            let clip = &clips[ci];
            let mapped = draft::map_to_timeline(tokens, clip);
            clip_usage.push(ClipUsage {
                name: clip.name.clone(),
                start: clip.tl_start,
                audio_s: clip.src_dur,
                words: mapped.len(),
                request: *request,
            });
            captions.extend(group_tokens(
                &deps.glossary.restore(&mapped),
                &deps.grouping,
            ));
        }
    }
    clip_usage.sort_by(|a, b| a.start.total_cmp(&b.start));
    let all_cached = requests.iter().all(|r| r.cached);
    emit(json!({"event": "info", "transcript_source": if all_cached { "cache" } else { "api" }}));
    emit(json!({
        "event": "usage",
        "requests": requests.len(),
        "billed_requests": requests.iter().filter(|r| !r.cached).count(),
        "cached_requests": requests.iter().filter(|r| r.cached).count(),
        "billed_seconds": requests.iter().filter(|r| !r.cached).map(|r| r.audio_s).sum::<f64>(),
    }));
    record(clip_usage);

    stage(emit, "captions", "Building captions");
    let captions = draft::resolve_overlaps(captions);
    if captions.is_empty() {
        return Err("No speech was found in the selected clips.".into());
    }
    let lines: Vec<Value> = captions
        .iter()
        .map(|c| json!({"start": c.start, "end": c.end, "text": c.text}))
        .collect();
    emit(json!({"event": "captions", "lines": lines}));

    // Saved before the project is touched, so a failed write never wastes paid transcription.
    if let Some(save) = deps.save_srt {
        match save(&captions) {
            Ok(file) => emit(json!({"event": "srt", "saved": true, "file": file})),
            Err(message) => emit(json!({"event": "srt", "saved": false, "message": message})),
        }
    }

    stage(emit, "write", "Writing to your project");
    let n = draft::write_captions_with(project, &captions, &deps.style, 0.0, deps.check_capcut)?;
    Ok((n, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Run;
    use std::cell::Cell;

    #[test]
    fn cache_round_trip() {
        let p = std::env::temp_dir().join(format!("scs-test-cache-{}.json", std::process::id()));
        let toks = vec![Token {
            text: "ඔයාට".into(),
            start: 1.5,
            end: 2.0,
        }];
        save_cache(&p, &toks, 3.5).unwrap();
        assert_eq!(load_cached(&p).unwrap(), (toks, Some(3.5)));
        let _ = fs::remove_file(p);
    }

    #[test]
    fn an_old_cache_file_without_a_duration_still_loads() {
        let p =
            std::env::temp_dir().join(format!("scs-test-cache-old-{}.json", std::process::id()));
        fs::write(&p, r#"{"words":[{"text":"a","start":0.0,"end":0.5}]}"#).unwrap();
        let (t, audio) = load_cached(&p).unwrap();
        assert_eq!((t.len(), audio), (1, None));
        let _ = fs::remove_file(p);
    }

    #[test]
    fn missing_or_corrupt_cache_is_a_miss() {
        assert!(load_cached(Path::new("Z:/nope.json")).is_none());
        let p =
            std::env::temp_dir().join(format!("scs-test-cache-bad-{}.json", std::process::id()));
        fs::write(&p, "{ not json").unwrap();
        assert!(load_cached(&p).is_none());
        let _ = fs::remove_file(p);
    }

    #[test]
    fn missing_font_stops_before_any_paid_work() {
        let cfg = store::default_config(&[]);
        assert!(resolve_style(&cfg, &[])
            .unwrap_err()
            .contains("not installed"));
    }

    #[test]
    fn same_file_same_key_different_file_different_key() {
        let dir = std::env::temp_dir();
        let (a, b) = (dir.join("scs-key-a.bin"), dir.join("scs-key-b.bin"));
        fs::write(&a, b"one").unwrap();
        fs::write(&b, b"two").unwrap();
        assert_eq!(file_key(&a).unwrap(), file_key(&a).unwrap());
        assert_ne!(file_key(&a).unwrap(), file_key(&b).unwrap());
        assert_eq!(file_key(&a).unwrap().len(), 32);
        let _ = (fs::remove_file(a), fs::remove_file(b));
    }

    /// The cache key must not change when the hashing library is upgraded, or every saved
    /// transcript would quietly stop being found. SHA-256 of "abc" is a published test value.
    #[test]
    fn the_cache_key_is_the_start_of_the_standard_sha256() {
        let f = std::env::temp_dir().join(format!("scs-sha-abc-{}.bin", std::process::id()));
        fs::write(&f, b"abc").unwrap();
        assert_eq!(file_key(&f).unwrap(), "ba7816bf8f01cfea414140de5dae2223");
        let _ = fs::remove_file(f);
    }

    #[test]
    fn different_ranges_of_one_file_are_cached_separately() {
        let a = cache_path("abc", 0.0, 4.3).unwrap();
        let b = cache_path("abc", 4.0, 8.3).unwrap();
        assert_ne!(a, b);
        assert!(a.to_string_lossy().contains("abc_0_4300_"));
    }

    fn mk_clip(path: &str, src: (f64, f64)) -> Clip {
        Clip {
            id: "x".into(),
            ids: vec!["x".into()],
            path: path.into(),
            name: path.into(),
            file_ok: true,
            tl_start: 0.0,
            tl_dur: src.1 - src.0,
            src_start: src.0,
            src_dur: src.1 - src.0,
            volume: 1.0,
        }
    }

    #[test]
    fn planner_groups_close_cuts_of_one_file_and_separates_the_rest() {
        let clips = vec![
            mk_clip("a", (0.0, 2.0)),
            mk_clip("a", (3.0, 5.0)),
            mk_clip("a", (30.0, 32.0)),
            mk_clip("b", (0.0, 1.0)),
            mk_clip("a", (5.5, 7.0)),
        ];
        let plan = plan_fetches(&clips);
        assert_eq!(plan.len(), 3);
        // Gaps of 1 s and 0.5 s are bridged.
        assert_eq!(
            (plan[0].lo, plan[0].hi, plan[0].clips.clone()),
            (0.0, 7.0, vec![0, 1, 4])
        );
        assert_eq!(plan[1].clips, vec![2]); // 25 s away, so its own request
        assert_eq!(plan[2].path, "b");
    }

    // ------------------------------------------------------------- end to end, no network

    struct Fixture {
        root: PathBuf,
        project: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("scs-e2e-{tag}-{}", std::process::id()));
            let project = root.join("com.lveditor.draft").join("proj");
            fs::create_dir_all(&project).unwrap();
            for f in ["a.mov", "b.mov"] {
                fs::write(root.join(f), b"x").unwrap();
            }
            let p = |f: &str| root.join(f).to_string_lossy().replace('\\', "/");
            let us = |x: f64| (x * 1e6) as i64;
            let seg = |id: &str, m: &str, tl: (f64, f64), src: (f64, f64), vol: f64| {
                json!({"id": id, "material_id": m, "volume": vol, "reverse": false,
                       "target_timerange": {"start": us(tl.0), "duration": us(tl.1)},
                       "source_timerange": {"start": us(src.0), "duration": us(src.1)}})
            };
            let draft = json!({"duration": us(12.0), "tracks": [
                {"type": "video", "segments": [
                    seg("a1", "A", (0.0, 2.0), (0.0, 2.0), 1.0),
                    seg("a2", "A", (2.0, 2.0), (2.0, 2.0), 1.0),
                    seg("b1", "B", (4.0, 1.0), (10.0, 2.0), 1.0)]},
                {"type": "video", "segments": [seg("leak", "A", (1.0, 1.0), (0.0, 1.0), 0.2)]}],
                "materials": {"videos": [
                    {"id": "A", "type": "video", "path": p("a.mov"), "has_audio": true},
                    {"id": "B", "type": "video", "path": p("b.mov"), "has_audio": true}],
                    "texts": [], "material_animations": []}});
            fs::write(project.join("draft_content.json"), draft.to_string()).unwrap();
            Self { root, project }
        }

        fn captions_in_draft(&self) -> Vec<(String, f64)> {
            let d: Value = serde_json::from_str(
                &fs::read_to_string(self.project.join("draft_content.json")).unwrap(),
            )
            .unwrap();
            let tr = d["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == draft::TRACK_NAME)
                .expect("caption track");
            tr["segments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    let m = d["materials"]["texts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|m| m["id"] == s["material_id"])
                        .unwrap();
                    let c: Value = serde_json::from_str(m["content"].as_str().unwrap()).unwrap();
                    (
                        c["text"].as_str().unwrap().to_string(),
                        s["target_timerange"]["start"].as_f64().unwrap() / 1e6,
                    )
                })
                .collect()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn test_style() -> draft::Style {
        draft::Style {
            si_font_path: "si.ttf".into(),
            en_font_path: "en.ttf".into(),
            si_size: 17.0,
            en_size: 14.0,
            en_bold: true,
            color: [1.0, 1.0, 1.0],
            position_y: -0.45,
        }
    }

    /// One word per whole second inside the requested source range, named after its second.
    fn fake(calls: &Cell<usize>) -> impl Fn(&Clip, f64, f64) -> Result<Fetched, String> + '_ {
        move |_c, s, e| {
            calls.set(calls.get() + 1);
            let first = s.ceil() as i64;
            let tokens = (first..=e.floor() as i64)
                .map(|t| Token {
                    text: format!("w{t}"),
                    start: t as f64,
                    end: t as f64 + 0.4,
                })
                .collect();
            Ok(Fetched {
                tokens,
                cached: false,
                audio_s: e - s,
            })
        }
    }

    fn run_fixture(
        f: &Fixture,
        selected: Option<&[String]>,
        tx: Transcriber,
    ) -> Result<(usize, usize), String> {
        let deps = Deps {
            transcribe: tx,
            style: test_style(),
            glossary: Glossary::new(&Default::default()),
            check_capcut: false,
            record: None,
            grouping: Grouping::default(),
            save_srt: None,
        };
        run_with(&f.project, selected, &|_| {}, &deps)
    }

    #[test]
    fn the_srt_is_saved_from_the_same_captions_before_the_project_is_written() {
        let f = Fixture::new("srt");
        let calls = Cell::new(0);
        let tx = fake(&calls);
        let saved = RefCell::new(Vec::new());
        let order = RefCell::new(Vec::new());
        let save = |caps: &[Token]| {
            saved.borrow_mut().extend(caps.iter().cloned());
            Ok("demo.si_LK.srt".to_string())
        };
        let deps = Deps {
            transcribe: &tx,
            style: test_style(),
            glossary: Glossary::new(&Default::default()),
            check_capcut: false,
            record: None,
            grouping: Grouping::default(),
            save_srt: Some(&save),
        };
        let emit = |ev: Value| {
            order
                .borrow_mut()
                .push(ev["event"].as_str().unwrap_or("").to_string())
        };
        let (n, _) = run_with(&f.project, None, &emit, &deps).unwrap();
        let saved = saved.into_inner();
        assert_eq!(
            saved.len(),
            n,
            "the file and the project hold the same captions"
        );
        let order = order.into_inner();
        let pos = |name: &str| order.iter().position(|e| e == name);
        assert!(pos("srt").is_some(), "{order:?}");
        assert!(pos("captions") < pos("srt"), "{order:?}");
    }

    #[test]
    fn a_failed_srt_save_does_not_stop_the_captions() {
        let f = Fixture::new("srt-fail");
        let calls = Cell::new(0);
        let tx = fake(&calls);
        let save = |_: &[Token]| Err("Could not save the caption file.".to_string());
        let deps = Deps {
            transcribe: &tx,
            style: test_style(),
            glossary: Glossary::new(&Default::default()),
            check_capcut: false,
            record: None,
            grouping: Grouping::default(),
            save_srt: Some(&save),
        };
        let events = RefCell::new(Vec::new());
        let emit = |ev: Value| events.borrow_mut().push(ev);
        let (n, _) = run_with(&f.project, None, &emit, &deps).unwrap();
        assert!(n > 0 && f.captions_in_draft().len() == n);
        let events = events.into_inner();
        let srt = events.iter().find(|e| e["event"] == "srt").unwrap();
        assert_eq!(srt["saved"], false);
    }

    #[test]
    fn multi_clip_end_to_end_with_defaults() {
        let f = Fixture::new("defaults");
        let calls = Cell::new(0);
        let tx = fake(&calls);
        let (n, skipped) = run_fixture(&f, None, &tx).unwrap();
        assert_eq!(skipped, 0);
        // a1 and a2 are back to back so they merge into one request. b1 is the second.
        // The quiet overlay is not sent.
        assert_eq!(calls.get(), 2, "should transcribe two ranges");
        let caps = f.captions_in_draft();
        assert_eq!(caps.len(), n);
        let text: Vec<&str> = caps.iter().map(|(t, _)| t.as_str()).collect();
        assert!(
            text.iter().any(|t| t.contains("w0")) && text.iter().any(|t| t.contains("w3")),
            "{text:?}"
        );
        // b1 plays source 10..12 at double speed from timeline 4.0: w10 lands at 4.0, w11 at 4.5.
        let b_starts: Vec<f64> = caps
            .iter()
            .filter(|(t, _)| t.contains("w10") || t.contains("w11"))
            .map(|(_, s)| *s)
            .collect();
        assert!(
            !b_starts.is_empty() && (b_starts[0] - 4.0).abs() < 1e-6,
            "{caps:?}"
        );
        for w in caps.windows(2) {
            assert!(w[0].1 <= w[1].1);
        }
    }

    #[test]
    fn user_selection_is_respected_including_the_overlay() {
        let f = Fixture::new("selection");
        let calls = Cell::new(0);
        let tx = fake(&calls);
        run_fixture(&f, Some(&["leak".to_string()]), &tx).unwrap();
        assert_eq!(calls.get(), 1);
        assert!(run_fixture(&f, Some(&["nope".to_string()]), &tx)
            .unwrap_err()
            .contains("No clips are selected"));
    }

    #[test]
    fn rerun_replaces_captions_and_clips_without_sound_are_skipped() {
        let f = Fixture::new("rerun");
        let calls = Cell::new(0);
        let tx = fake(&calls);
        run_fixture(&f, None, &tx).unwrap();
        let first = f.captions_in_draft().len();
        let silent_b = |c: &Clip, s: f64, e: f64| {
            if c.name == "b.mov" {
                Err("This video has no audio track.".to_string())
            } else {
                fake(&calls)(c, s, e)
            }
        };
        let (_, skipped) = run_fixture(&f, None, &silent_b).unwrap();
        assert_eq!(skipped, 1);
        assert!(
            f.captions_in_draft().len() < first,
            "the second run must replace the first"
        );
    }

    #[test]
    fn a_transcription_failure_stops_everything_and_changes_nothing() {
        let f = Fixture::new("fail");
        let before = fs::read(f.project.join("draft_content.json")).unwrap();
        let boom = |_: &Clip, _: f64, _: f64| {
            Err::<Fetched, String>("ElevenLabs is having problems right now.".into())
        };
        assert!(run_fixture(&f, None, &boom)
            .unwrap_err()
            .contains("problems"));
        assert_eq!(
            fs::read(f.project.join("draft_content.json")).unwrap(),
            before
        );
    }

    #[test]
    fn a_missing_video_file_is_skipped_by_default_and_named_if_chosen() {
        let f = Fixture::new("missing");
        fs::remove_file(f.root.join("b.mov")).unwrap();
        let calls = Cell::new(0);
        let tx = fake(&calls);
        // The clip with a missing file is not ticked by default, so the rest still works.
        run_fixture(&f, None, &tx).unwrap();
        assert_eq!(calls.get(), 1, "only the merged a.mov range is transcribed");
        // If the user ticks it anyway, fail clearly and before any paid work.
        calls.set(0);
        let err = run_fixture(&f, Some(&["b1".to_string()]), &tx).unwrap_err();
        assert!(err.contains("b.mov"), "{err}");
        assert_eq!(calls.get(), 0, "must fail before any paid work");
    }

    #[test]
    fn close_cuts_with_a_gap_cost_one_request() {
        let f = Fixture::new("gap");
        // Two cuts of a.mov with a 1 s hole between them.
        let us = |x: f64| (x * 1e6) as i64;
        let p = f.root.join("a.mov").to_string_lossy().replace('\\', "/");
        let seg = |id: &str, tl: f64, src: f64| {
            json!({"id": id, "material_id": "A", "volume": 1.0, "reverse": false,
                   "target_timerange": {"start": us(tl), "duration": us(2.0)},
                   "source_timerange": {"start": us(src), "duration": us(2.0)}})
        };
        let d = json!({"duration": us(4.0), "tracks": [{"type": "video", "segments": [seg("c1", 0.0, 0.0), seg("c2", 2.0, 3.0)]}],
            "materials": {"videos": [{"id": "A", "type": "video", "path": p, "has_audio": true}], "texts": [], "material_animations": []}});
        fs::write(f.project.join("draft_content.json"), d.to_string()).unwrap();
        let calls = Cell::new(0);
        let tx = fake(&calls);
        run_fixture(&f, None, &tx).unwrap();
        assert_eq!(calls.get(), 1, "one request covers both cuts");
        let all: String = f
            .captions_in_draft()
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        // The word at source second 2 sits in the removed gap, so it must not appear.
        assert!(
            all.contains("w0") && all.contains("w1") && all.contains("w3") && all.contains("w4"),
            "{all}"
        );
        assert!(!all.contains("w2"), "{all}");
    }

    // ------------------------------------------------------------------------- usage

    fn collect(
        f: &Fixture,
        selected: Option<&[String]>,
        tx: Transcriber,
    ) -> (Result<(usize, usize), String>, Vec<Run>) {
        let runs = RefCell::new(Vec::new());
        let rec = |r: Run| runs.borrow_mut().push(r);
        let deps = Deps {
            transcribe: tx,
            style: test_style(),
            glossary: Glossary::new(&Default::default()),
            check_capcut: false,
            record: Some(&rec),
            grouping: Grouping::default(),
            save_srt: None,
        };
        let out = run_with(&f.project, selected, &|_| {}, &deps);
        (out, runs.into_inner())
    }

    #[test]
    fn usage_is_recorded_per_request_and_per_clip() {
        let f = Fixture::new("usage");
        let calls = Cell::new(0);
        let tx = fake(&calls);
        let (res, runs) = collect(&f, None, &tx);
        res.unwrap();
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!(r.project, "proj");
        assert_eq!(r.requests.len(), 2);
        assert_eq!(r.billed_requests(), 2);
        // a.mov covers source 0..4 plus padding (0..4.3), b.mov 10..12 plus padding (9.7..12.3).
        assert!(
            (r.requests[0].audio_s - 4.3).abs() < 1e-9
                && (r.requests[1].audio_s - 2.6).abs() < 1e-9
        );
        assert!((r.billed_seconds() - 6.9).abs() < 1e-9);
        // a1 and a2 were merged into one clip, so there are two clips: the merged a.mov and b1.
        assert_eq!(r.clips.len(), 2);
        assert_eq!((r.clips[0].name.as_str(), r.clips[0].request), ("a.mov", 1));
        assert!((r.clips[0].audio_s - 4.0).abs() < 1e-9);
        assert_eq!((r.clips[1].name.as_str(), r.clips[1].request), ("b.mov", 2));
        assert!(r.clips.iter().all(|c| c.words > 0));
    }

    #[test]
    fn answers_from_the_cache_are_marked_and_not_billed() {
        let f = Fixture::new("usage-cache");
        let calls = Cell::new(0);
        let cached_b = |c: &Clip, s: f64, e: f64| {
            let mut got = fake(&calls)(c, s, e)?;
            got.cached = c.name == "b.mov";
            Ok(got)
        };
        let (res, runs) = collect(&f, None, &cached_b);
        res.unwrap();
        let r = &runs[0];
        assert_eq!(r.billed_requests(), 1);
        assert!((r.billed_seconds() - 4.3).abs() < 1e-9);
        assert!(r.requests[1].cached);
    }

    #[test]
    fn a_failure_after_a_paid_request_still_records_that_request() {
        let f = Fixture::new("usage-partial");
        let before = fs::read(f.project.join("draft_content.json")).unwrap();
        let calls = Cell::new(0);
        let flaky = |c: &Clip, s: f64, e: f64| {
            if c.name == "b.mov" {
                Err("ElevenLabs is having problems right now.".to_string())
            } else {
                fake(&calls)(c, s, e)
            }
        };
        let (res, runs) = collect(&f, None, &flaky);
        assert!(res.unwrap_err().contains("problems"));
        assert_eq!(runs.len(), 1, "the paid request must be on record");
        assert_eq!(runs[0].billed_requests(), 1);
        assert!(runs[0].clips.is_empty());
        assert_eq!(
            fs::read(f.project.join("draft_content.json")).unwrap(),
            before
        );
    }

    #[test]
    fn nothing_is_recorded_when_nothing_was_requested() {
        let f = Fixture::new("usage-none");
        let calls = Cell::new(0);
        let tx = fake(&calls);
        let (res, runs) = collect(&f, Some(&["nope".to_string()]), &tx);
        assert!(res.is_err());
        assert!(runs.is_empty());
    }
}
