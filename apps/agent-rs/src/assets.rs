use axum::{
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../ui/dist"]
struct Assets;

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Serves the embedded UI. `home` is the page shown at `/`: the console or the helper page.
pub fn serve(path: &str, home: &str) -> Response {
    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { home } else { path };

    let Some(file) = Assets::get(path) else {
        return if path == home {
            (StatusCode::SERVICE_UNAVAILABLE, "UI is not built. Run `pnpm build`.").into_response()
        } else {
            (StatusCode::NOT_FOUND, "Not found").into_response()
        };
    };

    (
        [
            (header::CONTENT_TYPE, content_type(path)),
            (header::CACHE_CONTROL, "no-cache"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        file.data.into_owned(),
    )
        .into_response()
}

fn origin_authority(origin: &str) -> Option<&str> {
    let rest = origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://"))?;
    rest.split('/').next()
}

/// The `Host` header's hostname, provided any `Origin` header names the same site. The caller checks
/// the hostname against DNS rebinding; the `Origin` match blocks cross-site requests.
pub fn same_site_hostname(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    if let Some(origin) = headers.get(header::ORIGIN) {
        if origin.to_str().ok().and_then(origin_authority) != Some(host) {
            return None;
        }
    }
    Some(host.split(':').next().unwrap_or_default().to_ascii_lowercase())
}
