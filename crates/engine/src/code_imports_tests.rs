//! Import lines in generated layer trees: the code for a frame (or a component, or an instance
//! of a component that isn't connected) leads with the imports of every connected instance
//! written into it, one line per import path, names in order and once each, per language;
//! instances that aren't connected, and connected ones the code doesn't reach, bring none.

use crate::Session;
use crate::components::tests::{ex, idv};
use figcraft_doc::*;
use serde_json::json;

fn code(s: &mut Session, id: NodeId, lang: &str) -> String {
    ex(s, "inspect.code", json!({"id": id, "language": lang}))["code"].as_str().unwrap_or_default().to_string()
}

/// A component named `name` at `y`, connected in `lang` to `code_name` from `path` ("" for none).
fn component(s: &mut Session, name: &str, y: f64, links: &[(&str, &str, &str)]) -> NodeId {
    let f = idv(&ex(s, "frame.create", json!({"x": 600, "y": y, "width": 80, "height": 40, "name": name})));
    let c = idv(&ex(s, "component.create", json!({"ids": [f]})));
    for (lang, path, code_name) in links {
        ex(s, "codeConnect.set", json!({"id": c, "language": lang, "importPath": path, "name": code_name, "template": format!("<{code_name} />")}));
    }
    c
}

fn instance(s: &mut Session, component: NodeId, parent: NodeId, x: f64) -> NodeId {
    idv(&ex(s, "instance.create", json!({"component": component, "x": x, "y": 10, "parent": parent})))
}

#[test]
fn a_layer_trees_code_leads_with_the_imports_of_its_connected_instances() {
    let mut s = Session::new();
    let tag = component(&mut s, "Tag", 0.0, &[("react", "@acme/ui", "Tag"), ("swiftui", "AcmeUI", "Tag"), ("compose", "com.acme.ui.Tag", "Tag")]);
    let avatar = component(&mut s, "Avatar", 60.0, &[("react", "@acme/ui", "Avatar"), ("swiftui", "AcmeUI", "Avatar")]);
    let icon = component(&mut s, "Icon", 120.0, &[("react", "@acme/icons", "Icons.Star"), ("compose", "com.acme.icons.Star", "Star")]);
    let local = component(&mut s, "Local", 180.0, &[("react", "", "Local")]);
    let plain = component(&mut s, "Plain", 240.0, &[]);
    let unused = component(&mut s, "Unused", 300.0, &[("react", "@acme/unused", "Unused")]);
    let frame = idv(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 500, "height": 200, "name": "Header"})));
    let group = idv(&ex(&mut s, "frame.create", json!({"x": 300, "y": 0, "width": 180, "height": 100, "name": "Actions", "parent": frame})));
    // Two tags, an icon, an avatar deeper down, and instances without a link or an import path.
    for (c, parent, x) in
        [(tag, frame, 0.0), (icon, frame, 90.0), (tag, frame, 180.0), (avatar, group, 0.0), (local, group, 90.0), (plain, frame, 270.0)]
    {
        instance(&mut s, c, parent, x);
    }
    let react = code(&mut s, frame, "react");
    let want = "import { Icons } from \"@acme/icons\";\nimport { Avatar, Tag } from \"@acme/ui\";\nimport \"./header.css\";\n\n";
    assert!(react.starts_with(want), "{react}");
    assert_eq!(react.matches("@acme/ui").count(), 1, "once, however many instances");
    assert_eq!((react.matches("<Tag />").count(), react.matches("<Avatar />").count(), react.matches("<Icons.Star />").count()), (2, 1, 1));
    assert!(!react.contains("@acme/unused") && !react.contains("import { Local }") && !react.contains("import { Plain }"));
    // Per language: a module or a package each, in order, once.
    let swift = code(&mut s, frame, "swiftui");
    assert!(swift.starts_with("import AcmeUI\n\n") && swift.matches("import ").count() == 1, "{swift}");
    let compose = code(&mut s, frame, "compose");
    assert!(compose.starts_with("import com.acme.icons.Star\nimport com.acme.ui.Tag\n\n") && compose.matches("import ").count() == 2, "{compose}");
    // The part of the tree asked for: only what is in it.
    let part = code(&mut s, group, "react");
    assert!(part.starts_with("import { Avatar } from \"@acme/ui\";\nimport \"./actions.css\";\n\n"), "{part}");
    // CSS and HTML have no imports.
    assert!(!code(&mut s, frame, "css").contains("import") && !code(&mut s, frame, "html").contains("@acme"));
    // A layer tree without connected instances is as it was.
    let bare = idv(&ex(&mut s, "frame.create", json!({"x": 0, "y": 400, "width": 100, "height": 100, "name": "Bare"})));
    instance(&mut s, plain, bare, 0.0);
    assert!(code(&mut s, bare, "react").starts_with("import \"./bare.css\";\n\n"));
    assert!(!code(&mut s, bare, "swiftui").contains("import "));
    let _ = unused;
}

#[test]
fn a_components_definition_brings_the_imports_of_what_it_is_made_of() {
    let mut s = Session::new();
    let tag = component(&mut s, "Tag", 0.0, &[("react", "@acme/ui", "Tag")]);
    let f = idv(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 300, "height": 100, "name": "Card"})));
    instance(&mut s, tag, f, 10.0);
    let card = idv(&ex(&mut s, "component.create", json!({"ids": [f]})));
    // The component, and an instance of it (its call, then the definition): the import leads both.
    let want = "import { Tag } from \"@acme/ui\";\n";
    assert!(code(&mut s, card, "react").starts_with(want));
    let i = idv(&ex(&mut s, "instance.create", json!({"component": card, "x": 0, "y": 300})));
    let of_instance = code(&mut s, i, "react");
    assert!(of_instance.starts_with(&format!("{want}\n<Card />")), "{of_instance}");
    assert_eq!(of_instance.matches("@acme/ui").count(), 1);
    // A connected layer's own code is its import and snippet, as before.
    let t = instance(&mut s, tag, f, 100.0);
    assert_eq!(code(&mut s, t, "react"), "import { Tag } from \"@acme/ui\";\n\n<Tag />\n");
    // Nothing is left collecting: the next layer's code starts clean.
    let empty = idv(&ex(&mut s, "frame.create", json!({"x": 0, "y": 600, "width": 50, "height": 50, "name": "Empty"})));
    assert!(code(&mut s, empty, "react").starts_with("import \"./empty.css\";"));
}
