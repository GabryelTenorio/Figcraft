//! Canvas fixes, driven through egui headless: View ▸ Frame outlines and View ▸ Slices draw (off
//! by default), and the render worker counts a job as in flight until it is done.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use egui::{Pos2, Shape};
use figcraft_engine::Session;
use serde_json::json;

use crate::layers_drag_tests::Ui;
use crate::live_tests::flat;
use crate::present_tests::id;

/// Whether `shapes` hold a closed outline through the corners `q`.
fn outlined(shapes: &[Shape], q: [Pos2; 4]) -> bool {
    shapes.iter().any(|s| matches!(s, Shape::Path(p) if p.closed && p.points.len() == 4 && p.points.iter().zip(q).all(|(a, b)| a.distance(b) < 0.5)))
}

/// How many line segments lie along the top edge from `a` to `b` (a dashed outline's dashes).
fn dashes_along(shapes: &[Shape], a: Pos2, b: Pos2) -> usize {
    let on = |p: Pos2| (p.y - a.y).abs() < 0.5 && p.x >= a.x - 0.5 && p.x <= b.x + 0.5;
    shapes.iter().filter(|s| matches!(s, Shape::LineSegment { points: [p, q], .. } if on(*p) && on(*q))).count()
}

#[test]
fn frame_outlines_and_slices_draw_when_shown() {
    let mut s = Session::new();
    let f = id(&s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 200, "height": 120})).unwrap());
    let inner = id(&s.execute("frame.create", &json!({"x": 20, "y": 20, "width": 80, "height": 60, "parent": f})).unwrap());
    let slice = id(&s.execute("node.create", &json!({"type": "slice", "x": 300, "y": 0, "width": 100, "height": 50})).unwrap());
    s.execute("select.set", &json!({"ids": []})).unwrap();
    let mut u = Ui::new(s);
    let quad = |u: &Ui, i| crate::canvas::node_quad(&u.app, u.app.session.doc().unwrap(), i).unwrap();
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let sq = quad(&u, slice);
    assert!(!outlined(&shapes, quad(&u, inner)), "off by default");
    assert_eq!(dashes_along(&shapes, sq[0], sq[1]), 0, "slices hidden by default");
    u.app.run("view.frameOutlines", json!({}));
    u.app.run("view.slices", json!({}));
    let shapes = flat(&u.frame_output(vec![]).shapes);
    for frame in [f, inner] {
        assert!(outlined(&shapes, quad(&u, frame)), "every frame is outlined");
    }
    assert!(dashes_along(&shapes, sq[0], sq[1]) > 3, "the slice shows as a dashed rectangle");
    u.app.run("view.frameOutlines", json!({"on": false}));
    u.app.run("view.slices", json!({"on": false}));
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(!outlined(&shapes, quad(&u, inner)) && dashes_along(&shapes, sq[0], sq[1]) == 0, "both turn off again");
}

/// A job a few hundred milliseconds of rendering big.
fn big_job(doc: &Arc<figcraft_doc::Document>, page: figcraft_doc::NodeId, scale: f64) -> crate::render_worker::Job {
    crate::render_worker::Job {
        doc: doc.clone(),
        page,
        view: figcraft_render::View { origin: (0.0, 0.0), scale, width: 2400, height: 2400 },
        outline: false,
        outline_extras: Default::default(),
        outlined: vec![],
        skip: vec![],
        solo: None,
        proof: false,
    }
}

#[test]
fn a_job_queued_behind_a_finished_one_stays_in_flight() {
    let ctx = egui::Context::default();
    let wakes = Arc::new(AtomicUsize::new(0));
    let w = wakes.clone();
    ctx.set_request_repaint_callback(move |_| {
        w.fetch_add(1, Ordering::SeqCst);
    });
    let mut s = Session::new();
    for k in 0..40 {
        s.execute("shape.ellipse", &json!({"x": k * 50, "y": k * 50, "width": 400, "height": 400})).unwrap();
    }
    let ds = s.ds().unwrap();
    let mut worker = crate::render_worker::Worker::new(ctx.clone());
    worker.submit(big_job(&ds.doc, ds.page, 1.0));
    let start = Instant::now();
    while wakes.load(Ordering::SeqCst) == 0 {
        assert!(start.elapsed() < Duration::from_secs(60), "the first render never finished");
        std::thread::sleep(Duration::from_millis(2));
    }
    // As in a canvas frame: a new job goes out, then the finished one is picked up.
    worker.submit(big_job(&ds.doc, ds.page, 0.9));
    assert!(worker.poll().is_some(), "the first image");
    // Not in flight only once the second is done: its image is then waiting.
    assert!(worker.in_flight() || worker.poll().is_some(), "the second job counts until its image is there");
}
