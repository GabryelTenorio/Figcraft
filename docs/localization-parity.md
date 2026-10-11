# Localization parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-12 · **Change:** minor (Latin American Spanish overlay) · **Target:** Figma Design (desktop app 126.10.7)

FigCraft's interface has a localization layer with an English catalog and complete Spanish (with a
Latin American overlay), French, German, Brazilian Portuguese, Japanese and Korean drafts
(machine-drafted, waiting for native speakers' review). This page lists the twelve key languages of the craftrules standard, then the other languages Figma
ships. How to add a language: [`translating.md`](translating.md).

## What Figma ships

Figma's help center lists its web, desktop and mobile apps in **English, Japanese, Korean, French,
German, Spanish (Spain and Latin America) and Portuguese (Brazil)** (checked 2026-10-10). Simplified
Chinese, Hindi, Arabic, Indonesian and Vietnamese are not offered; Figma points users to the
browser's page translation. The `.lproj` folders in `/Applications/Figma.app` (60+) are the
Electron/Chromium locale packs, not Figma's own UI languages. Text *content* in any script is
supported by both apps (CJK, Arabic and Devanagari shaping, IME); Figma has no vertical text.

## What FigCraft has (measured 2026-10-12, from source)

- **Layer:** one TOML catalog per language in `crates/ui-egui/locales/`, baked into the binary by
  the UI crate's build script (desktop and web; nothing read at run time). Keys are the English text
  (`tr("Fill")`, `trf` for `{named}` placeholders, `trn` for counts with one/other plural forms), so
  a missing translation shows English, never a raw key. One English text that needs two
  translations takes a context (`"Fill|sizing"`).
- **Strings:** 1,351 UI keys in `en.toml` (36 with plural forms; ~110 are labels from engine and
  document data, such as tool names, blend modes and Dev Mode properties) plus 519 command labels
  and 29 menu wordings by command id: 1,899 entries. Command ids, the CLI, MCP and the control
  channel stay English. Accessibility names come from the same strings.
- **Language preference:** Preferences ▸ Language (System, English, Deutsch, Español, Español
  (Latinoamérica), Français, 日本語, 한국어, Português (Brasil)), applied at once without a restart;
  System takes the first of the OS's preferred languages that has a catalog (Windows' language
  list, macOS AppleLanguages, `LANGUAGE` then the locale variables on Linux and BSD; a regional tag
  falls back to its language, pt-PT or pt to pt-BR, and es-419, es-MX, es-AR, es-CO and the other
  Latin American and US Spanish tags to es-419) or the browser's language on the web.
  `pref.language {language}` sets it from the CLI, MCP and the control channel.
- **Regional overlays:** a regional catalog whose language has a catalog lists only what differs
  (`es-419.toml` over `es.toml`); its UI strings, command labels and menu wordings read the
  overlay, then the base language, then English.
- **Check:** `cargo test -p figcraft-ui-egui i18n` fails on keys the code uses that `en.toml`
  lacks, keys no code uses, unknown command ids, placeholders that differ from English, overlay
  entries that read as in their base language and, for complete languages (Spanish), anything
  untranslated. The French, German, Brazilian Portuguese, Japanese and Korean drafts translate
  everything today but are not held to it: a string added later shows English there until someone
  translates it.
- **Not yet localized:** messages the engine writes (errors, a few toasts, Dev Mode property values
  such as "inside"), number formatting (decimal points stay `.`), and plural forms beyond one/other
  (French and Brazilian Portuguese take the singular for 0 as well; Japanese and Korean have one
  form; CLDR's "many" for millions reads as the plural).
- **Script support in documents:** Unicode bidi with Arabic shaping and mirrored brackets in text
  layers; right-to-left list markers and indents; right-to-left names in the layers panel, tabs
  and canvas labels (visual order, joined Arabic); inline IME composition with the candidate window
  at the caret; system-font fallback for scripts the UI fonts lack. Devanagari shapes through
  harfrust. No vertical text (Figma has none either).
- **Script support in the UI:** menus widen to their longest label; dropdowns, the two-column
  labels of the right panel and the home screen's cards keep a long translation on one line with an
  ellipsis and show it in full on hover (checked in German, the longest); no mirrored
  (right-to-left) layout of panels. CJK translations are shorter than English; the same ellipsis
  and tooltip cover the few that don't fit (the sizing dropdowns).
- **CJK UI fonts:** egui's fonts and Inter have no CJK glyphs. Once the UI language, the open
  file's names or comments, typed text or the language picker needs Japanese kana, Hangul or Han,
  the desktop app adds one installed face per script to the UI's fallbacks (Windows: Yu Gothic UI,
  Meiryo, Malgun Gothic, Microsoft YaHei; macOS: Hiragino Sans, Apple SD Gothic Neo, PingFang SC;
  Linux and BSD: Noto Sans CJK, Source Han Sans, Nanum, WenQuanYi; the craft-fonts build's BIZ
  UDPGothic first), the UI language's script first; Han without kana outside a Japanese or Korean UI
  reads as Chinese. Only that face's file is read and egui shares its bytes, so English users load
  nothing (startup and memory measured unchanged); no CJK font installed means tofu, never a
  crash. The web build has no system fonts: built with craft-fonts and `FIGCRAFT_EMBED_CJK=1` it
  embeds Noto Sans CJK (+16 MB) and offers Japanese and Korean; otherwise the language picker and
  System leave them out. No font file is in this repo.

## Languages

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Hours to full |
|---|---|---|---|---|---|---|---|
| English | en | 1,899 / 1,899 (100%, source) | yes | Latin | yes (source) | full | 0 |
| Simplified Chinese (Mandarin) | zh-Hans | 0 (0%) | no | CJK text, IME and font fallback work in documents; CJK UI faces load on demand (desktop) | no | none | 3–5 |
| Spanish | es, es-419 | 1,899 / 1,899 (100%, draft); es-419 overlays 87 Latin American wordings (draft) | yes | Latin | no | draft | 0–1 (after native review) |
| Hindi | hi | 0 (0%) | no | Devanagari shaping works in documents; UI font fallback to check | no | none | 4–7 |
| Arabic | ar | 0 (0%) | no | Bidi, shaping and RTL names work; the UI does not mirror | no | none | 9–15 |
| French | fr | 1,899 / 1,899 (100%, draft) | yes | Latin | no | draft | 1–2 (after native review) |
| Portuguese | pt-BR | 1,899 / 1,899 (100%, draft) | yes | Latin | no | draft | 1–2 (after native review) |
| Indonesian | id | 0 (0%) | no | Latin | no | none | 3–5 |
| Japanese | ja | 1,899 / 1,899 (100%, draft) | yes | CJK text and IME in documents; UI text in an installed Japanese face (web: the `FIGCRAFT_EMBED_CJK` build) | no | draft | 1–2 (after native review) |
| German | de | 1,899 / 1,899 (100%, draft) | yes | Latin; long words elide in narrow fields | no | draft | 1–2 (after native review) |
| Korean | ko | 1,899 / 1,899 (100%, draft) | yes | Hangul text and IME in documents; UI text in an installed Korean face (web: the `FIGCRAFT_EMBED_CJK` build) | no | draft | 1–2 (after native review) |
| Vietnamese | vi | 0 (0%) | no | Latin with stacked diacritics | no | none | 3–5 |

Other languages: none shipped (Figma ships none beyond the eight above).

The layer is done. Per-language hours above are agent hours for a catalog of ~1,800 entries
(a machine draft, then a pass over the panels for overflow), plus each language's plural rule where
it isn't one/other; every language also needs a native speaker's review (human). Left before full
parity besides the languages: localizing engine messages and number formatting (4–6 h);
right-to-left mirroring of the panels is in Arabic's hours. Total: **31–54 h**.

Priority: native reviews of the six drafts and the es-419 overlay (every one of Figma's languages
and variants now has one), then Simplified Chinese, whose UI fonts already work.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-12 | minor | Latin American Spanish overlay (gap 5): `es-419.toml` lists only the 87 entries (61 UI strings, 26 command labels) whose wording differs in Latin American usage (agregar for añadir, presionar for pulsar, configuración for settings, Enter for the Intro key, control for a gamepad, resorte for a spring) and reads `es.toml`, then English, for the rest (a regional catalog overlays its language's); same tú register as Spanish; a draft awaiting native review. Preferences ▸ Language lists Español (Latinoamérica); System maps es-419, es-MX, es-AR, es-CO and the other Latin American and US Spanish tags to it, es-ES and es to Spanish. A regional catalog's lookups (UI strings, command labels, menu wordings) go overlay → base language → English; the check fails on an overlay entry that reads as in its base. Localization 35% → 36%, 32–55 h → 31–54 h |
| 2026-10-12 | minor | Japanese and Korean draft catalogs and CJK UI fonts (gap 5): `ja.toml` and `ko.toml` translate every UI string (1,351 keys; plurals give only `other`, and `plural_one` never picks "one" in either), command label (519) and menu wording (29), machine-drafted and marked as drafts awaiting native review; Preferences ▸ Language lists 日本語 and 한국어, System picks ja* and ko*. CJK UI text renders: once the UI language, the open file's names or comments, typed text or the language picker needs Japanese kana, Hangul or Han, the desktop app appends one installed face per script to egui's fallbacks (Windows Yu Gothic UI / Meiryo / Malgun Gothic / Microsoft YaHei, macOS Hiragino Sans / Apple SD Gothic Neo / PingFang SC, Linux Noto Sans CJK; the craft-fonts build's BIZ UDPGothic first), reading only that face's file and sharing its bytes with egui (English users load none: startup and memory unchanged); the web shows Japanese and Korean only when built with craft-fonts' Noto Sans CJK (`FIGCRAFT_EMBED_CJK`), else it keeps them out of the picker and System. Localization 30% → 35%, 40–65 h → 32–55 h |
| 2026-10-12 | minor | French, German and Brazilian Portuguese draft catalogs (gap 5): `fr.toml`, `de.toml` and `pt-BR.toml` translate every UI string (1,351 keys, 36 with plural forms), command label (519) and menu wording (29), machine-drafted and marked as drafts awaiting native review; they fall back to English per key, so new strings need only English and Spanish. French and Brazilian Portuguese count 0 as singular (`plural_one`); System picks the first OS language with a catalog (Windows' and macOS's preferred-language lists, `LANGUAGE` then the locale on Linux and BSD; pt-PT and pt read pt-BR); Preferences ▸ Language lists Français, Deutsch and Português (Brasil). Dropdowns, two-column labels and the home cards keep long translations on one line with an ellipsis and show them in full on hover. Localization 20% → 30%, 45–75 h → 40–65 h |
| 2026-10-11 | major | Localization layer landed (gap 5): catalogs per language baked in at build time, English text as keys and fallback, `{named}` placeholders and one/other plurals, command labels by command id, Preferences ▸ Language (System follows the OS or the browser) applied live, `pref.language`, and a catalog check; English catalog (1,280 UI keys) and a complete Spanish draft (1,810 entries with command labels) waiting for native review; localization 10% → 20%, 60–100 h → 45–75 h |
| 2026-10-10 | major | Created: English only, string counts measured from source, Figma's eight UI languages from its help center |
