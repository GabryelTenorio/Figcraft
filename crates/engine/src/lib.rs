//! The FigCraft engine.
//!
//! Every user-visible action is a command with a stable id (`node.create`, `align.left`,
//! `layout.add`, `component.create`…) taking JSON params. The egui UI, the CLI, the control channel
//! and the MCP server all go through [`Session::execute`], so everything is scriptable and every
//! change is one undo step. Pointer tools ([`tools`]) and text editing reduce to the same document
//! edits and are journaled the same way.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod al_keys;
pub mod annotations;
pub mod arc_handles;
pub mod booleans;
pub mod catalog;
pub mod cmd;
pub mod codegen;
pub mod collab;
pub mod component_codegen;
pub mod components;
pub mod devcompare;
pub mod devinspect;
pub mod distance;
pub mod drop;
pub mod guard;
pub mod guides;
pub mod inspect;
pub mod invite_links;
pub mod keymap;
pub mod library;
pub mod live;
pub mod measure;
pub mod params;
pub mod plugins;
pub mod proto_run;
pub mod shapebuilder;
pub mod snap;
pub mod spelling;
pub mod synthetic;
pub mod synthetic_ds;
pub mod textedit;
pub mod textpreview;
pub mod tools;
pub mod vectoredit;

pub use cmd::{CommandSpec, all_commands, find_command};
pub use figcraft_collab;
pub use figcraft_doc as doc;
pub use figcraft_svg::looks_like_svg;
pub use tools::{PointerEvent, PointerKind, ToolId};

use std::collections::BTreeSet;
use std::sync::Arc;

use figcraft_doc::{Document, Node, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("no open document")]
    NoDocument,
    #[error("no such node {0}")]
    NoNode(NodeId),
    #[error("{0}")]
    Other(String),
    /// A bug: the command panicked. The document was rolled back.
    #[error("internal error in `{cmd}`: {msg} (the document was left as it was before; please report this bug)")]
    Internal { cmd: String, msg: String },
}

impl From<figcraft_doc::DocError> for EngineError {
    fn from(e: figcraft_doc::DocError) -> Self {
        match e {
            figcraft_doc::DocError::NoNode(id) => EngineError::NoNode(id),
            other => EngineError::Other(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

pub fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.to_string(), msg: msg.into() }
}

/// One undo step.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub label: String,
    pub doc: Arc<Document>,
    pub selection: Vec<NodeId>,
    pub page: NodeId,
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub undo: Vec<HistoryEntry>,
    pub redo: Vec<HistoryEntry>,
    pub limit: usize,
}

/// The canvas view of a document: what's at the centre of the viewport and the zoom.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    /// Page coordinate at the viewport centre.
    pub cx: f64,
    pub cy: f64,
    /// Zoom (1 = 100%).
    pub zoom: f64,
    /// Viewport size in logical pixels (set by the UI; used by zoom-to-fit).
    pub vw: f64,
    pub vh: f64,
    pub pixel_grid: bool,
    pub layout_grids: bool,
    pub rulers: bool,
    pub outlines: bool,
    pub pixel_preview: bool,
    /// Pixel preview resolution: rendered pixels per page pixel (1 or 2).
    pub pixel_scale: u8,
    pub show_slices: bool,
    pub comments: bool,
    pub frame_outlines: bool,
    pub snap_geometry: bool,
    pub snap_objects: bool,
    pub snap_pixel: bool,
    pub ui_hidden: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            cx: 0.0,
            cy: 0.0,
            zoom: 1.0,
            vw: 1200.0,
            vh: 800.0,
            pixel_grid: true,
            layout_grids: true,
            rulers: false,
            outlines: false,
            pixel_preview: false,
            pixel_scale: 1,
            show_slices: false,
            comments: true,
            frame_outlines: false,
            snap_geometry: true,
            snap_objects: true,
            snap_pixel: true,
            ui_hidden: false,
        }
    }
}

impl ViewState {
    pub const MIN_ZOOM: f64 = 0.02;
    pub const MAX_ZOOM: f64 = 256.0;
    /// Page coordinate at the viewport's top-left.
    pub fn origin(&self) -> (f64, f64) {
        let z = self.zoom.max(1e-9);
        (self.cx - self.vw / 2.0 / z, self.cy - self.vh / 2.0 / z)
    }
    pub fn set_zoom_at(&mut self, zoom: f64, anchor: (f64, f64)) {
        let z = if zoom.is_finite() { zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM) } else { self.zoom };
        let old = self.zoom.max(1e-9);
        // Keep the page point under `anchor` fixed.
        self.cx = anchor.0 + (self.cx - anchor.0) * old / z;
        self.cy = anchor.1 + (self.cy - anchor.1) * old / z;
        self.zoom = z;
    }
    /// Fit a page rectangle into the viewport with margin.
    pub fn fit(&mut self, r: figcraft_geom::Rect, max_zoom: f64) {
        if !(r.width() > 0.0 || r.height() > 0.0) || !r.x0.is_finite() {
            self.cx = r.center().x;
            self.cy = r.center().y;
            return;
        }
        let margin = 64.0;
        let zx = (self.vw - margin * 2.0).max(50.0) / r.width().max(1e-6);
        let zy = (self.vh - margin * 2.0).max(50.0) / r.height().max(1e-6);
        self.zoom = zx.min(zy).clamp(Self::MIN_ZOOM, max_zoom);
        self.cx = r.center().x;
        self.cy = r.center().y;
    }
    pub fn page_to_screen(&self, x: f64, y: f64) -> (f64, f64) {
        let (ox, oy) = self.origin();
        ((x - ox) * self.zoom, (y - oy) * self.zoom)
    }
    pub fn screen_to_page(&self, x: f64, y: f64) -> (f64, f64) {
        let (ox, oy) = self.origin();
        let z = self.zoom.max(1e-9);
        (ox + x / z, oy + y / z)
    }
}

/// An in-progress gesture: the document before it started, so previews re-apply from scratch.
#[derive(Clone, Debug)]
pub struct Interaction {
    pub label: String,
    pub doc: Arc<Document>,
    pub selection: Vec<NodeId>,
}

/// The text being edited on the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextEdit {
    pub id: NodeId,
    /// Caret position and selection anchor (char indices).
    pub caret: usize,
    pub anchor: usize,
    /// The node was created by this edit (deleted if left empty).
    pub created: bool,
    /// An IME composition in progress, shown inline at the caret; the document gets it on commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preedit: Option<Preedit>,
}

/// IME composition text (not yet in the document) and the IME's cursor inside it (chars).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preedit {
    pub text: String,
    pub cursor: usize,
}

/// Per-document editing state.
#[derive(Clone, Debug)]
pub struct DocState {
    pub doc: Arc<Document>,
    pub selection: Vec<NodeId>,
    pub page: NodeId,
    pub history: History,
    saved_doc: Arc<Document>,
    pub path: Option<String>,
    pub revision: u64,
    pub view: ViewState,
    pub interaction: Option<Interaction>,
    pub text_edit: Option<TextEdit>,
    /// Hovered node (canvas hover outline).
    pub hover: Option<NodeId>,
    /// Vector edit mode: the vector being edited, its selected points and the edit tool.
    pub vector_edit: Option<vectoredit::VectorEdit>,
    pub uid: u64,
    /// The file's version history, oldest first (saved in the `.figcraft` file; not undoable).
    pub versions: Vec<figcraft_format::Version>,
    /// The version history changed since the last save.
    versions_changed: bool,
    /// A version shown in place of the document (read-only).
    pub preview: Option<VersionPreview>,
    /// The document under a live text preview (a hovered text style or font instance, a dragged
    /// axis slider): `doc` shows the preview until [`textpreview::end`] puts this back.
    pub text_preview: Option<Arc<Document>>,
    /// The document as of the last version saved (or as opened): autosaves only follow changes.
    checkpoint: Arc<Document>,
    /// Dev Mode focus view: the frame or section shown alone on the canvas (see [`Self::focused`]).
    pub focus: Option<NodeId>,
}

/// A version shown on the canvas in place of the document: read-only until it's restored or the
/// preview ends, which puts the document back as it was.
#[derive(Clone, Debug)]
pub struct VersionPreview {
    pub id: String,
    pub name: String,
    /// When the version was saved (Unix seconds).
    pub time: u64,
    doc: Arc<Document>,
    selection: Vec<NodeId>,
    page: NodeId,
}

/// Why edits are refused while a version is shown.
pub const PREVIEW_READ_ONLY: &str = "you're viewing an earlier version: restore it or go back to the current version to edit";

/// Commands that first end a version preview: they act on the document itself.
const ENDS_PREVIEW: &[&str] = &["file.save", "file.saveAs", "file.close", "file.revert", "file.saveVersion", "file.duplicate", "version.restore"];

static NEXT_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl DocState {
    pub fn new(mut doc: Document, path: Option<String>) -> Self {
        if doc.pages.is_empty() {
            doc.repair();
        }
        doc.link_layer_text_styles();
        let page = doc.current_page.filter(|p| doc.pages.contains(p)).or_else(|| doc.first_page()).unwrap_or(NodeId(0));
        let doc = Arc::new(doc);
        let mut st = Self {
            checkpoint: doc.clone(),
            saved_doc: doc.clone(),
            doc,
            selection: Vec::new(),
            page,
            history: History { limit: 1000, ..Default::default() },
            path,
            revision: 1,
            view: ViewState::default(),
            interaction: None,
            text_edit: None,
            hover: None,
            vector_edit: None,
            uid: NEXT_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            versions: Vec::new(),
            versions_changed: false,
            preview: None,
            text_preview: None,
            focus: None,
        };
        st.zoom_to_fit();
        st
    }

    /// With the version history read from its file.
    pub fn with_versions(mut self, versions: Vec<figcraft_format::Version>) -> Self {
        self.versions = versions;
        self
    }

    /// The version history, for editing (the file then has unsaved changes).
    pub fn versions_mut(&mut self) -> &mut Vec<figcraft_format::Version> {
        self.versions_changed = true;
        &mut self.versions
    }

    /// The document changed since the last version was saved (or since it was opened).
    pub fn changed_since_checkpoint(&self) -> bool {
        !Arc::ptr_eq(self.preview.as_ref().map_or(&self.doc, |p| &p.doc), &self.checkpoint)
    }

    /// Note that a version of the current document was just saved.
    pub(crate) fn mark_checkpoint(&mut self) {
        self.checkpoint = self.doc.clone();
    }

    /// Push a snapshot undo step: the document was `before` (shown with `selection` on `page`).
    pub(crate) fn push_undo(&mut self, label: &str, before: Arc<Document>, selection: Vec<NodeId>, page: NodeId) {
        self.history.undo.push(HistoryEntry { label: label.to_string(), doc: before, selection, page });
        if self.history.undo.len() > self.history.limit.max(1) {
            self.history.undo.remove(0);
        }
        self.history.redo.clear();
    }

    pub fn is_dirty(&self) -> bool {
        let doc = self.preview.as_ref().map(|p| &p.doc).or(self.text_preview.as_ref()).unwrap_or(&self.doc);
        !Arc::ptr_eq(doc, &self.saved_doc) || self.versions_changed
    }

    /// Show `version` in place of the document (or switch to it from another preview).
    pub fn start_preview(&mut self, id: &str, name: &str, time: u64, doc: Arc<Document>) {
        self.end_preview();
        self.preview = Some(VersionPreview {
            id: id.to_string(),
            name: name.to_string(),
            time,
            doc: std::mem::replace(&mut self.doc, doc),
            selection: std::mem::take(&mut self.selection),
            page: self.page,
        });
        self.page = Some(self.page)
            .filter(|pg| self.doc.pages.contains(pg))
            .or_else(|| self.doc.current_page.filter(|pg| self.doc.pages.contains(pg)))
            .or_else(|| self.doc.first_page())
            .unwrap_or(self.page);
        self.text_edit = None;
        self.vector_edit = None;
        self.interaction = None;
        self.revision += 1;
    }

    /// End a version preview, putting the document, selection and page back. False when none.
    pub fn end_preview(&mut self) -> bool {
        let Some(p) = self.preview.take() else { return false };
        self.doc = p.doc;
        self.selection = p.selection;
        self.page = p.page;
        self.text_edit = None;
        self.vector_edit = None;
        self.interaction = None;
        self.clean_selection();
        self.revision += 1;
        true
    }

    pub fn mark_saved(&mut self) {
        self.saved_doc = self.doc.clone();
        self.versions_changed = false;
    }

    pub fn doc_mut(&mut self) -> &mut Document {
        Arc::make_mut(&mut self.doc)
    }

    /// Fit the current page's content (or centre on the origin when empty).
    pub fn zoom_to_fit(&mut self) {
        match self.doc.page_content_bounds(self.page) {
            Some(b) => self.view.fit(b, 1.0),
            None => {
                self.view.zoom = 1.0;
                self.view.cx = self.view.vw / 2.0 - 100.0;
                self.view.cy = self.view.vh / 2.0 - 100.0;
            }
        }
    }

    /// The layer the focus view shows, while it exists on the current page.
    pub fn focused(&self) -> Option<NodeId> {
        self.focus.filter(|f| self.doc.page_of(*f) == Some(self.page))
    }

    /// Drop selected ids that no longer exist or aren't on the current page.
    pub fn clean_selection(&mut self) {
        let doc = self.doc.clone();
        let page = self.page;
        self.selection.retain(|id| doc.contains(*id) && doc.page_of(*id) == Some(page) && *id != page);
        let mut seen = BTreeSet::new();
        self.selection.retain(|id| seen.insert(*id));
        if let Some(te) = &self.text_edit
            && !doc.contains(te.id)
        {
            self.text_edit = None;
        }
        // Vector edit mode ends when its layer is gone (or no longer a vector, e.g. undone).
        match self.vector_edit.as_mut() {
            Some(ve) if doc.page_of(ve.id) == Some(page) => match vectoredit::network(&doc, ve.id) {
                Some(net) => ve.clamp(net),
                None => self.vector_edit = None,
            },
            Some(_) => self.vector_edit = None,
            None => {}
        }
    }
}

/// App-wide preferences that commands read (nudge amounts, behaviours).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub nudge_small: f64,
    pub nudge_big: f64,
    pub keep_tool: bool,
    pub rename_duplicates: bool,
    pub highlight_on_hover: bool,
    pub number_keys_opacity: bool,
    pub show_dimensions: bool,
    pub theme: String,
    /// The name comments are signed with (and whose comments count as yours).
    pub user_name: String,
    /// Reduce motion: the interface's own animations are off (the system's setting turns them off
    /// too). Prototype transitions still play.
    pub reduce_motion: bool,
    /// `figcraft://` invite links open this app (`collab.registerLinks`); `None`: on for installed
    /// builds, off for development builds.
    pub invite_links: Option<bool>,
    /// Underline misspelled words while editing text (`text.spelling`).
    pub spell_check: bool,
    /// Words never flagged as misspelled (`spelling.addWord`), lowercased.
    pub dictionary: Vec<String>,
    /// Installed plugins (Plugins menu), in menu order.
    #[serde(serialize_with = "plugins::persisted")]
    pub plugins: Vec<plugins::InstalledPlugin>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            nudge_small: 1.0,
            nudge_big: 10.0,
            keep_tool: false,
            rename_duplicates: false,
            highlight_on_hover: true,
            number_keys_opacity: true,
            show_dimensions: true,
            theme: "light".into(),
            user_name: cmd::comment::DEFAULT_AUTHOR.into(),
            reduce_motion: false,
            invite_links: None,
            spell_check: true,
            dictionary: Vec::new(),
            plugins: Vec::new(),
        }
    }
}

/// A request from a command to the UI (open a dialog or panel, show a message). Programmatic
/// callers ignore them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UiRequest {
    pub kind: String,
    pub data: Value,
}

/// Clipboard contents (internal; the UI mirrors to the system clipboard).
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    /// Copied subtrees: (root, nodes) in z-order.
    pub nodes: Vec<(NodeId, Vec<figcraft_doc::Node>)>,
    pub images: std::collections::BTreeMap<String, figcraft_doc::ImageData>,
    /// Copied properties (Copy properties / Paste properties).
    pub props: Option<Value>,
    /// The page-space bounds of the copied selection.
    pub bounds: Option<figcraft_geom::Rect>,
    pub source_doc: u64,
    /// The document copied from (pastes into other files follow its mains) and its file path.
    pub source: Option<Arc<Document>>,
    pub source_path: Option<String>,
}

/// The editing session: open documents, the current tool, clipboard and prefs.
pub struct Session {
    pub docs: Vec<DocState>,
    pub active: usize,
    pub clipboard: Clipboard,
    pub prefs: Prefs,
    pub tool: tools::ToolState,
    pub ui_requests: Vec<UiRequest>,
    /// Command log (id + params) for actions/automation.
    pub journal: Vec<(String, Value)>,
    /// Recently used colours.
    pub recent_colors: Vec<figcraft_color::Color>,
    /// The last command executed through [`Session::execute`] (for "Repeat").
    pub last_command: Option<(String, Value)>,
    /// Status / toast message for the UI.
    pub message: Option<String>,
    /// Library files read for the open documents.
    pub libraries: library::Cache,
    /// Plugin modules loaded in this session and the last plugin run.
    pub plugins: plugins::Runtime,
    /// The live session (collaboration): status, peers, presence.
    pub live: live::Live,
    /// Spell check while editing text.
    pub spelling: spelling::Spelling,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// A session with one new document.
    pub fn new() -> Self {
        let mut s = Self::empty();
        s.docs.push(DocState::new(Document::new("Untitled"), None));
        s
    }

    /// A session with no documents (the home screen).
    pub fn empty() -> Self {
        Self {
            docs: Vec::new(),
            active: 0,
            clipboard: Clipboard::default(),
            prefs: Prefs::default(),
            tool: tools::ToolState::default(),
            ui_requests: Vec::new(),
            journal: Vec::new(),
            recent_colors: Vec::new(),
            last_command: None,
            message: None,
            libraries: library::Cache::default(),
            plugins: plugins::Runtime::default(),
            live: live::Live::default(),
            spelling: spelling::Spelling::default(),
        }
    }

    pub fn with_document(doc: Document) -> Self {
        let mut s = Self::empty();
        s.docs.push(DocState::new(doc, None));
        s
    }

    pub fn ds(&self) -> Result<&DocState> {
        self.docs.get(self.active).ok_or(EngineError::NoDocument)
    }

    pub fn ds_mut(&mut self) -> Result<&mut DocState> {
        self.docs.get_mut(self.active).ok_or(EngineError::NoDocument)
    }

    pub fn doc(&self) -> Result<&Document> {
        Ok(self.ds()?.doc.as_ref())
    }

    pub fn doc_mut(&mut self) -> Result<&mut Document> {
        Ok(self.ds_mut()?.doc_mut())
    }

    pub fn selection(&self) -> Vec<NodeId> {
        self.ds().map(|d| d.selection.clone()).unwrap_or_default()
    }

    pub fn set_selection(&mut self, ids: Vec<NodeId>) {
        if let Ok(ds) = self.ds_mut() {
            ds.selection = ids;
            ds.clean_selection();
            ds.revision += 1;
        }
    }

    pub fn page(&self) -> Result<NodeId> {
        Ok(self.ds()?.page)
    }

    /// Run a command by id. Mutations become one undo step; a failure or panic leaves the document
    /// as it was.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        if textpreview::COMMANDS.contains(&id) {
            // A live preview is no edit: no undo step, normalisation or rollback here.
            let r = guard::run(id, || (spec.run)(self, params));
            if r.is_err() {
                textpreview::end(self);
            }
            return r;
        }
        // Every other command acts on the document itself, not on a live text preview.
        textpreview::end(self);
        if ENDS_PREVIEW.contains(&id)
            && let Ok(ds) = self.ds_mut()
        {
            ds.end_preview();
        }
        let before = self.ds().ok().map(|d| (d.doc.clone(), d.selection.clone(), d.page, d.uid));
        let result = guard::run(id, || (spec.run)(self, params))
            .and_then(|v| self.check_instance_structure(id, &before).map(|()| v))
            .and_then(|v| self.check_preview(id, &before).map(|()| v))
            .and_then(|v| self.check_live_access(id, &before).map(|()| v));
        match &result {
            Ok(_) => {
                self.after_command(spec, before);
                if spec.history {
                    self.last_command = Some((id.to_string(), params.clone()));
                }
                self.journal.push((id.to_string(), params.clone()));
                if self.journal.len() > 5000 {
                    self.journal.drain(..1000);
                }
            }
            Err(_) => {
                // Roll back any partial edit.
                if let Some((doc, sel, page, uid)) = before
                    && let Some(ds) = self.docs.iter_mut().find(|d| d.uid == uid)
                    && ds.interaction.is_none()
                {
                    ds.doc = doc;
                    ds.selection = sel;
                    ds.page = page;
                }
            }
        }
        result
    }

    /// A version shown in place of the document is read-only: refuse an edit (it is rolled back).
    fn check_preview(&self, id: &str, before: &Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) -> Result<()> {
        let Some((bdoc, _, _, uid)) = before else { return Ok(()) };
        let Some(ds) = self.docs.iter().find(|d| d.uid == *uid) else { return Ok(()) };
        if ds.preview.is_none() || Arc::ptr_eq(&ds.doc, bdoc) || id == "version.preview" {
            return Ok(());
        }
        Err(EngineError::Disabled(id.to_string(), PREVIEW_READ_ONLY.into()))
    }

    /// A live session's document you may only view (or that is on its way to a server) is
    /// read-only: refuse an edit (it is rolled back).
    fn check_live_access(&self, id: &str, before: &Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) -> Result<()> {
        let Some((bdoc, _, _, uid)) = before else { return Ok(()) };
        let Some(ds) = self.docs.iter().find(|d| d.uid == *uid) else { return Ok(()) };
        match self.live_read_only(*uid) {
            Some(why) if !Arc::ptr_eq(&ds.doc, bdoc) => Err(EngineError::Disabled(id.to_string(), why.into())),
            _ => Ok(()),
        }
    }

    /// Instances can't gain, lose or reorder layers, and library copies are read-only: refuse such
    /// an edit (it is rolled back).
    fn check_instance_structure(&self, id: &str, before: &Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) -> Result<()> {
        let Some((bdoc, _, _, uid)) = before else { return Ok(()) };
        let Some(ds) = self.docs.iter().find(|d| d.uid == *uid) else { return Ok(()) };
        if Arc::ptr_eq(&ds.doc, bdoc) {
            return Ok(());
        }
        // "Repeat" runs the repeated command.
        let effective = match (&self.last_command, id) {
            (Some((last, _)), "edit.repeat") => last.as_str(),
            _ => id,
        };
        library::check_read_only(effective, bdoc, &ds.doc)
            .and_then(|()| components::check_structure_edit(bdoc, &ds.doc))
            .map_err(|m| EngineError::Disabled(id.to_string(), m))
    }

    /// Bookkeeping after a successful command: normalise what changed, push undo, bump revision.
    fn after_command(&mut self, spec: &CommandSpec, before: Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) {
        let Some((bdoc, bsel, bpage, uid)) = before else {
            if let Ok(ds) = self.ds_mut() {
                ds.revision += 1;
            }
            return;
        };
        let Some(ds) = self.docs.iter_mut().find(|d| d.uid == uid) else { return };
        let changed = !Arc::ptr_eq(&ds.doc, &bdoc);
        if changed {
            normalize(ds.doc_mut(), &bdoc);
        }
        let edit = changed && ds.interaction.is_none();
        ds.clean_selection();
        ds.revision += 1;
        if edit {
            self.record(uid, spec.history.then_some(spec.label), bdoc, bsel, bpage);
        }
    }

    /// Record an edit of document `uid` (it was `before`, shown with `selection` on `page`) as one
    /// undo step called `label` (`None`: not undoable). On a live session's document the edit is
    /// synced and the step is the user's own ([`live`]); otherwise it is a snapshot of `before`.
    pub(crate) fn record(&mut self, uid: u64, label: Option<&str>, before: Arc<Document>, selection: Vec<NodeId>, page: NodeId) {
        if self.live_doc(uid) {
            return self.live_edit(label);
        }
        if let (Some(label), Some(ds)) = (label, self.docs.iter_mut().find(|d| d.uid == uid)) {
            ds.push_undo(label, before, selection, page);
        }
    }

    /// How many undo and redo steps the active document has (the user's own in a live session).
    pub fn undo_depth(&self) -> (usize, usize) {
        match self.ds() {
            Ok(ds) if self.live_doc(ds.uid) => self.live.client().map_or((0, 0), figcraft_collab::Client::history_len),
            Ok(ds) => (ds.history.undo.len(), ds.history.redo.len()),
            Err(_) => (0, 0),
        }
    }

    /// The labels of the active document's last `n` undo steps, the next one first.
    pub fn undo_labels(&self, n: usize) -> Vec<String> {
        match self.ds() {
            Ok(ds) if self.live_doc(ds.uid) => {
                self.live.client().map(|c| c.undo_labels().rev().take(n).map(str::to_string).collect()).unwrap_or_default()
            }
            Ok(ds) => ds.history.undo.iter().rev().take(n).map(|h| h.label.clone()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Start a gesture: later [`preview`](Self::preview) calls re-apply edits to this snapshot.
    pub fn begin_interaction(&mut self, label: &str) -> Result<()> {
        textpreview::end(self);
        if let Some(why) = self.active_read_only() {
            return Err(EngineError::Disabled(label.to_string(), why.into()));
        }
        let ds = self.ds_mut()?;
        if ds.preview.is_some() {
            return Err(EngineError::Disabled(label.to_string(), PREVIEW_READ_ONLY.into()));
        }
        if ds.interaction.is_none() {
            ds.interaction = Some(Interaction { label: label.to_string(), doc: ds.doc.clone(), selection: ds.selection.clone() });
        }
        Ok(())
    }

    /// Replace the document with `f(snapshot)` for a live preview during a gesture.
    pub fn preview(&mut self, f: impl FnOnce(&mut Document, &mut Vec<NodeId>) -> Result<()>) -> Result<()> {
        let ds = self.ds_mut()?;
        let Some(it) = ds.interaction.clone() else { return Err(EngineError::Other("no gesture in progress".into())) };
        let mut doc = (*it.doc).clone();
        let mut sel = it.selection.clone();
        let res = f(&mut doc, &mut sel);
        if res.is_ok() {
            normalize(&mut doc, &it.doc);
            // The same preview again (a handle held still) keeps the document: a new one would
            // re-render the canvas for nothing.
            if doc == *ds.doc && sel == ds.selection {
                return res;
            }
            ds.doc = Arc::new(doc);
            ds.selection = sel;
            ds.clean_selection();
            ds.revision += 1;
        }
        res
    }

    /// Finish a gesture as one undo step (no step if nothing changed).
    pub fn commit_interaction(&mut self) -> Result<()> {
        let ds = self.ds_mut()?;
        let Some(it) = ds.interaction.take() else { return Ok(()) };
        let (uid, page) = (ds.uid, ds.page);
        ds.revision += 1;
        if !Arc::ptr_eq(&it.doc, &ds.doc) && *it.doc != *ds.doc {
            self.record(uid, Some(&it.label), it.doc, it.selection, page);
        } else {
            ds.doc = it.doc;
        }
        Ok(())
    }

    /// Abandon a gesture, restoring the document.
    pub fn cancel_interaction(&mut self) -> Result<()> {
        let ds = self.ds_mut()?;
        if let Some(it) = ds.interaction.take() {
            ds.doc = it.doc;
            ds.selection = it.selection;
            ds.revision += 1;
        }
        Ok(())
    }

    pub fn undo(&mut self) -> Result<bool> {
        textpreview::end(self);
        let live = self.ds().is_ok_and(|d| self.live_doc(d.uid));
        let ds = self.ds_mut()?;
        if ds.preview.is_some() {
            return Err(EngineError::Disabled("edit.undo".into(), PREVIEW_READ_ONLY.into()));
        }
        if let Some(it) = ds.interaction.take()
            && live
        {
            // The gesture in progress is dropped (it was never synced).
            ds.doc = it.doc;
        }
        ds.text_edit = None;
        if live {
            return Ok(self.live_travel(true));
        }
        let Some(e) = ds.history.undo.pop() else { return Ok(false) };
        let redo = HistoryEntry { label: e.label.clone(), doc: ds.doc.clone(), selection: ds.selection.clone(), page: ds.page };
        ds.history.redo.push(redo);
        ds.doc = e.doc;
        ds.selection = e.selection;
        if ds.doc.pages.contains(&e.page) {
            ds.page = e.page;
        }
        ds.clean_selection();
        ds.revision += 1;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool> {
        textpreview::end(self);
        let live = self.ds().is_ok_and(|d| self.live_doc(d.uid));
        let ds = self.ds_mut()?;
        if ds.preview.is_some() {
            return Err(EngineError::Disabled("edit.redo".into(), PREVIEW_READ_ONLY.into()));
        }
        if let Some(it) = ds.interaction.take()
            && live
        {
            ds.doc = it.doc;
        }
        ds.text_edit = None;
        if live {
            return Ok(self.live_travel(false));
        }
        let Some(e) = ds.history.redo.pop() else { return Ok(false) };
        let undo = HistoryEntry { label: e.label.clone(), doc: ds.doc.clone(), selection: ds.selection.clone(), page: ds.page };
        ds.history.undo.push(undo);
        ds.doc = e.doc;
        ds.selection = e.selection;
        if ds.doc.pages.contains(&e.page) {
            ds.page = e.page;
        }
        ds.clean_selection();
        ds.revision += 1;
        Ok(true)
    }

    /// Queue a UI request (dialogs etc.).
    pub fn request_ui(&mut self, kind: &str, data: Value) {
        self.ui_requests.push(UiRequest { kind: kind.to_string(), data });
    }

    /// Run several commands as one undo step.
    pub fn batch(&mut self, label: &str, cmds: &[(String, Value)]) -> Result<Vec<Value>> {
        self.grouped(label, |s| cmds.iter().map(|(id, p)| s.execute(id, p)).collect())
    }

    /// Run `f` as one undo step called `label`: the commands it executes push no steps of their
    /// own, and when it fails the document, selection, page and history are put back.
    pub fn grouped<T>(&mut self, label: &str, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let ds = self.ds_mut()?;
        let (before, sel, page, saved_len, uid) = (ds.doc.clone(), ds.selection.clone(), ds.page, ds.history.undo.len(), ds.uid);
        // The steps inside are dropped again below, so they mustn't push the oldest ones out.
        let limit = std::mem::replace(&mut ds.history.limit, usize::MAX);
        self.live_hold(uid, true);
        let result = f(self);
        self.live_hold(uid, false);
        let Some(ds) = self.docs.iter_mut().find(|d| d.uid == uid) else { return result };
        ds.history.limit = limit;
        ds.history.undo.truncate(saved_len);
        if result.is_err() {
            ds.doc = before;
            ds.selection = sel;
            ds.page = page;
        } else if !Arc::ptr_eq(&before, &ds.doc) {
            self.record(uid, Some(label), before, sel, page);
        }
        result
    }

    /// A summary for status displays.
    pub fn status(&self) -> Value {
        match self.ds() {
            Ok(ds) => json!({
                "document": ds.doc.name,
                "path": ds.path,
                "dirty": ds.is_dirty(),
                "page": ds.page,
                "selection": ds.selection,
                "tool": self.tool.tool.id(),
                "zoom": ds.view.zoom,
                "undo": self.undo_depth().0,
                "redo": self.undo_depth().1,
                "documents": self.docs.len(),
            }),
            Err(_) => json!({"documents": 0}),
        }
    }
}

/// Nodes whose `Arc` differs from `before` (new or edited).
pub fn changed_nodes(doc: &Document, before: &Document) -> Vec<NodeId> {
    doc.ids()
        .filter(|id| match (doc.node_arc(*id), before.node_arc(*id)) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            _ => true,
        })
        .collect()
}

/// Keep derived state consistent after edits: text boxes resize to their content, groups wrap
/// their children, instances follow their main components, auto layout re-flows.
pub fn normalize(doc: &mut Document, before: &Document) {
    // Only what was touched since the last normalisation (edited, added or removed).
    let mut touched: BTreeSet<NodeId> = doc.take_dirty();
    // A variable edit changes no layer directly, but bound layers must follow it.
    if touched.is_empty() && doc.variables == before.variables {
        return;
    }
    let m = measure::TextMeasure;
    // 0. Variable bindings and style links feed node values.
    let scope = binding_scope(doc, before, &touched);
    cmd::styles::apply_bindings_to(doc, scope.as_ref());
    if doc.styles != before.styles {
        let changed_styles: Vec<String> = doc.styles.iter().filter(|s| !before.styles.contains(s)).map(|s| s.id.clone()).collect();
        for sid in changed_styles {
            cmd::styles::propagate_style(doc, &sid);
        }
    }
    touched.extend(doc.take_dirty());
    // Text chars edited away from their text style unlink from it.
    let live_now: Vec<NodeId> = touched.iter().copied().filter(|i| doc.contains(*i)).collect();
    cmd::styles::sync_text_styles(doc, &live_now);
    touched.extend(doc.take_dirty());
    let live = |doc: &Document, t: &BTreeSet<NodeId>| -> Vec<NodeId> { t.iter().copied().filter(|i| doc.contains(*i)).collect() };
    // 0b. Fill and hug only where they can apply (a layer moved out of auto layout, a parent that
    // started to hug or lost auto layout): the rest becomes fixed at its current size.
    for id in live(doc, &touched) {
        figcraft_layout::coherent_sizing(doc, id);
    }
    touched.extend(doc.take_dirty());
    // 1. Text auto-size.
    for id in live(doc, &touched) {
        let Some(n) = doc.node(id) else { continue };
        if let Some((w, h)) = figcraft_text::auto_size(n)
            && ((w - n.width).abs() > 1e-6 || (h - n.height).abs() > 1e-6)
        {
            figcraft_layout::set_size(doc, id, w, h);
        }
    }
    touched.extend(doc.take_dirty());
    // 2. Instances follow edited main components (and record overrides inside instances).
    let changed = live(doc, &touched);
    components::sync_after_edit(doc, before, &changed, &touched);
    // Rebuilt copies took their mains' values: their bindings resolve in their own modes.
    let synced = doc.take_dirty();
    if !synced.is_empty() {
        cmd::styles::apply_bindings_to(doc, Some(&synced));
        touched.extend(synced);
    }
    touched.extend(doc.take_dirty());
    // Parents of removed nodes (as they were before) count as touched.
    let removed_parents: Vec<NodeId> =
        touched.iter().filter(|i| !doc.contains(**i)).filter_map(|i| before.node(*i).and_then(|n| n.parent)).filter(|p| doc.contains(*p)).collect();
    // 3. Groups / booleans wrap their children.
    let mut fit: BTreeSet<NodeId> = BTreeSet::new();
    for id in live(doc, &touched).into_iter().chain(removed_parents.iter().copied()) {
        if doc.node(id).is_some_and(|n| n.kind.bounds_follow_children()) {
            fit.insert(id);
        }
        for a in doc.ancestors(id) {
            if doc.node(a).is_some_and(|n| n.kind.bounds_follow_children()) {
                fit.insert(a);
            } else {
                break;
            }
        }
    }
    let mut fit: Vec<NodeId> = fit.into_iter().collect();
    fit.sort_by_key(|id| std::cmp::Reverse(doc.ancestors(*id).len()));
    for id in fit {
        if doc.node(id).is_some_and(|n| n.children.is_empty()) {
            // Empty groups disappear, like the incumbent.
            let _ = doc.remove(id);
        } else {
            let _ = doc.fit_to_children(id);
        }
    }
    touched.extend(doc.take_dirty());
    // 4. Auto layout for frames where something that sizes or places layers changed.
    let mut frames: BTreeSet<NodeId> = BTreeSet::new();
    let moved = live(doc, &touched).into_iter().filter(|id| affects_layout(before, doc, *id));
    for id in moved.chain(removed_parents) {
        if !doc.contains(id) {
            continue;
        }
        let mut chain = vec![id];
        chain.extend(doc.ancestors(id));
        for c in chain {
            if doc.node(c).is_some_and(|n| n.layout.is_some()) {
                frames.insert(c);
            }
        }
    }
    let mut frames: Vec<NodeId> = frames.into_iter().collect();
    frames.sort_by_key(|id| std::cmp::Reverse(doc.ancestors(*id).len()));
    for f in frames {
        // Layers the layout leaves where they were stay untouched.
        let kept: Vec<(Arc<Node>, bool)> = doc
            .node(f)
            .map(|n| std::iter::once(f).chain(n.children.iter().copied()).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|i| doc.node_arc(i).map(|a| (a.clone(), doc.dirty().contains(&i))))
            .collect();
        figcraft_layout::layout_frame(doc, f, &m);
        for (a, was_dirty) in kept {
            doc.keep_if_unchanged(a, was_dirty);
        }
    }
    touched.extend(doc.take_dirty());
    // 5. Ready-for-dev layers whose contents changed show "Changed".
    mark_dev_changes(doc, before, &touched);
    doc.take_dirty();
    // 5. Comment pins follow their layers to other pages and stay put when the layers are deleted.
    cmd::comment::follow_anchors(doc, before);
}

/// Whether layer `id` changed (since `before`) in a way that can move or resize layers in an auto
/// layout frame around it: new, resized, moved, shown or hidden, restructured, other text or
/// layout settings.
fn affects_layout(before: &Document, doc: &Document, id: NodeId) -> bool {
    let (Some(a), Some(b)) = (before.node(id), doc.node(id)) else { return true };
    a.width != b.width
        || a.height != b.height
        || a.transform != b.transform
        || a.visible != b.visible
        || a.parent != b.parent
        || a.children != b.children
        || a.layout != b.layout
        || a.child_layout != b.child_layout
        || a.stroke != b.stroke
        || a.strokes.len() != b.strokes.len()
        || a.kind != b.kind
}

/// The layers whose variable bindings an edit may have changed: every bound layer when a variable
/// changed, else the touched layers and everything below those that moved, are new or chose
/// another variable mode (modes are inherited).
fn binding_scope(doc: &Document, before: &Document, touched: &BTreeSet<NodeId>) -> Option<BTreeSet<NodeId>> {
    if doc.variables != before.variables {
        return None;
    }
    let mut out = BTreeSet::new();
    for &id in touched {
        let Some(n) = doc.node(id) else { continue };
        let inherited = before.node(id).is_none_or(|b| b.var_modes != n.var_modes || b.parent != n.parent);
        if inherited {
            out.extend(doc.subtree(id));
        } else {
            out.insert(id);
        }
    }
    Some(out)
}

/// Flag the frames and sections marked ready for dev (before this edit) that contain a layer whose
/// design `touched` changed (added, removed or edited; the handoff status itself and hiding a
/// section's contents don't count).
fn mark_dev_changes(doc: &mut Document, before: &Document, touched: &BTreeSet<NodeId>) {
    let ready = |d: &Document, id: NodeId| d.node(id).is_some_and(|n| n.dev_status == figcraft_doc::DevStatus::ReadyForDev);
    let mut changed: BTreeSet<NodeId> = BTreeSet::new();
    for &id in touched {
        // Ready-for-dev frames around it first (the cheap test).
        let mut chain = vec![id];
        chain.extend(if doc.contains(id) { doc.ancestors(id) } else { before.ancestors(id) });
        chain.retain(|m| ready(before, *m) && ready(doc, *m));
        if chain.is_empty() {
            continue;
        }
        let differs = match (before.node(id), doc.node(id)) {
            (Some(a), Some(b)) => !a.same_design(b),
            (None, None) => false,
            _ => true,
        };
        if differs {
            changed.extend(chain);
        }
    }
    for m in changed {
        if doc.node(m).is_some_and(|n| !n.dev_changed) {
            // Its first change since it was marked: keep what it looked like (for Compare changes).
            if let Some(snap) = before.snapshot_subtree(m) {
                doc.dev_snapshots.insert(m, Arc::new(snap));
            }
            if let Some(n) = doc.node_mut(m) {
                n.dev_changed = true;
            }
        }
    }
    // Snapshots last until the change is resolved, the layer is marked again or it's deleted.
    if !doc.dev_snapshots.is_empty() {
        let keep = |d: &Document, id: NodeId| d.node(id).is_some_and(|n| n.dev_status == figcraft_doc::DevStatus::ReadyForDev && n.dev_changed);
        let stale: Vec<NodeId> = doc.dev_snapshots.keys().copied().filter(|id| !keep(doc, *id)).collect();
        for id in stale {
            doc.dev_snapshots.remove(&id);
        }
    }
}

#[cfg(test)]
mod a11y_keys_tests;
#[cfg(test)]
mod al_drag_tests;
#[cfg(test)]
mod al_keys_tests;
#[cfg(test)]
mod al_nesting_tests;
#[cfg(test)]
mod annotation_categories_tests;
#[cfg(test)]
mod annotations_tests;
#[cfg(test)]
mod arc_fields_tests;
#[cfg(test)]
mod arc_handles_tests;
#[cfg(test)]
mod autosave_tests;
#[cfg(test)]
mod binding_lists_tests;
#[cfg(test)]
mod booleans_tests;
#[cfg(test)]
mod collab_ids_tests;
#[cfg(test)]
mod collab_live_tests;
#[cfg(test)]
mod collab_tests;
#[cfg(test)]
mod collab_undo_tests;
#[cfg(test)]
mod comments_polish_tests;
#[cfg(test)]
mod component_codegen_tests;
#[cfg(test)]
mod components_scale_tests;
#[cfg(test)]
mod devcompare_tests;
#[cfg(test)]
mod devfocus_tests;
#[cfg(test)]
mod devinspect_tests;
#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod frame_guides_tests;
#[cfg(test)]
mod frame_label_tests;
#[cfg(test)]
mod frame_tests;
#[cfg(test)]
mod gesture_preview_tests;
#[cfg(test)]
mod html_codegen_tests;
#[cfg(test)]
mod import_export_tests;
#[cfg(test)]
mod invite_links_tests;
#[cfg(test)]
mod keymap_tests;
#[cfg(test)]
mod library_variables_tests;
#[cfg(test)]
mod live_access_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod mask_tests;
#[cfg(test)]
mod measure_tests;
#[cfg(test)]
mod nested_override_tests;
#[cfg(test)]
mod paint_reorder_tests;
#[cfg(test)]
mod paint_tests;
#[cfg(test)]
mod pdf_print_tests;
#[cfg(test)]
mod pixel_preview_tests;
#[cfg(test)]
mod plugins_tests;
#[cfg(test)]
mod progressive_blur_tests;
#[cfg(test)]
mod proto_conditional_tests;
#[cfg(test)]
mod proto_multi_tests;
#[cfg(test)]
mod proto_polish_tests;
#[cfg(test)]
mod proto_present_tests;
#[cfg(test)]
mod proto_responsive_tests;
#[cfg(test)]
mod proto_run_tests;
#[cfg(test)]
mod retained_walk_tests;
#[cfg(test)]
mod rotated_guides_tests;
#[cfg(test)]
mod rotated_select_tests;
#[cfg(test)]
mod scale_tool_tests;
#[cfg(test)]
mod section_contents_tests;
#[cfg(test)]
mod section_tests;
#[cfg(test)]
mod selection_depth_tests;
#[cfg(test)]
mod shapebuilder_strokes_tests;
#[cfg(test)]
mod shapebuilder_tests;
#[cfg(test)]
mod slot_tests;
#[cfg(test)]
mod spelling_tests;
#[cfg(test)]
mod style_tests;
#[cfg(test)]
mod svg_import_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_fill_tests;
#[cfg(test)]
mod text_style_picker_tests;
#[cfg(test)]
mod variable_tests;
#[cfg(test)]
mod variables_editor_tests;
#[cfg(test)]
mod version_preview_tests;
#[cfg(test)]
mod version_tests;
