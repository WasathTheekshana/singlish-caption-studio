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

impl Grouping {
    /// The limits for a "most words in one caption" setting. The other limits grow with it, so the
    /// word count is what decides where captions end, and the character and time limits are only
    /// a safety net against very long words or very slow speech.
    pub fn with_max_words(words: usize) -> Self {
        let words = words.max(1);
        Self {
            max_chars: (words * 9).max(18),
            max_words: words,
            max_gap_s: 0.6,
            max_duration_s: (words as f64 * 0.6).max(3.5),
        }
    }
}

/// How much a break between two words sounds like a natural stop: 0 when the speaker runs straight
/// on, 1 for a clear pause or a comma.
fn pause_strength(before: &Token, after: &Token) -> f64 {
    let punctuated = before
        .text
        .trim_end()
        .chars()
        .last()
        .is_some_and(|c| matches!(c, ',' | ';' | ':' | ')' | '\u{2013}' | '\u{2014}'));
    if punctuated {
        return 1.0;
    }
    ((after.start - before.end) / 0.35).clamp(0.0, 1.0)
}

fn is_english_word(t: &Token) -> bool {
    t.text.chars().any(|c| c.is_ascii_alphabetic())
        && t.text
            .chars()
            .all(|c| c.is_ascii() && !c.is_ascii_control())
}

/// Sinhala words that belong to the word before them: "video එක", "phone එකක්", "ඒ වගේ". A caption
/// should not start with one, so these are never cut off from what they follow.
const ATTACHED: &[&str] = &[
    "එක",
    "එකක්",
    "එකට",
    "එකේ",
    "එකෙන්",
    "එකත්",
    "වගේ",
    "වල",
    "වලට",
    "වලින්",
    "ද",
    "නම්",
    "කියලා",
    "කියල",
];

fn is_attached(t: &Token) -> bool {
    let bare = t
        .text
        .trim_matches(|c: char| matches!(c, ',' | '.' | '?' | '!' | ';' | ':'));
    ATTACHED.contains(&bare)
}

fn ends_sentence(t: &Token) -> bool {
    t.text
        .trim_end()
        .chars()
        .last()
        .is_some_and(|c| matches!(c, '.' | '?' | '!'))
}

// Costs for choosing where to cut. Each extra caption costs `PER_CAPTION`, which is larger than any
// other term, so the fewest captions win first. Among those, `CUT` pushes breaks to pauses, `BALANCE`
// keeps the pieces even, `ENGLISH_GLUE` keeps an English phrase like "new phone" together, and
// `ATTACHED_GLUE` keeps a word like "එක" with the word it belongs to.
const PER_CAPTION: f64 = 2.0;
const CUT: f64 = 1.5;
const BALANCE: f64 = 2.0;
const ENGLISH_GLUE: f64 = 0.6;
const ATTACHED_GLUE: f64 = 2.5;

/// Split one stretch of continuous speech into captions that each respect the limits, choosing the
/// cut points over the whole stretch instead of cutting wherever the limit happens to be reached.
fn split_phrase<'a>(words: &'a [Token], g: &Grouping) -> Vec<&'a [Token]> {
    let n = words.len();
    let max_words = g.max_words.max(1);
    let lens: Vec<usize> = words.iter().map(|w| w.text.chars().count()).collect();
    let fits = |from: usize, to: usize| {
        if to - from == 1 {
            return true; // a single word always gets its own caption, however long
        }
        let chars: usize = lens[from..to].iter().sum::<usize>() + (to - from - 1);
        to - from <= max_words
            && chars <= g.max_chars
            && words[to - 1].end - words[from].start <= g.max_duration_s
    };
    // How many captions the limits force, counted by filling each one as far as it goes. The pieces
    // are then balanced around n / that count.
    let (mut needed, mut start) = (0, 0);
    while start < n {
        let mut end = start + 1;
        while end < n && fits(start, end + 1) {
            end += 1;
        }
        needed += 1;
        start = end;
    }
    let ideal = n as f64 / needed as f64;
    let cut_cost = |at: usize| {
        if at == n {
            return 0.0;
        }
        let strength = pause_strength(&words[at - 1], &words[at]);
        let mut glue = 0.0;
        if is_english_word(&words[at - 1]) && is_english_word(&words[at]) {
            glue += ENGLISH_GLUE;
        }
        if is_attached(&words[at]) {
            glue += ATTACHED_GLUE;
        }
        (CUT + glue) * (1.0 - strength)
    };

    // best[i] is the cheapest way to caption the first i words; from[i] is where its last caption starts.
    let mut best = vec![f64::INFINITY; n + 1];
    let mut from = vec![0usize; n + 1];
    best[0] = 0.0;
    for to in 1..=n {
        for start in (0..to).rev() {
            if !fits(start, to) {
                break; // starting earlier only makes it longer
            }
            let size = (to - start) as f64;
            let off = (size - ideal) / max_words as f64;
            let cost = best[start] + PER_CAPTION + BALANCE * off * off + cut_cost(to);
            if cost < best[to] {
                best[to] = cost;
                from[to] = start;
            }
        }
    }
    let mut cuts = vec![n];
    let mut at = n;
    while from[at] > 0 {
        at = from[at];
        cuts.push(at);
    }
    cuts.reverse();
    let mut out = Vec::with_capacity(cuts.len());
    let mut start = 0;
    for end in cuts {
        out.push(&words[start..end]);
        start = end;
    }
    out
}

pub fn group_tokens(tokens: &[Token], g: &Grouping) -> Vec<Token> {
    // A long pause or the end of a sentence always starts a new caption.
    let mut phrases: Vec<&[Token]> = Vec::new();
    let mut start = 0;
    for i in 1..tokens.len() {
        if tokens[i].start - tokens[i - 1].end > g.max_gap_s || ends_sentence(&tokens[i - 1]) {
            phrases.push(&tokens[start..i]);
            start = i;
        }
    }
    if start < tokens.len() {
        phrases.push(&tokens[start..]);
    }
    let mut out: Vec<Token> = phrases
        .into_iter()
        .flat_map(|p| split_phrase(p, g))
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

    fn fixture_tokens() -> Vec<Token> {
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
        Glossary::new(&golden_glossary()).restore(&tokens)
    }

    /// Words one after another, 0.3 s each with a 0.02 s gap, like the steady speech in the fixture.
    fn steady(words: &[&str]) -> Vec<Token> {
        words
            .iter()
            .enumerate()
            .map(|(i, w)| Token {
                text: (*w).into(),
                start: i as f64 * 0.32,
                end: i as f64 * 0.32 + 0.3,
            })
            .collect()
    }

    fn texts(caps: &[Token]) -> Vec<&str> {
        caps.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn a_real_transcript_splits_at_natural_points() {
        // "credit card එකක්" and "Video එක" stay whole, and no caption is left with a lone word.
        let caps = group_tokens(&fixture_tokens(), &Grouping::default());
        assert_eq!(
            texts(&caps),
            [
                "ඔයාට තාම credit card එකක්",
                "තිබ්බත් නැතත්",
                "මේ Video එක",
                "බලලම යන්න වෙනවා"
            ]
        );
        assert!((caps[0].start - 3.06).abs() < 1e-9 && (caps[3].end - 6.58).abs() < 1e-9);
    }

    #[test]
    fn a_phrase_that_fits_is_left_alone() {
        let caps = group_tokens(
            &steady(&["a", "b", "c", "d", "e"]),
            &Grouping::with_max_words(6),
        );
        assert_eq!(texts(&caps), ["a b c d e"]);
    }

    #[test]
    fn the_word_limit_is_never_exceeded_and_no_word_is_lost() {
        let words: Vec<String> = (0..120).map(|i| format!("w{i}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        for max in 2..=10 {
            let caps = group_tokens(&steady(&refs), &Grouping::with_max_words(max));
            for c in &caps {
                assert!(c.text.split(' ').count() <= max, "{max}: {}", c.text);
            }
            let joined = caps
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(joined, refs.join(" "), "words changed at limit {max}");
            for w in caps.windows(2) {
                assert!(w[0].end <= w[1].start + 1e-9, "captions overlap");
            }
        }
    }

    #[test]
    fn pieces_are_balanced_instead_of_leaving_a_lone_word() {
        // Seven words with a limit of six: 4 + 3, not 6 + 1.
        let caps = group_tokens(
            &steady(&["a", "b", "c", "d", "e", "f", "g"]),
            &Grouping::with_max_words(6),
        );
        let sizes: Vec<usize> = caps.iter().map(|c| c.text.split(' ').count()).collect();
        assert_eq!(sizes.len(), 2);
        assert!(sizes.iter().all(|&n| n >= 3), "{sizes:?}");
    }

    #[test]
    fn a_pause_is_preferred_over_an_even_split() {
        // Eight words with a limit of six. The speaker stops after the sixth word.
        let mut toks = steady(&["a", "b", "c", "d", "e", "f", "g", "h"]);
        for t in &mut toks[6..] {
            t.start += 0.5;
            t.end += 0.5;
        }
        let caps = group_tokens(&toks, &Grouping::with_max_words(6));
        assert_eq!(texts(&caps), ["a b c d e f", "g h"]);
    }

    #[test]
    fn a_comma_counts_as_a_pause() {
        let toks = steady(&["a", "b", "c,", "d", "e", "f", "g"]);
        let caps = group_tokens(&toks, &Grouping::with_max_words(5));
        assert_eq!(texts(&caps), ["a b c,", "d e f g"]);
    }

    #[test]
    fn english_phrases_and_attached_words_are_not_cut_apart() {
        // Nowhere to pause, so the cut falls where it hurts least.
        let toks = steady(&["අද", "අපි", "new", "phone", "එකක්", "බලමු", "ඉක්මනින්", "යමු"]);
        let caps = group_tokens(&toks, &Grouping::with_max_words(5));
        assert_eq!(texts(&caps), ["අද අපි new phone එකක්", "බලමු ඉක්මනින් යමු"]);
        for c in &caps {
            assert!(!c.text.starts_with("එකක්"), "{caps:?}");
            assert!(
                !c.text.ends_with("new") && !c.text.starts_with("phone"),
                "{caps:?}"
            );
        }
    }

    #[test]
    fn a_long_pause_or_the_end_of_a_sentence_always_starts_a_caption() {
        let mut toks = steady(&["a", "b", "c.", "d", "e"]);
        let caps = group_tokens(&toks, &Grouping::with_max_words(10));
        assert_eq!(texts(&caps), ["a b c.", "d e"]);
        toks = steady(&["a", "b", "c", "d"]);
        toks[2].start += 1.0;
        toks[2].end += 1.0;
        toks[3].start += 1.0;
        toks[3].end += 1.0;
        let caps = group_tokens(&toks, &Grouping::with_max_words(10));
        assert_eq!(texts(&caps), ["a b", "c d"]);
    }

    #[test]
    fn a_higher_word_limit_really_gives_longer_captions() {
        let words: Vec<String> = (0..10).map(|_| "ඔයාට".to_string()).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let few = group_tokens(&steady(&refs), &Grouping::with_max_words(3)).len();
        let many = group_tokens(&steady(&refs), &Grouping::with_max_words(10)).len();
        assert!(many < few, "{many} captions at 10 words, {few} at 3");
        assert_eq!(many, 1);
    }

    #[test]
    fn one_very_long_word_still_gets_a_caption() {
        let long = "අ".repeat(80);
        let caps = group_tokens(
            &steady(&["a", long.as_str(), "b"]),
            &Grouping::with_max_words(4),
        );
        assert!(caps.iter().any(|c| c.text.contains(&long)));
        assert_eq!(
            caps.iter()
                .map(|c| c.text.split(' ').count())
                .sum::<usize>(),
            3
        );
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
