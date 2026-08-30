use crate::config::config_app::ApiConfig;
use crate::state::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;

pub fn routes() -> Router<AppState> {
    Router::new().route("/config", get(get_api_config))
}

async fn get_api_config(State(api_config): State<Arc<ApiConfig>>) -> Json<ApiConfig> {
    let config = api_config.as_ref();
    Json(config.clone())
}
