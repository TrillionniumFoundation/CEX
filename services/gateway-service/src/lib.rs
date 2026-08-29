pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod interfaces;

use axum::Router;
use infrastructure::state::AppState;

pub fn build_router(state: AppState) -> Router {
    interfaces::http::router(state.clone()).merge(interfaces::saga::router(state))
}
