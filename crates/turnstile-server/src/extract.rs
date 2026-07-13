//! Custom extractors.

use std::convert::Infallible;
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

/// Optional peer address.
///
/// `Some(SocketAddr)` when the server runs with
/// [`axum::serve`] + `into_make_service_with_connect_info::<SocketAddr>()`;
/// `None` otherwise (e.g. oneshot integration tests, which carry no real
/// connection). Reads the `ConnectInfo` request extension directly so it never
/// rejects — unlike `Option<ConnectInfo<_>>`, which doesn't satisfy axum's
/// `Handler` bound in 0.8.
pub struct OptionalConnectInfo(pub Option<SocketAddr>);

impl<S> FromRequestParts<S> for OptionalConnectInfo
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let addr = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0);
        Ok(OptionalConnectInfo(addr))
    }
}
