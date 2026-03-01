use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use prost::Message;
use std::net::SocketAddr;

/// Start the OTEL HTTP/protobuf ingestion server on localhost:4318.
///
/// Accepts OTLP/HTTP trace exports at POST /v1/traces.
/// Gracefully handles port-already-in-use by logging a warning and returning.
pub async fn start() {
    let app = Router::new().route("/v1/traces", post(handle_traces));

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

async fn handle_traces(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
            log_spans(&request);

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

fn log_spans(request: &ExportTraceServiceRequest) {
    for resource_spans in &request.resource_spans {
        let service_name = resource_spans
            .resource
            .as_ref()
            .and_then(|r| {
                r.attributes
                    .iter()
                    .find(|kv| kv.key == "service.name")
                    .and_then(|kv| kv.value.as_ref())
                    .and_then(|v| match &v.value {
                        Some(Value::StringValue(s)) => Some(s.as_str()),
                        _ => None,
                    })
            })
            .unwrap_or("unknown");

        for scope_spans in &resource_spans.scope_spans {
            for span in &scope_spans.spans {
                let trace_id: String =
                    span.trace_id.iter().map(|b| format!("{:02x}", b)).collect();
                eprintln!(
                    "[otel] span: name={}, service={}, trace_id={}",
                    span.name, service_name, trace_id
                );
            }
        }
    }
}
