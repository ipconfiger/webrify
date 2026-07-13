//! `GET /demo` — a minimal same-origin page exercising the embedded widget.
//!
//! Useful for manual smoke-testing and the Playwright E2E. Because the page is
//! served from the same origin as the API, the widget's `/challenge` and
//! `/verify` calls carry the correct `Origin` and pass the allowlist.

use axum::response::Html;
use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/demo", get(demo))
}

pub async fn demo() -> Html<&'static str> {
    Html(DEMO_HTML)
}

const DEMO_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Webrify Turnstile — Demo</title>
  <style>
    body { font-family: system-ui, sans-serif; max-width: 32rem; margin: 2rem auto; padding: 0 1rem; color: #1f1f1f; }
    h1 { font-size: 1.25rem; }
    #ts { margin: 1rem 0; }
    #result { margin-top: 1rem; word-break: break-all; min-height: 1.2em; }
  </style>
</head>
<body>
  <h1>Webrify Turnstile</h1>
  <div id="ts"></div>
  <p id="result" aria-live="polite"></p>
  <script type="module">
    import { mount } from '/widget/turnstile.js';
    mount(document.getElementById('ts'), {
      onVerify: (token) => {
        document.getElementById('result').textContent = 'OK token=' + token.slice(0, 24) + '…';
      },
      onError: (msg) => {
        document.getElementById('result').textContent = 'ERROR ' + msg;
      },
    });
  </script>
</body>
</html>"#;
