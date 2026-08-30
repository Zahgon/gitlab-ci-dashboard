use crate::error::ApiError;
use crate::model::ScheduleProjectPipeline;
use crate::schedule::PipelineAggregator;
use crate::state::AppState;
use crate::util::querystring::QueryString;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

pub fn routes() -> Router<AppState> {
    Router::new().route("/schedules/latest-pipelines", get(get_with_latest_pipeline))
}

#[derive(Deserialize)]
struct GetQuery {
    group_id: u64,
    project_ids: Option<Vec<u64>>,
}

async fn get_with_latest_pipeline(
    QueryString(GetQuery {
        group_id,
        project_ids,
    }): QueryString<GetQuery>,
    State(aggregator): State<Arc<PipelineAggregator>>,
) -> Result<Json<Vec<ScheduleProjectPipeline>>, ApiError> {
    let result = aggregator
        .get_schedules_with_latest_pipeline(group_id, project_ids)
        .await?;

    Ok(Json(result))
}
