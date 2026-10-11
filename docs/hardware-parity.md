# Hardware and platform parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created) · **Target:** Figma Design (desktop app 126.10.7)

How FigCraft uses hardware and which platforms it runs on, against Figma. Figma's desktop app is
Electron with a GPU helper process; its canvas renders on the GPU (WebGL, with WebGPU rolling out
since 2025). FigCraft renders the canvas on the CPU and presents it through wgpu.

**Summary (estimated):** hardware ~50%, performance ~60%, platforms ~75%.

## Hardware

| Feature | Figma | FigCraft (macOS / Windows / Linux / web) | Hours |
|---|---|---|---|
| Canvas rendering | GPU | CPU (tiny-skia), banded across cores, retained (re-renders only what an edit or pan touches), levels of detail below 50% zoom; shown through wgpu (WebGPU or WebGL2 in the browser) | 30–60 for a GPU path |
| Effects (blurs, glass, noise) | GPU | CPU | in the above |
| HiDPI / Retina | yes | yes (all) | — |
| Wide-gamut display (Display P3) | yes, on capable displays | colours stored and exported in P3; the screen shows sRGB (egui) | 3–6 |
| Trackpad pinch zoom and two-finger pan | yes | yes (egui zoom delta, smooth scroll) | — |
| Pen tablet pressure (pencil, Figma Draw brushes) | yes (forum reports of regressions in 2025) | no | 3–6 |
| Several windows / multi-monitor | yes (a window per file; presenter in its own window) | one window with file tabs; presenter inside it | 4–8 |
| Screen readers | partial | AccessKit tree on desktop (names, roles, states), never tested with a real screen reader; none in the browser | 4–8 + human QA |
| Keyboard layouts (non-US) | yes | keymap uses logical keys; not checked on AZERTY/QWERTZ/JIS | 1–2 |

## Performance (measured by `cargo xtask bench`, synthetic files, owner's Mac)

| Case | FigCraft | Figma |
|---|---|---|
| 50k-layer page, zoomed out, full render | ~8 ms (levels of detail) | not timed side by side |
| 10k layers: full 1440×900 render | 7–15 ms | not timed |
| 10k layers: move one layer, re-render | ~0.2 ms | not timed |
| Design system of ~300 components × 2,000 instances: recolour a main used ~900 times | 4–18 ms | not timed |
| `.figcraft` at 50k layers: save / load | ~0.09 s / ~0.16 s | n/a |
| Idle window | 0 frames, ~0% CPU | — |

Nothing has been timed against Figma on the same document (needs `.fig` import). 

## Platforms

| Platform | Figma | FigCraft | Notes |
|---|---|---|---|
| macOS | desktop app (Intel and Apple silicon) | universal `.dmg`, signed and notarized (v0.2.0) | where nearly all QA happens |
| Windows x64 / arm64 | desktop app | `.msi` and portable zip for x64, x86 and arm64 | built in CI; little hands-on QA |
| Linux | none (browser only) | AppImage, `.deb`, `.rpm`, Flatpak, tarball, x86_64 and aarch64; riscv64 tarball | CI tests run here |
| FreeBSD | none | tarball | |
| Web | primary (figma.com) | `figcraft-web` (WASM, WebGPU/WebGL2), web zip | no accessibility tree; file access through the browser |
| iOS / Android | apps: view, comment, mirror prototypes | none (the web build is not touch-first) | 20–40 h for a viewer |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
