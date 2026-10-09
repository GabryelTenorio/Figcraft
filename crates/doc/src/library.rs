//! Libraries: a file's published components and styles, and the libraries other files use.
//!
//! **Publishing** records a [`PublishedLibrary`] on the document: a [`LibraryManifest`] (id,
//! version, the components, component sets and styles it offers, each stamped with the library
//! version it last changed in) and a snapshot document holding just those assets and what they
//! depend on. The format crate stores both next to `document.json`, so files using the library
//! only ever see what was published, never later unpublished edits.
//!
//! **Using** a library: a document lists the library files it has enabled ([`LibraryLink`]).
//! Placing a library component copies it (with the components and styles it uses) into the
//! document's *library store*: a hidden page-like root that is not in [`Document::pages`]. The
//! copies are read-only "remote" main components, so their instances behave like any other
//! instance. [`ImportedComponent`] / [`ImportedStyle`] records map each copy to its library asset
//! and version; updating re-imports a copy in place, keeping its layer ids (and with them the
//! overrides of its instances).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use figcraft_color::{Paint, PaintKind};
use serde::{Deserialize, Serialize};

use crate::{Document, Node, NodeId, NodeKind, StyleKind};

/// Name of the library store's root (never shown; older readers list it as a page).
pub const STORE_NAME: &str = "Library components";

/// What a published library offers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryManifest {
    /// Stable id of the library (kept across publishes).
    pub id: String,
    /// The library file's name when it was published.
    pub name: String,
    /// Increases by one with every publish.
    pub version: u64,
    pub description: String,
    /// When it was published (Unix seconds).
    pub time: u64,
    /// Component and component set roots in the snapshot: the published ones and those they use.
    pub components: Vec<LibraryAsset>,
    /// Styles in the snapshot: the published ones and those the components use.
    pub styles: Vec<LibraryStyle>,
}

impl LibraryManifest {
    pub fn component(&self, id: NodeId) -> Option<&LibraryAsset> {
        self.components.iter().find(|a| a.id == id)
    }

    pub fn style(&self, id: &str) -> Option<&LibraryStyle> {
        self.styles.iter().find(|s| s.id == id)
    }
}

/// A component (or component set) root in a library snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibraryAsset {
    /// Its node id in the snapshot (the library file's own id).
    pub id: NodeId,
    #[serde(default)]
    pub name: String,
    /// A component set (its variants come with it).
    #[serde(default)]
    pub set: bool,
    /// The library version this asset (or anything it is made of) last changed in.
    #[serde(default)]
    pub version: u64,
    /// Offered in the Assets panel; `false` for components only other assets use.
    #[serde(default)]
    pub published: bool,
}

/// A style in a library snapshot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryStyle {
    pub id: String,
    pub name: String,
    /// paint | text | effect | grid
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u64,
    pub published: bool,
}

/// A document's published library: the manifest and the snapshot it describes (with its images).
#[derive(Clone, Debug)]
pub struct PublishedLibrary {
    pub manifest: LibraryManifest,
    pub doc: Arc<Document>,
}

/// The snapshot follows from the manifest (every publish gets a new version), so comparing
/// manifests is enough and never walks the snapshot.
impl PartialEq for PublishedLibrary {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.doc, &other.doc) || self.manifest == other.manifest
    }
}

/// A library file this document uses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryLink {
    /// The library's id (its manifest's).
    pub id: String,
    pub name: String,
    /// Where the library file is.
    pub path: String,
    /// The library version this document last saw (when it was added or its updates accepted).
    pub version: u64,
}

/// A library component (or set) copied into the library store.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportedComponent {
    /// The library's id.
    pub library: String,
    /// The asset's id in the library snapshot.
    pub key: NodeId,
    /// The local copy's root.
    pub root: NodeId,
    /// The asset version copied.
    #[serde(default)]
    pub version: u64,
    /// Library node id → local node id for every layer of the copy (kept across updates).
    #[serde(default)]
    pub ids: BTreeMap<NodeId, NodeId>,
}

/// A library style copied into the document's styles (a linked, read-only remote style).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportedStyle {
    pub library: String,
    /// The style's id in the library.
    pub key: String,
    /// The local style's id.
    pub local: String,
    pub version: u64,
}

/// A document's library data: enabled libraries and the assets copied from them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Libraries {
    pub enabled: Vec<LibraryLink>,
    /// Root of the library store (a page-kind node not listed in `pages`).
    pub store: Option<NodeId>,
    pub components: Vec<ImportedComponent>,
    pub styles: Vec<ImportedStyle>,
}

impl Libraries {
    pub fn is_empty(&self) -> bool {
        self.enabled.is_empty() && self.store.is_none() && self.components.is_empty() && self.styles.is_empty()
    }

    /// Serde helper: leave the field out of files that use no libraries.
    pub fn is_unused(l: &Arc<Libraries>) -> bool {
        l.is_empty()
    }

    pub fn link(&self, id: &str) -> Option<&LibraryLink> {
        self.enabled.iter().find(|l| l.id == id)
    }

    /// The import record of the copy rooted at local node `root`.
    pub fn import_of(&self, root: NodeId) -> Option<&ImportedComponent> {
        self.components.iter().find(|c| c.root == root)
    }

    /// The import record of the local (remote) style `local`.
    pub fn style_import(&self, local: &str) -> Option<&ImportedStyle> {
        self.styles.iter().find(|s| s.local == local)
    }
}

impl Document {
    /// The library store's root, if the document has one.
    pub fn library_store(&self) -> Option<NodeId> {
        self.libraries.store.filter(|s| self.node(*s).is_some_and(|n| n.is_page() && n.parent.is_none()) && !self.pages.contains(s))
    }

    /// The library store's root, created when missing.
    pub fn ensure_library_store(&mut self) -> NodeId {
        if let Some(s) = self.library_store() {
            return s;
        }
        let id = self.alloc_id();
        let node = Node::new(
            id,
            NodeKind::Page { background: figcraft_color::Color::DEFAULT_PAGE, flows: Vec::new(), prototype: Default::default(), guides: Vec::new() },
            STORE_NAME,
            0.0,
            0.0,
        );
        self.nodes.insert(id, Arc::new(node));
        self.dirty.0.insert(id);
        Arc::make_mut(&mut self.libraries).store = Some(id);
        id
    }

    /// Whether `id` is a remote (library) layer kept in the library store.
    pub fn is_remote(&self, id: NodeId) -> bool {
        self.library_store().is_some_and(|s| self.page_of(id) == Some(s))
    }

    /// Mutable library data (copy-on-write).
    pub fn libraries_mut(&mut self) -> &mut Libraries {
        Arc::make_mut(&mut self.libraries)
    }

    /// Keep [`alloc_id`](Self::alloc_id) above `id` (for a document that reuses another's ids).
    pub fn reserve_ids(&mut self, id: NodeId) {
        if id.0 >= self.next_id {
            self.next_id = id.0.saturating_add(1);
        }
    }
}

/// Image hashes painted by `paints`.
pub fn paint_images<'a>(paints: impl IntoIterator<Item = &'a Paint>, out: &mut BTreeSet<String>) {
    for p in paints {
        if let PaintKind::Image(img) = &p.kind {
            out.insert(img.image.clone());
        }
    }
}

/// Image hashes a node paints (fills, strokes, text and text run fills).
pub fn node_images(n: &Node, out: &mut BTreeSet<String>) {
    paint_images(n.fills.iter().chain(&n.strokes), out);
    if let NodeKind::Text(t) = &n.kind {
        paint_images(t.style.fills.iter().flatten(), out);
        for r in &t.runs {
            paint_images(r.style.fills.iter().flatten(), out);
        }
    }
}

/// Image hashes a document uses (its nodes and its paint styles).
pub fn used_images(doc: &Document) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for n in doc.nodes() {
        node_images(n, &mut out);
    }
    for s in &doc.styles {
        if let StyleKind::Paint { paints } = &s.kind {
            paint_images(paints, &mut out);
        }
    }
    out
}
