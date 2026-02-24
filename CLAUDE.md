# CLAUDE.md

## Project Overview

**Scrapbin** — turn scattered intrigue into context environments.

Tauri v2 desktop app (macOS) that ingests personal context from multiple sources, embeds and clusters them via HDBSCAN, and detects cross-cluster threads. Scrap bin model: ingest cheap, cluster automatically, reason on demand.

## Architecture

```
React Frontend (design pending)
        │ Tauri IPC (invoke)
        ▼
┌─────────────────────────────┐
│  Rust Backend (Tauri v2)    │
│  ├── watcher       (notify) │──▶ file events
│  ├── markdown      (parser) │──▶ parsed notes
│  ├── chunker       (split)  │──▶ chunks
│  ├── chroma/       (REST)   │──▶ Chroma server (port 8000)
│  ├── sources/      (ingest) │     ▲ retrieval: query_texts (no Python)
│  ├── pipeline      (glue)   │     │
│  └── grpc_client   (tonic)  │─────┘
│       │                     │
│       │ gRPC (port 50051)   │
│       ▼                     │
│  Python Sidecar             │
│  ├── embed (nomic-embed)    │  batch ingestion only
│  ├── cluster (HDBSCAN)      │  on-demand
│  └── threads (similarity)   │  on-demand
└─────────────────────────────┘
```

**Key insight:** Retrieval never touches Python. Chroma embeds queries internally. Python sidecar is only in the ingestion/clustering path (batch, background).

## Data Sources

- **Obsidian vault** — file watcher with incremental indexing (SHA-256 dedup)
- **Twitter bookmarks** — JSON export import
- **Readwise highlights** — API v2 with incremental sync
- **Podcast transcripts** — .txt / .srt / .vtt files

## Build & Run

```bash
# Install frontend deps
npm install

# Run dev (starts both Vite + Tauri)
npm run tauri dev

# Python sidecar setup (separate terminal, for dev)
cd sidecar
pip install -r requirements.txt
python -m grpc_tools.protoc -I../proto --python_out=. --grpc_python_out=. ../proto/sidecar.proto
python server.py --port 50051
```

## Repository Structure

```
scrapbin/
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── main.rs         # Tauri commands, plugin registration
│   │   ├── config.rs       # App config (~/.dialectic-rl/)
│   │   ├── watcher.rs      # File system watcher (300ms debounce)
│   │   ├── markdown.rs     # Markdown parser (frontmatter, headings, links, tags)
│   │   ├── chunker.rs      # Hierarchical chunking (512 token max, 50 overlap)
│   │   ├── fragment.rs     # Unified Fragment type across all sources
│   │   ├── state.rs        # Incremental indexing state (SHA-256 per file)
│   │   ├── pipeline.rs     # Watcher → chunk → embed → Chroma pipeline
│   │   ├── search.rs       # Cross-collection search (Chroma query_texts)
│   │   ├── grpc_client.rs  # Tonic gRPC client for Python sidecar
│   │   ├── sidecar.rs      # Unified sidecar manager (Chroma + Python)
│   │   ├── clustering.rs   # Cluster orchestration (9 Tauri commands)
│   │   ├── threads.rs      # Cross-cluster thread detection
│   │   ├── chroma/         # Chroma client, sidecar, collections
│   │   └── sources/        # Twitter, Readwise, Podcast ingesters
│   ├── Cargo.toml
│   ├── build.rs            # tonic_build + tauri_build
│   └── tauri.conf.json
├── sidecar/                # Python gRPC sidecar
│   ├── server.py           # gRPC server entry
│   ├── embedding_service.py
│   ├── clustering_service.py
│   ├── thread_service.py
│   └── requirements.txt
├── proto/                  # Protobuf definitions
│   └── sidecar.proto
├── shared/                 # Shared Python utilities
│   ├── embeddings.py
│   ├── chroma_client.py
│   └── types.py
├── src/                    # React frontend (design pending)
│   ├── App.tsx
│   └── main.tsx
├── docs/
│   └── UI_REQUIREMENTS.md  # Figma design spec
├── package.json
├── index.html
├── vite.config.ts
└── tsconfig.json
```

## Coding Conventions

- **Rust:** Edition 2021, Tauri v2, thiserror for errors (with manual Serialize impl), parking_lot for concurrency
- **Python:** 3.12+, type hints mandatory, ruff for linting
- **TypeScript:** strict mode, React 19 functional components
- **Config:** YAML for tunable parameters, JSON for app state

## Key Patterns

- **Error types:** `thiserror::Error` + manual `Serialize` for Tauri command compatibility
- **Global singletons:** `parking_lot::RwLock<Option<T>>` or `Mutex<Option<T>>`
- **State management:** `load_state()` → mutate → `save_state()` (index_state.json)
- **Fragment:** Unified type across all sources, `fragment_to_chroma_metadata()` for Chroma storage
- **Sidecar lifecycle:** Spawn → health poll with backoff → SIGTERM → 5s → SIGKILL → max 3 restarts

## Chroma Collections

| Collection | Content |
|---|---|
| `vault` | Obsidian vault chunks |
| `twitter` | Twitter bookmark chunks |
| `readwise` | Readwise highlight chunks |
| `podcasts` | Podcast transcript chunks |
| `clusters` | Cluster metadata (label, members, centroid) |
| `threads` | Thread metadata (connections, labels) |
