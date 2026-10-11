# Prototyping parity

> **Last reviewed:** 2026-10-11 · **Last updated:** 2026-10-11 · **Change:** minor (behaviour suite written from the public documentation, every rule's status; fixes and the missing setting it found) · **Target:** Figma Design (desktop app 126.10.7)

**Breadth ~90%, ready ~74% (estimated).** Every Figma trigger except media and gamepad, every
action except media control, every transition and easing exist. Every rule of the public
documentation on triggers, actions, overlays, animations, easing, scrolling and interactive
components is a test case with what it implies (`crates/engine/src/proto_behaviour_tests.rs`, 7
tests on the model, and `crates/ui-egui/src/present_behaviour_tests.rs`, 12 tests driving the
player, 21 gestures among them), and all pass. Not yet checked against larger real prototypes.
Remaining: 3–6 h.

| Group | Figma | FigCraft | Missing |
|---|---|---|---|
| Triggers | On click, on drag, while hovering, while pressing, key/gamepad, mouse enter, mouse leave, mouse down, mouse up, after delay, on media hit, on media end | 10: OnClick, OnHover, OnPress, OnDrag, MouseEnter, MouseLeave, MouseDown, MouseUp, AfterDelay, KeyPress (keys and combinations) | gamepad, media triggers, delays on mouse enter/leave/down/up |
| Actions | Navigate to, change to, back, scroll to, open link, open / swap / close overlay, set variable, set variable mode, conditional, play/pause/mute/seek media | 11: Navigate, Back, ScrollTo (with an offset), OpenOverlay, SwapOverlay, CloseOverlay, OpenUrl, ChangeTo, SetVariable, SetVariableMode, Conditional | media actions |
| Transitions | Instant, dissolve, smart animate, move in/out, push, slide in/out | all 8 | — |
| Easing | Linear, ease in/out/in-and-out, back variants, gentle, quick, bouncy, slow, custom bezier, custom spring | all 13 (springs physical) | — |

## Rules from the documentation

Status: **pass** (a case in the suite holds), **fixed** (the suite found a divergence, now fixed),
**added** (the setting was missing and now exists), **silent** (the documentation doesn't say; the
case pins the current behaviour).

| Rule | Status | Evidence / notes |
|---|---|---|
| On click: press and release on the layer; a drag is not a click | pass | `triggers_fire_on_their_gesture` |
| On drag: a drag that starts on the layer; a click is not a drag | pass | same |
| On drag with a directional animation (move, push, slide): only a drag that way starts it; a drag another way can scroll instead | **fixed** | any drag started it; `on_drag_starts_only_a_drag_its_animation_goes` (exactly diagonal counts for both axes: silent) |
| On drag: the animation it starts follows the pointer while it stays down, then plays on from there | **fixed** | it played by time; `a_drag_started_transition_follows_the_pointer` |
| Mouse down / mouse up: as the button goes down / comes up | pass | `triggers_fire_on_their_gesture` |
| While pressing: shown while the button is down, reverts on release | pass | same |
| While hovering: shown while the pointer is over the layer, reverts when it leaves; mouse enter fires once and doesn't revert; mouse leave fires on leaving | pass | same |
| Key/gamepad: a key or a combination of keys, pressed anywhere on the screen | **fixed** | combinations never fired (`Shift+K` didn't match) and `K` also fired for Shift+K; the editor's recorder now records the modifiers held, and REST files' modifier key codes import as a combination (they imported as the key alone); `key_triggers_fire_for_their_key_or_combination` |
| After delay: timed from when the screen shows | pass | `triggers_fire_on_their_gesture` |
| After delay on an overlay: timed from when the overlay opens, again each time it opens | **fixed** | timed from when the screen under it arrived (a toast opened late closed at once) and fired only the first time; `an_overlays_after_delay_counts_from_when_it_opens` |
| Nested layers with the same trigger: the top-most layer's interaction wins; a layer without that trigger lets it through to the one under it | pass | `nested_hotspots_and_click_through`, `hotspots_priority_and_click_through` |
| A layer without interactions over a hotspot | silent | it doesn't block it: clicks reach the hotspot under it |
| Hidden layers aren't hotspots; clipping frames cut their hotspots | pass | same |
| Navigate to: replaces the screen; from an overlay it closes the overlays; preserve scroll position keeps the offset (as far as the destination scrolls) | pass | `overlays_stack_swap_close_and_go_with_navigation`, `navigating_resets_scroll_unless_it_preserves_it` |
| Back: closes the top overlay first, else returns to the previous screen playing its animation backwards; restart empties the history | pass | `back_plays_the_way_in_backwards_and_restart_starts_over`, `back_and_reverting_play_the_animation_the_other_way` |
| Scroll to: every frame around the layer scrolls to bring it in, animated with the interaction's animation | pass | `scroll_to_moves_every_scrolling_frame_around_the_layer` |
| Scroll to with an offset: the layer stops that far from the top-left (room for a fixed header) | **added** | `offset: [x, y]` on the action, X / Y fields in the interaction editor; `scroll_to_brings_the_layer_to_the_top_less_its_offset` (nested scrolling frames: the innermost keeps the offset; silent) |
| Open overlay: positions, background dimming, close on a click outside (on by default, can be turned off) | pass | `a_click_outside_an_overlay_closes_it_unless_turned_off`, `overlay_positions` |
| Overlays stack; swap overlay replaces the top one; close overlay closes it | pass | `overlays_stack_swap_close_and_go_with_navigation` |
| Manual overlay position relative to the hotspot | silent | below the layer that opened it; no offset is stored yet |
| Open link | pass | always a new tab or browser window (opening in place would leave the prototype) |
| Change to: another variant of the instance, in place, with the interaction's animation | pass | `interactive_components_change_in_place` |
| Interactive components: every instance plays the interactions of the variant it shows; an interaction on the instance with the same trigger wins | **fixed** | instances played only interactions copied onto them when they were placed: an instance placed before its main got interactions did nothing, and one placed after kept the first variant's interactions after changing (a toggle went on but never off). New instances no longer copy them, and copies in older files give way to the variant shown; `instances_play_their_variants_interactions` |
| Change to on a layer inside a nested instance changes the instance whose variant set holds the destination | **fixed** | the innermost instance (an icon inside a button) was changed instead |
| Set variable, set variable mode, conditional | pass | `proto_run_tests`, `proto_conditional_tests`, `present_variables_tests` |
| Smart animate: layers matched by name and hierarchy tween position, size, rotation, opacity, solid fills and strokes and corner radius; unmatched layers dissolve | pass | `smart_animate_tweens_matched_layers_and_overshoots_with_the_easing`, `smart_interpolate_tweens_matched_and_fades_the_rest` |
| Smart animate with a spring or back easing overshoots (and pulls back) | **fixed** | layers snapped to their end once the curve passed 1 |
| Visibility, text content, gradients and images in smart animate | silent | they show the destination's |
| Move in/out, push, slide in/out move the way their direction names; reversed they go the other way | pass | `back_and_reverting_play_the_animation_the_other_way`, `transitions_move_the_right_way` |
| Easing curves: linear, ease in, ease out, ease in and out, back variants, custom bezier | pass | `easing_curves_and_springs` |
| Spring presets are physical springs (mass 1; stiffness / damping: gentle 100/15, quick 300/20, bouncy 600/15, slow 80/20) | **fixed** | they were ad-hoc curves |
| A spring plays until it settles: its duration follows from its mass, stiffness and damping, not from the duration field | **fixed** | it was squeezed into the duration field's time; the editor shows the spring's duration in place of the field; `springs_play_until_they_settle` |
| Scrolling: horizontal, vertical or both; momentum; fixed layers stay put; sticky layers stop at the edge | pass | `scroll_offsets_fixed_and_sticky`, `present_view_tests` (sticky on the left edge too: silent) |
| Fixed layers draw and take clicks above the content scrolling under them, wherever they are in the layer list | **fixed** | they drew in layer order, under later layers; the file's order doesn't change; `fixed_layers_stay_above_the_content_that_scrolls` |
| Flows and starting points; device frames; presentation options; hotspot hints | pass | `proto_present_tests`, `present_view_tests`, `present_tests` |

## Features

| Feature | Status | Notes |
|---|---|---|
| Connections drawn and dragged on the canvas; interaction editor | done | |
| Several actions per interaction; drag to reorder; conditional blocks with else-if/else | done | one undo step per edit |
| Expressions (`count + 1`, `mode(Theme) == "Dark"`, `accent[Dark]`) | done | checked when entered |
| Flows, starting points, descriptions | done | |
| Overlays: position, close on click outside, background, animate out, stacking, timers from when they open | done | |
| Scroll behaviour: overflow directions, fixed (drawn on top), sticky, preserve scroll position, scroll to with an offset, momentum and overscroll | done | |
| Key combinations (`Shift+K`), recorded in the editor and imported from REST files | done | |
| Interactive components: instances play their variant's interactions, overridden per trigger | done | |
| Drag-driven animations that follow the pointer, in their direction | done | |
| Presenter: device frames and colours, rotation, custom sizes, fit/fill/100%, responsive frames | done | device bodies drawn in code |
| Hotspot hints, inspect panel in the presenter, comments in the presenter | done | |
| Preview pane following edits live | done | |
| Edit interactions on several layers at once | done | |
| Delays on mouse enter / leave / down / up; manual overlay offsets; opening a link in place | missing | the REST import warns about mouse-trigger delays |
| Prototype in a separate window; mirror to a phone | missing | see [hardware-parity](hardware-parity.md) |
| Video in prototypes | missing | needs video fills ([gaps](gaps.md) #12, #15) |
| Share a prototype link | missing | needs multiplayer/sharing |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Behaviour suite from the public documentation (7 model tests, 12 player tests with 21 gestures) and its fixes: on drag follows its animation's direction and the pointer, key combinations, overlay timers from when they open, interactive components play their variant's interactions and change the right instance, physical springs that play until they settle, smart animate overshoots with the easing, fixed layers above scrolling content; added scroll to offsets. Breadth 88% → 90%, ready 70% → 74%, 5–9 h → 3–6 h |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
