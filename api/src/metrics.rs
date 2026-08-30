use axum::extract::{MatchedPath, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};
use std::sync::Arc;
use std::time::Instant;

const LABELS: [&str; 3] = ["endpoint", "method", "status"];

/// Prometheus request metrics, replacing `actix-web-prom`. The metric names,
/// labels and buckets are the ones `PrometheusMetricsBuilder::new("")` used to
/// register: an empty namespace leaves the metric name unprefixed.
pub struct Metrics {
    registry: Registry,
    http_requests_total: IntCounterVec,
    http_requests_duration_seconds: HistogramVec,
}

impl Metrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let http_requests_total = IntCounterVec::new(
            Opts::new("http_requests_total", "Total number of HTTP requests"),
            &LABELS,
        )?;
        let http_requests_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "http_requests_duration_seconds",
                "HTTP request duration in seconds for all requests",
            ),
            &LABELS,
        )?;

        registry.register(Box::new(http_requests_total.clone()))?;
        registry.register(Box::new(http_requests_duration_seconds.clone()))?;

        // Matches the `process` feature of actix-web-prom, which is only able
        // to read process statistics on linux.
        #[cfg(target_os = "linux")]
        registry.register(Box::new(
            prometheus::process_collector::ProcessCollector::for_self(),
        ))?;

        Ok(Self {
            registry,
            http_requests_total,
            http_requests_duration_seconds,
        })
    }

    fn observe(&self, endpoint: &str, method: &str, status: &str, elapsed_seconds: f64) {
        let labels = [endpoint, method, status];
        self.http_requests_total.with_label_values(&labels).inc();
        self.http_requests_duration_seconds
            .with_label_values(&labels)
            .observe(elapsed_seconds);
    }

    fn encode(&self) -> Result<(String, Vec<u8>), prometheus::Error> {
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        encoder.encode(&self.registry.gather(), &mut buffer)?;
        Ok((format!("{}; charset=utf-8", encoder.format_type()), buffer))
    }
}

/// Middleware counting every request and recording how long it took.
pub async fn track(State(metrics): State<Arc<Metrics>>, request: Request, next: Next) -> Response {
    let endpoint = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
        .unwrap_or_else(|| endpoint_without_match(request.uri().path()));
    let method = request.method().to_string();

    let started = Instant::now();
    let response = next.run(request).await;
    let elapsed = started.elapsed().as_secs_f64();

    metrics.observe(&endpoint, &method, response.status().as_str(), elapsed);

    response
}

/// Label for a request no route pattern matched. The SPA was mounted on the
/// empty root prefix, so actix-web-prom labelled every path it served with the
/// empty pattern; only requests that left the `/api` scope unmatched were
/// labelled by their path. Falling back to the raw path everywhere would make
/// the label unbounded, since any client-side route would mint a new series.
fn endpoint_without_match(path: &str) -> String {
    if path == "/api" || path.starts_with("/api/") {
        path.to_owned()
    } else {
        String::new()
    }
}

/// Renders the registry in the prometheus text exposition format. Served on
/// the endpoint actix-web-prom exposed by itself.
pub async fn render(Extension(metrics): Extension<Arc<Metrics>>) -> Response {
    match metrics.encode() {
        Ok((content_type, body)) => ([(header::CONTENT_TYPE, content_type)], body).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}
