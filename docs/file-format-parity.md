# File format parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created; formats from the installed Figma's Info.plist and help center, ours from source) · **Target:** Figma Design (desktop app 126.10.7)

Every format Figma reads or writes, with FigCraft's support. Sources: `Info.plist` of
`/Applications/Figma.app` (document types only; nothing inside the bundle's code was read), Figma's
help center (image and video types, export formats), and FigCraft's source.

**Summary (estimated):** ~40% ready. The core interchange formats (SVG, PNG, JPG, PDF) are solid
and tested; Figma's own document, `.fig`, can't be read or written, which alone keeps FigCraft at
alpha.

## Documents

| Format | Figma | FigCraft read | FigCraft write | Fidelity / testing | Hours |
|---|---|---|---|---|---|
| `.fig` (Figma Design, `com.figma.document`, `application/x-figma`) | native (cloud; "Save local copy") | **no** | **no** | none | read 30–60, write 15–30 |
| `.jam` (FigJam), `.deck` (Slides) | native to other Figma products | no | no | out of scope | — |
| REST API file JSON (`GET /v1/files/:key`) + local variables JSON + `images/` folder | the API's output | **yes** (`file.open`, `file.import`, CLI) | no | pages, layer tree, geometry, paints, strokes, effects, text runs, vectors, auto layout, constraints, grids, components, sets, instances with overrides, component properties, styles, prototyping (incl. conditionals and variable modes), variables with modes, aliases, scopes and bindings; warnings for what's skipped (video triggers and actions, modifier keys). Unit tests in `crates/format/src/rest/*_tests.rs` | — |
| `.figcraft` (ours: zip of `document.json` + images, versions inside) | — | yes | yes | streamed JSON, 50k layers save ~0.09 s / load ~0.16 s (`cargo xtask bench`); old files keep opening (`large_tests.rs`) | — |
| Local library files | team libraries in the cloud | yes (enable a `.figcraft` as a library) | yes (publish) | per-asset versions, updates with overrides kept | see [collaboration-parity](collaboration-parity.md) |

## Import (place / paste / drop)

| Format | Figma | FigCraft | Notes | Hours |
|---|---|---|---|---|
| SVG | editable vectors | **yes**, editable | corpus of 21 synthetic files with hand-computed expected pixels, structure and round-trip checks; gradients, patterns as image fills, clip paths, masks, shadow/blur filters, blend modes, text as text layers; other filters dropped with a warning; text on a path as outlines; fuzzed | 2–4 (filters) |
| PNG, JPG | yes | yes | | — |
| GIF | yes (animated) | first frame only | | 2–4 |
| WebP | yes | yes | | — |
| HEIC | yes | **no** | needs a decoder with an acceptable licence | 2–3 |
| TIFF | Safari only | **no** | | 1 |
| BMP | — | yes | | — |
| MP4, MOV, WebM (video fills, paid plans) | yes | **no** | presenter playback is most of the work | 4–8 |
| Images over 4096 px | downscaled to 4096 | kept | | — |
| Pasted content from other apps | images, SVG, text, Figma clipboard | images, SVG, text (newest clipboard content wins) | no Figma clipboard (it carries Figma's own binary) | with `.fig` |

## Export

| Format | Figma | FigCraft | Notes | Hours |
|---|---|---|---|---|
| PNG | yes, scales/suffixes | yes | export settings per layer, presets (`2x`, `512w`, `512h`), preview, "Export N layers" | — |
| JPG | yes | yes | | — |
| SVG | yes (outline text option, ids) | yes | alpha/luminance masks kept; noise, texture, progressive and background blur, glass embedded as images | 1–2 |
| PDF | yes | yes | one page per frame, real selectable text in subset fonts, links, vectors and gradients as vectors, effects as images; bleed, crop/bleed marks, page boxes, naive CMYK; checked by rendering the PDF back against PNG exports | CMYK output intent 2–4 |
| WebP | — | yes (ours) | | — |
| Copy as CSS / SVG / PNG / text / link | yes | yes | | — |
| Dev Mode code (CSS, iOS SwiftUI, Android Compose) | yes | yes, plus HTML and React | see [dev-mode-parity](dev-mode-parity.md) | — |
| `.fig` ("Save local copy") | yes | no | | 15–30 |

## Fonts

| | Figma | FigCraft |
|---|---|---|
| Local fonts | yes (font helper / desktop app) | yes (system scan) |
| Bundled/shared fonts | Google Fonts in the cloud | optional `CRAFT_FONTS_DIR` build input (storytold/craft-fonts) |
| Variable fonts | yes | yes (axes, named instances, per-range axis values) |
| Missing-font replacement on open | yes | warnings only (REST import) | 

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
