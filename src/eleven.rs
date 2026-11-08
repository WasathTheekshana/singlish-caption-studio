//! ElevenLabs speech-to-text client. The only network code in the app: one fixed HTTPS
//! address, a short timeout, size-limited responses, and the key goes only in a header.

use std::{io::Read, time::Duration};

use serde_json::Value;
use uuid::Uuid;

use crate::text::Token;

const URL: &str = "https://api.elevenlabs.io/v1/speech-to-text";
pub const MODEL: &str = "scribe_v2";
const MAX_RESPONSE_BYTES: u64 = 20_000_000;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(600))
        .timeout_connect(Duration::from_secs(15))
        .https_only(true)
        .redirects(0)
        .try_proxy_from_env(true)
        .build()
}

struct Form {
    boundary: String,
    body: Vec<u8>,
}

impl Form {
    fn new() -> Self {
        Self {
            boundary: Uuid::new_v4().simple().to_string(),
            body: Vec::new(),
        }
    }
    fn field(&mut self, name: &str, value: &str) {
        self.body.extend(
            format!(
                "--{}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n",
                self.boundary
            )
            .as_bytes(),
        );
    }
    fn file(&mut self, name: &str, filename: &str, mime: &str, data: &[u8]) {
        self.body.extend(
            format!(
                "--{}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n",
                self.boundary
            )
            .as_bytes(),
        );
        self.body.extend(data);
        self.body.extend(b"\r\n");
    }
    fn finish(mut self) -> (Vec<u8>, String) {
        self.body
            .extend(format!("--{}--\r\n", self.boundary).as_bytes());
        let ctype = format!("multipart/form-data; boundary={}", self.boundary);
        (self.body, ctype)
    }
}

fn read_body(resp: ureq::Response) -> String {
    let mut s = String::new();
    let _ = resp
        .into_reader()
        .take(MAX_RESPONSE_BYTES)
        .read_to_string(&mut s);
    s
}

/// Turn an HTTP failure into a message a person can act on. Never includes the key.
fn explain(code: u16, body: &str) -> String {
    let detail: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let kind = detail["detail"]["type"].as_str().unwrap_or("");
    let status = detail["detail"]["status"].as_str().unwrap_or("");
    match (code, kind, status) {
        (_, "authentication_error", "invalid_api_key") => "ElevenLabs does not accept this key. Check that you copied all of it.".into(),
        (401 | 403, _, _) => "This key is not allowed to transcribe speech. Give it the Speech to Text permission.".into(),
        (429, _, _) => "ElevenLabs says you are sending too many requests or are out of credits. Try again later.".into(),
        (402, _, _) => "Your ElevenLabs account is out of credits.".into(),
        (500..=599, _, _) => "ElevenLabs is having problems right now. Try again in a few minutes.".into(),
        _ => format!("ElevenLabs rejected the request (error {code})."),
    }
}

/// Free key test: send an incomplete request. A valid key is told the request is
/// incomplete (validation_error); a bad key is told it is invalid. No credits are used.
pub fn check_key(key: &str) -> Result<(), String> {
    let mut form = Form::new();
    form.field("model_id", MODEL);
    let (body, ctype) = form.finish();
    match agent()
        .post(URL)
        .set("xi-api-key", key)
        .set("Content-Type", &ctype)
        .send_bytes(&body)
    {
        // An incomplete request must be refused. Any success here means we are not talking to
        // the real service, so the key is not accepted.
        Ok(_) => {
            Err("ElevenLabs sent an unexpected reply, so the key could not be checked.".into())
        }
        Err(ureq::Error::Status(code, resp)) => {
            let text = read_body(resp);
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if v["detail"]["type"] == "validation_error" {
                Ok(())
            } else {
                Err(explain(code, &text))
            }
        }
        Err(_) => Err("Could not reach ElevenLabs. Check your internet connection.".into()),
    }
}

/// What ElevenLabs sent back for one request.
#[derive(Debug, PartialEq)]
pub struct Transcript {
    /// One token per spoken word.
    pub words: Vec<Token>,
    /// The audio length ElevenLabs says it processed, when it reports one.
    pub audio_s: Option<f64>,
}

/// Transcribe a 16 kHz mono WAV.
pub fn transcribe(key: &str, wav: &[u8], language: Option<&str>) -> Result<Transcript, String> {
    let mut form = Form::new();
    form.field("model_id", MODEL);
    form.field("timestamps_granularity", "word");
    form.field("tag_audio_events", "false");
    if let Some(l) = language {
        form.field("language_code", l);
    }
    form.file("file", "audio.wav", "audio/wav", wav);
    let (body, ctype) = form.finish();
    match agent()
        .post(URL)
        .set("xi-api-key", key)
        .set("Content-Type", &ctype)
        .send_bytes(&body)
    {
        Ok(resp) if resp.status() == 200 => parse_response(&read_body(resp)),
        Ok(_) => Err("ElevenLabs sent an unexpected reply.".into()),
        Err(ureq::Error::Status(code, resp)) => Err(explain(code, &read_body(resp))),
        Err(_) => Err("Could not reach ElevenLabs. Check your internet connection.".into()),
    }
}

pub fn parse_response(body: &str) -> Result<Transcript, String> {
    let v: Value = serde_json::from_str(body)
        .map_err(|_| "ElevenLabs sent a reply that could not be read.")?;
    let words = v["words"]
        .as_array()
        .ok_or("ElevenLabs did not return word timings.")?;
    let words = words
        .iter()
        .filter(|w| w.get("type").is_none_or(|t| t == "word"))
        .filter_map(|w| {
            let (s, e) = (w["start"].as_f64()?, w["end"].as_f64()?);
            let text = w["text"].as_str()?.trim().to_string();
            (s.is_finite() && e.is_finite() && e >= s).then_some(Token {
                text,
                start: s,
                end: e,
            })
        })
        .filter(|t| !t.text.is_empty())
        .collect();
    let audio_s = v["audio_duration_secs"]
        .as_f64()
        .filter(|s| s.is_finite() && *s >= 0.0);
    Ok(Transcript { words, audio_s })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_words_and_skips_spacing_and_junk() {
        let body = r#"{"words":[
            {"text":"ඔයාට","start":3.06,"end":3.26,"type":"word"},
            {"text":" ","start":3.26,"end":3.28,"type":"spacing"},
            {"text":"(laughs)","start":3.3,"end":3.5,"type":"audio_event"},
            {"text":"bad","start":2.0,"end":1.0,"type":"word"},
            {"text":"නැතත්","start":4.46,"end":4.7}]}"#;
        let t = parse_response(body).unwrap();
        assert_eq!(
            t.words.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(),
            ["ඔයාට", "නැතත්"]
        );
        assert_eq!(t.audio_s, None);
        assert!(parse_response("not json").is_err());
        assert!(parse_response("{}").is_err());
    }

    #[test]
    fn reads_the_audio_duration_when_elevenlabs_reports_it() {
        let ok = parse_response(r#"{"words":[],"audio_duration_secs":8.937}"#).unwrap();
        assert_eq!(ok.audio_s, Some(8.937));
        let junk = parse_response(r#"{"words":[],"audio_duration_secs":-3}"#).unwrap();
        assert_eq!(junk.audio_s, None);
    }

    #[test]
    fn error_messages_never_echo_the_response_and_are_helpful() {
        let invalid = r#"{"detail":{"type":"authentication_error","status":"invalid_api_key","message":"secret stuff"}}"#;
        let m = explain(400, invalid);
        assert!(m.contains("does not accept this key") && !m.contains("secret"));
        assert!(explain(429, "").contains("credits"));
        assert!(explain(503, "").contains("problems"));
        assert!(explain(403, "{}").contains("Speech to Text"));
    }

    /// Live, free check against the real API. Run with:
    ///   cargo test live_ -- --ignored   (needs ELEVENLABS_API_KEY in the environment)
    #[test]
    #[ignore = "network: needs a real key"]
    fn live_check_key_accepts_real_and_rejects_fake() {
        let Ok(key) = std::env::var("ELEVENLABS_API_KEY") else {
            return;
        };
        assert!(check_key(&key).is_ok(), "a real key should be accepted");
        let fake = format!("sk_{}", "0".repeat(48));
        let err = check_key(&fake).unwrap_err();
        assert!(
            err.contains("does not accept this key"),
            "unexpected message: {err}"
        );
        assert!(!err.contains(&key));
    }

    #[test]
    fn multipart_body_is_well_formed() {
        let mut f = Form::new();
        f.field("a", "1");
        f.file("file", "x.wav", "audio/wav", b"DATA");
        let b = f.boundary.clone();
        let (body, ctype) = f.finish();
        let s = String::from_utf8_lossy(&body);
        assert!(ctype.ends_with(&b));
        assert!(s.starts_with(&format!("--{b}\r\n")) && s.ends_with(&format!("--{b}--\r\n")));
        assert!(s.contains("name=\"a\"\r\n\r\n1\r\n") && s.contains("DATA"));
    }
}
