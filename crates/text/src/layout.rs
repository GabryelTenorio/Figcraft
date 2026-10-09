//! Text layout: shaping styled spans, fallback fonts, line breaking, alignment and decorations.

use std::sync::Arc;

use figcraft_doc::{LineHeight, ListStyle, TextAlignH, TextAutoResize, TextCase, TextData, TextDecoration, TextStyle, valid_feature_tag};
use figcraft_geom::{Affine, BezPath, Rect};
use harfrust::{Direction, Feature, ShapeOptions, Tag, UnicodeBuffer};

use crate::fontdb::{Face, db};

/// A positioned glyph (pen position on the baseline, layout coordinates, y down).
#[derive(Clone, Debug)]
pub struct Glyph {
    pub face: Arc<Face>,
    pub gid: u32,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub advance: f64,
    /// Source char range of the glyph's cluster.
    pub char_start: usize,
    pub char_end: usize,
    /// Index into [`TextLayout::styles`].
    pub style: usize,
}

impl Glyph {
    /// The glyph outline placed in layout coordinates.
    pub fn path(&self) -> BezPath {
        let s = self.size / self.face.upem;
        let mut p = (*self.face.outline(self.gid)).clone();
        p.apply_affine(Affine::new([s, 0.0, 0.0, s, self.x, self.y]));
        p
    }
}

#[derive(Clone, Debug)]
pub struct Line {
    /// Glyph index range.
    pub glyph_start: usize,
    pub glyph_end: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub top: f64,
    pub height: f64,
    pub baseline: f64,
    pub width: f64,
    pub x: f64,
    /// The line ends a paragraph (hard break or end of text).
    pub para_end: bool,
}

/// A decoration line (underline/strikethrough) to paint with the style's fills.
#[derive(Clone, Debug)]
pub struct Decoration {
    pub rect: Rect,
    pub style: usize,
}

#[derive(Clone, Debug, Default)]
pub struct TextLayout {
    pub glyphs: Vec<Glyph>,
    pub lines: Vec<Line>,
    pub decorations: Vec<Decoration>,
    /// The distinct styles the glyphs refer to.
    pub styles: Vec<TextStyle>,
    /// Size of the laid-out content.
    pub width: f64,
    pub height: f64,
    /// Whether text was cut off (truncation).
    pub truncated: bool,
}

/// Ratio of Figma's "Auto" line height to the font size (font ascent+descent+gap, per face).
fn natural_ratio(face: &Face) -> f64 {
    ((face.ascent + face.descent + face.line_gap.max(0.0)) / face.upem).max(0.5)
}

struct Item {
    face: Arc<Face>,
    /// Glyph scale relative to the style's size (synthesised small caps).
    scale: f64,
    gid: u32,
    adv: f64,
    dx: f64,
    dy: f64,
    char_start: usize,
    char_end: usize,
    style: usize,
    ch: char,
}

impl Item {
    fn is_space(&self) -> bool {
        matches!(self.ch, ' ' | '\t' | '\u{3000}' | '\u{2002}'..='\u{200B}')
    }
    fn break_after(&self) -> bool {
        self.is_space() || matches!(self.ch, '-' | '\u{2010}' | '\u{2013}' | '\u{2014}' | '/') || is_cjk(self.ch)
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF)
}

/// Lay out `text` for a box of `box_width` (used when the text wraps or aligns; for auto width
/// pass `None`). `box_height` is used for truncation only.
pub fn layout(text: &TextData, box_width: Option<f64>, box_height: Option<f64>) -> TextLayout {
    let chars: Vec<char> = text.characters.chars().collect();
    let spans = text.spans();
    let mut styles: Vec<TextStyle> = Vec::new();
    let mut span_style: Vec<usize> = Vec::with_capacity(spans.len());
    for (_, _, s) in &spans {
        let idx = match styles.iter().position(|x| x == *s) {
            Some(i) => i,
            None => {
                styles.push((*s).clone());
                styles.len() - 1
            }
        };
        span_style.push(idx);
    }
    let mut dbw = db().write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let fallbacks = dbw.fallbacks();
    let mut faces: Vec<Option<Arc<Face>>> = Vec::with_capacity(styles.len());
    for s in &styles {
        faces.push(dbw.find(&s.font_family, &s.font_style).or_else(|| dbw.default_face()));
    }
    drop(dbw);

    // OpenType features the user toggled (global for the node).
    let features: Vec<Feature> = text
        .features
        .iter()
        .filter(|(k, _)| valid_feature_tag(k))
        .filter_map(|(k, on)| <[u8; 4]>::try_from(k.as_bytes()).ok().map(|t| Feature::new(Tag::new(&t), u32::from(*on), ..)))
        .collect();

    // Shape each span, split by paragraph, falling back per char.
    let mut items: Vec<Item> = Vec::with_capacity(chars.len());
    for (si, (start, end, _)) in spans.iter().enumerate() {
        let st_idx = span_style.get(si).copied().unwrap_or(0);
        let Some(style) = styles.get(st_idx) else { continue };
        let Some(Some(primary)) = faces.get(st_idx) else { continue };
        let size = sanitize_size(style.font_size);
        let spacing = style.letter_spacing.resolve(size);
        // Segment by face.
        let mut seg_start = *start;
        while seg_start < *end {
            let face_for = |c: char| -> Arc<Face> {
                if c == '\n' || primary.has_char(c) {
                    return primary.clone();
                }
                fallbacks.iter().find(|f| f.has_char(c)).cloned().unwrap_or_else(|| primary.clone())
            };
            let first = chars.get(seg_start).copied().unwrap_or(' ');
            let face = face_for(first);
            let mut seg_end = seg_start + 1;
            while seg_end < *end && chars.get(seg_end).is_some_and(|c| *c != '\n' && first != '\n' && face_for(*c).id == face.id) {
                seg_end += 1;
            }
            if first == '\n' {
                items.push(Item {
                    face: face.clone(),
                    scale: 1.0,
                    gid: 0,
                    adv: 0.0,
                    dx: 0.0,
                    dy: 0.0,
                    char_start: seg_start,
                    char_end: seg_start + 1,
                    style: st_idx,
                    ch: '\n',
                });
            } else {
                let seg = Segment { start: seg_start, end: seg_end, style, size, spacing, st_idx };
                shape_segment(&chars, &seg, &face, &features, &mut items);
            }
            seg_start = seg_end;
        }
    }

    // List markers sit in a gutter and wrapped lines hang at the text start; paragraph indent
    // applies to plain paragraphs only.
    let marker_width = list_gutter(text.list, &chars, styles.first(), faces.first().cloned().flatten().as_ref());
    let indent = if text.list == ListStyle::None && text.paragraph_indent.is_finite() { text.paragraph_indent.clamp(0.0, 10_000.0) } else { 0.0 };

    // Line breaking.
    let wrap = match text.auto_resize {
        TextAutoResize::WidthAndHeight => None,
        _ => box_width.filter(|w| w.is_finite() && *w > 0.0),
    };
    let mut lines: Vec<(usize, usize, bool)> = Vec::new(); // item range, paragraph end
    let mut line_start = 0;
    let mut i = 0;
    let mut x = 0.0;
    let mut last_break: Option<usize> = None;
    let mut para_first_line = true;
    while i < items.len() {
        let Some(it) = items.get(i) else { break };
        if it.ch == '\n' {
            lines.push((line_start, i + 1, true));
            i += 1;
            line_start = i;
            x = 0.0;
            last_break = None;
            para_first_line = true;
            continue;
        }
        let line_indent = marker_width + if para_first_line { indent } else { 0.0 };
        let w = it.adv;
        if let Some(maxw) = wrap
            && x + w > maxw - line_indent + 1e-6
            && i > line_start
            && !it.is_space()
        {
            let brk = last_break.filter(|b| *b >= line_start).map(|b| b + 1).unwrap_or(i);
            lines.push((line_start, brk, false));
            line_start = brk;
            i = brk;
            x = 0.0;
            last_break = None;
            para_first_line = false;
            continue;
        }
        x += w;
        if it.break_after() {
            last_break = Some(i);
        }
        i += 1;
    }
    if line_start < items.len() || lines.is_empty() || lines.last().is_some_and(|l| l.2) {
        lines.push((line_start, items.len(), true));
    }

    // Line metrics and positions.
    let default_style = styles.first().cloned().unwrap_or_default();
    let default_face = faces.first().cloned().flatten();
    let mut out = TextLayout { styles: styles.clone(), ..Default::default() };
    let mut y = 0.0;
    let mut para_start_line = true;
    let max_lines = if text.max_lines > 0 && matches!(text.auto_resize, TextAutoResize::Truncate) { text.max_lines as usize } else { usize::MAX };
    for (li, (s, e, para_end)) in lines.iter().enumerate() {
        if li >= max_lines {
            out.truncated = true;
            break;
        }
        // Metrics from the styles present on the line (or the style at the break for empty lines).
        let mut asc: f64 = 0.0;
        let mut desc: f64 = 0.0;
        let mut lh: f64 = 0.0;
        let mut any = false;
        for it in items.get(*s..*e).unwrap_or(&[]) {
            let Some(st) = styles.get(it.style) else { continue };
            let size = sanitize_size(st.font_size);
            let k = size / it.face.upem;
            asc = asc.max(it.face.ascent * k);
            desc = desc.max(it.face.descent * k);
            lh = lh.max(st.line_height.resolve(size, natural_ratio(&it.face)));
            any = true;
        }
        if !any {
            let st = items.get(s.saturating_sub(1)).and_then(|it| styles.get(it.style)).unwrap_or(&default_style);
            let size = sanitize_size(st.font_size);
            if let Some(f) = items.get(s.saturating_sub(1)).map(|it| it.face.clone()).or_else(|| default_face.clone()) {
                let k = size / f.upem;
                asc = f.ascent * k;
                desc = f.descent * k;
                lh = st.line_height.resolve(size, natural_ratio(&f));
            } else {
                asc = size * 0.8;
                desc = size * 0.2;
                lh = size * 1.2;
            }
        }
        if para_start_line && li > 0 {
            y += text.paragraph_spacing.max(0.0);
        }
        let baseline = y + (lh - (asc + desc)) / 2.0 + asc;
        // Trailing spaces don't count for width/alignment.
        let mut vis_end = *e;
        while vis_end > *s && items.get(vis_end - 1).is_some_and(|it| it.is_space() || it.ch == '\n') {
            vis_end -= 1;
        }
        let width: f64 = items.get(*s..vis_end).unwrap_or(&[]).iter().map(|it| it.adv).sum();
        let line_indent = marker_width + if para_start_line { indent } else { 0.0 };
        let gstart = out.glyphs.len();
        let mut pen = line_indent;
        // Justify: extra space per gap.
        let gaps = items.get(*s..vis_end).unwrap_or(&[]).iter().filter(|it| it.is_space()).count();
        let justify_extra = match (text.align_h, wrap) {
            (TextAlignH::Justified, Some(maxw)) if !para_end && gaps > 0 => ((maxw - line_indent - width) / gaps as f64).max(0.0),
            _ => 0.0,
        };
        // List marker (before the line's text; zero-length char range so carets skip it).
        if para_start_line && text.list != ListStyle::None {
            let marker = match text.list {
                ListStyle::Bulleted => "•".to_string(),
                _ => format!("{}.", count_paragraphs_before(&chars, items.get(*s).map(|it| it.char_start).unwrap_or(0)) + 1),
            };
            let st_idx = items.get(*s).map(|it| it.style).unwrap_or(0);
            let at = items.get(*s).map(|it| it.char_start).unwrap_or(chars.len());
            if let (Some(st), Some(face)) = (styles.get(st_idx), items.get(*s).map(|it| it.face.clone()).or_else(|| default_face.clone())) {
                let size = sanitize_size(st.font_size);
                let k = size / face.upem;
                // Numbers end at the same x; bullets start at the gutter's left edge.
                let mut mx =
                    if text.list == ListStyle::Numbered { (marker_width - size * 0.5 - marker_advance(&face, &marker, size)).max(0.0) } else { 0.0 };
                for c in marker.chars() {
                    let gid = face.glyph_for(c);
                    let adv = face.advance(gid) * k;
                    out.glyphs.push(Glyph {
                        face: face.clone(),
                        gid,
                        x: mx,
                        y: baseline,
                        size,
                        advance: adv,
                        char_start: at,
                        char_end: at,
                        style: st_idx,
                    });
                    mx += adv;
                }
            }
        }
        for it in items.get(*s..*e).unwrap_or(&[]) {
            if it.ch == '\n' {
                continue;
            }
            let size = styles.get(it.style).map(|st| sanitize_size(st.font_size)).unwrap_or(12.0) * it.scale;
            if it.gid != 0 || !it.is_space() {
                out.glyphs.push(Glyph {
                    face: it.face.clone(),
                    gid: it.gid,
                    x: pen + it.dx,
                    y: baseline - it.dy,
                    size,
                    advance: it.adv,
                    char_start: it.char_start,
                    char_end: it.char_end,
                    style: it.style,
                });
            }
            pen += it.adv;
            if it.is_space() {
                pen += justify_extra;
            }
        }
        let char_start = items.get(*s).map(|it| it.char_start).unwrap_or_else(|| chars.len());
        let char_end = items.get(e.saturating_sub(1)).map(|it| it.char_end).unwrap_or(char_start).max(char_start);
        out.lines.push(Line {
            glyph_start: gstart,
            glyph_end: out.glyphs.len(),
            char_start,
            char_end,
            top: y,
            height: lh,
            baseline,
            width: width + line_indent,
            x: 0.0,
            para_end: *para_end,
        });
        y += lh;
        para_start_line = *para_end;
    }
    out.height = y;
    out.width = out.lines.iter().map(|l| l.width).fold(0.0, f64::max);

    // Truncation by height.
    if matches!(text.auto_resize, TextAutoResize::Truncate)
        && let Some(h) = box_height
        && out.height > h + 0.5
    {
        let keep = out.lines.iter().take_while(|l| l.top + l.height <= h + 0.5).count().max(1);
        if keep < out.lines.len() {
            out.lines.truncate(keep);
            let gend = out.lines.last().map(|l| l.glyph_end).unwrap_or(0);
            out.glyphs.truncate(gend);
            out.truncated = true;
            out.height = out.lines.last().map(|l| l.top + l.height).unwrap_or(0.0);
        }
    }
    if out.truncated {
        add_ellipsis(&mut out, wrap);
    }

    // Horizontal alignment within the box.
    let align_width = match text.auto_resize {
        TextAutoResize::WidthAndHeight => out.width,
        _ => box_width.unwrap_or(out.width),
    };
    for li in 0..out.lines.len() {
        let Some(line) = out.lines.get(li) else { continue };
        let dx = match text.align_h {
            TextAlignH::Left | TextAlignH::Justified => 0.0,
            TextAlignH::Center => (align_width - line.width) / 2.0,
            TextAlignH::Right => align_width - line.width,
        };
        if dx != 0.0 {
            let (gs, ge) = (line.glyph_start, line.glyph_end);
            for g in out.glyphs.get_mut(gs..ge).unwrap_or(&mut []) {
                g.x += dx;
            }
            if let Some(l) = out.lines.get_mut(li) {
                l.x = dx;
            }
        }
    }

    // Decorations per contiguous glyph run of the same style on a line.
    let mut decos = Vec::new();
    for line in &out.lines {
        let gl = out.glyphs.get(line.glyph_start..line.glyph_end).unwrap_or(&[]);
        let mut j = 0;
        while j < gl.len() {
            let Some(g0) = gl.get(j) else { break };
            // List markers and the ellipsis are not decorated.
            if g0.char_end == g0.char_start {
                j += 1;
                continue;
            }
            let st = out.styles.get(g0.style).cloned().unwrap_or_default();
            let mut k = j + 1;
            while gl.get(k).is_some_and(|g| g.style == g0.style && g.char_end > g.char_start) {
                k += 1;
            }
            if st.decoration != TextDecoration::None
                && let Some(last) = gl.get(k - 1)
            {
                let x0 = g0.x;
                let x1 = last.x + last.advance;
                let f = &g0.face;
                let s = g0.size / f.upem;
                let th = (f.underline_thickness * s).max(0.5);
                let yy = match st.decoration {
                    TextDecoration::Underline => line.baseline - f.underline_pos * s,
                    _ => line.baseline - f.strike_pos * s,
                };
                decos.push(Decoration { rect: Rect::new(x0, yy - th / 2.0, x1, yy + th / 2.0), style: g0.style });
            }
            j = k;
        }
    }
    out.decorations = decos;
    let _ = LineHeight::Auto;
    out
}

/// Width of the list marker gutter: room for the widest marker plus a gap.
fn list_gutter(list: ListStyle, chars: &[char], style: Option<&TextStyle>, face: Option<&Arc<Face>>) -> f64 {
    let size = style.map(|s| sanitize_size(s.font_size)).unwrap_or(12.0);
    match list {
        ListStyle::None => 0.0,
        ListStyle::Bulleted => size * 1.5,
        ListStyle::Numbered => {
            let paras = chars.iter().filter(|c| **c == '\n').count() + 1;
            let widest = face.map(|f| marker_advance(f, &format!("{paras}."), size)).unwrap_or(size * 0.6 * (paras.to_string().len() + 1) as f64);
            (widest + size * 0.5).max(size * 1.5)
        }
    }
}

fn marker_advance(face: &Face, s: &str, size: f64) -> f64 {
    let k = size / face.upem;
    s.chars().map(|c| face.advance(face.glyph_for(c)) * k).sum()
}

fn count_paragraphs_before(chars: &[char], pos: usize) -> usize {
    chars.iter().take(pos).filter(|c| **c == '\n').count()
}

fn add_ellipsis(out: &mut TextLayout, wrap: Option<f64>) {
    let Some(line) = out.lines.last().cloned() else { return };
    let Some(last) = out.glyphs.get(line.glyph_end.saturating_sub(1)).cloned() else { return };
    let face = last.face.clone();
    let gid = face.glyph_for('…');
    let gid = if gid == 0 { face.glyph_for('.') } else { gid };
    let k = last.size / face.upem;
    let adv = face.advance(gid) * k;
    let limit = wrap.unwrap_or(f64::INFINITY);
    // Drop glyphs until the ellipsis fits, then any trailing spaces ("word…", not "word …").
    while out.glyphs.len() > line.glyph_start && out.glyphs.last().is_some_and(|g| g.x + g.advance + adv > limit) {
        out.glyphs.pop();
    }
    while out.glyphs.len() > line.glyph_start && out.glyphs.last().is_some_and(|g| g.char_end > g.char_start && g.gid == g.face.glyph_for(' ')) {
        out.glyphs.pop();
    }
    let x = out.glyphs.last().filter(|_| out.glyphs.len() > line.glyph_start).map(|g| g.x + g.advance).unwrap_or(0.0);
    out.glyphs.push(Glyph {
        face,
        gid,
        x,
        y: line.baseline,
        size: last.size,
        advance: adv,
        char_start: last.char_end,
        char_end: last.char_end,
        style: last.style,
    });
    if let Some(l) = out.lines.last_mut() {
        l.glyph_end = out.glyphs.len();
        l.width = x + adv;
    }
}

fn sanitize_size(s: f64) -> f64 {
    if s.is_finite() { s.clamp(0.5, 5000.0) } else { 12.0 }
}

/// One run of chars shaped with a single face and style.
struct Segment<'a> {
    start: usize,
    end: usize,
    style: &'a TextStyle,
    size: f64,
    spacing: f64,
    st_idx: usize,
}

/// Synthesised small caps: lower-case letters drawn as capitals at this fraction of the size.
const SMALL_CAP_SCALE: f64 = 0.72;

fn shape_segment(chars: &[char], seg: &Segment, face: &Arc<Face>, features: &[Feature], items: &mut Vec<Item>) {
    let (start, end, style) = (seg.start, seg.end, seg.style);
    let k = seg.size / face.upem;
    let small_caps = style.case == TextCase::SmallCaps;
    let native_smcp = small_caps && face.has_feature(b"smcp");
    // Map each source char through the case (title case looks at the char before the segment, so
    // a word split across styled runs is capitalised once) and remember synthesised small caps.
    let mut mapped: Vec<(char, usize, bool)> = Vec::with_capacity(end.saturating_sub(start));
    let mut prev = start.checked_sub(1).and_then(|i| chars.get(i)).copied();
    for (i, c) in chars.get(start..end).unwrap_or(&[]).iter().enumerate() {
        let src = start + i;
        if small_caps && !native_smcp && c.is_lowercase() {
            for u in c.to_uppercase() {
                mapped.push((u, src, true));
            }
        } else {
            let mut buf = [0u8; 4];
            for u in style.case.apply_after(c.encode_utf8(&mut buf), prev).chars() {
                mapped.push((u, src, false));
            }
        }
        prev = Some(*c);
    }
    let mut small = vec![false; end.saturating_sub(start)];
    for (_, src, sm) in &mapped {
        if *sm && let Some(f) = small.get_mut(src - start) {
            *f = true;
        }
    }
    let small_at = |src: usize| src.checked_sub(start).and_then(|i| small.get(i)).copied().unwrap_or(false);
    let mut feats: Vec<Feature> = features.to_vec();
    if native_smcp {
        feats.push(Feature::new(Tag::new(b"smcp"), 1, ..));
    }
    let mut raw: Vec<(u32, u32, i32, i32, i32)> = Vec::new();
    if let Some(hb) = face.hb() {
        let shaper = face.shaper.shaper(&hb).build();
        let mut buf = UnicodeBuffer::new();
        for (u, src, _) in &mapped {
            buf.add(*u, *src as u32);
        }
        buf.set_direction(Direction::LeftToRight);
        buf.guess_segment_properties();
        let gb = shaper.shape(buf, ShapeOptions::new().features(&feats));
        for (info, pos) in gb.glyph_infos().iter().zip(gb.glyph_positions()) {
            raw.push((info.glyph_id, info.cluster, pos.x_advance, pos.x_offset, pos.y_offset));
        }
    } else {
        for (u, src, _) in &mapped {
            let gid = face.glyph_for(*u);
            raw.push((gid, *src as u32, face.advance(gid) as i32, 0, 0));
        }
    }
    let n = raw.len();
    for gi in 0..n {
        let Some(&(gid, cl, xa, xo, yo)) = raw.get(gi) else { continue };
        let cl = cl as usize;
        // Cluster end: the next larger cluster value, else the segment end.
        let cend = raw.iter().skip(gi + 1).map(|r| r.1 as usize).find(|c| *c > cl).unwrap_or(end).max(cl + 1);
        let last_in_cluster = raw.get(gi + 1).is_none_or(|r| r.1 as usize != cl);
        let ch = chars.get(cl).copied().unwrap_or(' ');
        let scale = if small_at(cl) { SMALL_CAP_SCALE } else { 1.0 };
        let adv = f64::from(xa) * k * scale + if last_in_cluster { seg.spacing * (cend - cl) as f64 } else { 0.0 };
        items.push(Item {
            face: face.clone(),
            scale,
            gid,
            adv,
            dx: f64::from(xo) * k * scale,
            dy: f64::from(yo) * k * scale,
            char_start: cl,
            char_end: cend,
            style: seg.st_idx,
            ch,
        });
    }
}

impl TextLayout {
    /// The caret position (x, line index) before char `index`.
    pub fn caret(&self, index: usize) -> (f64, usize) {
        for (li, line) in self.lines.iter().enumerate() {
            let last_line = li + 1 == self.lines.len();
            let in_line = index >= line.char_start && (index < line.char_end || (last_line && index <= line.char_end));
            // A caret at the end of a soft-wrapped line belongs to the next line.
            if !(in_line || index == line.char_end && line.para_end && !last_line) {
                continue;
            }
            if index == line.char_end && line.para_end && !last_line {
                // Before the newline: end of this line.
                let x = self.line_end_x(line);
                return (x, li);
            }
            let gl = self.glyphs.get(line.glyph_start..line.glyph_end).unwrap_or(&[]);
            for g in gl {
                if g.char_end == g.char_start {
                    continue;
                }
                if index >= g.char_start && index < g.char_end {
                    let frac = (index - g.char_start) as f64 / (g.char_end - g.char_start) as f64;
                    return (g.x + g.advance * frac, li);
                }
            }
            return (self.line_end_x(line), li);
        }
        let li = self.lines.len().saturating_sub(1);
        (self.lines.last().map(|l| self.line_end_x(l)).unwrap_or(0.0), li)
    }

    fn line_end_x(&self, line: &Line) -> f64 {
        self.glyphs
            .get(line.glyph_start..line.glyph_end)
            .unwrap_or(&[])
            .iter()
            .filter(|g| g.char_end > g.char_start)
            .map(|g| g.x + g.advance)
            .fold(line.x, f64::max)
    }

    /// The char index nearest to `(x, y)` (layout coordinates).
    pub fn hit(&self, x: f64, y: f64) -> usize {
        let Some((li, line)) =
            self.lines.iter().enumerate().find(|(_, l)| y < l.top + l.height).or_else(|| self.lines.iter().enumerate().next_back())
        else {
            return 0;
        };
        let gl = self.glyphs.get(line.glyph_start..line.glyph_end).unwrap_or(&[]);
        for g in gl {
            if g.char_end <= g.char_start {
                continue;
            }
            if x < g.x + g.advance / 2.0 {
                return g.char_start;
            }
        }
        let last_line = li + 1 == self.lines.len();
        if line.para_end && !last_line { line.char_end.saturating_sub(1).max(line.char_start) } else { line.char_end }
    }

    /// Selection rectangles for chars `start..end`.
    pub fn selection_rects(&self, start: usize, end: usize) -> Vec<Rect> {
        let (a, b) = (start.min(end), start.max(end));
        let mut out = Vec::new();
        for (li, line) in self.lines.iter().enumerate() {
            if b <= line.char_start || a > line.char_end || (a == line.char_end && li + 1 < self.lines.len()) {
                continue;
            }
            let (x0, _) = self.caret(a.max(line.char_start));
            let x1 = if b >= line.char_end {
                self.line_end_x(line) + if line.para_end && b > line.char_end.saturating_sub(1) { 4.0 } else { 0.0 }
            } else {
                self.caret(b).0
            };
            out.push(Rect::new(x0, line.top, x1.max(x0 + 1.0), line.top + line.height));
        }
        out
    }

    /// All glyph outlines as one path (for outline/flatten and boolean ops).
    pub fn to_path(&self) -> BezPath {
        let mut p = BezPath::new();
        for g in &self.glyphs {
            p.extend(g.path());
        }
        for d in &self.decorations {
            p.extend(figcraft_geom::rounded_rect(d.rect.width(), d.rect.height(), [0.0; 4], 0.0).iter().map(|el| {
                let mut e = el;
                e.apply_affine(Affine::translate((d.rect.x0, d.rect.y0)));
                e
            }));
        }
        p
    }
}

trait ApplyAffine {
    fn apply_affine(&mut self, a: Affine);
}

impl ApplyAffine for figcraft_geom::kurbo::PathEl {
    fn apply_affine(&mut self, a: Affine) {
        use figcraft_geom::kurbo::PathEl::*;
        *self = match *self {
            MoveTo(p) => MoveTo(a * p),
            LineTo(p) => LineTo(a * p),
            QuadTo(p1, p2) => QuadTo(a * p1, a * p2),
            CurveTo(p1, p2, p3) => CurveTo(a * p1, a * p2, a * p3),
            ClosePath => ClosePath,
        };
    }
}
