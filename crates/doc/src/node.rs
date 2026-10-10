//! Nodes: one struct with the properties every node can have, and a kind with per-type data.

use crate::NodeId;
use crate::props::*;
use crate::proto::{Interaction, Overflow, ScrollPosition};
use crate::styles::{CollectionId, ModeId, StyleRefs, VariableId};
use crate::text::TextData;
use crate::vector::VectorNetwork;
use figcraft_color::{BlendMode, Color, Paint};
use figcraft_geom::Affine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Boolean operation of a boolean group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BoolOp {
    #[default]
    Union,
    Subtract,
    Intersect,
    Exclude,
}

impl BoolOp {
    pub fn label(self) -> &'static str {
        match self {
            BoolOp::Union => "Union",
            BoolOp::Subtract => "Subtract",
            BoolOp::Intersect => "Intersect",
            BoolOp::Exclude => "Exclude",
        }
    }
}

/// A component property definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PropDef {
    Boolean {
        default: bool,
    },
    Text {
        default: String,
    },
    InstanceSwap {
        default: NodeId,
    },
    Variant {
        options: Vec<String>,
        default: String,
    },
    /// A frame (bound through `prop_refs["slot"]`) whose instances take any content of their own;
    /// `preferred` components are offered first when filling it.
    Slot {
        #[serde(default)]
        preferred: Vec<NodeId>,
    },
}

/// What a node is, with per-type data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeKind {
    Page {
        #[serde(default = "page_bg")]
        background: Color,
        #[serde(default)]
        flows: Vec<crate::proto::Flow>,
        #[serde(default)]
        prototype: crate::proto::PrototypeSettings,
        #[serde(default)]
        guides: Vec<Guide>,
    },
    Frame,
    Group,
    Section {
        #[serde(default)]
        contents_hidden: bool,
    },
    Rectangle,
    Ellipse {
        #[serde(default)]
        start: f64,
        #[serde(default = "tau")]
        sweep: f64,
        #[serde(default)]
        inner: f64,
    },
    Polygon {
        count: u32,
    },
    Star {
        count: u32,
        inner_ratio: f64,
    },
    Line,
    Vector {
        network: VectorNetwork,
    },
    Text(TextData),
    BooleanOp {
        op: BoolOp,
    },
    Component {
        #[serde(default)]
        description: String,
        #[serde(default)]
        props: BTreeMap<String, PropDef>,
    },
    ComponentSet {
        #[serde(default)]
        description: String,
    },
    Instance {
        component: NodeId,
        /// Values for the component's properties.
        #[serde(default)]
        props: BTreeMap<String, serde_json::Value>,
    },
    Slice,
}

fn page_bg() -> Color {
    Color::DEFAULT_PAGE
}
fn tau() -> f64 {
    std::f64::consts::TAU
}

/// How a mask layer masks the layers above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MaskType {
    /// The mask's rendered alpha (soft edges, gradients, opacity).
    #[default]
    Alpha,
    /// The mask's outline geometry, fully opaque (fills and opacity don't matter).
    Vector,
    /// The mask's brightness: white shows, black hides.
    Luminance,
}

impl MaskType {
    pub const ALL: [MaskType; 3] = [MaskType::Alpha, MaskType::Vector, MaskType::Luminance];
    pub fn is_alpha(&self) -> bool {
        *self == MaskType::Alpha
    }
    pub fn id(self) -> &'static str {
        match self {
            MaskType::Alpha => "alpha",
            MaskType::Vector => "vector",
            MaskType::Luminance => "luminance",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            MaskType::Alpha => "Alpha",
            MaskType::Vector => "Vector",
            MaskType::Luminance => "Luminance",
        }
    }
}

/// Handoff status of a frame or section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DevStatus {
    #[default]
    None,
    ReadyForDev,
    Completed,
}

impl DevStatus {
    pub const ALL: [DevStatus; 3] = [DevStatus::None, DevStatus::ReadyForDev, DevStatus::Completed];
    pub fn is_none(&self) -> bool {
        *self == DevStatus::None
    }
    pub fn id(self) -> &'static str {
        match self {
            DevStatus::None => "none",
            DevStatus::ReadyForDev => "readyForDev",
            DevStatus::Completed => "completed",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            DevStatus::None => "No status",
            DevStatus::ReadyForDev => "Ready for dev",
            DevStatus::Completed => "Completed",
        }
    }
}

/// A ruler guide on a page or frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    /// `true` for a vertical guide (x position), `false` for horizontal (y).
    pub vertical: bool,
    pub offset: f64,
}

impl NodeKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::Page { .. } => "PAGE",
            NodeKind::Frame => "FRAME",
            NodeKind::Group => "GROUP",
            NodeKind::Section { .. } => "SECTION",
            NodeKind::Rectangle => "RECTANGLE",
            NodeKind::Ellipse { .. } => "ELLIPSE",
            NodeKind::Polygon { .. } => "POLYGON",
            NodeKind::Star { .. } => "STAR",
            NodeKind::Line => "LINE",
            NodeKind::Vector { .. } => "VECTOR",
            NodeKind::Text(_) => "TEXT",
            NodeKind::BooleanOp { .. } => "BOOLEAN_OPERATION",
            NodeKind::Component { .. } => "COMPONENT",
            NodeKind::ComponentSet { .. } => "COMPONENT_SET",
            NodeKind::Instance { .. } => "INSTANCE",
            NodeKind::Slice => "SLICE",
        }
    }

    /// The label the design panel's type dropdown shows.
    pub fn label(&self) -> &'static str {
        match self {
            NodeKind::Page { .. } => "Page",
            NodeKind::Frame => "Frame",
            NodeKind::Group => "Group",
            NodeKind::Section { .. } => "Section",
            NodeKind::Rectangle => "Rectangle",
            NodeKind::Ellipse { .. } => "Ellipse",
            NodeKind::Polygon { .. } => "Polygon",
            NodeKind::Star { .. } => "Star",
            NodeKind::Line => "Line",
            NodeKind::Vector { .. } => "Vector",
            NodeKind::Text(_) => "Text",
            NodeKind::BooleanOp { op } => op.label(),
            NodeKind::Component { .. } => "Component",
            NodeKind::ComponentSet { .. } => "Component set",
            NodeKind::Instance { .. } => "Instance",
            NodeKind::Slice => "Slice",
        }
    }

    /// Frame-like nodes: can have auto layout, clip content, layout grids, and act as artboards.
    pub fn is_frame_like(&self) -> bool {
        matches!(self, NodeKind::Frame | NodeKind::Component { .. } | NodeKind::ComponentSet { .. } | NodeKind::Instance { .. })
    }

    /// Nodes that may contain children.
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            NodeKind::Page { .. }
                | NodeKind::Frame
                | NodeKind::Group
                | NodeKind::Section { .. }
                | NodeKind::BooleanOp { .. }
                | NodeKind::Component { .. }
                | NodeKind::ComponentSet { .. }
                | NodeKind::Instance { .. }
        )
    }

    /// Group-like containers whose bounds follow their children.
    pub fn bounds_follow_children(&self) -> bool {
        matches!(self, NodeKind::Group | NodeKind::BooleanOp { .. })
    }

    /// Shapes with a geometric outline of their own.
    pub fn is_shape(&self) -> bool {
        matches!(
            self,
            NodeKind::Rectangle
                | NodeKind::Ellipse { .. }
                | NodeKind::Polygon { .. }
                | NodeKind::Star { .. }
                | NodeKind::Line
                | NodeKind::Vector { .. }
        )
    }

    /// Kinds whose corners can be rounded from the panel.
    pub fn has_corner_radius(&self) -> bool {
        matches!(
            self,
            NodeKind::Rectangle
                | NodeKind::Frame
                | NodeKind::Component { .. }
                | NodeKind::ComponentSet { .. }
                | NodeKind::Instance { .. }
                | NodeKind::Polygon { .. }
                | NodeKind::Star { .. }
                | NodeKind::Vector { .. }
                | NodeKind::Section { .. }
        )
    }

    pub fn text(&self) -> Option<&TextData> {
        match self {
            NodeKind::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut TextData> {
        match self {
            NodeKind::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// One node of the scene graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    #[serde(flatten)]
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<NodeId>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub children: Vec<NodeId>,
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub locked: bool,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f32,
    #[serde(default = "pass", skip_serializing_if = "is_pass")]
    pub blend: BlendMode,
    /// Transform relative to the parent (the page for top-level nodes).
    #[serde(default = "identity", skip_serializing_if = "is_identity")]
    pub transform: Affine,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub fills: Vec<Paint>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub strokes: Vec<Paint>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub stroke: StrokeProps,
    #[serde(default, skip_serializing_if = "is_default")]
    pub effects: Vec<Effect>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub corners: Corners,
    #[serde(default, skip_serializing_if = "is_default")]
    pub constraints: Constraints,
    /// Auto layout (frame-like nodes only).
    #[serde(default, skip_serializing_if = "is_default")]
    pub layout: Option<AutoLayout>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub child_layout: ChildLayout,
    /// Clip content (frames).
    #[serde(default, skip_serializing_if = "is_default")]
    pub clip: bool,
    /// This node masks the siblings above it.
    #[serde(default, skip_serializing_if = "is_default")]
    pub is_mask: bool,
    /// How a mask masks: by its alpha, its outline, or its luminance.
    #[serde(default, skip_serializing_if = "MaskType::is_alpha")]
    pub mask_type: MaskType,
    #[serde(default, skip_serializing_if = "is_default")]
    pub grids: Vec<LayoutGrid>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub exports: Vec<ExportSetting>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub interactions: Vec<Interaction>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub overflow: Overflow,
    #[serde(default, skip_serializing_if = "is_default")]
    pub scroll_position: ScrollPosition,
    #[serde(default, skip_serializing_if = "is_default")]
    pub styles: StyleRefs,
    /// Variable bindings: field name ("fills/0/color", "width", "cornerRadius", "characters", …) → id.
    #[serde(default, skip_serializing_if = "is_default")]
    pub bound: BTreeMap<String, VariableId>,
    /// Explicit variable modes (collection → mode) set on this container.
    #[serde(default, skip_serializing_if = "is_default")]
    pub var_modes: BTreeMap<CollectionId, ModeId>,
    /// Lock aspect ratio when resizing from the panel.
    #[serde(default, skip_serializing_if = "is_default")]
    pub constrain_proportions: bool,
    /// Inside an instance: the node of the main component this one mirrors.
    #[serde(default, skip_serializing_if = "is_default")]
    pub source: Option<NodeId>,
    /// A nested instance inside a main component whose properties show on the main's instances.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exposed: bool,

    /// Handoff status of a frame or section (shown as a badge next to its name).
    #[serde(default, skip_serializing_if = "DevStatus::is_none")]
    pub dev_status: DevStatus,
    /// Inside an instance: property groups overridden locally ("fills", "characters", …; "slot":
    /// this slot holds the instance's own content).
    #[serde(default, skip_serializing_if = "is_default")]
    pub overrides: BTreeSet<String>,
    /// Component property references: which property controls this node's
    /// "visible" (boolean), "characters" (text) or "mainComponent" (instance swap), or which slot
    /// property this frame is ("slot").
    #[serde(default, skip_serializing_if = "is_default")]
    pub prop_refs: BTreeMap<String, String>,
    /// Free-form plugin / agent data.
    #[serde(default, skip_serializing_if = "is_default")]
    pub plugin_data: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}
fn pass() -> BlendMode {
    BlendMode::PassThrough
}
fn identity() -> Affine {
    Affine::IDENTITY
}

// Fields at their default value are left out of the JSON (the file is several times smaller and
// faster to write and read); reading fills them back in from the same defaults.

/// Whether `v` is its type's default (what `#[serde(default)]` reads back for a missing field).
pub(crate) fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}
fn is_yes(v: &bool) -> bool {
    *v == yes()
}
fn is_one(v: &f32) -> bool {
    *v == one()
}
fn is_pass(v: &BlendMode) -> bool {
    *v == pass()
}
fn is_identity(v: &Affine) -> bool {
    *v == identity()
}

impl Node {
    /// A node with Figma's defaults for its kind.
    pub fn new(id: NodeId, kind: NodeKind, name: impl Into<String>, width: f64, height: f64) -> Self {
        let fills = match &kind {
            NodeKind::Frame | NodeKind::Component { .. } => vec![Paint::solid(Color::WHITE)],
            NodeKind::Section { .. } => vec![Paint::solid(Color::WHITE)],
            NodeKind::Rectangle | NodeKind::Ellipse { .. } | NodeKind::Polygon { .. } | NodeKind::Star { .. } | NodeKind::Vector { .. } => {
                vec![Paint::solid(Color::DEFAULT_SHAPE)]
            }
            NodeKind::Text(_) => vec![Paint::solid(Color::BLACK)],
            _ => vec![],
        };
        let strokes = match &kind {
            NodeKind::Line => vec![Paint::solid(Color::BLACK)],
            NodeKind::ComponentSet { .. } => vec![Paint::solid(Color::from_rgb8_const(0x97, 0x47, 0xFF))],
            _ => vec![],
        };
        let stroke = match &kind {
            NodeKind::Line | NodeKind::Vector { .. } => StrokeProps { align: StrokeAlign::Center, ..Default::default() },
            NodeKind::ComponentSet { .. } => StrokeProps { dashes: vec![10.0, 5.0], ..Default::default() },
            _ => StrokeProps::default(),
        };
        let blend = if kind.is_container() { BlendMode::PassThrough } else { BlendMode::Normal };
        let clip = matches!(kind, NodeKind::Frame | NodeKind::Component { .. } | NodeKind::ComponentSet { .. } | NodeKind::Instance { .. });
        let corners = match &kind {
            NodeKind::ComponentSet { .. } => Corners { radii: [5.0; 4], smoothing: 0.0 },
            _ => Corners::default(),
        };
        Self {
            id,
            name: name.into(),
            kind,
            parent: None,
            children: Vec::new(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend,
            transform: Affine::IDENTITY,
            width: width.max(0.0),
            height: height.max(0.0),
            fills,
            strokes,
            stroke,
            effects: Vec::new(),
            corners,
            constraints: Constraints::default(),
            layout: None,
            child_layout: ChildLayout::default(),
            clip,
            is_mask: false,
            mask_type: MaskType::Alpha,
            grids: Vec::new(),
            exports: Vec::new(),
            interactions: Vec::new(),
            overflow: Default::default(),
            scroll_position: Default::default(),
            styles: StyleRefs::default(),
            bound: BTreeMap::new(),
            var_modes: BTreeMap::new(),
            constrain_proportions: false,
            source: None,
            exposed: false,

            dev_status: DevStatus::None,
            overrides: BTreeSet::new(),
            prop_refs: BTreeMap::new(),
            plugin_data: BTreeMap::new(),
        }
    }

    pub fn size(&self) -> (f64, f64) {
        (self.width, self.height)
    }

    pub fn local_rect(&self) -> figcraft_geom::Rect {
        figcraft_geom::Rect::new(0.0, 0.0, self.width, self.height)
    }

    pub fn is_page(&self) -> bool {
        matches!(self.kind, NodeKind::Page { .. })
    }

    /// Whether painted strokes are present.
    pub fn has_visible_stroke(&self) -> bool {
        self.strokes.iter().any(|p| p.visible) && self.stroke.weight > 0.0
    }

    /// Maximum distance (in local units) painting can reach outside the node box.
    pub fn paint_reach(&self) -> f64 {
        let stroke = if self.has_visible_stroke() {
            let w = self.stroke.sides.map(|s| s.iter().copied().fold(0.0, f64::max)).unwrap_or(self.stroke.weight);
            let base = match self.stroke.align {
                StrokeAlign::Inside => 0.0,
                StrokeAlign::Center => w / 2.0,
                StrokeAlign::Outside => w,
            };
            let caps = if self.stroke.cap_start.is_arrow() || self.stroke.cap_end.is_arrow() { w * 3.0 } else { w };
            base.max(if matches!(self.kind, NodeKind::Line | NodeKind::Vector { .. }) { caps } else { 0.0 }) * self.stroke.miter_limit.clamp(1.0, 4.0)
        } else {
            0.0
        };
        let fx = self.effects.iter().map(Effect::reach).fold(0.0, f64::max);
        stroke + fx
    }
}
