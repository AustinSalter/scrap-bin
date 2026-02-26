# Scrapbin

Turn scattered intrigue into context environments.

A Tauri v2 desktop app (macOS) that ingests personal context from multiple sources—notes, bookmarks, highlights, transcripts—embeds and clusters them via HDBSCAN, and detects cross-cluster threads. Ingest cheap, cluster automatically, reason on demand.

## Architecture

```
React Frontend
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

Retrieval never touches Python. Chroma embeds queries internally. The Python sidecar handles batch ingestion, clustering, and thread detection only.

## Data Sources

- **Obsidian vault** — file watcher with incremental indexing (SHA-256 dedup)
- **Twitter bookmarks** — JSON export import
- **Readwise highlights** — API v2 with incremental sync
- **Podcast transcripts** — .txt / .srt / .vtt files

## Prerequisites

- macOS
- Node.js
- Rust / Cargo
- Python 3.12+

## Build & Run

```bash
# Install frontend dependencies
npm install

# Python sidecar setup (separate terminal)
pip install -e .
cd sidecar
python -m grpc_tools.protoc -I../proto --python_out=. --grpc_python_out=. ../proto/sidecar.proto
python server.py --port 50051

# Run dev (starts Vite + Tauri)
npm run tauri dev
```

## Tech Stack

Tauri v2, React 19, Rust (edition 2021), Python 3.12, ChromaDB, HDBSCAN, gRPC (tonic + grpcio), nomic-embed-text

## Project Structure

```
scrapbin/
├── src-tauri/          # Rust backend (Tauri commands, watcher, chunker, pipeline, search)
├── sidecar/            # Python gRPC sidecar (embedding, clustering, threads)
├── proto/              # Protobuf definitions
├── shared/             # Shared Python utilities
├── src/                # React frontend
├── design mockups/     # UI design reference (HTML mockup, specs, frame exports)
```
