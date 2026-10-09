//! FigCraft's icon set, drawn in code from simple primitives on a 16×16 grid. Original artwork
//! (MIT OR Apache-2.0): no third-party icons are used anywhere.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Move,
    Scale,
    Hand,
    Frame,
    Section,
    Slice,
    Rectangle,
    Line,
    Arrow,
    Ellipse,
    Polygon,
    Star,
    Image,
    Pen,
    Pencil,
    Text,
    Comment,
    Actions,
    Chevron,
    ChevronRight,
    ChevronLeft,
    ChevronUp,
    Plus,
    Minus,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    Component,
    Instance,
    ComponentSet,
    Group,
    Vector,
    Bend,
    PaintBucket,
    Lasso,
    Cut,
    Boolean,
    Page,
    Check,
    Close,
    Search,
    Grid,
    AlignLeft,
    AlignHCenter,
    AlignRight,
    AlignTop,
    AlignVCenter,
    AlignBottom,
    DistributeH,
    DistributeV,
    Rotate,
    FlipH,
    FlipV,
    CornerRadius,
    IndependentCorners,
    LinkOn,
    LinkOff,
    LayoutH,
    LayoutV,
    LayoutWrap,
    LayoutGrid,
    AutoLayout,
    ClipContent,
    Effects,
    Styles,
    Menu,
    Play,
    Home,
    Bell,
    Panels,
    Help,
    Dots,
    Mask,
    TextAlignLeft,
    TextAlignCenter,
    TextAlignRight,
    TextAlignJustify,
    TextTop,
    TextMiddle,
    TextBottom,
    Eyedropper,
    Code,
    Assets,
    Layers,
    Opacity,
    Blend,
    Export,
    StrokeIcon,
    Fill,
    Sun,
    Moon,
    Undo,
    Redo,
    Union,
    Subtract,
    Intersect,
    Exclude,
    Hug,
    FillContainer,
    Fixed,
    PaddingH,
    PaddingV,
    Gap,
    Reset,
    Detach,
    Swap,
    Link,
    Prototype,
    Section2,
    Logo,
}

struct P<'a> {
    painter: &'a Painter,
    o: Pos2,
    s: f32,
    stroke: Stroke,
    color: Color32,
}

impl P<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.o.x + x * self.s, self.o.y + y * self.s)
    }
    fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.painter.line_segment([self.p(x0, y0), self.p(x1, y1)], self.stroke);
    }
    fn poly(&self, pts: &[(f32, f32)], closed: bool) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.p(*x, *y)).collect();
        if closed {
            self.painter.add(Shape::closed_line(v, self.stroke));
        } else {
            self.painter.add(Shape::line(v, self.stroke));
        }
    }
    fn fill_poly(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.p(*x, *y)).collect();
        self.painter.add(Shape::convex_polygon(v, self.color, Stroke::NONE));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) {
        let rr = Rect::from_min_max(self.p(x0, y0), self.p(x1, y1));
        self.painter.rect_stroke(rr, egui::CornerRadius::same((r * self.s) as u8), self.stroke, egui::StrokeKind::Middle);
    }
    fn fill_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) {
        let rr = Rect::from_min_max(self.p(x0, y0), self.p(x1, y1));
        self.painter.rect_filled(rr, egui::CornerRadius::same((r * self.s) as u8), self.color);
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.painter.circle_stroke(self.p(x, y), r * self.s, self.stroke);
    }
    fn dot(&self, x: f32, y: f32, r: f32) {
        self.painter.circle_filled(self.p(x, y), r * self.s, self.color);
    }
    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        let n = 16;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let t = a0 + (a1 - a0) * i as f32 / n as f32;
                (cx + r * t.cos(), cy + r * t.sin())
            })
            .collect();
        self.poly(&pts, false);
    }
}

/// Paint `icon` centred in `rect` (a 16-unit glyph scaled to fit) in `color`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height());
    let s = size / 16.0;
    let o = rect.center() - vec2(8.0 * s, 8.0 * s);
    let w = (1.0 * s).clamp(1.0, 2.0);
    let p = P { painter, o, s, stroke: Stroke::new(w, color), color };
    use Icon::*;
    match icon {
        Move => p.fill_poly(&[(4.0, 2.5), (4.0, 13.0), (6.6, 10.6), (8.4, 14.2), (9.9, 13.5), (8.2, 10.0), (11.6, 9.8)]),
        Scale => {
            p.rect(2.5, 6.5, 9.5, 13.5, 0.0);
            p.line(7.0, 9.0, 13.5, 2.5);
            p.poly(&[(9.5, 2.5), (13.5, 2.5), (13.5, 6.5)], false);
        }
        Hand => {
            p.poly(&[(5.0, 8.5), (5.0, 4.0), (6.5, 4.0), (6.5, 7.5)], false);
            p.poly(&[(6.5, 7.5), (6.5, 2.8), (8.0, 2.8), (8.0, 7.5)], false);
            p.poly(&[(8.0, 7.5), (8.0, 3.5), (9.5, 3.5), (9.5, 8.0)], false);
            p.poly(&[(9.5, 8.0), (9.5, 5.0), (11.0, 5.0), (11.0, 10.0), (9.5, 13.5), (6.0, 13.5), (3.5, 9.5), (4.5, 8.5), (5.0, 8.5)], false);
        }
        Frame => {
            p.line(5.0, 2.0, 5.0, 14.0);
            p.line(11.0, 2.0, 11.0, 14.0);
            p.line(2.0, 5.0, 14.0, 5.0);
            p.line(2.0, 11.0, 14.0, 11.0);
        }
        Section | Section2 => {
            p.rect(2.5, 4.5, 13.5, 13.5, 1.5);
            p.fill_rect(2.5, 2.5, 8.5, 5.5, 1.0);
        }
        Slice => {
            p.line(3.0, 13.0, 13.0, 3.0);
            p.rect(2.5, 2.5, 13.5, 13.5, 0.0);
        }
        Bend => {
            // A curve with a smooth point and its handle line.
            let pts: Vec<(f32, f32)> = (0..=12)
                .map(|i| {
                    let t = i as f32 / 12.0;
                    let u = 1.0 - t;
                    (u * u * 2.5 + 2.0 * u * t * 8.0 + t * t * 13.5, u * u * 13.0 + 2.0 * u * t * 1.5 + t * t * 13.0)
                })
                .collect();
            p.poly(&pts, false);
            p.line(3.5, 7.25, 12.5, 7.25);
            p.dot(3.5, 7.25, 1.2);
            p.dot(12.5, 7.25, 1.2);
            p.fill_rect(6.9, 6.15, 9.1, 8.35, 0.4);
        }
        PaintBucket => {
            p.poly(&[(2.5, 8.0), (7.5, 3.0), (12.5, 8.0), (7.5, 13.0)], true);
            p.line(4.0, 6.5, 11.0, 6.5);
            p.line(7.5, 3.0, 5.5, 1.0);
            p.dot(13.5, 11.8, 1.4);
        }
        Lasso => {
            let pts: Vec<(f32, f32)> = (0..=20)
                .map(|i| {
                    let a = i as f32 / 20.0 * std::f32::consts::TAU;
                    (8.5 + 5.5 * a.cos(), 6.0 + 3.5 * a.sin())
                })
                .collect();
            p.poly(&pts, true);
            p.poly(&[(5.0, 8.8), (4.6, 11.0), (5.6, 12.6), (4.2, 14.5)], false);
        }
        Cut => {
            p.circle(4.5, 11.5, 2.2);
            p.circle(11.5, 11.5, 2.2);
            p.line(6.0, 9.8, 11.5, 2.5);
            p.line(10.0, 9.8, 4.5, 2.5);
        }
        Rectangle => p.rect(3.0, 3.0, 13.0, 13.0, 0.5),
        Line => p.line(3.0, 13.0, 13.0, 3.0),
        Arrow => {
            p.line(3.0, 13.0, 13.0, 3.0);
            p.poly(&[(8.0, 3.0), (13.0, 3.0), (13.0, 8.0)], false);
        }
        Ellipse => p.circle(8.0, 8.0, 5.5),
        Polygon => p.poly(&[(8.0, 2.5), (13.5, 12.5), (2.5, 12.5)], true),
        Star => {
            let pts: Vec<(f32, f32)> = (0..10)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 5.0;
                    let r = if i % 2 == 0 { 6.0 } else { 2.6 };
                    (8.0 + r * a.cos(), 8.5 + r * a.sin())
                })
                .collect();
            p.poly(&pts, true);
        }
        Image => {
            p.rect(2.5, 3.5, 13.5, 12.5, 1.0);
            p.poly(&[(2.5, 11.0), (6.0, 7.5), (9.0, 10.5), (10.5, 9.0), (13.5, 12.0)], false);
            p.dot(10.5, 6.0, 1.0);
        }
        Pen => {
            p.poly(&[(8.0, 2.0), (12.5, 9.0), (10.0, 13.5), (6.0, 13.5), (3.5, 9.0)], true);
            p.line(8.0, 2.0, 8.0, 8.0);
            p.dot(8.0, 9.0, 1.0);
        }
        Pencil => {
            p.poly(&[(3.0, 13.0), (4.0, 10.0), (11.0, 3.0), (13.0, 5.0), (6.0, 12.0)], true);
            p.line(9.5, 4.5, 11.5, 6.5);
        }
        Text => {
            p.line(3.5, 3.5, 12.5, 3.5);
            p.line(8.0, 3.5, 8.0, 13.0);
            p.line(3.5, 3.5, 3.5, 5.0);
            p.line(12.5, 3.5, 12.5, 5.0);
            p.line(6.5, 13.0, 9.5, 13.0);
        }
        Comment => {
            p.poly(
                &[(8.0, 2.5), (11.5, 3.2), (13.3, 6.3), (12.8, 9.8), (10.0, 12.8), (8.0, 13.2), (2.8, 13.2), (2.8, 8.0), (3.6, 4.8), (5.5, 3.1)],
                true,
            );
        }
        Actions => {
            p.poly(&[(9.0, 2.0), (4.0, 9.0), (8.0, 9.0), (7.0, 14.0), (12.0, 7.0), (8.0, 7.0)], true);
        }
        Chevron => p.poly(&[(5.0, 6.5), (8.0, 9.5), (11.0, 6.5)], false),
        ChevronUp => p.poly(&[(5.0, 9.5), (8.0, 6.5), (11.0, 9.5)], false),
        ChevronRight => p.poly(&[(6.5, 5.0), (9.5, 8.0), (6.5, 11.0)], false),
        ChevronLeft => p.poly(&[(9.5, 5.0), (6.5, 8.0), (9.5, 11.0)], false),
        Plus => {
            p.line(8.0, 3.0, 8.0, 13.0);
            p.line(3.0, 8.0, 13.0, 8.0);
        }
        Minus => p.line(3.0, 8.0, 13.0, 8.0),
        Eye => {
            p.arc(8.0, 12.5, 7.0, -2.5, -0.64);
            p.arc(8.0, 3.5, 7.0, 0.64, 2.5);
            p.circle(8.0, 8.0, 2.0);
        }
        EyeOff => {
            p.arc(8.0, 12.5, 7.0, -2.5, -0.64);
            p.arc(8.0, 3.5, 7.0, 0.64, 2.5);
            p.line(3.0, 13.0, 13.0, 3.0);
        }
        Lock => {
            p.rect(3.5, 7.5, 12.5, 13.5, 1.0);
            p.arc(8.0, 7.5, 3.0, std::f32::consts::PI, std::f32::consts::TAU);
        }
        Unlock => {
            p.rect(3.5, 7.5, 12.5, 13.5, 1.0);
            p.arc(11.0, 7.5, 3.0, std::f32::consts::PI, std::f32::consts::TAU - 0.3);
        }
        Component => {
            for (cx, cy) in [(8.0, 4.2), (11.8, 8.0), (8.0, 11.8), (4.2, 8.0)] {
                p.fill_poly(&[(cx, cy - 2.4), (cx + 2.4, cy), (cx, cy + 2.4), (cx - 2.4, cy)]);
            }
        }
        ComponentSet => {
            for (cx, cy) in [(8.0, 4.2), (11.8, 8.0), (8.0, 11.8), (4.2, 8.0)] {
                p.poly(&[(cx, cy - 2.2), (cx + 2.2, cy), (cx, cy + 2.2), (cx - 2.2, cy)], true);
            }
        }
        Instance => p.poly(&[(8.0, 2.5), (13.5, 8.0), (8.0, 13.5), (2.5, 8.0)], true),
        Group => {
            for (x0, y0, x1, y1) in [(3.0, 3.0, 6.0, 3.0), (10.0, 3.0, 13.0, 3.0), (3.0, 13.0, 6.0, 13.0), (10.0, 13.0, 13.0, 13.0)] {
                p.line(x0, y0, x1, y1);
            }
            for (x0, y0, x1, y1) in [(3.0, 3.0, 3.0, 6.0), (13.0, 3.0, 13.0, 6.0), (3.0, 10.0, 3.0, 13.0), (13.0, 10.0, 13.0, 13.0)] {
                p.line(x0, y0, x1, y1);
            }
        }
        Vector => {
            p.poly(&[(3.0, 12.0), (6.0, 4.0), (10.0, 12.0), (13.0, 4.0)], false);
            p.dot(3.0, 12.0, 1.2);
            p.dot(13.0, 4.0, 1.2);
        }
        Boolean | Union => {
            p.rect(2.5, 2.5, 9.5, 9.5, 0.0);
            p.rect(6.5, 6.5, 13.5, 13.5, 0.0);
        }
        Subtract => {
            p.fill_rect(2.5, 2.5, 9.5, 9.5, 0.0);
            p.rect(6.5, 6.5, 13.5, 13.5, 0.0);
        }
        Intersect => {
            p.rect(2.5, 2.5, 9.5, 9.5, 0.0);
            p.rect(6.5, 6.5, 13.5, 13.5, 0.0);
            p.fill_rect(6.5, 6.5, 9.5, 9.5, 0.0);
        }
        Exclude => {
            p.fill_rect(2.5, 2.5, 9.5, 9.5, 0.0);
            p.fill_rect(6.5, 6.5, 13.5, 13.5, 0.0);
        }
        Page => {
            p.poly(&[(4.0, 2.5), (9.5, 2.5), (12.5, 5.5), (12.5, 13.5), (4.0, 13.5)], true);
            p.poly(&[(9.5, 2.5), (9.5, 5.5), (12.5, 5.5)], false);
        }
        Check => p.poly(&[(3.5, 8.5), (6.5, 11.5), (12.5, 4.5)], false),
        Close => {
            p.line(4.0, 4.0, 12.0, 12.0);
            p.line(12.0, 4.0, 4.0, 12.0);
        }
        Search => {
            p.circle(7.0, 7.0, 4.0);
            p.line(10.0, 10.0, 13.5, 13.5);
        }
        Grid | LayoutGrid => {
            for i in [3.0, 6.5, 10.0] {
                for j in [3.0, 6.5, 10.0] {
                    p.rect(i, j, i + 2.5, j + 2.5, 0.0);
                }
            }
        }
        AlignLeft => {
            p.line(2.5, 2.0, 2.5, 14.0);
            p.fill_rect(4.5, 4.0, 12.0, 6.5, 0.5);
            p.fill_rect(4.5, 9.5, 9.0, 12.0, 0.5);
        }
        AlignHCenter => {
            p.line(8.0, 2.0, 8.0, 14.0);
            p.fill_rect(3.5, 4.0, 12.5, 6.5, 0.5);
            p.fill_rect(5.5, 9.5, 10.5, 12.0, 0.5);
        }
        AlignRight => {
            p.line(13.5, 2.0, 13.5, 14.0);
            p.fill_rect(4.0, 4.0, 11.5, 6.5, 0.5);
            p.fill_rect(7.0, 9.5, 11.5, 12.0, 0.5);
        }
        AlignTop => {
            p.line(2.0, 2.5, 14.0, 2.5);
            p.fill_rect(4.0, 4.5, 6.5, 12.0, 0.5);
            p.fill_rect(9.5, 4.5, 12.0, 9.0, 0.5);
        }
        AlignVCenter => {
            p.line(2.0, 8.0, 14.0, 8.0);
            p.fill_rect(4.0, 3.5, 6.5, 12.5, 0.5);
            p.fill_rect(9.5, 5.5, 12.0, 10.5, 0.5);
        }
        AlignBottom => {
            p.line(2.0, 13.5, 14.0, 13.5);
            p.fill_rect(4.0, 4.0, 6.5, 11.5, 0.5);
            p.fill_rect(9.5, 7.0, 12.0, 11.5, 0.5);
        }
        DistributeH | Gap => {
            p.line(2.5, 3.0, 2.5, 13.0);
            p.line(13.5, 3.0, 13.5, 13.0);
            p.fill_rect(6.5, 5.0, 9.5, 11.0, 0.5);
        }
        DistributeV => {
            p.line(3.0, 2.5, 13.0, 2.5);
            p.line(3.0, 13.5, 13.0, 13.5);
            p.fill_rect(5.0, 6.5, 11.0, 9.5, 0.5);
        }
        Rotate => {
            p.arc(8.0, 8.0, 5.0, -2.6, 2.2);
            p.poly(&[(3.0, 2.5), (3.4, 5.5), (6.4, 5.0)], false);
        }
        FlipH => {
            p.line(8.0, 2.0, 8.0, 14.0);
            p.poly(&[(6.0, 4.0), (2.5, 12.0), (6.0, 12.0)], true);
            p.fill_poly(&[(10.0, 4.0), (13.5, 12.0), (10.0, 12.0)]);
        }
        FlipV => {
            p.line(2.0, 8.0, 14.0, 8.0);
            p.poly(&[(4.0, 6.0), (12.0, 2.5), (12.0, 6.0)], true);
            p.fill_poly(&[(4.0, 10.0), (12.0, 13.5), (12.0, 10.0)]);
        }
        CornerRadius => p.poly(&[(3.0, 13.0), (3.0, 8.0), (3.4, 5.6), (5.6, 3.4), (8.0, 3.0), (13.0, 3.0)], false),
        IndependentCorners => {
            for (a, b, c) in [
                ((2.5, 6.0), (2.5, 2.5), (6.0, 2.5)),
                ((10.0, 2.5), (13.5, 2.5), (13.5, 6.0)),
                ((13.5, 10.0), (13.5, 13.5), (10.0, 13.5)),
                ((6.0, 13.5), (2.5, 13.5), (2.5, 10.0)),
            ] {
                p.poly(&[a, b, c], false);
            }
        }
        LinkOn | Link => {
            p.rect(2.5, 6.0, 8.5, 10.0, 2.0);
            p.rect(7.5, 6.0, 13.5, 10.0, 2.0);
        }
        LinkOff => {
            p.rect(2.5, 6.0, 7.0, 10.0, 2.0);
            p.rect(9.0, 6.0, 13.5, 10.0, 2.0);
        }
        LayoutH => {
            p.fill_rect(2.5, 4.0, 6.5, 12.0, 1.0);
            p.fill_rect(9.5, 4.0, 13.5, 12.0, 1.0);
        }
        LayoutV => {
            p.fill_rect(4.0, 2.5, 12.0, 6.5, 1.0);
            p.fill_rect(4.0, 9.5, 12.0, 13.5, 1.0);
        }
        LayoutWrap => {
            p.poly(&[(2.5, 4.5), (12.0, 4.5), (13.5, 6.0), (13.5, 8.0), (12.0, 9.5), (4.0, 9.5)], false);
            p.poly(&[(6.0, 7.5), (4.0, 9.5), (6.0, 11.5)], false);
        }
        AutoLayout => {
            p.rect(2.5, 2.5, 13.5, 13.5, 2.0);
            p.fill_rect(5.0, 5.0, 7.0, 11.0, 0.5);
            p.fill_rect(9.0, 5.0, 11.0, 11.0, 0.5);
        }
        ClipContent => {
            p.rect(3.5, 3.5, 12.5, 12.5, 0.5);
            p.line(5.0, 1.5, 5.0, 3.5);
            p.line(1.5, 5.0, 3.5, 5.0);
        }
        Effects => {
            p.circle(8.0, 8.0, 3.0);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                p.line(8.0 + 4.8 * a.cos(), 8.0 + 4.8 * a.sin(), 8.0 + 6.2 * a.cos(), 8.0 + 6.2 * a.sin());
            }
        }
        Styles => {
            for (x, y) in [(5.0, 5.0), (11.0, 5.0), (5.0, 11.0), (11.0, 11.0)] {
                p.dot(x, y, 2.0);
            }
        }
        Menu => {
            p.line(3.0, 4.5, 13.0, 4.5);
            p.line(3.0, 8.0, 13.0, 8.0);
            p.line(3.0, 11.5, 13.0, 11.5);
        }
        Play => p.fill_poly(&[(5.0, 3.0), (13.0, 8.0), (5.0, 13.0)]),
        Home => {
            p.poly(&[(2.5, 7.5), (8.0, 2.5), (13.5, 7.5)], false);
            p.poly(&[(4.0, 6.5), (4.0, 13.5), (12.0, 13.5), (12.0, 6.5)], false);
        }
        Bell => {
            p.poly(
                &[(4.0, 11.0), (4.0, 7.0), (5.5, 4.0), (8.0, 3.0), (10.5, 4.0), (12.0, 7.0), (12.0, 11.0), (13.0, 12.0), (3.0, 12.0), (4.0, 11.0)],
                false,
            );
            p.arc(8.0, 12.5, 1.5, 0.0, std::f32::consts::PI);
        }
        Panels => {
            p.rect(2.5, 3.5, 13.5, 12.5, 1.0);
            p.line(5.5, 3.5, 5.5, 12.5);
            p.line(10.5, 3.5, 10.5, 12.5);
        }
        Help => {
            p.circle(8.0, 8.0, 6.0);
            p.arc(8.0, 6.5, 2.0, std::f32::consts::PI, std::f32::consts::TAU + 0.6);
            p.line(8.0, 8.4, 8.0, 9.5);
            p.dot(8.0, 11.5, 0.8);
        }
        Dots => {
            for x in [4.0, 8.0, 12.0] {
                p.dot(x, 8.0, 1.1);
            }
        }
        Mask => {
            p.circle(8.0, 8.0, 5.5);
            p.fill_poly(&[(8.0, 2.5), (10.5, 3.1), (12.4, 4.8), (13.5, 8.0), (12.4, 11.2), (10.5, 12.9), (8.0, 13.5)]);
        }
        TextAlignLeft => {
            for (y, x1) in [(4.0, 13.0), (7.0, 10.0), (10.0, 13.0), (13.0, 9.0)] {
                p.line(3.0, y, x1, y);
            }
        }
        TextAlignCenter => {
            for (y, h) in [(4.0, 5.0), (7.0, 3.5), (10.0, 5.0), (13.0, 3.0)] {
                p.line(8.0 - h, y, 8.0 + h, y);
            }
        }
        TextAlignRight => {
            for (y, x0) in [(4.0, 3.0), (7.0, 6.0), (10.0, 3.0), (13.0, 7.0)] {
                p.line(x0, y, 13.0, y);
            }
        }
        TextAlignJustify => {
            for y in [4.0, 7.0, 10.0, 13.0] {
                p.line(3.0, y, 13.0, y);
            }
        }
        TextTop => {
            p.line(3.0, 3.0, 13.0, 3.0);
            p.line(8.0, 5.5, 8.0, 13.0);
            p.poly(&[(5.5, 8.0), (8.0, 5.5), (10.5, 8.0)], false);
        }
        TextMiddle => {
            p.line(3.0, 8.0, 13.0, 8.0);
            p.poly(&[(6.0, 3.0), (8.0, 5.5), (10.0, 3.0)], false);
            p.poly(&[(6.0, 13.0), (8.0, 10.5), (10.0, 13.0)], false);
        }
        TextBottom => {
            p.line(3.0, 13.0, 13.0, 13.0);
            p.line(8.0, 3.0, 8.0, 10.5);
            p.poly(&[(5.5, 8.0), (8.0, 10.5), (10.5, 8.0)], false);
        }
        Eyedropper => {
            p.poly(&[(3.0, 13.0), (3.5, 10.5), (9.0, 5.0), (11.0, 7.0), (5.5, 12.5)], true);
            p.line(9.5, 3.0, 13.0, 6.5);
            p.line(8.0, 4.0, 12.0, 8.0);
        }
        Code => {
            p.poly(&[(5.5, 4.5), (2.5, 8.0), (5.5, 11.5)], false);
            p.poly(&[(10.5, 4.5), (13.5, 8.0), (10.5, 11.5)], false);
        }
        Assets => {
            p.fill_poly(&[(8.0, 2.5), (10.5, 5.0), (8.0, 7.5), (5.5, 5.0)]);
            p.poly(&[(3.0, 8.5), (5.5, 11.0), (3.0, 13.5), (0.5, 11.0)], true);
            p.rect(9.5, 9.0, 14.0, 13.5, 0.5);
        }
        Layers => {
            p.poly(&[(8.0, 2.5), (14.0, 5.5), (8.0, 8.5), (2.0, 5.5)], true);
            p.poly(&[(2.0, 8.5), (8.0, 11.5), (14.0, 8.5)], false);
        }
        Opacity => {
            p.circle(8.0, 8.0, 5.5);
            p.fill_poly(&[(8.0, 2.5), (8.0, 13.5), (5.5, 12.9), (3.6, 11.2), (2.5, 8.0), (3.6, 4.8), (5.5, 3.1)]);
        }
        Blend => {
            p.circle(6.0, 8.0, 3.8);
            p.circle(10.0, 8.0, 3.8);
        }
        Export => {
            p.line(8.0, 2.5, 8.0, 10.0);
            p.poly(&[(5.0, 5.5), (8.0, 2.5), (11.0, 5.5)], false);
            p.poly(&[(3.0, 9.0), (3.0, 13.5), (13.0, 13.5), (13.0, 9.0)], false);
        }
        StrokeIcon => p.rect(3.0, 3.0, 13.0, 13.0, 1.5),
        Fill => p.fill_rect(3.0, 3.0, 13.0, 13.0, 1.5),
        Sun => {
            p.circle(8.0, 8.0, 2.8);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                p.line(8.0 + 4.5 * a.cos(), 8.0 + 4.5 * a.sin(), 8.0 + 6.0 * a.cos(), 8.0 + 6.0 * a.sin());
            }
        }
        Moon => p.arc(8.0, 8.0, 5.0, 0.9, 5.4),
        Undo => {
            p.arc(9.0, 9.0, 4.5, -2.6, 1.6);
            p.poly(&[(3.0, 3.5), (4.0, 7.0), (7.3, 6.0)], false);
        }
        Redo => {
            p.arc(7.0, 9.0, 4.5, 1.5, 5.7);
            p.poly(&[(13.0, 3.5), (12.0, 7.0), (8.7, 6.0)], false);
        }
        Hug => {
            p.poly(&[(2.5, 5.0), (5.0, 8.0), (2.5, 11.0)], false);
            p.poly(&[(13.5, 5.0), (11.0, 8.0), (13.5, 11.0)], false);
            p.line(5.0, 8.0, 11.0, 8.0);
        }
        FillContainer => {
            p.poly(&[(5.0, 5.0), (2.5, 8.0), (5.0, 11.0)], false);
            p.poly(&[(11.0, 5.0), (13.5, 8.0), (11.0, 11.0)], false);
            p.line(2.5, 8.0, 13.5, 8.0);
        }
        Fixed => {
            p.line(2.5, 4.0, 2.5, 12.0);
            p.line(13.5, 4.0, 13.5, 12.0);
            p.line(2.5, 8.0, 13.5, 8.0);
        }
        PaddingH => {
            p.rect(2.5, 3.5, 13.5, 12.5, 1.0);
            p.fill_rect(5.0, 6.0, 11.0, 10.0, 0.5);
        }
        PaddingV => {
            p.rect(3.5, 2.5, 12.5, 13.5, 1.0);
            p.fill_rect(6.0, 5.0, 10.0, 11.0, 0.5);
        }
        Reset => {
            p.arc(8.0, 8.0, 5.0, -2.2, 2.6);
            p.poly(&[(2.8, 2.8), (3.3, 5.8), (6.3, 5.3)], false);
        }
        Detach => {
            p.poly(&[(8.0, 2.5), (13.5, 8.0), (8.0, 13.5), (2.5, 8.0)], true);
            p.line(4.0, 12.0, 12.0, 4.0);
        }
        Swap => {
            p.poly(&[(3.0, 5.5), (12.0, 5.5), (10.0, 3.5)], false);
            p.poly(&[(13.0, 10.5), (4.0, 10.5), (6.0, 12.5)], false);
        }
        Prototype => {
            p.circle(4.5, 11.5, 2.0);
            p.poly(&[(6.0, 10.0), (9.0, 6.0), (12.5, 4.0)], false);
            p.poly(&[(10.0, 3.0), (12.8, 3.8), (12.0, 6.6)], false);
        }
        Logo => {
            // FigCraft's own mark: three stacked rounded tiles forming an "F" shape.
            p.fill_rect(3.0, 2.0, 13.0, 5.5, 1.75);
            p.fill_rect(3.0, 6.25, 10.5, 9.75, 1.75);
            p.fill_rect(3.0, 10.5, 6.5, 14.0, 1.75);
        }
    }
}

/// Allocate a square icon button; returns the response. `active` paints the selected look.
pub fn button(ui: &mut egui::Ui, icon: Icon, size: f32, active: bool, tooltip: &str) -> egui::Response {
    let t = crate::theme::Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
    let bg = if active {
        t.selected_row
    } else if resp.is_pointer_button_down_on() {
        t.pressed
    } else if resp.hovered() {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, egui::CornerRadius::same(5), bg);
    let col = if active { t.accent_text } else { t.icon };
    paint(ui.painter(), rect.shrink(size * 0.17), icon, col);
    if tooltip.is_empty() { resp } else { resp.on_hover_text(tooltip) }
}
