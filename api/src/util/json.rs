use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

/// JSON body extractor reproducing the rejections `actix_web::web::Json`
/// produced: `400 Content type error` for a non JSON content type and
/// `400 Json deserialize error: ..` for a body that does not deserialize.
/// `axum::Json` answers `415` and `422` for those two cases instead.
pub struct JsonBody<T>(pub T);

impl<T, S> FromRequest<S> for JsonBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        if !is_json_content_type(&request) {
            return Err((StatusCode::BAD_REQUEST, "Content type error").into_response());
        }

        let body = Bytes::from_request(request, state)
            .await
            .map_err(IntoResponse::into_response)?;

        serde_json::from_slice(&body).map(JsonBody).map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("Json deserialize error: {error}"),
            )
                .into_response()
        })
    }
}

fn is_json_content_type(request: &Request) -> bool {
    let Some(content_type) = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };

    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    mime == "application/json" || (mime.starts_with("application/") && mime.ends_with("+json"))
}
