//! Keyboard handling: the catalog's shortcuts, tool keys, nudging, and text editing keys.

use egui::{Context, Event, Key, Modifiers};
use figcraft_engine::catalog::MENU;
use serde_json::json;

use crate::FigApp;

/// Parse "Shift+Cmd+K" into modifiers + key.
pub fn parse(s: &str) -> Option<(Modifiers, Key)> {
    let mut m = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part {
            "Shift" => m.shift = true,
            "Cmd" => m.command = true,
            "Alt" => m.alt = true,
            "Ctrl" => m.ctrl = true,
            "" => {}
            k => key = key_of(k),
        }
    }
    key.map(|k| (m, k))
}

fn key_of(k: &str) -> Option<Key> {
    Some(match k {
        "Delete" => Key::Backspace,
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Tab" => Key::Tab,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "=" => Key::Equals,
        "-" => Key::Minus,
        "[" => Key::OpenBracket,
        "]" => Key::CloseBracket,
        "\\" => Key::Backslash,
        "'" => Key::Quote,
        "," => Key::Comma,
        "." => Key::Period,
        "/" | "?" => Key::Slash,
        other => Key::from_name(other)?,
    })
}

fn mods_match(want: Modifiers, got: Modifiers) -> bool {
    want.shift == got.shift && want.command == got.command && want.alt == got.alt && (want.ctrl == got.ctrl || got.mac_cmd)
}

pub fn handle(app: &mut FigApp, ctx: &Context) {
    // Text fields in panels own the keyboard.
    if ctx.memory(|m| m.focused().is_some()) || app.ui.palette.is_some() {
        return;
    }
    let events: Vec<Event> = ctx.input(|i| i.events.clone());
    let editing = app.session.ds().map(|d| d.text_edit.is_some()).unwrap_or(false);
    if editing {
        text_keys(app, ctx, &events);
        return;
    }
    // An image-only clipboard brings no paste event, just the key: paste from the key then.
    let paste_event = events.iter().any(|e| matches!(e, Event::Paste(_)));
    let paste_key = events.iter().any(|e| matches!(e, Event::Key { key: Key::V, pressed: true, repeat: false, modifiers, .. } if modifiers.command && !modifiers.shift && !modifiers.alt));
    if paste_key && !paste_event {
        crate::menus::paste(app, None);
    }
    for ev in &events {
        match ev {
            Event::Copy => crate::menus::invoke(app, "edit.copy"),
            Event::Cut => crate::menus::invoke(app, "edit.cut"),
            Event::Paste(text) => crate::menus::paste(app, Some(text)),
            Event::Key { key, pressed: true, modifiers, repeat, .. } if key_event(app, *key, *modifiers, *repeat, ctx) => {
                // handled
            }
            _ => {}
        }
    }
}

fn key_event(app: &mut FigApp, key: Key, m: Modifiers, repeat: bool, ctx: &Context) -> bool {
    // A gradient open in the colour picker: Delete removes its selected stop.
    if matches!(key, Key::Backspace | Key::Delete) && m.is_none() && app.ui.picker.is_some() && crate::handles::remove_selected_stop(app, ctx) {
        return true;
    }
    // Vector edit mode and the shape builder have their own keys (tools, nudging points, delete,
    // join, Enter to finish); the rest fall through.
    match app.session.tool_key(key.name(), m.shift, m.alt, m.command) {
        Ok(true) => return true,
        Ok(false) => {}
        Err(e) => {
            app.toast(e.to_string());
            return true;
        }
    }
    // Catalog shortcuts (exact modifier match).
    for item in MENU.iter().filter(|i| !i.shortcut.is_empty()) {
        if let Some((want, k)) = parse(item.shortcut)
            && k == key
            && mods_match(want, m)
        {
            // Copy/cut/paste arrive as clipboard events on most platforms.
            if matches!(item.id, "edit.copy" | "edit.cut" | "edit.paste") {
                return true;
            }
            if repeat && !matches!(item.id, "edit.undo" | "edit.redo" | "view.zoomIn" | "view.zoomOut") {
                return true;
            }
            if item.id == "select.none" {
                if let Err(e) = app.session.escape() {
                    app.toast(e.to_string());
                }
                app.ui.context_menu = None;
                return true;
            }
            if item.id == "edit.delete" {
                // Delete key also removes the pen path / ends gestures.
                if app.session.tool.pen.is_some() {
                    let _ = app.session.pen_finish();
                    return true;
                }
            }
            if item.id == "select.children" && enter_text(app) {
                return true;
            }
            crate::menus::invoke(app, item.id);
            return true;
        }
    }
    // Delete also via the Delete key.
    if key == Key::Delete && m.is_none() {
        app.run("edit.delete", json!({}));
        return true;
    }
    // Arrow nudges.
    let (dx, dy) = match key {
        Key::ArrowLeft => (-1.0, 0.0),
        Key::ArrowRight => (1.0, 0.0),
        Key::ArrowUp => (0.0, -1.0),
        Key::ArrowDown => (0.0, 1.0),
        _ => (0.0, 0.0),
    };
    if (dx, dy) != (0.0, 0.0) && !m.alt {
        if !app.session.selection().is_empty() {
            if m.command {
                // Cmd/Ctrl+arrows resize: right/down grow, left/up shrink.
                app.run("transform.nudgeSize", json!({"dw": dx, "dh": dy, "big": m.shift}));
            } else {
                app.run("transform.nudge", json!({"dx": dx, "dy": dy, "big": m.shift}));
            }
        }
        return true;
    }
    // Number keys: opacity.
    if app.session.prefs.number_keys_opacity && m.is_none() && !app.session.selection().is_empty() {
        let digit = match key {
            Key::Num0 => Some(1.0),
            Key::Num1 => Some(0.1),
            Key::Num2 => Some(0.2),
            Key::Num3 => Some(0.3),
            Key::Num4 => Some(0.4),
            Key::Num5 => Some(0.5),
            Key::Num6 => Some(0.6),
            Key::Num7 => Some(0.7),
            Key::Num8 => Some(0.8),
            Key::Num9 => Some(0.9),
            _ => None,
        };
        if let Some(o) = digit {
            let ids = app.session.selection();
            app.set_props(&ids, json!({"opacity": o}));
            return true;
        }
    }
    if let Some((want, k)) = parse(crate::present::PRESENT_SHORTCUT)
        && k == key
        && mods_match(want, m)
    {
        app.run("proto.present", json!({}));
        return true;
    }
    // Enter while the pen is drawing finishes it.
    if key == Key::Enter && app.session.tool.pen.is_some() {
        let _ = app.session.pen_finish();
        return true;
    }
    false
}

/// Enter on a single selected text layer starts editing it.
fn enter_text(app: &mut FigApp) -> bool {
    let sel = app.session.selection();
    if sel.len() != 1 {
        return false;
    }
    let Some(id) = sel.first().copied() else { return false };
    let kind = app.session.doc().ok().and_then(|d| d.node(id)).map(|n| (matches!(n.kind, figcraft_doc::NodeKind::Text(_)), n.kind.is_shape()));
    match kind {
        Some((true, _)) => {
            let _ = figcraft_engine::textedit::begin(&mut app.session, id, None, false);
            true
        }
        Some((_, true)) => {
            app.run("vector.edit.begin", json!({"id": id}));
            true
        }
        _ => false,
    }
}

fn text_keys(app: &mut FigApp, ctx: &Context, events: &[Event]) {
    // A key that ran a shortcut swallows the text it also produced (Alt+. types "≥" on macOS).
    let mut skip_text = false;
    for ev in events {
        match ev {
            Event::Text(t) | Event::Ime(egui::ImeEvent::Commit(t)) => {
                if skip_text && matches!(ev, Event::Text(_)) {
                    skip_text = false;
                    continue;
                }
                if let Err(e) = figcraft_engine::textedit::insert(&mut app.session, t) {
                    app.toast(e.to_string());
                }
            }
            // The composition shows inline at the caret; the document gets it on Commit.
            Event::Ime(egui::ImeEvent::Preedit { text, active_range_chars }) => {
                let cursor = active_range_chars.as_ref().map(|r| r.start);
                if let Err(e) = figcraft_engine::textedit::preedit(&mut app.session, text, cursor) {
                    app.toast(e.to_string());
                }
            }
            Event::Paste(t) => {
                // Plain text only: the engine normalises line endings and drops control chars.
                if let Err(e) = figcraft_engine::textedit::insert(&mut app.session, t) {
                    app.toast(e.to_string());
                }
            }
            Event::Copy | Event::Cut => {
                if let Some(text) = figcraft_engine::textedit::selected_text(&app.session).filter(|t| !t.is_empty()) {
                    app.services.clipboard_set_text(&text);
                    if matches!(ev, Event::Cut) {
                        let _ = figcraft_engine::textedit::key(&mut app.session, "Delete", false, false, false);
                    }
                }
            }
            Event::Key { key, pressed: true, modifiers: m, .. } => {
                if (m.command || m.alt) && text_shortcut(app, *key, *m) {
                    skip_text = true;
                    continue;
                }
                let name = match key {
                    Key::Backspace => "Backspace",
                    Key::Delete => "Delete",
                    Key::ArrowLeft => "Left",
                    Key::ArrowRight => "Right",
                    Key::ArrowUp => "Up",
                    Key::ArrowDown => "Down",
                    Key::Home => "Home",
                    Key::End => "End",
                    Key::Enter => "Enter",
                    Key::Escape => "Escape",
                    Key::Tab => "Tab",
                    Key::A if m.command => "SelectAll",
                    Key::Z if m.command => {
                        let _ = figcraft_engine::textedit::end(&mut app.session);
                        app.run(if m.shift { "edit.redo" } else { "edit.undo" }, json!({}));
                        continue;
                    }
                    _ => continue,
                };
                if name == "Enter" && m.command {
                    let _ = figcraft_engine::textedit::end(&mut app.session);
                    continue;
                }
                if let Err(e) = figcraft_engine::textedit::key(&mut app.session, name, m.shift, m.alt, m.command) {
                    app.toast(e.to_string());
                }
            }
            _ => {}
        }
    }
    // Triple-click on the edited text selects its line (double-click is handled by the engine).
    if ctx.input(|i| i.pointer.button_triple_clicked(egui::PointerButton::Primary))
        && let Some(pos) = ctx.input(|i| i.pointer.interact_pos())
        && app.canvas.rect.contains(pos)
        && let Some(te) = app.session.ds().ok().and_then(|d| d.text_edit.clone())
    {
        let p = app.to_page(pos);
        let inside = app.session.doc().ok().and_then(|d| d.world_bounds(te.id)).is_some_and(|b| b.contains(p));
        if inside && let Ok(idx) = app.session.text_index_at(te.id, p) {
            app.run("text.edit.selectLine", json!({"index": idx}));
        }
    }
    app.sync_preedit();
    ime_output(app, ctx);
}

/// Text-menu shortcuts (Bold, lists, alignment, size/weight/spacing steps…) while editing text.
fn text_shortcut(app: &mut FigApp, key: Key, m: Modifiers) -> bool {
    for item in MENU.iter().filter(|i| i.id.starts_with("text.") && !i.shortcut.is_empty()) {
        if let Some((want, k)) = parse(item.shortcut)
            && k == key
            && mods_match(want, m)
        {
            crate::menus::invoke(app, item.id);
            return true;
        }
    }
    false
}

/// Tell the platform where the caret is (inside the composition while composing) so IME
/// candidate windows open next to it.
fn ime_output(app: &FigApp, ctx: &Context) {
    let Some(te) = app.session.ds().ok().and_then(|d| d.text_edit.clone()) else { return };
    let preedit = app.preedit_view();
    let Some(doc) = preedit.map(|p| &*p.doc).or_else(|| app.session.doc().ok()) else { return };
    let Some(node) = doc.node(te.id) else { return };
    let Some((layout, dy)) = figcraft_text::layout_node(node) else { return };
    let w = doc.world_transform(te.id);
    let (cx, li) = layout.caret(preedit.map(|p| p.cursor).unwrap_or(te.caret));
    let (top, h) = layout.lines.get(li).map(|l| (l.top, l.height)).unwrap_or((0.0, 14.0));
    let p0 = w * figcraft_geom::Point::new(cx, top + dy);
    let p1 = w * figcraft_geom::Point::new(cx, top + h + dy);
    let (a, b) = (app.to_screen(p0.x, p0.y), app.to_screen(p1.x, p1.y));
    if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
        return;
    }
    let cursor_rect = egui::Rect::from_two_pos(a, b + egui::vec2(1.0, 0.0));
    ctx.output_mut(|o| {
        o.ime = Some(egui::output::IMEOutput {
            purpose: egui::IMEPurpose::Normal,
            rect: app.canvas.rect,
            cursor_rect,
            should_interrupt_composition: false,
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_catalog_shortcuts() {
        let (m, k) = parse("Shift+Cmd+Z").unwrap();
        assert!(m.shift && m.command && k == Key::Z);
        assert_eq!(parse("Alt+Cmd+]").map(|x| x.1), Some(Key::CloseBracket));
        // Every catalog shortcut parses.
        for item in MENU.iter().filter(|i| !i.shortcut.is_empty()) {
            assert!(parse(item.shortcut).is_some(), "{}", item.shortcut);
        }
    }
}
