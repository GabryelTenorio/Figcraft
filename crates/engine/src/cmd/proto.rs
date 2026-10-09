//! Prototype commands: interactions, flows, settings, presenting.

use figcraft_doc::proto::*;
use figcraft_doc::*;
use serde_json::{Value, json};

use super::{CommandSpec, ok};
use crate::params::*;
use crate::{Result, Session, bad, spec};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "proto.interaction.add",
            "Add interaction",
            "{id?, trigger? {type: onClick|onHover|onPress|onDrag|mouseEnter|mouseLeave|mouseDown|mouseUp|afterDelay {ms}|keyPress {key}}, action {type: navigate|back|scrollTo|openOverlay {position?, close_on_outside?, background?}|swapOverlay|closeOverlay|openUrl {url}|changeTo|setVariable {variable, value}, dest?}, actions? [..], transition? {type: instant|dissolve|smartAnimate|moveIn|moveOut|push|slideIn|slideOut, direction?: left|right|top|bottom}, durationMs?, easing? linear|easeIn|easeOut|easeInOut|easeInBack|easeOutBack|easeInOutBack|gentle|quick|bouncy|slow|{customBezier {x1, y1, x2, y2}}|{customSpring {mass, stiffness, damping}}} — or {id, dest} for a click-navigate. Returns {index}",
            true,
            add
        ),
        spec!("proto.interaction.remove", "Remove interaction", "{id?, index}", true, |s, p| {
            let cmd = "proto.interaction.remove";
            let k = req_num(cmd, p, "index")?;
            if k < 0.0 {
                return Err(bad(cmd, "`index` must be ≥ 0"));
            }
            let k = k as usize;
            let mut hit = false;
            for i in req_targets(cmd, s, p)? {
                let n = s.doc_mut()?.get_mut(i)?;
                if k < n.interactions.len() {
                    n.interactions.remove(k);
                    hit = true;
                }
            }
            if !hit {
                return Err(bad(cmd, format!("no interaction {k}")));
            }
            ok()
        }),
        spec!(
            "proto.interaction.set",
            "Edit interaction",
            "{id?, index, interaction {..}} replaces it; or patch fields: {id?, index, trigger?, action? (first action), actions?, transition?, durationMs?, easing?}",
            true,
            set
        ),
        spec!("proto.flow.add", "Add flow starting point", "{start? (top-level frame id; default selection), name?}", true, |s, p| {
            let cmd = "proto.flow.add";
            let start = id(p, "start").or_else(|| s.selection().first().copied()).ok_or_else(|| bad(cmd, "select a top-level frame"))?;
            let page = s.page()?;
            let doc = s.doc_mut()?;
            if doc.top_level_frame(start) != Some(start) {
                return Err(bad(cmd, "a flow starts at a top-level frame"));
            }
            let flows = match &doc.get(page)?.kind {
                NodeKind::Page { flows, .. } => flows.clone(),
                _ => vec![],
            };
            if let Some(f) = flows.iter().find(|f| f.start == start) {
                return Ok(json!({"name": f.name, "start": start}));
            }
            let name = string(p, "name").filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
                let mut k = flows.len() + 1;
                while flows.iter().any(|f| f.name == format!("Flow {k}")) {
                    k += 1;
                }
                format!("Flow {k}")
            });
            if let NodeKind::Page { flows, .. } = &mut doc.get_mut(page)?.kind {
                flows.push(Flow { name: name.clone(), start, description: String::new() });
            }
            Ok(json!({"name": name, "start": start}))
        }),
        spec!("proto.flow.rename", "Rename flow", "{start, name?, description?}", true, |s, p| {
            let cmd = "proto.flow.rename";
            let start = req_id(cmd, p, "start")?;
            let name = string(p, "name").map(|n| n.trim().to_string());
            if name.as_deref() == Some("") {
                return Err(bad(cmd, "`name` can't be empty"));
            }
            let desc = string(p, "description");
            let page = s.page()?;
            let NodeKind::Page { flows, .. } = &mut s.doc_mut()?.get_mut(page)?.kind else { return Err(bad(cmd, "not a page")) };
            let f = flows.iter_mut().find(|f| f.start == start).ok_or_else(|| bad(cmd, "no flow starts at that frame"))?;
            if let Some(n) = name {
                f.name = n;
            }
            if let Some(d) = desc {
                f.description = d;
            }
            ok()
        }),
        spec!("proto.flow.remove", "Remove flow", "{start}", true, |s, p| {
            let start = req_id("proto.flow.remove", p, "start")?;
            let page = s.page()?;
            if let NodeKind::Page { flows, .. } = &mut s.doc_mut()?.get_mut(page)?.kind {
                flows.retain(|f| f.start != start);
            }
            ok()
        }),
        spec!("proto.settings", "Prototype settings", "{device?, background?}", true, |s, p| {
            let page = s.page()?;
            let bg = color(p, "background");
            let dev = string(p, "device");
            if let NodeKind::Page { prototype, .. } = &mut s.doc_mut()?.get_mut(page)?.kind {
                if let Some(c) = bg {
                    prototype.background = c;
                }
                if let Some(d) = dev {
                    prototype.device = d;
                }
            }
            ok()
        }),
        spec!("proto.present", "Present", "{start? (frame id)}", false, |s, p| {
            let start = id(p, "start").or_else(|| {
                let ds = s.ds().ok()?;
                ds.selection
                    .first()
                    .and_then(|i| ds.doc.top_level_frame(*i))
                    .or_else(|| match &ds.doc.node(ds.page)?.kind {
                        NodeKind::Page { flows, .. } => flows.first().map(|f| f.start),
                        _ => None,
                    })
                    .or_else(|| ds.doc.top_frames(ds.page).first().copied())
            });
            let start = start.ok_or_else(|| bad("proto.present", "no frame to present"))?;
            s.request_ui("present", json!({"start": start}));
            Ok(json!({"start": start}))
        }),
    ]
}

/// Every node an action points at must exist.
fn check_dests(cmd: &str, s: &Session, it: &Interaction) -> Result<()> {
    let doc = s.doc()?;
    for a in &it.actions {
        if let Some(d) = a.dest()
            && !doc.contains(d)
        {
            return Err(bad(cmd, format!("destination {d} doesn't exist")));
        }
    }
    Ok(())
}

fn duration(p: &Value) -> Option<u32> {
    num(p, "durationMs").or_else(|| num(p, "duration_ms")).map(|d| d.clamp(0.0, 60_000.0) as u32)
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.interaction.add";
    let it = if let Some(d) = id(p, "dest")
        && get(p, "action").is_none()
    {
        Interaction {
            trigger: parse(cmd, p, "trigger")?.unwrap_or_default(),
            actions: vec![Action::Navigate { dest: d }],
            transition: parse(cmd, p, "transition")?.unwrap_or_default(),
            duration_ms: duration(p).unwrap_or(300),
            easing: parse(cmd, p, "easing")?.unwrap_or_default(),
        }
    } else {
        let trigger: Trigger = parse(cmd, p, "trigger")?.unwrap_or_default();
        let actions: Vec<Action> = match parse::<Vec<Action>>(cmd, p, "actions")? {
            Some(a) if !a.is_empty() => a,
            _ => vec![parse(cmd, p, "action")?.ok_or_else(|| bad(cmd, "`action` required"))?],
        };
        let transition: Transition = parse(cmd, p, "transition")?.unwrap_or_default();
        let easing: Easing = parse(cmd, p, "easing")?.unwrap_or_default();
        Interaction { trigger, actions, transition, duration_ms: duration(p).unwrap_or(300), easing }
    };
    check_dests(cmd, s, &it)?;
    let mut index = 0;
    for i in req_targets(cmd, s, p)? {
        let n = s.doc_mut()?.get_mut(i)?;
        n.interactions.push(it.clone());
        index = n.interactions.len() - 1;
    }
    Ok(json!({"index": index}))
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.interaction.set";
    let k = req_num(cmd, p, "index")?;
    if k < 0.0 {
        return Err(bad(cmd, "`index` must be ≥ 0"));
    }
    let k = k as usize;
    let whole: Option<Interaction> = parse(cmd, p, "interaction")?;
    let trigger: Option<Trigger> = parse(cmd, p, "trigger")?;
    let action: Option<Action> = parse(cmd, p, "action")?;
    let actions: Option<Vec<Action>> = parse(cmd, p, "actions")?;
    let transition: Option<Transition> = parse(cmd, p, "transition")?;
    let easing: Option<Easing> = parse(cmd, p, "easing")?;
    let dur = duration(p);
    let mut hit = false;
    for i in req_targets(cmd, s, p)? {
        let Some(cur) = s.doc()?.get(i)?.interactions.get(k).cloned() else { continue };
        let mut it = whole.clone().unwrap_or(cur);
        if let Some(t) = &trigger {
            it.trigger = t.clone();
        }
        if let Some(a) = &actions {
            it.actions = a.clone();
        }
        if let Some(a) = &action {
            match it.actions.first_mut() {
                Some(slot) => *slot = a.clone(),
                None => it.actions.push(a.clone()),
            }
        }
        if let Some(t) = transition {
            it.transition = t;
        }
        if let Some(e) = easing {
            it.easing = e;
        }
        if let Some(d) = dur {
            it.duration_ms = d;
        }
        if it.actions.is_empty() {
            return Err(bad(cmd, "an interaction needs an action"));
        }
        check_dests(cmd, s, &it)?;
        if let Some(slot) = s.doc_mut()?.get_mut(i)?.interactions.get_mut(k) {
            *slot = it;
            hit = true;
        }
    }
    if !hit {
        return Err(bad(cmd, format!("no interaction {k}")));
    }
    ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::id_of;

    fn frame(s: &mut Session, x: f64) -> NodeId {
        let r = s.execute("node.create", &json!({"type": "frame", "x": x, "y": 0, "width": 100, "height": 200})).unwrap();
        id_of(&r["id"]).unwrap()
    }

    fn page_flows(s: &Session) -> Vec<Flow> {
        let ds = s.ds().unwrap();
        match &ds.doc.node(ds.page).unwrap().kind {
            NodeKind::Page { flows, .. } => flows.clone(),
            _ => vec![],
        }
    }

    #[test]
    fn add_set_patch_remove() {
        let mut s = Session::new();
        let a = frame(&mut s, 0.0);
        let b = frame(&mut s, 200.0);
        let r = s.execute("proto.interaction.add", &json!({"id": a, "dest": b})).unwrap();
        assert_eq!(r["index"], 0);
        let r = s
            .execute(
                "proto.interaction.add",
                &json!({"id": a, "trigger": {"type": "keyPress", "key": "Space"}, "action": {"type": "back"}, "transition": {"type": "moveIn"}, "durationMs": 450, "easing": "bouncy"}),
            )
            .unwrap();
        assert_eq!(r["index"], 1);
        let it = s.doc().unwrap().node(a).unwrap().interactions[1].clone();
        assert_eq!(it.trigger, Trigger::KeyPress { key: "Space".into() });
        assert_eq!(it.transition, Transition::MoveIn { direction: Direction::Left });
        assert_eq!(it.duration_ms, 450);
        assert_eq!(it.easing, Easing::Bouncy);
        // Patch only the transition and easing; the action stays.
        s.execute("proto.interaction.set", &json!({"id": a, "index": 0, "transition": {"type": "push", "direction": "top"}, "easing": "linear"}))
            .unwrap();
        let it = s.doc().unwrap().node(a).unwrap().interactions[0].clone();
        assert_eq!(it.actions, vec![Action::Navigate { dest: b }]);
        assert_eq!(it.transition, Transition::Push { direction: Direction::Top });
        assert_eq!(it.easing, Easing::Linear);
        // Overlay action with settings; camelCase accepted.
        s.execute(
            "proto.interaction.set",
            &json!({"id": a, "index": 0, "action": {"type": "openOverlay", "dest": b, "position": "bottomCenter", "closeOnOutside": true, "background": "#00000040"}}),
        )
        .unwrap();
        let it = s.doc().unwrap().node(a).unwrap().interactions[0].clone();
        let Action::OpenOverlay { position, close_on_outside, background, .. } = &it.actions[0] else { panic!("{it:?}") };
        assert_eq!(*position, OverlayPosition::BottomCenter);
        assert!(*close_on_outside);
        assert!(background.is_some());
        // Whole replacement round-trips through serde.
        let mut whole = it.clone();
        whole.trigger = Trigger::AfterDelay { ms: 800 };
        s.execute("proto.interaction.set", &json!({"id": a, "index": 0, "interaction": whole})).unwrap();
        assert_eq!(s.doc().unwrap().node(a).unwrap().interactions[0].trigger, Trigger::AfterDelay { ms: 800 });
        // Errors leave the document untouched.
        assert!(s.execute("proto.interaction.set", &json!({"id": a, "index": 9, "easing": "linear"})).is_err());
        assert!(s.execute("proto.interaction.set", &json!({"id": a, "index": 0, "action": {"type": "navigate", "dest": 99999}})).is_err());
        assert!(s.execute("proto.interaction.add", &json!({"id": a, "action": {"type": "navigate", "dest": 99999}})).is_err());
        assert!(s.execute("proto.interaction.set", &json!({"id": a, "index": 0, "actions": []})).is_err());
        assert!(s.execute("proto.interaction.remove", &json!({"id": a, "index": -1})).is_err());
        assert_eq!(s.doc().unwrap().node(a).unwrap().interactions.len(), 2);
        s.execute("proto.interaction.remove", &json!({"id": a, "index": 0})).unwrap();
        assert_eq!(s.doc().unwrap().node(a).unwrap().interactions.len(), 1);
        assert!(s.undo().unwrap());
        assert_eq!(s.doc().unwrap().node(a).unwrap().interactions.len(), 2);
    }

    #[test]
    fn flows_add_rename_remove() {
        let mut s = Session::new();
        let a = frame(&mut s, 0.0);
        let b = frame(&mut s, 200.0);
        let r = s.execute("proto.flow.add", &json!({"start": a})).unwrap();
        assert_eq!(r["name"], "Flow 1");
        // Adding again keeps the existing flow.
        s.execute("proto.flow.add", &json!({"start": a, "name": "Other"})).unwrap();
        assert_eq!(page_flows(&s).len(), 1);
        s.execute("proto.flow.add", &json!({"start": b})).unwrap();
        s.execute("proto.flow.rename", &json!({"start": b, "name": "Checkout", "description": "Pay"})).unwrap();
        let flows = page_flows(&s);
        assert_eq!(flows.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Flow 1", "Checkout"]);
        assert_eq!(flows[1].description, "Pay");
        assert!(s.execute("proto.flow.rename", &json!({"start": b, "name": "  "})).is_err());
        assert!(s.execute("proto.flow.rename", &json!({"start": 4242, "name": "x"})).is_err());
        // Only top-level frames start flows.
        let r = s.execute("node.create", &json!({"type": "rectangle", "parent": a, "x": 5, "y": 5, "width": 10, "height": 10})).unwrap();
        let child = id_of(&r["id"]).unwrap();
        assert!(s.execute("proto.flow.add", &json!({"start": child})).is_err());
        s.execute("proto.flow.remove", &json!({"start": a})).unwrap();
        assert_eq!(page_flows(&s).len(), 1);
        // A new flow gets an unused default name.
        let r = s.execute("proto.flow.add", &json!({"start": a})).unwrap();
        assert_eq!(r["name"], "Flow 2");
    }

    #[test]
    fn scroll_props_via_node_set() {
        let mut s = Session::new();
        let a = frame(&mut s, 0.0);
        s.execute("node.set", &json!({"id": a, "props": {"overflow": "vertical", "scrollPosition": "fixed"}})).unwrap();
        let n = s.doc().unwrap().node(a).unwrap().clone();
        assert_eq!(n.overflow, Overflow::Vertical);
        assert_eq!(n.scroll_position, ScrollPosition::Fixed);
    }
}
