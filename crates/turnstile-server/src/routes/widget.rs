//! `GET /widget/{*path}` — serve the embedded frontend widget.
//!
//! The widget is compiled into the binary via `rust-embed` (the Vite build
//! output under `packages/turnstile-widget/dist/`). Responses carry explicit
//! Content-Type overrides — wasm-bindgen's streaming instantiation requires
//! `application/wasm`, and a silent fallback to `application/octet-stream`
//! would force the slower array-buffer path. A split cache policy keeps hashed
//! chunks under `assets/` immutable while the entry (`turnstile.js`) is always
//! revalidated, so a binary redeploy can never serve a stale entry pointing at
//! old hashed chunks.

use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;

use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "../../packages/turnstile-widget/dist/"]
struct WidgetAsset;

pub fn router() -> Router<AppState> {
    Router::new().route("/widget/{*path}", get(serve))
}

pub async fn serve(Path(path): Path<String>) -> Response {
    // `GET /widget` (empty path) → the entry document.
    let key = if path.is_empty() {
        "index.html".to_string()
    } else {
        path
    };
    let asset = WidgetAsset::get(&key).or_else(|| WidgetAsset::get(&format!("{key}/index.html")));
    let Some(asset) = asset else {
        return (StatusCode::NOT_FOUND, "widget asset not found").into_response();
    };

    let mime = mime_for(&key);
    let mut resp = (
        [(header::CONTENT_TYPE, HeaderValue::from_static(mime))],
        Body::from(asset.data.into_owned()),
    )
        .into_response();
    apply_cache(&key, &mut resp);
    resp
}

/// Explicit Content-Type map — never trust a silent `octet-stream` fallback
/// (it breaks `WebAssembly.instantiateStreaming` for `.wasm`).
fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "wasm" => "application/wasm",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "map" => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Split cache: hashed chunks under `assets/` are far-future immutable; the
/// entry and root files are revalidated every request.
fn apply_cache(path: &str, resp: &mut Response) {
    let cc = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cc));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_overrides() {
        assert_eq!(
            mime_for("turnstile.js"),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(mime_for("assets/x_bg.wasm"), "application/wasm");
        assert_eq!(mime_for("assets/style.css"), "text/css; charset=utf-8");
        assert_eq!(mime_for("index.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for("data.bin"), "application/octet-stream");
    }
}
