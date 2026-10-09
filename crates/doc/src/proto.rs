//! Prototype data: interactions (trigger → actions) with transitions, flows and overflow.

use crate::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[derive(Default)]
pub enum Trigger {
    #[default]
    OnClick,
    OnHover,
    OnPress,
    OnDrag,
    MouseEnter,
    MouseLeave,
    MouseDown,
    MouseUp,
    AfterDelay {
        ms: u32,
    },
    KeyPress {
        key: String,
    },
}

impl Trigger {
    pub fn label(&self) -> String {
        match self {
            Trigger::OnClick => "On click".into(),
            Trigger::OnHover => "While hovering".into(),
            Trigger::OnPress => "While pressing".into(),
            Trigger::OnDrag => "On drag".into(),
            Trigger::MouseEnter => "Mouse enter".into(),
            Trigger::MouseLeave => "Mouse leave".into(),
            Trigger::MouseDown => "Mouse down".into(),
            Trigger::MouseUp => "Mouse up".into(),
            Trigger::AfterDelay { ms } => format!("After delay {ms}ms"),
            Trigger::KeyPress { key } => format!("Key/Gamepad {key}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Easing {
    Linear,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    Gentle,
    Quick,
    Bouncy,
    Slow,
    /// A CSS-style cubic bezier through (0,0), (x1,y1), (x2,y2), (1,1).
    #[serde(rename_all = "camelCase")]
    CustomBezier {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    /// A damped spring (mass, stiffness, damping), played over the interaction's duration.
    #[serde(rename_all = "camelCase")]
    CustomSpring {
        mass: f64,
        stiffness: f64,
        damping: f64,
    },
}

impl Easing {
    /// The starting point when switching to a custom curve.
    pub const CUSTOM_BEZIER: Easing = Easing::CustomBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
    pub const CUSTOM_SPRING: Easing = Easing::CustomSpring { mass: 1.0, stiffness: 100.0, damping: 15.0 };

    pub const ALL: [Easing; 11] = [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
        Easing::EaseInBack,
        Easing::EaseOutBack,
        Easing::EaseInOutBack,
        Easing::Gentle,
        Easing::Quick,
        Easing::Bouncy,
        Easing::Slow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Easing::Linear => "Linear",
            Easing::EaseIn => "Ease in",
            Easing::EaseOut => "Ease out",
            Easing::EaseInOut => "Ease in and out",
            Easing::EaseInBack => "Ease in back",
            Easing::EaseOutBack => "Ease out back",
            Easing::EaseInOutBack => "Ease in and out back",
            Easing::Gentle => "Gentle",
            Easing::Quick => "Quick",
            Easing::Bouncy => "Bouncy",
            Easing::Slow => "Slow",
            Easing::CustomBezier { .. } => "Custom bezier",
            Easing::CustomSpring { .. } => "Custom spring",
        }
    }

    /// Spring presets run longer than the chosen duration in the incumbent; we keep the duration.
    pub fn is_spring(self) -> bool {
        matches!(self, Easing::Gentle | Easing::Quick | Easing::Bouncy | Easing::Slow | Easing::CustomSpring { .. })
    }

    /// Evaluate the curve at `t` (0..1).
    pub fn eval(self, t: f64) -> f64 {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 1.0 };
        let back = 1.70158;
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::EaseInBack => (back + 1.0) * t * t * t - back * t * t,
            Easing::EaseOutBack => 1.0 + (back + 1.0) * (t - 1.0).powi(3) + back * (t - 1.0).powi(2),
            Easing::EaseInOutBack => {
                let c2 = back * 1.525;
                if t < 0.5 {
                    ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
                } else {
                    ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
                }
            }
            // Spring presets approximated by damped oscillations.
            Easing::Gentle => spring(t, 6.0, 0.7),
            Easing::Quick => spring(t, 10.0, 0.8),
            Easing::Bouncy => spring(t, 9.0, 0.35),
            Easing::Slow => spring(t, 4.0, 0.9),
            Easing::CustomBezier { x1, y1, x2, y2 } => cubic_bezier(t, x1, y1, x2, y2),
            Easing::CustomSpring { mass, stiffness, damping } => physical_spring(t, mass, stiffness, damping),
        }
    }
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at time `t` (x clamped to 0..1 so time stays monotonic).
fn cubic_bezier(t: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let fin = |v: f64, d: f64| if v.is_finite() { v } else { d };
    let (x1, x2) = (fin(x1, 0.42).clamp(0.0, 1.0), fin(x2, 0.58).clamp(0.0, 1.0));
    let (y1, y2) = (fin(y1, 0.0).clamp(-10.0, 10.0), fin(y2, 1.0).clamp(-10.0, 10.0));
    let b = |p1: f64, p2: f64, s: f64| {
        let u = 1.0 - s;
        3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s
    };
    // x(s) is monotonic for x1, x2 in 0..1: bisect for s with x(s) = t.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..48 {
        let mid = (lo + hi) / 2.0;
        if b(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    b(y1, y2, (lo + hi) / 2.0)
}

/// A unit step response of a damped spring, with its settling time mapped onto 0..1.
fn physical_spring(t: f64, mass: f64, stiffness: f64, damping: f64) -> f64 {
    if t >= 1.0 {
        return 1.0;
    }
    let ok = |v: f64, lo: f64, d: f64| if v.is_finite() && v >= lo { v } else { d };
    let m = ok(mass, 0.01, 1.0).min(1e4);
    let k = ok(stiffness, 0.01, 100.0).min(1e6);
    let c = ok(damping, 0.0, 15.0).min(1e6);
    let w0 = (k / m).sqrt();
    let zeta = c / (2.0 * (k * m).sqrt());
    // Time to settle within 0.1% (an undamped spring is cut at 10 s).
    let settle = if zeta > 1e-6 { (6.9 / (zeta.min(1.0) * w0)).clamp(0.05, 10.0) } else { 10.0 };
    let tau = t * settle;
    let x = if zeta < 1.0 {
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        1.0 - (-zeta * w0 * tau).exp() * ((wd * tau).cos() + zeta * w0 / wd * (wd * tau).sin())
    } else if (zeta - 1.0).abs() < 1e-9 {
        1.0 - (-w0 * tau).exp() * (1.0 + w0 * tau)
    } else {
        let s = (zeta * zeta - 1.0).sqrt();
        let (r1, r2) = (-w0 * (zeta - s), -w0 * (zeta + s));
        1.0 - (r2 * (r1 * tau).exp() - r1 * (r2 * tau).exp()) / (r2 - r1)
    };
    if x.is_finite() { x } else { 1.0 }
}

fn spring(t: f64, freq: f64, damping: f64) -> f64 {
    if t >= 1.0 {
        return 1.0;
    }
    let decay = (-damping * 8.0 * t).exp();
    1.0 - decay * (freq * t * std::f64::consts::PI).cos() * (1.0 - t)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[derive(Default)]
pub enum Transition {
    #[default]
    Instant,
    Dissolve,
    SmartAnimate,
    MoveIn {
        #[serde(default)]
        direction: Direction,
    },
    MoveOut {
        #[serde(default)]
        direction: Direction,
    },
    Push {
        #[serde(default)]
        direction: Direction,
    },
    SlideIn {
        #[serde(default)]
        direction: Direction,
    },
    SlideOut {
        #[serde(default)]
        direction: Direction,
    },
}

impl Transition {
    /// The transition's direction, if it has one.
    pub fn direction(&self) -> Option<Direction> {
        match self {
            Transition::MoveIn { direction }
            | Transition::MoveOut { direction }
            | Transition::Push { direction }
            | Transition::SlideIn { direction }
            | Transition::SlideOut { direction } => Some(*direction),
            _ => None,
        }
    }

    /// The same transition with another direction (no-op for directionless ones).
    pub fn with_direction(self, d: Direction) -> Transition {
        match self {
            Transition::MoveIn { .. } => Transition::MoveIn { direction: d },
            Transition::MoveOut { .. } => Transition::MoveOut { direction: d },
            Transition::Push { .. } => Transition::Push { direction: d },
            Transition::SlideIn { .. } => Transition::SlideIn { direction: d },
            Transition::SlideOut { .. } => Transition::SlideOut { direction: d },
            other => other,
        }
    }

    /// The opposite motion (used when a "while hovering/pressing" state reverts).
    pub fn reversed(self) -> Transition {
        let opp = |d: Direction| match d {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Top => Direction::Bottom,
            Direction::Bottom => Direction::Top,
        };
        match self {
            Transition::MoveIn { direction } => Transition::MoveOut { direction: opp(direction) },
            Transition::MoveOut { direction } => Transition::MoveIn { direction: opp(direction) },
            Transition::Push { direction } => Transition::Push { direction: opp(direction) },
            Transition::SlideIn { direction } => Transition::SlideOut { direction: opp(direction) },
            Transition::SlideOut { direction } => Transition::SlideIn { direction: opp(direction) },
            other => other,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Transition::Instant => "Instant",
            Transition::Dissolve => "Dissolve",
            Transition::SmartAnimate => "Smart animate",
            Transition::MoveIn { .. } => "Move in",
            Transition::MoveOut { .. } => "Move out",
            Transition::Push { .. } => "Push",
            Transition::SlideIn { .. } => "Slide in",
            Transition::SlideOut { .. } => "Slide out",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OverlayPosition {
    #[default]
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Manual,
}

impl OverlayPosition {
    pub const ALL: [OverlayPosition; 8] = [
        OverlayPosition::Center,
        OverlayPosition::TopLeft,
        OverlayPosition::TopCenter,
        OverlayPosition::TopRight,
        OverlayPosition::BottomLeft,
        OverlayPosition::BottomCenter,
        OverlayPosition::BottomRight,
        OverlayPosition::Manual,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OverlayPosition::Center => "Center",
            OverlayPosition::TopLeft => "Top left",
            OverlayPosition::TopCenter => "Top center",
            OverlayPosition::TopRight => "Top right",
            OverlayPosition::BottomLeft => "Bottom left",
            OverlayPosition::BottomCenter => "Bottom center",
            OverlayPosition::BottomRight => "Bottom right",
            OverlayPosition::Manual => "Manual",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Navigate {
        dest: NodeId,
        /// Keep the scroll position of the frame being left (else the destination starts at the top).
        #[serde(default, alias = "preserveScroll", skip_serializing_if = "is_false")]
        preserve_scroll: bool,
    },
    Back,
    ScrollTo {
        dest: NodeId,
    },
    OpenOverlay {
        dest: NodeId,
        #[serde(default)]
        position: OverlayPosition,
        #[serde(default, alias = "closeOnOutside")]
        close_on_outside: bool,
        /// Colour drawn behind the overlay (over the frame underneath); None = no background.
        #[serde(default)]
        background: Option<figcraft_color::Color>,
    },
    SwapOverlay {
        dest: NodeId,
    },
    CloseOverlay,
    OpenUrl {
        url: String,
    },
    /// Change an instance to another variant (interactive components).
    ChangeTo {
        dest: NodeId,
    },
    SetVariable {
        variable: String,
        value: serde_json::Value,
    },
}

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Action::Navigate { .. } => "Navigate to",
            Action::Back => "Back",
            Action::ScrollTo { .. } => "Scroll to",
            Action::OpenOverlay { .. } => "Open overlay",
            Action::SwapOverlay { .. } => "Swap overlay",
            Action::CloseOverlay => "Close overlay",
            Action::OpenUrl { .. } => "Open link",
            Action::ChangeTo { .. } => "Change to",
            Action::SetVariable { .. } => "Set variable",
        }
    }
    pub fn dest(&self) -> Option<NodeId> {
        match self {
            Action::Navigate { dest, .. }
            | Action::ScrollTo { dest }
            | Action::OpenOverlay { dest, .. }
            | Action::SwapOverlay { dest }
            | Action::ChangeTo { dest } => Some(*dest),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interaction {
    #[serde(default)]
    pub trigger: Trigger,
    pub actions: Vec<Action>,
    #[serde(default)]
    pub transition: Transition,
    #[serde(default = "default_duration", alias = "durationMs")]
    pub duration_ms: u32,
    #[serde(default)]
    pub easing: Easing,
}

fn default_duration() -> u32 {
    300
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Overflow {
    #[default]
    None,
    Horizontal,
    Vertical,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ScrollPosition {
    #[default]
    Scrolls,
    Fixed,
    Sticky,
}

/// A prototype flow's starting point on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    pub start: NodeId,
    #[serde(default)]
    pub description: String,
}

/// Prototype device settings on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrototypeSettings {
    pub device: String,
    pub background: figcraft_color::Color,
}

impl Default for PrototypeSettings {
    fn default() -> Self {
        Self { device: "None".into(), background: figcraft_color::Color::from_rgb8_const(0x1E, 0x1E, 0x1E) }
    }
}

/// A device prototypes are presented on: its name and screen size in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Device {
    pub name: &'static str,
    pub width: f64,
    pub height: f64,
}

impl PrototypeSettings {
    /// The devices to pick from ("None" presents the frame at its own size).
    pub const DEVICES: [Device; 6] = [
        Device { name: "iPhone 16", width: 393.0, height: 852.0 },
        Device { name: "iPhone 16 Pro Max", width: 440.0, height: 956.0 },
        Device { name: "Android Compact", width: 412.0, height: 917.0 },
        Device { name: "iPad mini", width: 744.0, height: 1133.0 },
        Device { name: "MacBook Air", width: 1280.0, height: 832.0 },
        Device { name: "Desktop", width: 1440.0, height: 1024.0 },
    ];

    /// The chosen device (None for "None" or a name we don't know).
    pub fn device(&self) -> Option<Device> {
        Self::DEVICES.iter().find(|d| d.name == self.device).copied()
    }
}
