use axum::{
    http::header,
    response::{Html, IntoResponse, Redirect},
};

const CACHE_CONTROL: &str = "no-store, max-age=0";
const REFERRER_POLICY: &str = "no-referrer";
const CONTENT_TYPE_OPTIONS: &str = "nosniff";
const DASHBOARD_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

pub(crate) async fn dashboard_root() -> Redirect {
    Redirect::temporary("/dashboard")
}

pub(crate) async fn dashboard_index() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE_CONTROL),
            (header::REFERRER_POLICY, REFERRER_POLICY),
            (header::X_CONTENT_TYPE_OPTIONS, CONTENT_TYPE_OPTIONS),
            (header::CONTENT_SECURITY_POLICY, DASHBOARD_CSP),
        ],
        Html(include_str!("../static/dashboard.html")),
    )
}

pub(crate) async fn dashboard_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE_CONTROL),
            (header::REFERRER_POLICY, REFERRER_POLICY),
            (header::X_CONTENT_TYPE_OPTIONS, CONTENT_TYPE_OPTIONS),
        ],
        include_str!("../static/dashboard.css"),
    )
}

pub(crate) async fn dashboard_js() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, CACHE_CONTROL),
            (header::REFERRER_POLICY, REFERRER_POLICY),
            (header::X_CONTENT_TYPE_OPTIONS, CONTENT_TYPE_OPTIONS),
        ],
        include_str!("../static/dashboard.js"),
    )
}
