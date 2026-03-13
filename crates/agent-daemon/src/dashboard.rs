use axum::{
    http::header,
    response::{Html, IntoResponse, Redirect},
};

const CACHE_CONTROL: &str = "no-store, max-age=0";

pub(crate) async fn dashboard_root() -> Redirect {
    Redirect::temporary("/dashboard")
}

pub(crate) async fn dashboard_index() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE_CONTROL),
        ],
        Html(include_str!("../static/dashboard.html")),
    )
}

pub(crate) async fn dashboard_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE_CONTROL),
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
        ],
        include_str!("../static/dashboard.js"),
    )
}
