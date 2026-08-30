use crate::branch::PipelineAggregator;
use crate::error::ApiError;
use crate::model::{Branch, BranchPipeline};
use crate::state::AppState;
use crate::util::querystring::QueryString;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

use super::BranchService;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/branches/latest-pipelines", get(get_with_latest_pipeline))
        .route("/branches", get(get_branches))
}

#[derive(Deserialize)]
struct GetQuery {
    project_id: u64,
}

async fn get_branches(
    QueryString(GetQuery { project_id }): QueryString<GetQuery>,
    State(branch_service): State<Arc<BranchService>>,
) -> Result<Json<Vec<Branch>>, ApiError> {
    let result = branch_service.get_branches(project_id).await?;
    Ok(Json(result))
}

async fn get_with_latest_pipeline(
    QueryString(GetQuery { project_id }): QueryString<GetQuery>,
    State(aggregator): State<Arc<PipelineAggregator>>,
) -> Result<Json<Vec<BranchPipeline>>, ApiError> {
    let result = aggregator
        .get_branches_with_latest_pipeline(project_id)
        .await?;
    Ok(Json(result))
}
