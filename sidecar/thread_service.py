"""ThreadService gRPC implementation.

Detects thematic threads between clusters by computing pairwise cosine
similarity of cluster centroids.
"""

from __future__ import annotations

import logging
import time
from itertools import combinations

import grpc
import numpy as np
from sklearn.metrics.pairwise import cosine_similarity

import sidecar_pb2
import sidecar_pb2_grpc

logger = logging.getLogger(__name__)

DEFAULT_SIMILARITY_THRESHOLD = 0.65
DEFAULT_MAX_THREADS = 50


class ThreadServiceServicer(sidecar_pb2_grpc.ThreadServiceServicer):
    """gRPC servicer for thread detection between clusters."""

    def DetectThreads(
        self,
        request: sidecar_pb2.ThreadRequest,
        context: grpc.ServicerContext,
    ) -> sidecar_pb2.ThreadResponse:
        """Detect threads between clusters based on centroid similarity.

        Computes pairwise cosine similarity between all cluster centroids and
        returns pairs above the similarity threshold, sorted by similarity
        descending, limited to max_threads results.

        Args:
            request: ThreadRequest with centroids, threshold, and max_threads.
            context: gRPC servicer context.

        Returns:
            ThreadResponse with detected thread connections.
        """
        centroids = list(request.centroids)

        if len(centroids) < 2:
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                "at least 2 centroids are required for thread detection",
            )

        # Extract parameters with defaults.
        similarity_threshold = (
            request.similarity_threshold
            if request.similarity_threshold > 0.0
            else DEFAULT_SIMILARITY_THRESHOLD
        )
        max_threads = (
            request.max_threads if request.max_threads > 0 else DEFAULT_MAX_THREADS
        )

        # Build centroid matrix.
        centroid_vectors = np.array(
            [list(c.centroid) for c in centroids], dtype=np.float32
        )

        logger.info(
            "Detecting threads among %d centroids (threshold=%.2f, max=%d)",
            len(centroids),
            similarity_threshold,
            max_threads,
        )

        start = time.perf_counter()

        # Compute pairwise cosine similarity.
        sim_matrix = cosine_similarity(centroid_vectors)

        # Collect all pairs above threshold.
        threads: list[sidecar_pb2.Thread] = []
        for i, j in combinations(range(len(centroids)), 2):
            sim = float(sim_matrix[i, j])
            if sim >= similarity_threshold:
                source = centroids[i]
                target = centroids[j]
                suggested_label = _suggest_label(source.label, target.label)
                threads.append(
                    sidecar_pb2.Thread(
                        source_cluster_id=source.cluster_id,
                        target_cluster_id=target.cluster_id,
                        similarity=sim,
                        suggested_label=suggested_label,
                    )
                )

        # Sort by similarity descending and limit.
        threads.sort(key=lambda t: t.similarity, reverse=True)
        threads = threads[:max_threads]

        elapsed = time.perf_counter() - start
        logger.info(
            "Thread detection complete in %.3fs: %d threads found",
            elapsed,
            len(threads),
        )

        return sidecar_pb2.ThreadResponse(threads=threads)


def _suggest_label(source_label: str, target_label: str) -> str:
    """Generate a suggested label for a thread connection.

    If both labels are identical, returns that label. Otherwise concatenates
    them with a bidirectional arrow.

    Args:
        source_label: Label of the source cluster.
        target_label: Label of the target cluster.

    Returns:
        A human-readable suggested label string.
    """
    if source_label and target_label and source_label == target_label:
        return source_label
    parts = [p for p in (source_label, target_label) if p]
    if not parts:
        return "unlabeled"
    if len(parts) == 1:
        return parts[0]
    return f"{parts[0]} <-> {parts[1]}"
