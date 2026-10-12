//! Export to `.fig` (experimental, best effort), from the same public format knowledge as the
//! import: the inverse of its mapping ([`super::map`]).
//!
//! The file is the zip the import reads: `canvas.fig` (the fig-kiwi header, then the binary schema
//! and the message, each a raw-deflate chunk), `meta.json`, an optional `thumbnail.png` and
//! `images/<hash>`. The schema travels in the file, so it describes only the fields written here
//! ([`SCHEMA`]). The message lists one node change per layer (its id, its parent and its position
//! among the siblings, then its fields), parents first, plus blobs (vector networks and outlines).
//! Instances are written as their main and the fields their layers override (by id path), like
//! the files the import reads. What the format can't carry yet (shared styles, variables, Dev Mode
//! data, …) is left out and listed in the warnings. Round trips through our own importer are
//! tested, and written files were opened in other software (`docs/file-format-parity.md` says what
//! was checked); other apps may not open every file.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;

use figcraft_doc::color::{BlendMode, Color, GradientKind, ImageScaleMode, Paint, PaintKind};
use figcraft_doc::geom::kurbo::PathEl;
use figcraft_doc::geom::{Affine, BezPath};
use figcraft_doc::proto::{
    Action, Direction, Easing, GAMEPAD_BUTTONS, Interaction, KeyMods, Overflow, OverlayPosition, ScrollPosition, Transition, Trigger,
};
use figcraft_doc::{
    AutoLayout, BoolOp, CrossAlign, Document, EffectKind, ExportSize, GridAlign, GridKind, HConstraint, LayoutMode, LetterSpacing, LineHeight,
    ListStyle, MainAlign, MaskType, Node, NodeId, NodeKind, NoiseMode, PropDef, Sizing, StrokeAlign, StrokeCap, StrokeJoin, TextAlignH, TextAlignV,
    TextAutoResize, TextCase, TextData, TextDecoration, TextStyle, VConstraint, VectorNetwork, WindingRule,
};
use serde_json::Value;

use super::kiwi::{Kv, Schema, encode};
use super::network;
use crate::FormatError;

/// Most layers written, and deepest nesting followed (the import's limits).
const MAX_NODES: usize = crate::rest::MAX_NODES;
const MAX_DEPTH: usize = 256;
/// Largest `canvas.fig` and image written (what the import reads).
const MAX_CANVAS: u64 = super::MAX_CANVAS;
const MAX_IMAGE: u64 = super::MAX_IMAGE;
/// Largest whole file written.
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
/// Most characters of one text layer, and vertices + segments + loop entries of one vector
/// network (what the import reads).
const MAX_CHARS: usize = 1 << 20;
const MAX_NETWORK: usize = 200_000;
/// Largest magnitude written for a float (the format stores 32-bit floats).
const MAX_FLOAT: f64 = 1e30;
/// The version number in the `canvas.fig` header.
const VERSION: u32 = 101;
/// Session ids of the GUIDs: layers, and grid tracks and component properties.
const LAYERS: u32 = 1;
const AUX: u32 = 2;

/// The schema written into every file: `enum Name A=value …`, `struct Name type field, …` (every
/// field, in order) and `message Name type field=id, …`, with the names, kinds and numbers the
/// format gives them. Only what the export writes.
pub(super) const SCHEMA: &str = "
enum MessageType JOIN_START=0 NODE_CHANGES=1
enum NodePhase CREATED=0 REMOVED=1
enum NodeType NONE=0 DOCUMENT=1 CANVAS=2 GROUP=3 FRAME=4 BOOLEAN_OPERATION=5 VECTOR=6 STAR=7 LINE=8 ELLIPSE=9 RECTANGLE=10 REGULAR_POLYGON=11 ROUNDED_RECTANGLE=12 TEXT=13 SLICE=14 SYMBOL=15 INSTANCE=16 SECTION=25
enum BlendMode PASS_THROUGH=0 NORMAL=1 DARKEN=2 MULTIPLY=3 LINEAR_BURN=4 COLOR_BURN=5 LIGHTEN=6 SCREEN=7 LINEAR_DODGE=8 COLOR_DODGE=9 OVERLAY=10 SOFT_LIGHT=11 HARD_LIGHT=12 DIFFERENCE=13 EXCLUSION=14 HUE=15 SATURATION=16 COLOR=17 LUMINOSITY=18
enum PaintType SOLID=0 GRADIENT_LINEAR=1 GRADIENT_RADIAL=2 GRADIENT_ANGULAR=3 GRADIENT_DIAMOND=4 IMAGE=5
enum ImageScaleMode STRETCH=0 FIT=1 FILL=2 TILE=3
enum WindingRule NONZERO=0 ODD=1
enum StrokeAlign CENTER=0 INSIDE=1 OUTSIDE=2
enum StrokeCap NONE=0 ROUND=1 SQUARE=2 ARROW_LINES=3 ARROW_EQUILATERAL=4 TRIANGLE_FILLED=6 DIAMOND_FILLED=5 CIRCLE_FILLED=14
enum StrokeJoin MITER=0 BEVEL=1 ROUND=2
enum ConstraintType MIN=0 CENTER=1 MAX=2 STRETCH=3 SCALE=4
enum StackMode NONE=0 HORIZONTAL=1 VERTICAL=2 GRID=3
enum StackAlign MIN=0 CENTER=1 MAX=2 BASELINE=3
enum StackJustify MIN=0 CENTER=1 MAX=2 SPACE_EVENLY=3 SPACE_BETWEEN=4 SPACE_AROUND=5
enum StackCounterAlign MIN=0 CENTER=1 MAX=2 STRETCH=3 AUTO=4 BASELINE=5
enum StackSize FIXED=0 RESIZE_TO_FIT=1
enum StackWrap NO_WRAP=0 WRAP=1
enum StackPositioning AUTO=0 ABSOLUTE=1
enum EffectType INNER_SHADOW=0 DROP_SHADOW=1 FOREGROUND_BLUR=2 BACKGROUND_BLUR=3 NOISE=7 GRAIN=6 GLASS=8
enum BlurOpType NORMAL=0 PROGRESSIVE=1
enum NoiseType MONOTONE=1 DUOTONE=2 MULTITONE=0
enum TextAlignHorizontal LEFT=0 CENTER=1 RIGHT=2 JUSTIFIED=3
enum TextAlignVertical TOP=0 CENTER=1 BOTTOM=2
enum TextAutoResize NONE=0 WIDTH_AND_HEIGHT=1 HEIGHT=2
enum TextTruncation DISABLED=0 ENDING=1
enum TextCase ORIGINAL=0 UPPER=1 LOWER=2 TITLE=3 SMALL_CAPS=4
enum TextDecoration NONE=0 UNDERLINE=1 STRIKETHROUGH=2
enum NumberUnits RAW=0 PIXELS=1 PERCENT=2
enum LineType PLAIN=0 ORDERED_LIST=1 UNORDERED_LIST=2
enum SourceDirectionality AUTO=0 LTR=1 RTL=2
enum DocumentColorProfile LEGACY=0 SRGB=1 DISPLAY_P3=2
enum EmojiImageSet APPLE=0 NOTO=1
enum MaskType ALPHA=0 OUTLINE=1 LUMINANCE=2
enum VariableResolvedDataType BOOLEAN=0 FLOAT=1 STRING=2 COLOR=4 SYMBOL_ID=6 TEXT_DATA=8
enum VectorMirror NONE=0 ANGLE=1 ANGLE_AND_LENGTH=2
enum BooleanOperation UNION=0 INTERSECT=1 SUBTRACT=2 XOR=3
enum LayoutGridType MIN=0 CENTER=1 STRETCH=2 MAX=3
enum LayoutGridPattern STRIPES=0 GRID=1
enum Axis X=0 Y=1
enum ImageType PNG=0 JPEG=1 SVG=2 PDF=3
enum ExportConstraintType CONTENT_SCALE=0 CONTENT_WIDTH=1 CONTENT_HEIGHT=2
enum ComponentPropType BOOL=0 TEXT=1 INSTANCE_SWAP=3 VARIANT=4
enum VariableField MISSING=0 VISIBLE=10 TEXT_DATA=11 OVERRIDDEN_SYMBOL_ID=37
enum VariableDataType BOOLEAN=0 FLOAT=1 STRING=2 ALIAS=3 COLOR=4 PROP_REF=13
enum InteractionType ON_CLICK=0 DRAG=9 ON_HOVER=4 ON_PRESS=7 MOUSE_IN=2 MOUSE_OUT=3 MOUSE_DOWN=5 MOUSE_UP=6 AFTER_TIMEOUT=1 ON_KEY_DOWN=10
enum TriggerDevice KEYBOARD=0 UNKNOWN_CONTROLLER=1
enum ConnectionType NONE=0 INTERNAL_NODE=1 URL=2 BACK=3 CLOSE=4
enum NavigationType NAVIGATE=0 OVERLAY=1 SWAP=2 SWAP_STATE=3 SCROLL_TO=4
enum TransitionType INSTANT_TRANSITION=0 DISSOLVE=1 SMART_ANIMATE=24 SLIDE_FROM_LEFT=3 SLIDE_FROM_RIGHT=4 SLIDE_FROM_TOP=5 SLIDE_FROM_BOTTOM=6 PUSH_FROM_LEFT=7 PUSH_FROM_RIGHT=8 PUSH_FROM_TOP=9 PUSH_FROM_BOTTOM=10 MOVE_FROM_LEFT=11 MOVE_FROM_RIGHT=12 MOVE_FROM_TOP=13 MOVE_FROM_BOTTOM=14 SLIDE_OUT_TO_LEFT=15 SLIDE_OUT_TO_RIGHT=16 SLIDE_OUT_TO_TOP=17 SLIDE_OUT_TO_BOTTOM=18 MOVE_OUT_TO_LEFT=19 MOVE_OUT_TO_RIGHT=20 MOVE_OUT_TO_TOP=21 MOVE_OUT_TO_BOTTOM=22
enum EasingType IN_CUBIC=0 OUT_CUBIC=1 INOUT_CUBIC=2 LINEAR=3 IN_BACK_CUBIC=4 OUT_BACK_CUBIC=5 INOUT_BACK_CUBIC=6 CUSTOM_CUBIC=7 GENTLE_SPRING=9
enum ScrollDirection NONE=0 HORIZONTAL=1 VERTICAL=2 BOTH=3
enum ScrollBehavior SCROLLS=0 FIXED_WHEN_CHILD_OF_SCROLLING_FRAME=1 STICKY_SCROLLS=2
enum OverlayPositionType CENTER=0 TOP_LEFT=1 TOP_CENTER=2 TOP_RIGHT=3 BOTTOM_LEFT=4 BOTTOM_CENTER=5 BOTTOM_RIGHT=6 MANUAL=7
enum OverlayBackgroundInteraction NONE=0 CLOSE_ON_CLICK_OUTSIDE=1
enum OverlayBackgroundType NONE=0 SOLID_COLOR=1
struct GUID uint sessionID, uint localID
struct Color float r, float g, float b, float a
struct Vector float x, float y
struct Matrix float m00, float m01, float m02, float m10, float m11, float m12
struct ColorStop Color color, float position
struct ParentIndex GUID guid, string position
message Image byte[] hash=1, string name=2
message Paint PaintType type=1, Color color=2, float opacity=3, bool visible=4, BlendMode blendMode=5, ColorStop[] stops=6, Matrix transform=7, Image image=8, ImageScaleMode imageScaleMode=10, float rotation=11, float scale=12
message Effect EffectType type=1, Color color=2, Vector offset=3, float radius=4, bool visible=5, BlendMode blendMode=6, float spread=7, bool showShadowBehindNode=8, BlurOpType blurOpType=19, float startRadius=21, Vector startOffset=20, Vector endOffset=28, NoiseType noiseType=26, Vector noiseSize=22, float density=25, float opacity=27, Color secondaryColor=24, bool clipToShape=23
struct FontName string family, string style, string postscript
struct Number float value, NumberUnits units
message Hyperlink string url=1
message TextLineData LineType lineType=1, int styleId=10, int indentationLevel=2, SourceDirectionality sourceDirectionality=9, int listStartOffset=7, bool isFirstLineOfList=8
message TextData string characters=1, uint[] characterStyleIDs=2, NodeChange[] styleOverrideTable=3, TextLineData[] lines=12
message Path WindingRule windingRule=1, uint commandsBlob=2, uint styleID=3
message VectorData uint vectorNetworkBlob=1, Vector normalizedSize=2, NodeChange[] styleOverrideTable=3
message ArcData float startingAngle=1, float endingAngle=2, float innerRadius=3
message OptionalVector Vector value=1
message LayoutGrid LayoutGridType type=1, Axis axis=2, bool visible=3, int numSections=4, float offset=5, float sectionSize=6, float gutterSize=7, Color color=8, LayoutGridPattern pattern=9
struct ExportConstraint ExportConstraintType type, float value
message ExportSettings string suffix=1, ImageType imageType=2, ExportConstraint constraint=3
message GUIDPath GUID[] guids=1
message SymbolData GUID symbolID=1, NodeChange[] symbolOverrides=2
message ComponentPropValue bool boolValue=1, TextData textValue=2, GUID guidValue=3
message ComponentPropDef GUID id=1, string name=2, ComponentPropValue initialValue=3, ComponentPropType type=6
message ComponentPropAssignment GUID defID=1, ComponentPropValue value=2
message PropRefValue GUID defId=1
message VariableAnyValue PropRefValue propRefValue=13
message VariableData VariableAnyValue value=1, VariableDataType dataType=2, VariableResolvedDataType resolvedDataType=3
message VariableDataMapEntry VariableData variableData=2, VariableField variableField=3
message VariableDataMap VariableDataMapEntry[] entries=1
message KeyTrigger int[] keyCodes=1, TriggerDevice triggerDevice=2
message PrototypeEvent InteractionType interactionType=1, float transitionTimeout=6, KeyTrigger keyTrigger=4
message PrototypeAction GUID transitionNodeID=1, TransitionType transitionType=2, float transitionDuration=3, EasingType easingType=4, float[] easingFunction=12, ConnectionType connectionType=7, NavigationType navigationType=10, string connectionURL=8, bool transitionPreserveScroll=11
message PrototypeInteraction PrototypeEvent event=2, PrototypeAction[] actions=3
message PrototypeStartingPoint string name=1, string position=3
message OverlayBackgroundAppearance OverlayBackgroundType backgroundType=1, Color backgroundColor=2
message GUIDPositionMapEntry GUID id=1, string position=2
message GUIDPositionMap GUIDPositionMapEntry[] entries=1
message NodeChange GUID guid=1, NodePhase phase=2, ParentIndex parentIndex=3, NodeType type=4, string name=5, bool visible=6, bool locked=7, float opacity=8, BlendMode blendMode=9, Vector size=11, Matrix transform=12, bool mask=16, MaskType maskType=317, float backgroundOpacity=19, bool backgroundEnabled=15, bool internalOnly=142, DocumentColorProfile documentColorProfile=349, Paint[] fillPaints=38, Paint[] strokePaints=39, float strokeWeight=26, StrokeAlign strokeAlign=29, StrokeJoin strokeJoin=31, StrokeCap strokeCap=30, float[] dashPattern=13, float miterLimit=25, bool borderStrokeWeightsIndependent=299, float borderTopWeight=295, float borderRightWeight=298, float borderBottomWeight=296, float borderLeftWeight=297, Effect[] effects=43, float cornerRadius=20, float cornerSmoothing=160, bool rectangleCornerRadiiIndependent=149, float rectangleTopLeftCornerRadius=145, float rectangleTopRightCornerRadius=146, float rectangleBottomRightCornerRadius=148, float rectangleBottomLeftCornerRadius=147, ConstraintType horizontalConstraint=28, ConstraintType verticalConstraint=37, bool frameMaskDisabled=115, bool resizeToFit=117, bool isStateGroup=225, bool sectionContentsHidden=283, Color backgroundColor=50, StackMode stackMode=105, float stackSpacing=107, float stackCounterSpacing=324, StackWrap stackWrap=323, float stackHorizontalPadding=209, float stackVerticalPadding=210, float stackPaddingRight=233, float stackPaddingBottom=234, StackJustify stackPrimaryAlignItems=230, StackAlign stackCounterAlignItems=231, StackSize stackPrimarySizing=229, StackSize stackCounterSizing=221, bool bordersTakeSpace=294, bool stackReverseZIndex=271, float gridColumnGap=438, float gridRowGap=437, GUIDPositionMap gridColumns=436, GUIDPositionMap gridRows=435, float stackChildPrimaryGrow=232, StackCounterAlign stackChildAlignSelf=236, StackPositioning stackPositioning=269, OptionalVector minSize=325, OptionalVector maxSize=326, GUID gridColumnAnchor=440, GUID gridRowAnchor=439, uint gridColumnSpan=442, uint gridRowSpan=441, LayoutGrid[] layoutGrids=47, ExportSettings[] exportSettings=45, TextData textData=42, FontName fontName=41, float fontSize=21, Number lineHeight=40, Number letterSpacing=165, TextCase textCase=34, TextDecoration textDecoration=35, TextAlignHorizontal textAlignHorizontal=32, TextAlignVertical textAlignVertical=33, TextAutoResize textAutoResize=46, TextTruncation textTruncation=280, int maxLines=351, float paragraphSpacing=23, float paragraphIndent=22, Hyperlink hyperlink=223, bool autoRename=14, bool fontVariantCommonLigatures=127, bool fontVariantContextualLigatures=128, string fontVersion=202, uint textUserLayoutVersion=203, uint textExplicitLayoutVersion=396, uint textBidiVersion=279, float textTracking=27, bool textDecorationSkipInk=412, EmojiImageSet emojiImageSet=391, uint styleID=49, Path[] fillGeometry=51, Path[] strokeGeometry=52, VectorData vectorData=48, VectorMirror handleMirroring=44, uint count=10, float starInnerScale=24, ArcData arcData=195, BooleanOperation booleanOperation=36, string symbolDescription=144, SymbolData symbolData=113, GUID overriddenSymbolID=143, ComponentPropDef[] componentPropDefs=266, ComponentPropAssignment[] componentPropAssignments=268, VariableDataMap parameterConsumptionMap=445, GUIDPath guidPath=111, PrototypeInteraction[] prototypeInteractions=226, PrototypeStartingPoint prototypeStartingPoint=249, ScrollDirection scrollDirection=159, ScrollBehavior scrollBehavior=186, OverlayPositionType overlayPositionType=198, OverlayBackgroundInteraction overlayBackgroundInteraction=200, OverlayBackgroundAppearance overlayBackgroundAppearance=201
struct Blob byte[] bytes
message Message MessageType type=1, uint sessionID=2, NodeChange[] nodeChanges=4, Blob[] blobs=6
";

/// Write `doc` as a `.fig` file, with `thumbnail` (a PNG) when given. Returns the bytes and the
/// warnings about what was left out or approximated.
pub fn export(doc: &Document, thumbnail: Option<&[u8]>) -> Result<(Vec<u8>, Vec<String>), FormatError> {
    let mut x = Exporter::new(doc);
    let message = x.message();
    let fail = |e: super::kiwi::KiwiError| FormatError::Io(format!(".fig export: {e}"));
    let schema = Schema::parse(SCHEMA).map_err(fail)?;
    let root = schema.find("Message").ok_or_else(|| FormatError::Io(".fig export: the schema has no Message".into()))?;
    let body = encode::message(&schema, root, &message).map_err(fail)?;
    let canvas = canvas(&encode::schema(&schema), &body)?;
    let background = doc.pages.first().and_then(|p| doc.node(*p)).and_then(|p| match &p.kind {
        NodeKind::Page { background, .. } => Some(*background),
        _ => None,
    });
    let bytes = archive(&doc.name, background.unwrap_or(Color::WHITE), &canvas, &x.files, thumbnail)?;
    Ok((bytes, x.warnings))
}

fn too_large() -> FormatError {
    FormatError::Io("the document is too large for a .fig file".into())
}

/// `canvas.fig`: the magic, the version and the two deflated chunks, each after its length.
fn canvas(schema: &[u8], message: &[u8]) -> Result<Vec<u8>, FormatError> {
    let mut out = b"fig-kiwi".to_vec();
    out.extend_from_slice(&VERSION.to_le_bytes());
    for chunk in [schema, message] {
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(chunk).map_err(crate::io_err)?;
        let c = z.finish().map_err(crate::io_err)?;
        let len = u32::try_from(c.len()).map_err(|_| too_large())?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&c);
    }
    if out.len() as u64 > MAX_CANVAS {
        return Err(too_large());
    }
    Ok(out)
}

/// The zip around `canvas.fig`.
fn archive(
    name: &str,
    background: Color,
    canvas: &[u8],
    images: &[(String, Arc<Vec<u8>>)],
    thumbnail: Option<&[u8]>,
) -> Result<Vec<u8>, FormatError> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        crate::put(&mut z, "canvas.fig", stored, canvas)?;
        if let Some(t) = thumbnail {
            crate::put(&mut z, "thumbnail.png", stored, t)?;
        }
        let mut client = serde_json::json!({"background_color": {"r": background.r, "g": background.g, "b": background.b, "a": background.a}});
        // A PNG says its size right after its signature and header chunk name.
        let be = |at: usize| thumbnail.and_then(|t| t.get(at..at + 4)).and_then(|b| b.try_into().ok()).map(u32::from_be_bytes);
        if let (Some(w), Some(h)) = (be(16), be(20)) {
            client["thumbnail_size"] = serde_json::json!({"width": w, "height": h});
        }
        let meta = serde_json::to_vec(&serde_json::json!({"client_meta": client, "file_name": name})).map_err(crate::io_err)?;
        crate::put(&mut z, "meta.json", crate::json_options(), &meta)?;
        let mut total = canvas.len() as u64;
        for (hash, bytes) in images {
            total += bytes.len() as u64;
            if total > MAX_FILE {
                return Err(too_large());
            }
            crate::put(&mut z, &format!("images/{hash}"), stored, bytes)?;
        }
        z.finish().map_err(crate::io_err)?;
    }
    Ok(buf.into_inner())
}

type Fields = Vec<(&'static str, Kv<'static>)>;

fn o(f: Fields) -> Kv<'static> {
    Kv::Object(f)
}
fn arr(v: Vec<Kv<'static>>) -> Kv<'static> {
    Kv::Array(v)
}
/// A float the format can hold: finite (non-finite values become 0) and within range.
fn fl(x: f64) -> Kv<'static> {
    Kv::Float(if x.is_finite() { x.clamp(-MAX_FLOAT, MAX_FLOAT) as f32 } else { 0.0 })
}
fn u(x: u32) -> Kv<'static> {
    Kv::Uint(u64::from(x))
}
fn int(x: i64) -> Kv<'static> {
    Kv::Int(x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)))
}
fn e(name: &'static str) -> Kv<'static> {
    Kv::Enum(name)
}
fn s(x: &str) -> Kv<'static> {
    Kv::Str(x.to_string())
}
fn b(x: bool) -> Kv<'static> {
    Kv::Bool(x)
}
fn guid(session: u32, local: u32) -> Kv<'static> {
    o(vec![("sessionID", u(session)), ("localID", u(local))])
}
fn xy(x: f64, y: f64) -> Kv<'static> {
    o(vec![("x", fl(x)), ("y", fl(y))])
}
fn color(c: Color) -> Kv<'static> {
    o(vec![("r", fl(c.r.into())), ("g", fl(c.g.into())), ("b", fl(c.b.into())), ("a", fl(c.a.into()))])
}
fn matrix(m: Affine) -> Kv<'static> {
    let [a, b, c, d, e, f] = m.as_coeffs();
    o(vec![("m00", fl(a)), ("m01", fl(c)), ("m02", fl(e)), ("m10", fl(b)), ("m11", fl(d)), ("m12", fl(f))])
}
/// What the document and its pages say besides their names: left out, a reader takes them as
/// hidden and fully transparent, and draws nothing below them.
fn root_fields() -> Fields {
    vec![
        ("visible", b(true)),
        ("opacity", fl(1.0)),
        ("transform", matrix(Affine::IDENTITY)),
        ("strokeWeight", fl(0.0)),
        ("strokeAlign", e("CENTER")),
        ("strokeJoin", e("BEVEL")),
    ]
}
/// A finite coordinate (vector network blobs store raw floats).
fn finite(x: f64) -> f64 {
    if x.is_finite() { x.clamp(-MAX_FLOAT, MAX_FLOAT) } else { 0.0 }
}

/// A position string that sorts as child `i` does among its siblings (fixed width, printable).
fn position(i: usize) -> String {
    let mut out = [0u8; 4];
    let mut v = i;
    for c in out.iter_mut().rev() {
        *c = b'!' + (v % 94) as u8;
        v /= 94;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The format's name of a blend mode: the one the import reads back as `m`.
fn blend_name(m: BlendMode) -> &'static str {
    const NAMES: [&str; 19] = [
        "PASS_THROUGH",
        "NORMAL",
        "DARKEN",
        "MULTIPLY",
        "LINEAR_BURN",
        "COLOR_BURN",
        "LIGHTEN",
        "SCREEN",
        "LINEAR_DODGE",
        "COLOR_DODGE",
        "OVERLAY",
        "SOFT_LIGHT",
        "HARD_LIGHT",
        "DIFFERENCE",
        "EXCLUSION",
        "HUE",
        "SATURATION",
        "COLOR",
        "LUMINOSITY",
    ];
    NAMES.into_iter().find(|n| crate::rest::props::blend_mode(n) == Some(m)).unwrap_or("NORMAL")
}

fn cap_name(c: StrokeCap) -> &'static str {
    match c {
        StrokeCap::None => "NONE",
        StrokeCap::Round => "ROUND",
        StrokeCap::Square => "SQUARE",
        StrokeCap::LineArrow => "ARROW_LINES",
        StrokeCap::TriangleArrow | StrokeCap::ReversedTriangle => "ARROW_EQUILATERAL",
        StrokeCap::CircleFilled => "CIRCLE_FILLED",
        StrokeCap::DiamondFilled => "DIAMOND_FILLED",
    }
}

fn join_name(j: StrokeJoin) -> &'static str {
    match j {
        StrokeJoin::Miter => "MITER",
        StrokeJoin::Bevel => "BEVEL",
        StrokeJoin::Round => "ROUND",
    }
}

/// A geometry blob (the import's `svg_path` reads it): commands `0` close, `1` move, `2` line,
/// `3` quadratic, `4` cubic, each followed by its points as little-endian `f32` pairs.
fn path_blob(p: &BezPath) -> Vec<u8> {
    let mut out = Vec::new();
    let mut put = |cmd: u8, pts: &[figcraft_doc::geom::Point]| {
        out.push(cmd);
        for q in pts {
            out.extend_from_slice(&(finite(q.x) as f32).to_le_bytes());
            out.extend_from_slice(&(finite(q.y) as f32).to_le_bytes());
        }
    };
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(a) => put(1, &[a]),
            PathEl::LineTo(a) => put(2, &[a]),
            PathEl::QuadTo(a, c) => put(3, &[a, c]),
            PathEl::CurveTo(a, c, d) => put(4, &[a, c, d]),
            PathEl::ClosePath => put(0, &[]),
        }
    }
    out
}

/// Settings an overlay's destination carries.
struct Overlay {
    position: OverlayPosition,
    close_on_outside: bool,
    background: Option<Color>,
}

struct Exporter<'d> {
    doc: &'d Document,
    /// Layer → local id of its GUID (session [`LAYERS`]).
    ids: BTreeMap<NodeId, u32>,
    nodes: Vec<Kv<'static>>,
    /// Blob bytes → index (identical blobs are written once).
    blobs: BTreeMap<Vec<u8>, u32>,
    blob_bytes: u64,
    /// Image key → its hash (lowercase hex) once written, `None` when its bytes aren't there.
    images: BTreeMap<String, Option<String>>,
    /// `images/<hash>` members.
    files: Vec<(String, Arc<Vec<u8>>)>,
    /// Component property (name, type) → local id of its definition's GUID (session [`AUX`]).
    defs: BTreeMap<(String, &'static str), u32>,
    /// Grid tracks of written grid frames: (columns, rows) local ids (session [`AUX`]).
    tracks: BTreeMap<NodeId, (Vec<u32>, Vec<u32>)>,
    aux: u32,
    overlays: BTreeMap<NodeId, Overlay>,
    /// Flow starting points: layer → (order, name).
    starts: BTreeMap<NodeId, (usize, String)>,
    warnings: Vec<String>,
}

impl<'d> Exporter<'d> {
    fn new(doc: &'d Document) -> Self {
        Self {
            doc,
            ids: BTreeMap::new(),
            nodes: Vec::new(),
            blobs: BTreeMap::new(),
            blob_bytes: 0,
            images: BTreeMap::new(),
            files: Vec::new(),
            defs: BTreeMap::new(),
            tracks: BTreeMap::new(),
            aux: 0,
            overlays: BTreeMap::new(),
            starts: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    fn warn(&mut self, w: &str) {
        if !self.warnings.iter().any(|x| x == w) {
            self.warnings.push(w.to_string());
        }
    }

    fn next_aux(&mut self) -> u32 {
        self.aux += 1;
        self.aux
    }

    fn guid_of(&self, id: NodeId) -> Option<Kv<'static>> {
        self.ids.get(&id).map(|l| guid(LAYERS, *l))
    }

    /// The message: the document, its pages and their layers, and the blobs.
    fn message(&mut self) -> Kv<'static> {
        let doc = self.doc;
        self.assign();
        let profile = if doc.color_profile == figcraft_doc::color::ColorProfile::DisplayP3 { "DISPLAY_P3" } else { "SRGB" };
        let mut root: Fields = vec![("guid", guid(0, 0)), ("phase", e("CREATED")), ("type", e("DOCUMENT")), ("name", s(&doc.name))];
        root.extend(root_fields());
        root.push(("documentColorProfile", e(profile)));
        self.nodes.push(o(root));
        for (i, p) in doc.pages.iter().enumerate() {
            self.emit(*p, guid(0, 0), i, None, 0);
        }
        if !doc.styles.is_empty() {
            self.warn("shared styles were left out (their layers keep the values)");
        }
        if !doc.variables.collections.is_empty() || !doc.variables.variables.is_empty() {
            self.warn("variables were left out");
        }
        if !doc.comments.is_empty() {
            self.warn("comments were left out");
        }
        let mut blobs: Vec<(u32, Vec<u8>)> = std::mem::take(&mut self.blobs).into_iter().map(|(bytes, i)| (i, bytes)).collect();
        blobs.sort_by_key(|x| x.0);
        o(vec![
            ("type", e("NODE_CHANGES")),
            ("sessionID", u(0)),
            ("nodeChanges", arr(std::mem::take(&mut self.nodes))),
            ("blobs", arr(blobs.into_iter().map(|(_, bytes)| o(vec![("bytes", Kv::Bytes(bytes))])).collect())),
        ])
    }

    /// GUIDs for every layer (pages first, then depth-first), component property definitions,
    /// flow starting points and overlay settings, before anything refers to them.
    fn assign(&mut self) {
        let doc = self.doc;
        let mut stack: Vec<NodeId> = doc.pages.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let Some(n) = doc.node(id) else { continue };
            if self.ids.len() >= MAX_NODES {
                self.warn("the document has more layers than a .fig file is read with; the rest were left out");
                break;
            }
            let local = u32::try_from(self.ids.len() + 1).unwrap_or(u32::MAX);
            self.ids.insert(id, local);
            // An instance's layers aren't written (its main says them), so nothing may refer to them.
            let mirrored = matches!(&n.kind, NodeKind::Instance { component, .. } if doc.node(*component).is_some_and(|c| matches!(c.kind, NodeKind::Component { .. })));
            if !mirrored {
                stack.extend(n.children.iter().rev());
            }
            match &n.kind {
                NodeKind::Component { props, .. } => {
                    for (name, d) in props {
                        if let Some(ty) = prop_type(d) {
                            let key = (name.clone(), ty);
                            if !self.defs.contains_key(&key) {
                                let id = self.next_aux();
                                self.defs.insert(key, id);
                            }
                        }
                    }
                }
                NodeKind::Page { flows, .. } => {
                    for (i, f) in flows.iter().enumerate() {
                        self.starts.entry(f.start).or_insert((i, f.name.clone()));
                    }
                }
                _ => {}
            }
            for a in n.interactions.iter().flat_map(|it| figcraft_doc::proto::walk(&it.actions)) {
                if let Action::OpenOverlay { dest, position, close_on_outside, background } = a {
                    let o = Overlay { position: *position, close_on_outside: *close_on_outside, background: *background };
                    match self.overlays.get(dest) {
                        Some(old) if (old.position, old.close_on_outside, old.background) != (o.position, o.close_on_outside, o.background) => {
                            self.warn("overlays opened with different settings keep the first one's");
                        }
                        Some(_) => {}
                        None => {
                            self.overlays.insert(*dest, o);
                        }
                    }
                }
            }
        }
    }

    /// Write layer `id` (child `index` of the layer with GUID `parent_guid`) and its subtree.
    fn emit(&mut self, id: NodeId, parent_guid: Kv<'static>, index: usize, parent: Option<&Node>, depth: usize) {
        let doc = self.doc;
        let (Some(&local), Some(n)) = (self.ids.get(&id), doc.node(id)) else { return };
        if depth > MAX_DEPTH {
            self.warn("layers nested deeper than 256 levels were left out");
            return;
        }
        let mut f: Fields = vec![
            ("guid", guid(LAYERS, local)),
            ("phase", e("CREATED")),
            ("parentIndex", o(vec![("guid", parent_guid), ("position", s(&position(index)))])),
        ];
        let children = if let NodeKind::Page { background, guides, prototype, .. } = &n.kind {
            f.extend([("type", e("CANVAS")), ("name", s(&n.name))]);
            f.extend(root_fields());
            f.extend([("backgroundColor", color(*background)), ("backgroundOpacity", fl(background.a.into())), ("backgroundEnabled", b(true))]);
            if !guides.is_empty() {
                self.warn("ruler guides were left out");
            }
            if *prototype != Default::default() {
                self.warn("prototype device settings were left out");
            }
            true
        } else {
            self.layer(n, parent, &mut f, false)
        };
        self.nodes.push(o(f));
        if children {
            for (i, c) in n.children.iter().enumerate() {
                self.emit(*c, guid(LAYERS, local), i, Some(n), depth + 1);
            }
        }
    }

    /// The fields of layer `n` (whose parent is `parent`). `nested`: for an instance's override
    /// (no type, no instance or component data). Returns whether its children are written as
    /// layers (an instance's come from its main).
    fn layer(&mut self, n: &Node, parent: Option<&Node>, f: &mut Fields, nested: bool) -> bool {
        let doc = self.doc;
        let main = match &n.kind {
            NodeKind::Instance { component, .. } => {
                let found = self.ids.contains_key(component) && doc.node(*component).is_some_and(|c| matches!(c.kind, NodeKind::Component { .. }));
                if !found && !nested {
                    self.warn("instances of components that aren't in the file were written as frames");
                }
                found.then_some(*component)
            }
            _ => None,
        };
        let ty = match &n.kind {
            NodeKind::Page { .. } | NodeKind::Frame | NodeKind::Group | NodeKind::ComponentSet { .. } => "FRAME",
            NodeKind::Instance { .. } if main.is_none() => "FRAME",
            NodeKind::Section { .. } => "SECTION",
            NodeKind::Rectangle => "ROUNDED_RECTANGLE",
            NodeKind::Ellipse { .. } => "ELLIPSE",
            NodeKind::Polygon { .. } => "REGULAR_POLYGON",
            NodeKind::Star { .. } => "STAR",
            NodeKind::Line => "LINE",
            NodeKind::Vector { .. } => "VECTOR",
            NodeKind::Text(_) => "TEXT",
            NodeKind::BooleanOp { .. } => "BOOLEAN_OPERATION",
            NodeKind::Component { .. } => "SYMBOL",
            NodeKind::Instance { .. } => "INSTANCE",
            NodeKind::Slice => "SLICE",
        };
        if !nested {
            f.push(("type", e(ty)));
        }
        f.push(("name", s(&n.name)));
        self.common(n, parent, f);
        match &n.kind {
            NodeKind::Group => f.push(("resizeToFit", b(true))),
            NodeKind::Section { contents_hidden } => f.push(("sectionContentsHidden", b(*contents_hidden))),
            NodeKind::Ellipse { start, sweep, inner } => {
                f.push(("arcData", o(vec![("startingAngle", fl(*start)), ("endingAngle", fl(start + sweep)), ("innerRadius", fl(*inner))])))
            }
            NodeKind::Polygon { count } => f.push(("count", u(*count))),
            NodeKind::Star { count, inner_ratio } => f.extend([("count", u(*count)), ("starInnerScale", fl(*inner_ratio))]),
            NodeKind::Vector { network } => self.vector(n, network, f),
            NodeKind::Text(td) => self.text(td, &n.fills, f),
            NodeKind::BooleanOp { op } => {
                let name = match op {
                    BoolOp::Union => "UNION",
                    BoolOp::Subtract => "SUBTRACT",
                    BoolOp::Intersect => "INTERSECT",
                    BoolOp::Exclude => "XOR",
                };
                f.push(("booleanOperation", e(name)));
                if n.children.iter().filter_map(|c| doc.node(*c)).any(|c| c.bool_op.is_some()) {
                    self.warn("boolean groups whose layers combine in different ways use the group's operation");
                }
            }
            NodeKind::Component { description, props } if !nested => {
                let defs: Vec<Kv> = props.iter().filter_map(|(name, d)| self.prop_def(name, d)).collect();
                if !defs.is_empty() {
                    f.push(("componentPropDefs", arr(defs)));
                }
                if props.values().any(|d| matches!(d, PropDef::Slot { .. })) {
                    self.warn("slot properties were left out");
                }
                if !description.is_empty() {
                    f.push(("symbolDescription", s(description)));
                }
            }
            NodeKind::ComponentSet { description } if !nested => {
                f.push(("isStateGroup", b(true)));
                if !description.is_empty() {
                    f.push(("symbolDescription", s(description)));
                }
            }
            NodeKind::Instance { props, .. } if !nested => {
                if let Some(main) = main {
                    self.instance(n, main, props, f);
                }
            }
            _ => {}
        }
        let boxy = n.kind.is_frame_like() || matches!(n.kind, NodeKind::Rectangle | NodeKind::Section { .. });
        if (boxy || matches!(n.kind, NodeKind::Ellipse { .. } | NodeKind::Polygon { .. } | NodeKind::Star { .. }))
            && let Some(p) = figcraft_doc::geometry::shape_path(n)
        {
            self.geometry("fillGeometry", &p, WindingRule::NonZero, f);
        }
        self.stroke_geometry(n, boxy, f);
        main.is_none()
    }

    /// Fields every layer kind shares.
    fn common(&mut self, n: &Node, parent: Option<&Node>, f: &mut Fields) {
        f.extend([
            ("visible", b(n.visible)),
            ("locked", b(n.locked)),
            ("opacity", fl(n.opacity.into())),
            ("blendMode", e(blend_name(n.blend))),
            ("size", xy(n.width, n.height)),
            ("transform", matrix(n.transform)),
        ]);
        if n.is_mask {
            let ty = match n.mask_type {
                MaskType::Alpha => "ALPHA",
                MaskType::Vector => "OUTLINE",
                MaskType::Luminance => "LUMINANCE",
            };
            f.extend([("mask", b(true)), ("maskType", e(ty))]);
        }
        // Text whose own style sets fills paints with them (the layer's fills only show through runs).
        let fills = match &n.kind {
            NodeKind::Text(td) => td.style.fills.as_deref().unwrap_or(&n.fills),
            _ => &n.fills,
        };
        let fills = self.paints(fills);
        let strokes = self.paints(&n.strokes);
        f.extend([("fillPaints", fills), ("strokePaints", strokes)]);
        self.stroke(n, f);
        if !n.effects.is_empty() {
            let effects = n.effects.iter().map(|x| self.effect(x)).collect();
            f.push(("effects", arr(effects)));
        }
        if n.kind.has_corner_radius() {
            let r = n.corners.radii;
            f.push(("cornerRadius", fl(r[0])));
            let differ = r.iter().any(|x| *x != r[0]);
            if n.kind.has_independent_corners() && (differ || r[0] > 0.0) {
                f.extend(differ.then(|| ("rectangleCornerRadiiIndependent", b(true))));
                f.extend([
                    ("rectangleTopLeftCornerRadius", fl(r[0])),
                    ("rectangleTopRightCornerRadius", fl(r[1])),
                    ("rectangleBottomRightCornerRadius", fl(r[2])),
                    ("rectangleBottomLeftCornerRadius", fl(r[3])),
                ]);
            }
            if n.corners.smoothing > 0.0 {
                f.push(("cornerSmoothing", fl(n.corners.smoothing)));
            }
        }
        let h = match n.constraints.horizontal {
            HConstraint::Left => "MIN",
            HConstraint::Right => "MAX",
            HConstraint::LeftRight => "STRETCH",
            HConstraint::Center => "CENTER",
            HConstraint::Scale => "SCALE",
        };
        let v = match n.constraints.vertical {
            VConstraint::Top => "MIN",
            VConstraint::Bottom => "MAX",
            VConstraint::TopBottom => "STRETCH",
            VConstraint::Center => "CENTER",
            VConstraint::Scale => "SCALE",
        };
        f.extend([("horizontalConstraint", e(h)), ("verticalConstraint", e(v))]);
        if n.kind.is_frame_like() || matches!(n.kind, NodeKind::Section { .. }) {
            f.push(("frameMaskDisabled", b(!n.clip)));
        }
        if n.kind.is_frame_like() {
            if let Some(l) = &n.layout {
                self.auto_layout(n, l, f);
            }
            if !n.grids.is_empty() {
                f.push(("layoutGrids", arr(n.grids.iter().map(layout_grid).collect())));
            }
        }
        self.child_layout(n, parent, f);
        if !n.exports.is_empty() {
            if n.exports.iter().any(|x| x.format == figcraft_doc::ExportFormat::Webp) {
                self.warn("WebP export settings were written as PNG");
            }
            f.push(("exportSettings", arr(n.exports.iter().map(export_setting).collect())));
        }
        self.prototype(n, f);
        let refs: Vec<Kv> = n.prop_refs.iter().filter_map(|(field, prop)| self.prop_ref(field, prop)).collect();
        if !refs.is_empty() {
            f.push(("parameterConsumptionMap", o(vec![("entries", arr(refs))])));
        }
        if !n.styles.is_empty() {
            self.warn("shared styles were left out (their layers keep the values)");
        }
        if !n.bound.is_empty() || !n.var_modes.is_empty() {
            self.warn("variables were left out");
        }
        if !n.guides.is_empty() {
            self.warn("ruler guides were left out");
        }
        if !n.annotations.is_empty() || !n.dev_resources.is_empty() || !n.code_links.is_empty() || !n.dev_status.is_none() {
            self.warn("Dev Mode annotations, statuses, resources and code links were left out");
        }
        if !n.plugin_data.is_empty() {
            self.warn("plugin data was left out");
        }
    }

    fn paints(&mut self, list: &[Paint]) -> Kv<'static> {
        arr(list.iter().filter_map(|p| self.paint(p)).collect())
    }

    fn paint(&mut self, p: &Paint) -> Option<Kv<'static>> {
        let mut f: Fields = vec![("visible", b(p.visible)), ("blendMode", e(blend_name(p.blend)))];
        let mut opacity = f64::from(p.opacity);
        match &p.kind {
            PaintKind::Solid { color: c } => {
                opacity *= f64::from(c.a);
                f.extend([("type", e("SOLID")), ("color", color(c.with_alpha(1.0)))]);
            }
            PaintKind::Gradient(g) => {
                let ty = match g.kind {
                    GradientKind::Linear => "GRADIENT_LINEAR",
                    GradientKind::Radial => "GRADIENT_RADIAL",
                    GradientKind::Angular => "GRADIENT_ANGULAR",
                    GradientKind::Diamond => "GRADIENT_DIAMOND",
                };
                // The handles in the unit box come from the inverse of the paint transform: it maps
                // (0, ½) → start, (1, ½) → end, (0, 1) → width for linear gradients, and the centre
                // (½, ½) and the points half a box away from it for the others.
                let (from, to, width) = (g.from, g.to, g.width);
                let (ax, ay) =
                    if g.kind == GradientKind::Linear { (to.0 - from.0, to.1 - from.1) } else { (2.0 * (to.0 - from.0), 2.0 * (to.1 - from.1)) };
                let (cx, cy) = (2.0 * (width.0 - from.0), 2.0 * (width.1 - from.1));
                let (ex, ey) = if g.kind == GradientKind::Linear {
                    (from.0 - 0.5 * cx, from.1 - 0.5 * cy)
                } else {
                    (from.0 - 0.5 * (ax + cx), from.1 - 0.5 * (ay + cy))
                };
                let handles = Affine::new([ax, ay, cx, cy, ex, ey]);
                let t = if handles.determinant().abs() > 1e-12 && handles.as_coeffs().iter().all(|x| x.is_finite()) {
                    handles.inverse()
                } else {
                    self.warn("gradients with handles on one line were written with the default direction");
                    Affine::IDENTITY
                };
                let stops = g.stops.iter().map(|st| o(vec![("color", color(st.color)), ("position", fl(st.position.into()))])).collect();
                f.extend([("type", e(ty)), ("stops", arr(stops)), ("transform", matrix(t))]);
            }
            PaintKind::Image(img) => {
                let hash = self.image(&img.image);
                let mode = match img.scale_mode {
                    ImageScaleMode::Fill => "FILL",
                    ImageScaleMode::Fit => "FIT",
                    ImageScaleMode::Crop => "STRETCH",
                    ImageScaleMode::Tile => "TILE",
                };
                f.extend([
                    ("type", e("IMAGE")),
                    ("image", o(vec![("hash", Kv::Bytes(hash)), ("name", s(""))])),
                    ("imageScaleMode", e(mode)),
                    ("rotation", fl(f64::from(img.rotation.rem_euclid(4)) * 90.0)),
                    ("scale", fl(img.scale)),
                ]);
                if let (ImageScaleMode::Crop, Some(c)) = (img.scale_mode, img.crop) {
                    f.push(("transform", matrix(Affine::new(c))));
                }
                if img.filters != Default::default() {
                    self.warn("image adjustments (exposure, contrast, …) were left out");
                }
            }
            PaintKind::Pattern(_) => {
                self.warn("pattern fills were left out");
                return None;
            }
        }
        f.push(("opacity", fl(opacity)));
        Some(o(f))
    }

    /// The hash bytes of image `key`, writing its file once (empty when its bytes aren't there).
    fn image(&mut self, key: &str) -> Vec<u8> {
        let hex = match self.images.get(key) {
            Some(h) => h.clone(),
            None => {
                let h = match self.doc.images.get(key) {
                    Some(img) if img.bytes.len() as u64 <= MAX_IMAGE => {
                        let h = crate::image_hash(&img.bytes);
                        if !self.files.iter().any(|(x, _)| *x == h) {
                            self.files.push((h.clone(), img.bytes.clone()));
                        }
                        Some(h)
                    }
                    Some(_) => {
                        self.warn("images larger than 256 MB were left out");
                        None
                    }
                    None => {
                        self.warn("image fills whose image isn't in the document were written without one");
                        None
                    }
                };
                self.images.insert(key.to_string(), h.clone());
                h
            }
        };
        let hex = hex.unwrap_or_default();
        (0..hex.len() / 2).filter_map(|i| hex.get(2 * i..2 * i + 2).and_then(|d| u8::from_str_radix(d, 16).ok())).collect()
    }

    fn stroke(&mut self, n: &Node, f: &mut Fields) {
        let sp = &n.stroke;
        let align = match sp.align {
            StrokeAlign::Inside => "INSIDE",
            StrokeAlign::Center => "CENTER",
            StrokeAlign::Outside => "OUTSIDE",
        };
        f.extend([
            ("strokeWeight", fl(sp.weight)),
            ("strokeAlign", e(align)),
            ("strokeJoin", e(join_name(sp.join))),
            ("strokeCap", e(cap_name(sp.cap_start))),
            ("miterLimit", fl(sp.miter_limit)),
        ]);
        if !sp.dashes.is_empty() {
            f.push(("dashPattern", arr(sp.dashes.iter().map(|d| fl(*d)).collect())));
        }
        if let Some([t, r, bo, l]) = sp.sides {
            f.extend([
                ("borderStrokeWeightsIndependent", b(true)),
                ("borderTopWeight", fl(t)),
                ("borderRightWeight", fl(r)),
                ("borderBottomWeight", fl(bo)),
                ("borderLeftWeight", fl(l)),
            ]);
        }
        if sp.width_profile.is_some() || sp.brush.is_some() {
            self.warn("variable-width and brush strokes were written as plain strokes");
        }
        if [sp.cap_start, sp.cap_end].contains(&StrokeCap::ReversedTriangle) {
            self.warn("reversed triangle stroke ends were written as triangle arrows");
        }
        // Vectors keep each end's cap on their end points; other layers have one cap.
        if sp.cap_start != sp.cap_end && !matches!(n.kind, NodeKind::Vector { .. }) {
            self.warn("lines with different stroke ends use the start's at both ends");
        }
    }

    fn effect(&mut self, x: &figcraft_doc::Effect) -> Kv<'static> {
        let mut f: Fields = vec![("visible", b(x.visible))];
        let blur = |f: &mut Fields, ty: &'static str, bl: &figcraft_doc::Blur| {
            f.extend([("type", e(ty)), ("radius", fl(bl.radius))]);
            if bl.blur_type == figcraft_doc::BlurType::Progressive {
                f.extend([
                    ("blurOpType", e("PROGRESSIVE")),
                    ("startRadius", fl(bl.start_radius)),
                    ("startOffset", xy(bl.start.0, bl.start.1)),
                    ("endOffset", xy(bl.end.0, bl.end.1)),
                ]);
            }
        };
        let grain = |size: f64, size_y: Option<f64>| xy(size, size_y.unwrap_or(size));
        match &x.kind {
            EffectKind::DropShadow(sh) | EffectKind::InnerShadow(sh) => {
                let ty = if matches!(x.kind, EffectKind::DropShadow(_)) { "DROP_SHADOW" } else { "INNER_SHADOW" };
                f.extend([
                    ("type", e(ty)),
                    ("color", color(sh.color)),
                    ("offset", xy(sh.offset.0, sh.offset.1)),
                    ("radius", fl(sh.radius)),
                    ("spread", fl(sh.spread)),
                    ("blendMode", e(blend_name(sh.blend))),
                    ("showShadowBehindNode", b(sh.show_behind)),
                ]);
            }
            EffectKind::LayerBlur(bl) => blur(&mut f, "FOREGROUND_BLUR", bl),
            EffectKind::BackgroundBlur(bl) => blur(&mut f, "BACKGROUND_BLUR", bl),
            EffectKind::Noise(nz) => {
                let mode = match nz.mode {
                    NoiseMode::Mono => "MONOTONE",
                    NoiseMode::Duo => "DUOTONE",
                    NoiseMode::Multi => "MULTITONE",
                };
                f.extend([
                    ("type", e("NOISE")),
                    ("noiseType", e(mode)),
                    ("noiseSize", grain(nz.size, nz.size_y)),
                    ("density", fl(nz.density)),
                    ("color", color(nz.color)),
                    ("secondaryColor", color(nz.secondary)),
                    ("opacity", fl(nz.opacity.into())),
                    ("blendMode", e(blend_name(nz.blend))),
                ]);
            }
            EffectKind::Texture(t) => {
                f.extend([
                    ("type", e("GRAIN")),
                    ("noiseSize", grain(t.size, t.size_y)),
                    ("radius", fl(t.radius)),
                    ("clipToShape", b(t.clip_to_shape)),
                ]);
            }
            EffectKind::Glass(g) => {
                self.warn("glass effects keep only their frost");
                f.extend([("type", e("GLASS")), ("radius", fl(g.frost))]);
            }
        }
        o(f)
    }

    fn auto_layout(&mut self, n: &Node, l: &AutoLayout, f: &mut Fields) {
        let grid = l.mode == LayoutMode::Grid;
        let mode = match l.mode {
            LayoutMode::Horizontal => "HORIZONTAL",
            LayoutMode::Vertical => "VERTICAL",
            LayoutMode::Grid => "GRID",
        };
        let [top, right, bottom, left] = l.padding;
        let main = match l.main_align {
            MainAlign::Min => "MIN",
            MainAlign::Center => "CENTER",
            MainAlign::Max => "MAX",
            MainAlign::SpaceBetween => "SPACE_BETWEEN",
            MainAlign::SpaceAround => "SPACE_AROUND",
            MainAlign::SpaceEvenly => "SPACE_EVENLY",
        };
        let cross = match l.cross_align {
            CrossAlign::Min => "MIN",
            CrossAlign::Center => "CENTER",
            CrossAlign::Max => "MAX",
            CrossAlign::Baseline => "BASELINE",
        };
        // The main axis is the width for horizontal and grid layouts.
        let (main_hug, cross_hug) = {
            let (h, v) = (n.child_layout.sizing_h == Sizing::Hug, n.child_layout.sizing_v == Sizing::Hug);
            if l.mode == LayoutMode::Vertical { (v, h) } else { (h, v) }
        };
        let size = |hug: bool| e(if hug { "RESIZE_TO_FIT" } else { "FIXED" });
        f.extend([
            ("stackMode", e(mode)),
            ("stackWrap", e(if l.wrap { "WRAP" } else { "NO_WRAP" })),
            ("stackHorizontalPadding", fl(left)),
            ("stackVerticalPadding", fl(top)),
            ("stackPaddingRight", fl(right)),
            ("stackPaddingBottom", fl(bottom)),
            ("stackPrimaryAlignItems", e(main)),
            ("stackCounterAlignItems", e(cross)),
            ("stackPrimarySizing", size(main_hug)),
            ("stackCounterSizing", size(cross_hug)),
            ("bordersTakeSpace", b(l.strokes_included)),
            ("stackReverseZIndex", b(l.reverse_z)),
        ]);
        if grid {
            // Instance layers share their main layer's track ids, so unchanged tracks and the cells
            // their layers sit in still match.
            let key = self.main_layer(n.id);
            let (want_c, want_r) = (l.grid_columns.min(1000) as usize, l.grid_rows.min(1000) as usize);
            let (cols, rows) = match self.tracks.get(&key) {
                Some((c, r)) if (c.len(), r.len()) == (want_c, want_r) => (c.clone(), r.clone()),
                _ => ((0..want_c).map(|_| self.next_aux()).collect(), (0..want_r).map(|_| self.next_aux()).collect()),
            };
            let map = |ids: &[u32]| {
                o(vec![(
                    "entries",
                    arr(ids.iter().enumerate().map(|(i, t)| o(vec![("id", guid(AUX, *t)), ("position", s(&position(i)))])).collect()),
                )])
            };
            f.extend([("gridColumnGap", fl(l.gap)), ("gridRowGap", fl(l.cross_gap)), ("gridColumns", map(&cols)), ("gridRows", map(&rows))]);
            if l.column_tracks.iter().chain(&l.row_tracks).any(|t| t.sizing != Sizing::Fill) {
                self.warn("grid tracks of fixed or content size were written as equal tracks");
            }
            self.tracks.insert(key, (cols, rows));
        } else {
            f.extend([("stackSpacing", fl(l.gap)), ("stackCounterSpacing", fl(l.cross_gap))]);
        }
    }

    /// How a layer sizes and sits in its auto layout parent.
    fn child_layout(&mut self, n: &Node, parent: Option<&Node>, f: &mut Fields) {
        let cl = &n.child_layout;
        let Some(pl) = parent.and_then(|p| p.layout.as_ref()) else { return };
        let (fill_h, fill_v) = (cl.sizing_h == Sizing::Fill, cl.sizing_v == Sizing::Fill);
        let (grow, stretch) = if pl.mode == LayoutMode::Vertical { (fill_v, fill_h) } else { (fill_h, fill_v) };
        if grow {
            f.push(("stackChildPrimaryGrow", fl(1.0)));
        }
        if stretch {
            f.push(("stackChildAlignSelf", e("STRETCH")));
        }
        if cl.absolute {
            f.push(("stackPositioning", e("ABSOLUTE")));
        }
        for (key, w, h) in [("minSize", cl.min_w, cl.min_h), ("maxSize", cl.max_w, cl.max_h)] {
            if w.is_some() || h.is_some() {
                f.push((key, o(vec![("value", xy(w.unwrap_or(0.0), h.unwrap_or(0.0)))])));
            }
        }
        if pl.mode == LayoutMode::Grid
            && let Some((cols, rows)) = parent.and_then(|p| self.tracks.get(&self.main_layer(p.id)))
        {
            let anchor = |ids: &[u32], i: Option<u32>| i.and_then(|i| ids.get(i as usize)).map(|t| guid(AUX, *t));
            let (c, r) = (anchor(cols, cl.grid_column), anchor(rows, cl.grid_row));
            f.extend(c.map(|g| ("gridColumnAnchor", g)));
            f.extend(r.map(|g| ("gridRowAnchor", g)));
            if cl.grid_column_span > 0 {
                f.push(("gridColumnSpan", int(cl.grid_column_span.into())));
            }
            if cl.grid_row_span > 0 {
                f.push(("gridRowSpan", int(cl.grid_row_span.into())));
            }
        }
    }

    /// Text: its style, box settings, and characters with their styled runs. `fills`: the layer's
    /// (what runs without fills of their own show when the text's own style sets other fills).
    fn text(&mut self, td: &TextData, fills: &[Paint], f: &mut Fields) {
        style_fields(&td.style, f);
        let (ha, va) = (
            match td.align_h {
                TextAlignH::Left => "LEFT",
                TextAlignH::Center => "CENTER",
                TextAlignH::Right => "RIGHT",
                TextAlignH::Justified => "JUSTIFIED",
            },
            match td.align_v {
                TextAlignV::Top => "TOP",
                TextAlignV::Middle => "CENTER",
                TextAlignV::Bottom => "BOTTOM",
            },
        );
        let resize = match td.auto_resize {
            TextAutoResize::WidthAndHeight => "WIDTH_AND_HEIGHT",
            TextAutoResize::Height => "HEIGHT",
            TextAutoResize::None | TextAutoResize::Truncate => "NONE",
        };
        f.extend([
            ("textAlignHorizontal", e(ha)),
            ("textAlignVertical", e(va)),
            ("textAutoResize", e(resize)),
            ("paragraphSpacing", fl(td.paragraph_spacing)),
            ("paragraphIndent", fl(td.paragraph_indent)),
            ("autoRename", b(false)),
            ("fontVariantCommonLigatures", b(true)),
            ("fontVariantContextualLigatures", b(true)),
            ("fontVersion", s("")),
            ("textUserLayoutVersion", u(5)),
            ("textExplicitLayoutVersion", u(1)),
            ("textBidiVersion", u(1)),
            ("textTracking", fl(0.0)),
            ("textDecorationSkipInk", b(true)),
            ("emojiImageSet", e("APPLE")),
        ]);
        if td.auto_resize == TextAutoResize::Truncate {
            f.push(("textTruncation", e("ENDING")));
        }
        if td.max_lines > 0 {
            f.push(("maxLines", int(td.max_lines.into())));
        }
        // Styles per UTF-16 unit: 0 for the base style, else an entry of the override table.
        if td.char_count() > MAX_CHARS {
            self.warn("text longer than 1,048,576 characters was cut");
        }
        let spans = td.spans();
        let mut table: Vec<&TextStyle> = Vec::new();
        let mut span_ids = Vec::with_capacity(spans.len());
        for (_, _, st) in &spans {
            span_ids.push(if **st == td.style {
                0
            } else {
                let i = table.iter().position(|t| *t == *st).unwrap_or_else(|| {
                    table.push(st);
                    table.len() - 1
                });
                i as u32 + 1
            });
        }
        let mut chars = String::new();
        let mut ids: Vec<Kv> = Vec::new();
        let mut span = 0;
        for (i, ch) in td.characters.chars().take(MAX_CHARS).enumerate() {
            while spans.get(span).is_some_and(|sp| i >= sp.1) {
                span += 1;
            }
            // NULs can't be stored in a string.
            if ch == '\0' {
                continue;
            }
            chars.push(ch);
            let id = span_ids.get(span).copied().unwrap_or(0);
            ids.extend((0..ch.len_utf16()).map(|_| u(id)));
        }
        let mut data: Fields = vec![("characters", Kv::Str(chars))];
        if !table.is_empty() {
            let entries = table
                .iter()
                .zip(1u32..)
                .map(|(st, id)| {
                    let mut ef: Fields = vec![("styleID", u(id))];
                    style_fields(st, &mut ef);
                    if let Some(p) = st.fills.as_deref().or(td.style.fills.as_ref().map(|_| fills)) {
                        ef.push(("fillPaints", self.paints(p)));
                    }
                    o(ef)
                })
                .collect();
            data.extend([("characterStyleIDs", arr(ids)), ("styleOverrideTable", arr(entries))]);
        }
        let lists: Vec<ListStyle> = (0..td.paragraph_count()).map(|p| td.para_list(p).list).collect();
        // One entry per paragraph, each with every field a reader expects.
        let line = |(i, l): (usize, &ListStyle)| {
            let ty = match l {
                ListStyle::None => "PLAIN",
                ListStyle::Numbered => "ORDERED_LIST",
                ListStyle::Bulleted => "UNORDERED_LIST",
            };
            let listed = *l != ListStyle::None;
            let first = listed && (i == 0 || lists.get(i - 1) != Some(l));
            o(vec![
                ("lineType", e(ty)),
                ("styleId", int(0)),
                ("indentationLevel", int(i64::from(listed))),
                ("sourceDirectionality", e("AUTO")),
                ("listStartOffset", int(0)),
                ("isFirstLineOfList", b(first)),
            ])
        };
        data.push(("lines", arr(lists.iter().enumerate().map(line).collect())));
        f.push(("textData", o(data)));
        if (0..td.paragraph_count()).any(|p| td.para_list(p).indent > 0) {
            self.warn("nested list levels were written as the first level");
        }
        let styles = std::iter::once(&td.style).chain(td.runs.iter().map(|r| &r.style));
        if styles.clone().any(|st| st.decoration_look.is_some() || !st.variations.is_empty()) {
            self.warn("text decoration looks and variable font settings were left out");
        }
        if styles.clone().any(|st| st.style_id.is_some()) {
            self.warn("shared styles were left out (their layers keep the values)");
        }
        if td.path.is_some() {
            self.warn("text on a path was written as plain text");
        }
        if td.list_spacing != 0.0 || td.hanging_punctuation || td.hanging_list || td.leading_trim != Default::default() || !td.features.is_empty() {
            self.warn("list spacing, hanging punctuation, leading trim and OpenType features were left out");
        }
    }

    /// The editable network (with per-point corner radii, handle mirroring and end caps) and,
    /// when it has filled areas, their outline.
    fn vector(&mut self, n: &Node, net: &VectorNetwork, f: &mut Fields) {
        let entries: usize = net.regions.iter().flat_map(|r| &r.loops).map(Vec::len).sum();
        if net.vertices.len() + net.segments.len() + entries > MAX_NETWORK {
            self.warn("vector networks too large for the format were written as outlines");
        } else {
            // The stroke starts at the first end point and stops at the last: the layer's end caps go
            // there (where the point has none of its own).
            let mut degree = vec![0usize; net.vertices.len()];
            for sg in &net.segments {
                for i in [sg.a, sg.b] {
                    if let Some(d) = degree.get_mut(i) {
                        *d += 1;
                    }
                }
            }
            let ends: Vec<usize> = (0..degree.len()).filter(|i| degree.get(*i) == Some(&1)).collect();
            let (first, last) = (ends.first().copied(), ends.last().copied());
            let caps_differ = n.stroke.cap_start != n.stroke.cap_end || net.vertices.iter().any(|v| v.cap.is_some());
            let mut styles: Vec<Fields> = Vec::new();
            let mut style_of = |fields: Fields| -> u32 {
                if fields.is_empty() {
                    return 0;
                }
                let i = styles.iter().position(|x| *x == fields).unwrap_or_else(|| {
                    styles.push(fields);
                    styles.len() - 1
                });
                i as u32 + 1
            };
            let vertices = net
                .vertices
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let mut st: Fields = Vec::new();
                    if v.radius > 0.0 {
                        st.push(("cornerRadius", fl(v.radius)));
                    }
                    if let Some(m) = v.mirror {
                        st.push((
                            "handleMirroring",
                            e(match m {
                                figcraft_doc::HandleMirroring::None => "NONE",
                                figcraft_doc::HandleMirroring::Angle => "ANGLE",
                                figcraft_doc::HandleMirroring::AngleAndLength => "ANGLE_AND_LENGTH",
                            }),
                        ));
                    }
                    let end_cap = if Some(i) == first {
                        Some(n.stroke.cap_start)
                    } else if Some(i) == last {
                        Some(n.stroke.cap_end)
                    } else {
                        None
                    };
                    if let Some(c) = v.cap.or(end_cap.filter(|_| caps_differ)) {
                        st.push(("strokeCap", e(cap_name(c))));
                    }
                    if let Some(j) = v.join {
                        st.push(("strokeJoin", e(join_name(j))));
                    }
                    network::Vertex { style: style_of(st), x: finite(v.x), y: finite(v.y) }
                })
                .collect();
            let segments = net
                .segments
                .iter()
                .map(|sg| network::Segment { a: sg.a, ta: (finite(sg.ta.0), finite(sg.ta.1)), b: sg.b, tb: (finite(sg.tb.0), finite(sg.tb.1)) })
                .collect();
            let regions = net
                .regions
                .iter()
                .map(|r| network::Region { style: 0, non_zero: r.winding == WindingRule::NonZero, loops: r.loops.clone() })
                .collect();
            let blob = network::encode(&network::Network { vertices, segments, regions });
            let table = styles.into_iter().zip(1u32..).map(|(mut st, id)| {
                st.insert(0, ("styleID", u(id)));
                o(st)
            });
            let table: Vec<Kv> = table.collect();
            if let Some(i) = self.blob(blob) {
                f.push((
                    "vectorData",
                    o(vec![("vectorNetworkBlob", u(i)), ("normalizedSize", xy(n.width, n.height)), ("styleOverrideTable", arr(table))]),
                ));
            }
        }
        if !net.regions.is_empty() {
            let winding = net.regions.first().map(|r| r.winding).unwrap_or_default();
            if net.regions.iter().any(|r| r.winding != winding) {
                self.warn("vectors whose filled areas use different winding rules use the first one's in their outline");
            }
            self.geometry("fillGeometry", &net.fill_path(), winding, f);
        }
    }

    /// `fillGeometry` or `strokeGeometry` (`key`) from a local path.
    fn geometry(&mut self, key: &'static str, p: &BezPath, winding: WindingRule, f: &mut Fields) {
        if p.elements().is_empty() {
            return;
        }
        if let Some(i) = self.blob(path_blob(p)) {
            let rule = if winding == WindingRule::EvenOdd { "ODD" } else { "NONZERO" };
            f.push((key, arr(vec![o(vec![("windingRule", e(rule)), ("commandsBlob", u(i)), ("styleID", u(0))])])));
        }
    }

    /// `strokeGeometry`, the outline of a plain stroke: a ring around boxes and whole ellipses, the
    /// widened path of other centred strokes. Dashed, per-side and other strokes get none (a reader
    /// builds theirs from the stroke settings).
    fn stroke_geometry(&mut self, n: &Node, boxy: bool, f: &mut Fields) {
        let sp = &n.stroke;
        let (w, h, weight) = (n.width, n.height, sp.weight);
        let plain = n.strokes.iter().any(|p| p.visible) && sp.dashes.is_empty() && sp.sides.is_none();
        if !plain || !(weight.is_finite() && weight > 0.0 && w.is_finite() && h.is_finite()) {
            return;
        }
        let whole = matches!(n.kind, NodeKind::Ellipse { sweep, inner, .. } if inner == 0.0 && sweep.abs() >= std::f64::consts::TAU - 1e-9);
        if boxy || whole {
            let (out, inset) = match sp.align {
                StrokeAlign::Inside => (0.0, weight),
                StrokeAlign::Center => (weight / 2.0, weight / 2.0),
                StrokeAlign::Outside => (weight, 0.0),
            };
            // The shape grown by `d` (shrunk when negative), or nothing when it shrinks away.
            let grown = |d: f64| -> Option<BezPath> {
                let (gw, gh) = (w + 2.0 * d, h + 2.0 * d);
                if gw <= 0.0 || gh <= 0.0 {
                    return None;
                }
                let mut p = if whole {
                    figcraft_doc::geom::ellipse(gw, gh, 0.0, std::f64::consts::TAU, 0.0)
                } else {
                    let radii = n.corners.radii.map(|r| if r > 0.0 { (r + d).max(0.0) } else { 0.0 });
                    figcraft_doc::geom::rounded_rect(gw, gh, radii, n.corners.smoothing)
                };
                p.apply_affine(Affine::translate((-d, -d)));
                Some(p)
            };
            let Some(mut ring) = grown(out) else { return };
            ring.extend(grown(-inset).iter().flat_map(|p| p.elements().iter().copied()));
            self.geometry("strokeGeometry", &ring, WindingRule::EvenOdd, f);
        } else if sp.align == StrokeAlign::Center {
            let path = match &n.kind {
                NodeKind::Vector { network } => Some(network.stroke_path()),
                NodeKind::Text(_) => None,
                _ => figcraft_doc::geometry::shape_path(n),
            };
            let Some(path) = path.filter(|p| !p.elements().is_empty()) else { return };
            use figcraft_doc::geom::kurbo::{Cap, Join, Stroke, StrokeOpts, stroke};
            let cap = match sp.cap_start {
                StrokeCap::Round => Cap::Round,
                StrokeCap::Square => Cap::Square,
                _ => Cap::Butt,
            };
            let join = match sp.join {
                StrokeJoin::Miter => Join::Miter,
                StrokeJoin::Bevel => Join::Bevel,
                StrokeJoin::Round => Join::Round,
            };
            let limit = if sp.miter_limit.is_finite() { sp.miter_limit.max(1.0) } else { 4.0 };
            let style = Stroke::new(weight).with_caps(cap).with_join(join).with_miter_limit(limit);
            self.geometry("strokeGeometry", &stroke(path, &style, &StrokeOpts::default(), 0.05), WindingRule::NonZero, f);
        }
    }

    /// Add a blob; its index (`None` once the blobs would make the file too large).
    fn blob(&mut self, bytes: Vec<u8>) -> Option<u32> {
        if let Some(i) = self.blobs.get(&bytes) {
            return Some(*i);
        }
        self.blob_bytes += bytes.len() as u64;
        if self.blob_bytes > MAX_CANVAS {
            self.warn("vector data too large for the format was left out");
            return None;
        }
        let i = u32::try_from(self.blobs.len()).ok()?;
        self.blobs.insert(bytes, i);
        Some(i)
    }

    /// A component property definition (variant properties are implied by the variants' names).
    fn prop_def(&mut self, name: &str, d: &PropDef) -> Option<Kv<'static>> {
        let ty = prop_type(d)?;
        let id = *self.defs.get(&(name.to_string(), ty))?;
        let value = match d {
            PropDef::Boolean { default } => o(vec![("boolValue", b(*default))]),
            PropDef::Text { default } => o(vec![("textValue", o(vec![("characters", s(default))]))]),
            PropDef::InstanceSwap { default, preferred } => {
                if !preferred.is_empty() {
                    self.warn("preferred values of instance swap properties were left out");
                }
                o(vec![("guidValue", self.guid_of(*default)?)])
            }
            PropDef::Variant { .. } | PropDef::Slot { .. } => return None,
        };
        Some(o(vec![("id", guid(AUX, id)), ("name", s(name)), ("initialValue", value), ("type", e(ty))]))
    }

    /// Which property controls a layer's `field`.
    fn prop_ref(&mut self, field: &str, prop: &str) -> Option<Kv<'static>> {
        let (var_field, ty, resolved) = match field {
            "visible" => ("VISIBLE", "BOOL", "BOOLEAN"),
            "characters" => ("TEXT_DATA", "TEXT", "TEXT_DATA"),
            "mainComponent" => ("OVERRIDDEN_SYMBOL_ID", "INSTANCE_SWAP", "SYMBOL_ID"),
            _ => {
                self.warn("slot layers were written as plain layers");
                return None;
            }
        };
        let id = *self.defs.get(&(prop.to_string(), ty))?;
        let data = o(vec![
            ("value", o(vec![("propRefValue", o(vec![("defId", guid(AUX, id))]))])),
            ("dataType", e("PROP_REF")),
            ("resolvedDataType", e(resolved)),
        ]);
        Some(o(vec![("variableData", data), ("variableField", e(var_field))]))
    }

    /// An instance: its main, the fields its layers override, its property values.
    fn instance(&mut self, n: &Node, main: NodeId, props: &BTreeMap<String, Value>, f: &mut Fields) {
        let doc = self.doc;
        let mut overrides = Vec::new();
        let mut stack: Vec<(NodeId, NodeId)> = n.children.iter().rev().map(|c| (*c, n.id)).collect();
        let mut seen = 0usize;
        while let Some((id, parent)) = stack.pop() {
            let Some(d) = doc.node(id) else { continue };
            seen += 1;
            if seen > MAX_NODES {
                break;
            }
            stack.extend(d.children.iter().rev().map(|c| (*c, id)));
            let path = d.source.and_then(|src| self.path_in(src, main, 0));
            let guids: Option<Vec<Kv>> = path.and_then(|p| p.iter().map(|x| self.guid_of(*x)).collect());
            let (Some(guids), Some(src)) = (guids, d.source.and_then(|src| doc.node(src))) else {
                self.warn("instance layers that don't come from their main component were left out");
                continue;
            };
            // The fields that differ from the main's layer: overrides, and layout derived from them.
            let (mut mine, mut theirs) = (Vec::new(), Vec::new());
            self.layer(d, doc.node(parent), &mut mine, true);
            self.layer(src, src.parent.and_then(|p| doc.node(p)), &mut theirs, true);
            let mut of: Fields = vec![("guidPath", o(vec![("guids", arr(guids))]))];
            // Fields the main's layer has and this one doesn't are written at their empty values.
            let cleared: Fields =
                theirs.iter().filter(|(k, _)| !mine.iter().any(|(mk, _)| mk == k)).filter_map(|(k, _)| Some((*k, unset(k)?))).collect();
            of.extend(mine.into_iter().filter(|(k, v)| !theirs.iter().any(|(tk, tv)| tk == k && tv == v)));
            of.extend(cleared);
            // A nested instance swapped for another component.
            if let NodeKind::Instance { component, .. } = &d.kind {
                let was = d.source.and_then(|src| doc.node(src)).and_then(|m| match &m.kind {
                    NodeKind::Instance { component, .. } => Some(*component),
                    _ => None,
                });
                if was != Some(*component)
                    && let Some(g) = self.guid_of(*component)
                {
                    of.push(("overriddenSymbolID", g));
                }
            }
            if of.len() > 1 {
                overrides.push(o(of));
            }
        }
        let Some(symbol) = self.guid_of(main) else { return };
        f.push(("symbolData", o(vec![("symbolID", symbol), ("symbolOverrides", arr(overrides))])));
        let mut assignments = Vec::new();
        for (name, v) in props {
            let (ty, value) = match v {
                Value::Bool(x) => ("BOOL", Some(o(vec![("boolValue", b(*x))]))),
                Value::String(t) => ("TEXT", Some(o(vec![("textValue", o(vec![("characters", s(t))]))]))),
                Value::Number(c) => ("INSTANCE_SWAP", c.as_u64().and_then(|c| self.guid_of(NodeId(c))).map(|g| o(vec![("guidValue", g)]))),
                _ => continue,
            };
            // Variant values have no definition of their own: the main they show says them.
            if let (Some(&id), Some(value)) = (self.defs.get(&(name.clone(), ty)), value) {
                assignments.push(o(vec![("defID", guid(AUX, id)), ("value", value)]));
            }
        }
        if !assignments.is_empty() {
            f.push(("componentPropAssignments", arr(assignments)));
        }
        if n.children.iter().filter_map(|c| doc.node(*c)).any(|c| c.source.is_none()) {
            self.warn("instance layers that don't come from their main component were left out");
        }
    }

    /// The main component layer an instance or instance layer mirrors (through nested instances),
    /// or `id` itself.
    fn main_layer(&self, id: NodeId) -> NodeId {
        let mut cur = id;
        for _ in 0..32 {
            let next = self.doc.node(cur).and_then(|n| match &n.kind {
                _ if n.source.is_some() => n.source,
                NodeKind::Instance { component, .. } => Some(*component),
                _ => None,
            });
            match next.filter(|s| self.doc.contains(*s)) {
                Some(s) => cur = s,
                None => break,
            }
        }
        cur
    }

    /// The id path of layer `s` inside component `comp`: `s` itself, or through the nested
    /// instance it is expanded in, then on inside that instance's main.
    fn path_in(&self, s: NodeId, comp: NodeId, depth: usize) -> Option<Vec<NodeId>> {
        if depth > 32 {
            return None;
        }
        let doc = self.doc;
        let up = doc.ancestors(s);
        let below: Vec<NodeId> = up.iter().take_while(|a| **a != comp).copied().collect();
        if below.len() == up.len() {
            return None;
        }
        // The outermost nested instance between the component and the layer.
        match below.iter().rev().find_map(|a| match doc.node(*a).map(|n| &n.kind) {
            Some(NodeKind::Instance { component, .. }) => Some((*a, *component)),
            _ => None,
        }) {
            None => Some(vec![s]),
            Some((inst, main)) => {
                let mut p = vec![inst];
                p.extend(self.path_in(doc.node(s)?.source?, main, depth + 1)?);
                Some(p)
            }
        }
    }

    /// Interactions, scrolling, flow starts and the overlay settings this layer is opened with.
    fn prototype(&mut self, n: &Node, f: &mut Fields) {
        let list: Vec<Kv> = n.interactions.iter().filter_map(|it| self.interaction(it)).collect();
        if !list.is_empty() {
            f.push(("prototypeInteractions", arr(list)));
        }
        let dir = match n.overflow {
            Overflow::None => None,
            Overflow::Horizontal => Some("HORIZONTAL"),
            Overflow::Vertical => Some("VERTICAL"),
            Overflow::Both => Some("BOTH"),
        };
        f.extend(dir.map(|d| ("scrollDirection", e(d))));
        let behavior = match n.scroll_position {
            ScrollPosition::Scrolls => None,
            ScrollPosition::Fixed => Some("FIXED_WHEN_CHILD_OF_SCROLLING_FRAME"),
            ScrollPosition::Sticky => Some("STICKY_SCROLLS"),
        };
        f.extend(behavior.map(|x| ("scrollBehavior", e(x))));
        if let Some((i, name)) = self.starts.get(&n.id) {
            f.push(("prototypeStartingPoint", o(vec![("name", s(name)), ("position", s(&position(*i)))])));
        }
        if let Some(ov) = self.overlays.get(&n.id) {
            const POSITIONS: [&str; 8] = ["CENTER", "TOP_LEFT", "TOP_CENTER", "TOP_RIGHT", "BOTTOM_LEFT", "BOTTOM_CENTER", "BOTTOM_RIGHT", "MANUAL"];
            let pos = POSITIONS.into_iter().find(|p| crate::rest::proto::overlay_position(p) == Some(ov.position)).unwrap_or("CENTER");
            let bg = match ov.background {
                Some(c) => o(vec![("backgroundType", e("SOLID_COLOR")), ("backgroundColor", color(c))]),
                None => o(vec![("backgroundType", e("NONE"))]),
            };
            f.extend([
                ("overlayPositionType", e(pos)),
                ("overlayBackgroundInteraction", e(if ov.close_on_outside { "CLOSE_ON_CLICK_OUTSIDE" } else { "NONE" })),
                ("overlayBackgroundAppearance", bg),
            ]);
        }
    }

    fn interaction(&mut self, it: &Interaction) -> Option<Kv<'static>> {
        let mut event: Fields = Vec::new();
        let ty = match &it.trigger {
            Trigger::OnClick => "ON_CLICK",
            Trigger::OnHover => "ON_HOVER",
            Trigger::OnPress => "ON_PRESS",
            Trigger::OnDrag => "DRAG",
            Trigger::MouseEnter => "MOUSE_IN",
            Trigger::MouseLeave => "MOUSE_OUT",
            Trigger::MouseDown => "MOUSE_DOWN",
            Trigger::MouseUp => "MOUSE_UP",
            Trigger::AfterDelay { ms } => {
                event.push(("transitionTimeout", fl(f64::from(*ms) / 1000.0)));
                "AFTER_TIMEOUT"
            }
            Trigger::KeyPress { key } => {
                let Some((codes, pad)) = key_codes(key) else {
                    self.warn("key triggers on keys the format has no code for were left out");
                    return None;
                };
                let device = e(if pad { "UNKNOWN_CONTROLLER" } else { "KEYBOARD" });
                event.push(("keyTrigger", o(vec![("keyCodes", arr(codes.into_iter().map(int).collect())), ("triggerDevice", device)])));
                "ON_KEY_DOWN"
            }
        };
        event.insert(0, ("interactionType", e(ty)));
        let actions: Vec<Kv> = it.actions.iter().filter_map(|a| self.action(a, it)).collect();
        (!actions.is_empty()).then(|| o(vec![("event", o(event)), ("actions", arr(actions))]))
    }

    fn action(&mut self, a: &Action, it: &Interaction) -> Option<Kv<'static>> {
        let node = |x: &mut Self, dest: &NodeId, nav: &'static str| -> Option<Kv<'static>> {
            let Some(g) = x.guid_of(*dest) else {
                x.warn("prototype actions whose destination isn't written were left out");
                return None;
            };
            let mut f: Fields = vec![("transitionNodeID", g), ("connectionType", e("INTERNAL_NODE")), ("navigationType", e(nav))];
            x.transition(it, &mut f);
            Some(o(f))
        };
        let plain = |ty: &'static str| Some(o(vec![("connectionType", e(ty))]));
        match a {
            Action::Navigate { dest, preserve_scroll } => {
                let mut act = node(self, dest, "NAVIGATE")?;
                if let Kv::Object(f) = &mut act {
                    f.push(("transitionPreserveScroll", b(*preserve_scroll)));
                }
                Some(act)
            }
            Action::OpenOverlay { dest, .. } => node(self, dest, "OVERLAY"),
            Action::SwapOverlay { dest } => node(self, dest, "SWAP"),
            Action::ChangeTo { dest } => node(self, dest, "SWAP_STATE"),
            Action::ScrollTo { dest, offset } => {
                if *offset != [0.0, 0.0] {
                    self.warn("scroll-to offsets were left out");
                }
                node(self, dest, "SCROLL_TO")
            }
            Action::Back => plain("BACK"),
            Action::CloseOverlay => plain("CLOSE"),
            Action::OpenUrl { url } => Some(o(vec![("connectionType", e("URL")), ("connectionURL", s(url))])),
            Action::SetVariable { .. } | Action::SetVariableMode { .. } | Action::Conditional { .. } => {
                self.warn("variable and conditional prototype actions were left out");
                None
            }
        }
    }

    /// An interaction's transition on one of its actions.
    fn transition(&mut self, it: &Interaction, f: &mut Fields) {
        let side_from = |d: Direction, l: &'static str, r: &'static str, t: &'static str, bo: &'static str| match d {
            // The direction names where the incoming screen moves: it comes from the other side.
            Direction::Right => l,
            Direction::Left => r,
            Direction::Bottom => t,
            Direction::Top => bo,
        };
        let side_to = |d: Direction, l: &'static str, r: &'static str, t: &'static str, bo: &'static str| match d {
            Direction::Left => l,
            Direction::Right => r,
            Direction::Top => t,
            Direction::Bottom => bo,
        };
        let ty = match it.transition {
            Transition::Instant => "INSTANT_TRANSITION",
            Transition::Dissolve => "DISSOLVE",
            Transition::SmartAnimate => "SMART_ANIMATE",
            Transition::SlideIn { direction } => side_from(direction, "SLIDE_FROM_LEFT", "SLIDE_FROM_RIGHT", "SLIDE_FROM_TOP", "SLIDE_FROM_BOTTOM"),
            Transition::Push { direction } => side_from(direction, "PUSH_FROM_LEFT", "PUSH_FROM_RIGHT", "PUSH_FROM_TOP", "PUSH_FROM_BOTTOM"),
            Transition::MoveIn { direction } => side_from(direction, "MOVE_FROM_LEFT", "MOVE_FROM_RIGHT", "MOVE_FROM_TOP", "MOVE_FROM_BOTTOM"),
            Transition::SlideOut { direction } => {
                side_to(direction, "SLIDE_OUT_TO_LEFT", "SLIDE_OUT_TO_RIGHT", "SLIDE_OUT_TO_TOP", "SLIDE_OUT_TO_BOTTOM")
            }
            Transition::MoveOut { direction } => side_to(direction, "MOVE_OUT_TO_LEFT", "MOVE_OUT_TO_RIGHT", "MOVE_OUT_TO_TOP", "MOVE_OUT_TO_BOTTOM"),
        };
        let easing = match it.easing {
            Easing::Linear => "LINEAR",
            Easing::EaseIn => "IN_CUBIC",
            Easing::EaseOut => "OUT_CUBIC",
            Easing::EaseInOut => "INOUT_CUBIC",
            Easing::EaseInBack => "IN_BACK_CUBIC",
            Easing::EaseOutBack => "OUT_BACK_CUBIC",
            Easing::EaseInOutBack => "INOUT_BACK_CUBIC",
            Easing::Gentle => "GENTLE_SPRING",
            Easing::CustomBezier { .. } => "CUSTOM_CUBIC",
            Easing::Quick | Easing::Bouncy | Easing::Slow | Easing::CustomSpring { .. } => {
                self.warn("spring animations other than Gentle were written as ease out");
                "OUT_CUBIC"
            }
        };
        f.extend([("transitionType", e(ty)), ("transitionDuration", fl(f64::from(it.duration_ms) / 1000.0)), ("easingType", e(easing))]);
        if let Easing::CustomBezier { x1, y1, x2, y2 } = it.easing {
            f.push(("easingFunction", arr(vec![fl(x1), fl(y1), fl(x2), fl(y2)])));
        }
    }
}

/// The empty value of a field only written when set (an override clears it with this).
fn unset(key: &str) -> Option<Kv<'static>> {
    Some(match key {
        "mask" | "rectangleCornerRadiiIndependent" | "borderStrokeWeightsIndependent" => b(false),
        "effects" | "dashPattern" | "layoutGrids" | "exportSettings" | "prototypeInteractions" | "fillGeometry" => arr(Vec::new()),
        "cornerSmoothing" | "stackChildPrimaryGrow" => fl(0.0),
        "stackMode" => e("NONE"),
        "stackChildAlignSelf" | "stackPositioning" => e("AUTO"),
        "minSize" | "maxSize" => o(vec![("value", xy(0.0, 0.0))]),
        "gridColumnSpan" | "gridRowSpan" | "maxLines" => int(0),
        "scrollDirection" => e("NONE"),
        "scrollBehavior" => e("SCROLLS"),
        "parameterConsumptionMap" => o(vec![("entries", arr(Vec::new()))]),
        "textTruncation" => e("DISABLED"),
        "hyperlink" => o(vec![("url", s(""))]),
        _ => return None,
    })
}

/// The format's type of a component property (`None`: not written).
fn prop_type(d: &PropDef) -> Option<&'static str> {
    match d {
        PropDef::Boolean { .. } => Some("BOOL"),
        PropDef::Text { .. } => Some("TEXT"),
        PropDef::InstanceSwap { .. } => Some("INSTANCE_SWAP"),
        PropDef::Variant { .. } | PropDef::Slot { .. } => None,
    }
}

/// The key codes of a key trigger (modifiers first), and whether it's a controller button.
fn key_codes(key: &str) -> Option<(Vec<i64>, bool)> {
    if let Some(i) = GAMEPAD_BUTTONS.iter().position(|g| g.eq_ignore_ascii_case(key.trim())) {
        return Some((vec![i as i64], true));
    }
    let (mods, k): (KeyMods, &str) = KeyMods::split(key);
    let code = (0..=255).find(|c| crate::rest::proto::key_name(f64::from(*c)).is_some_and(|n| n.eq_ignore_ascii_case(k)))?;
    let mut codes: Vec<i64> = [(mods.shift, 16), (mods.ctrl, 17), (mods.alt, 18), (mods.cmd, 91)].into_iter().filter(|m| m.0).map(|m| m.1).collect();
    codes.push(i64::from(code));
    Some((codes, false))
}

/// The text style fields of a layer or of an override table entry.
fn style_fields(st: &TextStyle, f: &mut Fields) {
    let line_height = match st.line_height {
        // Percent units count from the font's own line height (100 % is "auto"); a share of the
        // font size is a raw multiplier.
        LineHeight::Auto => o(vec![("value", fl(100.0)), ("units", e("PERCENT"))]),
        LineHeight::Percent(p) => o(vec![("value", fl(p / 100.0)), ("units", e("RAW"))]),
        LineHeight::Pixels(v) => o(vec![("value", fl(v)), ("units", e("PIXELS"))]),
    };
    let letter_spacing = match st.letter_spacing {
        LetterSpacing::Percent(p) => o(vec![("value", fl(p)), ("units", e("PERCENT"))]),
        LetterSpacing::Pixels(v) => o(vec![("value", fl(v)), ("units", e("PIXELS"))]),
    };
    let case = match st.case {
        TextCase::Original => "ORIGINAL",
        TextCase::Upper => "UPPER",
        TextCase::Lower => "LOWER",
        TextCase::Title => "TITLE",
        TextCase::SmallCaps => "SMALL_CAPS",
    };
    let decoration = match st.decoration {
        TextDecoration::None => "NONE",
        TextDecoration::Underline => "UNDERLINE",
        TextDecoration::Strikethrough => "STRIKETHROUGH",
    };
    f.extend([
        ("fontName", o(vec![("family", s(&st.font_family)), ("style", s(&super::style_name(&st.font_style))), ("postscript", s(""))])),
        ("fontSize", fl(st.font_size)),
        ("lineHeight", line_height),
        ("letterSpacing", letter_spacing),
        ("textCase", e(case)),
        ("textDecoration", e(decoration)),
    ]);
    if let Some(url) = &st.link {
        f.push(("hyperlink", o(vec![("url", s(url))])));
    }
}

fn layout_grid(g: &figcraft_doc::LayoutGrid) -> Kv<'static> {
    let (pattern, axis) = match g.kind {
        GridKind::Grid => ("GRID", "X"),
        GridKind::Columns => ("STRIPES", "X"),
        GridKind::Rows => ("STRIPES", "Y"),
    };
    let align = match g.align {
        GridAlign::Stretch => "STRETCH",
        GridAlign::Min => "MIN",
        GridAlign::Center => "CENTER",
        GridAlign::Max => "MAX",
    };
    o(vec![
        ("type", e(align)),
        ("axis", e(axis)),
        ("visible", b(g.visible)),
        ("numSections", int(g.count.into())),
        ("offset", fl(g.offset)),
        ("sectionSize", fl(g.size)),
        ("gutterSize", fl(g.gutter)),
        ("color", color(g.color)),
        ("pattern", e(pattern)),
    ])
}

fn export_setting(x: &figcraft_doc::ExportSetting) -> Kv<'static> {
    let format = match x.format {
        // The format has no WebP export setting.
        figcraft_doc::ExportFormat::Png | figcraft_doc::ExportFormat::Webp => "PNG",
        figcraft_doc::ExportFormat::Jpg => "JPEG",
        figcraft_doc::ExportFormat::Svg => "SVG",
        figcraft_doc::ExportFormat::Pdf => "PDF",
    };
    let (ty, value) = match x.size {
        ExportSize::Scale { value } => ("CONTENT_SCALE", value),
        ExportSize::Width { value } => ("CONTENT_WIDTH", value),
        ExportSize::Height { value } => ("CONTENT_HEIGHT", value),
    };
    o(vec![("suffix", s(&x.suffix)), ("imageType", e(format)), ("constraint", o(vec![("type", e(ty)), ("value", fl(value))]))])
}
