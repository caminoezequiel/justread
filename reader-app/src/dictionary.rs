//! The offline English → Spanish dictionary (English Wiktionary, CC BY-SA),
//! built by `dictionary/build.py` and embedded in the binary.
//!
//! [`Dictionary::lookup`] uses the words after the clicked one to find set
//! phrases first ("ran" in "ran out of time" → "run out"), then the word
//! itself, then its base forms ("ran" → "run").

use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

use flate2::read::GzDecoder;

/// `include_bytes!` puts the file's bytes into the binary at compile time.
const DATA: &[u8] = include_bytes!("../dictionary/en-es.tsv.gz");

/// Longest phrase tried after a clicked word, in extra words ("run out of").
const PHRASE_WORDS: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Sense {
    /// Part of speech: "verb", "noun", ….
    pub pos: String,
    /// Short English description of the sense.
    pub sense: String,
    pub spanish: Vec<String>,
}

/// One headword and its senses, in Wiktionary's order.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub headword: String,
    pub senses: Vec<Sense>,
}

#[derive(Default)]
pub struct Dictionary {
    senses: HashMap<String, Vec<Sense>>,
    /// Inflected form → base forms ("ran" → ["run"]).
    lemmas: HashMap<String, Vec<String>>,
}

impl Dictionary {
    /// The embedded dictionary, loaded on first use. `OnceLock` runs the
    /// closure once, even if several threads ask at the same time, and hands
    /// out `&'static` references afterwards. `None` if it can't be read.
    pub fn shared() -> Option<&'static Dictionary> {
        static SHARED: OnceLock<Option<Dictionary>> = OnceLock::new();
        SHARED
            .get_or_init(|| {
                let mut text = String::new();
                match GzDecoder::new(DATA).read_to_string(&mut text) {
                    Ok(_) => Some(Dictionary::parse(&text)),
                    Err(e) => {
                        eprintln!("justread: could not load the dictionary: {e}");
                        None
                    }
                }
            })
            .as_ref()
    }

    /// Parse the lines written by `build.py`; malformed lines are skipped.
    pub fn parse(text: &str) -> Self {
        let mut dictionary = Dictionary::default();
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            let mut fields = line.split('\t');
            match (fields.next(), fields.next(), fields.next(), fields.next()) {
                (Some(form), Some("="), Some(lemmas), None) => {
                    let lemmas = lemmas.split('|').map(str::to_owned).collect();
                    dictionary.lemmas.insert(form.to_owned(), lemmas);
                }
                (Some(word), Some(pos), Some(sense), Some(spanish)) => {
                    dictionary
                        .senses
                        .entry(word.to_owned())
                        .or_default()
                        .push(Sense {
                            pos: pos.to_owned(),
                            sense: sense.to_owned(),
                            spanish: spanish.split('|').map(str::to_owned).collect(),
                        });
                }
                _ => {}
            }
        }
        dictionary
    }

    /// Entries for `text` (a clicked word or a selection), most specific
    /// first. `after` is the rest of the sentence, for spotting phrases.
    pub fn lookup(&self, text: &str, after: &str) -> Vec<Entry> {
        let query = normalize(text);
        let Some(first) = query.split(' ').next().filter(|w| !w.is_empty()) else {
            return Vec::new();
        };
        let rest = query[first.len()..].to_owned();
        let mut heads = vec![first.to_owned()];
        heads.extend(self.base_forms(first));

        let mut found: Vec<String> = Vec::new();
        let mut add = |word: String| {
            if self.senses.contains_key(&word) && !found.contains(&word) {
                found.push(word);
            }
        };
        if rest.is_empty() {
            // The longest phrase made of the word (or a base form of it) and
            // the words after it, if any is a dictionary entry.
            let next = words(after);
            let phrase = (1..=PHRASE_WORDS.min(next.len())).rev().find_map(|n| {
                let tail = next[..n].join(" ");
                heads
                    .iter()
                    .map(|head| format!("{head} {tail}"))
                    .find(|p| self.senses.contains_key(p))
            });
            if let Some(phrase) = phrase {
                add(phrase);
            }
        }
        // A selection: as selected, then with its first word in base form
        // ("gave up" → "give up").
        for head in &heads {
            add(format!("{head}{rest}"));
        }
        found
            .into_iter()
            .filter_map(|headword| {
                let senses = self.senses.get(&headword)?.clone();
                Some(Entry { headword, senses })
            })
            .collect()
    }

    /// Base forms of `word` that are in the dictionary: Wiktionary's own
    /// links first, then guesses from common English endings.
    fn base_forms(&self, word: &str) -> Vec<String> {
        let mut forms: Vec<String> = self.lemmas.get(word).cloned().unwrap_or_default();
        for guess in suffix_guesses(word) {
            if guess != word && self.senses.contains_key(&guess) && !forms.contains(&guess) {
                forms.push(guess);
            }
        }
        forms
    }
}

/// Lowercase, straight apostrophes, single spaces, no punctuation around the
/// text and no possessive `'s`.
fn normalize(text: &str) -> String {
    let text = text.to_lowercase().replace(['’', '‘'], "'");
    let text = text.trim_matches(|c: char| !c.is_alphanumeric());
    let text = text.strip_suffix("'s").unwrap_or(text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first words of `text`, up to the first punctuation mark that ends a
/// phrase (a comma, a full stop…).
fn words(text: &str) -> Vec<String> {
    let clause = text
        .split([',', '.', ';', ':', '!', '?', '"', '“', '”', '(', ')'])
        .next();
    normalize(clause.unwrap_or(""))
        .split(' ')
        .filter(|w| !w.is_empty())
        .take(PHRASE_WORDS)
        .map(str::to_owned)
        .collect()
}

/// Possible base forms from regular endings: books, wishes, flies, played,
/// tried, liked, stopped, running, making, bigger, quickly.
fn suffix_guesses(word: &str) -> Vec<String> {
    let mut guesses = Vec::new();
    // `if let Some(stem) = …` runs the block only when the suffix is there.
    if let Some(stem) = word
        .strip_suffix("ies")
        .or_else(|| word.strip_suffix("ied"))
    {
        guesses.push(format!("{stem}y"));
    }
    for suffix in ["s", "es", "ed", "d", "ing", "er", "est", "ly"] {
        if let Some(stem) = word.strip_suffix(suffix).filter(|s| s.chars().count() >= 2) {
            guesses.push(stem.to_owned());
            // making → make, liked → like (via "d" above), bigger → big.
            guesses.push(format!("{stem}e"));
            let mut chars = stem.chars().rev();
            if let (Some(a), Some(b)) = (chars.next(), chars.next())
                && a == b
            {
                guesses.push(stem[..stem.len() - a.len_utf8()].to_owned());
            }
        }
    }
    guesses
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &str = "\
# test data
book\tnoun\tbound pages\tlibro
book\tverb\tto reserve\treservar
books\t=\tbook
give\tverb\tto transfer\tdar
give up\tverb\tsurrender\trendirse|entregar
gave\t=\tgive
run\tverb\tto move quickly on two feet\tcorrer
run out\tverb\tto use up\tagotar|quedarse sin
run out of\tverb\tto have no more of\tquedarse sin
ran\t=\trun
stop\tverb\tto cease\tparar
try\tverb\tto attempt\tintentar
make\tverb\tto create\thacer
don't\tverb\tdo not\tno
bad line
";

    fn headwords(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.headword.as_str()).collect()
    }

    #[test]
    fn parses_senses_and_forms() {
        let d = Dictionary::parse(DATA);
        let book = &d.senses["book"];
        assert_eq!(book.len(), 2);
        assert_eq!(book[1].pos, "verb");
        assert_eq!(d.senses["give up"][0].spanish, ["rendirse", "entregar"]);
        assert_eq!(d.lemmas["ran"], ["run"]);
        assert!(!d.senses.contains_key("bad line"));
    }

    #[test]
    fn a_word_and_its_base_form() {
        let d = Dictionary::parse(DATA);
        assert_eq!(headwords(&d.lookup("Books,", "are heavy")), ["book"]);
        assert_eq!(headwords(&d.lookup("book", "")), ["book"]);
        assert!(d.lookup("xyzzy", "").is_empty());
        assert!(d.lookup("…", "").is_empty());
    }

    #[test]
    fn regular_endings_without_a_form_entry() {
        let d = Dictionary::parse(DATA);
        assert_eq!(headwords(&d.lookup("stopped", "")), ["stop"]);
        assert_eq!(headwords(&d.lookup("tried", "")), ["try"]);
        assert_eq!(headwords(&d.lookup("making", "")), ["make"]);
        assert_eq!(headwords(&d.lookup("running", "")), ["run"]);
    }

    #[test]
    fn the_following_words_find_phrases() {
        let d = Dictionary::parse(DATA);
        // The longest phrase wins, then the word's own entry.
        assert_eq!(
            headwords(&d.lookup("ran", " out of time.")),
            ["run out of", "run"]
        );
        assert_eq!(
            headwords(&d.lookup("ran", " out, again")),
            ["run out", "run"]
        );
        // Punctuation ends the phrase.
        assert_eq!(headwords(&d.lookup("ran", ". Out of")), ["run"]);
    }

    #[test]
    fn selections_are_looked_up_whole() {
        let d = Dictionary::parse(DATA);
        assert_eq!(headwords(&d.lookup("gave  up", "")), ["give up"]);
        assert!(d.lookup("gave it up", "").is_empty());
    }

    #[test]
    fn curly_apostrophes_and_possessives() {
        let d = Dictionary::parse(DATA);
        assert_eq!(headwords(&d.lookup("Don’t", "")), ["don't"]);
        assert_eq!(headwords(&d.lookup("book’s", "")), ["book"]);
    }

    #[test]
    fn the_embedded_dictionary_loads() {
        let d = Dictionary::shared();
        assert!(d.is_some_and(|d| d.senses.len() > 10_000));
    }
}
