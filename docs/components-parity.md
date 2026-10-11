# Components, variants, styles and variables parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (styles and variables behaviour suite and fixes; components behaviour suite and fixes; team libraries on the server, library usage and swapping a library; extended collections, variables import/export, gradient stops and variant properties bound to variables) · **Target:** Figma Design (desktop app 126.10.7)

Design systems are why teams adopt Figma. **Components: breadth ~96%, ready ~72%. Styles and
variables: breadth ~92%, ready ~72% (estimated).** QA'd with a synthetic design system of ~300
components used 2,000 times (`crates/engine/src/synthetic_ds.rs`, `components_scale_tests.rs`);
not with real libraries. Remaining: 5–11 h here, plus team libraries' tail (4–8 h, partly after
accounts).

## Components and variants

| Feature | Status | Notes |
|---|---|---|
| Create component, instances, detach, reset, go to main, restore main, push changes | done | push from a nested layer goes to the outer main |
| Per-layer overrides with reset; kept across main edits, nested swaps, copy/paste | done | main edits match a full rebuild exactly at 2,000 instances |
| What an instance overrides: fills, strokes, effects, visibility, layer names, text content and every text property (resizing included), auto layout padding, gap and alignment, layout grids, export settings, prototype interactions, nested swaps and property values | done | each kept when the main changes that property, reset per group; opacity and corner radius too (documentation silent) |
| What it doesn't: position, rotation, size, constraints and resizing of layers inside it, its auto layout direction and other settings, its structure | done | refused with a message, the fields greyed out; the instance itself resizes and its layers follow the main's constraints |
| Component sets / variants, add variant, switch | done | |
| Switching variants keeps overrides where names match and both variants had the same value; text layers keep theirs | done | |
| Default variant: the top-left one (new instances, Assets thumbnail, generated code) | done | |
| Variant property order (names rewritten), duplicate combinations reported | done | reordering through `component.editProperty {index}`; no drag in the panel yet |
| Properties: boolean, text, instance swap (preferred values), variant | done | |
| Slots (preferred components, fill menu, kept across updates) | done | Figma 2025 feature |
| Exposed nested instances | done | hidden ones hide their properties |
| Descriptions | done | |
| Instance swap: panel, Alt-drop from Assets | done | the panel keeps overrides on same-named layers, an Alt-drop from Assets only text layers' |
| Assets panel: thumbnails, search, drag into frames and auto layout slots | done | virtualized |
| Instances pasted into other files follow their mains | done | missing main restorable |
| Component playground (try properties without placing) | missing | 1–2 h |
| Real-world libraries QA | **not checked** | [gaps](gaps.md) #3 |

The behaviour of instances, overrides, variants and swaps is checked against the public
documentation by `crates/engine/src/components_behaviour_tests.rs` (one table row per documented
rule, with its derivation). Where the documentation is silent, the suite pins today's behaviour:
layer opacity and corner radius are overridable; rotation of a layer inside an instance is refused
with its position; boolean and text property values stay across variants; a missing combination
picks the variant matching the most values; reset all keeps the instance's size; an instance's own
min / max width and height stay its own. An instance's own export settings and interactions are
its own (the main's later changes to them don't reach existing instances, so connections people
added are never lost). Nested instance overrides: `nested_override_tests.rs`; accepting library
updates with overrides kept: `library_tests.rs`, `components_scale_tests.rs`.

## Libraries

| Feature | Status | Notes |
|---|---|---|
| Publish a file's components, sets, styles and variables (per-asset versions) | done | as local library files |
| Enable a library in a file; read-only linked copies | done | |
| Library updates available; overrides kept | done | |
| Team libraries: publish to a `figcraft-server` under the file, enable by a share link (a view link reads it), background update checks | done | [server.md](server.md#team-libraries); no team-wide library list or accounts ([gaps](gaps.md) #10) |
| Library analytics | partial | instances per component and the files using them, as apps report them; nothing over time, no detach counts |
| Swap library (instances, styles and variables to another library's of the same names, overrides kept) | done | what has no match stays and is listed; one undo step |

## Styles and variables

| Feature | Status | Notes |
|---|---|---|
| Paint, text, effect, grid styles; style picker; text styles on character ranges | done | behaviour suite: `styles_behaviour_tests.rs` |
| Editing a style updates its layers; detach and delete keep values; editing a styled property unlinks that style | done | an edited fill/stroke/effect/grid list or text chars unlink; applying a style replaces variables bound to what it sets; text styles on text layers only |
| Organising styles: folders by `/` names, order, duplicate, rename folder, Local styles list | done | `style.move` (into a folder renames), `style.duplicate` (right below), `style.renameFolder`; the page's Local styles list by type and folder (drag to reorder, right-click) |
| Variables inside styles (colour variables in paint and shadow styles) | missing | 2–3 h |
| Variables: colour, number, string, boolean; collections, modes, aliases, scopes, groups | done | behaviour suite: `variables_behaviour_tests.rs`; aliases same type, no loops (also on create); scopes filter pickers only |
| Modes: add, duplicate, rename, delete, set as default / reorder | done | layers on Auto follow a new default; explicit modes stay |
| Deleting a bound variable or its collection | done | layers keep their last value and are unbound; aliases to it take its values |
| Code syntax per platform (Web, iOS, Android) | done | generated CSS uses the Web name; in the variables JSON; SwiftUI/Compose code doesn't name variables yet |
| Bindings: fills, strokes, effects, opacity, padding, gaps, sizes, radius, font size/weight, line height, letter spacing, paragraph spacing/indent, text, visibility, layout grids, instance properties | done | type-checked per field; number variables fill text content |
| Explicit modes per frame, library collections' modes | done | |
| Gradient stops and variant properties bound to variables | done | a stop's binding follows it when earlier stops are removed |
| Extended collections | done | library collections; overrides per mode with reset; follow library updates; modes inherited. New library variables arrive with the next other update ([gaps](gaps.md) #14) |
| Variables import/export (JSON) | done | the REST local variables shape (round-trips; merges by collection, mode and variable name); CSS custom properties export |
| Expressions in prototype set-variable actions | done | see [prototyping-parity](prototyping-parity.md) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Styles and variables behaviour suite from the public documentation (18 tests) and its fixes (styled properties edited away unlink, deleted variables unbind with values kept, type-checked bindings, alias checks on create, text styles on text only); style folders, order and duplicates with a Local styles list; mode duplicate and default; code syntax. Found missing: variables inside styles. Styles and variables breadth ~92% → ~94%, ready ~72% → ~76%; remaining here 7–14 h → 9–17 h |
| 2026-10-11 | minor | Behaviour suite from the public documentation and its fixes: overrides of padding, gap, alignment, layout grids, export settings, interactions and every text property kept (they were lost when the main changed); geometry, constraints and resizing of layers inside instances refused; variant switches keep only overrides both variants agreed on (text layers keep theirs); Alt-drops from Assets keep only text overrides; top-left default variant; hidden exposed instances hide their properties; push from nested layers goes to the outer main; variant property reordering and conflict warnings. Components ~95%/~68% → ~96%/~72%; remaining here 7–14 h → 5–11 h |
| 2026-10-11 | minor | Team libraries on `figcraft-server` (publish under the file, enable by a share link, updates with overrides kept), usage counts per component and file, and swapping a library by names (gap 10) |
| 2026-10-11 | minor | Extended collections, variables import/export (JSON, CSS) and gradient stop and variant property bindings landed: styles and variables breadth ~85% → ~92%, ready ~70% → ~72%; remaining here 10–20 h → 7–14 h |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
