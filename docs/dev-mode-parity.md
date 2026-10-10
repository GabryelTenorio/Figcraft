# Dev Mode parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created from the ROADMAP's Dev Mode items) · **Target:** Figma Design Dev Mode (desktop app 126.10.7)

**Breadth ~80%, ready ~62% (estimated).** The inspect surface is broad; what's missing connects
designs to real codebases. Remaining: 4–8 h.

| Feature | Status | Notes |
|---|---|---|
| Dev Mode toggle (Shift+D), Inspect panel | done | |
| Properties with their styles and variables; redlines on hover | done | |
| Code: CSS, iOS (SwiftUI), Android (Compose) | done | plus HTML and React (ours); components take properties as parameters, instances are calls |
| Assets list with export | done | |
| Annotations: pinned properties, categories, coloured callouts, filter | done | |
| Ready for dev list, statuses, "Changed" after edits | done | |
| Focus view (one frame, previous / next) | done | |
| Compare changes (before/after side by side or overlaid, property diffs) | done | only against the snapshot taken when a frame was marked ready; not against any named version |
| Code Connect (map components to repository code) | missing | |
| Plugins in Dev Mode (Jira, GitHub, etc.) | missing | needs plugins |
| VS Code extension | missing | FigCraft's MCP server and control channel already serve design data to agents and editors |
| Dev resources links on layers | missing | 1 h |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
