//! Spell check while editing text, against a fake checker (tests never touch the system's).

use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::Session;
use crate::params::id_of;
use crate::spelling::SpellChecker;

/// Knows a few words; counts how often it's asked.
struct Fake(Arc<Mutex<usize>>);

impl SpellChecker for Fake {
    fn misspelled(&mut self, word: &str) -> bool {
        *self.0.lock().unwrap() += 1;
        !["Hello", "world", "don't", "the"].contains(&word)
    }
    fn suggest(&mut self, word: &str) -> Vec<String> {
        vec![format!("{word}!"), "world".into()]
    }
}

/// Editing a text layer "Hello wrold, don't Helo 42" with the fake checker installed.
fn editing() -> (Session, Arc<Mutex<usize>>) {
    let asked = Arc::new(Mutex::new(0));
    let mut s = Session::new();
    s.spelling.checker = Some(Box::new(Fake(asked.clone())));
    let t = id_of(&s.execute("text.create", &json!({"text": "Hello wrold, don't Helo 42", "x": 0, "y": 0})).unwrap()["id"]).unwrap();
    s.execute("text.edit.begin", &json!({"id": t, "caret": 0})).unwrap();
    s.sync_spelling();
    (s, asked)
}

#[test]
fn misspelled_words_are_flagged_and_cached() {
    let (mut s, asked) = editing();
    // "wrold" and "Helo"; the apostrophe stays inside "don't" and numbers aren't words to check.
    assert_eq!(s.misspelled(), &[(6, 11), (19, 23)]);
    let n = *asked.lock().unwrap();
    s.sync_spelling();
    assert_eq!(*asked.lock().unwrap(), n, "nothing changed: nothing checked again");
    s.execute("text.edit.key", &json!({"key": "End"})).unwrap();
    s.execute("text.edit.insert", &json!({"text": " Hello"})).unwrap();
    s.sync_spelling();
    assert_eq!(*asked.lock().unwrap(), n, "known words come from the cache");
    s.execute("text.spelling", &json!({"on": false})).unwrap();
    s.sync_spelling();
    assert!(s.misspelled().is_empty(), "switched off");
    assert!(!s.prefs.spell_check);
}

#[test]
fn scripts_without_spaces_are_not_checked() {
    let (mut s, _) = editing();
    s.execute("text.edit.key", &json!({"key": "End"})).unwrap();
    s.execute("text.edit.insert", &json!({"text": " 这是中文 สวัสดี Привет"})).unwrap();
    s.sync_spelling();
    // CJK and Thai runs aren't single words; Cyrillic is checked like any other spaced script.
    assert_eq!(s.misspelled(), &[(6, 11), (19, 23), (39, 45)]);
}

#[test]
fn suggestions_replace_the_word_in_one_undo_step() {
    let (mut s, _) = editing();
    let r = s.execute("text.spellingSuggestions", &json!({"index": 8})).unwrap();
    assert_eq!(r, json!({"word": "wrold", "start": 6, "end": 11, "misspelled": true, "suggestions": ["wrold!", "world"]}));
    let ok = s.execute("text.spellingSuggestions", &json!({"index": 0})).unwrap();
    assert_eq!(ok["misspelled"], json!(false));
    assert_eq!(ok["suggestions"], json!([]));
    s.execute("text.replaceWord", &json!({"index": 11, "with": "world"})).unwrap();
    s.execute("text.edit.end", &json!({})).unwrap();
    let text = |s: &Session| s.doc().unwrap().node(s.selection()[0]).unwrap().kind.text().unwrap().characters.clone();
    assert_eq!(text(&s), "Hello world, don't Helo 42");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(text(&s), "Hello wrold, don't Helo 42");
    // Not editing: nothing to replace.
    assert!(s.execute("text.replaceWord", &json!({"index": 0, "with": "x"})).is_err());
}

#[test]
fn ignore_and_add_to_dictionary() {
    let (mut s, _) = editing();
    s.execute("spelling.ignore", &json!({"word": "wrold"})).unwrap();
    s.sync_spelling();
    assert_eq!(s.misspelled(), &[(19, 23)]);
    s.execute("spelling.addWord", &json!({"word": "Helo"})).unwrap();
    s.sync_spelling();
    assert!(s.misspelled().is_empty());
    assert_eq!(s.prefs.dictionary, vec!["helo".to_string()], "saved lowercased in preferences");
    assert!(s.execute("spelling.addWord", &json!({"word": "two words"})).is_err());
}
