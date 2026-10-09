//! FigCraft text: fonts, shaping and layout.
//!
//! Fonts come from the optional craft-fonts build input (Inter and friends, embedded at build time)
//! and, on desktop, from the system's font folders. [`layout`] turns a text node's
//! [`TextData`](figcraft_doc::TextData) into positioned glyphs, lines and decorations; the renderer
//! fills [`Glyph::path`]s and the editor uses [`TextLayout::caret`] / [`TextLayout::hit`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod fontdb;
mod layout;

pub use fontdb::{
    Axis, CRAFT_FONTS, Coords, Face, FontDb, NamedInstance, axis_label, db, family_names, family_styles, find_face, no_coords, scan_system_fonts,
};
pub use layout::{Decoration, Glyph, Line, TextLayout, layout};

use figcraft_doc::{Node, NodeKind, TextAlignV, TextAutoResize};

type CacheKey = (u64, u64, u64);

fn layout_cache() -> &'static std::sync::Mutex<std::collections::HashMap<CacheKey, std::sync::Arc<TextLayout>>> {
    static C: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<CacheKey, std::sync::Arc<TextLayout>>>> = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Forget cached layouts (after fonts were added).
pub fn clear_layout_cache() {
    layout_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
}

/// Layout with a process-wide cache keyed by the text content/style and box size.
pub fn layout_cached(t: &figcraft_doc::TextData, w: Option<f64>, h: Option<f64>) -> std::sync::Arc<TextLayout> {
    use std::hash::{Hash, Hasher};
    let mut hs = std::collections::hash_map::DefaultHasher::new();
    format!("{t:?}").hash(&mut hs);
    let key = (hs.finish(), w.map(f64::to_bits).unwrap_or(1), h.map(f64::to_bits).unwrap_or(1));
    if let Some(l) = layout_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
        return l.clone();
    }
    let l = std::sync::Arc::new(layout(t, w, h));
    let mut c = layout_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if c.len() > 4096 {
        c.clear();
    }
    c.insert(key, l.clone());
    l
}

/// Lay out a text node at its current box, returning the layout and the vertical offset that
/// applies its vertical alignment.
pub fn layout_node(n: &Node) -> Option<(TextLayout, f64)> {
    let NodeKind::Text(t) = &n.kind else { return None };
    let l = (*layout_cached(t, Some(n.width), Some(n.height))).clone();
    let dy = match (t.auto_resize, t.align_v) {
        (TextAutoResize::WidthAndHeight | TextAutoResize::Height, _) => 0.0,
        (_, TextAlignV::Top) => 0.0,
        (_, TextAlignV::Middle) => (n.height - l.height) / 2.0,
        (_, TextAlignV::Bottom) => n.height - l.height,
    };
    Some((l, dy))
}

/// The box size a text node should have after its content or style changed, per its resize mode.
pub fn auto_size(n: &Node) -> Option<(f64, f64)> {
    let NodeKind::Text(t) = &n.kind else { return None };
    match t.auto_resize {
        TextAutoResize::WidthAndHeight => {
            let l = layout_cached(t, None, None);
            Some((l.width.ceil().max(1.0), l.height.max(1.0)))
        }
        TextAutoResize::Height => {
            let l = layout_cached(t, Some(n.width), None);
            Some((n.width, l.height.max(1.0)))
        }
        TextAutoResize::None | TextAutoResize::Truncate => None,
    }
}

#[cfg(test)]
mod tests;
