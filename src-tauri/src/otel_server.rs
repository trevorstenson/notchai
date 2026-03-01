use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use opentelemetry_proto::tonic::trace::v1::span::SpanKind;
use opentelemetry_proto::tonic::trace::v1::status::StatusCode as OtelStatusCode;
use opentelemetry_proto::tonic::trace::v1::Span;
use prost::Message;
use std::net::SocketAddr;

use crate::event_bus::EventBus;
use crate::models::{AgentStatus, AgentType, EventSource, NormalizedEvent};

/// Start the OTEL HTTP/protobuf ingestion server on localhost:4318.
///
/// Accepts OTLP/HTTP trace exports at POST /v1/traces.
/// Maps incoming spans to NormalizedEvent and publishes them to the EventBus.
/// Gracefully handles port-already-in-use by logging a warning and returning.
pub async fn start(event_bus: EventBus) {
    let app = Router::new()
        .route("/v1/traces", post(handle_traces))
        .with_state(event_bus);

    let addr: SocketAddr = ([127, 0, 0, 1], 4318).into();

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "[otel] failed to bind to {}: {} — OTEL server disabled",
                addr, e
            );
            return;
        }
    };

    eprintln!("[otel] OTLP/HTTP server listening on {}", addr);

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("[otel] server error: {}", e);
    }
}

async fn handle_traces(
    State(event_bus): State<EventBus>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("application/x-protobuf") {
        return (
            StatusCode::BAD_REQUEST,
            "Expected Content-Type: application/x-protobuf",
        )
            .into_response();
    }

    match ExportTraceServiceRequest::decode(body) {
        Ok(request) => {
            process_spans(&request, &event_bus);

            let response_bytes = ExportTraceServiceResponse::default().encode_to_vec();
            (
                StatusCode::OK,
                [("content-type", "application/x-protobuf")],
                response_bytes,
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("[otel] protobuf decode error: {}", e);
            (
                StatusCode::BAD_REQUEST,
                format!("Decode error: {}", e),
            )
                .into_response()
        }
    }
}

/// Process all spans in the request: log them and map to NormalizedEvent.
fn process_spans(request: &ExportTraceServiceRequest, event_bus: &EventBus) {
    for resource_spans in &request.resource_spans {
        let service_name = extract_resource_attr(resource_spans, "service.name")
            .unwrap_or_else(|| "unknown".to_string());

        let agent_type = agent_type_from_service_name(&service_name);

        for scope_spans in &resource_spans.scope_spans {
            for span in &scope_spans.spans {
                let trace_id: String =
                    span.trace_id.iter().map(|b| format!("{:02x}", b)).collect();
                eprintln!(
                    "[otel] span: name={}, service={}, trace_id={}",
                    span.name, service_name, trace_id
                );

                if let Some(event) = map_span_to_event(span, agent_type, &service_name) {
                    event_bus.publish(event);
                }
            }
        }
    }
}

/// Extract a string attribute from resource_spans.resource.attributes by key.
fn extract_resource_attr(
    resource_spans: &opentelemetry_proto::tonic::trace::v1::ResourceSpans,
    key: &str,
) -> Option<String> {
    resource_spans
        .resource
        .as_ref()
        .and_then(|r| {
            r.attributes
                .iter()
                .find(|kv| kv.key == key)
                .and_then(|kv| kv.value.as_ref())
                .and_then(|v| match &v.value {
                    Some(Value::StringValue(s)) => Some(s.clone()),
                    _ => None,
                })
        })
}

/// Extract a string attribute from a span's attributes by key.
fn span_attr(span: &Span, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| kv.value.as_ref())
        .and_then(|v| match &v.value {
            Some(Value::StringValue(s)) => Some(s.clone()),
            _ => None,
        })
}

/// Extract an integer attribute from a span's attributes by key.
fn span_attr_int(span: &Span, key: &str) -> Option<u64> {
    span.attributes
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| kv.value.as_ref())
        .and_then(|v| match &v.value {
            Some(Value::IntValue(n)) => Some(*n as u64),
            _ => None,
        })
}

/// Derive AgentType from the OTEL service.name attribute.
fn agent_type_from_service_name(service_name: &str) -> AgentType {
    let lower = service_name.to_lowercase();
    if lower.contains("claude") {
        AgentType::Claude
    } else if lower.contains("codex") {
        AgentType::Codex
    } else if lower.contains("cursor") {
        AgentType::Cursor
    } else if lower.contains("gemini") {
        AgentType::Gemini
    } else {
        // Default to Claude for unknown services
        AgentType::Claude
    }
}

/// Convert an OTEL span timestamp (nanoseconds since epoch) to ISO 8601 string.
fn nanos_to_iso(nanos: u64) -> String {
    let secs = nanos / 1_000_000_000;
    let subsec_nanos = (nanos % 1_000_000_000) as u32;
    let dt = chrono::DateTime::from_timestamp(secs as i64, subsec_nanos)
        .unwrap_or_else(|| chrono::Utc::now());
    dt.to_rfc3339()
}

/// Map a single OTEL span to a NormalizedEvent, if the span type is recognized.
/// Returns None for unknown/unmapped span types (logged at debug level).
fn map_span_to_event(
    span: &Span,
    agent_type: AgentType,
    service_name: &str,
) -> Option<NormalizedEvent> {
    let session_id = span_attr(span, "session.id")
        .or_else(|| span_attr(span, "session_id"))
        .unwrap_or_else(|| {
            // Fall back to trace_id as session identifier
            span.trace_id.iter().map(|b| format!("{:02x}", b)).collect()
        });

    let timestamp = nanos_to_iso(span.start_time_unix_nano as u64);
    let name_lower = span.name.to_lowercase();

    // Session lifecycle spans
    if name_lower.contains("session.start") || name_lower.contains("session_start") {
        return Some(NormalizedEvent::SessionStarted {
            agent_type,
            session_id,
            timestamp,
            source: EventSource::Otel,
        });
    }

    if name_lower.contains("session.end")
        || name_lower.contains("session_end")
        || name_lower.contains("session.stop")
    {
        return Some(NormalizedEvent::SessionEnded {
            agent_type,
            session_id,
            timestamp,
            source: EventSource::Otel,
        });
    }

    // Token/usage spans
    if name_lower.contains("token") || name_lower.contains("usage") || name_lower.contains("llm") {
        let input_tokens = span_attr_int(span, "llm.input_tokens")
            .or_else(|| span_attr_int(span, "input_tokens"))
            .or_else(|| span_attr_int(span, "gen_ai.usage.prompt_tokens"));
        let output_tokens = span_attr_int(span, "llm.output_tokens")
            .or_else(|| span_attr_int(span, "output_tokens"))
            .or_else(|| span_attr_int(span, "gen_ai.usage.completion_tokens"));

        if let (Some(input), Some(output)) = (input_tokens, output_tokens) {
            return Some(NormalizedEvent::TokensUsed {
                agent_type,
                session_id,
                timestamp,
                source: EventSource::Otel,
                input_tokens: input,
                output_tokens: output,
            });
        }
    }

    // Tool spans
    if name_lower.contains("tool") || name_lower.contains("function_call") {
        let tool_name = span_attr(span, "tool.name")
            .or_else(|| span_attr(span, "tool_name"))
            .unwrap_or_else(|| span.name.clone());

        let span_status = span.status.as_ref().map(|s| s.code()).unwrap_or(OtelStatusCode::Unset);
        let duration_nanos = (span.end_time_unix_nano as u64).saturating_sub(span.start_time_unix_nano as u64);
        let has_ended = span.end_time_unix_nano > 0 && duration_nanos > 0;

        if has_ended {
            let status_str = match span_status {
                OtelStatusCode::Error => "error".to_string(),
                _ => "success".to_string(),
            };
            let result_preview = span_attr(span, "tool.result")
                .or_else(|| span_attr(span, "result"))
                .map(|s| if s.len() > 200 { s[..200].to_string() } else { s });

            return Some(NormalizedEvent::ToolCompleted {
                agent_type,
                session_id,
                timestamp,
                source: EventSource::Otel,
                tool_name,
                status: status_str,
                duration_ms: Some(duration_nanos / 1_000_000),
                result_preview,
            });
        } else {
            let tool_input = span_attr(span, "tool.input")
                .or_else(|| span_attr(span, "tool_input"));

            return Some(NormalizedEvent::ToolStarted {
                agent_type,
                session_id,
                timestamp,
                source: EventSource::Otel,
                tool_name,
                tool_input,
            });
        }
    }

    // Unrecognized span — log and skip
    eprintln!(
        "[otel] unmapped span type: name={}, service={}",
        span.name, service_name
    );
    None
}
