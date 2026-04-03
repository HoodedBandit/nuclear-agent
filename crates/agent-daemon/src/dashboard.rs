use axum::{
    extract::Path,
    http::header,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use include_dir::{include_dir, Dir, File};

const CACHE_CONTROL: &str = "no-store, max-age=0";
const REFERRER_POLICY: &str = "no-referrer";
const CONTENT_TYPE_OPTIONS: &str = "nosniff";
const DASHBOARD_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";
static MODERN_DASHBOARD_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/static-modern");
#[cfg(test)]
const DASHBOARD_ASSET_PATHS: &[&str] = &[
    "/dashboard.css",
    "/dashboard-core.js",
    "/dashboard-core-foundation.js",
    "/dashboard-core-chat.js",
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

pub(crate) async fn dashboard_classic_root() -> Redirect {
    Redirect::temporary("/dashboard-classic")
}

pub(crate) async fn dashboard_classic_index() -> impl IntoResponse {
    dashboard_index().await
}

pub(crate) async fn dashboard_modern_root() -> Redirect {
    Redirect::temporary("/dashboard-modern")
}

pub(crate) async fn dashboard_modern_index() -> impl IntoResponse {
    let Some(file) = MODERN_DASHBOARD_DIR.get_file("index.html") else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [
                (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
                (header::CACHE_CONTROL, CACHE_CONTROL),
                (header::REFERRER_POLICY, REFERRER_POLICY),
                (header::X_CONTENT_TYPE_OPTIONS, CONTENT_TYPE_OPTIONS),
                (header::CONTENT_SECURITY_POLICY, DASHBOARD_CSP),
            ],
            "Modern dashboard assets are not available. Run `npm --prefix ui/dashboard run build`."
                .to_string(),
        )
            .into_response();
    };

    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE_CONTROL),
            (header::REFERRER_POLICY, REFERRER_POLICY),
            (header::X_CONTENT_TYPE_OPTIONS, CONTENT_TYPE_OPTIONS),
            (header::CONTENT_SECURITY_POLICY, DASHBOARD_CSP),
        ],
        Html(file.contents_utf8().unwrap_or_default()),
    )
        .into_response()
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

fn modern_content_type(file: &File<'_>) -> &'static str {
    match file.path().extension().and_then(|value| value.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
}

pub(crate) async fn dashboard_modern_asset(
    Path(asset): Path<String>,
) -> Result<Response, StatusCode> {
    let asset = asset.trim_start_matches('/');
    let file = MODERN_DASHBOARD_DIR
        .get_file(asset)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok((
        [
            (header::CONTENT_TYPE, modern_content_type(file)),
            (header::CACHE_CONTROL, CACHE_CONTROL),
            (header::REFERRER_POLICY, REFERRER_POLICY),
            (header::X_CONTENT_TYPE_OPTIONS, CONTENT_TYPE_OPTIONS),
        ],
        file.contents(),
    )
        .into_response())
}

pub(crate) fn add_dashboard_asset_routes<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .route("/dashboard-assets/{*asset}", get(dashboard_modern_asset))
        .route("/dashboard.css", get(dashboard_css))
        .route("/dashboard-core.js", get(dashboard_core_js))
        .route(
            "/dashboard-core-foundation.js",
            get(dashboard_core_foundation_js),
        )
        .route("/dashboard-core-chat.js", get(dashboard_core_chat_js))
        .route("/dashboard-control.js", get(dashboard_control_js))
        .route("/dashboard-connectors.js", get(dashboard_connectors_js))
        .route("/dashboard-providers.js", get(dashboard_providers_js))
        .route("/dashboard-plugins.js", get(dashboard_plugins_js))
        .route("/dashboard-workspace.js", get(dashboard_workspace_js))
        .route("/dashboard-settings.js", get(dashboard_settings_js))
        .route("/dashboard.js", get(dashboard_js))
        .route("/dashboard-chat.js", get(dashboard_chat_js))
}

pub(crate) async fn dashboard_core_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-core.js"))
}

pub(crate) async fn dashboard_core_foundation_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-core-foundation.js"))
}

pub(crate) async fn dashboard_core_chat_js() -> impl IntoResponse {
    javascript_response(include_str!("../static/dashboard-core-chat.js"))
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

    #[test]
    fn modern_dashboard_bundle_exists() {
        assert!(
            MODERN_DASHBOARD_DIR.get_file("index.html").is_some(),
            "modern dashboard build output is missing index.html"
        );
        assert!(
            MODERN_DASHBOARD_DIR
                .get_file(".vite/manifest.json")
                .is_some(),
            "modern dashboard build output is missing the Vite manifest"
        );
    }
}
