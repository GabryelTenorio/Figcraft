# Localization parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created; measured from source) · **Target:** Figma Design (desktop app 126.10.7)

FigCraft's user interface is English only: there is no localization layer yet. This page lists the
twelve key languages of the craftrules standard, then the other languages Figma ships.

## What Figma ships

Figma's help center lists its web, desktop and mobile apps in **English, Japanese, Korean, French,
German, Spanish (Spain and Latin America) and Portuguese (Brazil)** (checked 2026-10-10). Simplified
Chinese, Hindi, Arabic, Indonesian and Vietnamese are not offered; Figma points users to the
browser's page translation. The `.lproj` folders in `/Applications/Figma.app` (60+) are the
Electron/Chromium locale packs, not Figma's own UI languages. Text *content* in any script is
supported by both apps (CJK, Arabic and Devanagari shaping, IME); Figma has no vertical text.

## What FigCraft has (measured 2026-10-10, from source)

- **Strings:** ~800 distinct capitalised string literals in `crates/ui-egui/src` (labels, tooltips,
  section titles, dialogs) and 435 command labels in `crates/engine/src` (menus, the Actions
  palette), all inline in the code. ~1,200–1,500 user-visible strings in all. No catalog, no
  translation function, no language preference.
- **Script support in documents:** Unicode bidi with Arabic shaping and mirrored brackets in text
  layers; right-to-left list markers and indents; right-to-left names in the layers panel, tabs
  and canvas labels (visual order, joined Arabic); inline IME composition with the candidate window
  at the caret; system-font fallback for scripts the UI fonts lack. Devanagari shapes through
  harfrust. No vertical text (Figma has none either).
- **Script support in the UI:** no mirrored (right-to-left) layout of panels.

## Languages

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Hours to full |
|---|---|---|---|---|---|---|---|
| English | en | ~1,300 / ~1,300 (100%, source) | yes | Latin | yes (source) | full | 0 |
| Simplified Chinese (Mandarin) | zh-Hans | 0 (0%) | no | CJK text, IME and font fallback work in documents; UI font needs CJK coverage | no | none | 5–8 |
| Spanish | es (es-ES, es-419) | 0 (0%) | no | Latin | no | none | 4–6 (both variants, as Figma ships) |
| Hindi | hi | 0 (0%) | no | Devanagari shaping works in documents; UI font fallback to check | no | none | 5–8 |
| Arabic | ar | 0 (0%) | no | Bidi, shaping and RTL names work; the UI does not mirror | no | none | 10–16 |
| French | fr | 0 (0%) | no | Latin | no | none | 3–5 |
| Portuguese | pt (pt-BR) | 0 (0%) | no | Latin | no | none | 3–5 |
| Indonesian | id | 0 (0%) | no | Latin | no | none | 3–5 |
| Japanese | ja | 0 (0%) | no | CJK text and IME work in documents; UI font needs CJK coverage | no | none | 5–8 |
| German | de | 0 (0%) | no | Latin; long words need wider panels | no | none | 3–5 |
| Korean | ko | 0 (0%) | no | Hangul text and IME work in documents; UI font needs coverage | no | none | 4–6 |
| Vietnamese | vi | 0 (0%) | no | Latin with stacked diacritics | no | none | 3–5 |

Other languages: none shipped (Figma ships none beyond the eight above).

Before any language: **15–25 h** to add the localization layer (string catalog format, a lookup
used by `ui-egui` and the command registry, plural and argument handling, a language preference,
a test that every string has a key, pseudo-localization to find truncation). Per-language hours
above assume that layer exists and are agent hours; every language also needs a native speaker's
review (human). Total for the eleven: **60–100 h** including the layer (48–77 h of languages).

Priority: Japanese, Korean, Spanish, Portuguese (Brazil), French and German first: those are
Figma's own languages, so their users expect them.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-10 | major | Created: English only, string counts measured from source, Figma's eight UI languages from its help center |
