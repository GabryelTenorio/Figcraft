//! The incumbent's main-menu tree with stable FigCraft command ids. A command registered under a
//! catalog id lights up its menu item, and `cargo xtask parity` measures coverage against it.
//!
//! Entries: (menu path, label, command id, default shortcut). `/` separates submenus; a label of
//! `-` is a separator.

pub struct MenuItem {
    pub menu: &'static str,
    pub label: &'static str,
    pub id: &'static str,
    pub shortcut: &'static str,
}

macro_rules! m {
    ($menu:expr, $label:expr, $id:expr, $sc:expr) => {
        MenuItem { menu: $menu, label: $label, id: $id, shortcut: $sc }
    };
}

pub const SEP: &str = "-";

pub static MENU: &[MenuItem] = &[
    // File
    m!("File", "New design file", "file.new", "Cmd+N"),
    m!("File", "Open…", "file.open", "Cmd+O"),
    m!("File", "Import from file…", "file.import", "Shift+Cmd+I"),
    m!("File", "Place image…", "file.placeImage", "Shift+Cmd+K"),
    m!("File", SEP, "", ""),
    m!("File", "Save", "file.save", "Cmd+S"),
    m!("File", "Save as…", "file.saveAs", "Shift+Cmd+S"),
    m!("File", "Save to version history…", "file.saveVersion", "Alt+Cmd+S"),
    m!("File", "Show version history", "file.versionHistory", ""),
    m!("File", SEP, "", ""),
    m!("File", "Export…", "file.export", "Shift+Cmd+E"),
    m!("File", "Export frames to PDF…", "file.exportFramesPdf", ""),
    m!("File", "Export selection as SVG…", "file.exportSvg", ""),
    m!("File", SEP, "", ""),
    m!("File", "Duplicate", "file.duplicate", ""),
    m!("File", "Rename", "file.rename", ""),
    m!("File", "Revert to saved", "file.revert", ""),
    m!("File", "Close tab", "file.close", "Cmd+W"),
    m!("File", "Open recent", "file.openRecent", ""),
    // Edit
    m!("Edit", "Undo", "edit.undo", "Cmd+Z"),
    m!("Edit", "Redo", "edit.redo", "Shift+Cmd+Z"),
    m!("Edit", SEP, "", ""),
    m!("Edit/Copy as", "Copy as text", "edit.copyAsText", ""),
    m!("Edit/Copy as", "Copy as CSS", "edit.copyAsCss", ""),
    m!("Edit/Copy as", "Copy as SVG", "edit.copyAsSvg", ""),
    m!("Edit/Copy as", "Copy as PNG", "edit.copyAsPng", "Shift+Cmd+C"),
    m!("Edit/Copy as", "Copy link to selection", "edit.copyLink", ""),
    m!("Edit", "Cut", "edit.cut", "Cmd+X"),
    m!("Edit", "Copy", "edit.copy", "Cmd+C"),
    m!("Edit", "Paste", "edit.paste", "Cmd+V"),
    m!("Edit", "Paste over selection", "edit.pasteOver", "Shift+Cmd+V"),
    m!("Edit", "Paste to replace", "edit.pasteReplace", "Shift+Cmd+R"),
    m!("Edit", "Duplicate", "edit.duplicate", "Cmd+D"),
    m!("Edit", "Delete", "edit.delete", "Delete"),
    m!("Edit", SEP, "", ""),
    m!("Edit", "Find and replace…", "edit.findReplace", "Cmd+F"),
    m!("Edit", "Copy properties", "edit.copyProperties", "Alt+Cmd+C"),
    m!("Edit", "Paste properties", "edit.pasteProperties", "Alt+Cmd+V"),
    m!("Edit", "Pick color", "edit.pickColor", "I"),
    m!("Edit", "Set default properties", "edit.setDefaultProperties", ""),
    m!("Edit", SEP, "", ""),
    m!("Edit", "Select all", "select.all", "Cmd+A"),
    m!("Edit", "Select matching layers", "select.matching", "Alt+Cmd+A"),
    m!("Edit", "Select none", "select.none", "Escape"),
    m!("Edit", "Select inverse", "select.inverse", "Shift+Cmd+A"),
    m!("Edit/Select all with same", "Fill", "select.sameFill", ""),
    m!("Edit/Select all with same", "Stroke", "select.sameStroke", ""),
    m!("Edit/Select all with same", "Effect", "select.sameEffect", ""),
    m!("Edit/Select all with same", "Text properties", "select.sameText", ""),
    m!("Edit/Select all with same", "Font", "select.sameFont", ""),
    m!("Edit/Select all with same", "Instance", "select.sameInstance", ""),
    m!("Edit/Select all with same", "Properties", "select.sameProperties", ""),
    // View
    m!("View", "Pixel grid", "view.pixelGrid", "Shift+'"),
    m!("View", "Layout grids", "view.layoutGrids", "Ctrl+G"),
    m!("View", "Rulers", "view.rulers", "Shift+R"),
    m!("View", "Slices", "view.slices", ""),
    m!("View", "Comments", "view.comments", "Shift+C"),
    m!("View", SEP, "", ""),
    m!("View/Outlines", "Show outlines", "view.outlines", "Cmd+Y"),
    m!("View/Outlines", "Show outlines for selection", "view.outlinesSelection", ""),
    m!("View", "Pixel preview", "view.pixelPreview", ""),
    m!("View", "Mask outlines", "view.maskOutlines", ""),
    m!("View", "Frame outlines", "view.frameOutlines", ""),
    m!("View", SEP, "", ""),
    m!("View/Panels", "Show/Hide UI", "view.toggleUi", "Cmd+\\"),
    m!("View/Panels", "Layers", "view.layers", "Alt+1"),
    m!("View/Panels", "Assets", "view.assets", "Alt+2"),
    m!("View/Panels", "Design", "view.design", "Alt+8"),
    m!("View/Panels", "Prototype", "view.prototype", "Alt+9"),
    m!("View", SEP, "", ""),
    m!("View", "Zoom in", "view.zoomIn", "Cmd+="),
    m!("View", "Zoom out", "view.zoomOut", "Cmd+-"),
    m!("View", "Zoom to 100%", "view.zoom100", "Shift+0"),
    m!("View", "Zoom to 50%", "view.zoom50", ""),
    m!("View", "Zoom to 200%", "view.zoom200", ""),
    m!("View", "Zoom to fit", "view.zoomFit", "Shift+1"),
    m!("View", "Zoom to selection", "view.zoomSelection", "Shift+2"),
    m!("View", "Zoom to previous frame", "view.prevFrame", "Shift+N"),
    m!("View", "Zoom to next frame", "view.nextFrame", "N"),
    m!("View", SEP, "", ""),
    m!("View", "Previous page", "view.prevPage", "PageUp"),
    m!("View", "Next page", "view.nextPage", "PageDown"),
    m!("View/Theme", "Light", "view.themeLight", ""),
    m!("View/Theme", "Dark", "view.themeDark", ""),
    m!("View/Theme", "Follow system", "view.themeSystem", ""),
    // Object
    m!("Object", "Frame selection", "object.frameSelection", "Alt+Cmd+G"),
    m!("Object", "Group selection", "object.group", "Cmd+G"),
    m!("Object", "Ungroup selection", "object.ungroup", "Shift+Cmd+G"),
    m!("Object", "Remove frame", "object.removeFrame", ""),
    m!("Object", "Resize to fit", "object.resizeToFit", "Alt+Shift+Cmd+R"),
    m!("Object", SEP, "", ""),
    m!("Object", "Add auto layout", "layout.add", "Shift+A"),
    m!("Object", "Remove auto layout", "layout.remove", "Alt+Shift+A"),
    m!("Object", "Suggest auto layout", "layout.suggest", ""),
    m!("Object", SEP, "", ""),
    m!("Object", "Create component", "component.create", "Alt+Cmd+K"),
    m!("Object", "Create multiple components", "component.createMultiple", ""),
    m!("Object", "Combine as variants", "component.combineVariants", ""),
    m!("Object", "Reset all changes", "component.resetInstance", ""),
    m!("Object", "Detach instance", "component.detach", "Alt+Cmd+B"),
    m!("Object", "Go to main component", "component.goToMain", ""),
    m!("Object", "Push changes to main component", "component.pushChanges", ""),
    m!("Object", "Restore main component", "component.restoreMain", ""),
    m!("Object", SEP, "", ""),
    m!("Object", "Use as mask", "object.mask", "Ctrl+Cmd+M"),
    m!("Object", "Release mask", "object.releaseMask", ""),
    m!("Object", "Set as thumbnail", "object.setThumbnail", ""),
    m!("Object", SEP, "", ""),
    m!("Object", "Bring to front", "arrange.front", "Alt+Cmd+]"),
    m!("Object", "Bring forward", "arrange.forward", "Cmd+]"),
    m!("Object", "Send backward", "arrange.backward", "Cmd+["),
    m!("Object", "Send to back", "arrange.back", "Alt+Cmd+["),
    m!("Object", SEP, "", ""),
    m!("Object", "Flip horizontal", "object.flipH", "Shift+H"),
    m!("Object", "Flip vertical", "object.flipV", "Shift+V"),
    m!("Object", "Rotate 180°", "object.rotate180", ""),
    m!("Object", "Rotate 90° left", "object.rotateLeft", ""),
    m!("Object", "Rotate 90° right", "object.rotateRight", ""),
    m!("Object", SEP, "", ""),
    m!("Object", "Flatten", "vector.flatten", "Cmd+E"),
    m!("Object", "Outline stroke", "vector.outlineStroke", "Alt+Cmd+O"),
    m!("Object/Boolean groups", "Union", "bool.union", ""),
    m!("Object/Boolean groups", "Subtract", "bool.subtract", ""),
    m!("Object/Boolean groups", "Intersect", "bool.intersect", ""),
    m!("Object/Boolean groups", "Exclude", "bool.exclude", ""),
    m!("Object/Boolean groups", "Flatten", "bool.flatten", ""),
    m!("Object", "Rasterize selection", "object.rasterize", ""),
    m!("Object", SEP, "", ""),
    m!("Object", "Show/Hide selection", "object.toggleVisible", "Shift+Cmd+H"),
    m!("Object", "Lock/Unlock selection", "object.toggleLock", "Shift+Cmd+L"),
    m!("Object", "Hide other layers", "object.hideOthers", ""),
    m!("Object", "Collapse layers", "layers.collapseAll", "Alt+L"),
    m!("Object", SEP, "", ""),
    m!("Object", "Remove fill", "object.removeFill", ""),
    m!("Object", "Remove stroke", "object.removeStroke", ""),
    m!("Object", "Swap fill and stroke", "object.swapFillStroke", "Shift+X"),
    m!("Object", SEP, "", ""),
    m!("Object", "Rename", "object.rename", "Cmd+R"),
    m!("Object", "Batch rename…", "object.batchRename", ""),
    m!("Object", "Move to page", "object.moveToPage", ""),
    m!("Object/Select layer", "Select children", "select.children", "Enter"),
    m!("Object/Select layer", "Select parent", "select.parent", "Shift+Enter"),
    m!("Object/Select layer", "Select next sibling", "select.next", "Tab"),
    m!("Object/Select layer", "Select previous sibling", "select.previous", "Shift+Tab"),
    // Text
    m!("Text", "Bold", "text.bold", "Cmd+B"),
    m!("Text", "Italic", "text.italic", "Cmd+I"),
    m!("Text", "Underline", "text.underline", "Cmd+U"),
    m!("Text", "Strikethrough", "text.strikethrough", "Shift+Cmd+X"),
    m!("Text", "Create link", "text.link", "Cmd+K"),
    m!("Text", SEP, "", ""),
    m!("Text", "Bulleted list", "text.bulletList", "Shift+Cmd+8"),
    m!("Text", "Numbered list", "text.numberList", "Shift+Cmd+7"),
    m!("Text", SEP, "", ""),
    m!("Text/Alignment", "Text align left", "text.alignLeft", "Alt+Cmd+L"),
    m!("Text/Alignment", "Text align center", "text.alignCenter", "Alt+Cmd+T"),
    m!("Text/Alignment", "Text align right", "text.alignRight", "Alt+Cmd+R"),
    m!("Text/Alignment", "Text align justified", "text.alignJustify", "Alt+Cmd+J"),
    m!("Text/Adjust", "Increase font size", "text.sizeUp", "Shift+Cmd+."),
    m!("Text/Adjust", "Decrease font size", "text.sizeDown", "Shift+Cmd+,"),
    m!("Text/Adjust", "Increase font weight", "text.weightUp", "Alt+Cmd+."),
    m!("Text/Adjust", "Decrease font weight", "text.weightDown", "Alt+Cmd+,"),
    m!("Text/Adjust", "Increase line height", "text.lineHeightUp", "Alt+Shift+."),
    m!("Text/Adjust", "Decrease line height", "text.lineHeightDown", "Alt+Shift+,"),
    m!("Text/Adjust", "Increase letter spacing", "text.trackingUp", "Alt+."),
    m!("Text/Adjust", "Decrease letter spacing", "text.trackingDown", "Alt+,"),
    m!("Text/Case", "Original case", "text.caseOriginal", ""),
    m!("Text/Case", "Uppercase", "text.caseUpper", ""),
    m!("Text/Case", "Lowercase", "text.caseLower", ""),
    m!("Text/Case", "Title case", "text.caseTitle", ""),
    // Arrange
    m!("Arrange", "Align left", "align.left", "Alt+A"),
    m!("Arrange", "Align horizontal centers", "align.hcenter", "Alt+H"),
    m!("Arrange", "Align right", "align.right", "Alt+D"),
    m!("Arrange", "Align top", "align.top", "Alt+W"),
    m!("Arrange", "Align vertical centers", "align.vcenter", "Alt+V"),
    m!("Arrange", "Align bottom", "align.bottom", "Alt+S"),
    m!("Arrange", SEP, "", ""),
    m!("Arrange", "Distribute horizontal spacing", "distribute.horizontal", "Alt+Shift+H"),
    m!("Arrange", "Distribute vertical spacing", "distribute.vertical", "Alt+Shift+V"),
    m!("Arrange", "Tidy up", "arrange.tidy", "Ctrl+Alt+T"),
    m!("Arrange", SEP, "", ""),
    m!("Arrange", "Pack horizontal", "arrange.packH", ""),
    m!("Arrange", "Pack vertical", "arrange.packV", ""),
    m!("Arrange", SEP, "", ""),
    m!("Arrange", "Round to pixels", "arrange.roundToPixels", ""),
    // Vector
    m!("Vector", "Join selection", "vector.join", "Cmd+J"),
    m!("Vector", "Smooth join selection", "vector.smoothJoin", "Shift+Cmd+J"),
    m!("Vector", "Delete and heal selection", "vector.deleteHeal", "Shift+Delete"),
    m!("Vector", "Simplify vector", "vector.simplify", ""),
    m!("Vector", "Reverse path direction", "vector.reverse", ""),
    m!("Vector", "Offset path", "vector.offset", ""),
    // Plugins
    m!("Plugins", "Run last plugin", "plugins.runLast", "Alt+Cmd+P"),
    m!("Plugins", "Manage plugins…", "plugins.manage", ""),
    // Preferences
    m!("Preferences", "Snap to geometry", "view.snapGeometry", ""),
    m!("Preferences", "Snap to objects", "view.snapObjects", ""),
    m!("Preferences", "Snap to pixel grid", "view.snapPixel", "Shift+Cmd+'"),
    m!("Preferences", "Keep tool selected after use", "pref.keepTool", ""),
    m!("Preferences", "Highlight layers on hover", "pref.highlightHover", ""),
    m!("Preferences", "Rename duplicated layers", "pref.renameDuplicates", ""),
    m!("Preferences", "Show dimensions on objects", "pref.showDimensions", ""),
    m!("Preferences", "Use number keys for opacity", "pref.numberKeysOpacity", ""),
    m!("Preferences", "Nudge amount…", "pref.nudge", ""),
    m!("Preferences", "Preferences…", "app.preferences", "Cmd+,"),
    // Libraries
    m!("Libraries", "Open libraries", "libraries.open", "Alt+3"),
    m!("Libraries", "Publish library…", "libraries.publish", ""),
    // Help
    m!("Help", "Keyboard shortcuts", "help.shortcuts", "Ctrl+Shift+?"),
    m!("Help", "Help page", "help.page", ""),
    m!("Help", "About FigCraft", "help.about", ""),
    // Toolbar
    m!("Tools", "Move", "tool.move", "V"),
    m!("Tools", "Scale", "tool.scale", "K"),
    m!("Tools", "Hand", "tool.hand", "H"),
    m!("Tools", "Frame", "tool.frame", "F"),
    m!("Tools", "Section", "tool.section", "Shift+S"),
    m!("Tools", "Slice", "tool.slice", "S"),
    m!("Tools", "Rectangle", "tool.rectangle", "R"),
    m!("Tools", "Line", "tool.line", "L"),
    m!("Tools", "Arrow", "tool.arrow", "Shift+L"),
    m!("Tools", "Ellipse", "tool.ellipse", "O"),
    m!("Tools", "Polygon", "tool.polygon", ""),
    m!("Tools", "Star", "tool.star", ""),
    m!("Tools", "Pen", "tool.pen", "P"),
    m!("Tools", "Pencil", "tool.pencil", "Shift+P"),
    m!("Tools", "Text", "tool.text", "T"),
    m!("Tools", "Comment", "tool.comment", "C"),
    m!("Tools", "Actions", "app.actions", "Cmd+K"),
];

/// Items that are real commands (not separators).
pub fn items() -> impl Iterator<Item = &'static MenuItem> {
    MENU.iter().filter(|m| m.label != SEP)
}

/// Coverage: (implemented, total, missing ids). `extra` lists ids implemented by the UI layer.
pub fn parity(extra: &[&str]) -> (usize, usize, Vec<&'static str>) {
    let mut done = 0;
    let mut missing = Vec::new();
    let mut total = 0;
    for m in items() {
        total += 1;
        if crate::find_command(m.id).is_some() || extra.contains(&m.id) {
            done += 1;
        } else {
            missing.push(m.id);
        }
    }
    (done, total, missing)
}

/// The default shortcut for a command id.
pub fn shortcut(id: &str) -> Option<&'static str> {
    MENU.iter().find(|m| m.id == id && !m.shortcut.is_empty()).map(|m| m.shortcut)
}
