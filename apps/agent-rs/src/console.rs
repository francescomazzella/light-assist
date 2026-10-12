use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderValue, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{agent::Agent, assets, install, protocol::*, settings::Settings};

type Shared = State<Arc<Agent>>;

pub fn router(agent: Arc<Agent>) -> Router {
    Router::new()
        .route("/api/state", get(state))
        .route("/api/request/start", post(request_start))
        .route("/api/request/stop", post(request_stop))
        .route("/api/request/admit", post(request_admit))
        .route("/api/session/control", post(session_control))
        .route("/api/session/view", post(session_view))
        .route("/api/session/terminal", post(session_terminal))
        .route("/api/window/show", post(window_show))
        .route("/api/permissions", post(permissions_ask))
        .route("/api/session/trust", post(session_trust))
        .route("/api/devices/remove", post(device_remove))
        .route("/api/open", post(open_page))
        .route("/api/settings", get(settings_read).post(settings_write))
        .route("/api/app", get(app_info))
        .route("/api/app/startup", post(app_startup))
        .route("/api/app/install", post(app_install))
        .fallback(get(static_file))
        .layer(DefaultBodyLimit::max(1024))
        .layer(middleware::from_fn_with_state(agent.clone(), guard))
        .with_state(agent)
}

/// The one thing anything on this computer may ask: a second launch asking the first to show itself.
const OPEN_TO_ALL: &str = "/api/window/show";
const KEY_HEADER: &str = "x-light-assist-key";

/// The Host check blocks DNS rebinding; the Origin check blocks cross-site requests to the console API.
/// The key keeps the API to the app's own window: a browser on this computer, which a helper with
/// mouse and keyboard control could open, can load the page but cannot make it do anything.
async fn guard(State(agent): Shared, request: Request, next: Next) -> Response {
    let local = assets::same_site_hostname(request.headers())
        .is_some_and(|hostname| hostname == "localhost" || hostname == "127.0.0.1");
    let path = request.uri().path();
    let keyed = !path.starts_with("/api/")
        || path == OPEN_TO_ALL
        || request.headers().get(KEY_HEADER).is_some_and(|key| key.as_bytes() == agent.console_key.as_bytes());
    if !local || !keyed {
        return json_response(StatusCode::FORBIDDEN, &json!({ "error": "forbidden" }));
    }

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("frame-ancestors 'none'"));
    response
}

fn json_response(status: StatusCode, body: &impl Serialize) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8"), (header::CACHE_CONTROL, "no-store")],
        serde_json::to_vec(body).unwrap_or_default(),
    )
        .into_response()
}

fn ok() -> Response {
    json_response(StatusCode::OK, &json!({ "ok": true }))
}

fn outcome(result: Result<(), Failure>) -> Response {
    json_response(StatusCode::OK, &Outcome::from(result))
}

async fn state(State(agent): Shared) -> Response {
    json_response(StatusCode::OK, &agent.state())
}

async fn request_start(State(agent): Shared) -> Response {
    outcome(agent.start_request().await)
}

/// Stops asking for help, which also ends a session in progress.
async fn request_stop(State(agent): Shared) -> Response {
    agent.stop_request();
    ok()
}

#[derive(Deserialize)]
struct AdmitBody {
    code: String,
}

async fn request_admit(State(agent): Shared, body: Bytes) -> Response {
    outcome(match serde_json::from_slice::<AdmitBody>(&body) {
        Ok(body) if body.code.len() == 6 && body.code.bytes().all(|byte| byte.is_ascii_digit()) => agent.admit(&body.code),
        _ => Err(Failure::BadRequest),
    })
}

/// The sharer's yes or no to something the helper asked for.
#[derive(Deserialize)]
struct ControlBody {
    allow: bool,
}

async fn session_control(State(agent): Shared, body: Bytes) -> Response {
    match serde_json::from_slice::<ControlBody>(&body) {
        Ok(body) => {
            agent.answer_control(body.allow);
            ok()
        }
        Err(_) => outcome(Err(Failure::BadRequest)),
    }
}

async fn session_view(State(agent): Shared) -> Response {
    agent.drop_to_view();
    ok()
}

async fn settings_read(State(agent): Shared) -> Response {
    json_response(StatusCode::OK, &agent.settings())
}

/// Saves the settings and answers with what was actually stored.
async fn settings_write(State(agent): Shared, body: Bytes) -> Response {
    match serde_json::from_slice::<Settings>(&body) {
        Ok(settings) => {
            agent.set_settings(settings).await;
            json_response(StatusCode::OK, &agent.settings())
        }
        Err(_) => outcome(Err(Failure::BadRequest)),
    }
}

/// How the app is installed, and whether it starts with Windows. The Store's startup task is asked
/// off the async thread, because the answer is waited for.
async fn app_info() -> Response {
    match tokio::task::spawn_blocking(install::info).await {
        Ok(info) => json_response(StatusCode::OK, &info),
        Err(_) => outcome(Err(Failure::BadRequest)),
    }
}

#[derive(Deserialize)]
struct StartupBody {
    on: bool,
}

async fn app_startup(body: Bytes) -> Response {
    let Ok(StartupBody { on }) = serde_json::from_slice::<StartupBody>(&body) else { return outcome(Err(Failure::BadRequest)) };
    match tokio::task::spawn_blocking(move || install::set_start_with_windows(on)).await {
        Ok(Ok(info)) => json_response(StatusCode::OK, &info),
        Ok(Err(message)) => json_response(StatusCode::OK, &json!({ "ok": false, "message": message })),
        Err(_) => outcome(Err(Failure::BadRequest)),
    }
}

/// Installs this portable copy and hands over to the installed one, which opens its own window.
async fn app_install(State(agent): Shared) -> Response {
    if agent.busy() {
        return json_response(StatusCode::OK, &json!({ "ok": false, "message": crate::lang::tr("Stop the request first.", "Annulla prima la richiesta.") }));
    }
    match tokio::task::spawn_blocking(install::install).await {
        Ok(Ok(())) => {
            // Answer first, then let go of the port for the installed copy.
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                agent.quit();
            });
            json_response(StatusCode::OK, &json!({ "ok": true }))
        }
        Ok(Err(message)) => json_response(StatusCode::OK, &json!({ "ok": false, "message": message })),
        Err(_) => outcome(Err(Failure::BadRequest)),
    }
}

#[derive(Deserialize)]
struct TrustBody {
    name: String,
}

/// Makes the connected helper's browser a trusted device, which may then connect while nobody is here.
async fn session_trust(State(agent): Shared, body: Bytes) -> Response {
    outcome(match serde_json::from_slice::<TrustBody>(&body) {
        Ok(body) => agent.trust_helper(&body.name),
        Err(_) => Err(Failure::BadRequest),
    })
}

#[derive(Deserialize)]
struct OpenBody {
    page: String,
}

/// Opens one of the app's own pages on the website, in the person's browser.
async fn open_page(State(agent): Shared, body: Bytes) -> Response {
    outcome(match serde_json::from_slice::<OpenBody>(&body) {
        Ok(body) => agent.open_website(&body.page),
        Err(_) => Err(Failure::BadRequest),
    })
}

#[derive(Deserialize)]
struct DeviceBody {
    id: String,
}

async fn device_remove(State(agent): Shared, body: Bytes) -> Response {
    match serde_json::from_slice::<DeviceBody>(&body) {
        Ok(body) => {
            agent.remove_device(&body.id).await;
            ok()
        }
        Err(_) => outcome(Err(Failure::BadRequest)),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Permission {
    Screen,
    Control,
}

#[derive(Deserialize)]
struct PermissionBody {
    ask: Permission,
}

/// Takes the sharer to where their Mac lets them allow the app something. macOS asks by itself the
/// first time only; after that the list in System Settings is the one place to change it.
async fn permissions_ask(body: Bytes) -> Response {
    let Ok(PermissionBody { ask }) = serde_json::from_slice::<PermissionBody>(&body) else { return outcome(Err(Failure::BadRequest)) };
    #[cfg(target_os = "macos")]
    match ask {
        Permission::Screen => {
            crate::mac::ask_to_record_screen();
            crate::mac::open_privacy_settings("Privacy_ScreenCapture");
        }
        Permission::Control => {
            crate::mac::ask_to_control();
            crate::mac::open_privacy_settings("Privacy_Accessibility");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = matches!(ask, Permission::Screen | Permission::Control);
    ok()
}

/// What a second launch of the app calls, so that starting it again brings up the one already running.
async fn window_show(State(agent): Shared) -> Response {
    agent.show_window();
    json_response(StatusCode::OK, &json!({ "ok": true, "app": "light-assist" }))
}

/// Answers a terminal request, or closes an open terminal when `allow` is false.
async fn session_terminal(State(agent): Shared, body: Bytes) -> Response {
    match serde_json::from_slice::<ControlBody>(&body) {
        Ok(body) => {
            agent.answer_terminal(body.allow);
            ok()
        }
        Err(_) => outcome(Err(Failure::BadRequest)),
    }
}

async fn static_file(uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return json_response(StatusCode::NOT_FOUND, &json!({ "error": "not-found" }));
    }
    assets::serve(uri.path(), "index.html")
}
