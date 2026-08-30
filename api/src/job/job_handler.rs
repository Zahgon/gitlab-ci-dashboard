use crate::error::ApiError;
use crate::job::JobService;
use crate::model::{Job, JobStatus};
use crate::state::AppState;
use crate::util::querystring::QueryString;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

pub fn routes() -> Router<AppState> {
    Router::new().route("/jobs", get(get_jobs))
}

#[derive(Deserialize)]
struct GetQuery {
    project_id: u64,
    pipeline_id: u64,
    scope: Vec<JobStatus>,
}

async fn get_jobs(
    QueryString(GetQuery {
        project_id,
        pipeline_id,
        scope,
    }): QueryString<GetQuery>,
    State(job_service): State<Arc<JobService>>,
) -> Result<Json<Vec<Job>>, ApiError> {
    let result = job_service
        .get_jobs(project_id, pipeline_id, &scope)
        .await?;
    Ok(Json(result))
}
