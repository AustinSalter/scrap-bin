# UI Requirements — Dialectic

Standalone specification for frontend implementation. The Rust backend and Python sidecar are fully implemented — this document defines the Tauri frontend that consumes them.

Reference mockup: `dialectic-concept-tidepool.html`

---

## Design Philosophy

Dialectic is a knowledge landscape. Things arrive from your sources, the system clusters them by semantic similarity, and you annotate what you see. The map is the home screen. The stream is periphery. Your notes are first-class fragments.

There are no separate screens — only **three states** of a single surface:

| State | What's visible | Trigger |
|---|---|---|
| **Overview** | Rail + Landscape (full width) | Default on open, click map background, `esc` |
| **Browsing** | Rail + Stream + Landscape + Margin | Click cluster node, click Stream rail icon |
| **Threaded** | Rail + Landscape (highlighted subset) | Search query, thread selection |

The landscape is always present. Everything else is a panel that slides in on top of it.

---

## Design System

### Palette: Tidepool (Locked)

Paper whites, black ink, ocean cerulean accent, lifted signals.

**Backgrounds (Zinc)**

| Token | Value | Usage |
|---|---|---|
| `--bg` | `#ffffff` | App background, canvas |
| `--bg2` | `#fafafa` | Rail, status bar, compose area |
| `--bg3` | `#f4f4f5` | Hover states, search box resting, tag chips |
| `--surface` | `#e4e4e7` | Card surfaces (alias of `--border`) |
| `--rule` | `#d4d4d8` | Dividers, cluster edges, heavier borders |

**Ink (Black)**

| Token | Value | Usage |
|---|---|---|
| `--ghost` | `#a1a1aa` | Placeholders, timestamps, metadata |
| `--muted` | `#71717a` | Secondary labels, section headers |
| `--secondary` | `#52525b` | Body text, fragment content |
| `--primary` | `#18181b` | Titles, selected items, emphasis |
| `--ink` | `#09090b` | Cluster names in margin, window title bar |

**Accent (Cerulean)**

| Token | Value | Usage |
|---|---|---|
| `--accent` | `#5b8def` | Active states, your-note markers, primary actions |
| `--accent-hover` | `#78a3ff` | Hover on accent elements |
| `--accent-bg` | `rgba(91,141,239,0.06)` | Selected items background, active rail icon |
| `--accent-12` | `rgba(91,141,239,0.12)` | Selected stream item border |
| `--accent-20` | `rgba(91,141,239,0.20)` | Strong thread edges, note marker fill |

**Signals (Lifted)**

| Token | Value | Usage |
|---|---|---|
| `--signal-blue` | `#60a5fa` | Twitter/X source dot |
| `--signal-green` | `#34d399` | Obsidian/vault source dot |
| `--signal-amber` | `#fbbf24` | RSS/newsletter source dot (Stratechery, Diff) |
| `--signal-purple` | `#a78bfa` | Readwise source dot |
| `--signal-red` | `#f87171` | New-item indicators, syncing status |

**Semantic**

| Token | Value | Usage |
|---|---|---|
| `--ok` | `#22c55e` | Chroma running, sidecar connected |
| `--warn` | `#f59e0b` | Loading model, pending |
| `--err` | `#ef4444` | Disconnected, error |

**Borders**

| Token | Value | Usage |
|---|---|---|
| `--border` | `#e4e4e7` | Panel borders, fragment dividers, light strokes |
| `--border2` | `#d4d4d8` | Heavier borders, compose area stroke, toolbar btn |

### Typography

| Font | Role | Sizes |
|---|---|---|
| Cormorant Garamond | Display: cluster labels on map, margin cluster name, app logo (δ), thread labels | 20px (margin name), 12px (map labels), 11px (thread pill) |
| Libre Baskerville | Scannable titles: stream item titles, fragment titles | 11px |
| Lora | Body/reading: fragment content, search placeholder, compose area, descriptions | 10.5–11px, always italic in compose |
| Cousine | Monospace metadata: timestamps, source labels, tag chips, status bar, counts, kbd shortcuts | 7–9px |
| Archivo Narrow | UI system text: section headers, toolbar buttons, compose label. Always uppercase, always 600 weight | 9–11px, letter-spacing 0.06–0.12em |

### Layout

- macOS-native window with hidden title bar (`titleBarStyle: "Overlay"`)
- 1400×900 default, 900×600 minimum
- Drag region at top 40px for window dragging
- Cards use 6px border-radius, 1px `--border` stroke
- All panels use 16px horizontal padding
- 8px gap between elements within panels

---

## Components

### Rail (always visible)

Fixed 44px-wide icon sidebar on the far left. `--bg2` background, `--border` right border.

| Position | Icon | Label | Action |
|---|---|---|---|
| Top | `δ` | Logo | — (Cormorant Garamond 18px) |
| 1 | `◎` | Landscape | Set state → Overview |
| 2 | `↓` | Stream | Toggle stream panel. Badge dot (`--signal-red`) when new items |
| 3 | `⌕` | Search | Focus search box, set state → Threaded |
| Bottom | `⚙` | Settings | Open Settings (overlay/modal) |

Active icon: `--accent-bg` background, `--accent` color. Hover: `--bg3` background.

### Landscape Canvas (always visible)

The central surface. Fills all available width between rail and any open panels.

**Background:** `--bg` with faint crosshatch pattern (45deg repeating lines at `rgba(0,0,0,0.02)`) fading to solid `--bg` at edges via radial gradient.

**Toolbar** (top edge, inside landscape):
- Search box: `--bg3` resting, `--accent` border + `--accent-bg` shadow on focus. Lora italic placeholder. `⌘K` badge.
- "Re-cluster" button: `--border2` outline, Archivo Narrow uppercase.
- "↓ Stream" button (Overview state only): `--accent` text, `--accent-20` border. Opens stream panel.

**Cluster Nodes:**
- Positioned via UMAP 2D projection of cluster centroids
- Circle with `--rule` 1.5px border, `--bg` fill
- Size proportional to fragment count (28px–80px diameter)
- Center: fragment count in Cousine mono
- Below: cluster label in Cormorant Garamond 12px `--secondary`
- Below label (large clusters only): subtopic keywords in Cousine 7px `--ghost`
- Hover: border → `--accent`, box-shadow `0 0 0 4px var(--accent-bg)`
- Active (selected): border → `--accent` 2px, fill → `--accent-bg`, count color → `--accent`

**Cluster Edges:**
- SVG lines between related clusters
- Default: `--rule` 1px
- Strong (thread connection): `--accent-20` 1.5px

**New-Item Dots:**
- 6px circles, `--signal-red`, positioned near clusters that received new fragments
- Pulsing animation (2s infinite): opacity 1→0.7, box-shadow 0→4px fade

**Your-Note Markers:**
- 10px diamonds (rotated square), `--accent-20` fill, `--accent` 1px border
- Positioned near the cluster they belong to
- Cursor: pointer

**Satellite Nodes:**
- 8px circles, `--bg3` fill, `--border` 1px stroke
- Represent unclustered or very small clusters

**Status Bar** (bottom edge, inside landscape):
- `--bg2` background, `--border` top border
- Left: service status dots (`--ok`/`--warn`/`--err`) + "Chroma", "Embeddings"
- Center: fragment count, cluster count, thread count (Cousine 8px `--ghost`)
- Right: sync status or "N new since last visit"
- Items separated by 1px `--border` vertical dividers

**Data sources:**
- `clustering_get_all() → Vec<ClusterView>` — cluster positions, labels, fragment counts
- `threads_get_all() → Vec<ThreadView>` — edges between clusters
- `sidecar_status()` — service health for status bar
- `chroma_get_collection_stats()` — fragment/cluster counts

### Stream Panel (left, toggle)

220px wide, slides in from left between rail and landscape. `--bg` background, `--border` right border.

**Header:** "STREAM" (Archivo Narrow 11px `--secondary`) + count badge (Cousine 9px in `--bg3` pill).

**Items grouped by day** with Cousine 8px uppercase `--ghost` date labels.

**Stream Item:**
- 6px border-radius, 1px transparent border
- Source dot (5px, signal color by source type) + origin text (Cousine 8px `--ghost`) + relative time (Cousine 8px `--ghost`, right-aligned)
- Title: Libre Baskerville 11px `--primary`
- Cluster assignment badge: Cousine 7px `--accent` on `--accent-bg` pill. "→ Compression"
- New items: 2px `--signal-red` left border
- Selected: `--accent-bg` fill, `--accent-12` border
- Hover: `--bg3` fill, `--border` border

**Behavior:**
- Click item → select its cluster on the landscape, open margin panel
- Items are all recently ingested fragments, reverse chronological
- Auto-updates via `vault-file-changed` Tauri event

### Margin Panel (right, contextual)

320px wide, slides in from right when a cluster is selected. `--bg` background, `--border` left border.

**Header:**
- Cluster name: Cormorant Garamond 20px `--ink`
- Meta line: Cousine 8px `--ghost` — "24 fragments · 3 sources · 2 notes"
- Close button (×): 24px, `--ghost`, hover → `--bg3` + `--primary`

**Fragment List** (scrollable body):
- Each fragment separated by 1px `--border` bottom border
- Source line: 5px dot (signal color) + Cousine 8px `--ghost` source + relative time
- Content: Lora 10.5px `--secondary`, 1.6 line-height
- Tags (if present): Cousine 8px `--muted` chips on `--bg2` with `--border` 1px stroke

**Your Notes (first-class fragments):**
- 2px `--accent` left border
- `--accent-bg` background
- Source dot uses `--accent`
- Source text: "Your note · 1h ago"
- Content: Lora 10.5px `--primary`, italic

**Fragment Actions (on hover or click):**
- "Open in Obsidian" (vault fragments only): `obsidian://open?vault=...&file=...`
- "Move to Cluster" dropdown
- Copy content to clipboard

**Compose Area** (pinned at bottom):
- `--bg2` background, `--border2` top border
- Label: Archivo Narrow 9px uppercase `--ghost` with diamond icon (12px, `--accent-20` fill, `--accent` border, rotated 45deg)
- Text area: `--bg` background, `--border2` 1.5px border, 6px radius
- Font: Lora 10.5px `--secondary` italic
- Hint line: Cousine 8px `--ghost` — "Markdown supported" left, `⌘ ↵ to save` right
- On save: note gets embedded, clustered, and added to the landscape as a fragment with `source_type: "note"`

**Data sources:**
- `clustering_get_fragments(cluster_id) → Vec<Fragment>` — fragment list
- `clustering_rename(cluster_id, label) → void` — inline editing of cluster name (double-click)
- `clustering_move_fragment(fragment_id, from, to) → void` — drag fragment to another cluster

### Search (integrated into landscape toolbar)

Search is not a separate screen. It operates through the toolbar search box and shifts the landscape to **Threaded** state.

**Behavior:**
- `⌘K` focuses the search box
- Debounced input (300ms) triggers `search_all(SearchParams)`
- Results: thread matches highlight on the map (relevant clusters glow, others dim to 0.25 opacity)
- Thread label floats as a pill on the strongest highlighted edge: `--accent` background, white Cormorant Garamond 11px
- Status bar shows: thread name, cluster count, fragment count, similarity score
- `esc` clears search, restores full landscape

**Threaded State visuals:**
- Highlighted cluster edges: `--accent` 2.5px at 0.5 opacity, with 8px glow line at 0.06 opacity
- Non-highlighted edges: `--rule` 1px at 0.3 opacity
- Non-highlighted clusters: full opacity → 0.25

**Data sources:**
- `search_all(SearchParams) → Vec<SearchResult>`
- `threads_get_all() → Vec<ThreadView>` — for thread highlighting
- `threads_detect(ThreadParams) → Vec<ThreadView>` — on-demand thread detection

### Settings (overlay)

Modal or full-panel overlay, triggered by `⚙` in rail. Not a navigation destination — it overlays the current state and dismisses back to it.

**Sections:**

**Vault Configuration**
- Vault path input with folder picker (Tauri `dialog.open`)
- "Start Watching" / "Stop Watching" toggle
- Display: file count, last event timestamp

**Source Connections**
- Twitter: file path input for JSON export + "Import" button
- Readwise: API key input (password field) + "Test Connection" + "Sync Now"
- Podcasts: directory path input with folder picker + "Import"

**Clustering Parameters**
- `min_cluster_size`: slider 2–20, default 5
- `min_samples`: slider 1–10, default 3
- `similarity_threshold`: slider 0.5–0.9, default 0.65
- `max_threads`: number input, default 50

**System**
- Data directory (read-only): `config_get_data_dir()`
- Sidecar ports (read-only): Chroma 8000, Python 50051
- Sidecar status with Start All / Stop All buttons

**Data contracts:**
- `config_get() → AppConfig`
- `config_set(AppConfig) → void`
- `source_readwise_configure(api_key) → void`
- `source_readwise_check_connection() → bool`

---

## Cluster Operations

These interactions live on the landscape and margin panel. No separate Cluster Explorer screen.

| Operation | Trigger | Command |
|---|---|---|
| Re-cluster | Toolbar button (shows spinner) | `clustering_run(ClusterParams) → Vec<ClusterView>` |
| Rename cluster | Double-click label (map or margin) | `clustering_rename(cluster_id, label) → void` |
| Pin label | Toggle pin icon on cluster node | `clustering_pin_label(cluster_id, pinned) → void` |
| Move fragment | Drag fragment in margin to another cluster node | `clustering_move_fragment(fragment_id, from, to) → void` |
| Merge clusters | Drag one cluster node onto another | `clustering_merge(ids) → ClusterView` |
| Split cluster | Select fragments in margin + "Split" button | `clustering_split(cluster_id, fragment_ids) → Vec<ClusterView>` |
| View orphans | Collapsible section at bottom of margin (when viewing orphan pseudo-cluster) | `clustering_get_orphans() → Vec<Fragment>` |

---

## Thread Operations

Threads are a highlighting mode on the landscape, not a separate destination.

| Operation | Trigger | Command |
|---|---|---|
| Detect threads | Toolbar button or search triggering thread matches | `threads_detect(ThreadParams) → Vec<ThreadView>` |
| Name thread | Double-click floating thread label | `threads_name(id, label) → void` |
| Confirm thread | Checkmark on thread pill (or in margin context) | `threads_confirm(id) → void` |
| Dismiss thread | × on thread pill | `threads_dismiss(id) → void` |

---

## Tauri Event Subscriptions

| Event | Payload | UI Response |
|---|---|---|
| `vault-file-changed` | `Vec<FileChangeEvent>` | Add items to stream, pulse new-dots on affected clusters, update status bar counts, badge the Stream rail icon |

---

## State Management

Recommended stores (Zustand):

| Store | State | Sources |
|---|---|---|
| `appStore` | current UI state (overview/browsing/threaded), selected cluster ID, selected thread ID | UI interactions |
| `configStore` | AppConfig, vault path, sidecar status | `config_get()`, `sidecar_status()` |
| `landscapeStore` | clusters, orphans, node positions (UMAP), edges, loading | `clustering_*` commands |
| `streamStore` | recent fragments (ring buffer, max 100), new-since-last count | `vault-file-changed` event, `clustering_get_all()` |
| `searchStore` | query, results, highlighted cluster IDs, loading | `search_*` commands |

---

## Interaction Inventory

### Buttons
- Re-cluster (landscape toolbar, shows spinner)
- Stream toggle (landscape toolbar or rail icon)
- Start/Stop Watching (settings)
- Start/Stop Sidecars (settings)
- Import Twitter JSON (settings, file picker)
- Import Podcasts (settings, folder picker)
- Sync Readwise (settings)
- Test Readwise Connection (settings)
- Split Cluster (margin panel, requires fragments selected)
- Open in Obsidian (margin fragment action, vault fragments only)
- Copy to Clipboard (margin fragment action)

### Drag & Drop
- Fragment (margin) → Cluster node (landscape): move fragment
- Cluster node → Cluster node: merge clusters

### Inline Editing
- Cluster label: double-click on map label or margin header
- Thread label: double-click on floating thread pill

### Keyboard Shortcuts
- `⌘K` — Focus search (enters Threaded state)
- `⌘R` — Re-cluster
- `⌘↵` — Save note (in compose area)
- `Escape` — Close margin panel / clear search / return to Overview
- `⌘,` — Open Settings

---

## Loading & Error States

- **Loading:** Skeleton shimmer for stream items and margin fragments. Landscape shows faint pulsing circles at node positions.
- **Empty state (no clusters):** Landscape shows centered message: "No clusters yet. Start by watching a vault." with action button → Settings.
- **Empty state (no fragments in cluster):** Margin body shows "No fragments in this cluster."
- **Error toast:** Bottom-right, auto-dismiss 5s. `--err` accent for errors, `--warn` for warnings.
- **Sync progress:** Status bar shows "Syncing N items…" with `--signal-red` dot. On complete, dot clears and counts update.
- **Re-clustering:** Toolbar button shows spinner. Landscape nodes animate to new positions on completion.

---

## Responsive Behavior

Not required — macOS desktop only. Minimum 900×600 window.

---

## What Changed from v1

| v1 (7 screens) | v2 (3 states) | Rationale |
|---|---|---|
| Dashboard | Landscape Overview state | The map IS the dashboard — cluster counts, service health, new items are all visible |
| Cluster Explorer (card grid) | Landscape + Margin panel | Click a node instead of a card. Same data, spatial context preserved |
| Thread Explorer (list) | Threaded state (highlight mode) | Threads are visual overlays, not a list to scroll |
| Search (separate screen) | Toolbar search + Threaded state | Search filters the landscape in place |
| Fragment Detail (modal) | Expanded fragment in margin panel | Fragment content is already in the margin; expand in-place |
| Source Status (tab view) | Folded into Settings | Low-frequency interaction, doesn't need its own nav item |
| 5-item sidebar | 3-item rail (Landscape, Stream, Settings) | Fewer destinations, panels instead of pages |
