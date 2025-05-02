use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{{\"error\": \"Cannot get config: Forbidden\"}}")]
    Forbidden,
    #[error("{{\"error\": \"Cannot get config: Unauthorized\"}}")]
    Unauthorized,
    #[error("{{\"error\": \"Cannot get config: Not found\"}}")]
    NotFound,
    #[error("{{\"error\": \"Internal error\"}}")]
    ServerError,

    #[error(transparent)]
    OpenTelemetry {
        #[from]
        source: opentelemetry::trace::TraceError,
    },
    #[error(transparent)]
    TracingErr {
        #[from]
        source: tracing::dispatcher::SetGlobalDefaultError,
    },
    #[error(transparent)]
    Hyper {
        #[from]
        source: hyper::Error,
    },
    #[error(transparent)]
    SerdeJson {
        #[from]
        source: serde_json::Error,
    },
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let payload = self.to_string();
        let body = axum::body::Body::new(payload);

        let status_code = match &self {
            Error::Unauthorized | Error::Forbidden => StatusCode::UNAUTHORIZED,
            Error::NotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Response::builder().status(status_code).body(body).unwrap()
    }
}
