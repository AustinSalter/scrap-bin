"""Thin wrapper around chromadb.HttpClient for shared use across phases.

Provides a consistent interface for connecting to the local Chroma instance
from experiment scripts, training data prep, and utility tools.
"""

from __future__ import annotations

from typing import Any

import chromadb
from chromadb.api.models.Collection import Collection


DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 8000

# Standard collection names matching the Rust backend
COLLECTION_VAULT = "vault"
COLLECTION_TWITTER = "twitter"
COLLECTION_READWISE = "readwise"
COLLECTION_PODCASTS = "podcasts"
COLLECTION_CLUSTERS = "clusters"
COLLECTION_THREADS = "threads"

ALL_COLLECTIONS = [
    COLLECTION_VAULT,
    COLLECTION_TWITTER,
    COLLECTION_READWISE,
    COLLECTION_PODCASTS,
    COLLECTION_CLUSTERS,
    COLLECTION_THREADS,
]


class ChromaClient:
    """Wrapper around chromadb.HttpClient with convenience methods."""

    def __init__(
        self,
        host: str = DEFAULT_HOST,
        port: int = DEFAULT_PORT,
    ) -> None:
        self.client = chromadb.HttpClient(host=host, port=port)

    def heartbeat(self) -> int:
        """Check if Chroma is alive. Returns nanosecond heartbeat."""
        return self.client.heartbeat()

    def get_or_create_collection(self, name: str) -> Collection:
        """Get or create a collection by name."""
        return self.client.get_or_create_collection(name=name)

    def get_collection(self, name: str) -> Collection:
        """Get an existing collection by name. Raises if not found."""
        return self.client.get_collection(name=name)

    def list_collections(self) -> list[Collection]:
        """List all collections."""
        return self.client.list_collections()

    def ensure_all_collections(self) -> dict[str, Collection]:
        """Create all standard collections, returning a name->Collection map."""
        result: dict[str, Collection] = {}
        for name in ALL_COLLECTIONS:
            result[name] = self.get_or_create_collection(name)
        return result

    def query(
        self,
        collection_name: str,
        query_texts: list[str],
        n_results: int = 10,
        where: dict[str, Any] | None = None,
        include: list[str] | None = None,
    ) -> dict[str, Any]:
        """Query a collection by text (Chroma handles embedding internally)."""
        collection = self.get_collection(collection_name)
        kwargs: dict[str, Any] = {
            "query_texts": query_texts,
            "n_results": n_results,
        }
        if where is not None:
            kwargs["where"] = where
        if include is not None:
            kwargs["include"] = include
        return collection.query(**kwargs)

    def add(
        self,
        collection_name: str,
        ids: list[str],
        documents: list[str] | None = None,
        embeddings: list[list[float]] | None = None,
        metadatas: list[dict[str, Any]] | None = None,
    ) -> None:
        """Add documents to a collection."""
        collection = self.get_or_create_collection(collection_name)
        kwargs: dict[str, Any] = {"ids": ids}
        if documents is not None:
            kwargs["documents"] = documents
        if embeddings is not None:
            kwargs["embeddings"] = embeddings
        if metadatas is not None:
            kwargs["metadatas"] = metadatas
        collection.add(**kwargs)

    def count(self, collection_name: str) -> int:
        """Get document count for a collection."""
        collection = self.get_collection(collection_name)
        return collection.count()

    def delete(self, collection_name: str, ids: list[str]) -> None:
        """Delete documents by ID from a collection."""
        collection = self.get_collection(collection_name)
        collection.delete(ids=ids)
