#![forbid(unsafe_code)]

use crate::config::config_app::{ApiConfig, AppConfig};
use crate::config::config_file;
use crate::gitlab::GitlabClient;
use crate::metrics::Metrics;
use crate::spa::Spa;
use crate::state::AppState;
use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderName, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Extension, Router};
use dotenv::dotenv;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;

mod artifact;
mod branch;
mod config;
mod error;
mod gitlab;
mod group;
mod job;
mod metrics;
mod model;
mod pipeline;
mod project;
mod schedule;
mod spa;
mod state;
mod util;

fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    let file_config = config_file::FileConfig::load_from_toml();
    let file_config = match file_config {
        Ok(ref c) => Some(c),
        Err(config_file::Error::Deserialize(msg)) => panic!("{}", msg),
        Err(_) => None,
    };

    let mut app_config = AppConfig::new();
    let mut api_config = ApiConfig::new();

    if let Some(fc) = file_config {
        app_config = app_config.merge_with_file_config(fc);
        api_config = api_config.merge_with_file_config(fc);
    };

    log::info!("Gitlab CI Dashboard :: {} ::", &api_config.api_version);

    log::debug!("{app_config:?}");
    log::debug!("{api_config:?}");

    // Actix spawned `server_workers` worker threads itself. Axum runs on a
    // plain tokio runtime, so the runtime is built by hand to keep the
    // `SERVER_WORKER_COUNT` setting meaningful.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(app_config.server_workers.max(1))
        .enable_all()
        .build()?
        .block_on(serve(app_config, api_config))
}

async fn serve(app_config: AppConfig, api_config: ApiConfig) -> std::io::Result<()> {
    let gitlab_client = Arc::new(GitlabClient::new(
        &app_config.gitlab_url,
        &app_config.gitlab_token,
    ));

    let group_service = Arc::new(group::GroupService::new(
        gitlab_client.clone(),
        app_config.clone(),
    ));
    let pipeline_service = Arc::new(pipeline::PipelineService::new(
        gitlab_client.clone(),
        app_config.clone(),
    ));
    let project_service = Arc::new(project::ProjectService::new(
        gitlab_client.clone(),
        app_config.clone(),
    ));
    let job_service = Arc::new(job::JobService::new(
        gitlab_client.clone(),
        app_config.clone(),
    ));
    let branch_service = Arc::new(branch::BranchService::new(
        gitlab_client.clone(),
        app_config.clone(),
    ));
    let artifact_service = Arc::new(artifact::ArtifactService::new(
        gitlab_client.clone(),
        app_config.clone(),
    ));

    let project_aggr = Arc::new(project::PipelineAggregator::new(
        project_service.as_ref().clone(),
        pipeline_service.as_ref().clone(),
        job_service.as_ref().clone(),
    ));
    let branch_aggr = Arc::new(branch::PipelineAggregator::new(
        branch_service.as_ref().clone(),
        pipeline_service.as_ref().clone(),
        job_service.as_ref().clone(),
    ));
    let schedule_aggr = Arc::new(schedule::PipelineAggregator::new(
        schedule::ScheduleService::new(gitlab_client.clone(), app_config.clone()),
        project_service.as_ref().clone(),
        pipeline_service.as_ref().clone(),
        job_service.as_ref().clone(),
    ));

    let app = configure_app(
        Arc::new(api_config),
        group_service,
        project_aggr,
        branch_aggr,
        schedule_aggr,
        job_service,
        pipeline_service,
        branch_service,
        artifact_service,
    );

    let listener = TcpListener::bind((app_config.server_ip.as_str(), app_config.server_port)).await?;

    axum::serve(listener, app).await
}

#[allow(clippy::too_many_arguments)]
fn configure_app(
    api_config: Arc<ApiConfig>,
    group_service: Arc<group::GroupService>,
    project_aggregator: Arc<project::PipelineAggregator>,
    branch_aggregator: Arc<branch::PipelineAggregator>,
    schedule_aggregator: Arc<schedule::PipelineAggregator>,
    job_service: Arc<job::JobService>,
    pipeline_service: Arc<pipeline::PipelineService>,
    branch_service: Arc<branch::BranchService>,
    artifact_service: Arc<artifact::ArtifactService>,
) -> Router {
    let state = AppState {
        api_config,
        group_service,
        project_aggregator,
        branch_aggregator,
        schedule_aggregator,
        job_service,
        pipeline_service,
        branch_service,
        artifact_service,
    };

    let prom = Arc::new(setup_prometheus());

    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics/prometheus", get(metrics::render))
        .nest("/api", api_routes())
        .route("/api/", any(api_not_found))
        .fallback_service(setup_spa())
        .with_state(state)
        .layer(Extension(prom.clone()))
        .layer(middleware::from_fn_with_state(prom, metrics::track))
        .layer(middleware::from_fn(log_request))
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(config::routes())
        .merge(group::routes())
        .merge(project::routes())
        .merge(pipeline::routes())
        .merge(branch::routes())
        .merge(schedule::routes())
        .merge(job::routes())
        .merge(artifact::routes())
        .fallback(api_not_found)
        .method_not_allowed_fallback(api_not_found)
        .layer(middleware::from_fn(reject_head))
}

/// Actix' `web::get()` guard matched GET alone, so `HEAD /api/..` fell out of
/// the scope as a `404`. Axum answers HEAD with the GET handler unless it is
/// stopped before routing.
async fn reject_head(request: Request, next: Next) -> Response {
    if request.method() == Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(request).await
}

/// Actix' `scope("/api")` consumed every request below the prefix, so unknown
/// paths and wrong methods answered `404` instead of reaching the SPA fallback
/// or producing a `405`.
async fn api_not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn health_handler() -> StatusCode {
    StatusCode::OK
}

fn setup_prometheus() -> Metrics {
    Metrics::new().expect("prometheus endpoint to be created")
}

fn setup_spa() -> Router {
    if cfg!(debug_assertions) {
        Spa::default().finish()
    } else {
        Spa::new("./spa/index.html", "/", "./spa").finish()
    }
}

/// Access log, replacing actix' `Logger::default()` middleware.
async fn log_request(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let version = request.version();
    let target = match request.uri().query() {
        Some(query) => format!("{}?{}", request.uri().path(), query),
        None => request.uri().path().to_owned(),
    };
    let referer = header_or_dash(request.headers(), header::REFERER);
    let user_agent = header_or_dash(request.headers(), header::USER_AGENT);

    let started = Instant::now();
    let response = next.run(request).await;
    let elapsed = started.elapsed().as_secs_f64();

    let status = response.status().as_u16();
    let size = header_or_dash(response.headers(), header::CONTENT_LENGTH);

    log::info!(
        "\"{method} {target} {version:?}\" {status} {size} \"{referer}\" \"{user_agent}\" {elapsed:.6}"
    );

    response
}

fn header_or_dash(headers: &HeaderMap, name: HeaderName) -> String {
    headers
        .get(&name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use bytes::Bytes;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use serde_json::json;
    use serial_test::serial;
    use std::collections::HashMap;
    use std::env;
    use std::ops::Deref;
    use tower::ServiceExt;

    use crate::error::ApiError;
    use crate::gitlab::GitlabApi;
    use crate::model::{
        Branch, BranchPipeline, Group, Job, JobStatus, Pipeline, Project, ProjectPipeline,
        ProjectPipelines, Schedule, ScheduleProjectPipeline,
    };

    use super::*;

    #[macro_export]
    macro_rules! setup_app {
        () => {{
            use super::*;

            env::set_var("GITLAB_BASE_URL", "https://gitlab.url");
            env::set_var("GITLAB_API_TOKEN", "token123");
            env::set_var("API_READ_ONLY", "false");

            let gcd_config = AppConfig::new();

            let gitlab_client = Arc::new(GitlabClientTest {});

            let api_config = Arc::new(ApiConfig::new());

            let group_service = Arc::new(group::GroupService::new(
                gitlab_client.clone(),
                gcd_config.clone(),
            ));
            let pipeline_service = Arc::new(pipeline::PipelineService::new(
                gitlab_client.clone(),
                gcd_config.clone(),
            ));
            let project_service = Arc::new(project::ProjectService::new(
                gitlab_client.clone(),
                gcd_config.clone(),
            ));
            let job_service = Arc::new(job::JobService::new(
                gitlab_client.clone(),
                gcd_config.clone(),
            ));
            let branch_service = Arc::new(branch::BranchService::new(
                gitlab_client.clone(),
                gcd_config.clone(),
            ));
            let artifact_service = Arc::new(artifact::ArtifactService::new(
                gitlab_client.clone(),
                gcd_config.clone(),
            ));

            let project_aggr = Arc::new(project::PipelineAggregator::new(
                project_service.as_ref().clone(),
                pipeline_service.as_ref().clone(),
                job_service.as_ref().clone(),
            ));
            let branch_aggr = Arc::new(branch::PipelineAggregator::new(
                branch_service.as_ref().clone(),
                pipeline_service.as_ref().clone(),
                job_service.as_ref().clone(),
            ));
            let schedule_aggr = Arc::new(schedule::PipelineAggregator::new(
                schedule::ScheduleService::new(gitlab_client.clone(), gcd_config.clone()),
                project_service.as_ref().clone(),
                pipeline_service.as_ref().clone(),
                job_service.as_ref().clone(),
            ));

            configure_app(
                api_config,
                group_service,
                project_aggr,
                branch_aggr,
                schedule_aggr,
                job_service,
                pipeline_service,
                branch_service,
                artifact_service,
            )
        }};
    }

    struct GitlabClientTest {}

    #[async_trait]
    impl GitlabApi for GitlabClientTest {
        async fn groups(
            &self,
            _skip_groups: &[u64],
            _top_level: bool,
        ) -> Result<Vec<Group>, ApiError> {
            Ok(vec![model::test::new_group()])
        }

        async fn projects(
            &self,
            _group_id: u64,
            _include_subgroups: bool,
        ) -> Result<Vec<Project>, ApiError> {
            Ok(vec![model::test::new_project()])
        }

        async fn latest_pipeline(
            &self,
            _project_id: u64,
            _branch: String,
        ) -> Result<Option<Pipeline>, ApiError> {
            Ok(Some(model::test::new_pipeline()))
        }

        async fn pipelines(
            &self,
            _project_id: u64,
            _updated_after: Option<DateTime<Utc>>,
        ) -> Result<Vec<Pipeline>, ApiError> {
            Ok(vec![model::test::new_pipeline()])
        }

        async fn retry_pipeline(
            &self,
            _project_id: u64,
            _pipeline_id: u64,
        ) -> Result<Pipeline, ApiError> {
            Ok(model::test::new_pipeline())
        }

        async fn start_pipeline(
            &self,
            _project_id: u64,
            _branch: String,
            _env_vars: Option<HashMap<String, String>>,
        ) -> Result<Pipeline, ApiError> {
            Ok(model::test::new_pipeline())
        }

        async fn cancel_pipeline(
            &self,
            _project_id: u64,
            _pipeline_id: u64,
        ) -> Result<Pipeline, ApiError> {
            Ok(model::test::new_pipeline())
        }

        async fn branches(&self, _project_id: u64) -> Result<Vec<Branch>, ApiError> {
            Ok(vec![model::test::new_branch()])
        }

        async fn schedules(&self, _project_id: u64) -> Result<Vec<Schedule>, ApiError> {
            Ok(vec![model::test::new_schedule()])
        }

        async fn jobs(
            &self,
            _project_id: u64,
            _pipeline_id: u64,
            _scope: &[JobStatus],
        ) -> Result<Vec<Job>, ApiError> {
            Ok(vec![model::test::new_job()])
        }

        async fn artifact(&self, _project_id: u64, _job_id: u64) -> Result<Bytes, ApiError> {
            Ok(Bytes::from("hello".to_string()))
        }
    }

    fn to_str(value: &[u8]) -> &str {
        std::str::from_utf8(value).expect("str to be created from bytes")
    }

    fn get(uri: &str) -> Request<Body> {
        Request::get(uri)
            .body(Body::empty())
            .expect("request to be created")
    }

    fn post(uri: &str) -> Request<Body> {
        Request::post(uri)
            .body(Body::empty())
            .expect("request to be created")
    }

    async fn call(app: Router, request: Request<Body>) -> (StatusCode, Bytes) {
        let response = app.oneshot(request).await.expect("response to be created");
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body to be read");

        (status, body)
    }

    #[tokio::test]
    async fn test_config_endpoint() {
        env::set_var("VERSION", "1.0.0");

        let app = setup_app!();
        let (status, body) = call(app, get("/api/config")).await;

        assert!(status.is_success());

        let result = serde_json::from_str::<ApiConfig>(to_str(&body)).unwrap();

        assert_eq!(result.api_version, "1.0.0");
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = setup_app!();
        let (status, _body) = call(app, get("/health")).await;

        assert!(status.is_success());
    }

    #[tokio::test]
    async fn test_groups_endpoint() {
        let app = setup_app!();
        let (status, _body) = call(app, get("/api/groups")).await;

        assert!(status.is_success());
    }

    #[tokio::test]
    async fn test_projects_with_latest_pipelines_endpoint() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/projects/latest-pipelines?group_id=1")).await;

        assert!(status.is_success());

        let result = serde_json::from_str::<Vec<ProjectPipeline>>(to_str(&body)).unwrap();
        assert_eq!(result.len(), 1);

        let first_entry = &result[0];
        let project = first_entry.clone().project;
        let pipeline = first_entry.clone().pipeline.unwrap();

        assert_eq!(project.id, 456);
        assert_eq!(pipeline.id, 1);
    }

    #[tokio::test]
    async fn test_projects_with_pipelines_endpoint() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/projects/pipelines?group_id=1")).await;

        assert!(status.is_success());

        let result = serde_json::from_str::<Vec<ProjectPipelines>>(to_str(&body)).unwrap();
        assert_eq!(result.len(), 1);

        let first_entry = &result[0];
        let project = first_entry.clone().project;
        assert_eq!(project.id, 456);

        let pipelines = first_entry.clone().pipelines;
        assert_eq!(pipelines.len(), 1);

        assert_eq!(pipelines[0].id, 1);
    }

    #[tokio::test]
    async fn test_branches_with_latest_pipelines_endpoint() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/branches/latest-pipelines?project_id=456")).await;

        assert!(status.is_success());

        let result = serde_json::from_str::<Vec<BranchPipeline>>(to_str(&body)).unwrap();
        assert_eq!(result.len(), 1);

        let first_entry = &result[0];
        let branch = first_entry.clone().branch;
        let pipeline = first_entry.clone().pipeline.unwrap();

        assert_eq!(branch.name, "branch-1");
        assert_eq!(pipeline.id, 1);
    }

    #[tokio::test]
    async fn test_branches_endpoint() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/branches?project_id=456")).await;

        assert!(status.is_success());

        let branches = serde_json::from_str::<Vec<Branch>>(to_str(&body)).unwrap();

        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "branch-1");
    }

    #[tokio::test]
    async fn test_schedules_with_latest_pipelines_endpoint() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/schedules/latest-pipelines?group_id=1")).await;

        assert!(status.is_success());

        let result = serde_json::from_str::<Vec<ScheduleProjectPipeline>>(to_str(&body)).unwrap();
        assert_eq!(result.len(), 1);

        let first_entry = &result[0];
        let schedule = first_entry.clone().schedule;
        let project = first_entry.clone().project;
        let pipeline = first_entry.clone().pipeline.unwrap();

        assert_eq!(schedule.id, 789);
        assert_eq!(project.id, 456);
        assert_eq!(pipeline.id, 1);
    }

    #[tokio::test]
    async fn test_jobs_endpoint() {
        let app = setup_app!();
        let (status, body) = call(
            app,
            get("/api/jobs?project_id=456&pipeline_id=1&scope=running"),
        )
        .await;

        assert!(status.is_success());

        let jobs = serde_json::from_str::<Vec<Job>>(to_str(&body)).unwrap();
        assert_eq!(jobs.len(), 1);

        assert_eq!(jobs[0].id, 1);
    }

    #[tokio::test]
    async fn test_pipelines_endpoint() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/pipelines?project_id=456&source=web")).await;

        assert!(status.is_success());

        let pipelines = serde_json::from_str::<Vec<Pipeline>>(to_str(&body)).unwrap();
        assert_eq!(pipelines.len(), 1);

        assert_eq!(pipelines[0].id, 1);
    }

    #[tokio::test]
    async fn test_retry_pipeline_endpoint() {
        let app = setup_app!();
        let (status, body) = call(
            app,
            post("/api/pipelines/retry?project_id=456&pipeline_id=1"),
        )
        .await;

        assert!(status.is_success());

        let pipeline = serde_json::from_str::<Pipeline>(to_str(&body)).unwrap();
        assert_eq!(pipeline.id, 1);
    }

    #[tokio::test]
    async fn test_cancel_pipeline_endpoint() {
        let app = setup_app!();
        let (status, body) = call(
            app,
            post("/api/pipelines/cancel?project_id=456&pipeline_id=1"),
        )
        .await;

        assert!(status.is_success());

        let pipeline = serde_json::from_str::<Pipeline>(to_str(&body)).unwrap();
        assert_eq!(pipeline.id, 1);
    }

    #[tokio::test]
    async fn test_start_pipeline_endpoint() {
        let app = setup_app!();
        let body = json!({
            "project_id": 1,
            "branch": "main",
            "env_vars": {
                "key1": "value1"
            }
        });
        let request = Request::post("/api/pipelines/start")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .expect("request to be created");

        let (status, body) = call(app, request).await;

        assert!(status.is_success());

        let pipeline = serde_json::from_str::<Pipeline>(to_str(&body)).unwrap();
        assert_eq!(pipeline.id, 1);
    }

    #[tokio::test]
    async fn test_artifact_endpoint() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/artifacts?project_id=456&job_id=1")).await;

        assert!(status.is_success());

        assert_eq!(String::from_utf8_lossy(body.deref()), "hello");
    }

    #[tokio::test]
    #[serial]
    async fn test_unknown_api_path_returns_not_found() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/does-not-exist")).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn test_api_route_with_wrong_method_returns_not_found() {
        let app = setup_app!();
        let (status, _body) = call(app, post("/api/config")).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    #[serial]
    async fn test_head_on_api_route_returns_not_found() {
        let app = setup_app!();
        let request = Request::head("/api/config")
            .body(Body::empty())
            .expect("request to be created");

        let (status, _body) = call(app, request).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    #[serial]
    async fn test_missing_query_parameter_returns_bad_request() {
        let app = setup_app!();
        let (status, body) = call(app, get("/api/jobs?project_id=456&pipeline_id=1")).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(to_str(&body).starts_with("Query deserialize error:"));
    }

    #[tokio::test]
    #[serial]
    async fn test_comma_delimited_scope_is_parsed_as_a_sequence() {
        let app = setup_app!();
        let (status, body) = call(
            app,
            get("/api/jobs?project_id=456&pipeline_id=1&scope=running,failed,success"),
        )
        .await;

        assert!(status.is_success());

        let jobs = serde_json::from_str::<Vec<Job>>(to_str(&body)).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_start_pipeline_with_incomplete_body_returns_bad_request() {
        let app = setup_app!();
        let request = Request::post("/api/pipelines/start")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "project_id": 1 }).to_string()))
            .expect("request to be created");

        let (status, body) = call(app, request).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(to_str(&body).starts_with("Json deserialize error:"));
    }

    #[tokio::test]
    #[serial]
    async fn test_start_pipeline_with_wrong_content_type_returns_bad_request() {
        let app = setup_app!();
        let request = Request::post("/api/pipelines/start")
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(
                json!({ "project_id": 1, "branch": "main" }).to_string(),
            ))
            .expect("request to be created");

        let (status, body) = call(app, request).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(to_str(&body), "Content type error");
    }
}
