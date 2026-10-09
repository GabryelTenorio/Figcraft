//! Node property types: strokes, effects, corners, constraints, auto layout, grids, exports.

use figcraft_color::{BlendMode, Color};
use serde::{Deserialize, Serialize};

/// Where a stroke sits relative to the shape edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StrokeCap {
    #[default]
    None,
    Round,
    Square,
    LineArrow,
    TriangleArrow,
    ReversedTriangle,
    CircleFilled,
    DiamondFilled,
}

impl StrokeCap {
    pub const ALL: [StrokeCap; 8] = [
        StrokeCap::None,
        StrokeCap::Round,
        StrokeCap::Square,
        StrokeCap::LineArrow,
        StrokeCap::TriangleArrow,
        StrokeCap::ReversedTriangle,
        StrokeCap::CircleFilled,
        StrokeCap::DiamondFilled,
    ];
    pub fn label(self) -> &'static str {
        match self {
            StrokeCap::None => "None",
            StrokeCap::Round => "Round",
            StrokeCap::Square => "Square",
            StrokeCap::LineArrow => "Line arrow",
            StrokeCap::TriangleArrow => "Triangle arrow",
            StrokeCap::ReversedTriangle => "Reversed triangle",
            StrokeCap::CircleFilled => "Circle",
            StrokeCap::DiamondFilled => "Diamond",
        }
    }
    pub fn is_arrow(self) -> bool {
        !matches!(self, StrokeCap::None | StrokeCap::Round | StrokeCap::Square)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StrokeJoin {
    #[default]
    Miter,
    Bevel,
    Round,
}

/// Stroke geometry shared by every stroke paint of a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrokeProps {
    pub weight: f64,
    pub align: StrokeAlign,
    /// Per-side weights `[top, right, bottom, left]` (rectangles and frames only).
    pub sides: Option<[f64; 4]>,
    pub cap_start: StrokeCap,
    pub cap_end: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f64,
    /// Dash pattern (dash, gap, …); empty = solid.
    pub dashes: Vec<f64>,
}

impl Default for StrokeProps {
    fn default() -> Self {
        Self {
            weight: 1.0,
            align: StrokeAlign::Inside,
            sides: None,
            cap_start: StrokeCap::None,
            cap_end: StrokeCap::None,
            join: StrokeJoin::Miter,
            miter_limit: 4.0,
            dashes: Vec::new(),
        }
    }
}

/// A shadow (drop or inner).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shadow {
    pub color: Color,
    pub offset: (f64, f64),
    /// Blur radius.
    pub radius: f64,
    pub spread: f64,
    pub blend: BlendMode,
    /// Drop shadows only: show behind transparent areas of the layer.
    #[serde(alias = "showBehind")]
    pub show_behind: bool,
}

impl Default for Shadow {
    fn default() -> Self {
        Self { color: Color::new(0.0, 0.0, 0.0, 0.25), offset: (0.0, 4.0), radius: 4.0, spread: 0.0, blend: BlendMode::Normal, show_behind: false }
    }
}

/// One effect in a node's effect list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EffectKind {
    DropShadow(Shadow),
    InnerShadow(Shadow),
    LayerBlur {
        radius: f64,
    },
    BackgroundBlur {
        radius: f64,
    },
    /// Monotone grain over the layer.
    Noise {
        #[serde(default = "noise_size")]
        size: f64,
        #[serde(default = "noise_density")]
        density: f64,
        #[serde(default = "noise_color")]
        color: Color,
    },
    /// Roughened edges.
    Texture {
        #[serde(default = "noise_size")]
        size: f64,
        #[serde(default = "texture_radius")]
        radius: f64,
    },
}

fn noise_size() -> f64 {
    0.5
}
fn noise_density() -> f64 {
    0.5
}
fn texture_radius() -> f64 {
    4.0
}
fn noise_color() -> Color {
    Color::new(0.0, 0.0, 0.0, 0.25)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    #[serde(flatten)]
    pub kind: EffectKind,
    #[serde(default = "yes")]
    pub visible: bool,
}

fn yes() -> bool {
    true
}

impl Effect {
    pub fn drop_shadow() -> Self {
        Self { kind: EffectKind::DropShadow(Shadow::default()), visible: true }
    }
    pub fn inner_shadow() -> Self {
        Self { kind: EffectKind::InnerShadow(Shadow { offset: (0.0, 4.0), ..Shadow::default() }), visible: true }
    }
    pub fn layer_blur() -> Self {
        Self { kind: EffectKind::LayerBlur { radius: 4.0 }, visible: true }
    }
    pub fn background_blur() -> Self {
        Self { kind: EffectKind::BackgroundBlur { radius: 4.0 }, visible: true }
    }
    pub fn label(&self) -> &'static str {
        match self.kind {
            EffectKind::DropShadow(_) => "Drop shadow",
            EffectKind::InnerShadow(_) => "Inner shadow",
            EffectKind::LayerBlur { .. } => "Layer blur",
            EffectKind::BackgroundBlur { .. } => "Background blur",
            EffectKind::Noise { .. } => "Noise",
            EffectKind::Texture { .. } => "Texture",
        }
    }
    /// How far outside the node's bounds this effect can paint.
    pub fn reach(&self) -> f64 {
        if !self.visible {
            return 0.0;
        }
        match &self.kind {
            EffectKind::DropShadow(s) => s.radius.abs() * 1.5 + s.spread.max(0.0) + s.offset.0.abs().max(s.offset.1.abs()),
            EffectKind::LayerBlur { radius } => radius.abs() * 1.5,
            EffectKind::Texture { radius, .. } => radius.abs(),
            _ => 0.0,
        }
    }
}

/// Corner radii `[tl, tr, br, bl]` and smoothing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Corners {
    pub radii: [f64; 4],
    pub smoothing: f64,
}

impl Corners {
    pub fn uniform(&self) -> Option<f64> {
        let [a, b, c, d] = self.radii;
        if a == b && b == c && c == d { Some(a) } else { None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum HConstraint {
    #[default]
    Left,
    Right,
    LeftRight,
    Center,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum VConstraint {
    #[default]
    Top,
    Bottom,
    TopBottom,
    Center,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Constraints {
    pub horizontal: HConstraint,
    pub vertical: VConstraint,
}

/// Auto layout direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LayoutMode {
    #[default]
    Horizontal,
    Vertical,
    Grid,
}

/// Alignment along the main axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MainAlign {
    #[default]
    Min,
    Center,
    Max,
    SpaceBetween,
}

/// Alignment across the main axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CrossAlign {
    #[default]
    Min,
    Center,
    Max,
    Baseline,
}

/// Auto layout container properties (a frame with auto layout).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoLayout {
    pub mode: LayoutMode,
    pub wrap: bool,
    /// Gap between items along the main axis.
    pub gap: f64,
    /// Gap between wrapped rows/columns.
    pub cross_gap: f64,
    /// Padding `[top, right, bottom, left]`.
    pub padding: [f64; 4],
    pub main_align: MainAlign,
    pub cross_align: CrossAlign,
    /// Grid mode: column and row counts (rows grow to fit the children).
    pub grid_columns: u32,
    pub grid_rows: u32,
    /// Grid mode: column track sizes (missing tracks are `1fr`).
    pub column_tracks: Vec<GridTrack>,
    /// Grid mode: row track sizes (missing tracks are `1fr`).
    pub row_tracks: Vec<GridTrack>,
    /// Include strokes in layout sizes.
    pub strokes_included: bool,
    /// Last item on top (canvas stacking).
    pub reverse_z: bool,
}

impl Default for AutoLayout {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Horizontal,
            wrap: false,
            gap: 10.0,
            cross_gap: 10.0,
            padding: [10.0; 4],
            main_align: MainAlign::Min,
            cross_align: CrossAlign::Min,
            grid_columns: 2,
            grid_rows: 2,
            column_tracks: Vec::new(),
            row_tracks: Vec::new(),
            strokes_included: false,
            reverse_z: false,
        }
    }
}

/// How a node sizes on one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Sizing {
    #[default]
    Fixed,
    /// Hug contents (auto layout frames, text).
    Hug,
    /// Fill the auto layout parent.
    Fill,
}

impl Sizing {
    pub fn label(self) -> &'static str {
        match self {
            Sizing::Fixed => "Fixed",
            Sizing::Hug => "Hug",
            Sizing::Fill => "Fill",
        }
    }
}

/// One grid auto layout track (a column or a row): a fixed size in px, hugging its contents, or
/// filling a share of the free space (`value` fractional units, like CSS `fr`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridTrack {
    pub sizing: Sizing,
    /// Fixed: the size in px. Fill: the fractional units. Hug: unused.
    pub value: f64,
}

impl Default for GridTrack {
    fn default() -> Self {
        Self { sizing: Sizing::Fill, value: 1.0 }
    }
}

impl GridTrack {
    pub fn fixed(px: f64) -> Self {
        Self { sizing: Sizing::Fixed, value: px }
    }
    pub fn hug() -> Self {
        Self { sizing: Sizing::Hug, value: 0.0 }
    }
    pub fn fill(fr: f64) -> Self {
        Self { sizing: Sizing::Fill, value: fr }
    }
}

/// Layout properties a node has as a child (of an auto layout frame) or as a resizable box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ChildLayout {
    pub sizing_h: Sizing,
    pub sizing_v: Sizing,
    /// Ignore auto layout (absolute position inside an auto layout frame).
    pub absolute: bool,
    pub min_w: Option<f64>,
    pub max_w: Option<f64>,
    pub min_h: Option<f64>,
    pub max_h: Option<f64>,
    /// Grid auto layout: anchor column / row (0-based); `None` = placed automatically.
    pub grid_column: Option<u32>,
    pub grid_row: Option<u32>,
    /// Grid auto layout: how many columns / rows the child spans (0 is read as 1).
    pub grid_column_span: u32,
    pub grid_row_span: u32,
}

impl ChildLayout {
    /// Column span, at least 1.
    pub fn column_span(&self) -> u32 {
        self.grid_column_span.max(1)
    }
    /// Row span, at least 1.
    pub fn row_span(&self) -> u32 {
        self.grid_row_span.max(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum GridKind {
    #[default]
    Grid,
    Columns,
    Rows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum GridAlign {
    #[default]
    Stretch,
    Min,
    Center,
    Max,
}

/// A layout grid on a frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutGrid {
    pub kind: GridKind,
    /// Grid: cell size. Columns/rows: count (0 = auto).
    pub size: f64,
    pub count: u32,
    pub gutter: f64,
    pub offset: f64,
    pub align: GridAlign,
    pub color: Color,
    pub visible: bool,
}

impl Default for LayoutGrid {
    fn default() -> Self {
        Self {
            kind: GridKind::Grid,
            size: 10.0,
            count: 5,
            gutter: 20.0,
            offset: 0.0,
            align: GridAlign::Stretch,
            color: Color::new(1.0, 0.0, 0.0, 0.1),
            visible: true,
        }
    }
}

impl LayoutGrid {
    pub fn columns() -> Self {
        Self { kind: GridKind::Columns, count: 5, gutter: 20.0, size: 0.0, ..Self::default() }
    }
    pub fn rows() -> Self {
        Self { kind: GridKind::Rows, count: 5, gutter: 20.0, size: 0.0, color: Color::new(0.0, 0.4, 1.0, 0.1), ..Self::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    #[default]
    Png,
    Jpg,
    Svg,
    Pdf,
    Webp,
}

impl ExportFormat {
    /// Every format, in the order the export menus list them.
    pub const ALL: [ExportFormat; 5] = [ExportFormat::Png, ExportFormat::Jpg, ExportFormat::Svg, ExportFormat::Pdf, ExportFormat::Webp];

    /// Pixel formats (their size depends on the export scale).
    pub fn is_raster(self) -> bool {
        matches!(self, ExportFormat::Png | ExportFormat::Jpg | ExportFormat::Webp)
    }

    /// The name menus show ("PNG", "WebP").
    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpg => "JPG",
            ExportFormat::Svg => "SVG",
            ExportFormat::Pdf => "PDF",
            ExportFormat::Webp => "WebP",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpg => "jpg",
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Webp => "webp",
        }
    }
    pub fn from_ext(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().trim_start_matches('.') {
            "png" => Some(ExportFormat::Png),
            "jpg" | "jpeg" => Some(ExportFormat::Jpg),
            "svg" => Some(ExportFormat::Svg),
            "pdf" => Some(ExportFormat::Pdf),
            "webp" => Some(ExportFormat::Webp),
            _ => None,
        }
    }
}

/// Export size constraint.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "camelCase")]
pub enum ExportSize {
    Scale { value: f64 },
    Width { value: f64 },
    Height { value: f64 },
}

impl Default for ExportSize {
    fn default() -> Self {
        ExportSize::Scale { value: 1.0 }
    }
}

impl ExportSize {
    /// Scale presets offered by the export size menu.
    pub const SCALES: [f64; 7] = [0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0];
    /// Largest export scale.
    pub const MAX_SCALE: f64 = 64.0;
    /// Largest exported width or height.
    pub const MAX_PIXELS: f64 = 16384.0;

    /// Parse "2x", "0.5X", "@2x", "2" (scale), "512w" (width) or "512h" (height). Values must be
    /// positive and within the export limits.
    pub fn parse(s: &str) -> Option<ExportSize> {
        let s = s.trim().trim_start_matches('@').to_ascii_lowercase();
        let (num, unit) = match s.char_indices().last() {
            Some((i, c)) if c.is_ascii_alphabetic() => (s.get(..i)?.trim(), Some(c)),
            _ => (s.as_str(), None),
        };
        let value: f64 = num.parse().ok()?;
        match unit {
            None | Some('x') => ExportSize::Scale { value },
            Some('w') => ExportSize::Width { value },
            Some('h') => ExportSize::Height { value },
            _ => return None,
        }
        .checked()
    }

    /// The size when its value is positive and within the export limits.
    pub fn checked(self) -> Option<ExportSize> {
        let (value, max) = match self {
            ExportSize::Scale { value } => (value, Self::MAX_SCALE),
            ExportSize::Width { value } | ExportSize::Height { value } => (value, Self::MAX_PIXELS),
        };
        (value.is_finite() && value > 0.0 && value <= max).then_some(self)
    }

    /// "2x", "512w", "512h".
    pub fn label(self) -> String {
        let n = |v: f64| crate::text::trim_num(v);
        match self {
            ExportSize::Scale { value } => format!("{}x", n(value)),
            ExportSize::Width { value } => format!("{}w", n(value)),
            ExportSize::Height { value } => format!("{}h", n(value)),
        }
    }

    /// The scale that gives a `w` × `h` box this size (1 for an empty box).
    pub fn scale_for(self, w: f64, h: f64) -> f64 {
        let s = match self {
            ExportSize::Scale { value } => value,
            ExportSize::Width { value } if w > 0.0 => value / w,
            ExportSize::Height { value } if h > 0.0 => value / h,
            _ => 1.0,
        };
        if s.is_finite() && s > 0.0 { s.min(Self::MAX_SCALE) } else { 1.0 }
    }
}

/// One row of a node's Export section.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ExportSetting {
    pub size: ExportSize,
    pub suffix: String,
    pub format: ExportFormat,
}

impl ExportSetting {
    /// The row adding a setting after `last` creates: 1x first, then one scale step up with a
    /// matching "@2x"-style suffix.
    pub fn next_after(last: Option<&ExportSetting>) -> ExportSetting {
        match last.map(|e| e.size) {
            Some(ExportSize::Scale { value }) => {
                let size = ExportSize::Scale { value: (value.floor() + 1.0).min(ExportSize::MAX_SCALE) };
                ExportSetting { suffix: format!("@{}", size.label()), size, format: ExportFormat::Png }
            }
            _ => ExportSetting::default(),
        }
    }

    /// What the exported file name gets after the layer name: the suffix, else "@2x"-style for
    /// raster exports at another scale.
    pub fn file_suffix(&self) -> String {
        match self.size {
            _ if !self.suffix.is_empty() => self.suffix.clone(),
            ExportSize::Scale { value } if (value - 1.0).abs() > 1e-9 && self.format.is_raster() => format!("@{}", self.size.label()),
            _ => String::new(),
        }
    }
}
