"""EmbeddingService gRPC implementation.

Uses sentence-transformers with nomic-embed-text-v1.5 for text embedding.
"""

from __future__ import annotations

import logging
import threading
import time
from typing import TYPE_CHECKING

import grpc
import numpy as np
import torch
from sentence_transformers import SentenceTransformer

import sidecar_pb2
import sidecar_pb2_grpc

if TYPE_CHECKING:
    from numpy.typing import NDArray

logger = logging.getLogger(__name__)

BATCH_SIZE = 64
MODEL_NAME = "nomic-ai/nomic-embed-text-v1.5"


def _detect_device() -> str:
    """Detect the best available device: MPS > CUDA > CPU."""
    if torch.backends.mps.is_available():
        return "mps"
    if torch.cuda.is_available():
        return "cuda"
    return "cpu"


class EmbeddingServiceServicer(sidecar_pb2_grpc.EmbeddingServiceServicer):
    """gRPC servicer for text embedding operations."""

    def __init__(self, model: SentenceTransformer, device: str) -> None:
        self._model = model
        self._model_lock = threading.Lock()
        self._device = device
        self._model_name = MODEL_NAME
        self._dimension: int = model.get_sentence_embedding_dimension()  # type: ignore[assignment]
        self._ready = True
        logger.info(
            "EmbeddingService initialized: model=%s, dimension=%d, device=%s",
            self._model_name,
            self._dimension,
            self._device,
        )

    def Embed(
        self,
        request: sidecar_pb2.EmbedRequest,
        context: grpc.ServicerContext,
    ) -> sidecar_pb2.EmbedResponse:
        """Embed a single text into a vector."""
        if not request.text:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, "text must not be empty")
            return sidecar_pb2.EmbedResponse()

        start = time.perf_counter()
        with self._model_lock:
            embedding: NDArray[np.float32] = self._model.encode(
                request.text,
                convert_to_numpy=True,
                normalize_embeddings=True,
            )
        elapsed = time.perf_counter() - start
        logger.debug("Embed: %.3fs for 1 text", elapsed)

        return sidecar_pb2.EmbedResponse(
            embedding=embedding.tolist(),
            dimension=self._dimension,
        )

    def EmbedBatch(
        self,
        request: sidecar_pb2.EmbedBatchRequest,
        context: grpc.ServicerContext,
    ) -> sidecar_pb2.EmbedBatchResponse:
        """Embed a batch of texts into vectors, processing in chunks of 64."""
        texts: list[str] = list(request.texts)
        if not texts:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, "texts must not be empty")
            return sidecar_pb2.EmbedBatchResponse()

        start = time.perf_counter()
        all_embeddings: list[sidecar_pb2.Embedding] = []

        for i in range(0, len(texts), BATCH_SIZE):
            batch = texts[i : i + BATCH_SIZE]
            with self._model_lock:
                vectors: NDArray[np.float32] = self._model.encode(
                    batch,
                    convert_to_numpy=True,
                    normalize_embeddings=True,
                    batch_size=BATCH_SIZE,
                )
            for vec in vectors:
                all_embeddings.append(sidecar_pb2.Embedding(values=vec.tolist()))

        elapsed = time.perf_counter() - start
        logger.info("EmbedBatch: %.3fs for %d texts", elapsed, len(texts))

        return sidecar_pb2.EmbedBatchResponse(
            embeddings=all_embeddings,
            dimension=self._dimension,
        )

    def Health(
        self,
        request: sidecar_pb2.HealthRequest,
        context: grpc.ServicerContext,
    ) -> sidecar_pb2.HealthResponse:
        """Return model info and readiness status."""
        return sidecar_pb2.HealthResponse(
            model_name=self._model_name,
            dimension=self._dimension,
            ready=self._ready,
        )


def load_model() -> tuple[SentenceTransformer, str]:
    """Load the sentence-transformers model on the best available device.

    Returns:
        A tuple of (model, device_string).
    """
    device = _detect_device()
    logger.info("Loading model %s on device=%s ...", MODEL_NAME, device)
    start = time.perf_counter()
    model = SentenceTransformer(MODEL_NAME, device=device, trust_remote_code=True)
    elapsed = time.perf_counter() - start
    logger.info("Model loaded in %.2fs", elapsed)
    return model, device
