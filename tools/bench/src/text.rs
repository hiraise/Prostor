//! Synthetic text: a vocabulary of made-up words used with Zipf's law, as words are in real
//! writing, so full-text search meets a realistic mix of rare and frequent terms.

use std::collections::HashSet;

use crate::rng::{Rng, Weighted};

/// Words in the vocabulary.
pub const VOCABULARY: usize = 20_000;
/// Zipf exponent of word frequencies; about 1 in natural languages.
const WORD_EXPONENT: f64 = 1.0;

const ONSETS: [&str; 26] = [
    "b", "c", "d", "f", "g", "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "z", "br", "cr",
    "dr", "gr", "pl", "st", "tr", "sh", "ch", "th",
];
const VOWELS: [&str; 8] = ["a", "e", "i", "o", "u", "ai", "ou", "ea"];
const CODAS: [&str; 11] = ["", "", "", "n", "r", "s", "l", "t", "x", "nd", "rk"];

#[derive(Debug, Clone)]
pub struct Vocabulary {
    /// Ordered by frequency: index 0 is the most frequent word.
    words: Vec<String>,
    frequency: Weighted,
}

impl Vocabulary {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::for_item(seed, "vocabulary", 0);
        let mut seen = HashSet::with_capacity(VOCABULARY);
        let mut words = Vec::with_capacity(VOCABULARY);
        while words.len() < VOCABULARY {
            let rank = words.len();
            // Frequent words are short, as in real languages.
            let syllables = match rank {
                0..100 => 1 + rng.below(2),
                100..2000 => 2 + rng.below(2),
                _ => 2 + rng.below(3),
            };
            let word = make_word(&mut rng, syllables);
            if seen.insert(word.clone()) {
                words.push(word);
            }
        }
        Self {
            words,
            frequency: Weighted::zipf(VOCABULARY, WORD_EXPONENT),
        }
    }

    /// A word drawn by frequency.
    pub fn word(&self, rng: &mut Rng) -> &str {
        self.at(self.frequency.sample(rng))
    }

    /// The word of frequency rank `rank` (0 is the most frequent).
    pub fn at(&self, rank: usize) -> &str {
        self.words
            .get(rank.min(VOCABULARY - 1))
            .map_or("", String::as_str)
    }

    /// `count` words separated by spaces, the first one capitalized.
    pub fn phrase(&self, rng: &mut Rng, count: usize, out: &mut String) {
        for i in 0..count {
            let word = self.word(rng);
            if i == 0 {
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                }
            } else {
                out.push(' ');
                out.push_str(word);
            }
        }
    }

    /// Sentences with `count` words in total.
    pub fn text(&self, rng: &mut Rng, mut count: usize) -> String {
        let mut out = String::with_capacity(count * 8);
        while count > 0 {
            let sentence = (4 + rng.index(12)).min(count);
            if !out.is_empty() {
                out.push(' ');
            }
            self.phrase(rng, sentence, &mut out);
            out.push('.');
            count -= sentence;
        }
        out
    }
}

fn make_word(rng: &mut Rng, syllables: u64) -> String {
    let mut word = String::new();
    for _ in 0..syllables {
        word.push_str(ONSETS[rng.index(ONSETS.len())]);
        word.push_str(VOWELS[rng.index(VOWELS.len())]);
    }
    word.push_str(CODAS[rng.index(CODAS.len())]);
    word
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_is_unique_and_stable() {
        let a = Vocabulary::new(1);
        let b = Vocabulary::new(1);
        assert_eq!(a.words, b.words);
        assert_eq!(a.words.len(), VOCABULARY);
        let unique: HashSet<&String> = a.words.iter().collect();
        assert_eq!(unique.len(), VOCABULARY);
        assert!(
            a.words
                .iter()
                .all(|w| w.chars().all(|c| c.is_ascii_lowercase()))
        );
    }

    #[test]
    fn text_has_the_requested_length() {
        let vocabulary = Vocabulary::new(1);
        let mut rng = Rng::new(5);
        let text = vocabulary.text(&mut rng, 37);
        assert_eq!(text.split_whitespace().count(), 37);
        assert!(text.ends_with('.'));
        assert!(text.starts_with(|c: char| c.is_ascii_uppercase()));
    }
}
