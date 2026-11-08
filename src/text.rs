//! Sinhala/English text logic: split into language runs, restore English words from a
//! glossary, and group words into caption lines. `tests/fixtures/golden.json` pins the
//! expected results.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const PUNCT: &[char] = &[
    '.', ',', '?', '!', ';', ':', '"', '\'', '(', ')', '[', ']', '\u{2026}', '\u{2013}',
    '\u{2014}', '-',
];

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Si,
    En,
}

impl Kind {
    #[cfg(test)]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Si => "si",
            Kind::En => "en",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum K {
    Si,
    En,
    Neutral,
}

fn char_kind(c: char) -> K {
    let o = c as u32;
    if (0x0D80..=0x0DFF).contains(&o) || o == 0x200C || o == 0x200D {
        K::Si
    } else if c.is_alphanumeric() {
        if o < 0x250 {
            K::En
        } else {
            K::Si
        }
    } else {
        K::Neutral
    }
}

/// Split into Sinhala/English runs. A space or punctuation joins an English run only
/// when English letters sit on both sides (the space in "credit card").
pub fn split_runs(text: &str) -> Vec<(Kind, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut ks: Vec<K> = chars.iter().map(|c| char_kind(*c)).collect();
    let n = ks.len();
    let mut i = 0;
    while i < n {
        if ks[i] != K::Neutral {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < n && ks[j] == K::Neutral {
            j += 1;
        }
        let en_both = i > 0 && j < n && ks[i - 1] == K::En && ks[j] == K::En;
        let fill = if en_both { K::En } else { K::Si };
        ks[i..j].iter_mut().for_each(|k| *k = fill);
        i = j;
    }
    let mut runs: Vec<(Kind, String)> = Vec::new();
    for (c, k) in chars.iter().zip(&ks) {
        let kind = if *k == K::En { Kind::En } else { Kind::Si };
        match runs.last_mut() {
            Some((last, s)) if *last == kind => s.push(*c),
            _ => runs.push((kind, c.to_string())),
        }
    }
    runs
}

/// Remove zero-width joiners and collapse whitespace so glossary keys match.
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(*c as u32, 0x200B..=0x200D))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GlossaryData {
    /// Sinhala-script spelling of an English word -> the English word ("වීඩියෝ" -> "Video").
    #[serde(default)]
    pub english: BTreeMap<String, String>,
    /// Preferred spelling fixes for Sinhala words ("තවම" -> "තාම").
    #[serde(default)]
    pub spelling: BTreeMap<String, String>,
}

pub struct Glossary {
    english: Vec<(String, String)>,
    spelling: Vec<(String, String)>,
    max_len: usize,
}

fn split_punct(t: &str) -> (&str, &str, &str) {
    let core = t.trim_matches(PUNCT);
    if core.is_empty() {
        return ("", t, "");
    }
    match t.find(core) {
        Some(a) => (&t[..a], core, &t[a + core.len()..]),
        None => ("", t, ""),
    }
}

impl Glossary {
    pub fn new(data: &GlossaryData) -> Self {
        let english: Vec<(String, String)> = data
            .english
            .iter()
            .map(|(k, v)| (norm(k), v.clone()))
            .collect();
        let spelling: Vec<(String, String)> = data
            .spelling
            .iter()
            .map(|(k, v)| (norm(k), v.clone()))
            .collect();
        let max_len = english
            .iter()
            .map(|(k, _)| k.split(' ').count())
            .max()
            .unwrap_or(1);
        Self {
            english,
            spelling,
            max_len,
        }
    }

    fn lookup<'a>(map: &'a [(String, String)], key: &str) -> Option<&'a str> {
        map.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn restore(&self, tokens: &[Token]) -> Vec<Token> {
        let mut out = Vec::with_capacity(tokens.len());
        let mut i = 0;
        while i < tokens.len() {
            let mut hit = false;
            for n in (1..=self.max_len.min(tokens.len() - i)).rev() {
                let parts = &tokens[i..i + n];
                let (lead, _, _) = split_punct(&parts[0].text);
                let (_, _, trail) = split_punct(&parts[n - 1].text);
                let key = norm(
                    &parts
                        .iter()
                        .map(|p| split_punct(&p.text).1)
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                if let Some(en) = Self::lookup(&self.english, &key) {
                    out.push(Token {
                        text: format!("{lead}{en}{trail}"),
                        start: parts[0].start,
                        end: parts[n - 1].end,
                    });
                    i += n;
                    hit = true;
                    break;
                }
            }
            if hit {
                continue;
            }
            let t = &tokens[i];
            let (lead, core, trail) = split_punct(&t.text);
            match Self::lookup(&self.spelling, &norm(core)) {
                Some(fix) => out.push(Token {
                    text: format!("{lead}{fix}{trail}"),
                    start: t.start,
                    end: t.end,
                }),
                None => out.push(t.clone()),
            }
            i += 1;
        }
        out
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Grouping {
    pub max_chars: usize,
    pub max_words: usize,
    pub max_gap_s: f64,
    pub max_duration_s: f64,
}

impl Default for Grouping {
    fn default() -> Self {
        Self {
            max_chars: 26,
            max_words: 6,
            max_gap_s: 0.6,
            max_duration_s: 3.5,
        }
    }
}

pub fn group_tokens(tokens: &[Token], g: &Grouping) -> Vec<Token> {
    let mut groups: Vec<Vec<&Token>> = Vec::new();
    let mut cur: Vec<&Token> = Vec::new();
    for t in tokens {
        if let Some(last) = cur.last() {
            let len = cur
                .iter()
                .map(|x| x.text.as_str())
                .chain([t.text.as_str()])
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .count();
            let ends_sentence = last
                .text
                .trim_end()
                .chars()
                .last()
                .is_some_and(|c| matches!(c, '.' | '?' | '!'));
            if t.start - last.end > g.max_gap_s
                || len > g.max_chars
                || cur.len() >= g.max_words
                || t.end - cur[0].start > g.max_duration_s
                || ends_sentence
            {
                groups.push(std::mem::take(&mut cur));
            }
        }
        cur.push(t);
    }
    if !cur.is_empty() {
        groups.push(cur);
    }
    let mut out: Vec<Token> = groups
        .iter()
        .map(|c| Token {
            text: c
                .iter()
                .map(|x| x.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            start: c[0].start,
            end: c[c.len() - 1].end,
        })
        .collect();
    for i in 1..out.len() {
        let gap = out[i].start - out[i - 1].end;
        if gap > 0.0 && gap < 0.3 {
            out[i - 1].end = out[i].start;
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::Value;

    fn golden() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/golden.json")).unwrap()
    }

    pub fn golden_glossary() -> GlossaryData {
        serde_json::from_value(golden()["glossary"].clone()).unwrap()
    }

    #[test]
    fn runs_match_python() {
        for case in golden()["runs"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let want: Vec<(String, String)> = case["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    (
                        r[0].as_str().unwrap().to_string(),
                        r[1].as_str().unwrap().to_string(),
                    )
                })
                .collect();
            let got: Vec<(String, String)> = split_runs(text)
                .into_iter()
                .map(|(k, s)| (k.as_str().to_string(), s))
                .collect();
            assert_eq!(got, want, "runs differ for {text:?}");
        }
    }

    #[test]
    fn captions_match_python() {
        let g = golden();
        let tokens: Vec<Token> = g["transcript_words"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w.get("type").is_none_or(|t| t == "word"))
            .map(|w| Token {
                text: w["text"].as_str().unwrap().into(),
                start: w["start"].as_f64().unwrap(),
                end: w["end"].as_f64().unwrap(),
            })
            .collect();
        let restored = Glossary::new(&golden_glossary()).restore(&tokens);
        let caps = group_tokens(&restored, &Grouping::default());
        let want = g["captions"].as_array().unwrap();
        assert_eq!(caps.len(), want.len());
        for (c, w) in caps.iter().zip(want) {
            assert_eq!(c.text, w["text"].as_str().unwrap());
            assert!((c.start - w["start"].as_f64().unwrap()).abs() < 1e-9);
            assert!((c.end - w["end"].as_f64().unwrap()).abs() < 1e-9);
        }
    }

    #[test]
    fn punctuation_kept_around_restored_words() {
        let gl = Glossary::new(&golden_glossary());
        let out = gl.restore(&[Token {
            text: "වීඩියෝ?".into(),
            start: 0.0,
            end: 1.0,
        }]);
        assert_eq!(out[0].text, "Video?");
    }

    #[test]
    fn empty_input_is_fine() {
        assert!(split_runs("").is_empty());
        assert!(group_tokens(&[], &Grouping::default()).is_empty());
        assert!(Glossary::new(&GlossaryData::default())
            .restore(&[])
            .is_empty());
    }
}
