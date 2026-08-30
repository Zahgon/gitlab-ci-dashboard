use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;
use serde_querystring::ParseMode;

/// Query string extractor which parses comma delimited values into sequences,
/// so `?scope=running,failed` deserializes into a `Vec`.
///
/// Replaces `serde_querystring_actix::QueryString`. The parse mode used to be
/// supplied at runtime through a `QueryStringConfig` registered as actix
/// application data; it is fixed here because the application only ever
/// registered the comma delimiter.
pub struct QueryString<T>(pub T);

impl<T, S> FromRequestParts<S> for QueryString<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or_default();
        serde_querystring::from_str::<T>(query, ParseMode::Delimiter(b','))
            .map(QueryString)
            .map_err(|error| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Query deserialize error: {error}"),
                )
                    .into_response()
            })
    }
}
