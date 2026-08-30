use crate::artifact::ArtifactService;
use crate::error::ApiError;
use crate::state::AppState;
use crate::util::querystring::QueryString;
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use serde::Deserialize;
use std::sync::Arc;

pub fn routes() -> Router<AppState> {
    Router::new().route("/artifacts", get(get_artifact))
}

#[derive(Deserialize)]
struct GetQuery {
    project_id: u64,
    job_id: u64,
}

async fn get_artifact(
    QueryString(GetQuery { project_id, job_id }): QueryString<GetQuery>,
    State(artifact_service): State<Arc<ArtifactService>>,
) -> Result<Bytes, ApiError> {
    artifact_service.get_artifact(project_id, job_id).await
}
