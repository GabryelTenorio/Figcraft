# Localization parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-12 · **Change:** minor (French, German and Brazilian Portuguese drafts) · **Target:** Figma Design (desktop app 126.10.7)

FigCraft's interface has a localization layer with an English catalog and complete Spanish, French,
German and Brazilian Portuguese drafts (machine-drafted, waiting for native speakers' review). This
page lists the twelve key languages of the craftrules standard, then the other languages Figma
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
- **Language preference:** Preferences ▸ Language (System, English, Deutsch, Español, Français,
  Português (Brasil)), applied at once without a restart; System takes the first of the OS's
  preferred languages that has a catalog (Windows' language list, macOS AppleLanguages, `LANGUAGE`
  then the locale variables on Linux and BSD; a regional tag falls back to its language, and pt-PT
  or pt to pt-BR) or the browser's language on the web. `pref.language {language}` sets it from
  the CLI, MCP and the control channel.
- **Check:** `cargo test -p figcraft-ui-egui i18n` fails on keys the code uses that `en.toml`
  lacks, keys no code uses, unknown command ids, placeholders that differ from English and, for
  complete languages (Spanish), anything untranslated. The French, German and Brazilian Portuguese
  drafts translate everything today but are not held to it: a string added later shows English
  there until someone translates it.
- **Not yet localized:** messages the engine writes (errors, a few toasts, Dev Mode property values
  such as "inside"), number formatting (decimal points stay `.`), and plural forms beyond one/other
  (French and Brazilian Portuguese take the singular for 0 as well; CLDR's "many" for millions
  reads as the plural).
- **Script support in documents:** Unicode bidi with Arabic shaping and mirrored brackets in text
  layers; right-to-left list markers and indents; right-to-left names in the layers panel, tabs
  and canvas labels (visual order, joined Arabic); inline IME composition with the candidate window
  at the caret; system-font fallback for scripts the UI fonts lack. Devanagari shapes through
  harfrust. No vertical text (Figma has none either).
- **Script support in the UI:** menus widen to their longest label; dropdowns, the two-column
  labels of the right panel and the home screen's cards keep a long translation on one line with an
  ellipsis and show it in full on hover (checked in German, the longest); no mirrored
  (right-to-left) layout of panels.

## Languages

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Hours to full |
|---|---|---|---|---|---|---|---|
| English | en | 1,899 / 1,899 (100%, source) | yes | Latin | yes (source) | full | 0 |
| Simplified Chinese (Mandarin) | zh-Hans | 0 (0%) | no | CJK text, IME and font fallback work in documents; UI font needs CJK coverage | no | none | 4–7 |
| Spanish | es | 1,899 / 1,899 (100%, draft) | yes | Latin | no | draft | 1–2 (es-419 differences, after native review) |
| Hindi | hi | 0 (0%) | no | Devanagari shaping works in documents; UI font fallback to check | no | none | 4–7 |
| Arabic | ar | 0 (0%) | no | Bidi, shaping and RTL names work; the UI does not mirror | no | none | 9–15 |
| French | fr | 1,899 / 1,899 (100%, draft) | yes | Latin | no | draft | 1–2 (after native review) |
| Portuguese | pt-BR | 1,899 / 1,899 (100%, draft) | yes | Latin | no | draft | 1–2 (after native review) |
| Indonesian | id | 0 (0%) | no | Latin | no | none | 3–5 |
| Japanese | ja | 0 (0%) | no | CJK text and IME work in documents; UI font needs CJK coverage | no | none | 4–7 |
| German | de | 1,899 / 1,899 (100%, draft) | yes | Latin; long words elide in narrow fields | no | draft | 1–2 (after native review) |
| Korean | ko | 0 (0%) | no | Hangul text and IME work in documents; UI font needs coverage | no | none | 3–5 |
| Vietnamese | vi | 0 (0%) | no | Latin with stacked diacritics | no | none | 3–5 |

Other languages: none shipped (Figma ships none beyond the eight above).

The layer is done. Per-language hours above are agent hours for a catalog of ~1,800 entries
(a machine draft, then a pass over the panels for overflow), plus each language's plural rule where
it isn't one/other; every language also needs a native speaker's review (human). Left before full
parity besides the languages: localizing engine messages and number formatting (4–6 h);
right-to-left mirroring of the panels is in Arabic's hours. Total: **40–65 h**.

Priority: Japanese and Korean next (the last of Figma's own languages, so their users expect
them), and native reviews of the Spanish, French, German and Brazilian Portuguese drafts.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-12 | minor | French, German and Brazilian Portuguese draft catalogs (gap 5): `fr.toml`, `de.toml` and `pt-BR.toml` translate every UI string (1,351 keys, 36 with plural forms), command label (519) and menu wording (29), machine-drafted and marked as drafts awaiting native review; they fall back to English per key, so new strings need only English and Spanish. French and Brazilian Portuguese count 0 as singular (`plural_one`); System picks the first OS language with a catalog (Windows' and macOS's preferred-language lists, `LANGUAGE` then the locale on Linux and BSD; pt-PT and pt read pt-BR); Preferences ▸ Language lists Français, Deutsch and Português (Brasil). Dropdowns, two-column labels and the home cards keep long translations on one line with an ellipsis and show them in full on hover. Localization 20% → 30%, 45–75 h → 40–65 h |
| 2026-10-11 | major | Localization layer landed (gap 5): catalogs per language baked in at build time, English text as keys and fallback, `{named}` placeholders and one/other plurals, command labels by command id, Preferences ▸ Language (System follows the OS or the browser) applied live, `pref.language`, and a catalog check; English catalog (1,280 UI keys) and a complete Spanish draft (1,810 entries with command labels) waiting for native review; localization 10% → 20%, 60–100 h → 45–75 h |
| 2026-10-10 | major | Created: English only, string counts measured from source, Figma's eight UI languages from its help center |
