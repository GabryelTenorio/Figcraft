//! `figcraft-cli perf`: synthetic large documents and timings of the hot paths.
//!
//! The documents come from [`figcraft_engine::synthetic`] (screens with an auto layout header, a
//! card group, button instances and a grid of shapes and text). Every row is the mean of several
//! runs; use a release build for meaningful numbers (`cargo xtask bench`).

use std::process::ExitCode;
use std::time::Instant;

use figcraft_doc::geom::Point;
use figcraft_doc::{Document, NodeId, NodeKind};
use figcraft_engine::{PointerEvent, PointerKind, Session, ViewState};
use serde_json::json;

use crate::{CliError, CliResult};

/// Canvas size of the measured viewport (a typical laptop window).
const VIEW_W: u32 = 1440;
const VIEW_H: u32 = 900;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Mean milliseconds of `n` runs of `f`.
fn mean(n: usize, mut f: impl FnMut(usize)) -> f64 {
    let t = Instant::now();
    for i in 0..n {
        f(i);
    }
    ms(t) / n.max(1) as f64
}

/// Set when a measured command fails (the numbers would be meaningless).
static FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn exec(s: &mut Session, id: &str, p: serde_json::Value) {
    if let Err(e) = s.execute(id, &p) {
        eprintln!("{id}: {e}");
        FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn failed(e: impl std::fmt::Display) -> CliError {
    CliError::Failed(e.to_string())
}

/// The `k`-th layer of `kind` among the descendants of `root` (pre-order).
fn find(doc: &Document, root: NodeId, k: usize, pred: impl Fn(&figcraft_doc::Node) -> bool) -> Option<NodeId> {
    doc.subtree(root).into_iter().filter(|i| doc.node(*i).is_some_and(&pred)).nth(k)
}

/// What the canvas renders with: a preview, with levels of detail when zoomed out.
fn canvas() -> figcraft_render::Options {
    figcraft_render::Options { lod: true, ..Default::default() }
}

fn render(doc: &Document, page: NodeId, v: &ViewState, opts: &figcraft_render::Options) -> bool {
    let (ox, oy) = v.origin();
    let view = figcraft_render::View { origin: (ox, oy), scale: v.zoom, width: VIEW_W, height: VIEW_H };
    figcraft_render::render_view(doc, page, &view, opts).is_some()
}

/// Render `v` panned right by `dx` page units through the retained renderer, as the canvas does.
fn retained(r: &mut figcraft_render::Retained, doc: &std::sync::Arc<Document>, page: NodeId, v: &ViewState, dx: f64) -> bool {
    let (ox, oy) = v.origin();
    let view = figcraft_render::View { origin: (ox + dx, oy), scale: v.zoom, width: VIEW_W, height: VIEW_H };
    r.render(doc, page, &view, &canvas()).is_some()
}

struct Table {
    rows: Vec<(String, Vec<f64>)>,
}

impl Table {
    fn put(&mut self, col: usize, name: &str, v: f64) {
        let i = match self.rows.iter().position(|r| r.0 == name) {
            Some(i) => i,
            None => {
                self.rows.push((name.to_string(), Vec::new()));
                self.rows.len() - 1
            }
        };
        if let Some(r) = self.rows.get_mut(i) {
            r.1.resize(col + 1, f64::NAN);
            if let Some(c) = r.1.get_mut(col) {
                *c = v;
            }
        }
    }
}

/// Measure one document size into column `col`.
fn measure(table: &mut Table, col: usize, layers: usize) -> CliResult<()> {
    let t = Instant::now();
    let (doc, page) = figcraft_engine::synthetic::large_document(layers).map_err(failed)?;
    table.put(col, "build + normalize (like open)", ms(t));

    let t = Instant::now();
    let bytes = figcraft_format::save_bytes(&doc, None).map_err(failed)?;
    table.put(col, "save .figcraft", ms(t));
    let t = Instant::now();
    let (back, _) = figcraft_format::load_bytes(&bytes).map_err(failed)?;
    table.put(col, "load .figcraft", ms(t));
    drop(back);

    // Rendering a 1440×900 viewport as the canvas does: 100% on the first screen, and zoomed out to
    // fit the page (also exactly, as an export of the same area would).
    let bounds = doc.page_content_bounds(page).ok_or_else(|| failed("empty page"))?;
    let mut at100 = ViewState { vw: f64::from(VIEW_W), vh: f64::from(VIEW_H), ..Default::default() };
    at100.cx = f64::from(VIEW_W) / 2.0;
    at100.cy = f64::from(VIEW_H) / 2.0;
    table.put(col, "render 1440×900 @100%", mean(5, |_| _ = render(&doc, page, &at100, &canvas())));
    let mut fit = at100.clone();
    fit.fit(bounds, 1.0);
    table.put(col, "render 1440×900 zoomed to fit", mean(5, |_| _ = render(&doc, page, &fit, &canvas())));
    table.put(col, "render 1440×900 zoomed to fit, exact (no LOD)", mean(3, |_| _ = render(&doc, page, &fit, &Default::default())));

    // The canvas keeps its last image: a pan renders the uncovered strips, an edit the layers'
    // areas.
    let doc = std::sync::Arc::new(doc);
    let pans = 10;
    // Zoomed out, but with content right up to the edges (a fitted page has margins).
    let mut out = fit.clone();
    out.zoom *= 2.0;
    table.put(col, "render 1440×900 zoomed out (2× fit)", mean(5, |_| _ = render(&doc, page, &out, &canvas())));
    for (name, v) in [("pan 40 px @100% (retained)", &at100), ("pan 40 px zoomed out (retained)", &out)] {
        let mut r = figcraft_render::Retained::default();
        let _ = retained(&mut r, &doc, page, v, 0.0);
        table.put(col, name, mean(pans, |i| _ = retained(&mut r, &doc, page, v, if i % 2 == 0 { 40.0 / v.zoom } else { 0.0 })));
    }
    for (name, v) in [("move one layer, re-render @100% (retained)", &at100), ("move one layer, re-render zoomed out (retained)", &out)] {
        // A shape in the screen nearest the view's centre.
        let centre = Point::new(v.cx, v.cy);
        let screens = doc.node(page).map(|p| p.children.clone()).unwrap_or_default();
        let nearest =
            screens.into_iter().filter_map(|c| doc.world_bounds(c).map(|b| (c, (b.center() - centre).hypot2()))).min_by(|a, b| a.1.total_cmp(&b.1));
        let target = nearest.and_then(|(c, _)| doc.node(c)?.children.last().copied()).ok_or_else(|| failed("no screen to edit"))?;
        let moved: Vec<std::sync::Arc<Document>> = (0..2)
            .map(|k| {
                let mut d = (*doc).clone();
                if let Some(n) = d.node_mut(target) {
                    n.transform = figcraft_doc::geom::Affine::translate((f64::from(k) * 6.0, 0.0)) * n.transform;
                }
                std::sync::Arc::new(d)
            })
            .collect();
        let mut r = figcraft_render::Retained::default();
        let _ = retained(&mut r, &doc, page, v, 0.0);
        table.put(col, name, mean(10, |i| _ = moved.get(i % 2).map(|d| retained(&mut r, d, page, v, 0.0))));
    }
    let doc = std::sync::Arc::unwrap_or_clone(doc);

    // Hit testing (a hover under the pointer).
    let hits = 2000;
    let pt = |i: usize| {
        let (fx, fy) = ((i * 7919 % 1000) as f64 / 1000.0, (i * 104_729 % 1000) as f64 / 1000.0);
        Point::new(bounds.x0 + bounds.width() * fx, bounds.y0 + bounds.height() * fy)
    };
    table.put(col, "hit test (µs each)", mean(hits, |i| _ = doc.hit_test(page, pt(i), 1.0)) * 1000.0);

    let mut s = Session::with_document(doc);
    let doc = s.doc().map_err(failed)?.clone();
    let rect =
        find(&doc, page, layers / 2 / 50, |n| matches!(n.kind, NodeKind::Rectangle) && n.name == "Rectangle").ok_or_else(|| failed("no rect"))?;
    let title = find(&doc, page, 0, |n| n.name == "Title").ok_or_else(|| failed("no title"))?;
    let button = find(&doc, page, 0, |n| matches!(n.kind, NodeKind::Component { .. })).ok_or_else(|| failed("no component"))?;
    let button_bg = doc.node(button).and_then(|b| b.children.first().copied()).ok_or_else(|| failed("no button layers"))?;

    // Hover with the Move tool: hit test + select target, as the canvas does per pointer move.
    let zoom = at100.zoom;
    table.put(
        col,
        "hover pointer move (µs each)",
        mean(hits, |i| {
            let p = pt(i);
            let ev =
                PointerEvent { kind: PointerKind::Hover, x: p.x, y: p.y, sx: 0.0, sy: 0.0, shift: false, alt: false, cmd: false, ctrl: false, zoom };
            let _ = s.pointer(&ev);
        }) * 1000.0,
    );

    // Single-layer edits through the engine (command + normalize + undo step).
    let colors = ["#FF0000", "#00FF00"];
    table.put(col, "node.set fill on one layer", mean(20, |i| exec(&mut s, "node.set", json!({"ids": [rect], "fill": colors[i % 2]}))));
    exec(&mut s, "select.set", json!({"ids": [rect]}));
    table.put(col, "transform.move one layer", mean(20, |i| exec(&mut s, "transform.move", json!({"dx": if i % 2 == 0 { 1 } else { -1 }, "dy": 0}))));
    table.put(
        col,
        "edit text in an auto layout frame",
        mean(20, |i| exec(&mut s, "node.set", json!({"ids": [title], "characters": format!("Title {i}")}))),
    );
    table.put(
        col,
        "edit main component (all instances follow)",
        mean(5, |i| exec(&mut s, "node.set", json!({"ids": [button_bg], "fill": colors[i % 2]}))),
    );
    table.put(col, "undo", mean(10, |_| _ = s.undo()));
    table.put(col, "redo", mean(10, |_| _ = s.redo()));

    // A drag: every pointer move re-applies the move to the gesture's snapshot.
    exec(&mut s, "select.set", json!({"ids": [rect]}));
    let _ = s.begin_interaction("Move");
    table.put(
        col,
        "drag preview frame (one layer)",
        mean(20, |i| {
            let _ = s.preview(|d, sel| {
                for id in sel.iter() {
                    if let Some(n) = d.node_mut(*id) {
                        n.transform = figcraft_doc::geom::Affine::translate((i as f64, 0.0)) * n.transform;
                    }
                }
                Ok(())
            });
        }),
    );
    let _ = s.commit_interaction();

    // Raw normalisation after a one-node edit (no command machinery).
    let before = s.doc().map_err(failed)?.clone();
    table.put(
        col,
        "normalize after a one-node edit",
        mean(50, |i| {
            let mut d = before.clone();
            if let Some(n) = d.node_mut(rect) {
                n.opacity = if i % 2 == 0 { 0.5 } else { 1.0 };
            }
            figcraft_engine::normalize(&mut d, &before);
        }),
    );

    table.put(col, "document.inspect (depth 8)", mean(3, |_| _ = figcraft_engine::inspect::document(&s, 8, false)));
    exec(&mut s, "select.none", json!({}));
    let t = Instant::now();
    exec(&mut s, "select.all", json!({}));
    exec(&mut s, "transform.move", json!({"dx": 10, "dy": 0}));
    table.put(col, "select all + move every screen", ms(t));
    Ok(())
}

/// Time the hot paths for each document size in `sizes` (layers) and print a table.
pub fn run(sizes: &[usize]) -> CliResult<ExitCode> {
    // Text renders with system fonts too, like the desktop app.
    figcraft_text::scan_system_fonts();
    let mut table = Table { rows: Vec::new() };
    for (col, n) in sizes.iter().enumerate() {
        eprintln!("measuring {n} layers…");
        measure(&mut table, col, *n)?;
    }
    if FAILED.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(failed("a measured command failed"));
    }
    println!(
        "figcraft perf ({} {}; ms unless noted; mean of several runs)",
        std::env::consts::OS,
        if cfg!(debug_assertions) { "debug build" } else { "release build" }
    );
    let w = table.rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(10);
    let mut head = format!("| {:<w$} |", "operation");
    let mut rule = format!("|{}|", "-".repeat(w + 2));
    for n in sizes {
        head.push_str(&format!(" {:>10} |", format!("{n} layers")));
        rule.push_str(&format!("{}:|", "-".repeat(11)));
    }
    println!("{head}\n{rule}");
    for (name, vals) in &table.rows {
        let mut line = format!("| {name:<w$} |");
        for v in vals {
            line.push_str(&format!(" {:>10} |", if v.is_finite() { format!("{v:.2}") } else { "-".into() }));
        }
        println!("{line}");
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    #[test]
    fn layer_sizes_parse_as_a_list() {
        use crate::args::Args;
        let argv = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let a = Args::parse(&argv(&["--layers", "1000,10_000, 50000"]), &["layers"], &[]).unwrap();
        assert_eq!(a.usizes("layers", &[1]).unwrap(), vec![1000, 10_000, 50_000]);
        assert_eq!(Args::parse(&[], &["layers"], &[]).unwrap().usizes("layers", &[7]).unwrap(), vec![7]);
        let bad = Args::parse(&argv(&["--layers", "10,x"]), &["layers"], &[]).unwrap();
        assert!(bad.usizes("layers", &[1]).is_err());
    }

    #[test]
    fn measures_a_small_document() {
        let mut t = super::Table { rows: Vec::new() };
        super::measure(&mut t, 0, 300).unwrap();
        assert!(t.rows.len() > 10);
        assert!(t.rows.iter().all(|r| r.1.len() == 1 && r.1[0].is_finite()), "{:?}", t.rows);
    }
}
