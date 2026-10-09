//! Variable bindings through the session: binding, value edits and modes reach the layers.

use serde_json::json;

use crate::Session;
use crate::params::id_of;

#[test]
fn bound_layers_follow_variable_values_and_unbind() {
    let mut s = Session::new();
    let r = id_of(&s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap()["id"]).unwrap();
    s.execute("variable.createCollection", &json!({"name": "Tokens", "modes": ["Light", "Dark"]})).unwrap();
    s.execute("variable.create", &json!({"collection": "Tokens", "name": "size/lg", "type": "number", "value": 48})).unwrap();
    s.execute("variable.create", &json!({"collection": "Tokens", "name": "brand", "type": "color", "value": "#0D99FF"})).unwrap();
    s.execute("variable.bind", &json!({"ids": [r], "field": "width", "variable": "size/lg"})).unwrap();
    s.execute("variable.bind", &json!({"ids": [r], "field": "fill", "variable": "brand"})).unwrap();
    let node = |s: &Session| s.doc().unwrap().node(r).unwrap().clone();
    assert_eq!(node(&s).width, 48.0);
    assert_eq!(node(&s).fills.first().and_then(|p| p.solid_color()).map(|c| c.to_hex()), Some("0D99FF".into()));
    // Editing the variable updates the bound layer.
    s.execute("variable.set", &json!({"variable": "size/lg", "value": 64})).unwrap();
    assert_eq!(node(&s).width, 64.0);
    // A mode with another value applies through the page.
    s.execute("variable.set", &json!({"variable": "size/lg", "mode": "Dark", "value": 80})).unwrap();
    s.execute("variable.setMode", &json!({"collection": "Tokens", "mode": "Dark"})).unwrap();
    assert_eq!(node(&s).width, 80.0);
    // Unbinding keeps the value and stops following.
    s.execute("variable.bind", &json!({"ids": [r], "field": "width", "variable": null})).unwrap();
    s.execute("variable.set", &json!({"variable": "size/lg", "mode": "Dark", "value": 99})).unwrap();
    assert_eq!(node(&s).width, 80.0);
    assert!(s.execute("variable.bind", &json!({"ids": [r], "field": "width", "variable": "nope"})).is_err());
}
