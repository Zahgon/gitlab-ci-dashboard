use crate::config::config_app::ApiConfig;
use crate::error::ApiError;
use crate::model::{Pipeline, PipelineSource};
use crate::pipeline::PipelineService;
use crate::state::AppState;
use crate::util::json::JsonBody;
use crate::util::querystring::QueryString;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pipelines", get(get_pipelines))
        .route("/pipelines/start", post(start_pipeline))
        .route("/pipelines/retry", post(retry_pipeline))
        .route("/pipelines/cancel", post(cancel_pipeline))
}

#[derive(Deserialize)]
struct GetQuery {
    project_id: u64,
    source: Option<PipelineSource>,
}

async fn get_pipelines(
    QueryString(GetQuery { project_id, source }): QueryString<GetQuery>,
    State(pipeline_service): State<Arc<PipelineService>>,
) -> Result<Json<Vec<Pipeline>>, ApiError> {
    let pipelines = pipeline_service.get_pipelines(project_id, source).await?;
    Ok(Json(pipelines))
}

#[derive(Deserialize)]
struct PostQuery {
    project_id: u64,
    pipeline_id: u64,
}

async fn retry_pipeline(
    QueryString(PostQuery {
        project_id,
        pipeline_id,
    }): QueryString<PostQuery>,
    State(pipeline_service): State<Arc<PipelineService>>,
    State(api_config): State<Arc<ApiConfig>>,
) -> Result<Json<Pipeline>, ApiError> {
    if api_config.read_only {
        return Err(ApiError::bad_request(
            "can't retry pipeline when in 'read only' mode",
        ));
    }

    let pipeline = pipeline_service
        .retry_pipeline(project_id, pipeline_id)
        .await?;

    Ok(Json(pipeline))
}

async fn cancel_pipeline(
    QueryString(PostQuery {
        project_id,
        pipeline_id,
    }): QueryString<PostQuery>,
    State(pipeline_service): State<Arc<PipelineService>>,
    State(api_config): State<Arc<ApiConfig>>,
) -> Result<Json<Pipeline>, ApiError> {
    if api_config.read_only {
        return Err(ApiError::bad_request(
            "can't cancel pipeline when in 'read only' mode",
        ));
    }

    let pipeline = pipeline_service
        .cancel_pipeline(project_id, pipeline_id)
        .await?;

    Ok(Json(pipeline))
}

#[derive(Deserialize, Serialize)]
struct PostBody {
    project_id: u64,
    branch: String,
    env_vars: Option<HashMap<String, String>>,
}

async fn start_pipeline(
    State(pipeline_service): State<Arc<PipelineService>>,
    State(api_config): State<Arc<ApiConfig>>,
    JsonBody(PostBody {
        project_id,
        branch,
        env_vars,
    }): JsonBody<PostBody>,
) -> Result<Json<Pipeline>, ApiError> {
    if api_config.read_only {
        return Err(ApiError::bad_request(
            "can't start a new pipeline when in 'read only' mode",
        ));
    }

    let pipeline = pipeline_service
        .start_pipeline(project_id, branch, env_vars)
        .await?;

    Ok(Json(pipeline))
}
