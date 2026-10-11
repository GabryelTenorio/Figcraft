# Localization parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** major (localization layer; English catalog; Spanish as a draft) · **Target:** Figma Design (desktop app 126.10.7)

FigCraft's interface has a localization layer with an English catalog and a complete Spanish draft
(machine-drafted, waiting for a native speaker's review). This page lists the twelve key languages
of the craftrules standard, then the other languages Figma ships. How to add a language:
[`translating.md`](translating.md).

## What Figma ships

Figma's help center lists its web, desktop and mobile apps in **English, Japanese, Korean, French,
German, Spanish (Spain and Latin America) and Portuguese (Brazil)** (checked 2026-10-10). Simplified
Chinese, Hindi, Arabic, Indonesian and Vietnamese are not offered; Figma points users to the
browser's page translation. The `.lproj` folders in `/Applications/Figma.app` (60+) are the
Electron/Chromium locale packs, not Figma's own UI languages. Text *content* in any script is
supported by both apps (CJK, Arabic and Devanagari shaping, IME); Figma has no vertical text.

## What FigCraft has (measured 2026-10-11, from source)

- **Layer:** one TOML catalog per language in `crates/ui-egui/locales/`, baked into the binary by
  the UI crate's build script (desktop and web; nothing read at run time). Keys are the English text
  (`tr("Fill")`, `trf` for `{named}` placeholders, `trn` for counts with one/other plural forms), so
  a missing translation shows English, never a raw key. One English text that needs two
  translations takes a context (`"Fill|sizing"`).
- **Strings:** 1,280 UI keys in `en.toml` (35 with plural forms; ~110 are labels from engine and
  document data, such as tool names, blend modes and Dev Mode properties) plus 501 command labels
  and 29 menu wordings by command id. Command ids, the CLI, MCP and the control channel stay
  English. Accessibility names come from the same strings.
- **Language preference:** Preferences ▸ Language (System, English, Español), applied at once
  without a restart; System follows the OS (Windows language list, macOS AppleLanguages, the
  locale variables on Linux and BSD) or the browser's language on the web. `pref.language
  {language}` sets it from the CLI, MCP and the control channel.
- **Check:** `cargo test -p figcraft-ui-egui i18n` fails on keys the code uses that `en.toml`
  lacks, keys no code uses, unknown command ids, placeholders that differ from English and, for
  complete languages (Spanish), anything untranslated.
- **Not yet localized:** messages the engine writes (errors, a few toasts, Dev Mode property values
  such as "inside"), number formatting (decimal points stay `.`), and plural rules beyond one/other
  (enough for English and Spanish).
- **Script support in documents:** Unicode bidi with Arabic shaping and mirrored brackets in text
  layers; right-to-left list markers and indents; right-to-left names in the layers panel, tabs
  and canvas labels (visual order, joined Arabic); inline IME composition with the candidate window
  at the caret; system-font fallback for scripts the UI fonts lack. Devanagari shapes through
  harfrust. No vertical text (Figma has none either).
- **Script support in the UI:** menus widen to their longest label; no mirrored (right-to-left)
  layout of panels.

## Languages

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Hours to full |
|---|---|---|---|---|---|---|---|
| English | en | 1,810 / 1,810 (100%, source) | yes | Latin | yes (source) | full | 0 |
| Simplified Chinese (Mandarin) | zh-Hans | 0 (0%) | no | CJK text, IME and font fallback work in documents; UI font needs CJK coverage | no | none | 4–7 |
| Spanish | es | 1,810 / 1,810 (100%, draft) | yes | Latin | no | draft | 1–2 (es-419 differences, after native review) |
| Hindi | hi | 0 (0%) | no | Devanagari shaping works in documents; UI font fallback to check | no | none | 4–7 |
| Arabic | ar | 0 (0%) | no | Bidi, shaping and RTL names work; the UI does not mirror | no | none | 9–15 |
| French | fr | 0 (0%) | no | Latin | no | none | 3–5 |
| Portuguese | pt (pt-BR) | 0 (0%) | no | Latin | no | none | 3–5 |
| Indonesian | id | 0 (0%) | no | Latin | no | none | 3–5 |
| Japanese | ja | 0 (0%) | no | CJK text and IME work in documents; UI font needs CJK coverage | no | none | 4–7 |
| German | de | 0 (0%) | no | Latin; long words need wider panels | no | none | 3–5 |
| Korean | ko | 0 (0%) | no | Hangul text and IME work in documents; UI font needs coverage | no | none | 3–5 |
| Vietnamese | vi | 0 (0%) | no | Latin with stacked diacritics | no | none | 3–5 |

Other languages: none shipped (Figma ships none beyond the eight above).

The layer is done. Per-language hours above are agent hours for a catalog of ~1,800 entries
(a machine draft, then a pass over the panels for overflow), plus each language's plural rule where
it isn't one/other; every language also needs a native speaker's review (human). Left before full
parity besides the languages: localizing engine messages and number formatting (4–6 h);
right-to-left mirroring of the panels is in Arabic's hours. Total: **45–75 h**.

Priority: Japanese, Korean, Portuguese (Brazil), French and German next (with Spanish, Figma's own
languages, so their users expect them), and a native review of the Spanish draft.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | major | Localization layer landed (gap 5): catalogs per language baked in at build time, English text as keys and fallback, `{named}` placeholders and one/other plurals, command labels by command id, Preferences ▸ Language (System follows the OS or the browser) applied live, `pref.language`, and a catalog check; English catalog (1,280 UI keys) and a complete Spanish draft (1,810 entries with command labels) waiting for native review; localization 10% → 20%, 60–100 h → 45–75 h |
| 2026-10-10 | major | Created: English only, string counts measured from source, Figma's eight UI languages from its help center |
