use axum::{
    http::header,
    response::{Html, IntoResponse, Redirect},
    routing::get,
    Router,
};

const CACHE_CONTROL: &str = "no-store, max-age=0";
const REFERRER_POLICY: &str = "no-referrer";
const CONTENT_TYPE_OPTIONS: &str = "nosniff";
const DASHBOARD_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";
#[cfg(test)]
const DASHBOARD_ASSET_PATHS: &[&str] = &[
    "/dashboard.css",
    "/dashboard-control.js",
    "/dashboard-connectors.js",
    "/dashboard-providers.js",
    "/dashboard-plugins.js",
    "/dashboard-workspace.js",
    "/dashboard-settings.js",
    "/dashboard.js",
    "/dashboard-chat.js",
];

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

fn javascript_response(source: &'static str) -> impl IntoResponse {
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
        source,
    )
}

pub(crate) fn add_dashboard_asset_routes<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .route("/dashboard.css", get(dashboard_css))
        .route("/dashboard-control.js", get(dashboard_control_js))
        .route("/dashboard-connectors.js", get(dashboard_connectors_js))
        .route("/dashboard-providers.js", get(dashboard_providers_js))
        .route("/dashboard-plugins.js", get(dashboard_plugins_js))
        .route("/dashboard-workspace.js", get(dashboard_workspace_js))
        .route("/dashboard-settings.js", get(dashboard_settings_js))
        .route("/dashboard.js", get(dashboard_js))
        .route("/dashboard-chat.js", get(dashboard_chat_js))
}

pub(crate) async fn dashboard_control_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-control.js"))
}

pub(crate) async fn dashboard_connectors_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-connectors.js"))
}

pub(crate) async fn dashboard_providers_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-providers.js"))
}

pub(crate) async fn dashboard_plugins_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-plugins.js"))
}

pub(crate) async fn dashboard_workspace_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-workspace.js"))
}

pub(crate) async fn dashboard_settings_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-settings.js"))
}

pub(crate) async fn dashboard_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard.js"))
}

pub(crate) async fn dashboard_chat_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-chat.js"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_html_references_all_registered_assets() {
        let html = include_str!("../static/dashboard.html");
        for asset in DASHBOARD_ASSET_PATHS {
            assert!(
                html.contains(asset),
                "dashboard HTML is missing registered asset {asset}"
            );
        }
    }
}
