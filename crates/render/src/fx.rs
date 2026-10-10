//! Raster effects on premultiplied RGBA8 pixmaps: blur (3× box ≈ Gaussian), dilate/erode (spread),
//! alpha extraction and tinting.

use tiny_skia::Pixmap;

/// Box radii for three passes approximating a Gaussian of `sigma` (Kovesi).
fn box_sizes(sigma: f64) -> [usize; 3] {
    let n = 3.0;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - n * (wl * wl) as f64 - 4.0 * n * wl as f64 - 3.0 * n) / (-4.0 * wl as f64 - 4.0);
    let m = m_ideal.round() as i64;
    let r = |i: i64| -> usize {
        let w = if i < m { wl } else { wu };
        ((w.max(1) - 1) / 2) as usize
    };
    [r(0), r(1), r(2)]
}

fn box_blur_h(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let div = (2 * r + 1) as u32;
    for y in 0..h {
        let row = y * w * 4;
        let mut acc = [0u32; 4];
        // Edge: transparent outside.
        for x in 0..=r.min(w.saturating_sub(1)) {
            for c in 0..4 {
                acc[c] += u32::from(src.get(row + x * 4 + c).copied().unwrap_or(0));
            }
        }
        for x in 0..w {
            for c in 0..4 {
                if let Some(d) = dst.get_mut(row + x * 4 + c) {
                    *d = (acc[c] / div) as u8;
                }
            }
            let add = x + r + 1;
            if add < w {
                for c in 0..4 {
                    acc[c] += u32::from(src.get(row + add * 4 + c).copied().unwrap_or(0));
                }
            }
            if x >= r {
                let sub = x - r;
                for c in 0..4 {
                    acc[c] = acc[c].saturating_sub(u32::from(src.get(row + sub * 4 + c).copied().unwrap_or(0)));
                }
            }
        }
    }
}

fn transpose(src: &[u8], dst: &mut [u8], w: usize, h: usize) {
    for y in 0..h {
        for x in 0..w {
            let s = (y * w + x) * 4;
            let d = (x * h + y) * 4;
            if let (Some(sp), Some(dp)) = (src.get(s..s + 4), dst.get_mut(d..d + 4)) {
                dp.copy_from_slice(sp);
            }
        }
    }
}

/// Gaussian-like blur of the whole pixmap with standard deviation `sigma` (pixels).
pub fn blur(p: &mut Pixmap, sigma: f64) {
    if !(sigma.is_finite() && sigma > 0.3) {
        return;
    }
    let sigma = sigma.min(500.0);
    let (w, h) = (p.width() as usize, p.height() as usize);
    let radii = box_sizes(sigma);
    let mut a = p.data().to_vec();
    let mut b = vec![0u8; a.len()];
    for r in radii {
        box_blur_h(&a, &mut b, w, h, r);
        std::mem::swap(&mut a, &mut b);
    }
    transpose(&a, &mut b, w, h);
    std::mem::swap(&mut a, &mut b);
    for r in radii {
        box_blur_h(&a, &mut b, h, w, r);
        std::mem::swap(&mut a, &mut b);
    }
    transpose(&a, &mut b, h, w);
    p.data_mut().copy_from_slice(&b);
}

/// Grow (`r > 0`) or shrink (`r < 0`) the alpha of a pixmap by `r` pixels (square structuring
/// element, separable max/min). Colour channels follow alpha (tinted afterwards anyway).
pub fn morph(p: &mut Pixmap, r: i32) {
    if r == 0 {
        return;
    }
    let grow = r > 0;
    let r = r.unsigned_abs().min(400) as usize;
    let (w, h) = (p.width() as usize, p.height() as usize);
    let alpha: Vec<u8> = p.data().as_chunks::<4>().0.iter().map(|c| c.get(3).copied().unwrap_or(0)).collect();
    let pass = |src: &[u8], w: usize, h: usize, horizontal: bool| -> Vec<u8> {
        let mut out = vec![0u8; src.len()];
        for y in 0..h {
            for x in 0..w {
                let mut v: u8 = if grow { 0 } else { 255 };
                let (lo, hi) = (x.saturating_sub(r), (x + r).min(w.saturating_sub(1)));
                for k in lo..=hi {
                    let idx = if horizontal { y * w + k } else { k * h + y };
                    let s = src.get(idx).copied().unwrap_or(0);
                    v = if grow { v.max(s) } else { v.min(s) };
                }
                // Shrinking treats outside as empty.
                if !grow && (x < r || x + r >= w) {
                    v = 0;
                }
                let o = if horizontal { y * w + x } else { x * h + y };
                if let Some(d) = out.get_mut(o) {
                    *d = v;
                }
            }
        }
        out
    };
    let a1 = pass(&alpha, w, h, true);
    // Vertical pass: treat columns as rows by swapping roles.
    let mut a2 = vec![0u8; a1.len()];
    for x in 0..w {
        for y in 0..h {
            let mut v: u8 = if grow { 0 } else { 255 };
            let (lo, hi) = (y.saturating_sub(r), (y + r).min(h.saturating_sub(1)));
            for k in lo..=hi {
                let s = a1.get(k * w + x).copied().unwrap_or(0);
                v = if grow { v.max(s) } else { v.min(s) };
            }
            if !grow && (y < r || y + r >= h) {
                v = 0;
            }
            if let Some(d) = a2.get_mut(y * w + x) {
                *d = v;
            }
        }
    }
    for (px, a) in p.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(a2) {
        px.copy_from_slice(&[a, a, a, a]);
    }
}

/// Replace every pixel with `color` (non-premultiplied RGBA 0..1) at the pixel's alpha × color alpha.
pub fn tint(p: &mut Pixmap, color: [f32; 4]) {
    let [r, g, b, a] = color;
    for px in p.data_mut().as_chunks_mut::<4>().0 {
        let alpha = f32::from(px.get(3).copied().unwrap_or(0)) / 255.0 * a;
        let q = |c: f32| (c * alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        px.copy_from_slice(&[q(r), q(g), q(b), (alpha * 255.0).round().clamp(0.0, 255.0) as u8]);
    }
}

/// Invert the alpha (premultiplied white with alpha 1-a): used for inner shadows.
pub fn invert_alpha(p: &mut Pixmap) {
    for px in p.data_mut().as_chunks_mut::<4>().0 {
        let a = 255 - px.get(3).copied().unwrap_or(0);
        px.copy_from_slice(&[a, a, a, a]);
    }
}

/// Multiply every pixel of `p` by the alpha of `m` (same size); `invert` uses 1-alpha.
pub fn mask_by(p: &mut Pixmap, m: &Pixmap, invert: bool) {
    for (px, mp) in p.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(m.data().chunks_exact(4)) {
        let ma = u32::from(mp.get(3).copied().unwrap_or(0));
        let ma = if invert { 255 - ma } else { ma };
        for c in px.iter_mut() {
            *c = ((u32::from(*c) * ma + 127) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_spreads_and_conserves() {
        let mut p = Pixmap::new(64, 64).unwrap();
        let i = (32 * 64 + 32) * 4;
        p.data_mut()[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        let before: u32 = p.data().iter().map(|v| u32::from(*v)).sum();
        blur(&mut p, 3.0);
        let after: u32 = p.data().iter().map(|v| u32::from(*v)).sum();
        assert!(p.data()[i + 3] < 255);
        // Integer rounding loses a little.
        assert!(after <= before && after > 0, "{before} {after}");
    }

    #[test]
    fn morph_grows() {
        let mut p = Pixmap::new(20, 20).unwrap();
        let i = (10 * 20 + 10) * 4;
        p.data_mut()[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        morph(&mut p, 2);
        let count = p.data().as_chunks::<4>().0.iter().filter(|c| c[3] == 255).count();
        assert_eq!(count, 25);
        morph(&mut p, -2);
        let count = p.data().as_chunks::<4>().0.iter().filter(|c| c[3] == 255).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn box_sizes_sane() {
        for s in [0.5, 1.0, 4.0, 50.0] {
            let b = box_sizes(s);
            assert!(b.iter().all(|r| *r < 1000));
        }
    }
}
