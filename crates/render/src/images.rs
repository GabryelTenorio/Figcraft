//! Decoded image cache (content hash + filters + colour profile → premultiplied pixmap). Images
//! are taken to be sRGB; a Display P3 document gets them converted into its space.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use figcraft_color::{ColorProfile, ImageFilters};
use figcraft_doc::ImageData;
use tiny_skia::Pixmap;

type Key = (String, [i32; 7], ColorProfile);

fn cache() -> &'static Mutex<HashMap<Key, Arc<Pixmap>>> {
    static C: OnceLock<Mutex<HashMap<Key, Arc<Pixmap>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fkey(f: &ImageFilters) -> [i32; 7] {
    let q = |v: f32| (v * 1000.0).round() as i32;
    [q(f.exposure), q(f.contrast), q(f.saturation), q(f.temperature), q(f.tint), q(f.highlights), q(f.shadows)]
}

/// Decode encoded image bytes into a premultiplied pixmap (max 16k per side).
pub fn decode(bytes: &[u8]) -> Option<Pixmap> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 || w > 16_384 || h > 16_384 {
        return None;
    }
    let mut p = Pixmap::new(w, h)?;
    for (dst, src) in p.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(rgba.as_raw().chunks_exact(4)) {
        let a = u32::from(src.get(3).copied().unwrap_or(255));
        let pm = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
        dst.copy_from_slice(&[
            pm(src.first().copied().unwrap_or(0)),
            pm(src.get(1).copied().unwrap_or(0)),
            pm(src.get(2).copied().unwrap_or(0)),
            a as u8,
        ]);
    }
    Some(p)
}

/// The decoded (and filtered) image for `hash` in the colour space `profile`, cached.
pub fn get(hash: &str, data: Option<&ImageData>, filters: &ImageFilters, profile: ColorProfile) -> Option<Arc<Pixmap>> {
    let key = (hash.to_string(), fkey(filters), profile);
    if let Some(p) = cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
        return Some(p.clone());
    }
    let data = data?;
    let mut p = decode(&data.bytes)?;
    if !filters.is_identity() {
        apply_filters(&mut p, filters);
    }
    ColorProfile::Srgb.conversion_to(profile).apply_pixels(p.data_mut());
    let p = Arc::new(p);
    let mut c = cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if c.len() > 256 {
        c.clear();
    }
    c.insert(key, p.clone());
    Some(p)
}

/// Image adjustments (approximations of exposure, contrast, saturation, temperature, tint,
/// highlights and shadows) on a premultiplied pixmap.
pub fn apply_filters(p: &mut Pixmap, f: &ImageFilters) {
    let exposure = 2f32.powf(f.exposure.clamp(-1.0, 1.0) * 2.0);
    let contrast = 1.0 + f.contrast.clamp(-1.0, 1.0);
    let sat = 1.0 + f.saturation.clamp(-1.0, 1.0);
    let temp = f.temperature.clamp(-1.0, 1.0) * 0.15;
    let tint = f.tint.clamp(-1.0, 1.0) * 0.15;
    let hi = f.highlights.clamp(-1.0, 1.0) * 0.3;
    let sh = f.shadows.clamp(-1.0, 1.0) * 0.3;
    for px in p.data_mut().as_chunks_mut::<4>().0 {
        let a = f32::from(px.get(3).copied().unwrap_or(0)) / 255.0;
        if a <= 0.0 {
            continue;
        }
        let un = |i: usize| f32::from(px.get(i).copied().unwrap_or(0)) / 255.0 / a;
        let (mut r, mut g, mut b) = (un(0), un(1), un(2));
        r *= exposure;
        g *= exposure;
        b *= exposure;
        r = (r - 0.5) * contrast + 0.5;
        g = (g - 0.5) * contrast + 0.5;
        b = (b - 0.5) * contrast + 0.5;
        let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        r = l + (r - l) * sat;
        g = l + (g - l) * sat;
        b = l + (b - l) * sat;
        r += temp;
        b -= temp;
        g += tint;
        let l = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);
        let adj = hi * l * l + sh * (1.0 - l) * (1.0 - l);
        r += adj;
        g += adj;
        b += adj;
        let q = |c: f32| (c.clamp(0.0, 1.0) * a * 255.0).round() as u8;
        let (qr, qg, qb) = (q(r), q(g), q(b));
        if let Some(s) = px.get_mut(0..3) {
            s.copy_from_slice(&[qr, qg, qb]);
        }
    }
}

/// Read an image's pixel size without keeping the decode.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    reader.into_dimensions().ok()
}

fn padded_cache() -> &'static Mutex<HashMap<Key, Arc<Pixmap>>> {
    static C: OnceLock<Mutex<HashMap<Key, Arc<Pixmap>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `pm` with a transparent 1-pixel border (cached).
pub fn padded(hash: &str, filters: &ImageFilters, profile: ColorProfile, pm: &Pixmap) -> Option<Arc<Pixmap>> {
    let key = (hash.to_string(), fkey(filters), profile);
    if let Some(p) = padded_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
        return Some(p.clone());
    }
    let mut out = Pixmap::new(pm.width() + 2, pm.height() + 2)?;
    out.draw_pixmap(1, 1, pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
    let out = Arc::new(out);
    let mut c = padded_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if c.len() > 256 {
        c.clear();
    }
    c.insert(key, out.clone());
    Some(out)
}
