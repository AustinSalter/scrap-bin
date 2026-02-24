"""ClusteringService gRPC implementation.

Uses HDBSCAN for density-based clustering of embedding vectors.
"""

from __future__ import annotations

import logging
import time
from collections import defaultdict

import grpc
import hdbscan
import numpy as np

import sidecar_pb2
import sidecar_pb2_grpc

logger = logging.getLogger(__name__)

DEFAULT_MIN_CLUSTER_SIZE = 5
DEFAULT_MIN_SAMPLES = 3


class ClusteringServiceServicer(sidecar_pb2_grpc.ClusteringServiceServicer):
    """gRPC servicer for HDBSCAN clustering operations."""

    def Cluster(
        self,
        request: sidecar_pb2.ClusterRequest,
        context: grpc.ServicerContext,
    ) -> sidecar_pb2.ClusterResponse:
        """Cluster embeddings using HDBSCAN.

        Args:
            request: ClusterRequest with embeddings, ids, and HDBSCAN parameters.
            context: gRPC servicer context.

        Returns:
            ClusterResponse with labels, cluster info, and noise count.
        """
        embeddings = request.embeddings
        ids = list(request.ids)

        if not embeddings:
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT, "embeddings must not be empty"
            )

        if len(embeddings) != len(ids):
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                f"embeddings length ({len(embeddings)}) must match ids length ({len(ids)})",
            )

        # Extract parameters with defaults.
        min_cluster_size = (
            request.min_cluster_size
            if request.min_cluster_size > 0
            else DEFAULT_MIN_CLUSTER_SIZE
        )
        min_samples = (
            request.min_samples if request.min_samples > 0 else DEFAULT_MIN_SAMPLES
        )

        # Convert proto embeddings to numpy array.
        vectors = np.array(
            [list(emb.values) for emb in embeddings], dtype=np.float32
        )

        logger.info(
            "Clustering %d embeddings (min_cluster_size=%d, min_samples=%d)",
            len(vectors),
            min_cluster_size,
            min_samples,
        )

        start = time.perf_counter()
        clusterer = hdbscan.HDBSCAN(
            min_cluster_size=min_cluster_size,
            min_samples=min_samples,
            metric="euclidean",
        )
        labels: np.ndarray = clusterer.fit_predict(vectors)
        elapsed = time.perf_counter() - start

        # Build cluster info.
        cluster_members: dict[int, list[int]] = defaultdict(list)
        for idx, label in enumerate(labels):
            if label != -1:
                cluster_members[int(label)].append(idx)

        noise_count = int(np.sum(labels == -1))
        n_clusters = len(cluster_members)

        logger.info(
            "Clustering complete in %.3fs: %d clusters, %d noise points",
            elapsed,
            n_clusters,
            noise_count,
        )

        cluster_infos: list[sidecar_pb2.ClusterInfo] = []
        for label, member_indices in sorted(cluster_members.items()):
            member_vectors = vectors[member_indices]
            centroid = member_vectors.mean(axis=0)
            member_ids = [ids[i] for i in member_indices]

            cluster_infos.append(
                sidecar_pb2.ClusterInfo(
                    label=label,
                    size=len(member_indices),
                    centroid=centroid.tolist(),
                    member_ids=member_ids,
                )
            )

        return sidecar_pb2.ClusterResponse(
            labels=[int(lbl) for lbl in labels],
            clusters=cluster_infos,
            noise_count=noise_count,
        )
