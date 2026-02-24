"""Shared type definitions mirroring key Rust structs.

These dataclasses provide a Python-side contract that matches the Rust backend's
types, used by experiment scripts and training data preparation.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class SourceType(str, Enum):
    """Matches Rust SourceType enum."""

    VAULT = "vault"
    TWITTER = "twitter"
    READWISE = "readwise"
    PODCAST = "podcast"

    @property
    def collection_name(self) -> str:
        """Return the Chroma collection name for this source type."""
        if self == SourceType.PODCAST:
            return "podcasts"
        return self.value


@dataclass
class Fragment:
    """Unified fragment type matching Rust Fragment struct."""

    id: str
    content: str
    source_type: SourceType
    source_path: str
    chunk_index: int
    heading_path: list[str] = field(default_factory=list)
    tags: list[str] = field(default_factory=list)
    token_count: int = 0
    content_hash: str = ""
    modified_at: str = ""
    cluster_id: int | None = None
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_chroma_metadata(self) -> dict[str, Any]:
        """Convert to flat metadata dict for Chroma storage."""
        meta: dict[str, Any] = {
            "source_type": self.source_type.value,
            "source_path": self.source_path,
            "chunk_index": self.chunk_index,
            "heading_path": "|".join(self.heading_path),
            "tags": ",".join(self.tags),
            "token_count": self.token_count,
            "content_hash": self.content_hash,
            "modified_at": self.modified_at,
        }
        if self.cluster_id is not None:
            meta["cluster_id"] = self.cluster_id
        return meta


@dataclass
class ClusterView:
    """Cluster representation matching Rust ClusterView struct."""

    label: int
    display_label: str
    size: int
    pinned: bool = False
    fragment_ids: list[str] = field(default_factory=list)


@dataclass
class ThreadView:
    """Thread representation matching Rust ThreadView struct."""

    id: str
    source_cluster: str
    target_cluster: str
    similarity: float
    label: str | None = None
    confirmed: bool = False
    dismissed: bool = False


@dataclass
class SearchResult:
    """Search result matching Rust SearchResult struct."""

    id: str
    content: str
    source_type: str
    source_path: str
    distance: float
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class PipelineStats:
    """Pipeline statistics matching Rust PipelineStats struct."""

    total_files_indexed: int
    total_chunks: int
    collections: list[CollectionStat] = field(default_factory=list)
    last_index_time: str | None = None


@dataclass
class CollectionStat:
    """Per-collection statistics."""

    name: str
    count: int
