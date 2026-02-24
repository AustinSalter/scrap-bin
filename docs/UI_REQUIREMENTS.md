# UI Requirements — Dialectic RL Ingestion App

Standalone specification for Figma design sessions. The Rust backend and Python sidecar are fully implemented — this document defines the frontend that consumes them.

## Design System

### Tokens

| Token | Value | Usage |
|---|---|---|
| `--bg-primary` | `#1a1a1a` | App background |
| `--bg-surface` | `#242424` | Cards, panels |
| `--bg-elevated` | `#2e2e2e` | Hover states, dropdowns |
| `--bg-input` | `#333333` | Input fields, search bars |
| `--text-primary` | `#fafafa` | Body text |
| `--text-secondary` | `#a0a0a0` | Labels, metadata |
| `--text-muted` | `#666666` | Placeholders, disabled |
| `--accent` | `#2563eb` | Primary actions, links |
| `--accent-hover` | `#3b82f6` | Hover on accent elements |
| `--success` | `#22c55e` | Running, connected |
| `--warning` | `#f59e0b` | Loading, pending |
| `--error` | `#ef4444` | Errors, disconnected |
| `--border` | `#333333` | Card borders, dividers |
| `--font-mono` | `JetBrains Mono, monospace` | Code, fragment content |
| `--font-sans` | `Inter, system-ui, sans-serif` | UI text |

### Layout

- macOS-native window with hidden title bar (`titleBarStyle: "Overlay"`)
- 1400×900 default, 900×600 minimum
- Drag region at top 40px for window dragging
- Sidebar (240px fixed) + main content area
- All panels use 16px padding, 8px gap between elements
- Cards use 12px border-radius, 1px `--border` stroke

---

## Screens

### 1. Dashboard (Home)

**Purpose:** At-a-glance status of the ingestion system.

**Layout:** 2-column grid of status cards + recent activity list below.

**Components:**

| Component | Data Source | Interaction |
|---|---|---|
| Vault Status Card | `watcher_is_active()`, `watcher_get_vault_path()` | Click → Settings |
| Sidecar Status Card | `sidecar_status()` | Start/Stop buttons |
| Collection Stats Grid | `chroma_get_collection_stats()` | Click collection → that source's fragments |
| Cluster Summary | `clustering_get_all()` count | Click → Cluster Explorer |
| Thread Summary | `threads_get_all()` count | Click → Thread Explorer |
| Recent Activity Feed | `vault-file-changed` Tauri events | Auto-scrolling event log |

**Sidecar Status Card detail:**
- Chroma: green dot + "Running on :8000" or red dot + "Stopped"
- Python Sidecar: green dot + "Model ready (nomic-embed-text)" or yellow dot + "Loading model..." or red dot + "Stopped"
- Start All / Stop All buttons

### 2. Settings

**Purpose:** Configure data sources and system parameters.

**Sections:**

**2a. Vault Configuration**
- Vault path input with folder picker (Tauri dialog)
- "Start Watching" / "Stop Watching" toggle
- Display: file count, last event timestamp

**2b. Source Connections**
- Twitter: file path input for JSON export + Import button
- Readwise: API key input (password field) + "Test Connection" button + "Sync Now" button
- Podcasts: directory path input with folder picker + Import button

**2c. Embedding & Clustering**
- Clustering params: min_cluster_size (slider, 2–20, default 5), min_samples (slider, 1–10, default 3)
- Thread detection: similarity_threshold (slider, 0.5–0.9, default 0.65), max_threads (number input, default 50)

**2d. System**
- Data directory display (read-only): `config_get_data_dir()`
- Sidecar ports (read-only): Chroma 8000, Python 50051

**Data contracts:**
- `config_get() → AppConfig`
- `config_set(AppConfig) → void`
- `source_readwise_configure(api_key) → void`
- `source_readwise_check_connection() → bool`

### 3. Cluster Explorer

**Purpose:** Browse, reshape, and label clusters.

**Layout:** Toolbar at top, scrollable card grid below. Click card → expanded cluster view (right panel or modal).

**Toolbar:**
- Re-cluster button: triggers `clustering_run(params)` with loading spinner
- View toggle: Grid / List
- Sort: by size (desc), by label (alpha), by newest fragment
- Filter: source type checkboxes (vault, twitter, readwise, podcasts)

**Cluster Card (grid item):**
- Display label (editable inline on double-click)
- Fragment count badge
- Top 3 fragment previews (first 80 chars each, truncated)
- Source type indicator dots
- Pin/unpin icon (prevents auto-relabel)

**Expanded Cluster View (right panel):**
- Cluster label (large, editable)
- Full fragment list (scrollable)
- Each fragment row: content preview, source type icon, source path, token count
- Click fragment → Fragment Detail (new panel or modal)
- Drag fragment to another cluster card → `clustering_move_fragment()`
- Select multiple fragments + "Split to New Cluster" button → `clustering_split()`
- Merge: drag one cluster card onto another, or select 2+ clusters + "Merge" button → `clustering_merge()`

**Orphan Bin:**
- Separate section below clusters (collapsible)
- Same fragment list, but no cluster label
- Drag orphan fragments into clusters

**Data contracts:**
- `clustering_run(ClusterParams) → Vec<ClusterView>`
- `clustering_get_all() → Vec<ClusterView>`
- `clustering_get_fragments(cluster_id) → Vec<Fragment>`
- `clustering_get_orphans() → Vec<Fragment>`
- `clustering_merge(ids) → ClusterView`
- `clustering_split(cluster_id, fragment_ids) → Vec<ClusterView>`
- `clustering_move_fragment(fragment_id, from, to) → void`
- `clustering_rename(cluster_id, label) → void`
- `clustering_pin_label(cluster_id, pinned) → void`

### 4. Thread Explorer

**Purpose:** View and manage cross-cluster connections.

**Layout:** List of thread cards, each showing the two connected clusters.

**Thread Card:**
- Source cluster label ←→ Target cluster label
- Similarity score (percentage bar)
- Suggested label (editable)
- Confirm (checkmark) / Dismiss (X) buttons
- Click → expanded view showing overlapping fragments from both clusters

**Toolbar:**
- "Detect Threads" button with threshold slider
- Filter: confirmed only, unconfirmed only, all
- Sort: by similarity (desc), by label (alpha)

**Data contracts:**
- `threads_detect(ThreadParams) → Vec<ThreadView>`
- `threads_get_all() → Vec<ThreadView>`
- `threads_name(id, label) → void`
- `threads_confirm(id) → void`
- `threads_dismiss(id) → void`

### 5. Search

**Purpose:** Cross-collection semantic search.

**Layout:** Search bar at top, results list below, filters in side panel.

**Search Bar:**
- Full-width text input with search icon
- Debounced input (300ms) triggers search
- Result count badge

**Filters (collapsible side panel):**
- Source type checkboxes
- Cluster filter dropdown
- Result count slider (5, 10, 20, 50)

**Result Item:**
- Content text (highlighted matching terms if possible)
- Source type icon + source path
- Distance score (lower = more relevant)
- Cluster label badge (if assigned)
- Click → Fragment Detail

**Data contracts:**
- `search_all(SearchParams) → Vec<SearchResult>`
- `search_collection(collection, SearchParams) → Vec<SearchResult>`

### 6. Fragment Detail

**Purpose:** View full content and metadata for a single fragment.

**Layout:** Modal or right panel overlay.

**Content:**
- Full fragment text (monospace, scrollable)
- Metadata table:
  - Source type, source path, chunk index
  - Heading path (breadcrumb)
  - Tags (chip list)
  - Token count
  - Modified at (relative time)
  - Content hash (truncated)
  - Cluster ID + cluster label
- Actions:
  - "Open in Obsidian" (for vault fragments): opens `obsidian://open?vault=...&file=...`
  - "Move to Cluster" dropdown
  - Copy content to clipboard

### 7. Source Status

**Purpose:** View import history and status per source.

**Layout:** Tab bar for each source type, content below.

**Per Source Tab:**
- Last import timestamp
- Item count
- Import history (table: date, items imported, items skipped, errors)
- Re-import button
- Configure button → Settings section

---

## Tauri Event Subscriptions

The frontend should listen for these events (via `listen()` from `@tauri-apps/api/event`):

| Event | Payload | UI Response |
|---|---|---|
| `vault-file-changed` | `Vec<FileChangeEvent>` | Update activity feed, show toast, trigger re-index indicator |

---

## State Management

Recommended stores (Zustand or similar):

| Store | State | Sources |
|---|---|---|
| `configStore` | AppConfig, vault path, sidecar status | `config_get()`, `sidecar_status()` |
| `clusterStore` | clusters, orphans, selected cluster, loading | `clustering_*` commands |
| `threadStore` | threads, selected thread, loading | `threads_*` commands |
| `searchStore` | query, results, filters, loading | `search_*` commands |
| `activityStore` | recent events (ring buffer, max 100) | `vault-file-changed` event |

---

## Interaction Inventory

### Buttons
- Start/Stop Watching (toggle)
- Start/Stop Sidecars (toggle)
- Re-cluster (action, shows spinner)
- Detect Threads (action, shows spinner)
- Import Twitter JSON (action with file picker)
- Import Podcasts (action with folder picker)
- Sync Readwise (action)
- Test Readwise Connection (action)
- Merge Clusters (action, requires 2+ selected)
- Split Cluster (action, requires fragments selected)
- Confirm Thread / Dismiss Thread (action)
- Open in Obsidian (action, vault fragments only)
- Copy to Clipboard (action)

### Drag & Drop
- Fragment → Cluster card (move fragment)
- Cluster card → Cluster card (merge clusters)

### Inline Editing
- Cluster label (double-click to edit)
- Thread label (double-click to edit)

### Filters & Toggles
- Source type checkboxes (cluster explorer, search)
- Cluster filter dropdown (search)
- View toggle: Grid/List (cluster explorer)
- Sort options (cluster explorer, thread explorer)
- Confirmed/Unconfirmed filter (thread explorer)

### Keyboard Shortcuts
- `⌘K` — Focus search
- `⌘R` — Re-cluster
- `Escape` — Close modal/panel
- `⌘,` — Open settings

---

## Navigation

Sidebar with icon+label items:
1. Dashboard (home icon)
2. Clusters (grid icon)
3. Threads (link icon)
4. Search (search icon)
5. Settings (gear icon)

Active item highlighted with `--accent` left border.

---

## Loading & Error States

- **Loading:** Skeleton placeholders for cards, shimmer animation
- **Empty state:** Illustration + "No clusters yet. Start by watching a vault." with action button
- **Error toast:** Bottom-right, auto-dismiss after 5s, red accent for errors, yellow for warnings
- **Progress indicator:** During vault indexing, show progress bar in dashboard with file count

---

## Responsive Behavior

Not required for MVP — macOS desktop only. Minimum 900×600 window.
