//! Swatch grid: a grid of colour swatches from a palette (hex colours), centred in the view, each
//! named after its colour and optionally labelled.

use figcraft_plugin_sdk::{Input, Result, Value, call, entry, json, log, notify};

/// Swatches made in one run, at most.
const MAX_SWATCHES: usize = 256;

/// `#RGB` or `#RRGGBB` (the `#` optional) as `#RRGGBB`.
fn hex(s: &str) -> Option<String> {
    let h = s.trim().trim_start_matches('#');
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match h.len() {
        3 => Some(h.chars().flat_map(|c| [c, c]).collect::<String>()),
        6 => Some(h.to_string()),
        _ => None,
    }
    .map(|h| format!("#{}", h.to_uppercase()))
}

/// The palette's valid colours, logging the rest.
fn palette(text: &str) -> Vec<String> {
    text.split([',', ' ', ';', '\n'])
        .filter(|s| !s.trim().is_empty())
        .filter_map(|s| {
            let c = hex(s);
            if c.is_none() {
                log(&format!("skipped `{s}`: not a hex colour"));
            }
            c
        })
        .take(MAX_SWATCHES)
        .collect()
}

fn run(input: Input) -> Result<Value> {
    let colors = palette(input.str("palette").unwrap_or_default());
    if colors.is_empty() {
        return Err("The palette has no hex colours (like #FF5A5F, #00A699)".into());
    }
    let columns = input.f64("columns").unwrap_or(3.0).clamp(1.0, 24.0) as usize;
    let size = input.f64("size").unwrap_or(64.0).clamp(8.0, 512.0);
    let gap = input.f64("gap").unwrap_or(16.0).clamp(0.0, 256.0);
    let labels = input.bool("labels").unwrap_or(true);
    let rows = colors.len().div_ceil(columns);
    let cell_h = if labels { size + 20.0 } else { size };
    let (w, h) = (columns as f64 * (size + gap) - gap, rows as f64 * (cell_h + gap) - gap);
    // Centred in the view, on whole pixels.
    let view = call("view.get", &json!({}))?;
    let (x0, y0) = ((view["cx"].as_f64().unwrap_or(0.0) - w / 2.0).round(), (view["cy"].as_f64().unwrap_or(0.0) - h / 2.0).round());
    let mut made = Vec::new();
    for (i, color) in colors.iter().enumerate() {
        let (x, y) = (x0 + (i % columns) as f64 * (size + gap), y0 + (i / columns) as f64 * (cell_h + gap));
        let r = call("shape.rectangle", &json!({"x": x, "y": y, "width": size, "height": size, "props": {"name": color, "fill": color}}))?;
        made.extend(r["id"].as_u64());
        if labels {
            let t = call("text.create", &json!({"text": color, "x": x, "y": y + size + 4.0, "props": {"fontSize": 11}}))?;
            made.extend(t["id"].as_u64());
        }
    }
    call("select.set", &json!({"ids": made}))?;
    notify(&format!("Made {} swatches", colors.len()));
    Ok(json!({"swatches": colors.len(), "ids": made}))
}

entry!(run);

#[cfg(test)]
mod tests {
    use super::{hex, palette};

    #[test]
    fn palettes_parse() {
        assert_eq!(hex("#f0a"), Some("#FF00AA".into()));
        assert_eq!(hex(" 00a699 "), Some("#00A699".into()));
        assert_eq!(hex("#12345"), None);
        assert_eq!(hex("red"), None);
        assert_eq!(palette("#FF5A5F, nope;#0f0\n00A699"), vec!["#FF5A5F", "#00FF00", "#00A699"]);
    }
}
