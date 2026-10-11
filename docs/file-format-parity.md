# File format parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (embedded image colour profiles; `.fig` import, best effort; animated GIF fills and TIFF images; last major: created, formats from the installed Figma's Info.plist and help center, ours from source) · **Target:** Figma Design (desktop app 126.10.7)

Every format Figma reads or writes, with FigCraft's support. Sources: `Info.plist` of
`/Applications/Figma.app` (document types only; nothing inside the bundle's code was read), Figma's
help center (image and video types, export formats), and FigCraft's source.

**Summary (estimated):** ~55% ready. The core interchange formats (SVG, PNG, JPG, PDF) are solid
and tested; Figma's own document, `.fig`, opens (best effort, not yet checked against real-world
files) but can't be written.

## Documents

| Format | Figma | FigCraft read | FigCraft write | Fidelity / testing | Hours |
|---|---|---|---|---|---|
| `.fig` (Figma Design, `com.figma.document`, `application/x-figma`) | native (cloud; "Save local copy") | **yes, best effort** (`file.open`, `file.import`, drop, CLI, MCP) | **no** | `.fig` files open (best effort): the zip, its deflate/zstd chunks and the kiwi schema are decoded generically and mapped by field name onto the REST importer, so pages, the layer tree, geometry, paints (styles resolved), strokes, effects, text with styled runs, vectors (as outlines), auto layout (grid included), constraints, components, sets, instances (expanded from their main with overrides, derived layout and property values), styles, variables with modes and bindings, prototyping and image fills from the archive come across; anything else is a warning. Vectors are outlines, not editable networks; sticky notes, connectors, tables and other non-design layers, set-variable and conditional actions and video fills are skipped with warnings. Tested on fixtures generated in code (`crates/format/src/fig/tests.rs`, fuzzed); no real-world files yet | read tail 8–15, write 15–30 |
| `.jam` (FigJam), `.deck` (Slides) | native to other Figma products | no | no | out of scope | — |
| REST API file JSON (`GET /v1/files/:key`) + local variables JSON + `images/` folder | the API's output | **yes** (`file.open`, `file.import`, CLI) | no | pages, layer tree, geometry, paints, strokes, effects, text runs, vectors, auto layout, constraints, grids, components, sets, instances with overrides, component properties, styles, prototyping (incl. conditionals and variable modes), variables with modes, aliases, scopes and bindings; warnings for what's skipped (video triggers and actions, modifier keys). Unit tests in `crates/format/src/rest/*_tests.rs` | — |
| `.figcraft` (ours: zip of `document.json` + images, versions inside) | — | yes | yes | streamed JSON, 50k layers save ~0.09 s / load ~0.16 s (`cargo xtask bench`); old files keep opening (`large_tests.rs`) | — |
| Local library files | team libraries in the cloud | yes (enable a `.figcraft` as a library) | yes (publish) | per-asset versions, updates with overrides kept | see [collaboration-parity](collaboration-parity.md) |

## Import (place / paste / drop)

| Format | Figma | FigCraft | Notes | Hours |
|---|---|---|---|---|
| SVG | editable vectors | **yes**, editable | corpus of 21 synthetic files with hand-computed expected pixels, structure and round-trip checks; gradients, patterns as image fills, clip paths, masks, shadow/blur filters, blend modes, text as text layers; other filters dropped with a warning; text on one path as text on a path (on several paths as outlines); fuzzed | 2–4 (filters) |
| PNG, JPG | yes | yes | embedded ICC profiles (PNG `iCCP`, JPG APP2) and PNG `sRGB` / `gAMA` / `cHRM` chunks convert the image into the file's colour space; no profile (or CMYK, damaged): taken as sRGB | — |
| GIF | yes (animated) | **yes**, animated | the file is kept as is; the canvas and PNG, JPG and PDF exports show the first frame (SVG exports embed the file, so viewers that animate GIFs animate it), the presenter and the preview play it (frames decoded on first play: at most 500 frames and 128 MB, past that the rest don't play; always loops) | — |
| WebP | yes | yes | embedded ICC profile honoured | — |
| HEIC | yes | **no** | shortcut: no Rust decoder; the usable ones (libheif, libde265) are C libraries, and HEVC carries patents. Revisit when a Rust decoder exists | 2–3 |
| TIFF | Safari only | **yes** | open, import, place, drop (the `image` crate's TIFF decoder; its embedded ICC profile is honoured); SVG exports embed it (and BMP) as PNG, which SVG viewers show | — |
| BMP | — | yes | | — |
| MP4, MOV, WebM (video fills, paid plans) | yes | **no** | shortcut: decoding H.264/HEVC/VP9 needs non-Rust libraries (FFmpeg or each platform's media framework); presenter playback would reuse the animated GIF clock | 4–8 |
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
| 2026-10-11 | minor | Embedded image colour profiles: placed and opened images are converted once, when decoded, from their own colour space into the file's (sRGB or Display P3): ICC profiles in PNG (`iCCP`), JPG (APP2), WebP and TIFF, matrix/curve and table profiles in RGB or grey (moxcms, already built for the `image` crate), and a PNG's `sRGB`, `gAMA` and `cHRM` chunks; images without a profile, or with one that can't be used (CMYK, damaged, over 4 MB), are taken as sRGB as before. The original file is kept (PDF exports still embed it); pixel exports carry the converted pixels with the file's profile. The image fill's details show the source profile (Unknown — treated as sRGB when there's none) and `image.place` reports it as `imageProfile`. |
| 2026-10-11 | minor | `.fig` import, best effort (gap 1): `.fig` files open (best effort): the zip, its deflate/zstd chunks and the kiwi schema are decoded generically and mapped by field name onto the REST importer, so pages, the layer tree, geometry, paints (styles resolved), strokes, effects, text with styled runs, vectors (as outlines), auto layout (grid included), constraints, components, sets, instances (expanded from their main with overrides, derived layout and property values), styles, variables with modes and bindings, prototyping and image fills from the archive come across; anything else is a warning. Summary ~40% → ~55%; `.fig` read 30–60 h → 8–15 h (write 15–30 h unchanged) |
| 2026-10-11 | minor | Animated GIF fills and TIFF images (gap 12, without video and HEIC): GIF fills keep the original file, show the first frame on the canvas and in exports and play in the presenter and the preview (frames decoded on first play, at most 500 frames and 128 MB; always looping; the window wakes only for a GIF's next frame); TIFF images open, import, place and paste. Paint breadth 85% → 87%, 5–9 h → 4–7 h; gap 12 6–12 h → 5–10 h (video playback and HEIC left, both needing non-Rust decoders); feature breadth (~79%), ready for real work (~47%) and full parity hours unchanged at this rounding |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
