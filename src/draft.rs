//! Reads and edits CapCut project files (`draft_content.json`). All edits are validated in
//! memory first, backed up, then written atomically. `tests/fixtures/golden.json` pins the
//! expected output.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::text::{split_runs, Kind, Token};

pub const TRACK_NAME: &str = "Auto captions (SI+EN)";
pub const DRAFT_DIR_NAME: &str = "com.lveditor.draft";
const TEMPLATE: &str = include_str!("../resources/text_template.json");
const MIN_DURATION_US: i64 = 100_000;

/// Resolved fonts and look for the caption text.
#[derive(Debug, Clone)]
pub struct Style {
    pub si_font_path: String,
    pub en_font_path: String,
    pub si_size: f64,
    pub en_size: f64,
    pub en_bold: bool,
    pub color: [f64; 3],
    pub position_y: f64,
}

fn new_id() -> String {
    Uuid::new_v4().to_string().to_uppercase()
}

/// 1.0 -> 1 (as CapCut writes it); 0.5 stays 0.5.
fn num(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        json!(f as i64)
    } else {
        json!(f)
    }
}

fn to_hex(rgb: &[f64; 3]) -> String {
    format!(
        "#{}",
        rgb.iter()
            .map(|c| format!("{:02x}", (c * 255.0).round() as u8))
            .collect::<String>()
    )
}

pub fn build_content(text: &str, style: &Style) -> String {
    let mut styles = Vec::new();
    let mut pos = 0usize;
    for (kind, run) in split_runs(text) {
        let (font, size) = match kind {
            Kind::Si => (&style.si_font_path, style.si_size),
            Kind::En => (&style.en_font_path, style.en_size),
        };
        let mut st = Map::new();
        st.insert(
            "fill".into(),
            json!({"content": {"render_type": "solid", "solid": {"color": style.color.map(num)}}}),
        );
        st.insert("font".into(), json!({"path": font, "id": ""}));
        st.insert("size".into(), json!(size));
        if kind == Kind::En && style.en_bold {
            st.insert("bold".into(), json!(true));
        }
        st.insert("useLetterColor".into(), json!(true));
        let len = run.chars().count();
        st.insert("range".into(), json!([pos, pos + len]));
        pos += len;
        styles.push(Value::Object(st));
    }
    json!({"text": text, "styles": styles}).to_string()
}

fn micros(seconds: f64) -> i64 {
    (seconds * 1e6).round() as i64
}

fn arr<'a>(d: &'a mut Value, path: &[&str]) -> Result<&'a mut Vec<Value>, String> {
    let mut cur = d;
    for (i, key) in path.iter().enumerate() {
        let obj = cur.as_object_mut().ok_or("Unexpected project layout.")?;
        cur = obj.entry(*key).or_insert_with(|| {
            if i + 1 == path.len() {
                json!([])
            } else {
                json!({})
            }
        });
    }
    cur.as_array_mut()
        .ok_or_else(|| "Unexpected project layout.".to_string())
}

/// Make re-runs idempotent: drop an earlier caption track and the materials it used.
fn remove_previous(d: &mut Value) -> Result<(), String> {
    let tracks = arr(d, &["tracks"])?;
    let (old, keep): (Vec<Value>, Vec<Value>) = std::mem::take(tracks)
        .into_iter()
        .partition(|t| t["name"] == TRACK_NAME);
    *tracks = keep;
    let mut mats: HashSet<String> = HashSet::new();
    let mut refs: HashSet<String> = HashSet::new();
    for t in &old {
        for s in t["segments"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            mats.extend(s["material_id"].as_str().map(str::to_string));
            for r in s["extra_material_refs"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                refs.extend(r.as_str().map(str::to_string));
            }
        }
    }
    let is_in =
        |set: &HashSet<String>, m: &Value| m["id"].as_str().is_some_and(|id| set.contains(id));
    arr(d, &["materials", "texts"])?.retain(|m| !is_in(&mats, m));
    arr(d, &["materials", "material_animations"])?.retain(|m| !is_in(&refs, m));
    Ok(())
}

pub fn add_captions(
    d: &mut Value,
    caps: &[Token],
    style: &Style,
    offset_s: f64,
) -> Result<usize, String> {
    let tpl: Value =
        serde_json::from_str(TEMPLATE).map_err(|_| "Built-in caption style is damaged.")?;
    remove_previous(d)?;
    let mut segs = Vec::with_capacity(caps.len());
    for cap in caps {
        let mut m = tpl["material"].clone();
        m["id"] = json!(new_id());
        m["content"] = json!(build_content(&cap.text, style));
        m["font_path"] = json!(style.si_font_path);
        m["font_size"] = json!(style.si_size);
        m["text_color"] = json!(to_hex(&style.color));
        let mat_id = m["id"].clone();
        arr(d, &["materials", "texts"])?.push(m);
        let anim_id = json!(new_id());
        arr(d, &["materials", "material_animations"])?
            .push(json!({"id": anim_id, "type": "sticker_animation", "animations": [], "multi_language_current": "none"}));
        let mut s = tpl["segment"].clone();
        s["id"] = json!(new_id());
        s["material_id"] = mat_id;
        s["group_id"] = json!(new_id());
        s["extra_material_refs"] = json!([anim_id]);
        s["keyframe_refs"] = json!([]);
        s["common_keyframes"] = json!([]);
        s["target_timerange"] = json!({"start": micros(offset_s + cap.start), "duration": micros(cap.end - cap.start).max(MIN_DURATION_US)});
        s["clip"]["transform"] = json!({"x": 0.0, "y": style.position_y});
        segs.push(s);
    }
    let n = segs.len();
    arr(d, &["tracks"])?.push(
        json!({"id": new_id(), "type": "text", "segments": segs, "flag": 0, "attribute": 0,
                                      "name": TRACK_NAME, "is_default_name": false}),
    );
    Ok(n)
}

/// Structural checks CapCut needs. Empty result means the draft is consistent.
pub fn validate(d: &Value) -> Vec<String> {
    let mut errs = Vec::new();
    let empty = Vec::new();
    let texts: HashMap<&str, &Value> = d["materials"]["texts"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|m| Some((m["id"].as_str()?, m)))
        .collect();
    let all_ids: HashSet<&str> = d["materials"]
        .as_object()
        .map(|o| {
            o.values()
                .filter_map(Value::as_array)
                .flatten()
                .filter_map(|m| m["id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    let mut seen: HashSet<&str> = HashSet::new();
    for tr in d["tracks"].as_array().unwrap_or(&empty) {
        for s in tr["segments"].as_array().unwrap_or(&empty) {
            if let Some(id) = s["id"].as_str() {
                if !seen.insert(id) {
                    errs.push(format!("duplicate segment id {id}"));
                }
            }
            if tr["type"] != "text" {
                continue;
            }
            let Some(m) = s["material_id"].as_str().and_then(|id| texts.get(id)) else {
                errs.push(format!("segment {} has no text material", s["id"]));
                continue;
            };
            let content: Value =
                serde_json::from_str(m["content"].as_str().unwrap_or("")).unwrap_or(Value::Null);
            let total = content["text"].as_str().map_or(0, |t| t.chars().count());
            let covered: i64 = content["styles"]
                .as_array()
                .unwrap_or(&empty)
                .iter()
                .map(|st| {
                    st["range"][1].as_i64().unwrap_or(0) - st["range"][0].as_i64().unwrap_or(0)
                })
                .sum();
            if covered != total as i64 {
                errs.push(format!(
                    "style ranges cover {covered} of {total} characters"
                ));
            }
            for r in s["extra_material_refs"].as_array().unwrap_or(&empty) {
                if !r.as_str().is_some_and(|id| all_ids.contains(id)) {
                    errs.push(format!("segment {} has a missing reference", s["id"]));
                }
            }
        }
    }
    errs
}

// ------------------------------------------------------------------ project files

/// Project files can be large, but not this large. Anything bigger is refused, not loaded.
const MAX_DRAFT_BYTES: u64 = 256 * 1024 * 1024;

/// A project with more video segments than this is refused. No real edit comes close, and it
/// stops a crafted file from keeping the app busy for minutes.
const MAX_SEGMENTS: usize = 20_000;

/// How many old backups to keep per file (the very first one is always kept as well).
const KEEP_BACKUPS: usize = 10;

/// Windows treats these names as devices, even with an extension (`CON.mov`). Opening one can
/// block forever, so a media path containing one is refused.
const RESERVED_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// File types the audio decoder understands. A project that points a clip at anything else
/// (an .exe, a document) is treated as having a missing file.
const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "m4v", "m4a", "mkv", "webm", "wav", "mp3", "ogg", "flac", "aif", "aiff",
];

/// Media paths come from the project file, so they are untrusted. Only a plain local path on a
/// drive letter is accepted. Network paths (\\server\share) are refused without being touched,
/// because merely opening one makes Windows send the user's login hash to that server.
pub fn media_path_ok(path: &str) -> bool {
    let b = path.as_bytes();
    let drive = b.len() > 2
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'/' || b[2] == b'\\');
    let ext_ok = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    let device = path.split(['/', '\\']).any(|part| {
        let stem = part
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end()
            .to_ascii_lowercase();
        RESERVED_NAMES.contains(&stem.as_str())
    });
    drive && ext_ok && !device && !path.contains("..") && !path.contains(['\0', '\n', '\r'])
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let len = fs::metadata(path)
        .map_err(|_| "Could not read the project.")?
        .len();
    if len > MAX_DRAFT_BYTES {
        return Err("The project file is too large to open safely.".into());
    }
    fs::read(path).map_err(|_| "Could not read the project.".to_string())
}

pub fn check_project_dir(p: &Path) -> Result<PathBuf, String> {
    let p = p.canonicalize().map_err(|_| "Project folder not found.")?;
    let ok = p
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == DRAFT_DIR_NAME)
        && p.join("draft_content.json").is_file();
    if ok {
        Ok(p)
    } else {
        Err("That is not a CapCut project folder.".into())
    }
}

pub fn capcut_running() -> bool {
    let sys = std::env::var_os("SYSTEMROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let mut cmd = Command::new(sys.join("System32").join("tasklist.exe"));
    cmd.args(["/FI", "IMAGENAME eq CapCut.exe"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("CapCut.exe"))
        .unwrap_or(false)
}

pub fn draft_files(project: &Path) -> Vec<PathBuf> {
    let mut files = vec![project.join("draft_content.json")];
    if let Ok(rd) = fs::read_dir(project.join("Timelines")) {
        let mut subs: Vec<PathBuf> = rd
            .filter_map(Result::ok)
            .map(|e| e.path().join("draft_content.json"))
            .collect();
        subs.sort();
        files.extend(subs);
    }
    files.into_iter().filter(|f| f.is_file()).collect()
}

/// One piece of video on the timeline that has sound.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// CapCut's segment id (the first one, if clips were merged).
    pub id: String,
    pub ids: Vec<String>,
    pub path: String,
    pub name: String,
    pub file_ok: bool,
    pub tl_start: f64,
    pub tl_dur: f64,
    pub src_start: f64,
    pub src_dur: f64,
    pub volume: f64,
}

impl Clip {
    /// Source seconds per timeline second (2.0 = played at double speed).
    pub fn speed(&self) -> f64 {
        if self.tl_dur > 0.0 {
            self.src_dur / self.tl_dur
        } else {
            1.0
        }
    }
    /// Quiet background clips (light leaks, music beds) are unchecked by default.
    pub fn default_selected(&self) -> bool {
        self.file_ok && self.volume >= 0.5
    }
}

#[derive(Debug, Clone, Default)]
pub struct Info {
    pub capcut_running: bool,
    pub duration_s: f64,
    pub has_captions: bool,
    pub clips: Vec<Clip>,
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
}

fn secs(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0) / 1e6
}

/// Find every video clip with sound. Pure function over the parsed draft so it can be tested;
/// `exists` tells it whether a media file is present.
pub fn analyze(d: &Value, exists: &dyn Fn(&str) -> bool) -> Info {
    let empty = Vec::new();
    let mut info = Info {
        duration_s: secs(&d["duration"]),
        has_captions: d["tracks"]
            .as_array()
            .is_some_and(|t| t.iter().any(|t| t["name"] == TRACK_NAME)),
        ..Info::default()
    };
    let mats: HashMap<&str, &Value> = d["materials"]["videos"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter(|m| m["type"] == "video")
        .filter_map(|m| Some((m["id"].as_str()?, m)))
        .collect();
    let (mut reversed, mut missing, mut visited) = (0, 0, 0usize);
    for track in d["tracks"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter(|t| t["type"] == "video")
    {
        for seg in track["segments"].as_array().unwrap_or(&empty) {
            visited += 1;
            if visited > MAX_SEGMENTS {
                info.clips.clear();
                info.problems
                    .push("This project has too many clips to open safely.".into());
                return info;
            }
            let Some(mat) = seg["material_id"].as_str().and_then(|id| mats.get(id)) else {
                continue; // photos and unknown media
            };
            if mat["has_audio"].as_bool() == Some(false) {
                continue;
            }
            if seg["reverse"].as_bool() == Some(true) {
                reversed += 1;
                continue;
            }
            let tl_dur = secs(&seg["target_timerange"]["duration"]);
            if tl_dur <= 0.0 {
                continue;
            }
            let src = &seg["source_timerange"];
            let path = mat["path"].as_str().unwrap_or("").to_string();
            let file_ok = media_path_ok(&path) && exists(&path);
            missing += usize::from(!file_ok);
            let id = seg["id"].as_str().unwrap_or("").to_string();
            info.clips.push(Clip {
                ids: vec![id.clone()],
                id,
                name: path.rsplit(['/', '\\']).next().unwrap_or("").to_string(),
                path,
                file_ok,
                tl_start: secs(&seg["target_timerange"]["start"]),
                tl_dur,
                src_start: secs(&src["start"]),
                src_dur: if src.is_null() {
                    tl_dur
                } else {
                    secs(&src["duration"])
                },
                volume: seg["volume"].as_f64().unwrap_or(1.0),
            });
        }
    }
    info.clips.sort_by(|a, b| a.tl_start.total_cmp(&b.tl_start));
    if info.clips.is_empty() {
        info.problems
            .push("No video clip with sound was found on the timeline.".into());
    }
    if reversed > 0 {
        info.warnings
            .push(format!("{reversed} reversed clip(s) are skipped."));
    }
    if missing > 0 {
        info.warnings.push(format!(
            "{missing} clip(s) use video files that were not found (moved or deleted)."
        ));
    }
    info
}

pub fn inspect(project: &Path) -> Result<Info, String> {
    let project = check_project_dir(project)?;
    let d: Value = serde_json::from_slice(&read_bounded(&project.join("draft_content.json"))?)
        .map_err(|_| "The project file is not readable.")?;
    let mut info = analyze(&d, &|p| Path::new(p).is_file());
    info.capcut_running = capcut_running();
    Ok(info)
}

/// Join clips that are back-to-back in both the timeline and the source file (a "split" with
/// nothing removed), so a word on the cut is transcribed once and in one piece.
pub fn merge_adjacent(clips: &[Clip]) -> Vec<Clip> {
    const TOL: f64 = 0.05;
    let mut out: Vec<Clip> = Vec::new();
    for c in clips {
        if let Some(last) = out.last_mut() {
            let same = last.path == c.path
                && (last.src_start + last.src_dur - c.src_start).abs() <= TOL
                && (last.tl_start + last.tl_dur - c.tl_start).abs() <= TOL
                && (last.speed() - c.speed()).abs() < 1e-3;
            if same {
                last.tl_dur += c.tl_dur;
                last.src_dur += c.src_dur;
                last.ids.extend(c.ids.iter().cloned());
                continue;
            }
        }
        out.push(c.clone());
    }
    out
}

/// Words from a clip's source file (times in source seconds) -> timeline time.
/// Keeps only words whose middle falls inside the part of the file the clip actually uses.
pub fn map_to_timeline(tokens: &[Token], clip: &Clip) -> Vec<Token> {
    let (lo, hi) = (clip.src_start, clip.src_start + clip.src_dur);
    let speed = clip.speed().max(1e-6);
    let (t_lo, t_hi) = (clip.tl_start, clip.tl_start + clip.tl_dur);
    tokens
        .iter()
        .filter(|t| {
            let mid = (t.start + t.end) / 2.0;
            mid >= lo && mid < hi
        })
        .map(|t| Token {
            text: t.text.clone(),
            start: (clip.tl_start + (t.start - clip.src_start) / speed).clamp(t_lo, t_hi),
            end: (clip.tl_start + (t.end - clip.src_start) / speed).clamp(t_lo, t_hi),
        })
        .collect()
}

/// CapCut cannot hold overlapping segments in one track: trim each caption so it ends
/// where the next begins, and drop any that would vanish.
pub fn resolve_overlaps(mut caps: Vec<Token>) -> Vec<Token> {
    caps.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut out: Vec<Token> = Vec::with_capacity(caps.len());
    for c in caps {
        if let Some(prev) = out.last_mut() {
            if c.start < prev.end {
                prev.end = c.start;
            }
        }
        if out.last().is_some_and(|p| p.end - p.start < 0.05) {
            out.pop();
        }
        out.push(c);
    }
    out
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unique_backup(f: &Path, tag: &str) -> PathBuf {
    let name = f
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = epoch();
    let mut n = 0u32;
    loop {
        let suffix = if n == 0 {
            format!("{tag}-{stamp}")
        } else {
            format!("{tag}-{stamp}-{n}")
        };
        let p = f.with_file_name(format!("{name}.{suffix}"));
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes).map_err(|_| "Could not write the project.")?;
    fs::rename(&tmp, path).map_err(|_| {
        let _ = fs::remove_file(&tmp);
        "Could not replace the project file.".to_string()
    })
}

/// Keep the newest `KEEP_BACKUPS` copies with this tag, plus the very first one (it is usually the
/// project as it was before any captions). Only files named `<draft>.<tag>-...` are touched.
fn prune_backups(f: &Path, tag: &str) {
    let (Some(dir), Some(name)) = (f.parent(), f.file_name()) else {
        return;
    };
    let prefix = format!("{}.{tag}-", name.to_string_lossy());
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut found: Vec<(SystemTime, PathBuf)> = rd
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    found.sort();
    if found.len() > KEEP_BACKUPS + 1 {
        let drop_to = found.len() - KEEP_BACKUPS;
        for (_, p) in &found[1..drop_to] {
            let _ = fs::remove_file(p);
        }
    }
}

/// Add captions to every copy of the draft. Nothing is written unless all copies validate,
/// and if a write fails part way, the copies already written are put back.
pub fn write_captions_with(
    project: &Path,
    caps: &[Token],
    style: &Style,
    offset_s: f64,
    check_capcut: bool,
) -> Result<usize, String> {
    let project = check_project_dir(project)?;
    if check_capcut && capcut_running() {
        return Err(
            "CapCut is running. Close it completely (check the tray icon), then try again.".into(),
        );
    }
    let mut staged: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut count = 0;
    for f in draft_files(&project) {
        let mut d: Value = serde_json::from_slice(&read_bounded(&f)?)
            .map_err(|_| "The project file is not readable.")?;
        count = add_captions(&mut d, caps, style, offset_s)?;
        let problems = validate(&d);
        if let Some(p) = problems.first() {
            return Err(format!("Validation failed, nothing was written: {p}"));
        }
        staged.push((
            f,
            serde_json::to_vec(&d).map_err(|_| "Could not build the project.")?,
        ));
    }
    let mut backups: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (f, _) in &staged {
        let b = unique_backup(f, "bak");
        fs::copy(f, &b).map_err(|_| "Could not make a backup, so nothing was changed.")?;
        backups.push((f.clone(), b));
    }
    for (i, (f, bytes)) in staged.iter().enumerate() {
        if let Err(e) = write_atomic(f, bytes) {
            for (done, backup) in &backups[..i] {
                if let Ok(original) = fs::read(backup) {
                    let _ = write_atomic(done, &original);
                }
            }
            return Err(format!("{e} Nothing was changed."));
        }
    }
    for (f, _) in &staged {
        prune_backups(f, "bak");
    }
    Ok(count)
}

fn newest_backup(f: &Path) -> Option<PathBuf> {
    let prefix = format!("{}.bak-", f.file_name()?.to_string_lossy());
    fs::read_dir(f.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max()
        .map(|(_, p)| p)
}

/// Put the project back the way it was before the latest caption run.
pub fn restore_latest_backup(project: &Path) -> Result<usize, String> {
    restore_with(project, true)
}

fn restore_with(project: &Path, check_capcut: bool) -> Result<usize, String> {
    let project = check_project_dir(project)?;
    if check_capcut && capcut_running() {
        return Err("CapCut is running. Close it completely, then try again.".into());
    }
    let pairs: Vec<(PathBuf, PathBuf)> = draft_files(&project)
        .into_iter()
        .filter_map(|f| newest_backup(&f).map(|b| (f, b)))
        .collect();
    if pairs.is_empty() {
        return Err("There is no backup to restore for this project.".into());
    }
    // Read and check every backup before changing anything.
    let mut restored: Vec<(&PathBuf, Vec<u8>)> = Vec::new();
    for (f, b) in &pairs {
        let bytes = read_bounded(b)?;
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|_| "The newest backup is damaged, so nothing was restored.")?;
        restored.push((f, bytes));
    }
    for (f, _) in &pairs {
        fs::copy(f, unique_backup(f, "pre-restore"))
            .map_err(|_| "Could not save the current state first.")?;
    }
    for (f, bytes) in &restored {
        write_atomic(f, bytes)?;
    }
    for (f, _) in &pairs {
        prune_backups(f, "pre-restore");
    }
    Ok(pairs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/golden.json")).unwrap()
    }

    fn style() -> Style {
        let c = &golden()["config"];
        Style {
            si_font_path: "si.ttf".into(),
            en_font_path: "en.ttf".into(),
            si_size: c["sinhala"]["size"].as_f64().unwrap(),
            en_size: c["english"]["size"].as_f64().unwrap(),
            en_bold: c["english"]["bold"].as_bool().unwrap(),
            color: [1.0, 1.0, 1.0],
            position_y: c["position_y"].as_f64().unwrap(),
        }
    }

    /// The captions the fixture expects. This test is about editing the draft, not about grouping.
    fn caps() -> Vec<Token> {
        golden()["captions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| Token {
                text: c["text"].as_str().unwrap().into(),
                start: c["start"].as_f64().unwrap(),
                end: c["end"].as_f64().unwrap(),
            })
            .collect()
    }

    /// Replace every UUID by its order of first appearance, so random ids compare equal.
    fn normalise(v: &Value, seen: &mut Vec<String>) -> Value {
        match v {
            Value::String(s)
                if s.len() == 36 && s.matches('-').count() == 4 && Uuid::parse_str(s).is_ok() =>
            {
                let i = seen.iter().position(|x| x == s).unwrap_or_else(|| {
                    seen.push(s.clone());
                    seen.len() - 1
                });
                json!(format!("<id{i}>"))
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| normalise(x, seen)).collect()),
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, x)| (k.clone(), normalise(x, seen)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    #[test]
    fn content_matches_python() {
        let want = golden()["contents"].as_array().unwrap().clone();
        let caps = caps();
        assert_eq!(caps.len(), want.len());
        for (c, w) in caps.iter().zip(want) {
            let got: Value = serde_json::from_str(&build_content(&c.text, &style())).unwrap();
            let exp: Value = serde_json::from_str(w.as_str().unwrap()).unwrap();
            assert_eq!(got, exp, "content differs for {:?}", c.text);
        }
    }

    #[test]
    fn draft_edit_matches_python_including_rerun() {
        let g = golden();
        let mut d = g["draft_in"].clone();
        assert_eq!(add_captions(&mut d, &caps(), &style(), 0.0).unwrap(), 4);
        assert_eq!(
            normalise(&d, &mut Vec::new()),
            normalise(&g["draft_after_1"], &mut Vec::new())
        );
        assert!(validate(&d).is_empty());
        add_captions(&mut d, &caps(), &style(), 2.5).unwrap(); // re-run replaces, never stacks
        assert_eq!(
            normalise(&d, &mut Vec::new()),
            normalise(&g["draft_after_2"], &mut Vec::new())
        );
        assert_eq!(d["tracks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn fresh_project_without_materials_keys_works() {
        let mut d = json!({"tracks": []});
        assert_eq!(add_captions(&mut d, &caps(), &style(), 0.0).unwrap(), 4);
        assert!(validate(&d).is_empty());
    }

    #[test]
    fn validate_catches_broken_drafts() {
        let mut d = golden()["draft_in"].clone();
        add_captions(&mut d, &caps(), &style(), 0.0).unwrap();
        d["materials"]["texts"] = json!([]);
        assert!(!validate(&d).is_empty());
    }

    fn video(id: &str, path: &str, has_audio: bool) -> Value {
        json!({"id": id, "type": "video", "path": path, "has_audio": has_audio})
    }
    fn seg(id: &str, mat: &str, tl: (f64, f64), src: (f64, f64), vol: f64, reverse: bool) -> Value {
        let us = |x: f64| (x * 1e6) as i64;
        json!({"id": id, "material_id": mat, "volume": vol, "reverse": reverse,
               "target_timerange": {"start": us(tl.0), "duration": us(tl.1)},
               "source_timerange": {"start": us(src.0), "duration": us(src.1)}})
    }
    /// Main track with cuts, a fast clip, a quiet overlay, a photo, a reversed, a silent and a missing file.
    fn busy_draft() -> Value {
        json!({"duration": 12_000_000, "tracks": [
            {"type": "video", "segments": [
                seg("a1", "A", (0.0, 2.0), (0.0, 2.0), 1.0, false),
                seg("a2", "A", (2.0, 2.0), (2.0, 2.0), 1.0, false),
                seg("b1", "B", (4.0, 1.0), (10.0, 2.0), 1.0, false),
                seg("rev", "A", (5.0, 1.0), (0.0, 1.0), 1.0, true),
                seg("gone", "MISSING", (6.0, 1.0), (0.0, 1.0), 1.0, false),
                seg("pic", "P", (7.0, 1.0), (0.0, 1.0), 1.0, false)]},
            {"type": "video", "segments": [
                seg("leak", "L", (1.0, 1.0), (0.0, 1.0), 0.2, false),
                seg("silent", "S", (8.0, 1.0), (0.0, 1.0), 1.0, false)]},
            {"type": "text", "segments": []}],
          "materials": {"videos": [video("A", "C:/v/a.mov", true), video("B", "C:/v/b.mov", true), video("L", "C:/v/leak.mp4", true),
                                   video("MISSING", "C:/v/missing.mov", true), video("S", "C:/v/silent.mp4", false),
                                   json!({"id": "P", "type": "photo", "path": "C:/v/p.png"})]}})
    }
    fn exists(p: &str) -> bool {
        !p.contains("missing")
    }

    #[test]
    fn analyze_finds_clips_with_sound_and_skips_the_rest() {
        let info = analyze(&busy_draft(), &exists);
        let ids: Vec<&str> = info.clips.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a1", "leak", "a2", "b1", "gone"]); // sorted by timeline; photo, silent, reversed skipped
        let sel: Vec<&str> = info
            .clips
            .iter()
            .filter(|c| c.default_selected())
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(sel, ["a1", "a2", "b1"]); // quiet overlay and missing file are unchecked
        assert!(info.warnings.iter().any(|w| w.contains("reversed")));
        assert!(info.warnings.iter().any(|w| w.contains("not found")));
        assert!(info.problems.is_empty());
        assert!((info.clips[3].speed() - 2.0).abs() < 1e-9);
        assert_eq!(info.clips[0].name, "a.mov");
    }

    #[test]
    fn analyze_reports_when_nothing_has_sound() {
        let info = analyze(
            &json!({"duration": 0, "tracks": [], "materials": {}}),
            &exists,
        );
        assert_eq!(info.problems.len(), 1);
    }

    #[test]
    fn merge_joins_only_back_to_back_cuts_of_the_same_file() {
        let info = analyze(&busy_draft(), &exists);
        let chosen: Vec<Clip> = info
            .clips
            .iter()
            .filter(|c| c.default_selected())
            .cloned()
            .collect();
        let merged = merge_adjacent(&chosen);
        assert_eq!(merged.len(), 2); // a1+a2 joined, b1 stays
        assert_eq!(merged[0].ids, ["a1", "a2"]);
        assert!((merged[0].tl_dur - 4.0).abs() < 1e-9 && (merged[0].src_dur - 4.0).abs() < 1e-9);
        let mut gap = chosen.clone();
        gap[1].src_start += 1.0; // a gap in the source breaks the join
        assert_eq!(merge_adjacent(&gap).len(), 3);
    }

    #[test]
    fn words_are_trimmed_to_the_clip_and_follow_its_speed() {
        let info = analyze(&busy_draft(), &exists);
        let b1 = info.clips.iter().find(|c| c.id == "b1").unwrap(); // source 10..12 shown at 2x from timeline 4.0
        let t = |s: f64, e: f64| Token {
            text: "w".into(),
            start: s,
            end: e,
        };
        let out = map_to_timeline(
            &[t(9.0, 9.4), t(10.0, 10.5), t(11.0, 11.5), t(12.2, 12.6)],
            b1,
        );
        assert_eq!(out.len(), 2); // before-start and after-end words dropped
        assert!((out[0].start - 4.0).abs() < 1e-9 && (out[0].end - 4.25).abs() < 1e-9);
        assert!((out[1].start - 4.5).abs() < 1e-9);
        let a2 = info.clips.iter().find(|c| c.id == "a2").unwrap();
        let out = map_to_timeline(&[t(2.5, 3.0)], a2);
        assert!((out[0].start - 2.5).abs() < 1e-9);
    }

    #[test]
    fn overlapping_captions_are_trimmed_not_stacked() {
        let t = |s: f64, e: f64| Token {
            text: "x".into(),
            start: s,
            end: e,
        };
        let out = resolve_overlaps(vec![t(3.0, 5.0), t(0.0, 2.0), t(4.0, 6.0), t(4.01, 4.5)]);
        for w in out.windows(2) {
            assert!(
                w[0].end <= w[1].start + 1e-9,
                "overlap: {:?} {:?}",
                w[0],
                w[1]
            );
        }
        assert!((out[0].start - 0.0).abs() < 1e-9 && (out[0].end - 2.0).abs() < 1e-9);
        assert!(resolve_overlaps(vec![]).is_empty());
    }

    #[test]
    fn only_plain_local_media_paths_are_accepted() {
        for ok in ["C:/v/a.mov", "D:\\clips\\take 1.MP4", "c:/x/y/z.wav"] {
            assert!(media_path_ok(ok), "should accept {ok}");
        }
        for bad in [
            "\\\\server\\share\\a.mov",
            "//server/share/a.mov",
            "\\\\?\\C:\\a.mov",
            "C:/v/../../Windows/a.mov",
            "C:/v/malware.exe",
            "C:/v/notes.docx",
            "a.mov",
            "",
            "C:/v/a.mov\0.exe",
        ] {
            assert!(!media_path_ok(bad), "should reject {bad:?}");
        }
    }

    #[test]
    fn a_clip_pointing_at_a_network_path_is_never_checked_on_disk() {
        let mut d = busy_draft();
        d["materials"]["videos"][0]["path"] = json!("\\\\evil\\share\\a.mov");
        let touched = std::cell::Cell::new(false);
        let info = analyze(&d, &|_| {
            touched.set(true);
            true
        });
        // Clips A and B are the first two videos; the first now has a hostile path.
        let a = info.clips.iter().find(|c| c.id == "a1").unwrap();
        assert!(!a.file_ok && !a.default_selected());
        assert!(touched.get(), "other clips are still checked");
    }

    #[test]
    fn windows_device_names_are_refused_as_media_paths() {
        for bad in [
            "C:/con.mov",
            "C:/clips/NUL.mp4",
            "D:\\x\\com1.wav",
            "C:/x/lpt9.mov",
            "C:/x/aux .mov",
        ] {
            assert!(!media_path_ok(bad), "should reject {bad}");
        }
        for ok in [
            "C:/clips/console.mov",
            "C:/clips/confusion.mp4",
            "C:/x/aux1.mov",
        ] {
            assert!(media_path_ok(ok), "should accept {ok}");
        }
    }

    #[test]
    fn a_project_with_an_absurd_number_of_clips_is_refused() {
        let segs: Vec<Value> = (0..(MAX_SEGMENTS + 1))
            .map(|i| seg(&format!("s{i}"), "A", (0.0, 1.0), (0.0, 1.0), 1.0, false))
            .collect();
        let d = json!({"duration": 1_000_000, "tracks": [{"type": "video", "segments": segs}],
                       "materials": {"videos": [video("A", "C:/v/a.mov", true)]}});
        let info = analyze(&d, &exists);
        assert!(info.clips.is_empty());
        assert!(info.problems[0].contains("too many clips"));
    }

    fn temp_project(tag: &str) -> PathBuf {
        let p = std::env::temp_dir()
            .join(format!("scs-draft-{tag}-{}", std::process::id()))
            .join(DRAFT_DIR_NAME)
            .join("proj");
        let _ = fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn set_age(f: &Path, secs_ago: u64) {
        let t = SystemTime::now() - std::time::Duration::from_secs(secs_ago);
        fs::OpenOptions::new()
            .write(true)
            .open(f)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    #[test]
    fn old_backups_are_pruned_but_the_first_and_the_newest_are_kept() {
        let proj = temp_project("prune");
        let draft = proj.join("draft_content.json");
        fs::write(&draft, "{}").unwrap();
        for i in 0..15u64 {
            let b = proj.join(format!("draft_content.json.bak-{i}"));
            fs::write(&b, "{}").unwrap();
            set_age(&b, 1000 - i * 10); // bak-0 is the oldest
        }
        fs::write(proj.join("notes.txt"), "keep me").unwrap();
        prune_backups(&draft, "bak");
        let left: Vec<String> = fs::read_dir(&proj)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".bak-"))
            .collect();
        assert_eq!(left.len(), KEEP_BACKUPS + 1, "{left:?}");
        assert!(
            left.contains(&"draft_content.json.bak-0".to_string()),
            "the first backup must stay"
        );
        assert!(
            left.contains(&"draft_content.json.bak-14".to_string()),
            "the newest must stay"
        );
        assert!(!left.contains(&"draft_content.json.bak-1".to_string()));
        assert!(
            proj.join("notes.txt").exists(),
            "unrelated files are never touched"
        );
        let _ = fs::remove_dir_all(proj.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn restore_puts_back_the_backup_and_refuses_a_damaged_one() {
        let proj = temp_project("restore");
        let draft = proj.join("draft_content.json");
        fs::write(&draft, r#"{"state":"captioned"}"#).unwrap();
        fs::write(
            proj.join("draft_content.json.bak-1"),
            r#"{"state":"original"}"#,
        )
        .unwrap();
        assert_eq!(restore_with(&proj, false).unwrap(), 1);
        assert_eq!(
            fs::read_to_string(&draft).unwrap(),
            r#"{"state":"original"}"#
        );
        // The state before the restore is itself saved, under a name restore does not pick up.
        assert!(fs::read_dir(&proj)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains(".pre-restore-")));

        // A damaged newest backup stops the restore and leaves the project untouched.
        fs::write(&draft, r#"{"state":"captioned again"}"#).unwrap();
        let broken = proj.join("draft_content.json.bak-2");
        fs::write(&broken, "{ this is not json").unwrap();
        set_age(&proj.join("draft_content.json.bak-1"), 500);
        let err = restore_with(&proj, false).unwrap_err();
        assert!(err.contains("damaged"), "{err}");
        assert_eq!(
            fs::read_to_string(&draft).unwrap(),
            r#"{"state":"captioned again"}"#
        );
        let _ = fs::remove_dir_all(proj.parent().unwrap().parent().unwrap());
    }
}
