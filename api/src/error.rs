use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Serialize)]
pub struct ApiError {
    status_code: u16,
    message: String,
}

impl ApiError {
    pub fn new(status_code: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status_code: status_code.as_u16(),
            message: message.into(),
        }
    }

    pub fn server_error(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn with_u16_code(status_code: u16, message: String) -> Self {
        Self {
            status_code,
            message,
        }
    }

    pub fn is_forbidden(&self) -> bool {
        StatusCode::from_u16(self.status_code)
            .map(|s| s == StatusCode::FORBIDDEN)
            .unwrap_or(false)
    }

    pub fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl Default for ApiError {
    fn default() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "an internal server error occured",
        )
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error {}: {}", self.status_code, self.message)
    }
}

impl Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_status() {
            return error.status().map_or_else(ApiError::default, |code| {
                ApiError::with_u16_code(code.as_u16(), error.to_string())
            });
        }

        match error.source() {
            Some(source) => ApiError::server_error(source.to_string()),
            None => ApiError::server_error(error.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status_code = self.status_code();
        let error_message = self.message.clone();

        serde_json::to_string(&ApiError::new(status_code, error_message)).map_or_else(
            |_| {
                (
                    status_code,
                    [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                    self.to_string(),
                )
                    .into_response()
            },
            |json| {
                (
                    status_code,
                    [(header::CONTENT_TYPE, "application/json")],
                    json,
                )
                    .into_response()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn into_parts(error: ApiError) -> (StatusCode, String, String) {
        let response = error.into_response();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body to be read");

        (status, content_type, String::from_utf8_lossy(&body).into_owned())
    }

    #[tokio::test]
    async fn bad_request_is_serialized_as_json() {
        let (status, content_type, body) = into_parts(ApiError::bad_request("nope")).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(content_type, "application/json");
        assert_eq!(body, r#"{"status_code":400,"message":"nope"}"#);
    }

    #[tokio::test]
    async fn server_error_is_serialized_as_json() {
        let (status, content_type, body) = into_parts(ApiError::server_error("boom")).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(content_type, "application/json");
        assert_eq!(body, r#"{"status_code":500,"message":"boom"}"#);
    }

    #[tokio::test]
    async fn default_error_is_an_internal_server_error() {
        let (status, _content_type, body) = into_parts(ApiError::default()).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            body,
            r#"{"status_code":500,"message":"an internal server error occured"}"#
        );
    }

    #[tokio::test]
    async fn out_of_range_status_code_falls_back_to_internal_server_error() {
        let (status, _content_type, _body) =
            into_parts(ApiError::with_u16_code(1000, "weird".to_owned())).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn is_forbidden_only_for_403() {
        assert!(ApiError::new(StatusCode::FORBIDDEN, "no").is_forbidden());
        assert!(!ApiError::bad_request("no").is_forbidden());
    }

    #[test]
    fn display_includes_status_code_and_message() {
        assert_eq!(
            ApiError::bad_request("nope").to_string(),
            "Error 400: nope"
        );
    }
}
