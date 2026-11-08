//! A small local log of what each caption run sent to ElevenLabs.
//!
//! ElevenLabs bills speech-to-text by the amount of audio, not by tokens, so this records
//! audio seconds, requests, words and cache hits. It never stores audio, text or the key.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::store;

const MAX_RUNS: usize = 200;
const MAX_FILE_BYTES: u64 = 4_000_000;
const FILE: &str = "usage.json";

/// One request to ElevenLabs (or one answer read from the local cache).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestUsage {
    /// Audio length of the request, in seconds.
    pub audio_s: f64,
    /// True when the answer came from the cache, which costs nothing.
    pub cached: bool,
    pub words: usize,
    pub chars: usize,
}

/// One clip on the timeline and the request that covered it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipUsage {
    pub name: String,
    /// Where the clip starts on the timeline, in seconds.
    pub start: f64,
    /// How much of the source file the clip uses, in seconds.
    pub audio_s: f64,
    pub words: usize,
    /// 1-based position in the run's request list.
    pub request: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Run {
    /// Seconds since the Unix epoch.
    pub when: u64,
    /// The CapCut project folder name.
    pub project: String,
    pub requests: Vec<RequestUsage>,
    pub clips: Vec<ClipUsage>,
}

impl Run {
    /// Seconds of audio that were actually sent to ElevenLabs in this run.
    pub fn billed_seconds(&self) -> f64 {
        self.requests
            .iter()
            .filter(|r| !r.cached)
            .map(|r| r.audio_s)
            .sum()
    }
    pub fn billed_requests(&self) -> usize {
        self.requests.iter().filter(|r| !r.cached).count()
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Ledger {
    pub runs: Vec<Run>,
}

impl Ledger {
    /// Keep only the newest runs so the file cannot grow forever.
    fn trim(&mut self) {
        let extra = self.runs.len().saturating_sub(MAX_RUNS);
        self.runs.drain(..extra);
    }

    /// The shape the UI shows: all-time totals, then one entry per project with its latest run.
    pub fn summary(&self) -> Value {
        let mut projects: Vec<Value> = Vec::new();
        let mut names: Vec<&str> = Vec::new();
        for r in self.runs.iter().rev() {
            if !names.contains(&r.project.as_str()) {
                names.push(&r.project);
            }
        }
        for name in names {
            let runs: Vec<&Run> = self.runs.iter().filter(|r| r.project == name).collect();
            let Some(last) = runs.last() else { continue };
            projects.push(json!({
                "project": name,
                "runs": runs.len(),
                "billed_seconds": runs.iter().map(|r| r.billed_seconds()).sum::<f64>(),
                "billed_requests": runs.iter().map(|r| r.billed_requests()).sum::<usize>(),
                "last": {"when": last.when, "requests": last.requests, "clips": last.clips},
            }));
        }
        json!({
            "runs": self.runs.len(),
            "billed_seconds": self.runs.iter().map(Run::billed_seconds).sum::<f64>(),
            "billed_requests": self.runs.iter().map(Run::billed_requests).sum::<usize>(),
            "cached_requests": self.runs.iter().flat_map(|r| &r.requests).filter(|q| q.cached).count(),
            "projects": projects,
        })
    }
}

fn path() -> Result<std::path::PathBuf, String> {
    Ok(store::data_dir()?.join(FILE))
}

fn load_from(p: &Path) -> Ledger {
    if fs::metadata(p)
        .map(|m| m.len() > MAX_FILE_BYTES)
        .unwrap_or(true)
    {
        return Ledger::default();
    }
    fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn load() -> Ledger {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

fn record_to(p: &Path, run: Run) -> Result<(), String> {
    let mut ledger = load_from(p);
    ledger.runs.push(run);
    ledger.trim();
    let body = serde_json::to_string(&ledger).map_err(|_| "Could not save the usage log.")?;
    store::write_atomic(p, &body)
}

pub fn record(run: Run) -> Result<(), String> {
    record_to(&path()?, run)
}

pub fn clear() -> Result<(), String> {
    match fs::remove_file(path()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Could not clear the usage log.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(project: &str, when: u64, requests: &[(f64, bool)]) -> Run {
        Run {
            when,
            project: project.into(),
            requests: requests
                .iter()
                .map(|(s, c)| RequestUsage {
                    audio_s: *s,
                    cached: *c,
                    words: 3,
                    chars: 12,
                })
                .collect(),
            clips: vec![ClipUsage {
                name: "a.mov".into(),
                start: 0.0,
                audio_s: 2.0,
                words: 3,
                request: 1,
            }],
        }
    }

    #[test]
    fn cached_requests_are_not_counted_as_billed() {
        let r = run("p", 1, &[(10.0, false), (4.0, true), (5.5, false)]);
        assert_eq!(r.billed_requests(), 2);
        assert!((r.billed_seconds() - 15.5).abs() < 1e-9);
    }

    #[test]
    fn summary_groups_by_project_newest_first_with_totals() {
        let l = Ledger {
            runs: vec![
                run("a", 1, &[(10.0, false)]),
                run("b", 2, &[(3.0, true)]),
                run("a", 3, &[(5.0, false)]),
            ],
        };
        let s = l.summary();
        assert_eq!(s["runs"], 3);
        assert_eq!(s["billed_requests"], 2);
        assert!((s["billed_seconds"].as_f64().unwrap() - 15.0).abs() < 1e-9);
        assert_eq!(s["cached_requests"], 1);
        let projects = s["projects"].as_array().unwrap();
        assert_eq!(projects[0]["project"], "a"); // most recently used first
        assert_eq!(projects[0]["runs"], 2);
        assert!((projects[0]["billed_seconds"].as_f64().unwrap() - 15.0).abs() < 1e-9);
        assert_eq!(projects[0]["last"]["when"], 3);
        assert_eq!(projects[1]["project"], "b");
    }

    #[test]
    fn the_log_is_capped_and_survives_a_round_trip() {
        let p = std::env::temp_dir().join(format!("scs-usage-{}.json", std::process::id()));
        let _ = fs::remove_file(&p);
        for i in 0..(MAX_RUNS as u64 + 5) {
            record_to(&p, run("p", i, &[(1.0, false)])).unwrap();
        }
        let l = load_from(&p);
        assert_eq!(l.runs.len(), MAX_RUNS);
        assert_eq!(l.runs.last().unwrap().when, MAX_RUNS as u64 + 4);
        assert_eq!(l.runs.first().unwrap().when, 5);
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_missing_or_damaged_log_reads_as_empty() {
        assert!(load_from(Path::new("Z:/nope/usage.json")).runs.is_empty());
        let p = std::env::temp_dir().join(format!("scs-usage-bad-{}.json", std::process::id()));
        fs::write(&p, "{ not json").unwrap();
        assert!(load_from(&p).runs.is_empty());
        let _ = fs::remove_file(p);
    }
}
