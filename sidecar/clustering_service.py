"""ClusteringService gRPC implementation.

Uses HDBSCAN for density-based clustering of embedding vectors.
"""

from __future__ import annotations

import logging
import math
import time
from collections import defaultdict

import grpc
import hdbscan
import numpy as np
import umap

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
            return sidecar_pb2.ClusterResponse()

        if len(embeddings) != len(ids):
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                f"embeddings length ({len(embeddings)}) must match ids length ({len(ids)})",
            )
            return sidecar_pb2.ClusterResponse()

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
        # Euclidean distance is equivalent to cosine distance for normalized
        # embeddings: d_euclidean = sqrt(2 - 2*cos_sim). nomic-embed-text
        # produces L2-normalized vectors, so euclidean is correct here.
        try:
            clusterer = hdbscan.HDBSCAN(
                min_cluster_size=min_cluster_size,
                min_samples=min_samples,
                metric="euclidean",
            )
            labels: np.ndarray = clusterer.fit_predict(vectors)
        except Exception as exc:
            logger.exception("HDBSCAN clustering failed")
            context.abort(grpc.StatusCode.INTERNAL, f"HDBSCAN failed: {exc}")
            return sidecar_pb2.ClusterResponse()
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

    def ProjectPositions(
        self,
        request: sidecar_pb2.ProjectRequest,
        context: grpc.ServicerContext,
    ) -> sidecar_pb2.ProjectResponse:
        """Project high-dimensional centroids to 2D positions via UMAP.

        Args:
            request: ProjectRequest with centroid embeddings and cluster IDs.
            context: gRPC servicer context.

        Returns:
            ProjectResponse with normalized 2D positions.
        """
        centroids = request.centroids
        cluster_ids = list(request.cluster_ids)
        n = len(centroids)

        if n == 0:
            return sidecar_pb2.ProjectResponse(positions=[])

        if n != len(cluster_ids):
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                f"centroids length ({n}) must match cluster_ids length ({len(cluster_ids)})",
            )
            return sidecar_pb2.ProjectResponse()

        # Single cluster: center it.
        if n == 1:
            return sidecar_pb2.ProjectResponse(
                positions=[
                    sidecar_pb2.Position2D(
                        cluster_id=cluster_ids[0], x=0.5, y=0.5
                    )
                ]
            )

        # Two clusters: place on a horizontal line.
        if n == 2:
            return sidecar_pb2.ProjectResponse(
                positions=[
                    sidecar_pb2.Position2D(
                        cluster_id=cluster_ids[0], x=0.3, y=0.5
                    ),
                    sidecar_pb2.Position2D(
                        cluster_id=cluster_ids[1], x=0.7, y=0.5
                    ),
                ]
            )

        # For < 3 effective unique points but n >= 3, use circle layout.
        vectors = np.array(
            [list(c.values) for c in centroids], dtype=np.float32
        )

        # Check if we have enough unique vectors for UMAP.
        unique_count = len(np.unique(vectors, axis=0))
        if unique_count < 3:
            # Evenly spaced circle layout.
            positions = []
            for i, cid in enumerate(cluster_ids):
                angle = 2 * math.pi * i / n
                x = 0.5 + 0.35 * math.cos(angle)
                y = 0.5 + 0.35 * math.sin(angle)
                positions.append(
                    sidecar_pb2.Position2D(cluster_id=cid, x=x, y=y)
                )
            return sidecar_pb2.ProjectResponse(positions=positions)

        logger.info("Projecting %d centroids to 2D via UMAP", n)
        start = time.perf_counter()

        n_neighbors = min(15, n - 1)
        try:
            reducer = umap.UMAP(
                n_components=2,
                metric="cosine",
                n_neighbors=n_neighbors,
                random_state=42,
            )
            projected = reducer.fit_transform(vectors)
        except Exception as exc:
            logger.exception("UMAP projection failed")
            context.abort(grpc.StatusCode.INTERNAL, f"UMAP failed: {exc}")
            return sidecar_pb2.ProjectResponse()
        elapsed = time.perf_counter() - start
        logger.info("UMAP projection complete in %.3fs", elapsed)

        # Guard against NaN values in UMAP output.
        if np.any(np.isnan(projected)):
            logger.warning("UMAP produced NaN values, falling back to circle layout")
            positions = []
            for i, cid in enumerate(cluster_ids):
                angle = 2 * math.pi * i / n
                x = 0.5 + 0.35 * math.cos(angle)
                y = 0.5 + 0.35 * math.sin(angle)
                positions.append(
                    sidecar_pb2.Position2D(cluster_id=cid, x=x, y=y)
                )
            return sidecar_pb2.ProjectResponse(positions=positions)

        # Normalize to [0.05, 0.95] range (5% padding).
        mins = projected.min(axis=0)
        maxs = projected.max(axis=0)
        ranges = maxs - mins
        # Avoid division by zero.
        ranges[ranges == 0] = 1.0
        normalized = (projected - mins) / ranges
        # Scale to [0.05, 0.95].
        normalized = normalized * 0.9 + 0.05

        positions = []
        for i, cid in enumerate(cluster_ids):
            positions.append(
                sidecar_pb2.Position2D(
                    cluster_id=cid,
                    x=float(normalized[i, 0]),
                    y=float(normalized[i, 1]),
                )
            )

        return sidecar_pb2.ProjectResponse(positions=positions)
