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

pub(crate) async fn dashboard_root() -> Redirect {
    Redirect::temporary("/dashboard")
}

pub(crate) async fn dashboard_index() -> impl IntoResponse {
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
    router.route("/dashboard-assets/{*asset}", get(dashboard_modern_asset))
}

#[cfg(test)]
mod tests {
    use super::*;

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
