use axum::Extension;
use axum::{extract::OriginalUri, http::StatusCode, response::IntoResponse, Json};
use clap::{crate_description, crate_name, crate_version};
use metrics_exporter_prometheus::PrometheusHandle;
use serde_json::json;
use serde_json::Value;

use crate::error::Error as RestError;
use crate::State;

// This is required in order to get the method from the request
#[derive(Debug)]
pub struct RequestMethod(pub hyper::Method);

pub async fn metrics(
    Extension(recorder_handle): Extension<PrometheusHandle>,
    Extension(state): Extension<State>,
) -> Result<String, RestError> {
    log::info!("{{\"fn\": \"metrics\", \"method\":\"get\"}}");
    match state.get_metrics().await {
        Ok(_) => metrics::gauge!("ece_cluster_up", 1f64),
        Err(_) => metrics::gauge!("ece_cluster_up", 0f64),
    };
    Ok(recorder_handle.render())
}

pub async fn health() -> Json<Value> {
    log::info!("{{\"fn\": \"health\", \"method\":\"get\"}}");
    Json(json!({ "msg": "Healthy"}))
}

pub async fn root() -> Json<Value> {
    log::info!("{{\"fn\": \"root\", \"method\":\"get\"}}");
    Json(
        json!({ "version": crate_version!(), "name": crate_name!(), "description": crate_description!()}),
    )
}

pub async fn handler_404(OriginalUri(original_uri): OriginalUri) -> impl IntoResponse {
    let parts = original_uri.into_parts();
    let path_and_query = parts.path_and_query.expect("Missing post path and query");
    log::info!(
        "{{\"fn\": \"handler_404\", \"method\":\"get\", \"path\":\"{}\"}}",
        path_and_query
    );
    (
        StatusCode::NOT_FOUND,
        "{\"error_code\": 404, \"message\": \"HTTP 404 Not Found\"}",
    )
}
