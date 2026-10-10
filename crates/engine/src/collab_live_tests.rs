//! Documents in a live session, through the engine: commands, gestures and groups sync as one
//! step each, `edit.undo` / `edit.redo` take back only the user's own steps, others' edits show
//! under a gesture in progress, refused edits come back with a toast, and outside a session undo
//! is unchanged.

use std::collections::VecDeque;

use figcraft_collab::sim::Rng;
use figcraft_collab::{Client, ClientMsg, Limits, Outgoing, Server, ServerMsg, message};
use figcraft_doc::{Document, NodeId};
use figcraft_geom::Affine;
use serde_json::json;

use crate::Session;
use crate::collab_tests::{base, random_command};

/// A server and several people, each an engine session with the shared document attached, over
/// links that deliver in order (messages go through the binary encoding).
struct Room {
    server: Server,
    people: Vec<(Session, u64)>,
    up: Vec<VecDeque<ClientMsg>>,
    down: Vec<VecDeque<ServerMsg>>,
    rng: Rng,
}

impl Room {
    fn new(doc: Document, n: usize, seed: u64) -> Room {
        let mut room =
            Room { server: Server::new(doc, "", Limits::default()), people: Vec::new(), up: Vec::new(), down: Vec::new(), rng: Rng::new(seed) };
        for _ in 0..n {
            let ClientMsg::Hello(h) = Client::hello("f", "", "Person", "#3366FF") else { panic!("not a hello") };
            let (id, out) = room.server.join(h).unwrap();
            let mut welcome = None;
            let mut rest = Vec::new();
            for o in out {
                match o {
                    Outgoing::To(to, m) if to == id => welcome = Some(m),
                    o => rest.push(o),
                }
            }
            let mut s = Session::new();
            let uid = s.collab_attach(Client::join(welcome.unwrap()).unwrap()).unwrap();
            room.people.push((s, uid));
            room.up.push(VecDeque::new());
            room.down.push(VecDeque::new());
            room.route(rest);
        }
        room
    }

    fn id(&self, i: usize) -> u32 {
        let (s, uid) = &self.people[i];
        s.collab_client(*uid).unwrap().id()
    }

    fn session(&mut self, i: usize) -> &mut Session {
        &mut self.people[i].0
    }

    fn route(&mut self, out: Vec<Outgoing>) {
        for o in out {
            let ids: Vec<u32> = (0..self.people.len()).map(|i| self.id(i)).collect();
            let (msg, to): (ServerMsg, Vec<usize>) = match o {
                Outgoing::To(id, m) => (m, (0..ids.len()).filter(|i| ids[*i] == id).collect()),
                Outgoing::All(m) => (m, (0..ids.len()).collect()),
                Outgoing::Others(id, m) => (m, (0..ids.len()).filter(|i| ids[*i] != id).collect()),
            };
            for i in to {
                let wire: ServerMsg = message::decode(&message::encode(&msg).unwrap(), usize::MAX).unwrap();
                self.down[i].push_back(wire);
            }
        }
    }

    /// Collect what everyone has to send.
    fn pump(&mut self) {
        for i in 0..self.people.len() {
            let (s, uid) = &mut self.people[i];
            let msgs = s.collab_outgoing(*uid);
            self.up[i].extend(msgs);
        }
    }

    /// Deliver one message from a random busy link. False when nothing is in flight.
    fn step(&mut self) -> bool {
        self.pump();
        let busy: Vec<(bool, usize)> = (0..self.people.len())
            .flat_map(|i| [(true, i), (false, i)])
            .filter(|(up, i)| if *up { !self.up[*i].is_empty() } else { !self.down[*i].is_empty() })
            .collect();
        let Some(&(up, i)) = self.rng.pick(&busy) else { return false };
        if up {
            let msg = self.up[i].pop_front().unwrap();
            let msg: ClientMsg = message::decode(&message::encode(&msg).unwrap(), usize::MAX).unwrap();
            let out = self.server.receive(self.id(i), msg);
            self.route(out);
        } else {
            let msg = self.down[i].pop_front().unwrap();
            let (s, uid) = &mut self.people[i];
            s.collab_receive(*uid, msg).unwrap();
        }
        self.pump();
        true
    }

    /// Person `i`'s connection drops (what is in flight is lost).
    fn disconnect(&mut self, i: usize) {
        self.up[i].clear();
        self.down[i].clear();
        let out = self.server.leave(self.id(i));
        self.route(out);
        let (s, uid) = &mut self.people[i];
        s.collab_disconnected(*uid);
    }

    /// Person `i` connects again.
    fn reconnect(&mut self, i: usize) {
        let ClientMsg::Hello(hello) = Client::hello("f", "", "Person", "#3366FF") else { panic!("not a hello") };
        let (s, uid) = &mut self.people[i];
        let Some(ClientMsg::Hello(h)) = s.collab_reconnect(*uid, hello) else { panic!("no hello") };
        let (_, out) = self.server.join(h).unwrap();
        self.route(out);
    }

    fn settle(&mut self) {
        let mut n = 0;
        while self.step() {
            n += 1;
            assert!(n < 1_000_000, "still busy");
        }
    }

    /// Everyone holds the server's document, in the session and on the canvas.
    fn check(&self) {
        for (i, (s, uid)) in self.people.iter().enumerate() {
            let c = s.collab_client(*uid).unwrap();
            assert_eq!(c.pending(), 0, "person {i}");
            assert!(c.view() == self.server.replica(), "person {i}: {}", figcraft_collab::sim::describe(self.server.replica(), c.view()));
            assert!(*s.doc().unwrap() == *self.server.document(), "person {i}: the canvas shows the session's document");
        }
    }
}

/// The shared file's first frame and one of its rectangles.
fn frame_and_rect(doc: &Document) -> (NodeId, NodeId) {
    let page = doc.first_page().unwrap();
    let f = doc.node(page).unwrap().children[0];
    (f, doc.node(f).unwrap().children[0])
}

#[test]
fn edits_sync_and_undo_takes_back_only_your_own() {
    let mut room = Room::new(base(), 2, 1);
    let (f, r) = frame_and_rect(room.server.document());
    let old = room.server.document().node(r).unwrap().name.clone();
    room.session(0).execute("node.set", &json!({"id": r, "props": {"name": "Mine"}})).unwrap();
    room.settle();
    room.session(1).execute("transform.move", &json!({"ids": [r], "dx": 25, "dy": 0})).unwrap();
    room.settle();
    room.check();
    let moved = room.server.document().node(r).unwrap().transform;
    // Person 0 undoes: their rename goes, person 1's later move stays.
    assert_eq!(room.session(0).execute("edit.undo", &json!({})).unwrap()["undone"], true);
    room.settle();
    room.check();
    let n = room.server.document().node(r).unwrap();
    assert_eq!((&n.name, n.transform), (&old, moved));
    assert_eq!(room.session(0).status()["redo"], 1);
    // Nothing left to undo for person 0; person 1 still has its move (remote edits never enter
    // anyone else's history).
    assert_eq!(room.session(0).execute("edit.undo", &json!({})).unwrap()["undone"], false);
    assert_eq!(room.session(1).status()["undo"], 1);
    room.session(0).execute("edit.redo", &json!({})).unwrap();
    room.settle();
    room.check();
    assert_eq!(room.server.document().node(r).unwrap().name, "Mine");
    // Deleting a frame with layers and undoing it brings it back whole.
    let before = room.server.replica().clone();
    room.session(1).execute("edit.delete", &json!({"ids": [f]})).unwrap();
    room.settle();
    assert!(!room.server.document().contains(f));
    room.session(1).execute("edit.undo", &json!({})).unwrap();
    room.settle();
    room.check();
    assert!(room.server.replica() == &before);
}

#[test]
fn a_gesture_is_one_step_and_carries_on_over_remote_edits() {
    let mut room = Room::new(base(), 2, 2);
    let (_, r) = frame_and_rect(room.server.document());
    let start = room.server.document().node(r).unwrap().transform;
    let s = room.session(0);
    s.begin_interaction("Move").unwrap();
    for dx in [5.0, 10.0, 15.0] {
        s.preview(|doc, _| {
            if let Some(n) = doc.node_mut(r) {
                n.transform = Affine::translate((dx, 0.0)) * start;
            }
            Ok(())
        })
        .unwrap();
    }
    // Nothing goes out during the gesture; someone else renames the layer meanwhile.
    room.pump();
    assert!(room.up[0].is_empty());
    room.session(1).execute("node.set", &json!({"id": r, "props": {"name": "Theirs"}})).unwrap();
    room.settle();
    let shown = room.session(0).doc().unwrap().node(r).unwrap().clone();
    assert_eq!((shown.name.as_str(), shown.transform), ("Theirs", Affine::translate((15.0, 0.0)) * start), "both show");
    room.session(0).commit_interaction().unwrap();
    room.settle();
    room.check();
    assert_eq!(room.session(0).status()["undo"], 1, "one step for the whole drag");
    room.session(0).execute("edit.undo", &json!({})).unwrap();
    room.settle();
    room.check();
    let n = room.server.document().node(r).unwrap();
    assert_eq!((n.name.as_str(), n.transform), ("Theirs", start));
}

#[test]
fn a_group_of_commands_syncs_as_one_step() {
    let mut room = Room::new(base(), 2, 3);
    let (f, r) = frame_and_rect(room.server.document());
    let start = room.server.replica().clone();
    let cmds = vec![
        ("node.set".to_string(), json!({"id": r, "props": {"name": "A"}})),
        ("node.set".to_string(), json!({"id": f, "props": {"name": "B"}})),
        ("shape.rectangle".to_string(), json!({"x": 0, "y": 0, "width": 5, "height": 5})),
    ];
    room.session(0).batch("Three", &cmds).unwrap();
    let uid = room.people[0].1;
    assert_eq!(room.session(0).collab_client(uid).unwrap().pending(), 1, "one batch");
    room.settle();
    room.check();
    assert_eq!(room.session(0).status()["undo"], 1);
    room.session(0).execute("edit.undo", &json!({})).unwrap();
    room.settle();
    room.check();
    assert!(room.server.replica() == &start, "all three are taken back");
}

#[test]
fn an_undo_waits_for_its_edit_and_shows_at_once() {
    let mut room = Room::new(base(), 2, 6);
    let (_, r) = frame_and_rect(room.server.document());
    let old = room.server.document().node(r).unwrap().name.clone();
    let start = room.server.replica().clone();
    room.session(0).execute("node.set", &json!({"id": r, "props": {"name": "Quick"}})).unwrap();
    room.session(0).execute("edit.undo", &json!({})).unwrap();
    assert_eq!(room.session(0).doc().unwrap().node(r).unwrap().name, old, "shown at once");
    room.pump();
    assert_eq!(room.up[0].len(), 1, "only the edit goes out until it is confirmed");
    room.settle();
    room.check();
    assert!(room.server.replica() == &start);
}

#[test]
fn a_refused_edit_is_rolled_back_with_a_toast() {
    let mut room = Room::new(base(), 2, 4);
    let (f, r) = frame_and_rect(room.server.document());
    // Person 1 deletes the frame; person 0 renames a layer inside it before hearing of it.
    room.session(1).execute("edit.delete", &json!({"ids": [f]})).unwrap();
    room.pump();
    let msg = room.up[1].pop_front().unwrap();
    let out = room.server.receive(room.id(1), msg);
    room.route(out);
    room.session(0).execute("node.set", &json!({"id": r, "props": {"name": "Too late"}})).unwrap();
    room.settle();
    room.check();
    let s = room.session(0);
    assert!(s.message.as_deref().unwrap_or_default().contains("deleted by someone else"), "{:?}", s.message);
    assert_eq!(s.status()["undo"], 0, "no undo step for refused work");
}

#[test]
fn random_commands_with_undo_converge_through_the_engine() {
    for seed in 0..4 {
        let mut room = Room::new(base(), 3, seed);
        let mut rng = Rng::new(seed + 100);
        for _ in 0..60 {
            let i = rng.below(3);
            let (cmd, params) = random_command(room.people[i].0.doc().unwrap(), &mut rng, true);
            let _ = room.session(i).execute(cmd, &params);
            for _ in 0..rng.below(4) {
                room.step();
            }
        }
        room.settle();
        room.check();
    }
}

#[test]
fn a_dropped_connection_keeps_editing_and_catches_up() {
    let mut room = Room::new(base(), 2, 7);
    let (f, r) = frame_and_rect(room.server.document());
    room.session(0).execute("node.set", &json!({"id": r, "props": {"name": "Sent"}})).unwrap();
    room.pump();
    room.disconnect(0);
    // Offline: edits and undo keep working; nothing goes out.
    room.session(0).execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 5, "height": 5})).unwrap();
    room.session(0).execute("edit.undo", &json!({})).unwrap();
    room.session(0).execute("node.set", &json!({"id": f, "props": {"name": "Offline"}})).unwrap();
    room.pump();
    assert!(room.up[0].is_empty());
    room.session(1).execute("transform.move", &json!({"ids": [r], "dx": 5, "dy": 5})).unwrap();
    room.settle();
    room.reconnect(0);
    room.settle();
    room.check();
    let doc = room.server.document();
    assert_eq!((doc.node(r).unwrap().name.as_str(), doc.node(f).unwrap().name.as_str()), ("Sent", "Offline"));
}

#[test]
fn outside_a_session_undo_is_unchanged() {
    let mut room = Room::new(base(), 1, 5);
    let uid = room.people[0].1;
    let (_, r) = frame_and_rect(room.server.document());
    room.session(0).execute("node.set", &json!({"id": r, "props": {"name": "Live"}})).unwrap();
    room.settle();
    let s = room.session(0);
    assert!(s.execute("collab.diff", &json!({})).is_err(), "undo is per user in a live session");
    assert!(s.execute("collab.diff", &json!({"against": "saved"})).is_ok());
    assert!(s.collab_detach(uid).is_some());
    assert!(s.ds().unwrap().collab.is_none());
    assert_eq!(s.status()["undo"], 0, "the live history doesn't carry over");
    s.execute("node.set", &json!({"id": r, "props": {"name": "Solo"}})).unwrap();
    assert!(s.collab_outgoing(uid).is_empty());
    assert_eq!(s.status()["undo"], 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().node(r).unwrap().name, "Live");
}
