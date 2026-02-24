"""gRPC sidecar server for the ingestion Tauri app.

Serves EmbeddingService, ClusteringService, and ThreadService on a
configurable port (default 50051). Loads the sentence-transformers model
on startup and detects the best available compute device.

Generating protobuf stubs
-------------------------
Run the following from the ``sidecar/`` directory before first use::

    python -m grpc_tools.protoc \
        -I../proto \
        --python_out=. \
        --grpc_python_out=. \
        ../proto/sidecar.proto

This produces ``sidecar_pb2.py`` and ``sidecar_pb2_grpc.py`` which are
imported by the service implementations.
"""

from __future__ import annotations

import argparse
import logging
import signal
import sys
import time
from concurrent import futures
from types import FrameType

import grpc

import sidecar_pb2_grpc
from clustering_service import ClusteringServiceServicer
from embedding_service import EmbeddingServiceServicer, load_model
from thread_service import ThreadServiceServicer

logger = logging.getLogger(__name__)

_SHUTDOWN_GRACE_SECONDS = 5


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="gRPC sidecar server for embedding, clustering, and thread detection.",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=50051,
        help="Port to listen on (default: 50051)",
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=4,
        help="Max gRPC thread-pool workers (default: 4)",
    )
    parser.add_argument(
        "--log-level",
        type=str,
        default="INFO",
        choices=["DEBUG", "INFO", "WARNING", "ERROR"],
        help="Logging level (default: INFO)",
    )
    return parser.parse_args()


def serve(port: int, workers: int) -> None:
    """Start the gRPC server with all services registered.

    Args:
        port: TCP port to listen on.
        workers: Maximum number of thread-pool workers.
    """
    # Load the embedding model (shared across requests).
    model, device = load_model()

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=workers))

    # Register services.
    sidecar_pb2_grpc.add_EmbeddingServiceServicer_to_server(
        EmbeddingServiceServicer(model=model, device=device),
        server,
    )
    sidecar_pb2_grpc.add_ClusteringServiceServicer_to_server(
        ClusteringServiceServicer(),
        server,
    )
    sidecar_pb2_grpc.add_ThreadServiceServicer_to_server(
        ThreadServiceServicer(),
        server,
    )

    listen_addr = f"[::]:{port}"
    server.add_insecure_port(listen_addr)
    server.start()
    logger.info("Server started on %s (workers=%d, device=%s)", listen_addr, workers, device)

    # Graceful shutdown on SIGTERM / SIGINT.
    shutdown_event = False

    def _handle_signal(signum: int, frame: FrameType | None) -> None:
        nonlocal shutdown_event
        if shutdown_event:
            return
        shutdown_event = True
        sig_name = signal.Signals(signum).name
        logger.info("Received %s, shutting down gracefully ...", sig_name)
        stopped = server.stop(grace=_SHUTDOWN_GRACE_SECONDS)
        stopped.wait()
        logger.info("Server stopped.")

    signal.signal(signal.SIGTERM, _handle_signal)
    signal.signal(signal.SIGINT, _handle_signal)

    # Block until shutdown.
    server.wait_for_termination()


def main() -> None:
    args = _parse_args()

    logging.basicConfig(
        level=getattr(logging, args.log_level),
        format="%(asctime)s %(levelname)-8s %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
        stream=sys.stderr,
    )

    logger.info("Starting sidecar server on port %d ...", args.port)
    start = time.perf_counter()
    serve(port=args.port, workers=args.workers)
    elapsed = time.perf_counter() - start
    logger.info("Server ran for %.1fs total.", elapsed)


if __name__ == "__main__":
    main()
