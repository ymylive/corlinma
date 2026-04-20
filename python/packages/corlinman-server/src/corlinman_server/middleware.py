"""gRPC server middleware — propagate W3C traceparent into structlog contextvars.

Responsibility: read ``traceparent`` from gRPC invocation metadata, bind it
(plus ``request_id``, ``subsystem``) into ``structlog.contextvars`` so every
log emitted while servicing the RPC carries the trace id. Mirrors plan §8 A4.

S7.T1 wires the actual extraction: when OTel is installed the interceptor
uses ``TraceContextTextMapPropagator`` to reconstruct a ``SpanContext`` from
the incoming metadata and activates it as the current context so every
span created inside the RPC chains back to the Rust-side trace.
"""

from __future__ import annotations

from typing import Any

import grpc.aio
import structlog

logger = structlog.get_logger(__name__)


def install_tracecontext_interceptor() -> Any:
    """Return a gRPC interceptor that wires W3C traceparent → OTel context.

    When ``opentelemetry`` isn't available the interceptor is a no-op
    passthrough. Safe to install unconditionally.
    """
    try:
        from opentelemetry import context as otel_context
        from opentelemetry import trace
        from opentelemetry.propagate import extract
    except ImportError:  # pragma: no cover — opentelemetry always installed
        has_otel = False
    else:
        has_otel = True

    class _TraceContextInterceptor(grpc.aio.ServerInterceptor):  # type: ignore[misc]
        async def intercept_service(self, continuation, handler_call_details):  # type: ignore[override]
            if not has_otel:
                return await continuation(handler_call_details)

            # Incoming metadata is a sequence of (key, value) pairs.
            md = handler_call_details.invocation_metadata or ()
            carrier = {str(k).lower(): str(v) for k, v in md}
            ctx = extract(carrier)
            token = otel_context.attach(ctx)
            try:
                # Bind trace_id / span_id into structlog contextvars so the
                # servicer's logs inherit them even before any new span
                # starts.
                span_ctx = trace.get_current_span(ctx).get_span_context()
                if span_ctx.is_valid:
                    structlog.contextvars.bind_contextvars(
                        trace_id=format(span_ctx.trace_id, "032x"),
                        span_id=format(span_ctx.span_id, "016x"),
                    )
                return await continuation(handler_call_details)
            finally:
                otel_context.detach(token)
                structlog.contextvars.unbind_contextvars("trace_id", "span_id")

    return _TraceContextInterceptor()
