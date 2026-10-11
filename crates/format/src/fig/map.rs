//! Decoded `.fig` message → REST API file JSON, which [`crate::rest`] then imports.
//!
//! The message lists node changes; each names its id (`guid`), its parent and its position among
//! the siblings (`parentIndex`). Nodes are read by field name, so files written with newer
//! schemas still import what is understood. Instances carry only their overrides: their layers
//! are expanded here from the main component (overrides and derived layout matched by id path,
//! component property values applied) so the import looks as saved.

use std::collections::BTreeMap;
use std::rc::Rc;

use figcraft_doc::geom::{Affine, Point};
use serde_json::{Map, Value, json};

use super::kiwi::Kv;

type Obj = Map<String, Value>;

/// Most layers produced (instance expansion included) and deepest nesting followed.
const MAX_NODES: usize = crate::rest::MAX_NODES;
const MAX_DEPTH: usize = 256;
/// Most path commands read from one geometry blob.
const MAX_COMMANDS: usize = 200_000;
/// Id of "no style".
const NO_STYLE: &str = "4294967295:4294967295";

/// The REST API file JSON for message `msg`, with the warnings about what was skipped.
pub(super) fn to_rest(msg: &Kv, name: &str) -> (Value, Vec<String>) {
    let mut m = Mapper::new(msg);
    let doc = m.document();
    let mut out = json!({"name": name, "document": doc});
    if !m.styles.is_empty() {
        out["styles"] = Value::Object(std::mem::take(&mut m.styles));
    }
    if !m.components.is_empty() {
        out["components"] = Value::Object(std::mem::take(&mut m.components));
    }
    if let Some(vars) = m.variables() {
        out["meta"] = vars;
    }
    (out, m.warnings)
}

/// REST importer warnings reworded for a `.fig` import (`None`: drop it).
pub(super) fn adapt_warning(w: String) -> Option<String> {
    if w.contains("geometry=paths") {
        return None;
    }
    if w.contains("without image data") {
        let count = w.split_whitespace().next().unwrap_or("Some");
        return Some(format!("{count} image fill(s) whose image isn't in the file show a grey placeholder"));
    }
    Some(w.replace("the variables JSON", "the file").replace("the file JSON", "the file"))
}

// ---- small readers ----

fn num(n: &Kv, k: &str) -> Option<f64> {
    n.get(k).and_then(Kv::f64)
}

fn guid(v: &Kv) -> Option<String> {
    Some(format!("{}:{}", num(v, "sessionID")? as u64, num(v, "localID")? as u64))
}

/// A `GUID`-holding reference (`StyleId`, `VariableID`, …): its `guid`.
fn ref_guid(v: &Kv) -> Option<String> {
    v.get("guid").and_then(guid).filter(|g| g != NO_STYLE)
}

fn color(c: &Kv) -> Value {
    let ch = |k: &str| num(c, k).unwrap_or(0.0);
    json!({"r": ch("r"), "g": ch("g"), "b": ch("b"), "a": num(c, "a").unwrap_or(1.0)})
}

fn vec2(v: &Kv) -> Option<Value> {
    Some(json!({"x": num(v, "x")?, "y": num(v, "y")?}))
}

fn matrix(m: &Kv) -> Option<Affine> {
    let c = |k: &str| num(m, k);
    let a = Affine::new([c("m00")?, c("m10")?, c("m01")?, c("m11")?, c("m02")?, c("m12")?]);
    a.as_coeffs().iter().all(|x| x.is_finite()).then_some(a)
}

fn rows(a: Affine) -> Value {
    let [a, b, c, d, e, f] = a.as_coeffs();
    json!([[a, c, e], [b, d, f]])
}

fn copy_num(o: &mut Obj, n: &View, from: &str, to: &str) {
    if let Some(x) = n.num(from) {
        o.insert(to.into(), json!(x));
    }
}

fn copy_str(o: &mut Obj, n: &View, from: &str, to: &str) {
    if let Some(s) = n.str(from) {
        o.insert(to.into(), json!(s));
    }
}

/// A node seen through override layers: the last layer with a field wins.
#[derive(Clone)]
struct View<'a> {
    layers: Vec<&'a Kv<'a>>,
}

impl<'a> View<'a> {
    fn of(n: &'a Kv<'a>) -> Self {
        Self { layers: vec![n] }
    }

    fn with(&self, top: impl IntoIterator<Item = &'a Kv<'a>>) -> Self {
        let mut v = self.clone();
        v.layers.extend(top);
        v
    }

    fn get(&self, k: &str) -> Option<&'a Kv<'a>> {
        self.layers.iter().rev().find_map(|l| l.get(k))
    }

    fn num(&self, k: &str) -> Option<f64> {
        self.get(k).and_then(Kv::f64)
    }

    fn str(&self, k: &str) -> Option<&'a str> {
        self.get(k).and_then(Kv::str)
    }

    fn flag(&self, k: &str) -> Option<bool> {
        self.get(k).and_then(Kv::bool)
    }

    fn list(&self, k: &str) -> &'a [Kv<'a>] {
        self.get(k).map(Kv::list).unwrap_or_default()
    }
}

/// Where an instance's expanded layers are: the overrides and derived layout of the outermost
/// instance (by id path), the path of the instance being expanded, and the property values.
#[derive(Clone)]
struct Inst<'a> {
    root: String,
    path: Vec<String>,
    overrides: Rc<BTreeMap<Vec<String>, Vec<&'a Kv<'a>>>>,
    props: Rc<BTreeMap<String, &'a Kv<'a>>>,
    /// Mains being expanded (an instance inside its own main stops there).
    symbols: Vec<String>,
}

struct Mapper<'a> {
    nodes: BTreeMap<String, &'a Kv<'a>>,
    kids: BTreeMap<String, Vec<(&'a str, String)>>,
    blobs: Vec<&'a [u8]>,
    /// Component property definitions: id → (REST name "Name#id", type).
    prop_defs: BTreeMap<String, (String, &'a str)>,
    styles: Obj,
    components: Obj,
    warnings: Vec<String>,
    count: usize,
}

impl<'a> Mapper<'a> {
    fn new(msg: &'a Kv<'a>) -> Self {
        let mut nodes = BTreeMap::new();
        let mut kids: BTreeMap<String, Vec<(&'a str, String)>> = BTreeMap::new();
        let mut prop_defs = BTreeMap::new();
        for n in msg.get("nodeChanges").map(Kv::list).unwrap_or_default() {
            let Some(id) = n.get("guid").and_then(guid) else { continue };
            if n.get("phase").and_then(Kv::str) == Some("REMOVED") {
                continue;
            }
            if let Some(p) = n.get("parentIndex")
                && let Some(parent) = p.get("guid").and_then(guid)
            {
                kids.entry(parent).or_default().push((p.get("position").and_then(Kv::str).unwrap_or(""), id.clone()));
            }
            for d in n.get("componentPropDefs").map(Kv::list).unwrap_or_default() {
                if let (Some(did), Some(name), Some(ty)) =
                    (d.get("id").and_then(guid), d.get("name").and_then(Kv::str), d.get("type").and_then(Kv::str))
                {
                    prop_defs.insert(did.clone(), (format!("{name}#{did}"), ty));
                }
            }
            nodes.insert(id, n);
        }
        for list in kids.values_mut() {
            list.sort();
            list.dedup_by(|a, b| a.1 == b.1);
        }
        let blobs =
            msg.get("blobs").map(Kv::list).unwrap_or_default().iter().map(|b| b.get("bytes").and_then(Kv::bytes).unwrap_or_default()).collect();
        Self { nodes, kids, blobs, prop_defs, styles: Obj::new(), components: Obj::new(), warnings: Vec::new(), count: 0 }
    }

    fn warn(&mut self, w: impl Into<String>) {
        let w = w.into();
        if !self.warnings.contains(&w) {
            self.warnings.push(w);
        }
    }

    fn children(&self, id: &str) -> Vec<String> {
        self.kids.get(id).map(|k| k.iter().map(|(_, c)| c.clone()).collect()).unwrap_or_default()
    }

    /// The DOCUMENT with its pages.
    fn document(&mut self) -> Value {
        let doc_id = self.nodes.iter().find(|(_, n)| n.get("type").and_then(Kv::str) == Some("DOCUMENT")).map(|(id, _)| id.clone());
        let pages: Vec<String> = match &doc_id {
            Some(d) => self.children(d),
            None => self.nodes.iter().filter(|(_, n)| n.get("type").and_then(Kv::str) == Some("CANVAS")).map(|(id, _)| id.clone()).collect(),
        };
        let mut out = Vec::new();
        for p in pages {
            let Some(&n) = self.nodes.get(&p) else { continue };
            if n.get("type").and_then(Kv::str) != Some("CANVAS") {
                self.warn("document children other than pages were skipped");
                continue;
            }
            // The hidden page that holds styles, variables and library copies.
            if n.get("internalOnly").and_then(Kv::bool) == Some(true) {
                continue;
            }
            out.push(self.page(&p, n));
        }
        if out.is_empty() {
            self.warn("the file has no pages");
        }
        json!({"id": doc_id.unwrap_or_else(|| "0:0".into()), "type": "DOCUMENT", "children": out})
    }

    fn page(&mut self, id: &str, n: &'a Kv<'a>) -> Value {
        let v = View::of(n);
        let mut o = Obj::new();
        o.insert("id".into(), json!(id));
        o.insert("type".into(), json!("CANVAS"));
        copy_str(&mut o, &v, "name", "name");
        if let Some(c) = v.get("backgroundColor") {
            o.insert("backgroundColor".into(), color(c));
        }
        if let Some(c) = v.get("prototypeBackgroundColor") {
            o.insert("prototypeBackgrounds".into(), json!([color(c)]));
        }
        let mut flows: Vec<(&str, Value)> = Vec::new();
        let mut kids = Vec::new();
        for c in self.children(id) {
            if let Some(sp) = self.nodes.get(&c).and_then(|k| k.get("prototypeStartingPoint")) {
                let name = sp.get("name").and_then(Kv::str).filter(|s| !s.is_empty()).unwrap_or("Flow");
                flows.push((sp.get("position").and_then(Kv::str).unwrap_or(""), json!({"nodeId": c, "name": name})));
            }
            kids.extend(self.layer(&c, None, Affine::IDENTITY, None, 0));
        }
        flows.sort_by(|a, b| a.0.cmp(b.0));
        if !flows.is_empty() {
            o.insert("flowStartingPoints".into(), Value::Array(flows.into_iter().map(|f| f.1).collect()));
        }
        o.insert("children".into(), Value::Array(kids));
        Value::Object(o)
    }

    /// Layer `id` (inside instance `inst` when expanding one) and its subtree. `space`: from the
    /// parent's coordinates to the enclosing frame's (groups and booleans place their children in
    /// the frame's space, like the REST JSON). `parent`: the parent's view (grid tracks).
    fn layer(&mut self, id: &str, inst: Option<&Inst<'a>>, space: Affine, parent: Option<&View<'a>>, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            self.warn(format!("layers nested deeper than {MAX_DEPTH} levels were skipped"));
            return None;
        }
        if self.count >= MAX_NODES {
            self.warn(format!("the file has more than {MAX_NODES} layers; the rest were skipped"));
            return None;
        }
        let base = *self.nodes.get(id)?;
        let path = inst.map(|i| [i.path.as_slice(), &[id.to_string()]].concat());
        let view = match (inst, &path) {
            (Some(i), Some(p)) => View::of(base).with(i.overrides.get(p).into_iter().flatten().copied()),
            _ => View::of(base),
        };
        let rest_id = match (inst, &path) {
            (Some(i), Some(p)) => format!("I{};{}", i.root, p.join(";")),
            _ => id.to_string(),
        };
        let ty = match view.str("type").unwrap_or("") {
            "ROUNDED_RECTANGLE" => "RECTANGLE",
            "SYMBOL" => "COMPONENT",
            "FRAME" if view.flag("isStateGroup") == Some(true) => "COMPONENT_SET",
            "FRAME" if view.flag("resizeToFit") == Some(true) => "GROUP",
            "VARIABLE" | "VARIABLE_SET" => return None,
            t => t,
        };
        self.count += 1;
        let mut o = Obj::new();
        o.insert("id".into(), json!(rest_id));
        o.insert("type".into(), json!(ty));
        copy_str(&mut o, &view, "name", "name");
        self.common(&mut o, &view, ty, space, parent);
        match ty {
            "TEXT" => self.text(&mut o, &view),
            "VECTOR" | "STAR" | "REGULAR_POLYGON" => self.geometry(&mut o, &view),
            _ => {}
        }
        if ty == "COMPONENT" || ty == "COMPONENT_SET" {
            self.component(&mut o, &view, &rest_id);
        }
        self.prop_refs(&mut o, &view, inst);
        let child_space = if matches!(ty, "GROUP" | "BOOLEAN_OPERATION") {
            space * view.get("transform").and_then(matrix).unwrap_or_default()
        } else {
            Affine::IDENTITY
        };
        let mut kids = Vec::new();
        if ty == "INSTANCE" {
            if let Some((sub, sym)) = self.instance(&mut o, &view, id, inst, &rest_id) {
                for c in self.children(&sym) {
                    kids.extend(self.layer(&c, Some(&sub), child_space, Some(&view), depth + 1));
                }
            }
        } else {
            for c in self.children(id) {
                kids.extend(self.layer(&c, inst, child_space, Some(&view), depth + 1));
            }
        }
        if !kids.is_empty() {
            o.insert("children".into(), Value::Array(kids));
        }
        Some(Value::Object(o))
    }

    /// Fields every layer type shares.
    fn common(&mut self, o: &mut Obj, v: &View<'a>, ty: &str, space: Affine, parent: Option<&View<'a>>) {
        for (from, to) in [("visible", "visible"), ("locked", "locked"), ("mask", "isMask")] {
            if let Some(b) = v.flag(from) {
                o.insert(to.into(), json!(b));
            }
        }
        copy_num(o, v, "opacity", "opacity");
        if let Some(b) = v.str("blendMode").filter(|b| *b != "UNSET") {
            o.insert("blendMode".into(), json!(b));
        }
        if let Some(s) = v.get("size").and_then(vec2) {
            o.insert("size".into(), s);
        }
        let t = v.get("transform").and_then(matrix).unwrap_or_default();
        o.insert("relativeTransform".into(), rows(space * t));
        // Paints, from the shared style when one is linked (the layer's own copy can be stale).
        let mut styles = Obj::new();
        let fill_style = self.style(v, "styleIdForFill", "inheritFillStyleID", "fill", &mut styles);
        let fills = fill_style.map_or_else(|| v.list("fillPaints"), |s| s.get("fillPaints").map(Kv::list).unwrap_or_default());
        o.insert("fills".into(), Value::Array(fills.iter().filter_map(paint).collect()));
        let stroke_style = self.style(v, "styleIdForStrokeFill", "inheritFillStyleIDForStroke", "stroke", &mut styles);
        let strokes = stroke_style.map_or_else(|| v.list("strokePaints"), |s| s.get("fillPaints").map(Kv::list).unwrap_or_default());
        o.insert("strokes".into(), Value::Array(strokes.iter().filter_map(paint).collect()));
        let effect_style = self.style(v, "styleIdForEffect", "inheritEffectStyleID", "effect", &mut styles);
        let effects = effect_style.map_or_else(|| v.list("effects"), |s| s.get("effects").map(Kv::list).unwrap_or_default());
        if !effects.is_empty() {
            o.insert("effects".into(), Value::Array(effects.iter().filter_map(effect).collect()));
        }
        let grid_style = self.style(v, "styleIdForGrid", "inheritGridStyleID", "grid", &mut styles);
        let grids = grid_style.map_or_else(|| v.list("layoutGrids"), |s| s.get("layoutGrids").map(Kv::list).unwrap_or_default());
        if !grids.is_empty() {
            o.insert("layoutGrids".into(), Value::Array(grids.iter().map(layout_grid).collect()));
        }
        if ty == "TEXT" {
            self.style(v, "styleIdForText", "inheritTextStyleID", "text", &mut styles);
        }
        if !styles.is_empty() {
            o.insert("styles".into(), Value::Object(styles));
        }
        stroke(o, v);
        copy_num(o, v, "cornerRadius", "cornerRadius");
        copy_num(o, v, "cornerSmoothing", "cornerSmoothing");
        if v.flag("rectangleCornerRadiiIndependent") == Some(true) {
            let r = |k: &str| v.num(k).unwrap_or(0.0);
            o.insert(
                "rectangleCornerRadii".into(),
                json!([
                    r("rectangleTopLeftCornerRadius"),
                    r("rectangleTopRightCornerRadius"),
                    r("rectangleBottomRightCornerRadius"),
                    r("rectangleBottomLeftCornerRadius")
                ]),
            );
        }
        let constraint = |k: &str, min: &'static str, max: &'static str, both: &'static str| match v.str(k) {
            Some("CENTER") => "CENTER",
            Some("MAX" | "FIXED_MAX") => max,
            Some("STRETCH") => both,
            Some("SCALE") => "SCALE",
            _ => min,
        };
        o.insert(
            "constraints".into(),
            json!({
                "horizontal": constraint("horizontalConstraint", "LEFT", "RIGHT", "LEFT_RIGHT"),
                "vertical": constraint("verticalConstraint", "TOP", "BOTTOM", "TOP_BOTTOM"),
            }),
        );
        if matches!(ty, "FRAME" | "COMPONENT" | "COMPONENT_SET" | "INSTANCE" | "SECTION") {
            o.insert("clipsContent".into(), json!(!v.flag("frameMaskDisabled").unwrap_or(false)));
            self.auto_layout(o, v);
        }
        child_layout(o, v, parent);
        let exports: Vec<Value> = v.list("exportSettings").iter().map(export_setting).collect();
        if !exports.is_empty() {
            o.insert("exportSettings".into(), Value::Array(exports));
        }
        if let Some(b) = v.flag("sectionContentsHidden") {
            o.insert("sectionContentsHidden".into(), json!(b));
        }
        self.prototype(o, v);
        self.variable_bindings(o, v);
    }

    /// The style node a layer links in `key` (or the older `legacy` field), recorded for the REST
    /// `styles` map under `slot`.
    fn style(&mut self, v: &View<'a>, key: &str, legacy: &str, slot: &str, out: &mut Obj) -> Option<&'a Kv<'a>> {
        let id = v.get(key).and_then(ref_guid).or_else(|| v.get(legacy).and_then(guid).filter(|g| g != NO_STYLE))?;
        let node = *self.nodes.get(&id)?;
        node.get("styleType").and_then(Kv::str)?;
        out.insert(slot.into(), json!(id));
        if !self.styles.contains_key(&id) {
            let name = node.get("name").and_then(Kv::str).unwrap_or("Style");
            let desc = node.get("styleDescription").or_else(|| node.get("description")).and_then(Kv::str).unwrap_or("");
            self.styles.insert(id, json!({"name": name, "description": desc}));
        }
        Some(node)
    }

    fn auto_layout(&mut self, o: &mut Obj, v: &View<'a>) {
        let Some(mode) = v.str("stackMode").filter(|m| matches!(*m, "HORIZONTAL" | "VERTICAL" | "GRID")) else { return };
        o.insert("layoutMode".into(), json!(mode));
        copy_num(o, v, "stackSpacing", "itemSpacing");
        copy_num(o, v, "stackCounterSpacing", "counterAxisSpacing");
        if v.str("stackWrap") == Some("WRAP") {
            o.insert("layoutWrap".into(), json!("WRAP"));
        }
        let (h, vert) = (v.num("stackHorizontalPadding").unwrap_or(0.0), v.num("stackVerticalPadding").unwrap_or(0.0));
        o.insert("paddingLeft".into(), json!(h));
        o.insert("paddingTop".into(), json!(vert));
        o.insert("paddingRight".into(), json!(v.num("stackPaddingRight").unwrap_or(h)));
        o.insert("paddingBottom".into(), json!(v.num("stackPaddingBottom").unwrap_or(vert)));
        let main = match v.str("stackPrimaryAlignItems").or_else(|| v.str("stackJustify")) {
            Some("CENTER") => "CENTER",
            Some("MAX") => "MAX",
            Some("SPACE_BETWEEN" | "SPACE_EVENLY" | "SPACE_AROUND" | "SPACE_EVENLY_CSS") => "SPACE_BETWEEN",
            _ => "MIN",
        };
        o.insert("primaryAxisAlignItems".into(), json!(main));
        let cross = match v.str("stackCounterAlignItems").or_else(|| v.str("stackAlign")) {
            Some("CENTER") => "CENTER",
            Some("MAX") => "MAX",
            Some("BASELINE") => "BASELINE",
            _ => "MIN",
        };
        o.insert("counterAxisAlignItems".into(), json!(cross));
        let sizing = |k: &str| if v.str(k).is_some_and(|s| s.starts_with("RESIZE_TO_FIT")) { "AUTO" } else { "FIXED" };
        o.insert("primaryAxisSizingMode".into(), json!(sizing("stackPrimarySizing")));
        o.insert("counterAxisSizingMode".into(), json!(sizing("stackCounterSizing")));
        if let Some(b) = v.flag("bordersTakeSpace") {
            o.insert("strokesIncludedInLayout".into(), json!(b));
        }
        if let Some(b) = v.flag("stackReverseZIndex") {
            o.insert("itemReverseZIndex".into(), json!(b));
        }
        if mode == "GRID" {
            copy_num(o, v, "gridColumnGap", "gridColumnGap");
            copy_num(o, v, "gridRowGap", "gridRowGap");
            o.insert("gridColumnCount".into(), json!(tracks(v, "gridColumns").len()));
            o.insert("gridRowCount".into(), json!(tracks(v, "gridRows").len()));
            let sizes = ["gridColumnsSizing", "gridRowsSizing"]
                .iter()
                .flat_map(|k| v.get(k).and_then(|m| m.get("entries")).map(Kv::list).unwrap_or_default());
            let flexible = sizes.into_iter().all(|e| {
                let t = e.get("trackSize");
                ["minSizing", "maxSizing"]
                    .iter()
                    .all(|s| t.and_then(|t| t.get(s)).and_then(|s| s.get("type")).and_then(Kv::str).is_none_or(|x| x == "FLEX"))
            });
            if !flexible {
                self.warn("grid tracks of fixed or content size import as equal tracks");
            }
        }
    }

    fn text(&mut self, o: &mut Obj, v: &View<'a>) {
        let style_node = v
            .get("styleIdForText")
            .and_then(ref_guid)
            .or_else(|| v.get("inheritTextStyleID").and_then(guid))
            .and_then(|s| self.nodes.get(&s).copied());
        let sv = v.with(style_node.filter(|s| s.get("styleType").and_then(Kv::str) == Some("TEXT")));
        let td = v.get("textData");
        let chars = td.and_then(|t| t.get("characters")).and_then(Kv::str).unwrap_or("");
        o.insert("characters".into(), json!(chars));
        let mut style = text_style(&sv, None);
        for k in ["textAlignHorizontal", "textAlignVertical", "textAutoResize", "textTruncation"] {
            copy_str(&mut style, v, k, k);
        }
        if let Some(n) = v.num("maxLines").filter(|n| *n >= 1.0) {
            style.insert("maxLines".into(), json!(n));
        }
        let size = sv.num("fontSize");
        o.insert("style".into(), Value::Object(style));
        let Some(td) = td else { return };
        let ids: Vec<u64> = td.get("characterStyleIDs").map(Kv::list).unwrap_or_default().iter().filter_map(Kv::f64).map(|i| i as u64).collect();
        if ids.iter().any(|i| *i != 0) {
            let mut table = Obj::new();
            for e in td.get("styleOverrideTable").map(Kv::list).unwrap_or_default() {
                let Some(sid) = num(e, "styleID") else { continue };
                let mut s = text_style(&View::of(e), size);
                if let Some(fills) = e.get("fillPaints") {
                    s.insert("fills".into(), Value::Array(fills.list().iter().filter_map(paint).collect()));
                }
                table.insert((sid as u64).to_string(), Value::Object(s));
            }
            o.insert("characterStyleOverrides".into(), json!(ids));
            o.insert("styleOverrideTable".into(), Value::Object(table));
        }
        let lines: Vec<&str> = td
            .get("lines")
            .map(Kv::list)
            .unwrap_or_default()
            .iter()
            .map(|l| match l.get("lineType").and_then(Kv::str) {
                Some("ORDERED_LIST") => "ORDERED",
                Some("UNORDERED_LIST") => "UNORDERED",
                _ => "NONE",
            })
            .collect();
        if lines.iter().any(|l| *l != "NONE") {
            o.insert("lineTypes".into(), json!(lines));
        }
    }

    /// `fillGeometry` / `strokeGeometry` as SVG path data, and star/polygon parameters.
    fn geometry(&mut self, o: &mut Obj, v: &View<'a>) {
        for key in ["fillGeometry", "strokeGeometry"] {
            let mut out = Vec::new();
            for p in v.list(key) {
                let Some(blob) = num(p, "commandsBlob").and_then(|i| self.blobs.get(i as usize)).copied() else { continue };
                let Some(d) = svg_path(blob) else {
                    self.warn("damaged vector geometry was skipped");
                    continue;
                };
                if d.is_empty() {
                    continue;
                }
                let winding = if p.get("windingRule").and_then(Kv::str) == Some("ODD") { "EVENODD" } else { "NONZERO" };
                out.push(json!({"path": d, "windingRule": winding}));
            }
            if !out.is_empty() {
                o.insert(key.into(), Value::Array(out));
            }
        }
        let vector = o.get("type").and_then(Value::as_str) == Some("VECTOR");
        if vector && !o.contains_key("fillGeometry") && !o.contains_key("strokeGeometry") {
            self.warn("vectors without geometry are empty");
        }
        if !vector {
            copy_num(o, v, "count", "pointCount");
            copy_num(o, v, "starInnerScale", "innerRadius");
        }
    }

    /// Component property definitions and description.
    fn component(&mut self, o: &mut Obj, v: &View<'a>, id: &str) {
        let mut defs = Obj::new();
        for d in v.list("componentPropDefs") {
            if d.get("isDeleted").and_then(Kv::bool) == Some(true) {
                continue;
            }
            let Some((name, ty)) = d.get("id").and_then(guid).and_then(|i| self.prop_defs.get(&i).cloned()) else { continue };
            let init = d.get("initialValue");
            let def = match ty {
                "BOOL" => json!({"type": "BOOLEAN", "defaultValue": init.and_then(|i| i.get("boolValue")).and_then(Kv::bool).unwrap_or(true)}),
                "TEXT" => json!({"type": "TEXT", "defaultValue": init.and_then(prop_text).unwrap_or("")}),
                "INSTANCE_SWAP" => match init.and_then(|i| i.get("guidValue")).and_then(guid) {
                    Some(g) => json!({"type": "INSTANCE_SWAP", "defaultValue": g}),
                    None => continue,
                },
                "VARIANT" => json!({"type": "VARIANT"}),
                _ => {
                    self.warn(format!("{} component properties are not supported and were skipped", ty.to_ascii_lowercase()));
                    continue;
                }
            };
            defs.insert(name, def);
        }
        if !defs.is_empty() {
            o.insert("componentPropertyDefinitions".into(), Value::Object(defs));
        }
        if let Some(d) = v.str("symbolDescription").or_else(|| v.str("description")).filter(|d| !d.is_empty()) {
            self.components.insert(id.to_string(), json!({"description": d}));
        }
    }

    /// A layer's component property references, and inside an instance the values they take.
    fn prop_refs(&mut self, o: &mut Obj, v: &View<'a>, inst: Option<&Inst<'a>>) {
        let mut refs = Obj::new();
        for e in v.get("parameterConsumptionMap").and_then(|m| m.get("entries")).map(Kv::list).unwrap_or_default() {
            let field = match e.get("variableField").and_then(Kv::str) {
                Some("VISIBLE") => "visible",
                Some("TEXT_DATA") => "characters",
                Some("OVERRIDDEN_SYMBOL_ID") => "mainComponent",
                _ => continue,
            };
            let Some(def) =
                e.get("variableData").and_then(|d| d.get("value")).and_then(|x| x.get("propRefValue")).and_then(|p| p.get("defId")).and_then(guid)
            else {
                continue;
            };
            if let Some((name, _)) = self.prop_defs.get(&def) {
                refs.insert(field.into(), json!(name));
            }
            let Some(val) = inst.and_then(|i| i.props.get(&def)) else { continue };
            match field {
                "visible" => {
                    if let Some(b) = val.get("boolValue").and_then(Kv::bool) {
                        o.insert("visible".into(), json!(b));
                    }
                }
                "characters" => {
                    if let Some(t) = prop_text(val) {
                        o.insert("characters".into(), json!(t));
                    }
                }
                _ => {}
            }
        }
        if !refs.is_empty() {
            o.insert("componentPropertyReferences".into(), Value::Object(refs));
        }
    }

    /// An instance's main and properties; the context its expanded layers are built in.
    fn instance(&mut self, o: &mut Obj, v: &View<'a>, id: &str, outer: Option<&Inst<'a>>, rest_id: &str) -> Option<(Inst<'a>, String)> {
        let sd = v.get("symbolData");
        let mut sym = v.get("overriddenSymbolID").and_then(guid).or_else(|| sd.and_then(|s| s.get("symbolID")).and_then(guid));
        // An instance swap property value picks the main.
        if let Some(i) = outer
            && let Some(def) = prop_ref(v, "OVERRIDDEN_SYMBOL_ID")
            && let Some(g) = i.props.get(&def).and_then(|p| p.get("guidValue")).and_then(guid)
        {
            sym = Some(g);
        }
        let Some(sym) = sym else {
            self.warn("instances without a main component were imported as empty frames");
            return None;
        };
        o.insert("componentId".into(), json!(sym));
        let assignments = v.list("componentPropAssignments");
        let mut props = Obj::new();
        for a in assignments {
            let Some((name, ty)) = a.get("defID").and_then(guid).and_then(|d| self.prop_defs.get(&d).cloned()) else { continue };
            let val = a.get("value");
            let p = match ty {
                "BOOL" => val.and_then(|x| x.get("boolValue")).and_then(Kv::bool).map(|b| json!({"type": "BOOLEAN", "value": b})),
                "TEXT" => val.and_then(prop_text).map(|t| json!({"type": "TEXT", "value": t})),
                "INSTANCE_SWAP" => val.and_then(|x| x.get("guidValue")).and_then(guid).map(|g| json!({"type": "INSTANCE_SWAP", "value": g})),
                _ => None,
            };
            if let Some(p) = p {
                props.insert(name, p);
            }
        }
        if !props.is_empty() {
            o.insert("componentProperties".into(), Value::Object(props));
        }
        if !self.nodes.contains_key(&sym) {
            self.warn("instances of components that aren't in the file show no layers");
            return None;
        }
        if outer.is_some_and(|i| i.symbols.contains(&sym)) {
            return None;
        }
        // Overrides by id path below this instance (the instance itself is addressed by its main's id).
        let own_path: Vec<String> = outer.map(|i| [i.path.as_slice(), &[id.to_string()]].concat()).unwrap_or_default();
        let mut overrides: BTreeMap<Vec<String>, Vec<&'a Kv<'a>>> = outer.map(|i| (*i.overrides).clone()).unwrap_or_default();
        let lists = [sd.and_then(|s| s.get("symbolOverrides")).map(Kv::list).unwrap_or_default(), v.list("derivedSymbolData")];
        for (layer, list) in lists.into_iter().enumerate() {
            for ov in list {
                let rel: Vec<String> =
                    ov.get("guidPath").and_then(|g| g.get("guids")).map(Kv::list).unwrap_or_default().iter().filter_map(guid).collect();
                if rel.is_empty() || rel == [sym.clone()] {
                    continue;
                }
                let e = overrides.entry([own_path.as_slice(), &rel].concat()).or_default();
                // The outer instance's overrides win over this one's own; derived layout comes last.
                if layer == 0 && outer.is_some() {
                    e.insert(0, ov);
                } else {
                    e.push(ov);
                }
            }
        }
        let mut all_props: BTreeMap<String, &'a Kv<'a>> = outer.map(|i| (*i.props).clone()).unwrap_or_default();
        for a in assignments {
            if let (Some(d), Some(val)) = (a.get("defID").and_then(guid), a.get("value")) {
                all_props.entry(d).or_insert(val);
            }
        }
        let mut symbols = outer.map(|i| i.symbols.clone()).unwrap_or_default();
        symbols.push(sym.clone());
        let root = outer.map_or_else(|| rest_id.to_string(), |i| i.root.clone());
        Some((Inst { root, path: own_path, overrides: Rc::new(overrides), props: Rc::new(all_props), symbols }, sym))
    }

    fn prototype(&mut self, o: &mut Obj, v: &View<'a>) {
        let mut list = Vec::new();
        for it in v.list("prototypeInteractions") {
            if it.get("isDeleted").and_then(Kv::bool) == Some(true) {
                continue;
            }
            if let Some(i) = self.interaction(it) {
                list.push(i);
            }
        }
        if !list.is_empty() {
            o.insert("interactions".into(), Value::Array(list));
        }
        let overflow = match v.str("scrollDirection") {
            Some("HORIZONTAL") => Some("HORIZONTAL_SCROLLING"),
            Some("VERTICAL") => Some("VERTICAL_SCROLLING"),
            Some("BOTH") => Some("HORIZONTAL_AND_VERTICAL_SCROLLING"),
            _ => None,
        };
        if let Some(d) = overflow {
            o.insert("overflowDirection".into(), json!(d));
        }
        let behavior = match v.str("scrollBehavior") {
            Some("FIXED_WHEN_CHILD_OF_SCROLLING_FRAME") => Some("FIXED"),
            Some("STICKY_SCROLLS") => Some("STICKY_SCROLLS"),
            _ => None,
        };
        if let Some(b) = behavior {
            o.insert("scrollBehavior".into(), json!(b));
        }
        copy_str(o, v, "overlayPositionType", "overlayPositionType");
        copy_str(o, v, "overlayBackgroundInteraction", "overlayBackgroundInteraction");
        if let Some(a) = v.get("overlayBackgroundAppearance") {
            let ty = a.get("backgroundType").and_then(Kv::str).unwrap_or("NONE");
            o.insert("overlayBackground".into(), json!({"type": ty, "color": a.get("backgroundColor").map_or(Value::Null, color)}));
        }
    }

    fn interaction(&mut self, it: &Kv) -> Option<Value> {
        let ev = it.get("event");
        let ty = match ev.and_then(|e| e.get("interactionType")).and_then(Kv::str).unwrap_or("ON_CLICK") {
            "DRAG" => "ON_DRAG",
            "MOUSE_IN" => "MOUSE_ENTER",
            "MOUSE_OUT" => "MOUSE_LEAVE",
            t => t,
        };
        let mut trigger = json!({"type": ty});
        if let Some(t) = ev.and_then(|e| num(e, "transitionTimeout")) {
            trigger["timeout"] = json!(t * 1000.0);
        }
        if let Some(k) = ev.and_then(|e| e.get("keyTrigger")) {
            let codes: Vec<f64> = k.get("keyCodes").map(Kv::list).unwrap_or_default().iter().filter_map(Kv::f64).collect();
            trigger["keyCodes"] = json!(codes);
            if let Some(d) = k.get("triggerDevice").and_then(Kv::str) {
                trigger["device"] = json!(if d.contains("KEYBOARD") { "KEYBOARD" } else { d });
            }
        }
        let mut actions = Vec::new();
        for a in it.get("actions").map(Kv::list).unwrap_or_default() {
            match a.get("connectionType").and_then(Kv::str) {
                Some("INTERNAL_NODE") | None => {
                    let Some(dest) = a.get("transitionNodeID").and_then(guid) else { continue };
                    let nav = match a.get("navigationType").and_then(Kv::str) {
                        Some("SWAP_STATE") => "CHANGE_TO",
                        Some(n) => n,
                        None => "NAVIGATE",
                    };
                    let mut act = json!({"type": "NODE", "destinationId": dest, "navigation": nav});
                    if let Some(t) = transition(a) {
                        act["transition"] = t;
                    }
                    if let Some(b) = a.get("transitionPreserveScroll").and_then(Kv::bool) {
                        act["preserveScrollPosition"] = json!(b);
                    }
                    actions.push(act);
                }
                Some("URL") => {
                    if let Some(u) = a.get("connectionURL").and_then(Kv::str) {
                        actions.push(json!({"type": "URL", "url": u}));
                    }
                }
                Some("BACK") => actions.push(json!({"type": "BACK"})),
                Some("CLOSE") => actions.push(json!({"type": "CLOSE"})),
                Some("NONE") => {}
                Some(other) => {
                    self.warn(format!("{} prototype actions are not imported from .fig files yet", other.to_ascii_lowercase().replace('_', " ")))
                }
            }
        }
        (!actions.is_empty()).then(|| json!({"trigger": trigger, "actions": actions}))
    }

    fn variable_bindings(&mut self, o: &mut Obj, v: &View<'a>) {
        let mut bound = Obj::new();
        let mut text_bound = Obj::new();
        for e in v.get("variableConsumptionMap").and_then(|m| m.get("entries")).map(Kv::list).unwrap_or_default() {
            let Some(var) = alias(e.get("variableData")) else { continue };
            let a = json!({"type": "VARIABLE_ALIAS", "id": var});
            let field = e.get("variableField").and_then(Kv::str).unwrap_or("");
            match (binding_key(field), text_binding_key(field)) {
                (Some(k), _) => {
                    bound.insert(k.into(), a);
                }
                (_, Some(k)) => {
                    text_bound.insert(k.into(), a);
                }
                _ if field == "CORNER_RADIUS" => {
                    for k in ["topLeftRadius", "topRightRadius", "bottomLeftRadius", "bottomRightRadius"] {
                        bound.insert(k.into(), a.clone());
                    }
                }
                _ if field == "WIDTH" || field == "HEIGHT" => {
                    let axis = if field == "WIDTH" { "x" } else { "y" };
                    let size = bound.entry("size").or_insert_with(|| json!({}));
                    size[axis] = a;
                }
                _ => self.warn(format!("variables bound to {} are not imported from .fig files yet", field.to_ascii_lowercase().replace('_', " "))),
            }
        }
        if !bound.is_empty() {
            o.insert("boundVariables".into(), Value::Object(bound));
        }
        if !text_bound.is_empty()
            && let Some(Value::Object(s)) = o.get_mut("style")
        {
            s.insert("boundVariables".into(), Value::Object(text_bound));
        }
        let mut modes = Obj::new();
        for e in v.get("variableModeBySetMap").and_then(|m| m.get("entries")).map(Kv::list).unwrap_or_default() {
            if let (Some(set), Some(mode)) = (e.get("variableSetID").and_then(ref_guid), e.get("variableModeID").and_then(guid)) {
                modes.insert(set, json!(mode));
            }
        }
        if !modes.is_empty() {
            o.insert("explicitVariableModes".into(), Value::Object(modes));
        }
    }

    /// Collections and variables, as the local variables response's `meta`.
    fn variables(&self) -> Option<Value> {
        let of_type = |t: &'static str| {
            self.nodes
                .iter()
                .filter(move |(_, n)| n.get("type").and_then(Kv::str) == Some(t) && n.get("isSoftDeleted").and_then(Kv::bool) != Some(true))
        };
        let mut collections = Obj::new();
        for (id, n) in of_type("VARIABLE_SET") {
            let mut modes: Vec<(&str, Value)> = n
                .get("variableSetModes")
                .map(Kv::list)
                .unwrap_or_default()
                .iter()
                .filter_map(|m| {
                    let mode = json!({"modeId": m.get("id").and_then(guid)?, "name": m.get("name").and_then(Kv::str).unwrap_or("Mode")});
                    Some((m.get("sortPosition").and_then(Kv::str).unwrap_or(""), mode))
                })
                .collect();
            modes.sort_by(|a, b| a.0.cmp(b.0));
            let default = modes.first().map_or(Value::Null, |m| m.1["modeId"].clone());
            let name = n.get("name").and_then(Kv::str).unwrap_or("Collection");
            let modes: Vec<Value> = modes.into_iter().map(|m| m.1).collect();
            collections.insert(id.clone(), json!({"id": id, "name": name, "modes": modes, "defaultModeId": default}));
        }
        let mut variables = Obj::new();
        for (id, n) in of_type("VARIABLE") {
            let Some(set) = n.get("variableSetID").and_then(ref_guid).filter(|s| collections.contains_key(s)) else { continue };
            let ty = n.get("variableResolvedType").and_then(Kv::str).unwrap_or("");
            if !matches!(ty, "BOOLEAN" | "FLOAT" | "STRING" | "COLOR") {
                continue;
            }
            let mut values = Obj::new();
            for e in n.get("variableDataValues").and_then(|m| m.get("entries")).map(Kv::list).unwrap_or_default() {
                if let (Some(mode), Some(val)) = (e.get("modeID").and_then(guid), e.get("variableData").and_then(variable_value)) {
                    values.insert(mode, val);
                }
            }
            let name = n.get("name").and_then(Kv::str).unwrap_or("Variable");
            let desc = n.get("description").and_then(Kv::str).unwrap_or("");
            variables.insert(
                id.clone(),
                json!({"id": id, "name": name, "variableCollectionId": set, "resolvedType": ty, "valuesByMode": values, "description": desc}),
            );
        }
        (!collections.is_empty()).then(|| json!({"variableCollections": collections, "variables": variables}))
    }
}

/// The REST `boundVariables` key of a bound layer field.
fn binding_key(field: &str) -> Option<&'static str> {
    Some(match field {
        "STROKE_WEIGHT" => "strokeWeight",
        "STACK_SPACING" => "itemSpacing",
        "STACK_COUNTER_SPACING" => "counterAxisSpacing",
        "STACK_PADDING_LEFT" => "paddingLeft",
        "STACK_PADDING_TOP" => "paddingTop",
        "STACK_PADDING_RIGHT" => "paddingRight",
        "STACK_PADDING_BOTTOM" => "paddingBottom",
        "VISIBLE" => "visible",
        "TEXT_DATA" => "characters",
        "OPACITY" => "opacity",
        "MIN_WIDTH" => "minWidth",
        "MAX_WIDTH" => "maxWidth",
        "MIN_HEIGHT" => "minHeight",
        "MAX_HEIGHT" => "maxHeight",
        "RECTANGLE_TOP_LEFT_CORNER_RADIUS" => "topLeftRadius",
        "RECTANGLE_TOP_RIGHT_CORNER_RADIUS" => "topRightRadius",
        "RECTANGLE_BOTTOM_LEFT_CORNER_RADIUS" => "bottomLeftRadius",
        "RECTANGLE_BOTTOM_RIGHT_CORNER_RADIUS" => "bottomRightRadius",
        _ => return None,
    })
}

/// The REST text style `boundVariables` key of a bound text field.
fn text_binding_key(field: &str) -> Option<&'static str> {
    Some(match field {
        "FONT_SIZE" => "fontSize",
        "FONT_FAMILY" => "fontFamily",
        "FONT_STYLE" => "fontStyle",
        "LETTER_SPACING" => "letterSpacing",
        "LINE_HEIGHT" => "lineHeight",
        "PARAGRAPH_SPACING" => "paragraphSpacing",
        "PARAGRAPH_INDENT" => "paragraphIndent",
        _ => return None,
    })
}

fn stroke(o: &mut Obj, v: &View) {
    copy_num(o, v, "strokeWeight", "strokeWeight");
    if let Some(a) = v.str("strokeAlign").filter(|a| *a != "OFFSET") {
        o.insert("strokeAlign".into(), json!(a));
    }
    copy_str(o, v, "strokeJoin", "strokeJoin");
    if let Some(cap) = v.str("strokeCap") {
        let cap = match cap {
            "ARROW_LINES" => "LINE_ARROW",
            "ARROW_EQUILATERAL" => "TRIANGLE_ARROW",
            c => c,
        };
        o.insert("strokeCap".into(), json!(cap));
    }
    let dashes: Vec<f64> = v.list("dashPattern").iter().filter_map(Kv::f64).collect();
    if !dashes.is_empty() {
        o.insert("strokeDashes".into(), json!(dashes));
    }
    // A miter limit L bevels joins sharper than 2·asin(1/L).
    if let Some(l) = v.num("miterLimit").filter(|l| *l > 1.0) {
        o.insert("strokeMiterAngle".into(), json!((2.0 * (1.0 / l).asin()).to_degrees()));
    }
    if v.flag("borderStrokeWeightsIndependent") == Some(true) {
        let w = v.num("strokeWeight").unwrap_or(0.0);
        let side = |k: &str| v.num(k).unwrap_or(w);
        o.insert(
            "individualStrokeWeights".into(),
            json!({"top": side("borderTopWeight"), "right": side("borderRightWeight"), "bottom": side("borderBottomWeight"), "left": side("borderLeftWeight")}),
        );
    }
}

/// How a layer sizes and sits in an auto layout parent.
fn child_layout(o: &mut Obj, v: &View, parent: Option<&View>) {
    copy_num(o, v, "stackChildPrimaryGrow", "layoutGrow");
    if v.str("stackChildAlignSelf") == Some("STRETCH") {
        o.insert("layoutAlign".into(), json!("STRETCH"));
    }
    if v.str("stackPositioning") == Some("ABSOLUTE") {
        o.insert("layoutPositioning".into(), json!("ABSOLUTE"));
    }
    for (k, w, h) in [("minSize", "minWidth", "minHeight"), ("maxSize", "maxWidth", "maxHeight")] {
        if let Some(val) = v.get(k).and_then(|s| s.get("value")) {
            for (axis, to) in [("x", w), ("y", h)] {
                if let Some(x) = num(val, axis).filter(|x| *x > 0.0 && *x < 1e9) {
                    o.insert(to.into(), json!(x));
                }
            }
        }
    }
    if let Some(p) = parent.filter(|p| p.str("stackMode") == Some("GRID")) {
        for (anchor, list, to) in [("gridColumnAnchor", "gridColumns", "gridColumnAnchorIndex"), ("gridRowAnchor", "gridRows", "gridRowAnchorIndex")]
        {
            if let Some(i) = v.get(anchor).and_then(guid).and_then(|g| tracks(p, list).iter().position(|t| *t == g)) {
                o.insert(to.into(), json!(i));
            }
        }
        copy_num(o, v, "gridColumnSpan", "gridColumnSpan");
        copy_num(o, v, "gridRowSpan", "gridRowSpan");
    }
}

fn paint(p: &Kv) -> Option<Value> {
    let ty = p.get("type").and_then(Kv::str)?;
    let mut o = Obj::new();
    o.insert("type".into(), json!(ty));
    if let Some(x) = num(p, "opacity") {
        o.insert("opacity".into(), json!(x));
    }
    if let Some(b) = p.get("visible").and_then(Kv::bool) {
        o.insert("visible".into(), json!(b));
    }
    if let Some(b) = p.get("blendMode").and_then(Kv::str) {
        o.insert("blendMode".into(), json!(b));
    }
    let t = p.get("transform").and_then(matrix);
    match ty {
        "SOLID" => {
            if let Some(c) = p.get("color") {
                o.insert("color".into(), color(c));
            }
            if let Some(var) = alias(p.get("colorVar")) {
                o.insert("boundVariables".into(), json!({"color": {"type": "VARIABLE_ALIAS", "id": var}}));
            }
        }
        _ if ty.starts_with("GRADIENT_") => {
            // The paint transform maps the layer's unit box to gradient space, where a linear
            // gradient runs from (0, ½) to (1, ½) and the others are centred on (½, ½).
            let inv = t.filter(|t| t.determinant().abs() > 1e-12).map_or(Affine::IDENTITY, Affine::inverse);
            let pts = if ty == "GRADIENT_LINEAR" { [(0.0, 0.5), (1.0, 0.5), (0.0, 1.0)] } else { [(0.5, 0.5), (1.0, 0.5), (0.5, 1.0)] };
            let handles: Vec<Value> = pts
                .iter()
                .map(|(x, y)| {
                    let q = inv * Point::new(*x, *y);
                    json!({"x": q.x, "y": q.y})
                })
                .collect();
            o.insert("gradientHandlePositions".into(), Value::Array(handles));
            let stops: Vec<Value> = p
                .get("stops")
                .map(Kv::list)
                .unwrap_or_default()
                .iter()
                .map(|s| json!({"position": num(s, "position").unwrap_or(0.0), "color": s.get("color").map_or(Value::Null, color)}))
                .collect();
            o.insert("gradientStops".into(), Value::Array(stops));
        }
        "IMAGE" => {
            let hash: String =
                p.get("image").and_then(|i| i.get("hash")).and_then(Kv::bytes).unwrap_or_default().iter().map(|b| format!("{b:02x}")).collect();
            o.insert("imageRef".into(), json!(hash));
            let mode = p.get("imageScaleMode").and_then(Kv::str).unwrap_or("FILL");
            o.insert("scaleMode".into(), json!(mode));
            if let (Some(t), "STRETCH") = (t, mode) {
                o.insert("imageTransform".into(), rows(t));
            }
            if let Some(s) = num(p, "scale") {
                o.insert("scalingFactor".into(), json!(s));
            }
            if let Some(r) = num(p, "rotation") {
                o.insert("rotation".into(), json!(r));
            }
        }
        _ => {}
    }
    Some(Value::Object(o))
}

/// The track ids of a grid's columns or rows, in order.
fn tracks(v: &View, key: &str) -> Vec<String> {
    let mut t: Vec<(&str, String)> = v
        .get(key)
        .and_then(|m| m.get("entries"))
        .map(Kv::list)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| Some((e.get("position").and_then(Kv::str).unwrap_or(""), e.get("id").and_then(guid)?)))
        .collect();
    t.sort();
    t.into_iter().map(|e| e.1).collect()
}

/// The variable an `ALIAS` variable data points at.
fn alias(d: Option<&Kv>) -> Option<String> {
    let d = d?;
    if d.get("dataType").and_then(Kv::str) != Some("ALIAS") {
        return None;
    }
    d.get("value").and_then(|v| v.get("alias")).and_then(ref_guid)
}

fn variable_value(d: &Kv) -> Option<Value> {
    if let Some(a) = alias(Some(d)) {
        return Some(json!({"type": "VARIABLE_ALIAS", "id": a}));
    }
    let v = d.get("value")?;
    if let Some(c) = v.get("colorValue") {
        return Some(color(c));
    }
    if let Some(b) = v.get("boolValue").and_then(Kv::bool) {
        return Some(json!(b));
    }
    if let Some(x) = num(v, "floatValue") {
        return Some(json!(x));
    }
    v.get("textValue").and_then(Kv::str).map(|s| json!(s))
}

/// The text of a component property value.
fn prop_text<'k>(v: &'k Kv) -> Option<&'k str> {
    v.get("textValue").and_then(|t| t.get("characters")).and_then(Kv::str)
}

/// The property a layer's `field` is bound to (`parameterConsumptionMap`).
fn prop_ref(v: &View, field: &str) -> Option<String> {
    v.get("parameterConsumptionMap")?.get("entries")?.list().iter().find_map(|e| {
        if e.get("variableField").and_then(Kv::str) != Some(field) {
            return None;
        }
        e.get("variableData")?.get("value")?.get("propRefValue")?.get("defId").and_then(guid)
    })
}

/// The REST TypeStyle fields a node (or text override entry) sets. `size`: the font size that
/// letter spacing in percent refers to when the entry doesn't set one.
fn text_style(v: &View, size: Option<f64>) -> Obj {
    let mut s = Obj::new();
    if let Some(f) = v.get("fontName") {
        for (from, to) in [("family", "fontFamily"), ("style", "fontStyle"), ("postscript", "fontPostScriptName")] {
            if let Some(x) = f.get(from).and_then(Kv::str).filter(|x| !x.is_empty()) {
                s.insert(to.into(), json!(x));
            }
        }
    }
    copy_num(&mut s, v, "fontSize", "fontSize");
    copy_num(&mut s, v, "paragraphSpacing", "paragraphSpacing");
    copy_num(&mut s, v, "paragraphIndent", "paragraphIndent");
    copy_str(&mut s, v, "textCase", "textCase");
    copy_str(&mut s, v, "textDecoration", "textDecoration");
    let font_size = v.num("fontSize").or(size).unwrap_or(12.0);
    if let Some(ls) = v.get("letterSpacing") {
        let val = num(ls, "value").unwrap_or(0.0);
        let px = match ls.get("units").and_then(Kv::str) {
            Some("PIXELS") => val,
            Some("PERCENT") => val / 100.0 * font_size,
            _ => val * font_size,
        };
        s.insert("letterSpacing".into(), json!(px));
    }
    if let Some(lh) = v.get("lineHeight") {
        let val = num(lh, "value").unwrap_or(100.0);
        match lh.get("units").and_then(Kv::str) {
            Some("PIXELS") => {
                s.insert("lineHeightUnit".into(), json!("PIXELS"));
                s.insert("lineHeightPx".into(), json!(val));
            }
            // shortcut: 100 % reads as the font's own line height (new text saves "auto" so) and
            // other percentages as % of the font size; revisit with files that set 100 % explicitly.
            Some("PERCENT") if (val - 100.0).abs() < 1e-6 => {
                s.insert("lineHeightUnit".into(), json!("INTRINSIC_%"));
            }
            Some("PERCENT") => {
                s.insert("lineHeightUnit".into(), json!("FONT_SIZE_%"));
                s.insert("lineHeightPercentFontSize".into(), json!(val));
            }
            _ => {
                s.insert("lineHeightUnit".into(), json!("FONT_SIZE_%"));
                s.insert("lineHeightPercentFontSize".into(), json!(val * 100.0));
            }
        }
    }
    if let Some(u) = v.get("hyperlink").and_then(|h| h.get("url")).and_then(Kv::str).filter(|u| !u.is_empty()) {
        s.insert("hyperlink".into(), json!({"type": "URL", "url": u}));
    }
    s
}

fn effect(e: &Kv) -> Option<Value> {
    let ty = match e.get("type").and_then(Kv::str)? {
        "FOREGROUND_BLUR" => "LAYER_BLUR",
        t => t,
    };
    let mut o = Obj::new();
    o.insert("type".into(), json!(ty));
    for k in ["radius", "spread", "startRadius", "density", "opacity"] {
        if let Some(x) = num(e, k) {
            o.insert(k.into(), json!(x));
        }
    }
    for k in ["visible", "showShadowBehindNode", "clipToShape"] {
        if let Some(b) = e.get(k).and_then(Kv::bool) {
            o.insert(k.into(), json!(b));
        }
    }
    for k in ["blendMode", "noiseType"] {
        if let Some(s) = e.get(k).and_then(Kv::str) {
            o.insert(k.into(), json!(s));
        }
    }
    for k in ["color", "secondaryColor"] {
        if let Some(c) = e.get(k) {
            o.insert(k.into(), color(c));
        }
    }
    for (from, to) in [("offset", "offset"), ("startOffset", "startOffset"), ("endOffset", "endOffset"), ("noiseSize", "noiseSizeVector")] {
        if let Some(p) = e.get(from).and_then(vec2) {
            o.insert(to.into(), p);
        }
    }
    if e.get("blurOpType").and_then(Kv::str) == Some("PROGRESSIVE") {
        o.insert("blurType".into(), json!("PROGRESSIVE"));
    }
    Some(Value::Object(o))
}

fn layout_grid(g: &Kv) -> Value {
    let pattern = match (g.get("pattern").and_then(Kv::str), g.get("axis").and_then(Kv::str)) {
        (Some("GRID"), _) => "GRID",
        (_, Some("Y")) => "ROWS",
        _ => "COLUMNS",
    };
    let mut o = json!({"pattern": pattern, "alignment": g.get("type").and_then(Kv::str).unwrap_or("STRETCH")});
    for (from, to) in [("sectionSize", "sectionSize"), ("numSections", "count"), ("gutterSize", "gutterSize"), ("offset", "offset")] {
        if let Some(x) = num(g, from) {
            o[to] = json!(x);
        }
    }
    if let Some(c) = g.get("color") {
        o["color"] = color(c);
    }
    if let Some(b) = g.get("visible").and_then(Kv::bool) {
        o["visible"] = json!(b);
    }
    o
}

fn export_setting(e: &Kv) -> Value {
    let c = e.get("constraint");
    let ty = match c.and_then(|c| c.get("type")).and_then(Kv::str) {
        Some("CONTENT_WIDTH") => "WIDTH",
        Some("CONTENT_HEIGHT") => "HEIGHT",
        _ => "SCALE",
    };
    json!({
        "format": e.get("imageType").and_then(Kv::str).unwrap_or("PNG"),
        "suffix": e.get("suffix").and_then(Kv::str).unwrap_or(""),
        "constraint": {"type": ty, "value": c.and_then(|c| num(c, "value")).unwrap_or(1.0)},
    })
}

/// A prototype action's transition (`None`: instant).
fn transition(a: &Kv) -> Option<Value> {
    let ty = a.get("transitionType").and_then(Kv::str)?;
    // Directions name where the incoming screen moves (SLIDE_FROM_LEFT moves right).
    let (kind, dir) = match ty {
        "INSTANT_TRANSITION" => return None,
        "DISSOLVE" | "FADE" => ("DISSOLVE", None),
        "SMART_ANIMATE" | "MAGIC_MOVE" => ("SMART_ANIMATE", None),
        "SCROLL_ANIMATE" => ("SCROLL_ANIMATE", None),
        t => {
            let prefixes = [
                ("SLIDE_FROM_", "SLIDE_IN"),
                ("PUSH_FROM_", "PUSH"),
                ("MOVE_FROM_", "MOVE_IN"),
                ("SLIDE_OUT_TO_", "SLIDE_OUT"),
                ("MOVE_OUT_TO_", "MOVE_OUT"),
            ];
            let (kind, side) = prefixes.iter().find_map(|(p, k)| t.strip_prefix(p).map(|r| (*k, r)))?;
            let incoming = matches!(kind, "SLIDE_IN" | "PUSH" | "MOVE_IN");
            let dir = match (side, incoming) {
                ("LEFT", true) | ("RIGHT", false) => "RIGHT",
                ("RIGHT", true) | ("LEFT", false) => "LEFT",
                ("TOP", true) | ("BOTTOM", false) => "BOTTOM",
                _ => "TOP",
            };
            (kind, Some(dir))
        }
    };
    let easing = match a.get("easingType").and_then(Kv::str) {
        Some("IN_CUBIC" | "EASE_IN") => "EASE_IN",
        Some("INOUT_CUBIC") => "EASE_IN_AND_OUT",
        Some("LINEAR") => "LINEAR",
        Some("IN_BACK_CUBIC") => "EASE_IN_BACK",
        Some("OUT_BACK_CUBIC") => "EASE_OUT_BACK",
        Some("INOUT_BACK_CUBIC") => "EASE_IN_AND_OUT_BACK",
        Some("GENTLE_SPRING") => "GENTLE",
        Some("CUSTOM_CUBIC") => "CUSTOM_CUBIC_BEZIER",
        _ => "EASE_OUT",
    };
    let mut e = json!({"type": easing});
    let f: Vec<f64> = a.get("easingFunction").map(Kv::list).unwrap_or_default().iter().filter_map(Kv::f64).collect();
    if let (true, [x1, y1, x2, y2, ..]) = (easing == "CUSTOM_CUBIC_BEZIER", f.as_slice()) {
        e["easingFunctionCubicBezier"] = json!({"x1": x1, "y1": y1, "x2": x2, "y2": y2});
    }
    let mut t = json!({"type": kind, "duration": num(a, "transitionDuration").unwrap_or(0.3) * 1000.0, "easing": e});
    if let Some(d) = dir {
        t["direction"] = json!(d);
    }
    Some(t)
}

/// SVG path data of a geometry blob: commands `0` close, `1` move, `2` line, `3` quadratic and
/// `4` cubic, each followed by its points as little-endian `f32` pairs. `None` when damaged.
fn svg_path(b: &[u8]) -> Option<String> {
    let mut out = String::new();
    let mut i = 0usize;
    let mut n = 0usize;
    while i < b.len() {
        n += 1;
        if n > MAX_COMMANDS {
            return None;
        }
        let (letter, points) = match *b.get(i)? {
            0 => ('Z', 0),
            1 => ('M', 1),
            2 => ('L', 1),
            3 => ('Q', 2),
            4 => ('C', 3),
            _ => return None,
        };
        i += 1;
        if !out.is_empty() {
            out.push(' ');
        }
        out.push(letter);
        for _ in 0..points * 2 {
            let bytes: [u8; 4] = b.get(i..i + 4)?.try_into().ok()?;
            let x = f32::from_le_bytes(bytes);
            if !x.is_finite() {
                return None;
            }
            out.push_str(&format!(" {x}"));
            i += 4;
        }
    }
    Some(out)
}
