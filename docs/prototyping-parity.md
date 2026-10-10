# Prototyping parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created; triggers, actions, transitions and easings counted from `crates/doc/src/proto.rs`) · **Target:** Figma Design (desktop app 126.10.7)

**Breadth ~88%, ready ~70% (estimated).** Every Figma trigger except media and gamepad, every
action except media control, every transition and easing exist (measured from the model enums on
2026-10-10). Not yet checked against larger real prototypes. Remaining: 5–9 h.

| Group | Figma | FigCraft | Missing |
|---|---|---|---|
| Triggers | On click, on drag, while hovering, while pressing, key/gamepad, mouse enter, mouse leave, mouse down, mouse up, after delay, on media hit, on media end | 10: OnClick, OnHover, OnPress, OnDrag, MouseEnter, MouseLeave, MouseDown, MouseUp, AfterDelay, KeyPress | gamepad, media triggers |
| Actions | Navigate to, change to, back, scroll to, open link, open / swap / close overlay, set variable, set variable mode, conditional, play/pause/mute/seek media | 11: Navigate, Back, ScrollTo, OpenOverlay, SwapOverlay, CloseOverlay, OpenUrl, ChangeTo, SetVariable, SetVariableMode, Conditional | media actions |
| Transitions | Instant, dissolve, smart animate, move in/out, push, slide in/out | all 8 | — |
| Easing | Linear, ease in/out/in-and-out, back variants, gentle, quick, bouncy, slow, custom bezier, custom spring | all 13 | — |

| Feature | Status | Notes |
|---|---|---|
| Connections drawn and dragged on the canvas; interaction editor | done | |
| Several actions per interaction; drag to reorder; conditional blocks with else-if/else | done | one undo step per edit |
| Expressions (`count + 1`, `mode(Theme) == "Dark"`, `accent[Dark]`) | done | checked when entered |
| Flows, starting points, descriptions | done | |
| Overlays: position, close on click outside, background, animate out | done | |
| Scroll behaviour: overflow directions, fixed, sticky, preserve scroll position, momentum and overscroll | done | |
| Presenter: device frames and colours, rotation, custom sizes, fit/fill/100%, responsive frames | done | device bodies drawn in code |
| Hotspot hints, inspect panel in the presenter, comments in the presenter | done | |
| Preview pane following edits live | done | |
| Edit interactions on several layers at once | done | |
| Prototype in a separate window; mirror to a phone | missing | see [hardware-parity](hardware-parity.md) |
| Video in prototypes | missing | needs video fills ([gaps](gaps.md) #12, #15) |
| Share a prototype link | missing | needs multiplayer/sharing |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
