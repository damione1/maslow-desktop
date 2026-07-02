// Shared HTTP error mapping. Mirrors the gRPC layer's `tonic::Status`
// selection (`Status::not_found`, `invalid_argument`, `failed_precondition`,
// `deadline_exceeded`, `internal`) against axum's `IntoResponse`, so both
// transports classify the same underlying `Result<_, String>` outcomes the
// same way.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    NotFound,
    InvalidArgument,
    FailedPrecondition,
    DeadlineExceeded,
    Internal,
}

#[derive(Debug)]
pub struct ApiError {
    kind: ErrorKind,
    message: String,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl ApiError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::NotFound, message: message.into() }
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::InvalidArgument, message: message.into() }
    }

    pub fn failed_precondition(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::FailedPrecondition, message: message.into() }
    }

    pub fn deadline_exceeded(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::DeadlineExceeded, message: message.into() }
    }

    /// Catch-all mapping for a plain `Result<_, String>` from `MaslowService`,
    /// matching the gRPC layer's default `Status::internal` treatment.
    pub fn internal(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Internal, message: message.into() }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.kind {
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::InvalidArgument => StatusCode::BAD_REQUEST,
            ErrorKind::FailedPrecondition => StatusCode::PRECONDITION_FAILED,
            ErrorKind::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(ErrorBody { error: self.message })).into_response()
    }
}
