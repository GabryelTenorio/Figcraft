//! Canvas rendering off the UI thread (desktop) or inline (web). The latest request wins (queued
//! older ones are skipped); the UI shows each finished image, re-projected to the current view,
//! until the next one arrives.
//! The renderer keeps its last image ([`figcraft_render::Retained`]): a pan renders only the
//! uncovered strips and an edit only the areas of the layers that changed.

use std::sync::Arc;

use figcraft_color::ColorProfile;
use figcraft_doc::{Document, NodeId};
use figcraft_render::{Pixmap, Retained, View, Work};

#[derive(Clone)]
pub struct Job {
    pub doc: Arc<Document>,
    pub page: NodeId,
    pub view: View,
    pub outline: bool,
    /// Layers drawn as outlines (outlines for the selection).
    pub outlined: Vec<NodeId>,
    pub skip: Vec<NodeId>,
    /// Only this layer (the focus view).
    pub solo: Option<NodeId>,
}

pub struct Done {
    /// The image, ready for a texture (converted off the UI thread).
    pub image: egui::ColorImage,
    /// The view the image shows (the requested one snapped outwards to whole device pixels).
    pub view: View,
    pub ms: f64,
    pub work: Work,
}

/// A render of a document in `profile` as a texture image for the screen (sRGB).
pub fn screen_image(pix: &Pixmap, profile: ColorProfile) -> egui::ColorImage {
    let size = [pix.width() as usize, pix.height() as usize];
    if profile.is_srgb() {
        return egui::ColorImage::from_rgba_premultiplied(size, pix.data());
    }
    let mut data = pix.data().to_vec();
    profile.conversion_to(ColorProfile::Srgb).apply_pixels(&mut data);
    egui::ColorImage::from_rgba_premultiplied(size, &data)
}

fn run(job: &Job, retained: &mut Retained) -> Option<Done> {
    #[cfg(not(target_arch = "wasm32"))]
    let t0 = std::time::Instant::now();
    // The canvas is a preview: what's too small to see in detail at this zoom is approximated.
    let opts = figcraft_render::Options {
        outline_mode: job.outline,
        outlined: job.outlined.clone(),
        skip: job.skip.clone(),
        solo: job.solo,
        lod: true,
        ..Default::default()
    };
    let (view, pix, work) = retained.render(&job.doc, job.page, &job.view, &opts)?;
    let image = screen_image(pix, job.doc.color_profile);
    #[cfg(not(target_arch = "wasm32"))]
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    #[cfg(target_arch = "wasm32")]
    let ms = 0.0;
    Some(Done { image, view, ms, work })
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Worker {
    tx: std::sync::mpsc::Sender<Job>,
    rx: std::sync::mpsc::Receiver<Done>,
    pub in_flight: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Worker {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, jobs) = std::sync::mpsc::channel::<Job>();
        let (done_tx, rx) = std::sync::mpsc::channel::<Done>();
        let spawned = std::thread::Builder::new().name("figcraft-render".into()).spawn(move || {
            let mut retained = Retained::default();
            while let Ok(mut job) = jobs.recv() {
                // Skip to the newest queued job.
                while let Ok(newer) = jobs.try_recv() {
                    job = newer;
                }
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&job, &mut retained)));
                let done = match res {
                    Ok(d) => d,
                    Err(_) => {
                        // Don't build on an image a panic may have left half drawn.
                        retained.clear();
                        None
                    }
                };
                if let Some(d) = done
                    && done_tx.send(d).is_err()
                {
                    break;
                }
                ctx.request_repaint();
            }
        });
        if let Err(e) = spawned {
            log::error!("render thread: {e}");
        }
        Self { tx, rx, in_flight: false }
    }

    pub fn submit(&mut self, job: Job) {
        if self.tx.send(job).is_ok() {
            self.in_flight = true;
        }
    }

    pub fn poll(&mut self) -> Option<Done> {
        let mut last = None;
        while let Ok(d) = self.rx.try_recv() {
            last = Some(d);
        }
        if last.is_some() {
            self.in_flight = false;
        }
        last
    }
}

#[cfg(target_arch = "wasm32")]
pub struct Worker {
    pending: Option<Job>,
    retained: Retained,
    pub in_flight: bool,
}

#[cfg(target_arch = "wasm32")]
impl Worker {
    pub fn new(_ctx: egui::Context) -> Self {
        Self { pending: None, retained: Retained::default(), in_flight: false }
    }
    pub fn submit(&mut self, job: Job) {
        self.pending = Some(job);
        self.in_flight = true;
    }
    pub fn poll(&mut self) -> Option<Done> {
        let job = self.pending.take()?;
        self.in_flight = false;
        run(&job, &mut self.retained)
    }
}

/// Render synchronously from scratch (tests, screenshots).
pub fn render_now(job: &Job) -> Option<Done> {
    run(job, &mut Retained::default())
}
