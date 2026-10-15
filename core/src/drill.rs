//! Speaking-drill scoring: align what the learner said against the target
//! phrase, word by word, and grade each word.
//!
//! Comparison ignores case, accents, and punctuation ("¿Cómo?" == "como"),
//! and reads digits as Spanish number words, since whisper writes "3" for "tres".

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Said correctly.
    Correct,
    /// Recognizable but off (e.g. "playa" heard as "plaia").
    Close,
    /// A different word was heard.
    Wrong,
    /// Not said at all.
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct WordResult {
    /// The target word as written in the phrase, accents and all.
    pub word: String,
    pub status: Status,
    /// What was heard instead, for Close/Wrong.
    pub heard: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DrillResult {
    pub transcript: String,
    pub words: Vec<WordResult>,
    /// Words heard that are not in the phrase.
    pub extra: Vec<String>,
    /// 0–100.
    pub score: u32,
}

const NUMBERS: [&str; 21] = [
    "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez",
    "once", "doce", "trece", "catorce", "quince", "dieciseis", "diecisiete", "dieciocho",
    "diecinueve", "veinte",
];

/// Lowercase, strip accents and punctuation, spell out small numbers.
fn normalize(word: &str) -> String {
    let w: String = word
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            'á' | 'à' | 'ä' => Some('a'),
            'é' | 'è' | 'ë' => Some('e'),
            'í' | 'ì' | 'ï' => Some('i'),
            'ó' | 'ò' | 'ö' => Some('o'),
            'ú' | 'ù' | 'ü' => Some('u'),
            'ñ' => Some('n'),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect();
    match w.parse::<usize>() {
        Ok(n) if n < NUMBERS.len() => NUMBERS[n].to_string(),
        _ => w,
    }
}

/// Split into (original, normalized) pairs, dropping pure punctuation.
fn tokens(text: &str) -> Vec<(String, String)> {
    text.split_whitespace()
        .map(|w| (w.to_string(), normalize(w)))
        .filter(|(_, n)| !n.is_empty())
        .collect()
}

fn char_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Similar enough to count as a mispronunciation rather than a different word.
fn is_close(a: &str, b: &str) -> bool {
    let longest = a.chars().count().max(b.chars().count());
    longest >= 3 && char_distance(a, b) * 4 <= longest
}

/// Sentence-level comparison key: same words once case, accents, and
/// punctuation are ignored.
pub fn same_words(a: &str, b: &str) -> bool {
    let key = |s: &str| tokens(s).into_iter().map(|(_, n)| n).collect::<Vec<_>>();
    key(a) == key(b)
}

pub fn score(expected: &str, transcript: &str) -> DrillResult {
    let exp = tokens(expected);
    let got = tokens(transcript);
    let (n, m) = (exp.len(), got.len());

    // Word-level edit distance; substituting a close word costs less than a wrong one.
    let sub_cost = |i: usize, j: usize| -> usize {
        if exp[i].1 == got[j].1 {
            0
        } else if is_close(&exp[i].1, &got[j].1) {
            1
        } else {
            2
        }
    };
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        dp[i][0] = i * 2;
    }
    for j in 0..=m {
        dp[0][j] = j * 2;
    }
    for i in 1..=n {
        for j in 1..=m {
            dp[i][j] = (dp[i - 1][j - 1] + sub_cost(i - 1, j - 1))
                .min(dp[i - 1][j] + 2)
                .min(dp[i][j - 1] + 2);
        }
    }

    // Walk back through the table to recover the alignment.
    let mut words = Vec::with_capacity(n);
    let mut extra = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && dp[i][j] == dp[i - 1][j - 1] + sub_cost(i - 1, j - 1) {
            let status = match sub_cost(i - 1, j - 1) {
                0 => Status::Correct,
                1 => Status::Close,
                _ => Status::Wrong,
            };
            let heard = (status != Status::Correct).then(|| got[j - 1].0.clone());
            words.push(WordResult { word: exp[i - 1].0.clone(), status, heard });
            i -= 1;
            j -= 1;
        } else if i > 0 && dp[i][j] == dp[i - 1][j] + 2 {
            words.push(WordResult { word: exp[i - 1].0.clone(), status: Status::Missing, heard: None });
            i -= 1;
        } else {
            extra.push(got[j - 1].0.clone());
            j -= 1;
        }
    }
    words.reverse();
    extra.reverse();

    let points: f32 = words
        .iter()
        .map(|w| match w.status {
            Status::Correct => 1.0,
            Status::Close => 0.5,
            _ => 0.0,
        })
        .sum();
    let score = if n == 0 { 0 } else { (points / n as f32 * 100.0).round() as u32 };

    DrillResult { transcript: transcript.to_string(), words, extra, score }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statuses(r: &DrillResult) -> Vec<Status> {
        r.words.iter().map(|w| w.status).collect()
    }

    #[test]
    fn perfect_ignores_case_accents_punctuation() {
        let r = score("¿Cómo estás?", "como estas");
        assert_eq!(statuses(&r), vec![Status::Correct, Status::Correct]);
        assert_eq!(r.score, 100);
        assert_eq!(r.words[0].word, "¿Cómo");
    }

    #[test]
    fn digits_match_number_words() {
        let r = score("Nos vemos a las tres.", "Nos vemos a las 3.");
        assert_eq!(r.score, 100);
    }

    #[test]
    fn wrong_and_missing_words() {
        let r = score("Ayer fui a la playa", "Ayer va a playa");
        assert_eq!(
            statuses(&r),
            vec![Status::Correct, Status::Wrong, Status::Correct, Status::Missing, Status::Correct]
        );
        assert_eq!(r.words[1].heard.as_deref(), Some("va"));
        assert_eq!(r.score, 60);
    }

    #[test]
    fn close_mispronunciation() {
        let r = score("Me gusta la biblioteca", "me gusta la bibloteca");
        assert_eq!(r.words[3].status, Status::Close);
        assert_eq!(r.score, 88);
    }

    #[test]
    fn extra_words_reported() {
        let r = score("Muy bien", "muy muy bien gracias");
        assert_eq!(r.score, 100);
        assert_eq!(r.extra, vec!["muy", "gracias"]);
    }

    #[test]
    fn silence_scores_zero() {
        let r = score("Hola", "");
        assert_eq!(statuses(&r), vec![Status::Missing]);
        assert_eq!(r.score, 0);
    }
}
