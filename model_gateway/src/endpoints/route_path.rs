//! A path parameter read in the gateway's error envelope.
//!
//! axum's `Path` extractor answers a parameter it cannot decode (a
//! percent-encoded id that is not UTF-8) as `text/plain`, outside the
//! `{"error": {...}}` envelope every other refusal of a route takes, so an
//! SDK that decodes `error` as an object fails on the body instead of
//! reading the message. `RoutePath` reads the parameter through `Path` and
//! turns its rejection into the envelope with the same status and axum's
//! own words.

use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
    response::Response,
};
use serde::de::DeserializeOwned;

use crate::routers::error as route_error;

/// The route's path parameter(s), as `Path<T>` reads them; a parameter axum
/// cannot decode is refused in the envelope (`400 invalid_request`, or the
/// rejection's own 5xx as `internal_error`) with axum's message.
pub struct RoutePath<T>(pub T);

impl<S, T> FromRequestParts<S> for RoutePath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(value)) => Ok(Self(value)),
            Err(rejection) => {
                let status = rejection.status();
                let code = if status.is_server_error() {
                    "internal_error"
                } else {
                    "invalid_request"
                };
                Err(route_error::create_error(
                    status,
                    code,
                    rejection.body_text(),
                ))
            }
        }
    }
}
