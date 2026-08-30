use crate::error::ApiError;
use crate::group::group_service::GroupService;
use crate::model::Group;
use crate::state::AppState;
use axum::extract::{OriginalUri, State};
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;

pub fn routes() -> Router<AppState> {
    Router::new().route("/groups", get(get_groups))
}

async fn get_groups(
    OriginalUri(uri): OriginalUri,
    State(group_service): State<Arc<GroupService>>,
) -> Result<Json<Vec<Group>>, ApiError> {
    let result = group_service.get_groups(uri.path()).await?;
    Ok(Json(result))
}
