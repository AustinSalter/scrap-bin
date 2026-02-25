"""Shared embedding model wrapper for use across phases.

Wraps sentence-transformers for consistent embedding generation
in the sidecar, experiment scripts, and training data preparation.
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING

import numpy as np
import torch
from sentence_transformers import SentenceTransformer

if TYPE_CHECKING:
    from numpy.typing import NDArray


DEFAULT_MODEL = "nomic-ai/nomic-embed-text-v1.5"
BATCH_SIZE = 64


def detect_device() -> str:
    """Detect the best available compute device."""
    if torch.backends.mps.is_available():
        return "mps"
    if torch.cuda.is_available():
        return "cuda"
    return "cpu"


class EmbeddingModel:
    """Wrapper around sentence-transformers for consistent embedding generation.

    Usage:
        model = EmbeddingModel()
        vec = model.embed("some text")
        vecs = model.embed_batch(["text1", "text2"])
    """

    def __init__(
        self,
        model_name: str = DEFAULT_MODEL,
        device: str | None = None,
    ) -> None:
        self.model_name = model_name
        self.device = device or detect_device()

        start = time.perf_counter()
        self.model = SentenceTransformer(model_name, device=self.device)
        self.load_time = time.perf_counter() - start
        self.dimension = self.model.get_sentence_embedding_dimension()

    def embed(self, text: str, normalize: bool = True) -> NDArray[np.float32]:
        """Embed a single text string."""
        embedding = self.model.encode(
            text,
            convert_to_numpy=True,
            normalize_embeddings=normalize,
            show_progress_bar=False,
        )
        return np.asarray(embedding, dtype=np.float32)

    def embed_batch(
        self,
        texts: list[str],
        normalize: bool = True,
        batch_size: int = BATCH_SIZE,
    ) -> NDArray[np.float32]:
        """Embed a batch of texts. Returns shape (n, dimension)."""
        embeddings = self.model.encode(
            texts,
            convert_to_numpy=True,
            normalize_embeddings=normalize,
            batch_size=batch_size,
            show_progress_bar=False,
        )
        return np.asarray(embeddings, dtype=np.float32)

    @property
    def info(self) -> dict[str, str | int | float]:
        """Return model metadata."""
        return {
            "model_name": self.model_name,
            "device": self.device,
            "dimension": self.dimension,
            "load_time_seconds": round(self.load_time, 2),
        }
