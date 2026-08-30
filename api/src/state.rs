use crate::artifact::ArtifactService;
use crate::branch::{self, BranchService};
use crate::config::config_app::ApiConfig;
use crate::group::GroupService;
use crate::job::JobService;
use crate::pipeline::PipelineService;
use crate::{project, schedule};
use axum::extract::FromRef;
use std::sync::Arc;

/// Everything handlers used to receive as actix `Data<T>` application data.
/// The `FromRef` implementations below let a handler ask for a single
/// dependency with `State<Arc<T>>` instead of the whole state.
#[derive(Clone)]
pub struct AppState {
    pub api_config: Arc<ApiConfig>,
    pub group_service: Arc<GroupService>,
    pub project_aggregator: Arc<project::PipelineAggregator>,
    pub branch_aggregator: Arc<branch::PipelineAggregator>,
    pub schedule_aggregator: Arc<schedule::PipelineAggregator>,
    pub job_service: Arc<JobService>,
    pub pipeline_service: Arc<PipelineService>,
    pub branch_service: Arc<BranchService>,
    pub artifact_service: Arc<ArtifactService>,
}

impl FromRef<AppState> for Arc<ApiConfig> {
    fn from_ref(state: &AppState) -> Self {
        state.api_config.clone()
    }
}

impl FromRef<AppState> for Arc<GroupService> {
    fn from_ref(state: &AppState) -> Self {
        state.group_service.clone()
    }
}

impl FromRef<AppState> for Arc<project::PipelineAggregator> {
    fn from_ref(state: &AppState) -> Self {
        state.project_aggregator.clone()
    }
}

impl FromRef<AppState> for Arc<branch::PipelineAggregator> {
    fn from_ref(state: &AppState) -> Self {
        state.branch_aggregator.clone()
    }
}

impl FromRef<AppState> for Arc<schedule::PipelineAggregator> {
    fn from_ref(state: &AppState) -> Self {
        state.schedule_aggregator.clone()
    }
}

impl FromRef<AppState> for Arc<JobService> {
    fn from_ref(state: &AppState) -> Self {
        state.job_service.clone()
    }
}

impl FromRef<AppState> for Arc<PipelineService> {
    fn from_ref(state: &AppState) -> Self {
        state.pipeline_service.clone()
    }
}

impl FromRef<AppState> for Arc<BranchService> {
    fn from_ref(state: &AppState) -> Self {
        state.branch_service.clone()
    }
}

impl FromRef<AppState> for Arc<ArtifactService> {
    fn from_ref(state: &AppState) -> Self {
        state.artifact_service.clone()
    }
}
