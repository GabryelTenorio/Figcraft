//! Spell check while editing text (milestone M4): misspelled words in the text being edited are
//! underlined, and a flagged word's context menu offers suggestions, Ignore and Add to dictionary.
//!
//! The engine stays platform-free: the desktop app installs the operating system's checker as a
//! [`SpellChecker`] (like the [`LiveLink`](crate::live::LiveLink)). Words are checked one at a
//! time and cached, and the edited text's flagged words are worked out again only when the
//! document changes ([`Session::sync_spelling`]), never per frame.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, ok};
use crate::params::{boolean, req_index, req_string};
use crate::{EngineError, Result, Session, bad, spec, textedit};

/// The system's spell checker (the desktop app installs one: `session.spelling.checker`).
// shortcut: one language, the system's (no Text ▸ Spelling ▸ Language), add a language list to
// this trait when someone needs to check text in another language.
pub trait SpellChecker {
    /// Whether `word` (one word, no spaces) is misspelled.
    fn misspelled(&mut self, word: &str) -> bool;
    /// Replacements for a misspelled `word`, best first.
    fn suggest(&mut self, word: &str) -> Vec<String>;
}

/// Spell-check state: the checker, its answers so far and the words ignored until the app quits.
#[derive(Default)]
pub struct Spelling {
    pub checker: Option<Box<dyn SpellChecker>>,
    /// Word → misspelled, as the checker answered.
    known: HashMap<String, bool>,
    /// Lowercased words ignored for this run of the app.
    ignored: HashSet<String>,
    /// The flagged words of text layer `NodeId` in that document.
    flagged: Option<(Arc<Document>, NodeId, Ranges)>,
}

/// Char ranges.
type Ranges = Vec<(usize, usize)>;

/// Most suggestions offered for a word.
const MAX_SUGGESTIONS: usize = 5;
/// Longest word added to the dictionary, and most words in it (and in the checker's cache).
const MAX_WORD: usize = 64;
const MAX_DICTIONARY: usize = 10_000;

/// Scripts written without spaces between words: CJK, kana, Thai, Lao, Myanmar, Khmer.
// shortcut: text in these scripts is never checked (a run of it isn't one word, and the system
// checkers flag whole runs too), split it with a word breaker when someone needs it checked.
fn unspaced(c: char) -> bool {
    matches!(c as u32, 0x0E00..=0x0EFF | 0x1000..=0x109F | 0x1780..=0x17FF | 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF66..=0xFF9F | 0x20000..)
}

/// The words of `chars` as char ranges: letters and digits, with apostrophes inside a word
/// ("don't").
fn words(chars: &[char]) -> Vec<(usize, usize)> {
    let letter = |i: usize| chars.get(i).is_some_and(|c| c.is_alphanumeric() && !unspaced(*c));
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !letter(i) {
            i += 1;
            continue;
        }
        let a = i;
        while letter(i) || (matches!(chars.get(i), Some('\'' | '\u{2019}')) && letter(i + 1)) {
            i += 1;
        }
        out.push((a, i));
    }
    out
}

impl Session {
    /// Whether `word` should be flagged: the checker says so, and it isn't a number, ignored or in
    /// the dictionary.
    fn is_misspelled(&mut self, word: &str) -> bool {
        if word.chars().any(char::is_numeric) {
            return false;
        }
        let lower = word.to_lowercase();
        if self.prefs.dictionary.contains(&lower) || self.spelling.ignored.contains(&lower) {
            return false;
        }
        let sp = &mut self.spelling;
        let Some(checker) = sp.checker.as_mut() else { return false };
        if let Some(m) = sp.known.get(word) {
            return *m;
        }
        if sp.known.len() >= MAX_DICTIONARY {
            sp.known.clear();
        }
        let m = checker.misspelled(word);
        sp.known.insert(word.to_string(), m);
        m
    }

    /// Work out the edited text's misspelled words again if the document changed (the UI calls
    /// it before drawing; cheap otherwise).
    pub fn sync_spelling(&mut self) {
        let edited = self.ds().ok().and_then(|ds| Some((ds.doc.clone(), ds.text_edit.as_ref()?.id)));
        let Some((doc, id)) = edited.filter(|_| self.prefs.spell_check && self.spelling.checker.is_some()) else {
            self.spelling.flagged = None;
            return;
        };
        if self.spelling.flagged.as_ref().is_some_and(|(d, i, _)| Arc::ptr_eq(d, &doc) && *i == id) {
            return;
        }
        let chars: Vec<char> = doc.node(id).and_then(|n| n.kind.text()).map(|t| t.characters.chars().collect()).unwrap_or_default();
        let flagged = words(&chars)
            .into_iter()
            .filter(|(a, b)| {
                let word: String = chars.get(*a..*b).unwrap_or_default().iter().collect();
                self.is_misspelled(&word)
            })
            .collect();
        self.spelling.flagged = Some((doc, id, flagged));
    }

    /// The misspelled words (char ranges) of the text being edited, as of the last
    /// [`Session::sync_spelling`].
    pub fn misspelled(&self) -> &[(usize, usize)] {
        self.spelling.flagged.as_ref().map_or(&[], |(_, _, f)| f)
    }

    /// The word at char `index` of the text being edited (an index just after a word counts):
    /// its char range and text.
    fn word_at(&self, cmd: &str, index: usize) -> Result<Option<(usize, usize, String)>> {
        let te = self.ds()?.text_edit.clone().ok_or_else(|| EngineError::Other(format!("{cmd}: not editing text")))?;
        let chars: Vec<char> = self.doc()?.node(te.id).and_then(|n| n.kind.text()).map(|t| t.characters.chars().collect()).unwrap_or_default();
        Ok(words(&chars)
            .into_iter()
            .find(|(a, b)| (*a..=*b).contains(&index))
            .map(|(a, b)| (a, b, chars.get(a..b).unwrap_or_default().iter().collect())))
    }
}

fn toggle(s: &mut Session, p: &Value) -> Result<Value> {
    let on = boolean(p, "on").unwrap_or(!s.prefs.spell_check);
    s.prefs.spell_check = on;
    s.spelling.flagged = None;
    Ok(json!({"on": on, "available": s.spelling.checker.is_some()}))
}

fn suggestions(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "text.spellingSuggestions";
    let Some((a, b, word)) = s.word_at(CMD, req_index(CMD, p, "index")?)? else { return Ok(Value::Null) };
    let misspelled = s.is_misspelled(&word);
    let mut suggestions = match s.spelling.checker.as_mut() {
        Some(c) if misspelled => c.suggest(&word),
        _ => Vec::new(),
    };
    suggestions.truncate(MAX_SUGGESTIONS);
    Ok(json!({"word": word, "start": a, "end": b, "misspelled": misspelled, "suggestions": suggestions}))
}

fn replace_word(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "text.replaceWord";
    let with = req_string(CMD, p, "with")?;
    if with.is_empty() {
        return Err(bad(CMD, "`with` can't be empty"));
    }
    let (a, b, _) = s.word_at(CMD, req_index(CMD, p, "index")?)?.ok_or_else(|| bad(CMD, "no word at `index`"))?;
    textedit::set_caret(s, a, false)?;
    textedit::set_caret(s, b, true)?;
    textedit::insert(s, &with)?;
    ok()
}

/// A word from params, lowercased: one word of reasonable length.
fn word_param(cmd: &str, p: &Value) -> Result<String> {
    let w = req_string(cmd, p, "word")?.trim().to_lowercase();
    if w.is_empty() || w.chars().count() > MAX_WORD || w.chars().any(char::is_whitespace) {
        return Err(bad(cmd, format!("`word` must be one word of at most {MAX_WORD} characters")));
    }
    Ok(w)
}

fn add_word(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "spelling.addWord";
    let w = word_param(CMD, p)?;
    if !s.prefs.dictionary.contains(&w) {
        if s.prefs.dictionary.len() >= MAX_DICTIONARY {
            return Err(bad(CMD, format!("the dictionary is full ({MAX_DICTIONARY} words)")));
        }
        s.prefs.dictionary.push(w);
    }
    s.spelling.flagged = None;
    ok()
}

fn ignore(s: &mut Session, p: &Value) -> Result<Value> {
    let w = word_param("spelling.ignore", p)?;
    s.spelling.ignored.insert(w);
    s.spelling.flagged = None;
    ok()
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "text.spelling",
            "Check spelling",
            "{on? (default: switch)} → {on, available}: underline misspelled words while editing text (the system's spell checker; `available` is false where there is none)",
            false,
            toggle
        ),
        spec!(
            "text.spellingSuggestions",
            "Spelling suggestions",
            "{index (char)} → {word, start, end, misspelled, suggestions} for the word at `index` of the text being edited, or null",
            false,
            suggestions
        ),
        spec!("text.replaceWord", "Replace word", "{index (char), with}: replace the word at `index` of the text being edited", true, replace_word),
        spec!("spelling.addWord", "Add to dictionary", "{word}: never flag this word (saved in preferences)", false, add_word),
        spec!("spelling.ignore", "Ignore", "{word}: don't flag this word until the app quits", false, ignore),
    ]
}
