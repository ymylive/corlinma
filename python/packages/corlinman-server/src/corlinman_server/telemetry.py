"""OpenTelemetry OTLP exporter wiring for the Python agent server (S7.T1).

Contract (mirrors the Rust gateway's ``corlinman_gateway::telemetry``):

* Init is opt-in — activated only when ``OTEL_EXPORTER_OTLP_ENDPOINT`` is
  set and non-empty. Without it the functions here are no-ops.
* Init failures are warn-and-continue; ``structlog`` keeps working.
* Service name defaults to ``corlinman-server`` (``OTEL_SERVICE_NAME``
  overrides).
* Once :func:`init_telemetry` runs, ``structlog`` adds ``trace_id`` /
  ``span_id`` to every record via :func:`_bind_trace_ids_processor`.
"""

from __future__ import annotations

import contextlib
import os
from typing import Any

import structlog

__all__ = ["init_telemetry", "shutdown_telemetry"]

_PROVIDER: Any = None  # opentelemetry_sdk.trace.TracerProvider when wired


def _bind_trace_ids_processor(
    _logger: Any, _method: str, event_dict: dict[str, Any]
) -> dict[str, Any]:
    """Add ``trace_id`` / ``span_id`` hex strings to every structlog record.

    Called only when an OTel tracer is installed. Missing context yields
    empty strings rather than raising — matches the grpc interceptor,
    which may see requests with no ``traceparent`` header.
    """
    try:
        from opentelemetry import trace

        ctx = trace.get_current_span().get_span_context()
    except Exception:  # pragma: no cover — defensive
        return event_dict

    if ctx and ctx.is_valid:
        event_dict.setdefault("trace_id", format(ctx.trace_id, "032x"))
        event_dict.setdefault("span_id", format(ctx.span_id, "016x"))
    return event_dict


def init_telemetry() -> bool:
    """Configure the global OTel tracer + structlog binding once.

    Returns ``True`` when the tracer was installed, ``False`` otherwise
    (endpoint unset, dependency missing, or exporter init failed). The
    server boot treats both as success — telemetry stays optional.
    """
    global _PROVIDER

    endpoint = os.environ.get("OTEL_EXPORTER_OTLP_ENDPOINT", "").strip()
    if not endpoint:
        return False

    try:
        from opentelemetry import trace
        from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import (
            OTLPSpanExporter,
        )
        from opentelemetry.propagate import set_global_textmap
        from opentelemetry.sdk.resources import Resource
        from opentelemetry.sdk.trace import TracerProvider
        from opentelemetry.sdk.trace.export import BatchSpanProcessor
        from opentelemetry.trace.propagation.tracecontext import (
            TraceContextTextMapPropagator,
        )
    except ImportError as err:
        structlog.get_logger(__name__).warning(
            "otel.init.skip",
            reason="missing dependency",
            error=str(err),
        )
        return False

    service_name = os.environ.get("OTEL_SERVICE_NAME", "corlinman-server")
    try:
        resource = Resource.create(
            {
                "service.name": service_name,
                "service.version": "0.1.0",
            }
        )
        provider = TracerProvider(resource=resource)
        exporter = OTLPSpanExporter(endpoint=endpoint, insecure=endpoint.startswith("http://"))
        provider.add_span_processor(BatchSpanProcessor(exporter))
        trace.set_tracer_provider(provider)
        set_global_textmap(TraceContextTextMapPropagator())
        _PROVIDER = provider
    except Exception as err:  # pragma: no cover — collector-dependent
        structlog.get_logger(__name__).warning(
            "otel.init.failed", endpoint=endpoint, error=str(err)
        )
        return False

    # Rebind structlog so every record carries trace_id / span_id.
    try:
        current = list(structlog.get_config()["processors"])
        if _bind_trace_ids_processor not in current:
            # Insert before the final renderer (last processor).
            current.insert(max(len(current) - 1, 0), _bind_trace_ids_processor)
            structlog.configure(processors=current)
    except Exception:  # pragma: no cover — structlog not configured yet
        pass

    structlog.get_logger(__name__).info("otel.init.ok", endpoint=endpoint)
    return True


def shutdown_telemetry() -> None:
    """Best-effort flush + shutdown of the tracer provider. No-op when
    :func:`init_telemetry` returned ``False``.
    """
    global _PROVIDER
    if _PROVIDER is None:
        return
    with contextlib.suppress(Exception):  # pragma: no cover
        _PROVIDER.shutdown()
    _PROVIDER = None
