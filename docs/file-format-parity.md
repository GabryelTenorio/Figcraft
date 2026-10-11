# File format parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (export and inspect behaviour suite and fixes; .fig import: editable vector networks and more layer kinds; embedded image colour profiles; `.fig` import, best effort; animated GIF fills and TIFF images; last major: created, formats from the installed Figma's Info.plist and help center, ours from source) · **Target:** Figma Design (desktop app 126.10.7)

Every format Figma reads or writes, with FigCraft's support. Sources: `Info.plist` of
`/Applications/Figma.app` (document types only; nothing inside the bundle's code was read), Figma's
help center (image and video types, export formats), and FigCraft's source.

**Summary (estimated):** ~59% ready. The core interchange formats (SVG, PNG, JPG, PDF) are solid
and tested; Figma's own document, `.fig`, opens (best effort, not yet checked against real-world
files) but can't be written.

## Documents

| Format | Figma | FigCraft read | FigCraft write | Fidelity / testing | Hours |
|---|---|---|---|---|---|
| `.fig` (Figma Design, `com.figma.document`, `application/x-figma`) | native (cloud; "Save local copy") | **yes, best effort** (`file.open`, `file.import`, drop, CLI, MCP) | **no** | `.fig` files open (best effort): the zip, its deflate/zstd chunks and the kiwi schema are decoded generically and mapped by field name onto the REST importer, so pages, the layer tree, geometry, paints (styles resolved), strokes, effects, text with styled runs, vectors as editable vector networks (points, curved segments, filled areas with their winding, per-point corner radius and handle mirroring, end caps; outlines when the network is missing or damaged), auto layout (grid included), constraints, components, sets, instances (expanded from their main with overrides, derived layout and property values), styles, variables with modes and bindings, prototyping (set-variable, set-variable-mode and conditional actions included) and image fills from the archive come across; sticky notes, shapes with text, connectors, tables, code blocks, slides and similar layers come in as the closest frames, text layers and vectors, with a warning; anything else is a warning. Not read: connector labels, table cells beyond their own layers, filled areas with their own fills, per-point stroke joins, variables bound to fields the editor can't bind, fixed or content-sized grid tracks, video fills. Tested on fixtures generated in code (`crates/format/src/fig/tests.rs`, `tail_tests.rs`, fuzzed); no real-world files yet | read tail 5–10, write 15–30 |
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
| PNG | yes, scales/suffixes | yes | export settings per layer, presets (`2x`, `512w`, `512h`), preview, "Export N layers"; contents only (off: every layer showing in the area) and include bounding box (the layer's own box); slashes in layer names export into folders. The exported area is what the layer paints: shadows towards their offset, outside and centre strokes, unclipped children, a mask group's mask, nothing for hidden layers; slices export everything under them; sides round up to whole pixels | — |
| JPG | yes | yes | white where transparent; fixed quality (90) | — |
| SVG | yes (outline text option, ids) | yes | alpha/luminance masks kept; noise, texture, progressive and background blur, glass embedded as images; outline text (off: `<text>` with one `<tspan>` per line, one style per layer), unique id attributes from layer names, simplified strokes (stroke attributes for centred strokes and for inside and outside strokes on boxes and ellipses) | — |
| PDF | yes | yes | one page per frame, real selectable text in subset fonts, links, vectors and gradients as vectors, effects as images; bleed, crop/bleed marks, page boxes, naive CMYK; checked by rendering the PDF back against PNG exports | CMYK output intent 2–4 |
| WebP | — | yes (ours) | | — |
| Copy as CSS / SVG / PNG / text / link | yes | yes | Copy as PNG: the whole selection at 2x | — |
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
| 2026-10-11 | minor | Export and inspect behaviour suite written from the public documentation (139 cases in 19 tests; 47 failed before the fixes) and its fixes: an export covers what the layer paints, so a drop shadow adds room only where it falls, strokes on boxes and ellipses reach their real width (they added up to four times as much), hidden layers take no room and a mask group exports the mask's area; a slice exports everything under it; slashes in layer names export into folders; Copy as PNG copies the whole selection. Added: the export settings' contents only and include bounding box, and for SVG outline text (off writes `<text>`), unique id attributes and simplified strokes (stroke attributes where exact); Inspect colour formats (hex, RGB, CSS, HSL, HSB), CSS in rem, a Blend row, positions measured from the frame around a layer (groups aren't frames), CSS for rotation, blend modes, gradient directions and kinds, text alignment, case and decoration; icons and images in the asset list (`file.exportLayers {assets}`). Summary ~55% → ~56%; SVG options 1–2 h → done |
| 2026-10-11 | minor | `.fig` import's tail (gap 1): vectors open as editable vector networks (points, curved segments, filled areas with their winding, per-point corner radius and handle mirroring, end caps from the end points), falling back to outlines when the network is missing or damaged; sticky notes, shapes with text, connectors, tables, code blocks, slides and similar layers import as the closest frames, text layers and vectors with a warning instead of being dropped; set-variable, set-variable-mode and conditional prototype actions come across. New parsers fuzzed on fixtures generated in code. File formats 55% → 58%, 8–15 h → 5–10 h; import/export breadth 80% → 82%, ready 50% → 54%; feature and area totals and ready for real work (~49%) unchanged; beta ~173–310 h → ~170–305 h; full parity ~321–618 h → ~318–613 h |
| 2026-10-11 | minor | Embedded image colour profiles: placed and opened images are converted once, when decoded, from their own colour space into the file's (sRGB or Display P3): ICC profiles in PNG (`iCCP`), JPG (APP2), WebP and TIFF, matrix/curve and table profiles in RGB or grey (moxcms, already built for the `image` crate), and a PNG's `sRGB`, `gAMA` and `cHRM` chunks; images without a profile, or with one that can't be used (CMYK, damaged, over 4 MB), are taken as sRGB as before. The original file is kept (PDF exports still embed it); pixel exports carry the converted pixels with the file's profile. The image fill's details show the source profile (Unknown — treated as sRGB when there's none) and `image.place` reports it as `imageProfile`. |
| 2026-10-11 | minor | `.fig` import, best effort (gap 1): `.fig` files open (best effort): the zip, its deflate/zstd chunks and the kiwi schema are decoded generically and mapped by field name onto the REST importer, so pages, the layer tree, geometry, paints (styles resolved), strokes, effects, text with styled runs, vectors (as outlines), auto layout (grid included), constraints, components, sets, instances (expanded from their main with overrides, derived layout and property values), styles, variables with modes and bindings, prototyping and image fills from the archive come across; anything else is a warning. Summary ~40% → ~55%; `.fig` read 30–60 h → 8–15 h (write 15–30 h unchanged) |
| 2026-10-11 | minor | Animated GIF fills and TIFF images (gap 12, without video and HEIC): GIF fills keep the original file, show the first frame on the canvas and in exports and play in the presenter and the preview (frames decoded on first play, at most 500 frames and 128 MB; always looping; the window wakes only for a GIF's next frame); TIFF images open, import, place and paste. Paint breadth 85% → 87%, 5–9 h → 4–7 h; gap 12 6–12 h → 5–10 h (video playback and HEIC left, both needing non-Rust decoders); feature breadth (~79%), ready for real work (~47%) and full parity hours unchanged at this rounding |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
