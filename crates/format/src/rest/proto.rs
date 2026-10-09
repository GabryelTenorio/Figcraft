//! Prototyping: layers' `interactions` (triggers, actions, transitions and easing) and the legacy
//! `transitionNodeID`, scrolling (`overflowDirection`, `scrollBehavior`), and the pages'
//! `prototypeDevice`, `prototypeBackgrounds` and `prototypeStartNodeID`.

use figcraft_doc::proto::{
    Action, Direction, Easing, Interaction, Overflow, OverlayPosition, PrototypeSettings, ScrollPosition, Transition, Trigger,
};
use figcraft_doc::{Node, NodeId, VariableValue};
use serde_json::{Value, json};

use super::Ctx;
use super::props::{color, flag, list, num, text};
use super::variables;

/// Longest transition or delay kept (ms).
const MAX_MS: f64 = 60_000.0;
/// Deepest nesting of conditional blocks followed.
const MAX_ACTION_DEPTH: usize = 8;
/// Longest URL kept (characters).
const MAX_URL: usize = 4096;
/// Default transition length when the file names none (ms).
const DEFAULT_MS: u32 = 300;

/// Scrolling of a frame and how a layer behaves while its parent scrolls.
pub(super) fn scrolling(n: &mut Node, j: &Value) {
    n.overflow = match text(j, "overflowDirection") {
        Some("HORIZONTAL_SCROLLING") => Overflow::Horizontal,
        Some("VERTICAL_SCROLLING") => Overflow::Vertical,
        Some("HORIZONTAL_AND_VERTICAL_SCROLLING") => Overflow::Both,
        _ => Overflow::None,
    };
    n.scroll_position = match text(j, "scrollBehavior") {
        Some("FIXED") => ScrollPosition::Fixed,
        Some("STICKY_SCROLLS") => ScrollPosition::Sticky,
        Some(_) => ScrollPosition::Scrolls,
        // The deprecated flag, for files written before `scrollBehavior`.
        None if flag(j, "isFixed") == Some(true) => ScrollPosition::Fixed,
        None => ScrollPosition::Scrolls,
    };
}

/// A page's prototype device and background, when the file sets them.
pub(super) fn page_settings(c: &Value) -> Option<PrototypeSettings> {
    let device = c.get("prototypeDevice").filter(|d| d.is_object());
    let background = list(c, "prototypeBackgrounds").first().and_then(color).filter(|b| b.a > 0.0);
    if device.is_none() && background.is_none() {
        return None;
    }
    let mut s = PrototypeSettings::default();
    if let Some(d) = device {
        s.device = device_name(d).to_string();
    }
    if let Some(b) = background {
        s.background = b.with_alpha(1.0);
    }
    Some(s)
}

/// The closest of our device frames: preset identifiers by the device family they name, custom
/// sizes by their shorter side.
fn device_name(d: &Value) -> &'static str {
    let size = d.get("size").and_then(|s| Some((num(s, "width")?, num(s, "height")?)));
    let by_size = || match size {
        Some((w, h)) if w > 0.0 && h > 0.0 => {
            let (short, long) = (w.min(h), w.max(h));
            if short <= 500.0 {
                if long >= 900.0 { "iPhone 16 Pro Max" } else { "iPhone 16" }
            } else if long <= 1400.0 && long / short < 1.55 {
                // Tablets are squarer than laptop and desktop screens.
                "iPad mini"
            } else {
                "Desktop"
            }
        }
        _ => "None",
    };
    match text(d, "type") {
        Some("PRESET") => {
            let id = text(d, "presetIdentifier").unwrap_or("").to_ascii_uppercase();
            let has = |words: &[&str]| words.iter().any(|w| id.contains(w));
            if has(&["IPAD", "TABLET", "GALAXY_TAB", "SURFACE_PRO"]) {
                "iPad mini"
            } else if has(&["MACBOOK", "LAPTOP"]) {
                "MacBook Air"
            } else if has(&["DESKTOP", "IMAC", "MONITOR", "BROWSER", "TV"]) {
                "Desktop"
            } else if has(&["ANDROID", "PIXEL", "GALAXY"]) {
                "Android Compact"
            } else if has(&["IPHONE"]) {
                if has(&["MAX", "PLUS"]) { "iPhone 16 Pro Max" } else { "iPhone 16" }
            } else {
                by_size()
            }
        }
        Some("CUSTOM") => by_size(),
        _ => "None",
    }
}

impl Ctx<'_> {
    /// The layer's interactions (`interactions`, else the legacy `transitionNodeID`), with
    /// destinations resolved to imported layers.
    pub(super) fn interactions(&mut self, j: &Value) -> Vec<Interaction> {
        let list_ = list(j, "interactions");
        if list_.is_empty() {
            return self.legacy_transition(j).into_iter().collect();
        }
        list_.iter().filter_map(|it| self.interaction(it)).collect()
    }

    fn legacy_transition(&mut self, j: &Value) -> Option<Interaction> {
        let dest = self.dest(text(j, "transitionNodeID")?)?;
        let ms = num(j, "transitionDuration").map(|d| d.clamp(0.0, MAX_MS) as u32).unwrap_or(0);
        Some(Interaction {
            trigger: Trigger::OnClick,
            actions: vec![Action::Navigate { dest, preserve_scroll: false }],
            transition: if ms > 0 { Transition::Dissolve } else { Transition::Instant },
            duration_ms: if ms > 0 { ms } else { DEFAULT_MS },
            easing: easing_type(text(j, "transitionEasing"), None),
        })
    }

    fn interaction(&mut self, it: &Value) -> Option<Interaction> {
        let trigger = self.trigger(it.get("trigger").filter(|t| t.is_object())?)?;
        let mut actions = Vec::new();
        let mut transition: Option<&Value> = None;
        for a in list(it, "actions") {
            self.action(a, &mut actions, &mut transition, 0);
        }
        if actions.is_empty() {
            self.warn("prototype interactions without supported actions were skipped");
            return None;
        }
        let (transition, duration_ms, easing) = match transition {
            Some(t) => self.transition(t),
            None => (Transition::Instant, DEFAULT_MS, Easing::default()),
        };
        Some(Interaction { trigger, actions, transition, duration_ms, easing })
    }

    fn trigger(&mut self, t: &Value) -> Option<Trigger> {
        let mouse = |cx: &mut Self, tr: Trigger| {
            if num(t, "delay").is_some_and(|d| d > 0.0) {
                cx.warn("delays on mouse enter/leave/down/up triggers are not supported and were dropped");
            }
            Some(tr)
        };
        match text(t, "type") {
            Some("ON_CLICK") => Some(Trigger::OnClick),
            Some("ON_HOVER") => Some(Trigger::OnHover),
            Some("ON_PRESS") => Some(Trigger::OnPress),
            Some("ON_DRAG") => Some(Trigger::OnDrag),
            Some("AFTER_TIMEOUT") => Some(Trigger::AfterDelay { ms: num(t, "timeout").unwrap_or(0.0).clamp(0.0, MAX_MS) as u32 }),
            Some("MOUSE_ENTER") => mouse(self, Trigger::MouseEnter),
            Some("MOUSE_LEAVE") => mouse(self, Trigger::MouseLeave),
            Some("MOUSE_DOWN") => mouse(self, Trigger::MouseDown),
            Some("MOUSE_UP") => mouse(self, Trigger::MouseUp),
            Some(ty @ ("ON_KEY_DOWN" | "ON_KEY_UP")) => {
                if text(t, "device").is_some_and(|d| d != "KEYBOARD") {
                    self.warn("gamepad triggers are not supported and were skipped");
                    return None;
                }
                let codes: Vec<f64> = list(t, "keyCodes").iter().filter_map(Value::as_f64).collect();
                let Some(key) = codes.iter().rev().find(|c| !is_modifier(**c)).and_then(|c| key_name(*c)) else {
                    self.warn("key triggers without a supported key were skipped");
                    return None;
                };
                if codes.iter().any(|c| is_modifier(*c)) {
                    self.warn("key triggers with modifier keys import as the key alone");
                }
                if ty == "ON_KEY_UP" {
                    self.warn("key up triggers import as key presses");
                }
                Some(Trigger::KeyPress { key })
            }
            Some("ON_MEDIA_HIT" | "ON_MEDIA_END") => {
                self.warn("video triggers are not supported and were skipped");
                None
            }
            _ => {
                self.warn("prototype triggers of unknown types were skipped");
                None
            }
        }
    }

    /// Append the action `a` maps to (conditional blocks without a condition are inlined).
    fn action<'v>(&mut self, a: &'v Value, out: &mut Vec<Action>, transition: &mut Option<&'v Value>, depth: usize) {
        let mapped = match text(a, "type") {
            Some("BACK") => Some(Action::Back),
            Some("CLOSE") => Some(Action::CloseOverlay),
            Some("URL") => text(a, "url").filter(|u| !u.trim().is_empty()).map(|u| Action::OpenUrl { url: u.chars().take(MAX_URL).collect() }),
            Some("NODE") => {
                let mapped = self.node_action(a);
                if mapped.is_some() && transition.is_none() {
                    *transition = a.get("transition").filter(|t| t.is_object());
                }
                mapped
            }
            Some("SET_VARIABLE") => self.set_variable(a),
            Some("SET_VARIABLE_MODE") => {
                self.warn("prototype actions that change variable modes are not supported and were skipped");
                None
            }
            Some("CONDITIONAL") => {
                let blocks = list(a, "conditionalBlocks");
                match blocks {
                    // A lone block without a condition always runs.
                    [only] if only.get("condition").is_none_or(Value::is_null) && depth < MAX_ACTION_DEPTH => {
                        for inner in list(only, "actions") {
                            self.action(inner, out, transition, depth + 1);
                        }
                    }
                    _ => self.warn("conditional prototype actions are not supported and were skipped"),
                }
                None
            }
            Some("UPDATE_MEDIA_RUNTIME") => {
                self.warn("video actions are not supported and were skipped");
                None
            }
            _ => {
                self.warn("prototype actions of unknown types were skipped");
                None
            }
        };
        out.extend(mapped);
    }

    fn node_action(&mut self, a: &Value) -> Option<Action> {
        let Some(dest) = text(a, "destinationId").and_then(|d| self.dest(d)) else {
            self.warn("prototype actions without a destination in the file were skipped");
            return None;
        };
        Some(match text(a, "navigation") {
            Some("NAVIGATE") | None => {
                Action::Navigate { dest, preserve_scroll: a.get("preserveScrollPosition").and_then(Value::as_bool).unwrap_or(false) }
            }
            Some("SWAP") => Action::SwapOverlay { dest },
            Some("SCROLL_TO") => Action::ScrollTo { dest },
            Some("CHANGE_TO") => Action::ChangeTo { dest },
            Some("OVERLAY") => {
                let meta = text(a, "destinationId").and_then(|d| self.overlay_meta.get(d)).copied();
                let position = meta.and_then(|m| text(m, "overlayPositionType")).and_then(overlay_position).unwrap_or_default();
                // Overlays close on a click outside unless the destination says otherwise.
                let close_on_outside = meta.and_then(|m| text(m, "overlayBackgroundInteraction")).is_none_or(|i| i == "CLOSE_ON_CLICK_OUTSIDE");
                let background = meta
                    .and_then(|m| m.get("overlayBackground"))
                    .filter(|b| text(b, "type") == Some("SOLID_COLOR"))
                    .and_then(|b| b.get("color"))
                    .and_then(color);
                Action::OpenOverlay { dest, position, close_on_outside, background }
            }
            Some(_) => {
                self.warn("prototype navigations of unknown types were skipped");
                return None;
            }
        })
    }

    fn set_variable(&mut self, a: &Value) -> Option<Action> {
        let Some((id, kind)) = text(a, "variableId").and_then(|v| self.var_kinds.get_key_value(v)) else {
            self.warn("set-variable actions on variables that aren't in the variables JSON were skipped");
            return None;
        };
        let id = id.clone();
        let kind = *kind;
        let value = a.get("variableValue").and_then(|d| d.get("value")).and_then(|v| variables::value(kind, v));
        let value = match value {
            Some(VariableValue::Color(c)) => json!(c.to_css_hex()),
            Some(VariableValue::Number(n)) => json!(n),
            Some(VariableValue::String(s)) => json!(s),
            Some(VariableValue::Boolean(b)) => json!(b),
            Some(VariableValue::Alias(_)) | None => {
                self.warn("set-variable actions with expressions or variable references are not supported and were skipped");
                return None;
            }
        };
        Some(Action::SetVariable { variable: id, value })
    }

    /// The imported layer a REST id names.
    fn dest(&self, rest_id: &str) -> Option<NodeId> {
        self.ids.get(rest_id).copied()
    }

    /// Transition, duration (ms) and easing of a `transition` object.
    fn transition(&mut self, t: &Value) -> (Transition, u32, Easing) {
        let direction = match text(t, "direction") {
            Some("RIGHT") => Direction::Right,
            Some("TOP") => Direction::Top,
            Some("BOTTOM") => Direction::Bottom,
            _ => Direction::Left,
        };
        let tr = match text(t, "type") {
            Some("DISSOLVE") => Transition::Dissolve,
            // Animated scrolling: any animated transition scrolls smoothly.
            Some("SMART_ANIMATE" | "SCROLL_ANIMATE") => Transition::SmartAnimate,
            Some("MOVE_IN") => Transition::MoveIn { direction },
            Some("MOVE_OUT") => Transition::MoveOut { direction },
            Some("PUSH") => Transition::Push { direction },
            Some("SLIDE_IN") => Transition::SlideIn { direction },
            Some("SLIDE_OUT") => Transition::SlideOut { direction },
            _ => {
                self.warn("prototype transitions of unknown types play instantly");
                Transition::Instant
            }
        };
        let ms = num(t, "duration").map_or(DEFAULT_MS, |d| d.clamp(0.0, MAX_MS) as u32);
        let e = t.get("easing");
        (tr, ms, easing_type(e.and_then(|e| text(e, "type")), e))
    }
}

/// An easing curve by its type name, with custom parameters from `e`.
fn easing_type(ty: Option<&str>, e: Option<&Value>) -> Easing {
    let field = |obj: &str, k: &str| e.and_then(|e| e.get(obj)).and_then(|o| num(o, k));
    match ty {
        Some("LINEAR") => Easing::Linear,
        Some("EASE_IN") => Easing::EaseIn,
        Some("EASE_IN_AND_OUT") => Easing::EaseInOut,
        Some("EASE_IN_BACK") => Easing::EaseInBack,
        Some("EASE_OUT_BACK") => Easing::EaseOutBack,
        Some("EASE_IN_AND_OUT_BACK") => Easing::EaseInOutBack,
        Some("GENTLE" | "GENTLE_SPRING") => Easing::Gentle,
        Some("QUICK") => Easing::Quick,
        Some("BOUNCY") => Easing::Bouncy,
        Some("SLOW") => Easing::Slow,
        Some("CUSTOM_CUBIC_BEZIER") => {
            let b = |k: &str| field("easingFunctionCubicBezier", k);
            match (b("x1"), b("y1"), b("x2"), b("y2")) {
                (Some(x1), Some(y1), Some(x2), Some(y2)) => {
                    Easing::CustomBezier { x1: x1.clamp(0.0, 1.0), y1: y1.clamp(-10.0, 10.0), x2: x2.clamp(0.0, 1.0), y2: y2.clamp(-10.0, 10.0) }
                }
                _ => Easing::CUSTOM_BEZIER,
            }
        }
        Some("CUSTOM_SPRING") => {
            let s = |k: &str| field("easingFunctionSpring", k);
            match (s("mass"), s("stiffness"), s("damping")) {
                (Some(m), Some(k), Some(d)) if m > 0.0 && k > 0.0 && d >= 0.0 => {
                    Easing::CustomSpring { mass: m.min(1e4), stiffness: k.min(1e6), damping: d.min(1e6) }
                }
                _ => Easing::CUSTOM_SPRING,
            }
        }
        _ => Easing::EaseOut,
    }
}

fn overlay_position(s: &str) -> Option<OverlayPosition> {
    Some(match s {
        "CENTER" => OverlayPosition::Center,
        "TOP_LEFT" => OverlayPosition::TopLeft,
        "TOP_CENTER" => OverlayPosition::TopCenter,
        "TOP_RIGHT" => OverlayPosition::TopRight,
        "BOTTOM_LEFT" => OverlayPosition::BottomLeft,
        "BOTTOM_CENTER" => OverlayPosition::BottomCenter,
        "BOTTOM_RIGHT" => OverlayPosition::BottomRight,
        "MANUAL" => OverlayPosition::Manual,
        _ => return None,
    })
}

/// Shift, Control, Alt and the platform keys.
fn is_modifier(code: f64) -> bool {
    matches!(code as i64, 16 | 17 | 18 | 91 | 92 | 93 | 224)
}

/// Our key name for a keyboard key code.
fn key_name(code: f64) -> Option<String> {
    if !(code.is_finite() && (0.0..=255.0).contains(&code)) {
        return None;
    }
    let c = code as u32;
    Some(match c {
        8 => "Backspace".into(),
        9 => "Tab".into(),
        13 => "Enter".into(),
        27 => "Escape".into(),
        32 => "Space".into(),
        37 => "ArrowLeft".into(),
        38 => "ArrowUp".into(),
        39 => "ArrowRight".into(),
        40 => "ArrowDown".into(),
        46 => "Delete".into(),
        48..=57 | 65..=90 => char::from_u32(c)?.to_string(),
        112..=123 => format!("F{}", c - 111),
        _ => return None,
    })
}
