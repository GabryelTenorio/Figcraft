//! The catalog checks: every key the code looks up is in the English catalog, no catalog has keys
//! the code doesn't use, placeholders match English, and complete languages translate everything.

use std::collections::BTreeSet;

use super::*;

/// Languages that must translate every UI string and command label.
const COMPLETE: &[&str] = &["es"];

/// Keys looked up from data rather than written as `tr("…")` in the code: the main menu's
/// submenu names (the engine's menu catalog) and labels of engine and document enums.
fn dynamic_keys() -> BTreeSet<String> {
    let mut keys: BTreeSet<String> = figcraft_engine::catalog::MENU.iter().flat_map(|m| m.menu.split('/')).map(str::to_string).collect();
    keys.extend(dynamic_labels().into_iter().map(str::to_string));
    keys
}

/// Labels from engine and document data the UI shows through `tr(x.label())` and isn't written as
/// a literal anywhere in the UI: node kinds (and the Design panel header's plurals), blend modes,
/// stroke caps and width profiles, effects, brushes, image filters, frame preset categories, Dev Mode
/// properties, comment filters and sorts, change kinds, the live status.
// shortcut: a hand-kept list; enumerate each enum's `ALL` here when a label goes missing in review.
const DATA_LABELS: &[&str] = &[
    "Added",
    "Align",
    "All comments",
    "Alpha",
    "Background blur",
    "Black & white",
    "Blend",
    "Bristle",
    "Circle",
    "Color burn",
    "Color dodge",
    "Component",
    "Component set",
    "Component sets",
    "Confetti",
    "Content",
    "Cool",
    "Darken",
    "Desktop",
    "Difference",
    "Direction",
    "Dots",
    "Drop shadow",
    "Duo",
    "Ellipses",
    "Exclude",
    "Excludes",
    "Exclusion",
    "Eye",
    "Fade",
    "Fills",
    "Frames",
    "Glass",
    "Group",
    "Groups",
    "Hard light",
    "Hue",
    "Ink",
    "Inner shadow",
    "Instances",
    "Intersect",
    "Intersects",
    "Joining…",
    "Layer blur",
    "Leaves",
    "Lighten",
    "Line arrow",
    "Lines",
    "Luminance",
    "Luminosity",
    "Mono",
    "Multi",
    "Multiply",
    "Muted",
    "No status",
    "Noise",
    "Normal",
    "Only your threads",
    "Padding",
    "Paper",
    "Pass through",
    "Phone",
    "Pinch",
    "Plus darker",
    "Plus lighter",
    "Polygons",
    "Presentation",
    "Progressive",
    "Rectangles",
    "Removed",
    "Reversed triangle",
    "Ribbon",
    "Rounded",
    "Screen",
    "Sections",
    "Slices",
    "Social media",
    "Soft light",
    "Sort by date",
    "Sort by page",
    "Square",
    "Stars",
    "Strokes",
    "Style",
    "Subtract",
    "Subtracts",
    "Tablet",
    "Taper",
    "Taper both ends",
    "Text align",
    "Texts",
    "Texture",
    "Triangle arrow",
    "Uniform",
    "Union",
    "Unions",
    "Unread",
    "Vectors",
    "Vivid",
    "Warm",
    "Watch",
    "Wedge",
    "Weight",
];

/// Labels of engine enums the UI shows through `tr(x.label())`, read from the engine, plus [`DATA_LABELS`].
fn dynamic_labels() -> Vec<&'static str> {
    use figcraft_engine::keymap::{GESTURES, Group};
    let mut v: Vec<&'static str> = figcraft_engine::ToolId::ALL.iter().map(|t| t.label()).collect();
    v.extend(figcraft_engine::vectoredit::VTool::ALL.iter().map(|t| t.label()));
    v.extend(Group::ALL.iter().map(|g| g.label()));
    v.extend(GESTURES.iter().flat_map(|(_, label, mac, other)| [*label, *mac, *other]));
    v.extend(DATA_LABELS);
    v
}

/// Ids of main-menu items whose label isn't the command's (besides a closing "…").
fn menu_worded_differently() -> BTreeSet<&'static str> {
    let bare = |s: &'static str| s.trim_end_matches('…');
    figcraft_engine::catalog::MENU
        .iter()
        .filter(|m| figcraft_engine::find_command(m.id).is_some_and(|c| bare(c.label) != bare(m.label)))
        .map(|m| m.id)
        .collect()
}

/// The string literals passed to `tr`, `trf`, `trn` and `tr_noop` in this crate's sources.
fn used_keys() -> BTreeSet<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut keys = BTreeSet::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name == "i18n.rs" {
            continue;
        }
        keys.extend(literals(&std::fs::read_to_string(&path).unwrap()));
    }
    keys
}

/// The first-argument string literals of the lookup functions in `src`.
fn literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for f in ["tr(", "trf(", "trn(", "tr_noop("] {
        let mut from = 0;
        while let Some(i) = src.get(from..).and_then(|s| s.find(f)) {
            let at = from + i;
            from = at + f.len();
            let prev = src.get(..at).and_then(|s| s.chars().next_back());
            if prev.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = src.get(from..).unwrap().trim_start();
            if let Some(lit) = rest.strip_prefix('"').and_then(string_literal) {
                out.push(lit);
            }
        }
    }
    out
}

/// A Rust string literal's value (after its opening quote), up to the closing quote.
fn string_literal(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '\n' => {
                    while chars.peek().is_some_and(|c| c.is_whitespace()) {
                        chars.next();
                    }
                }
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
    None
}

fn placeholders(s: &str) -> BTreeSet<&str> {
    s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(name, _)| name)).collect()
}

fn forms(t: &Text) -> Vec<&'static str> {
    match t {
        Text::Plain(s) => vec![s],
        Text::Plural(one, other) => one.iter().copied().chain([*other]).collect(),
    }
}

fn english() -> &'static Lang {
    LANGS.first().filter(|l| l.code == "en").expect("locales/en.toml is baked in first")
}

#[test]
fn every_key_in_the_code_is_in_the_english_catalog_and_no_other() {
    let used: BTreeSet<String> = used_keys().into_iter().chain(dynamic_keys()).collect();
    let en: BTreeSet<String> = english().ui.iter().map(|(k, _)| k.to_string()).collect();
    let missing: Vec<_> = used.difference(&en).collect();
    let unused: Vec<_> = en.difference(&used).collect();
    assert!(
        missing.is_empty(),
        "keys missing from locales/en.toml:\n{}",
        missing.iter().map(|k| format!("{k:?} = {k:?}")).collect::<Vec<_>>().join("\n")
    );
    assert!(unused.is_empty(), "keys in locales/en.toml the code doesn't use: {unused:?}");
}

#[test]
fn english_text_is_its_key_except_plurals() {
    for (k, t) in english().ui {
        if let Text::Plain(s) = t {
            // "Fill|sizing": the same English as "Fill", translated apart (a context after `|`).
            assert_eq!(k.split('|').next(), Some(*s), "an English entry differs from its key (the key is the English text)");
        }
    }
    assert!(english().commands.is_empty() && english().menu.is_empty(), "English command labels come from the engine");
}

#[test]
fn catalogs_have_known_keys_and_matching_placeholders() {
    let en = english();
    let ids: BTreeSet<&str> = figcraft_engine::all_commands()
        .iter()
        .map(|c| c.id)
        .chain(figcraft_engine::catalog::MENU.iter().map(|m| m.id).filter(|id| !id.is_empty()))
        .collect();
    for lang in LANGS {
        assert!(!lang.name.is_empty(), "{}: [meta] name is missing", lang.code);
        for (k, t) in lang.ui {
            let src = find(en.ui, k).unwrap_or_else(|| panic!("{}: unknown key {k:?}", lang.code));
            let want = placeholders(forms(src).last().unwrap());
            for (i, form) in forms(t).iter().enumerate() {
                let got = placeholders(form);
                // A "one" form may spell the count out ("una capa").
                let ok = if matches!(t, Text::Plural(Some(_), _)) && i == 0 { got.is_subset(&want) } else { got == want };
                assert!(ok, "{}: {k:?} has placeholders {got:?}, English has {want:?}", lang.code);
            }
            assert_eq!(matches!(t, Text::Plural(..)), matches!(src, Text::Plural(..)), "{}: {k:?} plural or not as in English", lang.code);
        }
        for (id, t) in lang.commands.iter().chain(lang.menu) {
            assert!(ids.contains(id), "{}: unknown command id {id:?}", lang.code);
            assert!(matches!(t, Text::Plain(s) if !s.trim().is_empty()), "{}: {id:?} is empty or plural", lang.code);
        }
        for (id, _) in lang.menu {
            assert!(menu_worded_differently().contains(id), "{}: [menu] {id:?} reads like the command; [commands] covers it", lang.code);
        }
        if COMPLETE.contains(&lang.code) {
            let missing: Vec<_> = en.ui.iter().filter(|(k, _)| find(lang.ui, k).is_none()).map(|(k, _)| k).collect();
            assert!(missing.is_empty(), "{}: untranslated: {missing:?}", lang.code);
            let missing: Vec<_> = ids.iter().filter(|id| find(lang.commands, id).is_none()).collect();
            assert!(missing.is_empty(), "{}: untranslated commands: {missing:?}", lang.code);
            let missing: Vec<_> = menu_worded_differently().into_iter().filter(|id| find(lang.menu, id).is_none()).collect();
            assert!(missing.is_empty(), "{}: menu items worded unlike their command, missing from [menu]: {missing:?}", lang.code);
        }
    }
}

#[test]
fn lookups_fall_back_to_english_and_fill_placeholders() {
    assert_eq!(fill("Delete {name}?", &[("name", &"Frame {1}")]), "Delete Frame {1}?");
    assert_eq!(fill("{a} and {b", &[("a", &1)]), "1 and {b");
    assert!(lookup(0, "no such key").is_none());
    assert_eq!(resolve("system", Some("es_ES.UTF-8")), resolve("es", None));
    assert_eq!(resolve("system", None), 0);
    assert_eq!(resolve("xx-YY", None), 0);
    let es = resolve("es-419", None);
    assert_ne!(es, 0, "Spanish is baked in");
    // A key Spanish lacks reads English; Spanish entries read Spanish.
    let (k, t) = LANGS[es].ui.first().unwrap();
    assert!(matches!(lookup(es, k), Some(x) if std::ptr::eq(x, t)));
}

#[test]
fn the_key_scanner_reads_rust_literals() {
    let src = r#"x(tr("A \"b\"")); trf(
        "C {n}", &[]); attr("no"); tr_noop("D\
          E"); tr(label);"#;
    assert_eq!(literals(src), vec!["A \"b\"", "C {n}", "DE"]);
}

#[test]
fn the_language_preference_takes_language_codes() {
    let mut s = figcraft_engine::Session::new();
    assert_eq!(s.prefs.language, "system");
    s.execute("pref.language", &serde_json::json!({"language": "es-419"})).unwrap();
    assert_eq!(s.prefs.language, "es-419");
    for bad in ["", "../es", "es--ES", "abcdefghi"] {
        assert!(s.execute("pref.language", &serde_json::json!({"language": bad})).is_err(), "{bad:?}");
    }
}
